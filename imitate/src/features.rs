//! Features of the candidate answers of a decision, from what the deciding player sees.
//!
//! Each candidate is played on one copy of the game redealt from the player's point of view
//! (`Observation::determinize`), the rest of the player's action is finished by the fast policy,
//! and the player's GameView is read before and after. A candidate's features are:
//!
//! - the change of each evaluation term (`eval::Weights`: progress, Dusk outlook, margins,
//!   board, hand, Reserve, landmarks, Mana, ready Mana, empty expeditions), for each side,
//!   right after the answer (`i:`) and once the action is finished (`f:`);
//! - what the answer is: its kind and type, and for each card it names, the card's class, type,
//!   owner, zone, cost and statistics, under the role the card plays (played, put in Mana,
//!   targeted, discarded at Night...);
//! - the context (day, Mana, hand, Reserve, who passed, the race), crossed with the action class,
//!   since only differences between the candidates of a decision count.
//!
//! Nothing here is keyed to a game, a player or a move: the player-specific part of an AI is
//! only the weights it gives to these features.

use altered_ai::eval::{self, Weights};
use altered_ai::play::Observation;
use altered_ai::{legal, search};
use altered_rules::Game;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::collections::HashMap;

/// A candidate's features: (name, value), names may repeat (values add up)
pub type Feats = Vec<(String, f32)>;

#[derive(Clone, Debug)]
pub struct Config {
    /// Fast-policy steps to finish the action after the answer
    pub finish_steps: usize,
    /// The evaluation weights whose total change is one more feature (`f:eval`)
    pub weights: Weights,
    /// Features of the expedition a character is played into (`dst:`)
    pub dest_features: bool,
    /// The deck AI's pick as a feature (`s:pick`)
    pub search_pick: bool,
    /// Redealt copies the position features are averaged over
    pub copies: usize,
    /// Mixed into the redeal's random seed (another seed, another redeal of the hidden cards)
    pub seed: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config { finish_steps: 12, weights: Weights::default(), dest_features: true, search_pick: true, copies: 1, seed: 0 }
    }
}

fn num(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0)
}

/// The terms of the evaluation, per side, from a GameView
pub const TERMS: [&str; 12] = ["prog", "dusk", "margin", "board", "empty", "hand", "reserve", "landmark", "mana", "manaReady", "deck", "discard"];

fn terms(view: &Value, w: &Weights) -> [[f64; 12]; 2] {
    let mut out = [[0.0; 12]; 2];
    let me = view["you"].as_i64().unwrap_or(0);
    let Some(players) = view["players"].as_array().filter(|p| p.len() == 2) else { return out };
    let (mine, theirs) = if players[0]["id"].as_i64() == Some(me) { (&players[0], &players[1]) } else { (&players[1], &players[0]) };
    let biomes = |pos: i64| -> Vec<String> {
        let r = view["regions"].as_array().and_then(|rs| rs.iter().find(|r| r["index"].as_i64() == Some(pos)));
        match r.and_then(|r| r["biomes"].as_array()) {
            Some(b) => b.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            None => vec!["forest".into(), "mountain".into(), "ocean".into()],
        }
    };
    for (k, (a, b)) in [(mine, theirs), (theirs, mine)].into_iter().enumerate() {
        let t = &mut out[k];
        t[0] = num(&a["expeditions"]["hero"]["position"]) + (7.0 - num(&a["expeditions"]["companion"]["position"]));
        for side in ["hero", "companion"] {
            let e = &a["expeditions"][side];
            let o = &b["expeditions"][side];
            let total: f64 = ["forest", "mountain", "ocean"].iter().map(|x| num(&e["stats"][*x])).sum();
            t[3] += total;
            if total == 0.0 {
                t[4] += 1.0;
            }
            if e["blocked"].as_bool().unwrap_or(false) {
                continue;
            }
            let mut best = f64::MIN;
            let mut wins = false;
            for bio in biomes(e["position"].as_i64().unwrap_or(0)) {
                let (x, y) = (num(&e["stats"][&bio]), num(&o["stats"][&bio]));
                if x > y && x > 0.0 {
                    wins = true;
                }
                best = best.max(x - y);
            }
            if wins {
                t[1] += 1.0;
            }
            if best > f64::MIN {
                t[2] += best.clamp(-w.margin_cap, w.margin_cap);
            }
        }
        let id = a["id"].as_i64().unwrap_or(0);
        let count = |zone: &str| view["cards"].as_array().map(|c| c.iter().filter(|c| c["owner"].as_i64() == Some(id) && c["zone"] == zone).count()).unwrap_or(0) as f64;
        t[5] = num(&a["handCount"]);
        t[6] = count("reserve");
        t[7] = count("landmark");
        let m = num(&a["manaTotal"]);
        t[8] = m.min(w.mana_soft) + (w.mana_late / w.mana.max(1e-9)) * (m - w.mana_soft).max(0.0);
        t[9] = num(&a["manaAvailable"]);
        t[10] = num(&a["deckCount"]);
        t[11] = num(&a["discardCount"]);
    }
    out
}

