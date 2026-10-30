//! Command line of the imitation AIs (`docs/ai/IMITATION.md`).
//!
//!   imitate extract [--threads T]          the decision dataset (features, labels)
//!   imitate baseline [--threads T]         the simple bot's and the deck AIs' agreement
//!   imitate cv [--drop P1,P2] [--l-shared X] [--l-player Y] [--tag NAME]
//!                                          leave-one-game-out score (cv/NAME.json)
//!   imitate fit [--drop ..] [..]           the final model: shared.json, players/*.json
//!   imitate gate [--games N]               the AIs in full games against the deck AIs
//!
//! Data: native replays of `private/tournament-617607/native`, output in
//! `private/tournament-617607/imitation` (git-ignored).

use altered_ai::play::{self, Agent};
use altered_ai::search::{Ai, Params};
use altered_imitate::agent::{Imitator, Member, PlayerFile, Shared};
use altered_imitate::cv::{self, Summary};
use altered_imitate::dataset::{self, Interner, Tables};
use altered_imitate::features::Config;
use altered_imitate::model::Hyper;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

const PRIVATE: &str = "private/tournament-617607";
const OUT: &str = "private/tournament-617607/imitation";

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn replays() -> Vec<String> {
    let dir = format!("{}/native", PRIVATE);
    let mut files: Vec<String> = std::fs::read_dir(&dir).expect("native").flatten().map(|f| f.path().to_string_lossy().to_string()).filter(|f| f.ends_with(".replay.json")).collect();
    files.sort();
    if let Some(n) = std::env::var("IMIT_FILES").ok().and_then(|s| s.parse::<usize>().ok()) {
        files.truncate(n);
    }
    files
}

fn tables() -> Tables {
    Tables { seats: serde_json::from_str(&std::fs::read_to_string(format!("{}/table_seats.json", PRIVATE)).expect("table_seats")).unwrap() }
}

fn hyper(args: &[String]) -> Hyper {
    let mut h = Hyper::default();
    if let Some(x) = opt(args, "--l-shared") {
        h.l_shared = x.parse().unwrap();
    }
    if let Some(x) = opt(args, "--l-player") {
        h.l_player = x.parse().unwrap();
    }
    if let Some(x) = opt(args, "--iters") {
        h.iters = x.parse().unwrap();
    }
    if let Some(x) = opt(args, "--cand-power") {
        h.cand_power = x.parse().unwrap();
    }
    h
}

/// `--player-mult PREFIX:X,PREFIX:X`: l_player times X for the features of a prefix (`~x`: the
/// names containing x); the first matching rule applies
fn player_mult(args: &[String], names: &[String]) -> Vec<f64> {
    let Some(spec) = opt(args, "--player-mult") else { return vec![] };
    let rules: Vec<(String, f64)> = spec.split(',').filter_map(|r| r.rsplit_once(':').map(|(p, x)| (p.to_string(), x.parse().unwrap()))).collect();
    names
        .iter()
        .map(|n| {
            rules.iter().find(|(p, _)| if let Some(q) = p.strip_prefix('~') { n.contains(q) } else { n.starts_with(p.as_str()) }).map(|r| r.1).unwrap_or(1.0)
        })
        .collect()
}

fn drops(args: &[String]) -> Vec<String> {
    opt(args, "--drop").map(|s| s.split(',').map(String::from).collect()).unwrap_or_default()
}

fn data_dir(args: &[String]) -> String {
    opt(args, "--data").unwrap_or(OUT.to_string())
}

fn print_summary(s: &Summary, label: &str) {
    let all = &s.groups["ALL"];
    println!(
        "{}: player mean {:.2} %, decisions {:.2} % (top-3 {:.1} %, mean log-lik {:.3}, n {}), train {:.1} %, bot {:.1} %, deck AI {:.1} %",
        label, s.player_mean, s.decision_mean, s.top3, s.mean_ll, all.n, all.pct(all.train), all.base_pct(all.bot), all.base_pct(all.ai)
    );
    let groups: Vec<String> = s.groups.iter().filter(|(k, _)| *k != "ALL").map(|(k, r)| format!("{} {:.0}({})", k, r.pct(r.top1), r.n)).collect();
    println!("  {}", groups.join(", "));
}

