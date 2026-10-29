//! One traced game: AI_TRACE=1 cargo run --release -p altered-ai --example trace -- D01 D02 SEED
use altered_ai::play::{play_game, Agent, SimpleBot};
use altered_ai::search::{Ai, Params};
fn main() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}
fn run() {
    let a: Vec<String> = std::env::args().collect();
    let decks: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("server/goldens/tournament_decks.json").unwrap()).unwrap();
    let pair = [decks[&a[1]].clone(), decks[&a[2]].clone()];
    let mut agents: [Box<dyn Agent>; 2] = [Box::new(Ai::new("ai", Params::default())), Box::new(SimpleBot)];
    let r = play_game(&pair, &mut agents, a[3].parse().unwrap());
    eprintln!("{:?}", r);
}
