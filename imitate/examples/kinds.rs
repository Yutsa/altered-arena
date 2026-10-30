//! Prints two decisions of each kind met in native replays: kinds FILE...
use altered_replay::native::{self, NativeReplay};
fn main() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}
fn run() {
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    for f in std::env::args().skip(1) {
        let n: NativeReplay = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let mut g = native::start(&n).unwrap();
        for s in &n.steps {
            if let Ok(Some(d)) = g.try_decision(s.p) {
                let k = d["kind"].as_str().unwrap_or("").to_string();
                let c = seen.entry(k.clone()).or_default();
                if *c < 2 && k != "turn" {
                    *c += 1;
                    println!("{}\n  ANSWER {}", d, s.a);
                }
            } else if s.a["type"] != "act" {
                println!("NO DECISION for {}", s.a);
            }
            native::apply(&mut g, s).unwrap();
        }
    }
}
