//! Bot against bot on starter decks: every game the arena creates must reach its end (the bot
//! answers every decision, through its last resort when Bot.php's answers are refused), and the
//! views of both players and of a spectator must build at every step.

use altered_rules::arena::NewPlayer;
use altered_rules::managers::cards::STARTER_DECKS;
use altered_rules::Game;

#[test]
fn bot_games_end() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(run).unwrap().join().unwrap();
}

fn run() {
    let n = STARTER_DECKS.len();
    for i in 0..12 {
        let decks = [STARTER_DECKS[(i * 7) % n].0, STARTER_DECKS[(i * 11 + 3) % n].0];
        let players: Vec<NewPlayer> = decks.iter().map(|d| NewPlayer { name: "Bot".into(), bot: true, deck: Some(d.to_string()) }).collect();
        let dice: Vec<i64> = (0..100).map(|k| (k * 7 + i as i64) % 6 + 1).collect();
        let mut g = Game::create(1000 + i as i64, &players, &dice).unwrap();
        for _ in 0..50 {
            if g.is_over() {
                break;
            }
            g.run_bots().unwrap();
            for p in [Some(2001), Some(2002), None] {
                g.view(p, "test").unwrap();
            }
        }
        assert!(g.is_over(), "game {} ({:?}) did not end: {}", i, decks, g.oracle_state()["state"]);
    }
}
