//! States/SetupTrait.php: game setup and deck selection (starter decks; the deck APIs, random
//! decks and the DEMO format are not ported)

use crate::core::globals;
use crate::framework;
use crate::game::{Game, PId};
use crate::managers::{cards, meeples, players};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Map, Value};

/// `setupNewGame($players, $options)`: globals, player rows, first player, the starter decks of
/// every player, then everybody chooses a deck
// PHP: States/SetupTrait.php:20
pub fn setup_new_game(g: &mut Game, players: &[(PId, String)], options: &Value) -> R<()> {
    globals::setup_new_game(g, options)?;
    players::setup_new_game(g, players)?;
    let first = php::geti(&framework::get_next_player_table(g), 0).clone();
    globals::set(g, "firstPlayer", json!(php::intval(&first)))?;
    players::initialize_decks(g)?;
    // Stats::checkExistence(): the statistics table is not part of the game state here
    g.fw_set(10, 0);
    framework::set_all_players_multiactive(g);
    framework::active_next_player(g);
    Ok(())
}

/// `getDeckHero($deckNumber)`
pub fn get_deck_hero(g: &mut Game, deck: &Value) -> R<Option<i64>> {
    let ids = cards::get_in_location(g, &format!("deck-{}", php::strval(deck)), None)?;
    let heroes = cards::where_prop(g, ids, "type", &json!("hero"))?;
    Ok(heroes.first().copied())
}

/// `getStarterDeckPreview($pId, $deckNumber)`: hero name, faction and list of the chosen deck
// PHP: States/SetupTrait.php:99
pub fn get_starter_deck_preview(g: &mut Game, pid: PId, deck_number: &Value) -> R<Value> {
    let player_decks = php::coalesce(php::get(&globals::get(g, "playerDecks")?, &pid.to_string()), &php::arr()).clone();
    let deck_info = php::get(&player_decks, &php::key_of(deck_number)).clone();
    if deck_info.is_null() {
        return Ok(Value::Null);
    }
    let Some((deck_id, _)) = cards::STARTER_DECKS.iter().find(|(d, _)| json!(d) == *php::get(&deck_info, "deckId")) else {
        return Ok(Value::Null);
    };
    let heroes = cards::get_filtered(g, &json!(pid), &json!(format!("deck-{}", php::strval(deck_number))), &json!("hero"), false)?;
    let Some(hero) = heroes.first().copied() else {
        return Ok(Value::Null);
    };
    let mut content = Map::new();
    content.insert("hero".into(), json!({"card": {"properties": Value::Object(g.card(hero)?.props.to_map())}, "n": 1}));
    let mut i = 0;
    for (class, n) in cards::starter_contents(deck_id) {
        let props = cards::class_props(class)?;
        if props.get("type") == Some(&json!("hero")) || php::truthy(props.get("token").unwrap_or(&Value::Null)) {
            continue;
        }
        content.insert(i.to_string(), json!({"card": {"properties": Value::Object((*props).clone())}, "n": n}));
        i += 1;
    }
    Ok(json!({
        "deckName": card::get(g, hero, "name")?,
        "faction": php::get(&deck_info, "faction"),
        "cards": Value::Object(content),
    }))
}

/// `argsPrecoDeckSelection()`
// PHP: States/SetupTrait.php:184
pub fn args_preco_deck_selection(g: &mut Game) -> R<Value> {
    let demo = globals::get(g, "deckFormat")? == json!("DEMO");
    let mut args = json!({"_private": [], "demoDeck": demo});
    let all_decks = globals::get(g, "playerDecks")?;
    let selection = globals::get(g, "deckSelection")?;
    for pid in players::get_all(g) {
        let mut decks = php::coalesce(php::get(&all_decks, &pid.to_string()), &php::arr()).clone();
        for k in php::keys_str(&decks) {
            let dn = php::get(php::get(&decks, &k), "deckNumber").clone();
            let hero = get_deck_hero(g, &dn)?;
            let hv = match hero {
                Some(h) => card_ui(g, h)?,
                None => Value::Null,
            };
            php::set(php::get_mut(&mut decks, &k), "hero", hv);
        }
        let sel = php::get(&selection, &pid.to_string()).clone();
        let mut entry = json!({"decks": decks, "selection": sel});
        if php::loose_eq(&sel, &json!("API")) {
            let content = globals::get(g, "deckContent")?;
            php::set(&mut entry, "API", php::get(&content, &pid.to_string()).clone());
        }
        if php::loose_eq(&sel, &json!("random")) {
            let content = globals::get(g, "deckContent")?;
            php::set(&mut entry, "randomDeck", php::get(&content, &pid.to_string()).clone());
        }
        if !sel.is_null() && sel != json!("API") && sel != json!("random") {
            if demo {
                return Err(PhpError::nt("States/SetupTrait.php:51 getDemoDeckPreview"));
            }
            let preview = get_starter_deck_preview(g, pid, &sel)?;
            php::set(&mut entry, "starterDeck", preview);
        }
        php::set(php::get_mut(&mut args, "_private"), &pid.to_string(), entry);
    }
    Ok(args)
}

/// A card object as JSON (`jsonSerialize`: id, location, state, pId, properties)
pub fn card_ui(g: &Game, id: i64) -> R<Value> {
    let c = g.card(id)?;
    Ok(json!({"id": c.id, "location": c.location, "state": c.state, "pId": c.pid, "properties": Value::Object(c.props.to_map())}))
}