fn main() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}

fn run() {
    let args: Vec<String> = std::env::args().collect();
    let threads: usize = opt(&args, "--threads").and_then(|s| s.parse().ok()).unwrap_or(12);
    match args.get(1).map(|s| s.as_str()) {
        Some("extract") => {
            let files = replays();
            let t = std::time::Instant::now();
            let tables = tables();
            let mut cfg = Config::default();
            if let Some(c) = opt(&args, "--copies") {
                cfg.copies = c.parse().unwrap();
            }
            if let Some(c) = opt(&args, "--seed") {
                cfg.seed = c.parse().unwrap();
            }
            if let Some(c) = opt(&args, "--finish") {
                cfg.finish_steps = c.parse().unwrap();
            }
            let interner = Mutex::new(Interner::default());
            let res = cv::par_map(&files, threads, |f| dataset::decisions(f, &tables, &cfg, &interner, None));
            std::fs::create_dir_all(data_dir(&args)).unwrap();
            std::fs::write(format!("{}/config.json", data_dir(&args)), serde_json::to_string(&json!({"copies": cfg.copies, "finish_steps": cfg.finish_steps, "seed": cfg.seed})).unwrap()).unwrap();
            let mut decs = vec![];
            let mut skipped = dataset::Skipped::default();
            for (d, _, s) in res {
                decs.extend(d);
                skipped.act += s.act;
                skipped.no_decision += s.no_decision;
                skipped.single += s.single;
                skipped.replay_error += s.replay_error;
            }
            let names = interner.into_inner().unwrap().names;
            let dir = data_dir(&args);
            dataset::write(&dir, &names, &decs);
            let unreachable = decs.iter().filter(|d| !d.labels.iter().any(|x| *x)).count();
            let cands: usize = decs.iter().map(|d| d.labels.len()).sum();
            println!(
                "extract: {} files, {} decisions, {} candidates, {} features, {} decisions whose answer is no candidate, skipped {:?}, {:.0}s",
                files.len(), decs.len(), cands, names.len(), unreachable, skipped, t.elapsed().as_secs_f64()
            );
        }
        Some("baseline") => {
            let files = replays();
            let t = std::time::Instant::now();
            let tables = tables();
            let cfg = Config::default();
            let interner = Mutex::new(Interner::default());
            let params = opt(&args, "--params").unwrap_or("ai/params".into());
            let res = cv::par_map(&files, threads, |f| dataset::decisions(f, &tables, &cfg, &interner, Some(&params)));
            use std::io::Write;
            let mut out = std::fs::File::create(format!("{}/baselines.jsonl", OUT)).unwrap();
            let (mut n, mut bot, mut ai) = (0, 0, 0);
            for (_, b, _) in res {
                for x in b {
                    n += 1;
                    bot += x.bot as usize;
                    ai += x.ai as usize;
                    writeln!(out, "{}", serde_json::to_string(&x).unwrap()).unwrap();
                }
            }
            println!("baseline: {} decisions, simple bot {:.1} %, deck AI {:.1} %, {:.0}s", n, 100.0 * bot as f64 / n as f64, 100.0 * ai as f64 / n as f64, t.elapsed().as_secs_f64());
        }
        Some("cv") => {
            let t = std::time::Instant::now();
            let (names, decs) = dataset::read(&data_dir(&args));
            let base = dataset::read_baselines(OUT);
            let keep = cv::keep(&names, &drops(&args));
            let mut h = hyper(&args);
            h.player_mult = player_mult(&args, &names);
            // --ensemble DIR,DIR: one more model per dataset (other redeals), scores averaged
            let preds = match opt(&args, "--ensemble") {
                Some(l) => {
                    let mut sets = vec![(names.clone(), decs.clone())];
                    sets.extend(l.split(',').map(dataset::read));
                    cv::logo_ensemble(&sets, &drops(&args), &h, threads)
                }
                None => cv::logo(&decs, names.len(), &keep, &h, threads),
            };
            let train = if args.iter().any(|a| a == "--train") { Some(cv::train_fit(&decs, names.len(), &keep, &h).0) } else { None };
            let s = cv::summary(&decs, &preds, &base, train.as_deref());
            let tag = opt(&args, "--tag").unwrap_or("last".into());
            print_summary(&s, &tag);
            std::fs::create_dir_all(format!("{}/cv", OUT)).unwrap();
            std::fs::write(format!("{}/cv/{}.json", OUT, tag), serde_json::to_string_pretty(&json!({"hyper": h, "drop": drops(&args), "summary": s})).unwrap()).unwrap();
            let p: Vec<Value> = decs.iter().zip(&preds).map(|(d, p)| json!({"table": d.table, "step": d.step, "player": d.player, "group": d.group, "prompt": d.prompt, "day": d.day, "n": d.labels.len(), "top1": p.top1, "best": p.best, "labels": d.labels.iter().enumerate().filter(|(_, x)| **x).map(|(i, _)| i).collect::<Vec<_>>()})).collect();
            std::fs::write(format!("{}/cv/{}.preds.json", OUT, tag), serde_json::to_string(&p).unwrap()).unwrap();
            eprintln!("cv {:.0}s", t.elapsed().as_secs_f64());
        }
        Some("fit") => {
            // One member per dataset: the main one, then --ensemble DIR,DIR (other redeals)
            let mut dirs = vec![data_dir(&args)];
            dirs.extend(opt(&args, "--ensemble").map(|l| l.split(',').map(String::from).collect::<Vec<_>>()).unwrap_or_default());
            let mut members: Vec<(u64, BTreeMap<String, f64>, Vec<BTreeMap<String, f64>>)> = vec![];
            let mut players: Vec<String> = vec![];
            let mut first: Option<(Vec<String>, Vec<dataset::Decision>, Vec<bool>)> = None;
            let h0 = hyper(&args);
            for dir in &dirs {
                let (names, decs) = dataset::read(dir);
                let keep = cv::keep(&names, &drops(&args));
                let mut h = h0.clone();
                h.player_mult = player_mult(&args, &names);
                let (preds, theta, m) = cv::train_fit(&decs, names.len(), &keep, &h);
                let s = cv::summary(&decs, &preds, &dataset::read_baselines(OUT), None);
                print_summary(&s, &format!("training set ({})", dir));
                let f = names.len();
                let raw = |j: usize, x: f64| x / m.scale[j];
                let weights: BTreeMap<String, f64> = (0..f).filter(|&j| keep[j] && theta[j] != 0.0).map(|j| (names[j].clone(), raw(j, theta[j]))).collect();
                let offsets: Vec<BTreeMap<String, f64>> = (0..m.players.len())
                    .map(|p| (0..f).filter(|&j| keep[j] && theta[f * (1 + p) + j].abs() > 1e-9).map(|j| (names[j].clone(), raw(j, theta[f * (1 + p) + j]))).collect())
                    .collect();
                let dcfg: Value = std::fs::read_to_string(format!("{}/config.json", dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
                members.push((dcfg["seed"].as_u64().unwrap_or(0), weights, offsets));
                players = m.players.clone();
                if first.is_none() {
                    first = Some((names, decs, keep));
                }
            }
            let (names, decs, keep) = first.unwrap();
            let version = format!("{:016x}", {
                use std::hash::{Hash, Hasher};
                let mut hs = std::collections::hash_map::DefaultHasher::new();
                format!("{:?}{:?}", members.iter().map(|m| &m.1).collect::<Vec<_>>(), h0).hash(&mut hs);
                hs.finish()
            });
            let search_pick = names.iter().zip(&keep).any(|(n, k)| *k && n.starts_with("s:pick"));
            let dcfg: Value = std::fs::read_to_string(format!("{}/config.json", data_dir(&args))).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
            let copies = dcfg["copies"].as_u64().unwrap_or(1) as usize;
            let finish_steps = dcfg["finish_steps"].as_u64().map(|x| x as usize).unwrap_or(Config::default().finish_steps);
            let more: Vec<Member> = members[1..].iter().map(|(seed, w, _)| Member { seed: *seed, weights: w.clone() }).collect();
            let shared = Shared { version: version.clone(), finish_steps, search_pick, copies, weights: members[0].1.clone(), more };
            std::fs::create_dir_all(format!("{}/players", OUT)).unwrap();
            std::fs::write(format!("{}/shared.json", OUT), serde_json::to_string_pretty(&shared).unwrap()).unwrap();
            let deck_of: HashMap<&str, &str> = decs.iter().map(|d| (d.player.as_str(), d.deck.as_str())).collect();
            let deck_info: Value = std::fs::read_to_string(format!("{}/decks.json", PRIVATE)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
            for (p, name) in players.iter().enumerate() {
                let deck = deck_of[name.as_str()].to_string();
                let hero = deck_info[&deck]["heroName"].as_str().unwrap_or("").to_string();
                let label = format!("{} — {}", name, hero);
                let pf = PlayerFile {
                    shared: version.clone(),
                    player: name.clone(),
                    deck,
                    hero,
                    label,
                    offsets: members[0].2[p].clone(),
                    more_offsets: members[1..].iter().map(|m| m.2[p].clone()).collect(),
                };
                std::fs::write(format!("{}/players/{}.json", OUT, file_name(&pf.label)), serde_json::to_string_pretty(&pf).unwrap()).unwrap();
            }
            println!("fit: shared {} ({} weights, {} members), {} players -> {}/players", version, shared.weights.len(), members.len(), players.len(), OUT);
        }
        Some("gate") => gate(&args, threads),
        Some("report") => report(&args),
        Some("check") => check(&args, threads),
        _ => eprintln!("usage: imitate extract | baseline | cv | fit | gate"),
    }
}

/// A file name for a player (letters and digits kept)
pub fn file_name(p: &str) -> String {
    let s: String = p.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    s.trim_matches('_').to_string()
}

/// Every player's AI plays `games` full games against the deck AIs of other decks (mixed
/// opponents, fixed seeds), and the same matchups are played by the player's deck AI: no panic,
/// no stuck game, and the win rate of each against the same field.
fn gate(args: &[String], threads: usize) {
    let games: u64 = opt(args, "--games").and_then(|s| s.parse().ok()).unwrap_or(20);
    let decks: Value = serde_json::from_str(&std::fs::read_to_string("server/goldens/tournament_decks.json").unwrap()).unwrap();
    let shared: Shared = serde_json::from_str(&std::fs::read_to_string(format!("{}/shared.json", OUT)).unwrap()).unwrap();
    let mut files: Vec<PlayerFile> = std::fs::read_dir(format!("{}/players", OUT)).unwrap().flatten().filter_map(|f| serde_json::from_str(&std::fs::read_to_string(f.path()).ok()?).ok()).collect();
    files.sort_by(|a, b| a.deck.cmp(&b.deck));
    let ids: Vec<String> = decks.as_object().unwrap().keys().cloned().collect();
    let read_params = |d: &str| -> Params { std::fs::read_to_string(format!("ai/params/{}.json", d)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() };
    // (player index, game, imitation AI or deck AI)
    let jobs: Vec<(usize, u64, bool)> = (0..files.len()).flat_map(|p| (0..games).flat_map(move |g| [(p, g, true), (p, g, false)])).collect();
    let t = std::time::Instant::now();
    let res = play::run_parallel(
        &jobs,
        threads,
        |&(p, g, imit)| {
            let pf = &files[p];
            let i = ids.iter().position(|x| *x == pf.deck).unwrap_or(0);
            let opp = &ids[(i + 1 + (g as usize * 7) % (ids.len() - 1)) % ids.len()];
            let me: Box<dyn Agent> = if imit { Box::new(Imitator::new("imit", &shared, Some(pf), Some(read_params(&pf.deck)))) } else { Box::new(Ai::new("deck", read_params(&pf.deck))) };
            let other: Box<dyn Agent> = Box::new(Ai::new("opp", read_params(opp)));
            let swap = g % 2 == 1;
            let (pair, mut agents, seat) = if swap { ([decks[opp].clone(), decks[&pf.deck].clone()], [other, me], 1) } else { ([decks[&pf.deck].clone(), decks[opp].clone()], [me, other], 0) };
            let r = play::play_game(&pair, &mut agents, 300_000 + p as u64 * 1000 + g);
            (r.winner.map(|w| w == seat), r.error, r.day, r.refused[seat], r.millis)
        },
        |_, msg| (None, Some(format!("panic: {}", msg)), 0, 0, 0),
    );
    let mut rows = vec![];
    let (mut tw, mut tn, mut dw, mut dn) = (0.0, 0.0, 0.0, 0.0);
    for (p, pf) in files.iter().enumerate() {
        let mine: Vec<_> = jobs.iter().zip(&res).filter(|((q, _, _), _)| *q == p).collect();
        let stat = |imit: bool| {
            let r: Vec<_> = mine.iter().filter(|((_, _, i), _)| *i == imit).map(|(_, r)| r).collect();
            let wins = r.iter().filter(|x| x.0 == Some(true)).count();
            let decided = r.iter().filter(|x| x.0.is_some()).count();
            let errors: Vec<String> = r.iter().filter_map(|x| x.1.clone()).collect();
            let refused: usize = r.iter().map(|x| x.3).sum();
            let ms: u64 = r.iter().map(|x| x.4).sum();
            (wins, decided, errors, refused, ms as f64 / r.len().max(1) as f64)
        };
        let (iw, id, ie, ir, ims) = stat(true);
        let (aw, ad, ae, _, _) = stat(false);
        tw += iw as f64;
        tn += id as f64;
        dw += aw as f64;
        dn += ad as f64;
        let rate = 100.0 * iw as f64 / id.max(1) as f64;
        rows.push(json!({"player": pf.player, "deck": pf.deck, "imitation": {"wins": iw, "decided": id, "errors": ie, "refused": ir, "ms": ims},
            "deckAi": {"wins": aw, "decided": ad, "errors": ae}, "collapse": rate < 15.0}));
        println!("{} {}: imitation {}/{} ({:.0} %), deck AI {}/{}, errors {} / {}, refused {}, {:.0} ms/game{}", pf.deck, file_name(&pf.player), iw, id, rate, aw, ad, ie.len(), ae.len(), ir, ims, if rate < 15.0 { "  COLLAPSE" } else { "" });
    }
    println!("gate: imitation {:.1} % ({} games), deck AI {:.1} % on the same matchups, {:.0}s", 100.0 * tw / tn.max(1.0), tn, 100.0 * dw / dn.max(1.0), t.elapsed().as_secs_f64());
    std::fs::write(format!("{}/gate.json", OUT), serde_json::to_string_pretty(&json!({"games": games, "rows": rows, "imitation": tw / tn.max(1.0), "deckAi": dw / dn.max(1.0)})).unwrap()).unwrap();
}

fn pct(r: &Value, k: &str, n: &str) -> f64 {
    100.0 * r[k].as_f64().unwrap_or(0.0) / r[n].as_f64().unwrap_or(0.0).max(1.0)
}

/// `report --tag T --before B`: REPORT.md from the cross-validation runs T (final) and B (the
/// first model), the gate and the players' files
fn report(args: &[String]) {
    let tag = opt(args, "--tag").unwrap_or("last".into());
    let before = opt(args, "--before").unwrap_or("v1".into());
    let read = |p: String| -> Value { std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null) };
    let cv = read(format!("{}/cv/{}.json", OUT, tag));
    let b = read(format!("{}/cv/{}.json", OUT, before));
    let gate = read(format!("{}/gate.json", OUT));
    let preds = read(format!("{}/cv/{}.preds.json", OUT, tag));
    let s = &cv["summary"];
    let mut files: Vec<PlayerFile> = std::fs::read_dir(format!("{}/players", OUT)).map(|d| d.flatten().filter_map(|f| serde_json::from_str(&std::fs::read_to_string(f.path()).ok()?).ok()).collect()).unwrap_or_default();
    files.sort_by(|a, b| a.deck.cmp(&b.deck));
    let mut o = String::new();
    o += "# Imitation AIs of tournament 617607: report\n\n";
    o += "One AI per player: the shared softmax model plus the player's offsets, on the player's deck. Score: leave-one-game-out top-1 agreement (the held-out game is out of the shared model and of the offsets).\n\n";
    let all = &s["groups"]["ALL"];
    o += &format!("## Overall\n\n| | player mean | decisions | top-3 | mean log-lik |\n|---|---|---|---|---|\n| first model ({}) | {:.2} % | {:.2} % | {:.1} % | {:.3} |\n| final ({}) | **{:.2} %** | **{:.2} %** | {:.1} % | {:.3} |\n\n",
        before, b["summary"]["player_mean"].as_f64().unwrap_or(0.0), b["summary"]["decision_mean"].as_f64().unwrap_or(0.0), b["summary"]["top3"].as_f64().unwrap_or(0.0), b["summary"]["mean_ll"].as_f64().unwrap_or(0.0),
        tag, s["player_mean"].as_f64().unwrap_or(0.0), s["decision_mean"].as_f64().unwrap_or(0.0), s["top3"].as_f64().unwrap_or(0.0), s["mean_ll"].as_f64().unwrap_or(0.0));
    let mean_of = |k: &str, n: &str| -> f64 {
        let ps = s["players"].as_object().unwrap();
        ps.values().map(|r| pct(r, k, n)).sum::<f64>() / ps.len().max(1) as f64
    };
    o += &format!("Baselines on the same {} decisions (decision-weighted / player mean): simple bot {:.1} % / {:.1} %, deck AI {:.1} % / {:.1} % (in-sample: its card priors were fit on these replays). Training-set agreement (first member of the final model): {:.1} % / {:.1} %.\n\n",
        all["n"], pct(all, "bot", "base_n"), mean_of("bot", "base_n"), pct(all, "ai", "base_n"), mean_of("ai", "base_n"), pct(all, "train", "n"), mean_of("train", "n"));
    o += "### By decision kind (held-out top-1)\n\n| kind | decisions | final | first model | simple bot | deck AI |\n|---|---|---|---|---|---|\n";
    for (g, r) in s["groups"].as_object().unwrap() {
        let rb = &b["summary"]["groups"][g];
        o += &format!("| {} | {} | {:.1} % | {:.1} % | {:.1} % | {:.1} % |\n", g, r["n"], pct(r, "top1", "n"), pct(rb, "top1", "n"), pct(r, "bot", "base_n"), pct(r, "ai", "base_n"));
    }
    o += "\n### By day\n\n| day | decisions | final | first model |\n|---|---|---|---|\n";
    for (g, r) in s["days"].as_object().unwrap() {
        o += &format!("| {} | {} | {:.1} % | {:.1} % |\n", g, r["n"], pct(r, "top1", "n"), pct(if b["summary"]["days"][g].is_null() { &b["summary"]["days"][g.trim_end_matches('+')] } else { &b["summary"]["days"][g] }, "top1", "n"));
    }
    o += "\n## Players\n\nHeld-out top-1 per decision kind: turn / Mana / target / choice / other.\n\n| AI (player — hero) | deck | games | decisions | simple bot | deck AI | first model | **final** | top-3 | train | turn | Mana | target | choice | other | gate wins | deck AI wins |\n|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n";
    let gate_rows: HashMap<String, Value> = gate["rows"].as_array().into_iter().flatten().map(|r| (r["player"].as_str().unwrap_or("").to_string(), r.clone())).collect();
    for pf in &files {
        let r = &s["players"][&pf.player];
        let rb = &b["summary"]["players"][&pf.player];
        let pg = &s["player_groups"][&pf.player];
        let g = |k: &str| if pg[k]["n"].as_u64().unwrap_or(0) > 0 { format!("{:.0}", pct(&pg[k], "top1", "n")) } else { "-".into() };
        let mut other = (0.0, 0.0);
        for (k, v) in pg.as_object().into_iter().flatten() {
            if !["turn", "mana", "target", "choice"].contains(&k.as_str()) {
                other.0 += v["top1"].as_f64().unwrap_or(0.0);
                other.1 += v["n"].as_f64().unwrap_or(0.0);
            }
        }
        let gr = &gate_rows.get(&pf.player).cloned().unwrap_or(Value::Null);
        let gw = |k: &str| format!("{}/{}", gr[k]["wins"], gr[k]["decided"]);
        o += &format!("| {} | {} | {} | {} | {:.1} | {:.1} | {:.1} | **{:.1}** | {:.1} | {:.1} | {} | {} | {} | {} | {:.0} | {}{} | {} |\n",
            if pf.label.is_empty() { &pf.player } else { &pf.label }, pf.deck, s["games"][&pf.player], r["n"], pct(r, "bot", "base_n"), pct(r, "ai", "base_n"), pct(rb, "top1", "n"), pct(r, "top1", "n"), pct(r, "top3", "n"), pct(r, "train", "n"),
            g("turn"), g("mana"), g("target"), g("choice"), 100.0 * other.0 / other.1.max(1.0), gw("imitation"), if gr["collapse"] == true { " **collapse**" } else { "" }, gw("deckAi"));
    }
    // The most frequent remaining misses: decision group and prompt
    let mut misses: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for p in preds.as_array().into_iter().flatten() {
        let key = format!("{} | {}", p["group"].as_str().unwrap_or(""), altered_imitate::features::prompt_class(p["prompt"].as_str().unwrap_or("")));
        let e = misses.entry(key).or_default();
        e.1 += 1;
        if p["top1"] != true {
            e.0 += 1;
        }
    }
    let mut m: Vec<(String, (usize, usize))> = misses.into_iter().collect();
    m.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    o += "\n## Most frequent remaining misses (held-out)\n\n| kind / prompt | missed | of | rate |\n|---|---|---|---|\n";
    for (k, (x, n)) in m.iter().take(15) {
        o += &format!("| {} | {} | {} | {:.0} % |\n", k.replace('|', "/").replacen('/', "|", 0), x, n, 100.0 * *x as f64 / *n as f64);
    }
    if !gate.is_null() {
        let errors: usize = gate["rows"].as_array().into_iter().flatten().map(|r| r["imitation"]["errors"].as_array().map(|a| a.len()).unwrap_or(0)).sum();
        let collapses: Vec<String> = gate["rows"].as_array().into_iter().flatten().filter(|r| r["collapse"] == true).map(|r| r["deck"].as_str().unwrap_or("").to_string()).collect();
        o += &format!("\n## Gates\n\nEach AI played {} full games against the deck AIs of other decks (mixed opponents, fixed seeds, seats alternated); the player's deck AI played the same matchups. Imitation AIs win **{:.1} %**, the deck AIs {:.1} %. Errors (panics, stuck or unfinished games) in the imitation games: {}. AIs under 15 % wins: {}.\n",
            gate["games"], 100.0 * gate["imitation"].as_f64().unwrap_or(0.0), 100.0 * gate["deckAi"].as_f64().unwrap_or(0.0), errors, if collapses.is_empty() { "none".to_string() } else { collapses.join(", ") });
    }
    std::fs::write(format!("{}/REPORT.md", OUT), o).unwrap();
    println!("report -> {}/REPORT.md", OUT);
}

/// The exported AIs (`shared.json` + `players/`) against the model they come from: on the
/// decisions of the dataset, the agent, given the same random stream as the extraction, must
/// pick a candidate the fitted model ranks first
fn check(args: &[String], threads: usize) {
    // The fitted ensemble's best candidate: each member trained on its dataset, scores averaged
    let mut dirs = vec![data_dir(args)];
    dirs.extend(opt(args, "--ensemble").map(|l| l.split(',').map(String::from).collect::<Vec<_>>()).unwrap_or_default());
    let mut sum: Vec<Vec<f64>> = vec![];
    let mut decs = vec![];
    let mut labels: Vec<Vec<bool>> = vec![];
    for dir in &dirs {
        let (names, ds) = dataset::read(dir);
        let keep = cv::keep(&names, &drops(args));
        let mut h = hyper(args);
        h.player_mult = player_mult(args, &names);
        let (_, theta, m) = cv::train_fit(&ds, names.len(), &keep, &h);
        let mut s = vec![];
        for k in 0..m.ndec() {
            m.scores(&theta, k, &mut s);
            if sum.len() <= k {
                sum.push(vec![0.0; s.len()]);
            }
            for (x, y) in sum[k].iter_mut().zip(&s) {
                *x += y;
            }
        }
        if decs.is_empty() {
            labels = ds.iter().map(|d| d.labels.clone()).collect();
            decs = ds;
        }
    }
    let preds: Vec<altered_imitate::model::Pred> = sum
        .iter()
        .zip(&labels)
        .map(|(s, l)| {
            let best = (0..s.len()).fold(0, |b, k| if s[k] > s[b] { k } else { b });
            altered_imitate::model::Pred { top1: l[best], top3: false, logp: None, best }
        })
        .collect();
    let shared: Shared = serde_json::from_str(&std::fs::read_to_string(format!("{}/shared.json", OUT)).unwrap()).unwrap();
    let files: Vec<PlayerFile> = std::fs::read_dir(format!("{}/players", OUT)).unwrap().flatten().filter_map(|f| serde_json::from_str(&std::fs::read_to_string(f.path()).ok()?).ok()).collect();
    let by_player: HashMap<&str, &PlayerFile> = files.iter().map(|p| (p.player.as_str(), p)).collect();
    let best: HashMap<(i64, usize), (usize, bool)> = decs.iter().zip(&preds).map(|(d, p)| ((d.table, d.step), (p.best, p.top1))).collect();
    let tables = tables();
    let n: usize = opt(args, "--files").and_then(|s| s.parse().ok()).unwrap_or(6);
    let files_r: Vec<String> = replays().into_iter().take(n).collect();
    let read_params = |d: &str| -> Params { std::fs::read_to_string(format!("ai/params/{}.json", d)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() };
    let res = cv::par_map(&files_r, threads, |f| {
        let nr = dataset::read_replay(f).unwrap();
        let decks: [Value; 2] = [nr.decks[0].clone(), nr.decks[1].clone()];
        let mut agents: Vec<Imitator> = (0..2)
            .map(|s| {
                let deck = tables.deck_of(nr.table, play::SEATS[s] as u32);
                Imitator::new("check", &shared, by_player.get(nr.names[s].as_str()).copied(), Some(read_params(&deck)))
            })
            .collect();
        let mut g = altered_replay::native::start(&nr).unwrap();
        let (mut same, mut total, mut agree_real) = (0, 0, 0);
        for (k, s) in nr.steps.iter().enumerate() {
            if s.a["type"] != "act" {
                let mut ready = g.clone();
                dataset::prepare(&mut ready, s);
                if let (Ok(Some(d)), Some(&(b, top1))) = (ready.try_decision(s.p), best.get(&(nr.table, k))) {
                    let cands = altered_ai::legal::candidates(&d);
                    use rand::SeedableRng;
                    let base = nr.table as u64 * 7919 + k as u64;
                    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(base);
                    let obs = altered_ai::play::Observation::new(&ready, s.p as i64, &decks);
                    let a = agents[(s.p - 2001) as usize].decide_seeded(&obs, &d, base, &mut rng);
                    total += 1;
                    let canon = altered_ai::legal::canonical(&a);
                    if altered_ai::legal::canonical(&cands[b]) == canon {
                        same += 1;
                        agree_real += top1 as usize;
                    }
                }
            }
            altered_replay::native::apply(&mut g, s).unwrap();
        }
        (same, total, agree_real)
    });
    let (s, t): (usize, usize) = res.iter().fold((0, 0), |a, r| (a.0 + r.0, a.1 + r.1));
    println!("check: the agent picks the fitted model's best candidate at {} of {} decisions ({:.1} %)", s, t, 100.0 * s as f64 / t.max(1) as f64);
}
