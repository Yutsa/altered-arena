//! Replays BGA replays (the "logs" JSON of finished tables) on the engine.
//!
//!   cargo run --release -p altered-replay --bin replay -- [--out DIR] FILE_OR_DIR...
//!
//! Prints, per table, how far it replays; with --out, writes DIR/TABLE.json (the steps); with
//! --native DIR, writes the native replay of every game that replays to the end
//! (DIR/TABLE.replay.json) and checks it replays alone.

use altered_replay::{bga::BgaReplay, runner};
use rayon::prelude::*;
use std::path::PathBuf;

fn main() {
    rayon::ThreadPoolBuilder::new().stack_size(256 << 20).build_global().expect("thread pool");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args.iter().position(|a| a == "--out").and_then(|i| args.get(i + 1)).cloned();
    let native = args.iter().position(|a| a == "--native").and_then(|i| args.get(i + 1)).cloned();
    let mut files: Vec<PathBuf> = vec![];
    for a in args.iter().filter(|a| !a.starts_with("--") && Some(*a) != out.as_ref() && Some(*a) != native.as_ref()) {
        let p = PathBuf::from(a);
        if p.is_dir() {
            files.extend(std::fs::read_dir(&p).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")));
        } else {
            files.push(p);
        }
    }
    files.sort();
    let t = std::time::Instant::now();
    let results: Vec<(String, runner::Outcome)> = files
        .par_iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(f).ok()?;
            let r = match BgaReplay::parse(&text) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{}: {}", f.display(), e);
                    return None;
                }
            };
            let t = std::time::Instant::now();
            let o = runner::run(&r);
            if let (Some(dir), Some(n)) = (&native, altered_replay::native::from_outcome(&r, &o)) {
                std::fs::create_dir_all(dir).ok();
                let t = std::time::Instant::now();
                match altered_replay::native::replay(&n, |_, _| {}) {
                    Ok(_) => {
                        std::fs::write(format!("{}/{}.replay.json", dir, n.table), serde_json::to_string(&n).unwrap()).ok();
                        eprintln!("{}: native replay checked in {:.0} ms", n.table, t.elapsed().as_secs_f64() * 1000.0);
                    }
                    Err(e) => eprintln!("{}: native replay FAILED: {}", n.table, e),
                }
            }
            if let Some(dir) = &out {
                std::fs::create_dir_all(dir).ok();
                std::fs::write(format!("{}/{}.json", dir, o.table), serde_json::to_string(&o).unwrap()).ok();
            }
            Some((format!("{} ({:.1}s)", f.file_stem().unwrap().to_string_lossy(), t.elapsed().as_secs_f64()), o))
        })
        .collect();
    let mut full = 0;
    for (name, o) in &results {
        let ok = o.divergence.is_none();
        full += ok as usize;
        println!(
            "{} {}: {}/{} requests, {} steps{}{}{}",
            if ok { "OK  " } else { "FAIL" },
            name,
            o.replayed,
            o.requests,
            o.steps.len(),
            if o.game_over { ", game over" } else { "" },
            if o.closest > 0 || o.backtracks > 0 { format!(", {} closest, {} backtracks", o.closest, o.backtracks) } else { String::new() },
            o.divergence.as_ref().map(|d| format!("\n     {}", &d[..d.len().min(900)])).unwrap_or_default()
        );
    }
    println!("\n{}/{} replays to the end ({:.1}s)", full, results.len(), t.elapsed().as_secs_f64());
}