fn winner_term(view: &Value) -> f64 {
    match view["winner"].as_i64() {
        Some(w) if Some(w) == view["you"].as_i64() => 1.0,
        Some(_) => -1.0,
        None => 0.0,
    }
}

/// A prompt with its numbers replaced, as a category
pub fn prompt_class(p: &str) -> String {
    let s: String = p.to_lowercase().chars().map(|c| if c.is_ascii_digit() { '#' } else { c }).collect();
    s.chars().take(70).collect()
}

/// The words of a prompt that say what it does (`discard`, `boost`, `reserve`...): letters only,
/// the words every prompt has left out
pub fn prompt_words(p: &str) -> Vec<String> {
    const COMMON: [&str; 16] = ["you", "must", "may", "choose", "target", "card", "cards", "the", "from", "max", "base", "cost", "of", "up", "to", "and"];
    let mut out: Vec<String> = p
        .to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() >= 3 && !COMMON.contains(w))
        .map(String::from)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The group a decision belongs to: its kind, `mana` for a choice of cards to put in Mana
pub fn group(d: &Value) -> String {
    let kind = d["kind"].as_str().unwrap_or("").to_string();
    if kind == "select" {
        let p = d["prompt"].as_str().unwrap_or("").to_lowercase();
        if d["toMana"] == true || p.contains("mana") {
            return "mana".into();
        }
        return "target".into();
    }
    kind
}

/// What the player sees of a card, from the GameView and the visible properties
struct CardInfo {
    key: String,
    ctype: String,
    mine: bool,
    zone: String,
    cost: f64,
    power: f64,
    boosts: f64,
    exhausted: bool,
    /// Keywords of its texts: `{J}`-style symbols, `<BOOST>`-style tokens, `[[hand]]`-style
    /// zones (support text ones with an `s` in front)
    fx: Vec<String>,
}

/// The keywords of a card text
pub fn keywords(text: &str, prefix: &str) -> Vec<String> {
    let mut out = vec![];
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let close = match b[i] {
            b'{' => Some(b'}'),
            b'<' => Some(b'>'),
            b'[' if i + 1 < b.len() && b[i + 1] == b'[' => Some(b']'),
            _ => None,
        };
        if let Some(c) = close {
            let start = if b[i] == b'[' { i + 2 } else { i + 1 };
            if let Some(len) = b[start..].iter().position(|x| *x == c) {
                let w = &text[start..start + len];
                if !w.is_empty() && w.len() < 20 && w.chars().all(|ch| ch.is_alphabetic() || ch == '_') {
                    out.push(format!("{}{}", prefix, w.to_lowercase()));
                }
                i = start + len + 1;
                continue;
            }
        }
        i += 1;
    }
    if text.contains('#') {
        out.push(format!("{}static", prefix));
    }
    out.sort();
    out.dedup();
    out
}

struct Ctx<'a> {
    obs: &'a Observation<'a>,
    cards: HashMap<i64, &'a Value>,
    info: HashMap<i64, Option<CardInfo>>,
}

