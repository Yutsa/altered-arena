//! Helpers/Conditions.php: the condition predicates of card powers.
//!
//! `call` dispatches a condition name (case-insensitive, as PHP method names) to its function.
//! Each function receives the card (`$card`, None when PHP passes null), the event and the extra
//! arguments (strings when they come from a `"name:arg1:arg2"` condition, PHP values when the
//! engine calls a predicate directly), and returns the PHP return value: callers only test
//! `=== false`.

use crate::core::globals;
use crate::game::{CardId, Game};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

/// `Conditions::check($power, $card, $event)`: every condition of `conditions` + `condition`
/// must not return `false`
// PHP: Helpers/Conditions.php:15
pub fn check(g: &mut Game, power: &Value, card: Option<CardId>, event: &Value) -> R<bool> {
    let mut conditions: Vec<Value> = vec![];
    let cs = php::get(power, "conditions");
    if php::is_arr(cs) {
        conditions = php::vals(cs).into_iter().cloned().collect();
    }
    if php::isset(power, "condition") {
        let c = php::get(power, "condition");
        if php::is_arr(c) {
            conditions = php::vals(&php::array_merge(&[&Value::Array(conditions.clone()), c])).into_iter().cloned().collect();
        } else {
            conditions.push(c.clone());
        }
    }
    let mut event = event.clone();
    if php::isset(power, "cardId") {
        php::set(&mut event, "cardId", php::get(power, "cardId").clone());
    }
    if php::isset(power, "wasGigantic") {
        php::set(&mut event, "wasGigantic", php::get(power, "wasGigantic").clone());
    }
    for cond in conditions {
        let t = php::explode(":", &php::strval(&cond));
        let args: Vec<Value> = t[1..].iter().map(|s| json!(s)).collect();
        if call(g, &t[0], card, &event, &args)? == json!(false) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `Conditions::$name($card, $event, ...$args)`
pub fn call(g: &mut Game, name: &str, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    match name.to_lowercase().as_str() {
        "isfirstpassing" => is_first_passing(g, card, event, args),
        "isfromreserve" => is_from_reserve(g, card, event, args),
        "isfromhand" => is_from_hand(g, card, event, args),
        "istoreserve" => is_to_reserve(g, card, event, args),
        "istodiscard" => is_to_discard(g, card, event, args),
        "isstillsamelocation" => is_still_same_location(g, card, event, args),
        "issourcesamelocation" => is_source_same_location(g, card, event, args),
        "hasdeckcards" => has_deck_cards(g, card, event, args),
        "isinstorms" => is_in_storms(g, card, event, args),
        "issource" => is_source(g, card, event, args),
        "issourceoraugment" => is_source_or_augment(g, card, event, args),
        "isfacingsource" => is_facing_source(g, card, event, args),
        "hassameowner" => has_same_owner(g, card, event, args),
        "hassameeventcontroller" => has_same_event_controller(g, card, event, args),
        "isdiscardbyowner" => is_discard_by_owner(g, card, event, args),
        "excludeself" => exclude_self(g, card, event, args),
        "iscardplayedsameowner" => is_card_played_same_owner(g, card, event, args),
        "isme" => is_me(g, card, event, args),
        "isselfplaycardevent" => is_self_play_card_event(g, card, event, args),
        "isnotblockedbyopponentpower" => is_not_blocked_by_opponent_power(g, card, event, args),
        "isnotme" => is_not_me(g, card, event, args),
        "isfirstplayer" => is_first_player(g, card, event, args),
        "isnotfirstplayer" => is_not_first_player(g, card, event, args),
        "ismyturn" => is_my_turn(g, card, event, args),
        "isafternoon" => is_afternoon(g, card, event, args),
        "isnotfirstturn" => is_not_first_turn(g, card, event, args),
        "isnight" => is_night(g, card, event, args),
        "isnotnight" => is_not_night(g, card, event, args),
        "movesstormswithforest" => moves_storms_with_forest(g, card, event, args),
        "movesstormduetoascension" => moves_storm_due_to_ascension(g, card, event, args),
        "movesstormsduetoascension" => moves_storms_due_to_ascension(g, card, event, args),
        "movesanyexpeditionduetoascension" => moves_any_expedition_due_to_ascension(g, card, event, args),
        "movesascendedanyexpeditions" => moves_ascended_any_expeditions(g, card, event, args),
        "zhenzephyr" => zhen_zephyr(g, card, event, args),
        "zhenzephyrmoveexpedition" => zhen_zephyr_move_expedition(g, card, event, args),
        "hasnotmoved" => has_not_moved(g, card, event, args),
        "myexpeditionhasnotmoved" => my_expedition_has_not_moved(g, card, event, args),
        "myotherexpeditionhasmoved" => my_other_expedition_has_moved(g, card, event, args),
        "myexpeditionhasmoved" => my_expedition_has_moved(g, card, event, args),
        "anyofmyexpeditionshasmoved" => any_of_my_expeditions_has_moved(g, card, event, args),
        "myexpeditionisbehind" => my_expedition_is_behind(g, card, event, args),
        "myexpeditionisahead" => my_expedition_is_ahead(g, card, event, args),
        "ismyexpeditionaheadorbehind" => is_my_expedition_ahead_or_behind(g, card, event, args),
        "companionexpeditionisbehind" => companion_expedition_is_behind(g, card, event, args),
        "heroexpeditionisbehind" => hero_expedition_is_behind(g, card, event, args),
        "myexpeditionisnotbehind" => my_expedition_is_not_behind(g, card, event, args),
        "cardplayedexpeditionisbehind" => card_played_expedition_is_behind(g, card, event, args),
        "allexpeditionsarebehindortied" => all_expeditions_are_behind_or_tied(g, card, event, args),
        "allexpeditionsarenotbehindortied" => all_expeditions_are_not_behind_or_tied(g, card, event, args),
        "cardplayedascended" => card_played_ascended(g, card, event, args),
        "canpay" => can_pay(g, card, event, args),
        "hascardsinhand" => has_cards_in_hand(g, card, event, args),
        "hasxcardsinhand" => has_x_cards_in_hand(g, card, event, args),
        "hasxcardsinhandexceptcurrentcard" => has_x_cards_in_hand_except_current_card(g, card, event, args),
        "hasnotokensinlandmarks" => has_no_tokens_in_landmarks(g, card, event, args),
        "has6handcostlandmarks" => has6_hand_cost_landmarks(g, card, event, args),
        "hasdiscardpilecards" => has_discard_pile_cards(g, card, event, args),
        "haseverreachxdiscardedcards" => has_ever_reach_x_discarded_cards(g, card, event, args),
        "hascardindiscardpile" => has_card_in_discard_pile(g, card, event, args),
        "hascardindiscardpilesourcename" => has_card_in_discard_pile_source_name(g, card, event, args),
        "countcardsindiscardpilesourcename" => count_cards_in_discard_pile_source_name(g, card, event, args),
        "getcardsearchname" => get_card_search_name(g, card, event, args),
        "iscardmatchingsearch" => is_card_matching_search(g, card, event, args),
        "hascompletedfeat" => has_completed_feat(g, card, event, args),
        "isthisfeatincomplete" => is_this_feat_incomplete(g, card, event, args),
        "isthisfeatcompleted" => is_this_feat_completed(g, card, event, args),
        "istargetfeatcompleted" => is_target_feat_completed(g, card, event, args),
        "hascontrolfeat" => has_control_feat(g, card, event, args),
        "hasbiggerhand" => has_bigger_hand(g, card, event, args),
        "hassmallerhand" => has_smaller_hand(g, card, event, args),
        "cansabotage" => can_sabotage(g, card, event, args),
        "hasreserve" => has_reserve(g, card, event, args),
        "hasothersupportcardinreserveorexpeditions" => has_other_support_card_in_reserve_or_expeditions(g, card, event, args),
        "hasotheranimalinreserveorexpeditions" => has_other_animal_in_reserve_or_expeditions(g, card, event, args),
        "checkreservecards" => check_reserve_cards(g, card, event, args),
        "checkabilityactivatedthisturn" => check_ability_activated_this_turn(g, card, event, args),
        "checkabilitynotactivatedthisturn" => check_ability_not_activated_this_turn(g, card, event, args),
        "checkabilityactivatedthisturntypecount" => check_ability_activated_this_turn_type_count(g, card, event, args),
        "haslessreservecards" => has_less_reserve_cards(g, card, event, args),
        "hasless8mana" => has_less8_mana(g, card, event, args),
        "hasxmana" => has_x_mana(g, card, event, args),
        "owneronereserve" => owner_one_reserve(g, card, event, args),
        "hascontrol" => has_control(g, card, event, args),
        "hasopponentcontrol" => has_opponent_control(g, card, event, args),
        "hascontrolexhaustedpermanentorreserve" => has_control_exhausted_permanent_or_reserve(g, card, event, args),
        "has4permanentsorreserveexhausted" => has4_permanents_or_reserve_exhausted(g, card, event, args),
        "norobotnopermanent" => no_robotno_permanent(g, card, event, args),
        "noplantnopermanent" => no_plantno_permanent(g, card, event, args),
        "costcheck" => cost_check(g, card, event, args),
        "has3withzerostat" => has3_with_zero_stat(g, card, event, args),
        "hasxwithzerostat" => has_x_with_zero_stat(g, card, event, args),
        "cansacrifice" => can_sacrifice(g, card, event, args),
        "hascontrolfleetinganchoredasleep" => has_control_fleeting_anchored_asleep(g, card, event, args),
        "isincontact" => is_in_contact(g, card, event, args),
        "allincontact" => all_in_contact(g, card, event, args),
        "isnotincontact" => is_not_in_contact(g, card, event, args),
        "hasonecontact" => has_one_contact(g, card, event, args),
        "notfleeting" => not_fleeting(g, card, event, args),
        "isasleep" => is_asleep(g, card, event, args),
        "isanchored" => is_anchored(g, card, event, args),
        "isnotanchored" => is_not_anchored(g, card, event, args),
        "hasfleeting" => has_fleeting(g, card, event, args),
        "leaveandfleeting" => leave_and_fleeting(g, card, event, args),
        "nottapped" => not_tapped(g, card, event, args),
        "notused" => not_used(g, card, event, args),
        "isused" => is_used(g, card, event, args),
        "getsmokethemoutcharges" => get_smoke_them_out_charges(g, card, event, args),
        "issmokethemoutarmed" => is_smoke_them_out_armed(g, card, event, args),
        "armsmokethemout" => arm_smoke_them_out(g, card, event, args),
        "consumesmokethemoutcharge" => consume_smoke_them_out_charge(g, card, event, args),
        "disarmsmokethemout" => disarm_smoke_them_out(g, card, event, args),
        "smokethemoutlytrigger" => smoke_them_out_ly_trigger(g, card, event, args),
        "smokethemoutaxtrigger" => smoke_them_out_ax_trigger(g, card, event, args),
        "hasnoboost" => has_no_boost(g, card, event, args),
        "hasboost" => has_boost(g, card, event, args),
        "hascounteroncard" => has_counter_on_card(g, card, event, args),
        "herohascounter" => hero_has_counter(g, card, event, args),
        "hasgainedboost" => has_gained_boost(g, card, event, args),
        "hasgained" => has_gained(g, card, event, args),
        "isgain" => is_gain(g, card, event, args),
        "ismygaininreserve" => is_my_gain_in_reserve(g, card, event, args),
        "isgaininstorms" => is_gain_in_storms(g, card, event, args),
        "isgaincardtype" => is_gain_card_type(g, card, event, args),
        "hasgainedfleeting" => has_gained_fleeting(g, card, event, args),
        "hasplayergained" => has_player_gained(g, card, event, args),
        "isplayedinsamelocation" => is_played_in_same_location(g, card, event, args),
        "isnotmeinvoke" => is_not_me_invoke(g, card, event, args),
        "ismeinvoke" => is_me_invoke(g, card, event, args),
        "isnotplayedinsamelocation" => is_not_played_in_same_location(g, card, event, args),
        "isaddedcardevent" => is_added_card_event(g, card, event, args),
        "isexhaustedinlocation" => is_exhausted_in_location(g, card, event, args),
        "iscardoftype" => is_card_of_type(g, card, event, args),
        "iscardoftyperobotortoken" => is_card_of_type_robot_or_token(g, card, event, args),
        "iscardaddedrobotortoken" => is_card_added_robot_or_token(g, card, event, args),
        "iscardaddedanyplayer" => is_card_added_any_player(g, card, event, args),
        "iscardadded" => is_card_added(g, card, event, args),
        "iscardplayed" => is_card_played(g, card, event, args),
        "iscardplayednotcharacter" => is_card_played_not_character(g, card, event, args),
        "iscardplayedwithzerostat" => is_card_played_with_zero_stat(g, card, event, args),
        "cardplayedcostcheck" => card_played_cost_check(g, card, event, args),
        "ischaracterfromreservenotblocked" => is_character_from_reserve_not_blocked(g, card, event, args),
        "ischaractercosthigherthancounter" => is_character_cost_higher_than_counter(g, card, event, args),
        "isreallyplayed" => is_really_played(g, card, event, args),
        "specialeffect" => special_effect(g, card, event, args),
        "isplaycard" => is_play_card(g, card, event, args),
        "isaddedcardanyplayer" => is_added_card_any_player(g, card, event, args),
        "isaddedcardopponentevent" => is_added_card_opponent_event(g, card, event, args),
        "hasmarkers" => has_markers(g, card, event, args),
        "isdiscardedevent" => is_discarded_event(g, card, event, args),
        "isdiscarded" => is_discarded(g, card, event, args),
        "ismyselfdiscarded" => is_myself_discarded(g, card, event, args),
        "notdiscarded" => not_discarded(g, card, event, args),
        "notdestroyed" => not_destroyed(g, card, event, args),
        "notindiscard" => not_in_discard(g, card, event, args),
        "issacrifice" => is_sacrifice(g, card, event, args),
        "issacrificecharacterpermanent" => is_sacrifice_character_permanent(g, card, event, args),
        "issacrificed" => is_sacrificed(g, card, event, args),
        "isdiscardedtype" => is_discarded_type(g, card, event, args),
        "isnottoken" => is_not_token(g, card, event, args),
        "countownerboosts" => count_owner_boosts(g, card, event, args),
        "isopponentdraw" => is_opponent_draw(g, card, event, args),
        "realresupply" => real_resupply(g, card, event, args),
        "amiresupplied" => am_i_resupplied(g, card, event, args),
        "ispermanentfromtarget" => is_permanent_from_target(g, card, event, args),
        "isspellfromtarget" => is_spell_from_target(g, card, event, args),
        "ischaracterfromtarget" => is_character_from_target(g, card, event, args),
        "ishandempty" => is_hand_empty(g, card, event, args),
        "isreserveempty" => is_reserve_empty(g, card, event, args),
        "hasxexhaustedreserve" => has_x_exhausted_reserve(g, card, event, args),
        "isinbiome" => is_in_biome(g, card, event, args),
        "isnotinbiome" => is_not_in_biome(g, card, event, args),
        "allexpeditionsnotin" => all_expeditions_not_in(g, card, event, args),
        "isinbiomewithzerostat" => is_in_biome_with_zero_stat(g, card, event, args),
        "istargetasleep" => is_target_asleep(g, card, event, args),
        "istargetsameowner" => is_target_same_owner(g, card, event, args),
        "isplayedcardinbiome" => is_played_card_in_biome(g, card, event, args),
        "isdiscardedcardinbiome" => is_discarded_card_in_biome(g, card, event, args),
        "isdiscardedcardnotinbiome" => is_discarded_card_not_in_biome(g, card, event, args),
        "gatherthepackpass" => gather_the_pack_pass(g, card, event, args),
        "soundthehowlpass" => sound_the_howl_pass(g, card, event, args),
        "xcharacterinexpedition" => x_character_in_expedition(g, card, event, args),
        "isopponentexpeditionempty" => is_opponent_expedition_empty(g, card, event, args),
        "countopponentexpedition" => count_opponent_expedition(g, card, event, args),
        "haspresenceinbothstorms" => has_presence_in_both_storms(g, card, event, args),
        "controlinallexpeditions" => control_in_all_expeditions(g, card, event, args),
        "hasothercharacterinallexpeditions" => has_other_character_in_all_expeditions(g, card, event, args),
        "isopponentexpeditionnotempty" => is_opponent_expedition_not_empty(g, card, event, args),
        "isopponentexpeditionfilled" => is_opponent_expedition_filled(g, card, event, args),
        "isplayedinopponentexpedition" => is_played_in_opponent_expedition(g, card, event, args),
        "isplayedinopponentotherexp" => is_played_in_opponent_other_exp(g, card, event, args),
        "isplayedinownotherexpedition" => is_played_in_own_other_expedition(g, card, event, args),
        "isnotgigantic" => is_not_gigantic(g, card, event, args),
        "isopponentexpeditionin" => is_opponent_expedition_in(g, card, event, args),
        "iscardexpeditionascended" => is_card_expedition_ascended(g, card, event, args),
        "wascardexpeditionascended" => was_card_expedition_ascended(g, card, event, args),
        "isnotcardexpeditionascended" => is_not_card_expedition_ascended(g, card, event, args),
        "countsourceascended" => count_source_ascended(g, card, event, args),
        "hasnosourceplayerascended" => has_no_source_player_ascended(g, card, event, args),
        "hassourceplayerascended" => has_source_player_ascended(g, card, event, args),
        "hassourceplayerallascended" => has_source_player_all_ascended(g, card, event, args),
        "issupporteffect" => is_support_effect(g, card, event, args),
        "hasatleastonecardwithnosupportinreserve" => has_at_least_one_card_with_no_support_in_reserve(g, card, event, args),
        "istargetforestlessorequaltosource" => is_target_forest_less_or_equal_to_source(g, card, event, args),
        "countmonovisibleregions" => count_mono_visible_regions(g, card, event, args),
        "cardinmonoregion" => card_in_mono_region(g, card, event, args),
        "istype" => is_type(g, card, event, args),
        "issubtype" => is_subtype(g, card, event, args),
        "typecheck" => type_check(g, card, event, args),
        "subtypecheck" => sub_type_check(g, card, event, args),
        "hasrolled" => has_rolled(g, card, event, args),
        "selectedroll" => selected_roll(g, card, event, args),
        "boostedbyothercard" => boosted_by_other_card(g, card, event, args),
        "ischaracterboosted" => is_character_boosted(g, card, event, args),
        "gainfirstboost" => gain_first_boost(g, card, event, args),
        "iscontrolledcharactergain" => is_controlled_character_gain(g, card, event, args),
        "ischaracterboostedanduntap" => is_character_boosted_and_untap(g, card, event, args),
        "isrollequalcosthand" => is_roll_equal_cost_hand(g, card, event, args),
        "isnontokenboostedanduntap" => is_non_token_boosted_and_untap(g, card, event, args),
        _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Helpers\\Conditions::{}()", name))),
    }
}

/// `$card` as an object (PHP calls a method on it: null is a fatal error)
pub fn card_req(card: Option<CardId>) -> R<CardId> {
    card.ok_or_else(|| PhpError::new("Error: Call to a member function on null ($card)"))
}

/// An extra argument (null when not given)
pub fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Null)
}


