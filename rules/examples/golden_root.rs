//! Loads golden files and prints the root decision (or the error raised computing it)
//!
//!   cargo run --release -p altered-rules --example golden_root -- server/goldens/rich/AX_Common_TikTok.json ...

use altered_rules::Game;
use serde_json::Value;

fn main() {
    for path in std::env::args().skip(1) {
        let data: Value = serde_json::from_slice(&std::fs::read(&path).expect("file")).expect("json");
        let dice: Vec<i64> = data["dice"].as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
        match Game::from_bga_tables(&data["db"], &dice) {
            Err(e) => println!("{}: cannot load: {}", path, e),
            Ok(g) => {
                let p = data["tree"]["player"].as_u64().unwrap_or(0) as u32;
                match g.try_decision(p) {
                    Ok(d) => println!("{}: {}", path, d.map(|d| d.to_string()).unwrap_or("null".into())),
                    Err(e) => println!("{}: error: {}", path, e),
                }
            }
        }
    }
}
