//! SpecialEffect cases that draw or reveal cards: look at the top cards, reveal-then-keep,
//! revealed cards drawn on a die roll, random card of the opponent's hand, Romantic Encounter.

use super::{gain, has_subtype, player_of, req_player, Case, Out, Se};
use crate::core::{engine, node};
use crate::game::{CardId, Game, PId};
use crate::helpers::{conditions, ft};
use crate::managers::{cards, players};
use crate::models::{card, player};
use crate::php;
use crate::R;
use serde_json::{json, Value};

pub(super) fn case_of(effect: &str) -> Option<Case> {
    let f: Case = match effect {
        "MindApotheosis" => mind_apotheosis,
        "AuraqKibble" => auraq_kibble,
        "boostedRevealBaseStat" => |g, s| boosted_reveal(g, s, keep_base_stat),
        "boostedRevealArtistSong" => |g, s| boosted_reveal(g, s, keep_artist_song),
        "boostedRevealRobotPermanent" => |g, s| boosted_reveal(g, s, keep_robot_permanent),
        "RunesTestamentLook4" => runes_testament_look4,
        "revealTop" => reveal_top,
        "drawReveal" => draw_reveal,
        "drawTopIfRoll" => draw_top_if_roll,
        "exhaustPlayFree" => exhaust_play_free,
        "drawRevealed" => draw_revealed,
        "reveal" => reveal,
        "Phoibos" => |g, s| phoibos(g, s, false),
        "PhoibosUnique" => |g, s| phoibos(g, s, true),
        "RomanticEncounter" => romantic_encounter,
        "RomanticEncounterRare" => romantic_encounter_rare,
        "RomanticCleanLimbo" => romantic_clean_limbo,
        "RunningwiththeWolves" => running_with_the_wolves,
        "discardTopDeck" => discard_top_deck,
        _ => return None,
    };
    Some(f)
}

/// `$player->draw($nb, null, $to, $source)`
fn draw(g: &mut Game, p: PId, nb: i64, to: Option<&str>, source: CardId) -> R<Vec<CardId>> {
    player::draw(g, p, nb, None, to, Some(source), false, None)
}

/// `checkAfterListeners($player, ['draw' => 1, 'location' => HAND], true, 'Draw')`
fn draw_listeners(g: &mut Game, s: &Se, p: PId) -> R<()> {
    s.after_listeners(g, p, json!({"draw": 1, "location": "hand"}), true, Some("Draw"))
}