/// `isFirstPassing($card, $event)`
// PHP: Helpers/Conditions.php:51
pub fn is_first_passing(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let fp = globals::get(g, "firstPass")?;
    Ok(json!(php::loose_eq(&fp, &json!(g.card(c)?.pid))))
}

/// `isFromReserve($card, $event)` — not translated yet
pub fn is_from_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:65 isFromReserve"))
}

/// `isFromHand($card, $event)` — not translated yet
pub fn is_from_hand(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:70 isFromHand"))
}

/// `isToReserve($card, $event)` — not translated yet
pub fn is_to_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:75 isToReserve"))
}

/// `isToDiscard($card, $event)` — not translated yet
pub fn is_to_discard(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:80 isToDiscard"))
}

/// `isStillSameLocation($card, $event)` — not translated yet
pub fn is_still_same_location(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:85 isStillSameLocation"))
}

/// `isSourceSameLocation($card, $event)` — not translated yet
pub fn is_source_same_location(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:93 isSourceSameLocation"))
}

/// `hasDeckCards($card, $event)` — not translated yet
pub fn has_deck_cards(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:98 hasDeckCards"))
}

/// `isInStorms($card, $event)` — not translated yet
pub fn is_in_storms(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:103 isInStorms"))
}

/// `isSource($card, $event)` — not translated yet
pub fn is_source(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:108 isSource"))
}

