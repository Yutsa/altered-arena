//! States/SetupTrait.php: game setup and deck selection (BGA table creation; the deck lists and
//! deck APIs of cards.inc.php are not ported)

use crate::core::globals;
use crate::framework;
use crate::game::{Game, PId};
use crate::managers::{cards, meeples, players};
use crate::models::card;
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `getDeckHero($deckNumber)`
pub fn get_deck_hero(g: &mut Game, deck: &Value) -> R<Option<i64>> {
    let ids = cards::get_in_location(g, &format!("deck-{}", php::strval(deck)), None)?;
    let heroes = cards::where_prop(g, ids, "type", &json!("hero"))?;
    Ok(heroes.first().copied())
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
                Some(h) => json!({"id": h, "properties": Value::Object((*g.card(h)?.props).clone())}),
                None => Value::Null,
            };
            php::set(php::get_mut(&mut decks, &k), "hero", hv);
        }
        let sel = php::get(&selection, &pid.to_string()).clone();
        let mut entry = json!({"decks": decks, "selection": sel});
        let content = globals::get(g, "deckContent")?;
        if php::loose_eq(&sel, &json!("API")) {
            php::set(&mut entry, "API", php::get(&content, &pid.to_string()).clone());
        }
        if php::loose_eq(&sel, &json!("random")) {
            php::set(&mut entry, "randomDeck", php::get(&content, &pid.to_string()).clone());
        }
        if !sel.is_null() && sel != json!("API") && sel != json!("random") {
            // getDemoDeckPreview / getStarterDeckPreview read cards.inc.php deck lists
            return Err(PhpError::nt("States/SetupTrait.php:51/99 deck previews"));
        }
        php::set(php::get_mut(&mut args, "_private"), &pid.to_string(), entry);
    }
    Ok(args)
}

/// `actSelectPrecoDeck($choice)`
// PHP: States/SetupTrait.php:224
pub fn act_select_preco_deck(g: &mut Game, choice: &Value) -> R<()> {
    framework::check_possible_action(g, "actSelectPrecoDeck")?;
    let p = players::get_current(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut selection = globals::get(g, "deckSelection")?;
    php::set(&mut selection, &p.to_string(), choice.clone());
    globals::set(g, "deckSelection", selection)?;
    update_active_players_preco_deck_selection(g)
}

/// `actCancelPrecoDeckSelection()`
pub fn act_cancel_preco_deck_selection(g: &mut Game) -> R<()> {
    framework::check_possible_action(g, "actCancelPrecoDeckSelection")?;
    let p = players::get_current(g).ok_or_else(|| PhpError::new("Error: Call to a member function getId() on null"))?;
    let mut selection = globals::get(g, "deckSelection")?;
    php::unset(&mut selection, &p.to_string());
    globals::set(g, "deckSelection", selection)?;
    update_active_players_preco_deck_selection(g)
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

/// `stDeckSetup()`: the chosen decks (preco decks only), heroes, storm tokens, shuffles
// PHP: States/SetupTrait.php:482
pub fn st_deck_setup(g: &mut Game) -> R<()> {
    let selection = globals::get(g, "deckSelection")?;
    for pid in players::get_all(g) {
        let sel = php::get(&selection, &pid.to_string()).clone();
        if php::loose_eq(&sel, &json!("API")) || php::loose_eq(&sel, &json!("random")) {
            return Err(PhpError::nt("States/SetupTrait.php:487 stDeckSetup (API / random decks)"));
        }
        let decks = globals::get(g, "playerDecks")?;
        let faction = php::strval(php::get(php::get(php::get(&decks, &pid.to_string()), &php::key_of(&sel)), "faction"));
        // The deck preview stored for the end report reads cards.inc.php
        let faction: String = faction.chars().take(2).collect();
        if let Some(p) = g.players.get_mut(&pid) {
            p.faction = faction;
        }
    }
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
        meeples::setup_player(g, pid)?;
        cards::shuffle(g, &format!("deck-{}", pid))?;
    }
    framework::next_state(g, "")
}
