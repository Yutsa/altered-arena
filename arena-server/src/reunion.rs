//! Altered Reunion decks: fetched from decks-api (DECKS_API_URL, `GET /api/bga/decks/{id}`,
//! public), converted by `altered_rules::reunion::publisher_response`.

use altered_rules::reunion::publisher_response;
use serde_json::Value;

/// decks-api base URL (DECKS_API_URL)
pub fn api_url() -> String {
    std::env::var("DECKS_API_URL").unwrap_or_else(|_| "https://decks.alteredcore.org".into())
}

/// A decks-api deck id: a UUID (the only thing put in the URL)
pub fn is_deck_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Fetches deck `id` from decks-api and converts it (`publisher_response`)
pub async fn fetch(client: &reqwest::Client, id: &str) -> Result<Value, String> {
    if !is_deck_id(id) {
        return Err(format!("Not a deck id: {}", id));
    }
    let url = format!("{}/api/bga/decks/{}", api_url(), id);
    let res = client.get(&url).send().await.map_err(|e| format!("decks-api unreachable: {}", e))?;
    match res.status().as_u16() {
        200 => {}
        404 => return Err(format!("Unknown deck {}", id)),
        s => {
            let body = res.text().await.unwrap_or_default();
            return Err(format!("decks-api refused deck {} ({}): {}", id, s, body.chars().take(200).collect::<String>()));
        }
    }
    let deck: Value = res.json().await.map_err(|e| format!("decks-api answer: {}", e))?;
    publisher_response(&deck)
}

/// Whether the engine plays the deck (`publisher_response`): a game created with it, against a
/// starter deck, as `Game::create` does for a new game; else why not (not legal, card or Unique
/// power not implemented...). Runs on an engine thread (requests recurse).
pub fn check(deck: &Value) -> Result<(), String> {
    use altered_rules::arena::NewPlayer;
    let players = [
        NewPlayer { name: "A".into(), bot: false, deck: None, api_deck: None, reunion_deck: Some(deck.clone()) },
        NewPlayer { name: "B".into(), bot: false, deck: Some("ax_1_sierra".into()), api_deck: None, reunion_deck: None },
    ];
    altered_rules::Game::create(1, &players, &[]).map(|_| ()).map_err(|e| e.0)
}

/// What the lobby shows of a deck (protocol `ReunionDeckSummary`): its hero as a catalog key, and
/// whether the engine plays it
pub fn summary(id: &str, deck: &Value, checked: Result<(), String>) -> Value {
    let hero = deck["hero"].as_str().and_then(|r| altered_rules::managers::cards::get_card_class(r).ok());
    let mut v = serde_json::json!({
        "id": id,
        "name": deck["deckName"],
        "faction": hero.as_ref().and_then(|h| h.props.get("faction").cloned()),
        "heroRef": hero.as_ref().map(|h| h.class.clone()),
        "size": deck["cardCount"],
        "legal": deck["legality"],
        "playable": checked.is_ok(),
    });
    if let Err(e) = checked {
        v["problem"] = serde_json::json!(e);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A deck of decks-api as read on 2026-11-02 (public deck), with a Unique added
    fn sample() -> Value {
        let mut d: Value = serde_json::from_str(include_str!("../tests/data/reunion_deck.json")).unwrap();
        let unique = json!({
            "quantity": 1,
            "card": {
                "reference": "ALT_CORE_B_AX_08_U_29",
                "name": "Unique test",
                "cardType": {"reference": "CHARACTER"},
                "subTypes": ["ENGINEER"],
                "rarity": {"reference": "UNIQUE"},
                "typeline": "(Character - Engineer)",
                "mainFaction": {"reference": "AX"},
                "illustrator": {"nickName": "Someone"},
                "elements": {"MAIN_COST": 2, "RECALL_COST": 1, "FOREST_POWER": 1, "MOUNTAIN_POWER": 2, "OCEAN_POWER": 1},
                "uniqueReduced": [{"effects": [22, 181, 33]}]
            }
        });
        d["deckCardsByType"]["character"]["deckUserListCard"].as_array_mut().unwrap().push(unique);
        d
    }

    #[test]
    fn deck_ids() {
        assert!(is_deck_id("01a19df6-f90d-7b42-8ecf-4b8da7586cfb"));
        assert!(!is_deck_id("01a19df6-f90d-7b42-8ecf-4b8da7586cf"));
        assert!(!is_deck_id("../admin/x-f90d-7b42-8ecf-4b8da7586cfb"));
    }

    #[test]
    fn converts_a_deck() {
        let r = publisher_response(&sample()).unwrap();
        assert_eq!(r["hero"], "ALT_CYCLONE_B_AX_65_C");
        assert_eq!(r["legality"], true);
        assert_eq!(r["apiId"], "01a19df6-f90d-7b42-8ecf-4b8da7586cfb");
        let cards = r["cards"].as_object().unwrap();
        // the hero is not a card; the Unique carries its content, the others only their quantity
        assert!(!cards.contains_key("ALT_CYCLONE_B_AX_65_C"));
        assert_eq!(cards["ALT_CORE_B_AX_07_C"], json!({"quantity": 3}));
        assert_eq!(cards["ALT_CORE_B_AX_08_U_29"]["content"]["uniqueReduced"], json!([{"effects": [22, 181, 33]}]));
        assert_eq!(cards.values().map(|c| c["quantity"].as_i64().unwrap()).sum::<i64>(), 40);
    }

    #[test]
    fn summary_of_a_deck() {
        let r = publisher_response(&sample()).unwrap();
        let checked = std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn({
                let r = r.clone();
                move || check(&r)
            })
            .unwrap()
            .join()
            .unwrap();
        let s = summary("01a19df6-f90d-7b42-8ecf-4b8da7586cfb", &r, checked);
        assert_eq!(s["playable"], true);
        assert_eq!(s["heroRef"], "AX_Common_IsareePebble");
        assert_eq!(s["faction"], "AX");
        assert_eq!(s["size"], 39);
        let refused = summary("x", &json!({"deckName": "D", "hero": "ALT_NOPE", "legality": false}), Err("This deck is not legal".into()));
        assert_eq!(refused["playable"], false);
        assert_eq!(refused["problem"], "This deck is not legal");
    }

    /// The engine accepts the converted deck: actGetDeckInfos then actConfirmAPIDeck, and the deck
    /// is in the game once both players chose
    #[test]
    fn plays_a_converted_deck() {
        let r = publisher_response(&sample()).unwrap();
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn(move || {
                use altered_rules::arena::NewPlayer;
                let players = [
                    NewPlayer { name: "A".into(), bot: false, deck: None, api_deck: None, reunion_deck: Some(r) },
                    NewPlayer { name: "B".into(), bot: false, deck: Some("ax_1_sierra".into()), api_deck: None, reunion_deck: None },
                ];
                let g = altered_rules::Game::create(7, &players, &[3, 1, 6, 4, 2, 5]).unwrap();
                let mine = g.cards.values().filter(|c| c.pid == 2001).count();
                // 40 cards and the hero
                assert_eq!(mine, 41);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