impl<'a> Ctx<'a> {
    fn info(&mut self, id: i64) -> &Option<CardInfo> {
        if !self.info.contains_key(&id) {
            let v = self.cards.get(&id).copied();
            let i = v.map(|c| {
                let zone = c["zone"].as_str().unwrap_or("").to_string();
                let cost = if zone == "reserve" { num(&c["costReserve"]) } else { num(&c["costHand"]) };
                let power = ["forest", "mountain", "ocean"].iter().map(|b| num(&c["stats"][*b])).sum();
                let key = self.obs.card_key(&Value::from(id)).unwrap_or_else(|| "?".into());
                let ctype = self.obs.card_prop(id, "type").and_then(|t| t.as_str().map(String::from)).unwrap_or_else(|| "?".into());
                let text = |k1: &str, k2: &str| self.obs.card_prop(id, k1).or_else(|| self.obs.card_prop(id, k2)).and_then(|t| t.as_str().map(String::from)).unwrap_or_default();
                let mut fx = keywords(&text("effectDesc", "effect"), "");
                fx.extend(keywords(&text("supportDesc", "support"), "s"));
                CardInfo { key, ctype, mine: c["owner"].as_i64() == Some(self.obs.me), zone, cost, power, boosts: num(&c["boosts"]), exhausted: c["exhausted"] == true, fx }
            });
            self.info.insert(id, i);
        }
        &self.info[&id]
    }

    /// Where a card stands among `pool` (the cards the decision offers): the dearest, the
    /// cheapest, the strongest, the weakest, how many other copies of it
    fn relative(&mut self, f: &mut Feats, role: &str, id: &Value, pool: &[Value]) -> Vec<&'static str> {
        let mut flags = vec![];
        let Some(id) = id.as_i64() else { return flags };
        let Some((cost, power, key)) = self.info(id).as_ref().map(|c| (c.cost, c.power, c.key.clone())) else { return flags };
        let mut others = vec![];
        for p in pool {
            if let Some(q) = p.as_i64().filter(|q| *q != id) {
                if let Some(c) = self.info(q).as_ref() {
                    others.push((c.cost, c.power, c.key == key));
                }
            }
        }
        if others.is_empty() {
            return flags;
        }
        if others.iter().all(|o| o.0 < cost) {
            flags.push("dearest");
        }
        if others.iter().all(|o| o.0 > cost) {
            flags.push("cheapest");
        }
        if others.iter().all(|o| o.1 < power) {
            flags.push("strongest");
        }
        if others.iter().all(|o| o.1 > power) {
            flags.push("weakest");
        }
        for fl in &flags {
            f.push((format!("{}:{}", role, fl), 1.0));
        }
        let dup = others.iter().filter(|o| o.2).count();
        if dup > 0 {
            f.push((format!("{}:dup", role), dup as f32));
        }
        let mean = others.iter().map(|o| o.0).sum::<f64>() / others.len() as f64;
        f.push((format!("{}:costVsMean", role), (cost - mean) as f32));
        flags
    }

    /// A card in an expedition: whether that expedition wins or loses at Dusk as things stand
    fn expedition_status(&mut self, f: &mut Feats, v0: &Value, id: &Value) {
        let Some(id) = id.as_i64() else { return };
        let Some((zone, mine)) = self.info(id).as_ref().map(|c| (c.zone.clone(), c.mine)) else { return };
        let Some(side) = side_of(&zone) else { return };
        let me = self.obs.me;
        let players: Vec<&Value> = v0["players"].as_array().map(|p| p.iter().collect()).unwrap_or_default();
        let owner = players.iter().find(|p| (p["id"].as_i64() == Some(me)) == mine).copied();
        let other = players.iter().find(|p| (p["id"].as_i64() == Some(me)) != mine).copied();
        let (Some(owner), Some(other)) = (owner, other) else { return };
        let e = &owner["expeditions"][side];
        let o = &other["expeditions"][side];
        let (mut wins, mut loses) = (false, false);
        for b in region_biomes(v0, e["position"].as_i64().unwrap_or(0)) {
            let (x, y) = (num(&e["stats"][&b]), num(&o["stats"][&b]));
            wins |= x > y && x > 0.0;
            loses |= y > x;
        }
        let who = if mine { "mine" } else { "opp" };
        if wins {
            f.push((format!("tgtexp:{}:wins", who), 1.0));
        }
        if loses {
            f.push((format!("tgtexp:{}:behind", who), 1.0));
        }
    }

    /// Features of a card under a role
    fn card(&mut self, f: &mut Feats, role: &str, id: &Value) {
        let Some(id) = id.as_i64() else { return };
        match self.info(id) {
            None => f.push((format!("{}:hidden", role), 1.0)),
            Some(c) => {
                f.push((format!("{}:key={}", role, c.key), 1.0));
                f.push((format!("{}:type={}", role, c.ctype), 1.0));
                f.push((format!("{}:{}", role, if c.mine { "mine" } else { "opp" }), 1.0));
                f.push((format!("{}:zone={}", role, c.zone), 1.0));
                f.push((format!("{}:cost", role), c.cost as f32));
                f.push((format!("{}:power", role), c.power as f32));
                if c.boosts != 0.0 {
                    f.push((format!("{}:boosts", role), c.boosts as f32));
                }
                if c.exhausted {
                    f.push((format!("{}:exhausted", role), 1.0));
                }
                for k in &c.fx {
                    f.push((format!("{}:fx={}", role, k), 1.0));
                }
            }
        }
    }
}

