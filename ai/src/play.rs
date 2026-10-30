//! Headless games between two agents, and the parallel runner.

use crate::legal;
use crate::sim;
use altered_rules::arena::NewPlayer;
use altered_rules::game::PId;
use altered_rules::Game;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub const SEATS: [PId; 2] = [2001, 2002];
/// Steps after which a game counts as an error (a normal game takes 100 to 300)
pub const MAX_STEPS: usize = 3000;

/// What an agent may read: its own view of the game, the open decklists, and copies of the game
/// redealt from its point of view. The real game itself is not reachable from here.
pub struct Observation<'a> {
    game: &'a Game,
    pub me: PId,
    /// Each seat's deck content (open decklists)
    pub decks: &'a [Value; 2],
}

impl<'a> Observation<'a> {
    pub fn new(game: &'a Game, me: PId, decks: &'a [Value; 2]) -> Self {
        Observation { game, me, decks }
    }

    /// The GameView of `me` (protocol v1)
    pub fn view(&self) -> Value {
        let mut g = self.game.clone();
        g.view(Some(self.me as u32), "").unwrap_or(Value::Null)
    }

    /// A copy of the game with what `me` cannot see dealt again at random
    pub fn determinize(&self, rng: &mut impl RngCore) -> Game {
        sim::determinize(self.game, self.me, rng)
    }

    /// The prior key of a card `me` can see (its own hand, Mana, the board...)
    pub fn card_key(&self, id: &Value) -> Option<String> {
        let n = id.as_i64()?;
        if sim::hidden_groups(self.game, self.me).iter().any(|g| g.contains(&n)) {
            return None;
        }
        crate::search::card_key(self.game, id)
    }

    /// A property of a card `me` can see (its type, subtypes, faction...)
    pub fn card_prop(&self, id: i64, key: &str) -> Option<Value> {
        if sim::hidden_groups(self.game, self.me).iter().any(|g| g.contains(&id)) {
            return None;
        }
        self.game.cards.get(&id)?.props.get(key).cloned()
    }

    pub fn opponent(&self) -> PId {
        sim::other(self.game, self.me)
    }
}

