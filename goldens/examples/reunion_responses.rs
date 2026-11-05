//! Converts decks of decks-api (`GET /api/bga/decks/{id}`, one JSON file each) into what the
//! publisher API answers `get_player_deck_content` (`altered_rules::reunion::publisher_response`),
//! the input of `php tools/api_decks.php decks OUT_DIR RESPONSES` (real decks in the goldens).
//!
//!   cargo run --release -p goldens --example reunion_responses -- RAW_DIR OUT.json

use serde_json::Value;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dir, out) = (args.first().expect("RAW_DIR"), args.get(1).expect("OUT.json"));
    let mut files: Vec<_> =
        std::fs::read_dir(dir).expect("RAW_DIR").flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    files.sort();
    let mut responses = vec![];
    for f in files {
        let Some(deck) = std::fs::read(&f).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { continue };
        match altered_rules::reunion::publisher_response(&deck) {
            Ok(r) => responses.push(r),
            Err(e) => eprintln!("{}: {}", f.display(), e),
        }
    }
    std::fs::write(out, serde_json::to_vec(&responses).unwrap()).expect("OUT.json");
    println!("{} decks", responses.len());
}