fn ids(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.clone(),
        Value::Null => vec![],
        x => vec![x.clone()],
    }
}

/// The features of every candidate of `d` (in order). `None` for a candidate the redealt copy
/// refuses (it then keeps only its description features).
/// What the deck AI (`search::Ai`) thinks of the candidates: the one it picks, and for each
/// candidate it evaluated, its 1-ply and rollout scores
#[derive(Clone, Debug, Default)]
pub struct SearchView {
    pub pick: Option<usize>,
    pub ply: Vec<Option<f64>>,
    pub roll: Vec<Option<f64>>,
}

pub fn search_pick(ai: &mut search::Ai, obs: &Observation, d: &Value, cands: &[Value], rng: &mut ChaCha8Rng) -> SearchView {
    let a = legal::canonical(&ai.choose(obs, d, rng));
    let canon: Vec<Value> = cands.iter().map(legal::canonical).collect();
    let mut v = SearchView { pick: canon.iter().position(|c| *c == a), ply: vec![None; cands.len()], roll: vec![None; cands.len()] };
    for (ans, ply, roll) in &ai.last_scores {
        if let Some(k) = canon.iter().position(|c| *c == legal::canonical(ans)) {
            v.ply[k] = Some(*ply);
            v.roll[k] = *roll;
        }
    }
    v
}

/// The deck AI's parameters without the card priors (they were fit on the tournament replays,
/// so they would leak the held-out games)
pub fn without_priors(mut p: search::Params) -> search::Params {
    p.play_prior.clear();
    p.mana_prior.clear();
    p
}

/// `pick`: the candidate the deck AI picks (`s:pick`), when it is a feature
/// The action class of an answer (`turn:play`, `turn:stop`, `mana:select`, `target:pass`...)
pub fn action_class(d: &Value, a: &Value) -> String {
    let grp = group(d);
    match (grp.as_str(), a["type"].as_str().unwrap_or("")) {
        ("turn", "endTurn") | ("turn", "pass") => "turn:stop".to_string(),
        (g, "pass") => format!("{}:pass", g),
        (g, t) => format!("{}:{}", g, t),
    }
}

