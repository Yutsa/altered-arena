//! Prints the GameView of seat A after N random answers: cargo run --release -p altered-ai --example dump_view -- D01 D02 N
use altered_ai::legal;
use altered_ai::play::{new_game, Agent, Observation, RandomAgent};
use rand::SeedableRng;
fn main() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}
fn run() {
    let a: Vec<String> = std::env::args().collect();
    let decks: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("server/goldens/tournament_decks.json").unwrap()).unwrap();
    let pair = [decks[&a[1]].clone(), decks[&a[2]].clone()];
    let n: usize = a[3].parse().unwrap();
    let mut g = new_game(&pair, 7).unwrap();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1);
    let mut ag = RandomAgent;
    for _ in 0..n {
        let p = g.waiting_for()[0];
        let d = g.decision(p).unwrap();
        let ans = ag.decide(&Observation::new(&g, p as i64, &pair), &d, &mut rng);
        g.answer(p, &legal::with_id(&d, &ans)).unwrap();
    }
    let p = g.waiting_for()[0];
    let mut v = g.view(Some(p), "x").unwrap();
    v["log"] = serde_json::json!([]);
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
}