/// `isSourceOrAugment($card, $event)` — not translated yet
pub fn is_source_or_augment(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:113 isSourceOrAugment"))
}

/// `isFacingSource($card, $event)` — not translated yet
pub fn is_facing_source(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:118 isFacingSource"))
}

/// `hasSameOwner($card, $event)` — not translated yet
pub fn has_same_owner(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:124 hasSameOwner"))
}

/// `hasSameEventController($card, $event)` — not translated yet
pub fn has_same_event_controller(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:156 hasSameEventController"))
}

/// `isDiscardByOwner($card, $event)` — not translated yet
pub fn is_discard_by_owner(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:162 isDiscardByOwner"))
}

/// `excludeSelf($card, $event)` — not translated yet
pub fn exclude_self(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:186 excludeSelf"))
}

/// `isCardPlayedSameOwner($card, $event)` — not translated yet
pub fn is_card_played_same_owner(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:191 isCardPlayedSameOwner"))
}

/// `isMe($card, $event)` — not translated yet
pub fn is_me(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:210 isMe"))
}

/// `isSelfPlayCardEvent($card, $event)` — not translated yet
pub fn is_self_play_card_event(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:216 isSelfPlayCardEvent"))
}

/// `isNotBlockedByOpponentPower($card, $event)` — not translated yet
pub fn is_not_blocked_by_opponent_power(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:222 isNotBlockedByOpponentPower"))
}