// PHP: Actions/SpecialEffect.php:711
fn mind_apotheosis(g: &mut Game, s: &mut Se) -> R<Out> {
    engine::checkpoint(g)?;
    // draw 4 cards
    let c = s.card()?;
    let p = player_of(g, c)?;
    let drawn = draw(g, p, 4, None, c)?;
    if !player::has_deck_cards(g, p)? {
        return Ok(Out::Break);
    }
    // Target only Characters drawn
    s.insert(
        g,
        &ft::action(
            "Target",
            json!({
                "n": 2,
                "upTo": true,
                "effect": ft::seq(vec![
                    ft::action("PlayCard", json!({"free": true, "reallyPlayed": false, "effectHand": false}), php::arr()),
                    gain(json!("EFFECT"), "fleeting", json!(1)),
                ]),
                "targetLocation": ["hand"],
                "targetPlayer": "me",
                "cards": drawn,
                "discardRemaining": true,
            }),
            json!({"sourceId": c}),
        ),
    )?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:736
fn auraq_kibble(g: &mut Game, s: &mut Se) -> R<Out> {
    engine::checkpoint(g)?;
    let c = s.card()?;
    let p = player_of(g, c)?;
    if !player::has_deck_cards(g, p)? {
        return Ok(Out::Break);
    }
    let reveal = format!("reveal-{}", p);
    let Some(drawn) = draw(g, p, 1, Some(&reveal), c)?.first().copied() else {
        return Ok(Out::Break);
    };
    s.insert(
        g,
        &ft::xor(vec![
            ft::action("PlayCard", json!({"cardId": drawn, "free": true, "effectHand": false}), json!({"sourceId": c, "optional": true})),
            ft::action("SpecialEffect", json!({"effect": "drawReveal"}), json!({"sourceId": c})),
        ]),
    )?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1288
fn keep_base_stat(g: &mut Game, drawn: CardId) -> R<bool> {
    let mut base_stat = false;
    let biomes = card::get_biomes(g, drawn, false, false)?;
    for v in php::vals(&biomes) {
        if php::loose_eq(v, &json!(0)) {
            base_stat = true;
        }
    }
    if php::loose_ne(&card::get(g, drawn, "type")?, &json!("character")) {
        base_stat = false;
    }
    Ok(base_stat)
}

// PHP: Actions/SpecialEffect.php:1322
fn keep_artist_song(g: &mut Game, drawn: CardId) -> R<bool> {
    Ok(has_subtype(g, drawn, "artist")? || has_subtype(g, drawn, "song")?)
}

// PHP: Actions/SpecialEffect.php:1351
fn keep_robot_permanent(g: &mut Game, drawn: CardId) -> R<bool> {
    Ok(has_subtype(g, drawn, "robot")? || php::loose_eq(&card::get(g, drawn, "type")?, &json!("permanent")) || php::in_array(&json!("permanent"), &card::get_additional_type(g, drawn)?))
}

/// `boostedRevealBaseStat` / `boostedRevealArtistSong` / `boostedRevealRobotPermanent`: if
/// boosted (or `bypass`), reveal the top card, keep it in hand if it matches, else discard it
// PHP: Actions/SpecialEffect.php:1288
fn boosted_reveal(g: &mut Game, s: &mut Se, keep: fn(&mut Game, CardId) -> R<bool>) -> R<Out> {
    let bypass = php::truthy(php::coalesce(php::get(&s.args, "bypass"), &json!(false)));
    let c = s.card()?;
    if card::count_token(g, c, "boost")? > 0 || bypass {
        engine::checkpoint(g)?;
        let p = player_of(g, c)?;
        if !player::has_deck_cards(g, p)? {
            return Ok(Out::Break);
        }
        let Some(drawn) = draw(g, p, 1, Some("limbo"), c)?.first().copied() else {
            return Ok(Out::Break);
        };
        if keep(g, drawn)? {
            s.insert(g, &ft::action("Discard", json!({"destination": "hand", "cardId": drawn}), php::arr()))?;
            draw_listeners(g, s, p)?;
        } else {
            s.insert(g, &ft::action("Discard", json!({"cardId": drawn}), php::arr()))?;
        }
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1578
fn runes_testament_look4(g: &mut Game, s: &mut Se) -> R<Out> {
    engine::checkpoint(g)?;
    // draw 4 cards
    let c = s.card()?;
    let p = player_of(g, c)?;
    if !player::has_deck_cards(g, p)? {
        return Ok(Out::Break);
    }
    let drawn = draw(g, p, 4, None, c)?;
    s.insert(
        g,
        &ft::action(
            "Target",
            json!({
                "n": 1,
                "targetLocation": ["hand"],
                "targetType": ["permanent", "spell", "character"],
                "targetPlayer": "me",
                "cards": drawn,
                "discardRemaining": true,
            }),
            json!({"sourceId": c}),
        ),
    )?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1692
fn reveal_top(g: &mut Game, s: &mut Se) -> R<Out> {
    engine::checkpoint(g)?;
    let c = s.card()?;
    let p = player_of(g, c)?;
    let reveal = format!("reveal-{}", p);
    if cards::count_in_location(g, &reveal)? == 0 && player::has_deck_cards(g, p)? {
        draw(g, p, 1, Some(&reveal), c)?;
    }
    Ok(Out::Break)
}

/// Notifications of revealed cards moving to the hand
fn notify_drawn(g: &mut Game, p: PId, ids: &[CardId]) {
    g.notify("silentKill", json!({"cardIds": ids}));
    g.notify("drawCards", json!({"player_id": p, "cardIds": ids}));
}

// PHP: Actions/SpecialEffect.php:1700
fn draw_reveal(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let drawn = cards::get_in_location(g, &format!("reveal-{}", p), None)?;
    if drawn.is_empty() {
        return Ok(Out::Break);
    }
    for d in &drawn {
        card::set_location(g, *d, "hand")?;
    }
    notify_drawn(g, p, &drawn);
    draw_listeners(g, s, p)?;
    Ok(Out::Break)
}

/// `$this->getCtx()->toArray()['event']['selectedRoll']`
fn selected_roll(g: &Game, s: &Se) -> R<Value> {
    let n = s.node()?;
    Ok(php::get(php::get(&node::to_array(g, n), "event"), "selectedRoll").clone())
}

// PHP: Actions/SpecialEffect.php:1715
fn draw_top_if_roll(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut done = false;
    let mut roll = selected_roll(g, s)?;
    if roll.is_null() {
        roll = s.ctx_arg(g, "roll");
    }
    let drawn = cards::get_in_location(g, &format!("reveal-{}", p), None)?;
    for d in &drawn {
        if php::loose_eq(&roll, &card::get(g, *d, "costHand")?) {
            card::set_location(g, *d, "hand")?;
            done = true;
        }
    }
    if done {
        notify_drawn(g, p, &drawn);
        draw_listeners(g, s, p)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1734
fn exhaust_play_free(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut nodes = vec![];
    let roll = selected_roll(g, s)?;
    let drawn = cards::get_in_location(g, &format!("reveal-{}", p), None)?;
    for d in drawn {
        if php::loose_eq(&roll, &card::get(g, d, "costHand")?) {
            nodes.push(ft::action("Tap", json!({"cardId": c}), json!({"sourceId": c})));
            nodes.push(ft::action("PlayCard", json!({"cardId": d, "free": true, "effectHand": false}), json!({"sourceId": c})));
        }
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1953
fn draw_revealed(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let p = player_of(g, c)?;
    let ty = php::get(&s.args, "type").clone();
    let reserve = php::coalesce(php::get(&s.args, "reserve"), &Value::Null).clone();
    let reserve_if_not_type = php::truthy(php::coalesce(php::get(&s.args, "reserveIfNotType"), &json!(false)));
    let mut done = false;
    let mut resupply = false;
    let drawn = cards::get_in_location(g, &format!("reveal-{}", p), None)?;
    for d in &drawn {
        let dt = card::get(g, *d, "type")?;
        if php::loose_eq(&ty, &dt) || php::in_array(&ty, &card::get_additional_type(g, *d)?) {
            card::set_location(g, *d, "hand")?;
            done = true;
        } else if reserve_if_not_type || php::loose_eq(&reserve, &dt) || php::in_array(&reserve, &card::get_additional_type(g, *d)?) {
            card::set_location(g, *d, "reserve")?;
            resupply = true;
        }
    }
    if done {
        notify_drawn(g, p, &drawn);
        draw_listeners(g, s, p)?;
    }
    if resupply {
        g.notify("silentKill", json!({"cardIds": drawn}));
        g.notify("drawCards", json!({"player_id": p, "cardIds": drawn, "to": "reserve"}));
        let sid = s.source_id(g)?;
        s.after_listeners(g, p, json!({"draw": 1, "sourceId": sid, "notResupply": true}), true, Some("Resupply"))?;
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1817
fn reveal(g: &mut Game, s: &mut Se) -> R<Out> {
    let t = s.get_card(g)?;
    card::set_location(g, t, "limbo")?;
    g.notify("reveal", json!({"cardId": t, "sourceId": s.card}));
    // Interrupt to check that player saw the card
    let active = req_player(players::get_active(g))?;
    let next = req_player(players::get_next(g, active)?)?;
    s.insert(g, &ft::action("InteruptReveal", php::arr(), json!({"pId": next})))?;
    Ok(Out::Break)
}

/// `Phoibos` (play the revealed card only when in contact) / `PhoibosUnique`
/// `Collection::rand()` is PHP's array_rand: not reproducible, drawn here from the game RNG
// PHP: Actions/SpecialEffect.php:2135
fn phoibos(g: &mut Game, s: &mut Se, unique: bool) -> R<Out> {
    // Reveal random card
    engine::checkpoint(g)?;
    let c = s.card()?;
    let p = player_of(g, c)?;
    let opp = req_player(players::get_next(g, p)?)?;
    let hand = player::get_hand(g, opp, &Value::Null)?;
    if !hand.is_empty() {
        let i = g.bga_rand(0, hand.len() as i64 - 1) as usize;
        let r = hand[i];
        card::set_location(g, r, "limbo")?;
        g.notify("revealHand", json!({"cardId": r, "sourceId": c}));
        let play = ft::xor(vec![
            ft::seq(vec![gain(json!(r), "fleeting", json!(1)), ft::action("PlayCard", json!({"cardId": r, "free": true, "stealOwnership": true}), json!({"sourceId": c}))]),
            ft::action("Discard", json!({"cardId": r}), php::arr()),
        ]);
        if unique || php::truthy(&conditions::is_in_contact(g, Some(c), &json!([]), &[])?) {
            // if in contact, may play
            s.insert(g, &play)?;
        } else {
            // else (or not played) discard
            s.insert(g, &ft::action("Discard", json!({"cardId": r}), php::arr()))?;
        }
    }
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2196
fn romantic_encounter(g: &mut Game, s: &mut Se) -> R<Out> {
    engine::checkpoint(g)?;
    let c = s.card()?;
    let p = player_of(g, c)?;
    let opp = if php::loose_eq(php::get(&s.args, "player"), &json!("me")) { p } else { req_player(players::get_next(g, p)?)? };
    if !player::has_deck_cards(g, opp)? {
        return Ok(Out::Break);
    }
    let drawn = draw(g, opp, 4, Some("limbo"), c)?;
    let max_base_cost = if php::truthy(php::coalesce(php::get(&s.args, "rare"), &json!(false))) { 3 } else { 1000 };
    s.insert(
        g,
        &ft::seq(vec![
            ft::action(
                "Target",
                json!({
                    "n": 1,
                    "upTo": true,
                    "effect": ft::seq(vec![
                        ft::action("PlayCard", json!({"free": true, "stealOwnership": true, "effectHand": false}), php::arr()),
                        gain(json!("EFFECT"), "fleeting", json!(1)),
                    ]),
                    "targetLocation": ["limbo"],
                    "targetPlayer": "me",
                    "cards": drawn,
                    "maxBaseCost": max_base_cost,
                }),
                json!({"sourceId": c}),
            ),
            ft::action("SpecialEffect", json!({"effect": "RomanticCleanLimbo", "args": {"cards": drawn}}), json!({"sourceId": c})),
        ]),
    )?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2233
fn romantic_encounter_rare(g: &mut Game, s: &mut Se) -> R<Out> {
    let c = s.card()?;
    let nodes = vec![
        ft::action("SpecialEffect", json!({"effect": "RomanticEncounter", "args": {"player": "me", "rare": true}}), json!({"sourceId": c})),
        ft::action("SpecialEffect", json!({"effect": "RomanticEncounter", "args": {"player": "opponent", "rare": true}}), json!({"sourceId": c})),
    ];
    s.insert_seq(g, nodes)?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2239
fn romantic_clean_limbo(g: &mut Game, s: &mut Se) -> R<Out> {
    let mut discard = vec![];
    let p = req_player(players::get_active(g))?;
    for cid in php::vals(&php::get(&s.args, "cards").clone()) {
        let c = s.card()?;
        if php::loose_ne(cid, &json!(c)) {
            let x = cards::get(g, cid)?;
            if card::location(g, x)? == "limbo" {
                discard.push(cid.clone());
                cards::discard(g, cid, "discard", None)?;
                let sid = s.source_id(g)?;
                s.after_listeners(
                    g,
                    p,
                    json!({
                        "discardCard": true,
                        "cardsToListen": [],
                        "cardId": cid,
                        "token": false,
                        "from": "limbo",
                        "to": "discard",
                        "sacrifice": false,
                        "sourceId": sid,
                        "pId": p,
                    }),
                    true,
                    Some("Discard"),
                )?;
            }
        }
    }
    let c = s.card()?;
    let owner = player_of(g, c)?;
    let ids = cards::get_many(g, &Value::Array(discard), true)?;
    g.notify("publicDiscard", json!({"player_id": owner, "cardIds": ids, "source": "limbo", "destination": "Discard"}));
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:2271
fn running_with_the_wolves(g: &mut Game, s: &mut Se) -> R<Out> {
    let effect_hand = php::coalesce(php::get(&s.args, "effectHand"), &json!(false)).clone();
    let sub_types = php::coalesce(php::get(&s.args, "subTypes"), &json!("disabled")).clone();
    engine::checkpoint(g)?;
    // draw 5 cards
    let c = s.card()?;
    let p = player_of(g, c)?;
    let drawn = draw(g, p, 5, Some("limbo"), c)?;
    // The Target arguments are reset for the PlayCard action (see the PHP comment)
    s.insert(
        g,
        &ft::seq(vec![
            ft::action(
                "Target",
                json!({
                    "n": 5,
                    "upTo": true,
                    "effect": ft::action("PlayCard", json!({
                        "free": true,
                        "effectHand": effect_hand,
                        "reallyPlayed": true,
                        "n": 1,
                        "targetLocation": ["stormLeft", "stormRight", "landmark"],
                        "targetPlayer": "all",
                        "cards": [],
                        "subType": "disabled",
                        "totalCost": 1000,
                    }), php::arr()),
                    "targetLocation": ["limbo"],
                    "targetPlayer": "me",
                    "cards": drawn,
                    "subType": sub_types,
                    "totalCost": 7,
                }),
                json!({"sourceId": c}),
            ),
            ft::action("SpecialEffect", json!({"effect": "RomanticCleanLimbo", "args": {"cards": drawn}}), json!({"sourceId": c})),
        ]),
    )?;
    Ok(Out::Break)
}

// PHP: Actions/SpecialEffect.php:1219
fn discard_top_deck(g: &mut Game, s: &mut Se) -> R<Out> {
    let n = php::intval(php::coalesce(php::get(&s.args, "n"), &json!(1))).max(1);
    let c = s.card()?;
    let p = player_of(g, c)?;
    let mut nodes = vec![];
    for _ in 0..n {
        if !player::has_deck_cards(g, p)? {
            break;
        }
        let Some(drawn) = draw(g, p, 1, Some("limbo"), c)?.first().copied() else {
            continue;
        };
        nodes.push(ft::action("Discard", json!({"cardId": drawn}), json!({"sourceId": c})));
    }
    if !nodes.is_empty() {
        s.insert_seq(g, nodes)?;
    }
    Ok(Out::Break)
}