/// `search`: the deck AI's view of the decision
pub fn extract(obs: &Observation, d: &Value, cands: &[Value], rng: &mut ChaCha8Rng, cfg: &Config, search: Option<&SearchView>) -> Vec<Feats> {
    let pick = search.and_then(|s| s.pick);
    let best_ply = search.and_then(|s| s.ply.iter().flatten().cloned().reduce(f64::max));
    let best_roll = search.and_then(|s| s.roll.iter().flatten().cloned().reduce(f64::max));
    let me = obs.me;
    let base = obs.determinize(rng);
    let v0 = base.clone().view(Some(me as u32), "").unwrap_or(Value::Null);
    let t0 = terms(&v0, &cfg.weights);
    let e0 = eval::evaluate(&v0, &cfg.weights);
    let mut bases = vec![base.clone()];
    let mut starts = vec![(t0, e0)];
    for _ in 1..cfg.copies.max(1) {
        let b = obs.determinize(rng);
        let v = b.clone().view(Some(me as u32), "").unwrap_or(Value::Null);
        starts.push((terms(&v, &cfg.weights), eval::evaluate(&v, &cfg.weights)));
        bases.push(b);
    }
    let grp = group(d);
    let prompt = prompt_class(d["prompt"].as_str().unwrap_or(""));
    let words = prompt_words(d["prompt"].as_str().unwrap_or(""));
    let cards_vec: Vec<Value> = v0["cards"].as_array().cloned().unwrap_or_default();
    let mut ctx = Ctx { obs, cards: HashMap::new(), info: HashMap::new() };
    for c in &cards_vec {
        if let Some(id) = c["id"].as_i64() {
            ctx.cards.insert(id, c);
        }
    }
    // Context of the decision
    let players: Vec<&Value> = v0["players"].as_array().map(|p| p.iter().collect()).unwrap_or_default();
    let mine = players.iter().find(|p| p["id"].as_i64() == Some(me)).copied().cloned().unwrap_or(Value::Null);
    let theirs = players.iter().find(|p| p["id"].as_i64() != Some(me)).copied().cloned().unwrap_or(Value::Null);
    let context: Vec<(&str, f32)> = vec![
        ("day", num(&v0["day"]) as f32),
        ("manaAvail", num(&mine["manaAvailable"]) as f32),
        ("manaTotal", num(&mine["manaTotal"]) as f32),
        ("hand", num(&mine["handCount"]) as f32),
        ("reserve", t0[0][6] as f32),
        ("oppPassed", (theirs["passed"] == true) as u8 as f32),
        ("lead", (t0[0][0] - t0[1][0]) as f32),
        ("first", (v0["firstPlayer"].as_i64() == Some(me)) as u8 as f32),
        ("duskLead", (t0[0][1] - t0[1][1]) as f32),
        ("nPlayable", d["play"].as_array().map(|p| p.len()).unwrap_or(0) as f32),
        ("oppHand", num(&theirs["handCount"]) as f32),
    ];
    // Costs of the playable cards, for the rank of a play
    let mut play_costs: Vec<f64> = vec![];
    if d["kind"] == "turn" {
        for p in d["play"].as_array().into_iter().flatten() {
            if let Some(id) = p["cardId"].as_i64() {
                if let Some(c) = ctx.info(id) {
                    play_costs.push(c.cost);
                }
            }
        }
    }
    let mana_avail = num(&mine["manaAvailable"]);
    let mut out = vec![];
    for (k, a) in cands.iter().enumerate() {
        let mut f: Feats = vec![];
        let ty = a["type"].as_str().unwrap_or("").to_string();
        // Action class
        let class = action_class(d, a);
        f.push((format!("a:{}", class), 1.0));
        f.push((format!("a:{}[{}]", class, prompt), 1.0));
        if k == 0 {
            f.push(("a:first".into(), 1.0));
        }
        if pick == Some(k) {
            f.push(("s:pick".into(), 1.0));
            f.push((format!("s:pick:{}", grp), 1.0));
        }
        if let Some(sv) = search {
            match (sv.ply.get(k).copied().flatten(), best_ply) {
                (Some(x), Some(b)) => f.push((format!("sv:ply:{}", grp), ((x - b) / 10.0).clamp(-5.0, 0.0) as f32)),
                _ if best_ply.is_some() => f.push((format!("sv:cut:{}", grp), 1.0)),
                _ => {}
            }
            if let (Some(x), Some(b)) = (sv.roll.get(k).copied().flatten(), best_roll) {
                f.push(("sv:rolled".into(), 1.0));
                f.push(("sv:roll".into(), ((x - b) / 10.0).clamp(-5.0, 0.0) as f32));
            }
        }
        for (name, x) in &context {
            if *x != 0.0 {
                f.push((format!("c:{}:{}", class, name), *x));
            }
        }
        // What the answer names
        match ty.as_str() {
            "play" => {
                ctx.card(&mut f, "play", &a["cardId"]);
                let pool: Vec<Value> = d["play"].as_array().into_iter().flatten().map(|p| p["cardId"].clone()).collect();
                ctx.relative(&mut f, "playrel", &a["cardId"], &pool);
                let dest = a["destination"].as_str().unwrap_or("");
                f.push((format!("play:dest={}", dest), 1.0));
                if let Some(c) = a["cardId"].as_i64().and_then(|id| ctx.info(id).as_ref()) {
                    let (cost, key, zone) = (c.cost, c.key.clone(), c.zone.clone());
                    f.push((format!("play:from={}", zone), 1.0));
                    f.push((format!("play:key={}:dest={}", key, dest), 1.0));
                    f.push(("play:manaLeft".into(), (mana_avail - cost) as f32));
                    if mana_avail > 0.0 {
                        f.push(("play:costShare".into(), (cost / mana_avail) as f32));
                    }
                    let above = play_costs.iter().filter(|x| **x > cost).count();
                    let below = play_costs.iter().filter(|x| **x < cost).count();
                    if above == 0 {
                        f.push(("play:dearest".into(), 1.0));
                    }
                    if below == 0 {
                        f.push(("play:cheapest".into(), 1.0));
                    }
                    f.push((format!("play:day{}", num(&v0["day"]).min(4.0)), 1.0));
                    f.push((format!("play:key={}:day{}", key, num(&v0["day"]).min(4.0)), 1.0));
                }
                if cfg.dest_features {
                    if let (Some(side), Some(card)) = (side_of(dest), a["cardId"].as_i64().and_then(|id| ctx.cards.get(&id))) {
                        dest_feats(&mut f, &v0, &mine, &theirs, side, card);
                    }
                }
            }
            "support" => ctx.card(&mut f, "support", &a["cardId"]),
            "tap" => ctx.card(&mut f, "tap", &a["cardId"]),
            "select" => {
                let role = if grp == "mana" { "mana" } else if grp == "number" { "numdiscard" } else { "tgt" };
                let sel = ids(&a["cardIds"]);
                f.push((format!("{}:n", role), sel.len() as f32));
                if sel.is_empty() {
                    f.push((format!("{}:none", role), 1.0));
                }
                let pool = ids(&d["cardIds"]);
                if role != "mana" && sel.is_empty() {
                    for w in &words {
                        f.push((format!("tw:{}:none", w), 1.0));
                    }
                }
                for id in &sel {
                    ctx.card(&mut f, role, id);
                    let flags = ctx.relative(&mut f, &format!("{}rel", role), id, &pool);
                    if role != "mana" {
                        if let Some(c) = id.as_i64().and_then(|i| ctx.info(i).as_ref()) {
                            let place = match c.zone.as_str() {
                                "heroExpedition" | "companionExpedition" => "exp",
                                z => z,
                            };
                            let tags: Vec<String> = [if c.mine { "mine" } else { "opp" }.to_string(), format!("at={}", place)].into_iter().chain(flags.iter().map(|x| x.to_string())).collect();
                            let (power, cost) = (c.power, c.cost);
                            for w in &words {
                                for t in &tags {
                                    f.push((format!("tw:{}:{}", w, t), 1.0));
                                }
                                f.push((format!("tw:{}:power", w), power as f32));
                                f.push((format!("tw:{}:cost", w), cost as f32));
                            }
                        }
                    }
                    if role == "tgt" {
                        ctx.expedition_status(&mut f, &v0, id);
                        if let Some(c) = id.as_i64().and_then(|i| ctx.info(i).as_ref()) {
                            let m = if c.mine { "mine" } else { "opp" };
                            f.push((format!("tgt[{}]:{}", prompt, m), 1.0));
                            f.push((format!("tgt[{}]:power", prompt), c.power as f32));
                        }
                    }
                }
                if role == "mana" {
                    for id in &sel {
                        if let Some(c) = id.as_i64().and_then(|i| ctx.info(i).as_ref()) {
                            let key = c.key.clone();
                            f.push((format!("mana:key={}:day{}", key, num(&v0["day"]).min(4.0)), 1.0));
                        }
                    }
                }
            }
            "cleanup" => {
                let pool: Vec<Value> = ids(&d["reserve"]["cardIds"]).into_iter().chain(ids(&d["landmarks"]["cardIds"])).collect();
                for id in ids(&a["reserve"]).iter().chain(ids(&a["landmarks"]).iter()) {
                    ctx.card(&mut f, "clean", id);
                    ctx.relative(&mut f, "cleanrel", id, &pool);
                }
            }
            "exchange" => {
                ctx.card(&mut f, "xhand", &a["hand"]);
                ctx.card(&mut f, "xres", &a["reserve"]);
            }
            "choice" => {
                let c = d["choices"].as_array().and_then(|cs| cs.iter().find(|c| c["id"] == a["id"])).cloned().unwrap_or(Value::Null);
                let label = prompt_class(c["label"].as_str().unwrap_or(""));
                f.push((format!("choice:label={}", label), 1.0));
                if let Some(src) = c["sourceCardId"].as_i64() {
                    let key = ctx.info(src).as_ref().map(|c| c.key.clone()).unwrap_or_else(|| "?".into());
                    f.push((format!("choice:src={}", key), 1.0));
                    f.push((format!("choice:src={}:{}", key, label), 1.0));
                }
            }
            "number" => {
                let vals: Vec<f64> = d["values"].as_array().into_iter().flatten().map(num).collect();
                let x = num(&a["value"]);
                let lo = vals.iter().cloned().fold(f64::MAX, f64::min);
                let hi = vals.iter().cloned().fold(f64::MIN, f64::max);
                f.push(("num:above".into(), (x - lo) as f32));
                if x == hi {
                    f.push(("num:max".into(), 1.0));
                }
            }
            "expedition" => {
                for key in ids(&a["keys"]) {
                    let e = d["expeditions"].as_array().and_then(|es| es.iter().find(|e| e["key"] == key)).cloned().unwrap_or(Value::Null);
                    let m = if e["player"].as_i64() == Some(me) { "mine" } else { "opp" };
                    f.push((format!("exp:{}:{}", m, e["side"].as_str().unwrap_or("")), 1.0));
                    f.push((format!("exp[{}]:{}", prompt, m), 1.0));
                }
            }
            "location" => {
                let l = d["locations"].as_array().and_then(|ls| ls.iter().find(|l| l["key"] == a["key"])).cloned().unwrap_or(Value::Null);
                let m = if l["player"].as_i64() == Some(me) { "mine" } else { "opp" };
                f.push((format!("loc:{}:{}", m, l["zone"].as_str().unwrap_or("")), 1.0));
                f.push((format!("loc[{}]:{}", prompt, m), 1.0));
            }
            "player" => {
                let m = if a["player"].as_i64() == Some(me) { "mine" } else { "opp" };
                f.push((format!("player:{}", m), 1.0));
            }
            "button" => f.push((format!("btn:{}", prompt_class(a["key"].as_str().unwrap_or(""))), 1.0)),
            _ => {}
        }
        // The position after the answer, then after the end of the action, averaged over the
        // redealt copies
        let mut acc: Feats = vec![];
        let mut ok = 0;
        for (c, (b, (t0c, e0c))) in bases.iter().zip(starts.iter()).enumerate() {
            let mut s = b.clone();
            if s.answer(me as u32, &legal::with_id(d, a)).is_err() {
                if c == 0 {
                    break;
                }
                continue;
            }
            ok += 1;
            let vi = s.view(Some(me as u32), "").unwrap_or(Value::Null);
            push_deltas(&mut acc, "i", &terms(&vi, &cfg.weights), t0c);
            search::finish_action(&mut s, me, cfg.finish_steps);
            let vf = s.view(Some(me as u32), "").unwrap_or(Value::Null);
            push_deltas(&mut acc, "f", &terms(&vf, &cfg.weights), t0c);
            let w = winner_term(&vf);
            if w != 0.0 {
                acc.push(("f:win".into(), w as f32));
            }
            acc.push(("f:eval".into(), ((eval::evaluate(&vf, &cfg.weights) - e0c) / 10.0).clamp(-20.0, 20.0) as f32));
        }
        if ok == 0 {
            f.push(("refused".into(), 1.0));
            out.push(f);
            continue;
        }
        let acc: Feats = acc.into_iter().map(|(n, v)| (n, v / ok as f32)).collect();
        // The same changes per decision group (`g:GROUP:...`): their weight may differ
        for (n, v) in &acc {
            f.push((format!("g:{}:{}", grp, n), *v));
        }
        f.extend(acc);
        out.push(f);
    }
    out
}