/// `isNotMe($card, $event)` — not translated yet
pub fn is_not_me(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:228 isNotMe"))
}

/// `isFirstPlayer($card, $event)` — not translated yet
pub fn is_first_player(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:233 isFirstPlayer"))
}

/// `isNotFirstPlayer($card, $event)` — not translated yet
pub fn is_not_first_player(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:238 isNotFirstPlayer"))
}

/// `isMyTurn($card, $event)` — not translated yet
pub fn is_my_turn(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:252 isMyTurn"))
}

/// `isAfternoon($card, $event)` — not translated yet
pub fn is_afternoon(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:257 isAfternoon"))
}

/// `isNotFirstTurn($card, $event)` — not translated yet
pub fn is_not_first_turn(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:262 isNotFirstTurn"))
}

/// `isNight($card, $event)` — not translated yet
pub fn is_night(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:267 isNight"))
}

/// `isNotNight($card, $event)` — not translated yet
pub fn is_not_night(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:272 isNotNight"))
}

/// `movesStormsWithForest($card, $event)` — not translated yet
pub fn moves_storms_with_forest(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:277 movesStormsWithForest"))
}

/// `movesStormDueToAscension($card, $event)` — not translated yet
pub fn moves_storm_due_to_ascension(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:297 movesStormDueToAscension"))
}

/// `movesStormsDueToAscension($card, $event)` — not translated yet
pub fn moves_storms_due_to_ascension(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:317 movesStormsDueToAscension"))
}

/// `movesAnyExpeditionDueToAscension($card, $event)` — not translated yet
pub fn moves_any_expedition_due_to_ascension(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:333 movesAnyExpeditionDueToAscension"))
}

/// `movesAscendedAnyExpeditions($card, $event)` — not translated yet
pub fn moves_ascended_any_expeditions(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:348 movesAscendedAnyExpeditions"))
}

/// `zhenZephyr($card, $event)` — not translated yet
pub fn zhen_zephyr(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:369 zhenZephyr"))
}

/// `zhenZephyrMoveExpedition($card, $event)` — not translated yet
pub fn zhen_zephyr_move_expedition(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:405 zhenZephyrMoveExpedition"))
}

/// `hasNotMoved($card, $event)` — not translated yet
pub fn has_not_moved(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:428 hasNotMoved"))
}

/// `myExpeditionHasNotMoved($card, $event)` — not translated yet
pub fn my_expedition_has_not_moved(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:437 myExpeditionHasNotMoved"))
}

/// `myOtherExpeditionHasMoved($card, $event)` — not translated yet
pub fn my_other_expedition_has_moved(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:444 myOtherExpeditionHasMoved"))
}

/// `myExpeditionHasMoved($card, $event)` — not translated yet
pub fn my_expedition_has_moved(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:452 myExpeditionHasMoved"))
}

/// `anyOfMyExpeditionsHasMoved($card, $event)` — not translated yet
pub fn any_of_my_expeditions_has_moved(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:459 anyOfMyExpeditionsHasMoved"))
}

/// `myExpeditionIsBehind($card, $event)` — not translated yet
pub fn my_expedition_is_behind(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:475 myExpeditionIsBehind"))
}

/// `myExpeditionIsAhead($card, $event)` — not translated yet
pub fn my_expedition_is_ahead(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:480 myExpeditionIsAhead"))
}

/// `isMyExpeditionAheadOrBehind($card, $event, bool $ahead)` — not translated yet
pub fn is_my_expedition_ahead_or_behind(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:485 isMyExpeditionAheadOrBehind"))
}

/// `companionExpeditionIsBehind($card, $event)` — not translated yet
pub fn companion_expedition_is_behind(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:505 companionExpeditionIsBehind"))
}

/// `heroExpeditionIsBehind($card, $event)` — not translated yet
pub fn hero_expedition_is_behind(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:515 heroExpeditionIsBehind"))
}

/// `myExpeditionIsNotBehind($card, $event)` — not translated yet
pub fn my_expedition_is_not_behind(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:525 myExpeditionIsNotBehind"))
}

/// `cardPlayedExpeditionIsBehind($card, $event)` — not translated yet
pub fn card_played_expedition_is_behind(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:530 cardPlayedExpeditionIsBehind"))
}

/// `allExpeditionsAreBehindOrTied($card, $event)` — not translated yet
pub fn all_expeditions_are_behind_or_tied(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:536 allExpeditionsAreBehindOrTied"))
}

/// `allExpeditionsAreNotBehindOrTied($card, $event)` — not translated yet
pub fn all_expeditions_are_not_behind_or_tied(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:548 allExpeditionsAreNotBehindOrTied"))
}

/// `cardPlayedAscended($card, $event)` — not translated yet
pub fn card_played_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:553 cardPlayedAscended"))
}

/// `canPay($card, $event, $n)` — not translated yet
pub fn can_pay(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:567 canPay"))
}

/// `hasCardsInHand($card, $event)` — not translated yet
pub fn has_cards_in_hand(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:572 hasCardsInHand"))
}

