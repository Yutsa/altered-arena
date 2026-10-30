//! Prints the view and decision before step K of a native replay: peek FILE K
use altered_replay::native::{self, NativeReplay};
fn main() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}
fn run() {
    let a: Vec<String> = std::env::args().collect();
    let n: NativeReplay = serde_json::from_str(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let k: usize = a[2].parse().unwrap();
    let mut g = native::start(&n).unwrap();
    for s in &n.steps[..k] {
        native::apply(&mut g, s).unwrap();
    }
    let s = &n.steps[k];
    let d = g.try_decision(s.p).unwrap();
    let mut v = g.view(Some(s.p), "x").unwrap();
    v["log"] = serde_json::json!([]);
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
    println!("DECISION {}", serde_json::to_string_pretty(&d).unwrap());
    println!("ANSWER {}", s.a);
    let c = g.cards.values().next().unwrap();
    println!("PROPS {:?}", c.props.to_map());
    println!("DB {:?}", c.db_props.to_map());
}