fn side_of(dest: &str) -> Option<&'static str> {
    match dest {
        "heroExpedition" => Some("hero"),
        "companionExpedition" => Some("companion"),
        _ => None,
    }
}

fn region_biomes(view: &Value, pos: i64) -> Vec<String> {
    let r = view["regions"].as_array().and_then(|rs| rs.iter().find(|r| r["index"].as_i64() == Some(pos)));
    match r.and_then(|r| r["biomes"].as_array()) {
        Some(b) => b.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        None => vec!["forest".into(), "mountain".into(), "ocean".into()],
    }
}

/// A character played into one of my expeditions: how that expedition stands before (its
/// statistics against the opposing one, Dusk, distance) and how the card fits the region
fn dest_feats(f: &mut Feats, v0: &Value, mine: &Value, theirs: &Value, side: &str, card: &Value) {
    let other = if side == "hero" { "companion" } else { "hero" };
    let e = &mine["expeditions"][side];
    let o = &theirs["expeditions"][side];
    let total = |x: &Value| ["forest", "mountain", "ocean"].iter().map(|b| num(&x["stats"][*b])).sum::<f64>();
    let pos = e["position"].as_i64().unwrap_or(0);
    let biomes = region_biomes(v0, pos);
    let (mut margin, mut wins, mut loses) = (f64::MIN, false, false);
    let mut fit = 0.0f64;
    for b in &biomes {
        let (x, y) = (num(&e["stats"][b]), num(&o["stats"][b]));
        margin = margin.max(x - y);
        wins |= x > y && x > 0.0;
        loses |= y > x && y > 0.0;
        fit = fit.max(num(&card["stats"][b]));
    }
    let best = ["forest", "mountain", "ocean"].iter().map(|b| num(&card["stats"][*b])).fold(0.0, f64::max);
    f.push(("dst:mine".into(), total(e) as f32));
    f.push(("dst:opp".into(), total(o) as f32));
    f.push(("dst:sideDiff".into(), (total(e) - total(&mine["expeditions"][other])) as f32));
    f.push(("dst:margin".into(), margin.clamp(-6.0, 6.0) as f32));
    if wins {
        f.push(("dst:wins".into(), 1.0));
    }
    if loses {
        f.push(("dst:oppWins".into(), 1.0));
    }
    if total(e) == 0.0 {
        f.push(("dst:empty".into(), 1.0));
    }
    f.push(("dst:fit".into(), fit as f32));
    f.push(("dst:fitLoss".into(), (best - fit) as f32));
    let dist = if side == "hero" { pos as f64 } else { 7.0 - pos as f64 };
    f.push(("dst:dist".into(), dist as f32));
    // How far the opposing expedition of the same side is from its start
    let opos = o["position"].as_i64().unwrap_or(0);
    let odist = if side == "hero" { opos as f64 } else { 7.0 - opos as f64 };
    f.push(("dst:oppDist".into(), odist as f32));
    f.push((format!("dst:{}", side), 1.0));
}