/// `hasXCardsInHand($card, $event, $n, $op = 'GTE')` — not translated yet
pub fn has_x_cards_in_hand(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:580 hasXCardsInHand"))
}

/// `hasXCardsInHandExceptCurrentCard($card, $event, $n, $op = 'GTE')` — not translated yet
pub fn has_x_cards_in_hand_except_current_card(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:599 hasXCardsInHandExceptCurrentCard"))
}

/// `hasNoTokensInLandmarks($card, $event)` — not translated yet
pub fn has_no_tokens_in_landmarks(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:621 hasNoTokensInLandmarks"))
}

/// `has6HandCostLandmarks($card, $n)` — not translated yet
pub fn has6_hand_cost_landmarks(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:629 has6HandCostLandmarks"))
}

/// `hasDiscardPileCards($card, $event, $n, $op = 'GTE')` — not translated yet
pub fn has_discard_pile_cards(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:639 hasDiscardPileCards"))
}

/// `hasEverReachXDiscardedCards($card, $event, $n, $op = 'GTE')` — not translated yet
pub fn has_ever_reach_x_discarded_cards(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:656 hasEverReachXDiscardedCards"))
}

/// `hasCardInDiscardPile($card, $event, $name)` — not translated yet
pub fn has_card_in_discard_pile(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:678 hasCardInDiscardPile"))
}

/// `hasCardInDiscardPileSourceName($card, $event)` — not translated yet
pub fn has_card_in_discard_pile_source_name(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:694 hasCardInDiscardPileSourceName"))
}

/// `countCardsInDiscardPileSourceName($card, $event)` — not translated yet
pub fn count_cards_in_discard_pile_source_name(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:699 countCardsInDiscardPileSourceName"))
}

/// `getCardSearchName($card)` — not translated yet
pub fn get_card_search_name(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:720 getCardSearchName"))
}

/// `isCardMatchingSearch($card, $name)` — not translated yet
pub fn is_card_matching_search(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:738 isCardMatchingSearch"))
}

/// `hasCompletedFeat($card, $event, $n = 1, $op = 'GTE')` — not translated yet
pub fn has_completed_feat(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:754 hasCompletedFeat"))
}

/// `isThisFeatIncomplete($card, $event)` — not translated yet
pub fn is_this_feat_incomplete(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:760 isThisFeatIncomplete"))
}

/// `isThisFeatCompleted($card, $event)` — not translated yet
pub fn is_this_feat_completed(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:766 isThisFeatCompleted"))
}

/// `isTargetFeatCompleted($card, $event)` — not translated yet
pub fn is_target_feat_completed(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:775 isTargetFeatCompleted"))
}

/// `hasControlFeat($card, $event, $n = 1, $excludeMyself = 'false', $state = 'all', $op = 'GTE')` — not translated yet
pub fn has_control_feat(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:795 hasControlFeat"))
}

/// `hasBiggerHand($card, $event)` — not translated yet
pub fn has_bigger_hand(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:857 hasBiggerHand"))
}

/// `hasSmallerHand($card, $event)` — not translated yet
pub fn has_smaller_hand(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:865 hasSmallerHand"))
}

/// `canSabotage($card, $event)` — not translated yet
pub fn can_sabotage(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:873 canSabotage"))
}

/// `hasReserve($card, $event, $type = null, $costHand = null, $costReserve = null,  $op = 'GTE', $state = 'all', $n = 0)` — not translated yet
pub fn has_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:878 hasReserve"))
}

/// `hasOtherSupportCardInReserveOrExpeditions($card, $event)` — not translated yet
pub fn has_other_support_card_in_reserve_or_expeditions(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:928 hasOtherSupportCardInReserveOrExpeditions"))
}

/// `hasOtherAnimalInReserveOrExpeditions($card, $event)` — not translated yet
pub fn has_other_animal_in_reserve_or_expeditions(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:946 hasOtherAnimalInReserveOrExpeditions"))
}

/// `checkReserveCards($card, $event, $n, $op = 'GTE')` — not translated yet
pub fn check_reserve_cards(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:964 checkReserveCards"))
}

/// `checkAbilityActivatedThisTurn($card, $event, $type = 'any')` — not translated yet
pub fn check_ability_activated_this_turn(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:978 checkAbilityActivatedThisTurn"))
}

/// `checkAbilityNotActivatedThisTurn($card, $event, $type = 'any')` — not translated yet
pub fn check_ability_not_activated_this_turn(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:988 checkAbilityNotActivatedThisTurn"))
}

/// `checkAbilityActivatedThisTurnTypeCount($card, $event, $type, $n = 1, $op = 'GTE')` — not translated yet
pub fn check_ability_activated_this_turn_type_count(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:993 checkAbilityActivatedThisTurnTypeCount"))
}

/// `hasLessReserveCards($card, $event)` — not translated yet
pub fn has_less_reserve_cards(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1009 hasLessReserveCards"))
}

/// `hasLess8Mana($card, $event)` — not translated yet
pub fn has_less8_mana(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1020 hasLess8Mana"))
}

/// `hasXMana($card, $event, $n, $op = 'GTE')` — not translated yet
pub fn has_x_mana(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1025 hasXMana"))
}

/// `ownerOneReserve($card, $event)` — not translated yet
pub fn owner_one_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1039 ownerOneReserve"))
}

/// `hasControl($card, $event, $type, $n, $excludeMyself = 'false', $state = 'all', $op = 'GTE', $opponent = false)` — not translated yet
pub fn has_control(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1045 hasControl"))
}

/// `hasOpponentControl($card, $event, $type, $n, $excludeMyself = 'false', $state = 'all', $op = 'GTE')` — not translated yet
pub fn has_opponent_control(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1110 hasOpponentControl"))
}

/// `hasControlExhaustedPermanentOrReserve($card, $event)` — not translated yet
pub fn has_control_exhausted_permanent_or_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1115 hasControlExhaustedPermanentOrReserve"))
}

/// `has4PermanentsOrReserveExhausted($card, $event)` — not translated yet
pub fn has4_permanents_or_reserve_exhausted(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1120 has4PermanentsOrReserveExhausted"))
}

/// `noRobotnoPermanent($card, $event)` — not translated yet
pub fn no_robotno_permanent(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1133 noRobotnoPermanent"))
}

/// `noPlantnoPermanent($card, $event)` — not translated yet
pub fn no_plantno_permanent(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1149 noPlantnoPermanent"))
}

/// `costCheck($card, $event, $cost, $op = 'GTE')` — not translated yet
pub fn cost_check(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1165 costCheck"))
}

/// `has3WithZeroStat($card, $event)` — not translated yet
pub fn has3_with_zero_stat(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1178 has3WithZeroStat"))
}

/// `hasXWithZeroStat($card, $event, $location = 'all', $n = 1, $op = 'GTE')` — not translated yet
pub fn has_x_with_zero_stat(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1197 hasXWithZeroStat"))
}