/// `actSelectPrecoDeck($choice)`
// PHP: States/SetupTrait.php:224
pub fn act_select_preco_deck(g: &mut Game, choice: &Value) -> R<()> {
    framework::check_possible_action(g, "actSelectPrecoDeck")?;
    let p = players::get_current(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut selection = globals::get(g, "deckSelection")?;
    php::set(&mut selection, &p.to_string(), choice.clone());
    globals::set(g, "deckSelection", selection)?;
    // Notifications::updateInitialPrecoDeckSelection($player, $this->argsPrecoDeckSelection())
    let args = args_preco_deck_selection(g)?;
    g.notify_to(Some(p), "updateInitialPrecoDeckSelection", "", json!({"args": {"_private": php::get(php::get(&args, "_private"), &p.to_string())}}));
    update_active_players_preco_deck_selection(g)
}

/// `actCancelPrecoDeckSelection()`
// PHP: States/SetupTrait.php:236
pub fn act_cancel_preco_deck_selection(g: &mut Game) -> R<()> {
    framework::check_possible_action(g, "actCancelPrecoDeckSelection")?;
    let p = players::get_current(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut selection = globals::get(g, "deckSelection")?;
    php::unset(&mut selection, &p.to_string());
    globals::set(g, "deckSelection", selection)?;
    update_active_players_preco_deck_selection(g)?;
    let args = args_preco_deck_selection(g)?;
    g.notify_to(Some(p), "updateInitialPrecoDeckSelection", "", json!({"args": {"_private": php::get(php::get(&args, "_private"), &p.to_string())}}));
    Ok(())
}

/// `updateActivePlayersPrecoDeckSelection()`
// PHP: States/SetupTrait.php:253
pub fn update_active_players_preco_deck_selection(g: &mut Game) -> R<()> {
    let mut selection = globals::get(g, "deckSelection")?;
    let content = globals::get(g, "deckContent")?;
    for k in php::keys_str(&selection) {
        if php::loose_eq(php::get(&selection, &k), &json!("API")) && !php::isset(&content, &k) {
            php::unset(&mut selection, &k);
        }
    }
    let chosen = php::keys_str(&selection);
    let ids: Vec<PId> = players::get_all(g).into_iter().filter(|p| !chosen.contains(&p.to_string())).collect();
    if !ids.is_empty() {
        framework::set_players_multiactive(g, &ids, "done", true)?;
    } else {
        framework::next_state(g, "done")?;
    }
    Ok(())
}

const FACTION_NAMES: &[(&str, &str)] = &[("BR", "Bravos"), ("MU", "Muna"), ("OD", "Ordis"), ("AX", "Axiom"), ("LY", "Lyra"), ("YZ", "Yzmir")];

/// `stDeckSetup()`: the chosen decks (starter decks only), heroes, storm tokens, shuffles
// PHP: States/SetupTrait.php:482
pub fn st_deck_setup(g: &mut Game) -> R<()> {
    let selection = globals::get(g, "deckSelection")?;
    let mut factions = Map::new();
    for pid in players::get_all(g) {
        let sel = php::get(&selection, &pid.to_string()).clone();
        if php::loose_eq(&sel, &json!("API")) || php::loose_eq(&sel, &json!("random")) {
            return Err(PhpError::nt("States/SetupTrait.php:487 stDeckSetup (API / random decks)"));
        }
        let decks = globals::get(g, "playerDecks")?;
        let faction = php::strval(php::get(php::get(php::get(&decks, &pid.to_string()), &php::key_of(&sel)), "faction"));
        // Demo / starter decks: their list is kept for the end-of-game report
        if globals::get(g, "deckFormat")? == json!("DEMO") {
            return Err(PhpError::nt("States/SetupTrait.php:51 getDemoDeckPreview"));
        }
        let preview = get_starter_deck_preview(g, pid, &sel)?;
        if !preview.is_null() {
            let mut content = globals::get(g, "deckContent")?;
            php::set(&mut content, &pid.to_string(), preview);
            globals::set(g, "deckContent", content)?;
        }
        let faction: String = faction.chars().take(2).collect();
        if let Some(p) = g.players.get_mut(&pid) {
            p.faction = faction.clone();
        }
        // Stats::setFaction: not kept
        factions.insert(pid.to_string(), json!(faction));
    }
    g.notify("vsScreen", json!({"factions": factions}));
    for pid in players::get_all(g) {
        let deck = php::strval(php::get(&selection, &pid.to_string()));
        let mine = cards::get_filtered(g, &json!(pid), &Value::Null, &Value::Null, false)?;
        let to_del: Vec<i64> = mine.into_iter().filter(|c| g.cards[c].location != format!("deck-{}", deck)).collect();
        cards::delete(g, &json!(to_del))?;
        let hero = get_deck_hero(g, &json!(deck))?.ok_or_else(|| PhpError::new("Error: Call to a member function setLocation() on null"))?;
        card::set_location(g, hero, &format!("board-hero-{}", pid))?;
        for c in cards::get_filtered(g, &json!(pid), &Value::Null, &Value::Null, false)? {
            if g.cards[&c].location == format!("deck-{}", deck) {
                card::set_location(g, c, &format!("deck-{}", pid))?;
            }
        }
        let meeples = meeples::setup_player(g, pid)?;
        cards::shuffle(g, &format!("deck-{}", pid))?;
        // Notifications::setupDeck($player, $meeples, $hero)
        let faction = g.player(pid)?.faction.clone();
        let faction_name = FACTION_NAMES.iter().find(|(f, _)| *f == faction).map(|(_, n)| *n).unwrap_or("");
        let deck_count = crate::models::player::get_deck_count(g, pid)?;
        g.notify_to(
            None,
            "setupPlayer",
            "${player_name} will play the faction ${faction_name} with the hero ${card_name}",
            json!({"player_id": pid, "faction_name": faction_name, "faction": faction, "meeples": meeples, "cardId": hero, "deckCount": deck_count}),
        );
    }
    players::set_stats(g)?;
    framework::next_state(g, "")
}