fn push_deltas(f: &mut Feats, tag: &str, t: &[[f64; 12]; 2], t0: &[[f64; 12]; 2]) {
    for (side, name) in ["me", "op"].iter().enumerate() {
        for (k, term) in TERMS.iter().enumerate() {
            let x = t[side][k] - t0[side][k];
            if x != 0.0 {
                f.push((format!("{}:{}:{}", tag, name, term), x as f32));
            }
        }
    }
}

/// A copy of `g` with every card id of `a` replaced by what the card is (class, place, state),
/// so that two answers naming two copies of the same card compare equal
pub fn abstract_answer(g: &Game, a: &Value) -> Value {
    let desc = |id: &Value| -> Value {
        let Some(c) = id.as_i64().and_then(|i| g.cards.get(&i)) else { return id.clone() };
        let uid = c.props.get("uid").map(|u| u.to_string()).unwrap_or_default();
        let tapped = c.props.get("tapped").map(|t| t.to_string()).unwrap_or_default();
        let boost = c.props.get("boost").map(|t| t.to_string()).unwrap_or_default();
        Value::String(format!("{}|{}|{}|{}|{}", uid, c.location, c.pid, tapped, boost))
    };
    let mut a = legal::canonical(a);
    if let Some(o) = a.as_object_mut() {
        for k in ["cardId", "hand", "reserve", "cardIds", "landmarks"] {
            match o.get_mut(k) {
                Some(Value::Array(v)) => {
                    let mut x: Vec<Value> = v.iter().map(desc).collect();
                    x.sort_by_key(|y| y.to_string());
                    *v = x;
                }
                Some(v) if v.is_number() => *v = desc(v),
                _ => {}
            }
        }
    }
    a
}