/// `canSacrifice($card, $event)` — not translated yet
pub fn can_sacrifice(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1225 canSacrifice"))
}

/// `hasControlFleetingAnchoredAsleep($card, $event)` — not translated yet
pub fn has_control_fleeting_anchored_asleep(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1230 hasControlFleetingAnchoredAsleep"))
}

/// `isInContact($card, $event)` — not translated yet
pub fn is_in_contact(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1255 isInContact"))
}

/// `allInContact($card, $event)` — not translated yet
pub fn all_in_contact(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1284 allInContact"))
}

/// `isNotInContact($card, $event)` — not translated yet
pub fn is_not_in_contact(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1289 isNotInContact"))
}

/// `hasOneContact($card, $event)` — not translated yet
pub fn has_one_contact(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1294 hasOneContact"))
}

/// `notFleeting($card, $event)` — not translated yet
pub fn not_fleeting(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1312 notFleeting"))
}

/// `isAsleep($card, $event)` — not translated yet
pub fn is_asleep(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1317 isAsleep"))
}

/// `isAnchored($card, $event)` — not translated yet
pub fn is_anchored(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1322 isAnchored"))
}

/// `isNotAnchored($card, $event)` — not translated yet
pub fn is_not_anchored(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1327 isNotAnchored"))
}

/// `hasFleeting($card, $event)` — not translated yet
pub fn has_fleeting(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1332 hasFleeting"))
}

/// `leaveAndFleeting($card, $event)` — not translated yet
pub fn leave_and_fleeting(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1337 leaveAndFleeting"))
}

/// `notTapped($card, $event)` — not translated yet
pub fn not_tapped(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1342 notTapped"))
}

/// `notUsed($card, $event)` — not translated yet
pub fn not_used(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1347 notUsed"))
}

/// `isUsed($card, $event)` — not translated yet
pub fn is_used(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1352 isUsed"))
}

/// `getSmokeThemOutCharges($pId, $cardId = null)` — not translated yet
pub fn get_smoke_them_out_charges(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1362 getSmokeThemOutCharges"))
}

/// `isSmokeThemOutArmed($card, $event)` — not translated yet
pub fn is_smoke_them_out_armed(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1374 isSmokeThemOutArmed"))
}

/// `armSmokeThemOut($pId, $cardId)` — not translated yet
pub fn arm_smoke_them_out(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1379 armSmokeThemOut"))
}

/// `consumeSmokeThemOutCharge($pId, $cardId = null)` — not translated yet
pub fn consume_smoke_them_out_charge(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1388 consumeSmokeThemOutCharge"))
}

/// `disarmSmokeThemOut($pId)`: called directly with the player id as first argument
// PHP: Helpers/Conditions.php:1423
pub fn disarm_smoke_them_out(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::new("disarmSmokeThemOut takes a player id: use disarm_smoke_them_out_pid"))
}

/// `disarmSmokeThemOut($pId)`
// PHP: Helpers/Conditions.php:1423
pub fn disarm_smoke_them_out_pid(g: &mut Game, pid: i64) -> R<()> {
    let mut armed = globals::get(g, "smokeThemOutArmed")?;
    php::unset(&mut armed, &pid.to_string());
    globals::set(g, "smokeThemOutArmed", armed)?;
    Ok(())
}

/// `smokeThemOutLyTrigger($card, $event)` — not translated yet
pub fn smoke_them_out_ly_trigger(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1436 smokeThemOutLyTrigger"))
}

/// `smokeThemOutAxTrigger($card, $event)` — not translated yet
pub fn smoke_them_out_ax_trigger(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1464 smokeThemOutAxTrigger"))
}

/// `hasNoBoost($card, $event)` — not translated yet
pub fn has_no_boost(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1486 hasNoBoost"))
}

/// `hasBoost($card, $event, $n = 1, $op = 'GTE')` — not translated yet
pub fn has_boost(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1491 hasBoost"))
}

/// `hasCounterOnCard($card, $event, $n = 1, $op = 'GTE')` — not translated yet
pub fn has_counter_on_card(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1509 hasCounterOnCard"))
}

/// `heroHasCounter($card, $event, $n = 1, $op = 'GTE')` — not translated yet
pub fn hero_has_counter(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1524 heroHasCounter"))
}

/// `hasGainedBoost($card, $event, $n = 1)` — not translated yet
pub fn has_gained_boost(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1529 hasGainedBoost"))
}

/// `hasGained($card, $event, $type, $n = 1)` — not translated yet
pub fn has_gained(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1534 hasGained"))
}

/// `isGain($card, $event, $type, $n = 1)` — not translated yet
pub fn is_gain(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1550 isGain"))
}

/// `isMyGainInReserve($card, $event)` — not translated yet
pub fn is_my_gain_in_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1563 isMyGainInReserve"))
}

/// `isGainInStorms($card, $event)` — not translated yet
pub fn is_gain_in_storms(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1568 isGainInStorms"))
}

/// `isGainCardType($card, $event, $type)` — not translated yet
pub fn is_gain_card_type(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1573 isGainCardType"))
}

/// `hasGainedFleeting($card, $event)` — not translated yet
pub fn has_gained_fleeting(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1578 hasGainedFleeting"))
}

/// `hasPlayerGained($card, $event, $type)` — not translated yet
pub fn has_player_gained(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1602 hasPlayerGained"))
}

/// `isPlayedInSameLocation($card, $event)` — not translated yet
pub fn is_played_in_same_location(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1619 isPlayedInSameLocation"))
}

/// `isNotMeInvoke($card, $event)` — not translated yet
pub fn is_not_me_invoke(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1626 isNotMeInvoke"))
}

/// `isMeInvoke($card, $event)` — not translated yet
pub fn is_me_invoke(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1631 isMeInvoke"))
}

/// `isNotPlayedInSameLocation($card, $event)` — not translated yet
pub fn is_not_played_in_same_location(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1636 isNotPlayedInSameLocation"))
}

/// `isAddedCardEvent($card, $event, $playedOnly = false, $allPlayers = false)` — not translated yet
pub fn is_added_card_event(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1650 isAddedCardEvent"))
}

/// `isExhaustedInLocation($card, $event, $location)` — not translated yet
pub fn is_exhausted_in_location(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1668 isExhaustedInLocation"))
}

/// `isCardOfType($card, $event, $type = null)` — not translated yet
pub fn is_card_of_type(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1673 isCardOfType"))
}

/// `isCardOfTypeRobotOrToken($card, $event)` — not translated yet
pub fn is_card_of_type_robot_or_token(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1687 isCardOfTypeRobotOrToken"))
}

/// `isCardAddedRobotOrToken($card, $event)` — not translated yet
pub fn is_card_added_robot_or_token(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1692 isCardAddedRobotOrToken"))
}

