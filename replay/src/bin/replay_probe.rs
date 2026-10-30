//! Checks the replay setup and the notification signatures on replays whose answers are known
//! (golden files recorded by the PHP oracle): the created game against the golden's tables, then
//! each answer's notifications against BGA's.
//!
//!   cargo run --release -p altered-replay --bin replay_probe -- GOLDEN_DIR BGA_DIR [TABLE]

use altered_replay::{bga::BgaReplay, setup, sig};
use altered_rules::load;
use serde_json::Value;

fn read(path: &str) -> Value {
    let b = std::fs::read(path).expect(path);
    let mut de = serde_json::Deserializer::from_slice(&b);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (gdir, bdir) = (&args[0], &args[1]);
    let mut tables: Vec<String> = std::fs::read_dir(gdir).unwrap().flatten().map(|e| e.path().file_stem().unwrap().to_string_lossy().to_string()).collect();
    tables.sort();
    if let Some(t) = args.get(2) {
        tables.retain(|x| x == t);
    }
    for t in tables {
        let golden = read(&format!("{}/{}.json", gdir, t));
        let r = BgaReplay::parse(&std::fs::read_to_string(format!("{}/{}.json", bdir, t)).unwrap()).unwrap();
        let g = match setup::create(&r) {
            Ok(g) => g,
            Err(e) => {
                println!("{} setup error: {}", t, e);
                continue;
            }
        };
        let diffs: Vec<String> = load::diff_tables(&golden["db"], &g.bga_tables()).into_iter().filter(|d| !d.contains("tokenStyles")).collect();
        if args.iter().any(|a| a == "--setup") {
            println!("{} setup: {} table differences{}", t, diffs.len(), diffs.iter().take(6).map(|d| format!("\n    {}", &d[..d.len().min(300)])).collect::<String>());
        }
        let mut g = g;
        for p in golden["db"]["user_preferences"].as_array().into_iter().flatten() {
            let n = |k: &str| p[k].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            setup::set_pref(&mut g, n("player_id"), n("pref_id"), n("pref_value"));
        }
        // Answers of the golden's main line, grouped by move
        let mut line: Vec<(i64, u32, Value, Value)> = vec![];
        let mut node = &golden["tree"];
        while let (Some(p), Some(next)) = (node["player"].as_u64(), node["next"].as_array().and_then(|n| n.first())) {
            line.push((node["move"].as_i64().unwrap_or(0), p as u32, next["answer"].clone(), node["nextIds"].clone()));
            node = &next["node"];
        }
        let per_move = |m: i64| line.iter().filter(|x| x.0 == m).count();
        let moves: std::collections::HashMap<i64, &altered_replay::bga::Move> = r.moves.iter().map(|m| (m.id, m)).collect();
        let (mut ok, mut bad, mut skipped) = (0, 0, 0);
        let mut first: Option<String> = None;
        for (m, p, a, next_ids) in &line {
            g.fw_set(3, *m);
            if let Some(id) = next_ids["cards"].as_i64() { g.next_card_id = id; }
            if let Some(id) = next_ids["meeples"].as_i64() { g.next_meeple_id = id; }
            let before = sig::positions(&g);
            let notifs = match altered_replay::play(&mut g, *p, a) {
                Ok(n) => n,
                Err(e) => {
                    first.get_or_insert(format!("move {}: engine refused {}: {}", m, a, e));
                    break;
                }
            };
            if std::env::var("DUMP").ok().and_then(|d| d.parse::<i64>().ok()) == Some(*m) {
                for n in &notifs { println!("ENGINE {}", &n.to_string()[..n.to_string().len().min(700)]); }
                for n in &moves[m].notifs { println!("BGA {:?} {} {}", n.channel, n.kind, &n.args.to_string()[..n.args.to_string().len().min(700)]); }
            }
            let Some(bm) = moves.get(m) else { skipped += 1; continue };
            if per_move(*m) != 1 || a["type"] == "undo" { skipped += 1; continue; }
            match sig::diff(&g, bm, &before, &sig::positions(&g), &notifs) {
                None => ok += 1,
                Some(d) => {
                    bad += 1;
                    if std::env::var("ALL").is_ok() { println!("  move {} ({}): {}", m, a["type"], &d[..d.len().min(400)]); }
                    first.get_or_insert(format!("move {} ({}): {}", m, a["type"], d));
                }
            }
        }
        println!("{} {} answers: {} same effects, {} different, {} not compared{}", t, line.len(), ok, bad, skipped, first.map(|f| format!("\n    first: {}", &f[..f.len().min(600)])).unwrap_or_default());
    }
}