pub trait Agent: Send {
    fn name(&self) -> String;
    /// An answer (without decision id) to the decision `d` of `obs.me`
    fn decide(&mut self, obs: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value;
    /// Played by the arena's bot (Bot.php) instead of `decide`
    fn is_simple_bot(&self) -> bool {
        false
    }
}

/// The oracle's simple bot (server/src/Bot.php, first legal choices)
pub struct SimpleBot;

impl Agent for SimpleBot {
    fn name(&self) -> String {
        "simple".into()
    }
    fn decide(&mut self, _: &Observation, d: &Value, _: &mut ChaCha8Rng) -> Value {
        legal::candidates(d).into_iter().next().unwrap_or(serde_json::json!({"type": "pass"}))
    }
    fn is_simple_bot(&self) -> bool {
        true
    }
}

/// Uniformly random legal answers (turn: play something 80 % of the time, as explore.php does)
pub struct RandomAgent;

impl Agent for RandomAgent {
    fn name(&self) -> String {
        "random".into()
    }
    fn decide(&mut self, _: &Observation, d: &Value, rng: &mut ChaCha8Rng) -> Value {
        let c = legal::candidates(d);
        if c.is_empty() {
            return serde_json::json!({"type": "pass"});
        }
        if d["kind"] == "turn" {
            let stop: Vec<&Value> = c.iter().filter(|a| a["type"] == "endTurn" || a["type"] == "pass").collect();
            let act: Vec<&Value> = c.iter().filter(|a| a["type"] != "endTurn" && a["type"] != "pass").collect();
            if !act.is_empty() && (stop.is_empty() || rng.random_range(0..100) < 80) {
                return act[rng.random_range(0..act.len())].clone();
            }
            if let Some(s) = stop.first() {
                return (*s).clone();
            }
        }
        c[rng.random_range(0..c.len())].clone()
    }
}

#[derive(Clone, Debug, Default, Serialize, serde::Deserialize)]
pub struct GameResult {
    pub seed: u64,
    /// Seat index of the winner (None: a draw or an error)
    pub winner: Option<usize>,
    pub day: i64,
    /// Answers given by each seat
    pub decisions: [usize; 2],
    /// Answers refused by the engine (each seat), the agent's next candidates were used
    pub refused: [usize; 2],
    pub error: Option<String>,
    pub millis: u64,
    /// Each seat at the end: Mana Orbs, hand, Reserve, progress (regions covered)
    pub end: Vec<Value>,
}

/// The dice of a game (the arena's per-game list)
pub fn dice_for(seed: u64) -> Vec<i64> {
    let mut r = ChaCha8Rng::seed_from_u64(seed ^ 0xD1CE);
    (0..400).map(|_| r.random_range(1..=6)).collect()
}

pub fn new_game(decks: &[Value; 2], seed: u64) -> Result<Game, String> {
    let players: Vec<NewPlayer> = ["A", "B"]
        .iter()
        .enumerate()
        .map(|(i, n)| NewPlayer { name: n.to_string(), bot: false, deck: None, api_deck: Some(decks[i].clone()) })
        .collect();
    Game::create(seed as i64 % 1_000_000_000 + 1, &players, &dice_for(seed)).map_err(|e| format!("create: {}", e))
}

/// The winner seat from the spectator view
pub fn winner(g: &Game) -> Option<usize> {
    let mut c = g.clone();
    let v = c.view(None, "").ok()?;
    let w = v["winner"].as_i64()?;
    SEATS.iter().position(|s| *s == w)
}

pub fn day(g: &Game) -> i64 {
    let s = g.oracle_state();
    s["day"].as_i64().unwrap_or(0)
}

/// Plays one game to its end. `seed` sets the game (shuffles, dice) and the agents' randomness.
pub fn play_game(decks: &[Value; 2], agents: &mut [Box<dyn Agent>; 2], seed: u64) -> GameResult {
    let t = std::time::Instant::now();
    let mut res = GameResult { seed, ..Default::default() };
    let mut g = match new_game(decks, seed) {
        Ok(g) => g,
        Err(e) => {
            res.error = Some(e);
            return res;
        }
    };
    g.bots = SEATS.iter().enumerate().filter(|(i, _)| agents[*i].is_simple_bot()).map(|(_, p)| *p).collect();
    let mut rng = ChaCha8Rng::seed_from_u64(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA1);
    let mut steps = 0;
    while !g.is_over() {
        steps += 1;
        if steps > MAX_STEPS {
            res.error = Some("too many steps".into());
            break;
        }
        let w = g.waiting_for();
        let Some(&p) = w.first() else {
            res.error = Some("nobody to play".into());
            break;
        };
        let seat = SEATS.iter().position(|s| *s == p as PId).unwrap_or(0);
        if agents[seat].is_simple_bot() {
            let before = g.rng_acts;
            if let Err(e) = g.run_bots() {
                res.error = Some(format!("bot: {}", e));
                break;
            }
            res.decisions[seat] += 1;
            if g.rng_acts == before {
                res.error = Some("bot stuck".into());
                break;
            }
            continue;
        }
        let d = match g.try_decision(p) {
            Ok(Some(d)) => d,
            Ok(None) => {
                res.error = Some(format!("no decision for {}", p));
                break;
            }
            Err(e) => {
                res.error = Some(format!("decision: {}", e));
                break;
            }
        };
        let a = {
            let obs = Observation::new(&g, p as PId, decks);
            agents[seat].decide(&obs, &d, &mut rng)
        };
        res.decisions[seat] += 1;
        let first_err = match g.answer(p, &legal::with_id(&d, &a)) {
            Ok(_) => continue,
            Err(e) => e.to_string(),
        };
        // Refused: the other candidates in order, then the end of the turn / a pass
        res.refused[seat] += 1;
        let mut ok = false;
        for c in legal::candidates(&d) {
            if g.answer(p, &legal::with_id(&d, &c)).is_ok() {
                ok = true;
                break;
            }
        }
        if !ok {
            res.error = Some(format!("no answer accepted ({}): {}", d["kind"], first_err));
            break;
        }
    }
    if res.error.is_none() {
        res.winner = winner(&g);
    }
    let mut spectator = g.clone();
    if let Ok(v) = spectator.view(None, "") {
        for p in v["players"].as_array().into_iter().flatten() {
            let reserve = v["cards"].as_array().map(|c| c.iter().filter(|c| c["owner"] == p["id"] && c["zone"] == "reserve").count()).unwrap_or(0);
            let progress = p["expeditions"]["hero"]["position"].as_i64().unwrap_or(0) + 7 - p["expeditions"]["companion"]["position"].as_i64().unwrap_or(7);
            res.end.push(serde_json::json!({"mana": p["manaTotal"], "hand": p["handCount"], "reserve": reserve, "progress": progress}));
        }
    }
    res.day = day(&g);
    res.millis = t.elapsed().as_millis() as u64;
    res
}

/// Runs `jobs` on `threads` threads with 256 MB stacks; a job that panics gives `on_panic`
pub fn run_parallel<J: Sync, T: Send>(jobs: &[J], threads: usize, f: impl Fn(&J) -> T + Sync, on_panic: impl Fn(&J, String) -> T + Sync) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<T>>> = Mutex::new((0..jobs.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn_scoped(s, || loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= jobs.len() {
                        break;
                    }
                    // AI_JOBLOG: which jobs were running when the process died (stack overflow aborts)
                    let log = std::env::var_os("AI_JOBLOG").is_some();
                    // AI_ONLY=i,j: run only these jobs (the others are recorded as skipped)
                    if let Ok(only) = std::env::var("AI_ONLY") {
                        if !only.split(',').any(|x| x.trim().parse() == Ok(i)) {
                            let r = on_panic(&jobs[i], "skipped".into());
                            out.lock().unwrap()[i] = Some(r);
                            continue;
                        }
                    }
                    if log {
                        eprintln!("job {} start", i);
                    }
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&jobs[i])));
                    if log {
                        eprintln!("job {} end", i);
                    }
                    let r = match r {
                        Ok(r) => r,
                        Err(e) => {
                            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                            on_panic(&jobs[i], msg)
                        }
                    };
                    out.lock().unwrap()[i] = Some(r);
                })
                .unwrap();
        }
    });
    out.into_inner().unwrap().into_iter().map(|x| x.unwrap()).collect()
}