/// `isCardAddedAnyPlayer($card,  $event, $type = null, $cost = null, $op = 'GTE', $excludeMyself = '', $playedOnly = false)` — not translated yet
pub fn is_card_added_any_player(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1697 isCardAddedAnyPlayer"))
}

/// `isCardAdded($card, $event, $type = null, $cost = null, $op = 'GTE', $excludeMyself = '', $playedOnly = false)` — not translated yet
pub fn is_card_added(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1747 isCardAdded"))
}

/// `isCardPlayed($card, $event, $type, $cost = null, $op = 'GTE', $excludeMyself = '')` — not translated yet
pub fn is_card_played(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1788 isCardPlayed"))
}

/// `isCardPlayedNotCharacter($card, $event)` — not translated yet
pub fn is_card_played_not_character(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1793 isCardPlayedNotCharacter"))
}

/// `isCardPlayedWithZeroStat($card, $event)` — not translated yet
pub fn is_card_played_with_zero_stat(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1798 isCardPlayedWithZeroStat"))
}

/// `cardPlayedCostCheck($card, $event, $cost, $check = 'base', $op = 'GTE')` — not translated yet
pub fn card_played_cost_check(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1815 cardPlayedCostCheck"))
}

/// `isCharacterFromReserveNotBlocked($card, $event)` — not translated yet
pub fn is_character_from_reserve_not_blocked(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1841 isCharacterFromReserveNotBlocked"))
}

/// `isCharacterCostHigherThanCounter($card, $event)` — not translated yet
pub fn is_character_cost_higher_than_counter(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1858 isCharacterCostHigherThanCounter"))
}

/// `isReallyPlayed($card, $event)` — not translated yet
pub fn is_really_played(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1868 isReallyPlayed"))
}

/// `specialEffect($card, $event, $effect)` — not translated yet
pub fn special_effect(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1873 specialEffect"))
}

/// `isPlayCard($card, $event)` — not translated yet
pub fn is_play_card(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1878 isPlayCard"))
}

/// `isAddedCardAnyPlayer($card, $event, $type = null)` — not translated yet
pub fn is_added_card_any_player(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1886 isAddedCardAnyPlayer"))
}

/// `isAddedCardOpponentEvent($card, $event, $type = null, $cost = null, $op = 'GTE', $excludeMyself = '', $playedOnly = false)` — not translated yet
pub fn is_added_card_opponent_event(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1900 isAddedCardOpponentEvent"))
}

/// `hasMarkers($card, $event)` — not translated yet
pub fn has_markers(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1952 hasMarkers"))
}

/// `isDiscardedEvent($card, $event, $meOnly = false)` — not translated yet
pub fn is_discarded_event(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1967 isDiscardedEvent"))
}

/// `isDiscarded($card, $event, $from = null, $to = null, $type = null)` — not translated yet
pub fn is_discarded(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:1980 isDiscarded"))
}

/// `isMyselfDiscarded($card, $event, $from = null, $to = null)` — not translated yet
pub fn is_myself_discarded(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2003 isMyselfDiscarded"))
}

/// `notDiscarded($card, $event)` — not translated yet
pub fn not_discarded(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2008 notDiscarded"))
}

/// `notDestroyed($card, $event)` — not translated yet
pub fn not_destroyed(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2013 notDestroyed"))
}

/// `notInDiscard($card, $event)` — not translated yet
pub fn not_in_discard(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2018 notInDiscard"))
}

/// `isSacrifice($card, $event, $type = null, $subType = null, $exclude = false)` — not translated yet
pub fn is_sacrifice(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2023 isSacrifice"))
}

/// `isSacrificeCharacterPermanent($card, $event)` — not translated yet
pub fn is_sacrifice_character_permanent(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2046 isSacrificeCharacterPermanent"))
}

/// `isSacrificed($card, $event)` — not translated yet
pub fn is_sacrificed(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2051 isSacrificed"))
}

/// `isDiscardedType($card, $event, $type = null)` — not translated yet
pub fn is_discarded_type(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2056 isDiscardedType"))
}

/// `isNotToken($card, $event)` — not translated yet
pub fn is_not_token(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2076 isNotToken"))
}

/// `countOwnerBoosts($card, $event, $n = 1, $op = 'GTE')` — not translated yet
pub fn count_owner_boosts(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2081 countOwnerBoosts"))
}

/// `isOpponentDraw($card, $event)` — not translated yet
pub fn is_opponent_draw(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2100 isOpponentDraw"))
}

/// `realResupply($card, $event)` — not translated yet
pub fn real_resupply(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2105 realResupply"))
}

/// `amIResupplied($card, $event)` — not translated yet
pub fn am_i_resupplied(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2110 amIResupplied"))
}

/// `isPermanentFromTarget($card, $event)` — not translated yet
pub fn is_permanent_from_target(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2115 isPermanentFromTarget"))
}

/// `isSpellFromTarget($card, $event)` — not translated yet
pub fn is_spell_from_target(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2121 isSpellFromTarget"))
}

/// `isCharacterFromTarget($card, $event)` — not translated yet
pub fn is_character_from_target(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2127 isCharacterFromTarget"))
}

/// `isHandEmpty($card, $event)` — not translated yet
pub fn is_hand_empty(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2133 isHandEmpty"))
}

/// `isReserveEmpty($card, $event)` — not translated yet
pub fn is_reserve_empty(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2138 isReserveEmpty"))
}

/// `hasXExhaustedReserve($card, $event, $n, $op = 'GTE', $player = null)` — not translated yet
pub fn has_x_exhausted_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2143 hasXExhaustedReserve"))
}

/// `isInBiome($card, $event, $biome, $excludeMove = false)` — not translated yet
pub fn is_in_biome(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2164 isInBiome"))
}

/// `isNotInBiome($card, $event, $biome)` — not translated yet
pub fn is_not_in_biome(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2177 isNotInBiome"))
}

/// `allExpeditionsNotIn($card, $event, $biome, $excludeMove = false)` — not translated yet
pub fn all_expeditions_not_in(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2182 allExpeditionsNotIn"))
}

/// `isInBiomeWithZeroStat($card, $event)` — not translated yet
pub fn is_in_biome_with_zero_stat(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2194 isInBiomeWithZeroStat"))
}

/// `isTargetAsleep($card, $event)` — not translated yet
pub fn is_target_asleep(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2210 isTargetAsleep"))
}

/// `isTargetSameOwner($card, $event)` — not translated yet
pub fn is_target_same_owner(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2215 isTargetSameOwner"))
}

/// `isPlayedCardInBiome($card, $event, $biome)` — not translated yet
pub fn is_played_card_in_biome(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2220 isPlayedCardInBiome"))
}

/// `isDiscardedCardInBiome($card, $event, $biome)` — not translated yet
pub fn is_discarded_card_in_biome(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2225 isDiscardedCardInBiome"))
}

