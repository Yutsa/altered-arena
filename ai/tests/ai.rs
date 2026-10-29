//! The AI reads only what its player sees, answers legally, and is deterministic.
//! Games on starter decks (no private data needed).

use altered_ai::legal;
use altered_ai::play::{Agent, Observation, RandomAgent};
use altered_ai::search::{bot_step, Ai, Params};
use altered_ai::sim;
use altered_rules::arena::NewPlayer;
use altered_rules::Game;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;

fn big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().stack_size(256 << 20).spawn(f).unwrap().join().unwrap();
}

fn starter_game(seed: i64, a: &str, b: &str) -> Game {
    let players: Vec<NewPlayer> = [a, b].iter().map(|d| NewPlayer { name: "P".into(), bot: false, deck: Some(d.to_string()), api_deck: None }).collect();
    Game::create(seed, &players, &altered_ai::play::dice_for(seed as u64)).unwrap()
}

/// Plays `n` random answers
fn advance(g: &mut Game, n: usize, seed: u64) {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let decks = [Value::Null, Value::Null];
    for _ in 0..n {
        if g.is_over() {
            return;
        }
        let p = g.waiting_for()[0];
        let d = g.decision(p).unwrap();
        let a = RandomAgent.decide(&Observation::new(g, p as i64, &decks), &d, &mut rng);
        g.answer(p, &legal::with_id(&d, &a)).unwrap();
    }
}

#[test]
fn hidden_cards_do_not_change_what_the_ai_sees() {
    big_stack(|| {
        let mut g = starter_game(11, "ax_1_sierra", "br_1_kojo");
        advance(&mut g, 30, 3);
        while g.is_over() {
            g = starter_game(12, "ax_1_sierra", "br_1_kojo");
            advance(&mut g, 20, 4);
        }
        let me = g.waiting_for()[0] as i64;
        let opp = sim::other(&g, me);
        // The same game with two of the opponent's hidden cards swapped (one in hand, one in deck)
        let hand: Vec<i64> = g.cards.iter().filter(|(_, c)| c.pid == opp && c.location == "hand").map(|(id, _)| *id).collect();
        let deck: Vec<i64> = g.cards.iter().filter(|(_, c)| c.pid == opp && c.location == format!("deck-{}", opp)).map(|(id, _)| *id).collect();
        let (x, y) = (hand[0], *deck.iter().find(|id| g.cards[&**id].db_props != g.cards[&hand[0]].db_props).unwrap());
        let mut h = g.clone();
        let (px, dx) = (h.cards[&x].props.clone(), h.cards[&x].db_props.clone());
        let (py, dy) = (h.cards[&y].props.clone(), h.cards[&y].db_props.clone());
        let c = h.cards.get_mut(&x).unwrap();
        c.props = py;
        c.db_props = dy;
        let c = h.cards.get_mut(&y).unwrap();
        c.props = px;
        c.db_props = dx;
        assert_ne!(g.oracle_state(), h.oracle_state(), "the swap changes the game");
        let decks = [Value::Null, Value::Null];
        let (og, oh) = (Observation::new(&g, me, &decks), Observation::new(&h, me, &decks));
        // Same view
        let (mut vg, mut vh) = (og.view(), oh.view());
        vg["log"] = Value::Null;
        vh["log"] = Value::Null;
        assert_eq!(vg, vh);
        // Same imagined games
        for s in 0..5 {
            let a = og.determinize(&mut ChaCha8Rng::seed_from_u64(s));
            let b = oh.determinize(&mut ChaCha8Rng::seed_from_u64(s));
            assert_eq!(a.oracle_state(), b.oracle_state(), "determinisation {}", s);
        }
        // Same answer
        let d = g.decision(me as u32).unwrap();
        let mut ai = Ai::new("t", Params::default());
        let a1 = ai.decide(&og, &d, &mut ChaCha8Rng::seed_from_u64(9));
        let a2 = ai.decide(&oh, &d, &mut ChaCha8Rng::seed_from_u64(9));
        assert_eq!(a1, a2);
    });
}

#[test]
fn candidates_are_legal_and_cover_the_simple_bot() {
    big_stack(|| {
        let mut checked = 0;
        let mut covered = 0;
        for seed in 0..3 {
            let mut g = starter_game(100 + seed, ["ax_1_sierra", "ly_1_nevenka", "mu_1_teija"][seed as usize], "br_1_kojo");
            let mut rng = ChaCha8Rng::seed_from_u64(seed as u64);
            let decks = [Value::Null, Value::Null];
            for _ in 0..150 {
                if g.is_over() {
                    break;
                }
                let p = g.waiting_for()[0];
                let d = g.decision(p).unwrap();
                let cands = legal::candidates(&d);
                assert!(!cands.is_empty(), "no candidate for {}", d);
                let mut states = vec![];
                for c in cands.iter().take(40) {
                    let mut h = g.clone();
                    let ok = h.answer(p, &legal::with_id(&d, c));
                    // Selections may break a rule the decision does not show (total cost, Tough)
                    if d["kind"] != "select" && d["kind"] != "cleanup" {
                        assert!(ok.is_ok(), "{} refused: {:?} ({})", c, ok, d);
                    }
                    if ok.is_ok() {
                        states.push(format!("{:?}{}", h.waiting_for(), h.oracle_state()));
                    }
                    checked += 1;
                }
                // The simple bot's answer is one of the candidates (same resulting state)
                if cands.len() <= 40 {
                    let mut b = g.clone();
                    if bot_step(&mut b, p as i64) {
                        let s = format!("{:?}{}", b.waiting_for(), b.oracle_state());
                        assert!(states.contains(&s), "the simple bot's answer is not a candidate at {}", d);
                        covered += 1;
                    }
                }
                let a = RandomAgent.decide(&Observation::new(&g, p as i64, &decks), &d, &mut rng);
                if g.answer(p, &legal::with_id(&d, &a)).is_err() {
                    g.answer(p, &legal::with_id(&d, &cands[0])).unwrap();
                }
            }
        }
        assert!(checked > 200 && covered > 100, "checked {} covered {}", checked, covered);
    });
}

#[test]
fn games_are_deterministic() {
    big_stack(|| {
        let decks: Vec<Value> = ["ax_1_sierra", "br_1_kojo"].iter().map(|_| Value::Null).collect();
        let _ = decks;
        let run = || {
            let mut g = starter_game(77, "ax_1_sierra", "br_1_kojo");
            let mut ais = [Ai::new("a", Params::default()), Ai::new("b", Params::default())];
            let mut rng = ChaCha8Rng::seed_from_u64(5);
            let d2 = [Value::Null, Value::Null];
            let mut n = 0;
            while !g.is_over() && n < 60 {
                let p = g.waiting_for()[0];
                let d = g.decision(p).unwrap();
                let a = ais[(p - 2001) as usize].decide(&Observation::new(&g, p as i64, &d2), &d, &mut rng);
                if g.answer(p, &legal::with_id(&d, &a)).is_err() {
                    g.answer(p, &legal::with_id(&d, &legal::candidates(&d)[0])).unwrap();
                }
                n += 1;
            }
            g.oracle_state()
        };
        assert_eq!(run(), run());
    });
}