/// `isDiscardedCardNotInBiome($card, $event, $biome)` — not translated yet
pub fn is_discarded_card_not_in_biome(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2249 isDiscardedCardNotInBiome"))
}

/// `gatherThePackPass($card, $event, $filter = 'animal')` — not translated yet
pub fn gather_the_pack_pass(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2260 gatherThePackPass"))
}

/// `soundTheHowlPass($card, $event)` — not translated yet
pub fn sound_the_howl_pass(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2281 soundTheHowlPass"))
}

/// `XCharacterInExpedition($card, $event, $n, $op = 'GTE')` — not translated yet
pub fn x_character_in_expedition(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2295 XCharacterInExpedition"))
}

/// `isOpponentExpeditionEmpty($card, $event)` — not translated yet
pub fn is_opponent_expedition_empty(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2311 isOpponentExpeditionEmpty"))
}

/// `countOpponentExpedition($card, $event, $type = null)` — not translated yet
pub fn count_opponent_expedition(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2328 countOpponentExpedition"))
}

/// `hasPresenceInBothStorms($cards, $excludeCardId = null)` — not translated yet
pub fn has_presence_in_both_storms(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2353 hasPresenceInBothStorms"))
}

/// `controlInAllExpeditions($card, $event, $type = null, $includeSelf = 'false')` — not translated yet
pub fn control_in_all_expeditions(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2383 controlInAllExpeditions"))
}

/// `hasOtherCharacterInAllExpeditions($card, $event, $op = 'GTE')` — not translated yet
pub fn has_other_character_in_all_expeditions(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2397 hasOtherCharacterInAllExpeditions"))
}

/// `isOpponentExpeditionNotEmpty($card, $event)` — not translated yet
pub fn is_opponent_expedition_not_empty(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2407 isOpponentExpeditionNotEmpty"))
}

/// `isOpponentExpeditionFilled($card, $event, $type = '', $n = 1, $op = 'EQ')` — not translated yet
pub fn is_opponent_expedition_filled(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2412 isOpponentExpeditionFilled"))
}

/// `isPlayedInOpponentExpedition($card, $event)` — not translated yet
pub fn is_played_in_opponent_expedition(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2423 isPlayedInOpponentExpedition"))
}

/// `isPlayedInOpponentOtherExp($card, $event)` — not translated yet
pub fn is_played_in_opponent_other_exp(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2429 isPlayedInOpponentOtherExp"))
}

/// `isPlayedInOwnOtherExpedition($card, $event)` — not translated yet
pub fn is_played_in_own_other_expedition(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2436 isPlayedInOwnOtherExpedition"))
}

/// `isNotGigantic($card, $event)` — not translated yet
pub fn is_not_gigantic(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2443 isNotGigantic"))
}

/// `isOpponentExpeditionIn($card, $event, $biome)` — not translated yet
pub fn is_opponent_expedition_in(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2449 isOpponentExpeditionIn"))
}

/// `isCardExpeditionAscended($card, $event)` — not translated yet
pub fn is_card_expedition_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2460 isCardExpeditionAscended"))
}

/// `wasCardExpeditionAscended($card, $event)` — not translated yet
pub fn was_card_expedition_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2469 wasCardExpeditionAscended"))
}

/// `isNotCardExpeditionAscended($card, $event)` — not translated yet
pub fn is_not_card_expedition_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2474 isNotCardExpeditionAscended"))
}

/// `countSourceAscended($card, $event)` — not translated yet
pub fn count_source_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2479 countSourceAscended"))
}

/// `hasNoSourcePlayerAscended($card, $event)` — not translated yet
pub fn has_no_source_player_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2484 hasNoSourcePlayerAscended"))
}

/// `hasSourcePlayerAscended($card, $event)` — not translated yet
pub fn has_source_player_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2489 hasSourcePlayerAscended"))
}

/// `hasSourcePlayerAllAscended($card, $event)` — not translated yet
pub fn has_source_player_all_ascended(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2494 hasSourcePlayerAllAscended"))
}

/// `isSupportEffect($card, $event)` — not translated yet
pub fn is_support_effect(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2499 isSupportEffect"))
}

/// `hasAtLeastOneCardWithNoSupportInReserve($card, $event)` — not translated yet
pub fn has_at_least_one_card_with_no_support_in_reserve(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2504 hasAtLeastOneCardWithNoSupportInReserve"))
}

/// `isTargetForestLessOrEqualToSource($card, $event)` — not translated yet
pub fn is_target_forest_less_or_equal_to_source(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2517 isTargetForestLessOrEqualToSource"))
}

/// `countMonoVisibleRegions($card, $event, $n = 1, $op = 'GTE')` — not translated yet
pub fn count_mono_visible_regions(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2528 countMonoVisibleRegions"))
}

/// `cardInMonoRegion($card, $event)` — not translated yet
pub fn card_in_mono_region(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2545 cardInMonoRegion"))
}

/// `isType($card, $_event, $type)` — not translated yet
pub fn is_type(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2578 isType"))
}

/// `isSubtype($card, $_event, $subType)` — not translated yet
pub fn is_subtype(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2588 isSubtype"))
}

/// `typeCheck($type, $cardType, $isToken, $additionalType = [])` — not translated yet
pub fn type_check(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2600 typeCheck"))
}

/// `subTypeCheck($subType, $subTypes, $exclude = false)` — not translated yet
pub fn sub_type_check(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2623 subTypeCheck"))
}

/// `hasRolled($card, $event, $result = 1, $op = 'GTE')` — not translated yet
pub fn has_rolled(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2639 hasRolled"))
}

/// `selectedRoll($card, $event, $result, $op = 'GTE')` — not translated yet
pub fn selected_roll(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2652 selectedRoll"))
}

/// `boostedByOtherCard($card, $event)` — not translated yet
pub fn boosted_by_other_card(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2676 boostedByOtherCard"))
}

/// `isCharacterBoosted($card, $event)` — not translated yet
pub fn is_character_boosted(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2684 isCharacterBoosted"))
}

/// `GainFirstBoost($card, $event)` — not translated yet
pub fn gain_first_boost(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2691 GainFirstBoost"))
}

/// `isControlledCharacterGain($card, $event)` — not translated yet
pub fn is_controlled_character_gain(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2699 isControlledCharacterGain"))
}

/// `isCharacterBoostedAndUntap($card, $event)` — not translated yet
pub fn is_character_boosted_and_untap(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2705 isCharacterBoostedAndUntap"))
}

/// `isRollEqualCostHand($card, $event)` — not translated yet
pub fn is_roll_equal_cost_hand(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2717 isRollEqualCostHand"))
}

/// `isNonTokenBoostedAndUntap($card, $event)` — not translated yet
pub fn is_non_token_boosted_and_untap(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::nt("Helpers/Conditions.php:2732 isNonTokenBoostedAndUntap"))
}
