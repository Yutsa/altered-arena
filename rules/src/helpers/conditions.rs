//! Helpers/Conditions.php: the condition predicates of card powers.
//!
//! `call` dispatches a condition name (case-insensitive, as PHP method names) to its function.
//! Each function receives the card (`$card`, None when PHP passes null), the event and the extra
//! arguments (strings when they come from a `"name:arg1:arg2"` condition, PHP values when the
//! engine calls a predicate directly), and returns the PHP return value: callers only test
//! `=== false`.

use crate::core::globals;
use crate::game::{CardId, Game, PId};
use crate::helpers::collection;
use crate::managers::{cards, meeples, players};
use crate::models::{card, player};
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
        "hascontrolfeatwithmaxbasecost" => has_control_feat_with_max_base_cost(g, card, event, args),
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

// ---------------------------------------------------------------------------
// Constants and small helpers
// ---------------------------------------------------------------------------

const STORM_LEFT: &str = "stormLeft";
const STORM_RIGHT: &str = "stormRight";
const STORMS: [&str; 2] = [STORM_LEFT, STORM_RIGHT];
const GAIN: &str = "Gain";

/// constants.inc.php:226 TYPES
const TYPES: &[&str] = &["hero", "character", "token", "permanent", "spell"];

/// constants.inc.php:346 SUBTYPES
const SUBTYPES: &[&str] = &[
    "divinity", "adventurer", "pilot", "engineer", "robot", "elemental", "disruption", "titan", "cat", "trainer",
    "blade master", "support", "squirrel", "demon", "artist", "citizen", "druid", "plant", "spirit", "soldier",
    "bureaucrat", "mage", "noble", "song", "leviathan", "dragon", "deity", "messenger", "conjuration", "companion",
    "boon", "fairy", "apprentice", "maneuver", "scholar", "animal", "landmark", "illusion", "ore", "scientist", "gear",
    "expedition", "construction", "sap", "merchant", "rogue", "feat", "corruption", "feat",
];

fn storms() -> Value {
    php::str_list(&STORMS)
}

fn subtypes() -> Value {
    php::str_list(SUBTYPES)
}

/// An optional argument: the given value (even null), else the PHP default
fn opt(args: &[Value], i: usize, default: Value) -> Value {
    args.get(i).cloned().unwrap_or(default)
}

/// A required argument (PHP throws an ArgumentCountError when it is missing)
fn req(args: &[Value], i: usize, f: &str) -> R<Value> {
    args.get(i).cloned().ok_or_else(|| PhpError::new(format!("ArgumentCountError: Too few arguments to function ALT\\Helpers\\Conditions::{}()", f)))
}

/// `die($msg)`: the request stops without committing
fn die(msg: &str) -> PhpError {
    PhpError::new(format!("die: {}", msg))
}

fn b(x: bool) -> R<Value> {
    Ok(json!(x))
}

/// `$event[$k]` (null when missing or when the event is not an array)
fn ev<'a>(event: &'a Value, k: &str) -> &'a Value {
    php::get(event, k)
}

/// `$a[$k]` with a PHP value as key
fn at<'a>(a: &'a Value, k: &Value) -> &'a Value {
    php::get(a, &php::key_of(k))
}

/// `in_array($needle, $haystack)` that throws on a non array haystack, as PHP 8
fn in_array_t(needle: &Value, haystack: &Value) -> R<bool> {
    if !php::is_arr(haystack) {
        return Err(PhpError::new(format!("TypeError: in_array(): Argument #2 ($haystack) must be of type array, {} given", php::type_name(haystack))));
    }
    Ok(php::in_array(needle, haystack))
}

fn is_storm(loc: &Value) -> bool {
    php::in_array(loc, &storms())
}

fn other_exp(loc: &str) -> &'static str {
    if loc == STORM_LEFT {
        STORM_RIGHT
    } else {
        STORM_LEFT
    }
}

fn pidv(g: &Game, c: CardId) -> R<Value> {
    Ok(json!(g.card(c)?.pid))
}

fn locv(g: &Game, c: CardId) -> R<Value> {
    Ok(json!(g.card(c)?.location))
}

fn type_of(g: &mut Game, c: CardId) -> R<Value> {
    card::get(g, c, "type")
}

fn gigantic(g: &mut Game, c: CardId) -> R<bool> {
    Ok(php::truthy(&card::is_gigantic(g, c, false)?))
}

/// `$card->getPlayer()` used as an object
fn player_of(g: &mut Game, c: CardId) -> R<PId> {
    card::get_player(g, c)?.ok_or_else(|| PhpError::new("Error: Call to a member function on null ($card->getPlayer())"))
}

/// `Players::getNext($player)` used as an object
fn next_of(g: &Game, pid: PId) -> R<PId> {
    players::get_next(g, pid)?.ok_or_else(|| PhpError::new("Error: Call to a member function on null (Players::getNext)"))
}

/// The opponent of `countOpponentExpedition` / `isOpponentExpeditionIn`: the last other player
fn last_opponent(g: &Game, pid: PId) -> R<PId> {
    let mut opponent = None;
    for p in players::get_all(g) {
        if p != pid {
            opponent = Some(p);
        }
    }
    opponent.ok_or_else(|| PhpError::new("Error: Call to a member function on null ($opponent)"))
}

fn hand_count(g: &mut Game, pid: PId) -> R<usize> {
    Ok(player::get_hand(g, pid, &Value::Null)?.len())
}

fn discard_pile(g: &mut Game, pid: PId) -> R<Vec<CardId>> {
    cards::get_filtered(g, &json!(pid), &json!("discard"), &Value::Null, false)
}

fn count_feat_completed(g: &Game, c: CardId) -> R<usize> {
    meeples::count_meeples(g, &format!("card-{}", c), &json!("featCompleted"))
}

/// `in_array($type, $types) || count(array_intersect($types, $c->getAdditionalType())) > 0`
fn is_of_types(g: &mut Game, c: CardId, types: &Value) -> R<bool> {
    let t = type_of(g, c)?;
    if php::in_array(&t, types) {
        return Ok(true);
    }
    let at = card::get_additional_type(g, c)?;
    Ok(php::count(&php::array_intersect(types, &at)) > 0)
}

/// The `$state` filter of hasControl / hasReserve (boosted, fleeting, exhausted)
fn filter_state(g: &mut Game, ids: Vec<CardId>, state: &Value) -> R<Vec<CardId>> {
    if php::loose_eq(state, &json!("all")) {
        return Ok(ids);
    }
    let mut out = vec![];
    if php::loose_eq(state, &json!("boosted")) {
        for c in ids {
            if card::has_token(g, c, "boost")? {
                out.push(c);
            }
        }
    } else if php::loose_eq(state, &json!("fleeting")) {
        for c in ids {
            if card::has_token(g, c, "fleeting")? {
                out.push(c);
            }
        }
    } else if php::loose_eq(state, &json!("exhausted")) {
        for c in ids {
            if card::is_tapped(g, c)? {
                out.push(c);
            }
        }
    } else {
        out = ids;
    }
    Ok(out)
}

/// `Players::hasOpponentBlockingPower($player, $expedition, $isGigantic)`
/// TEMPORARY: port-storm adds players::has_opponent_blocking_power; switch to it once merged
// PHP: Managers/Players.php:156
fn has_opponent_blocking_power(g: &mut Game, pid: PId, expedition: &str, is_gigantic: bool) -> R<bool> {
    for p2 in players::get_all(g) {
        if p2 == pid {
            continue;
        }
        if has_blocking_power(g, p2, expedition, is_gigantic)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Player::hasBlockingPower($expedition, $isGigantic)`
/// TEMPORARY: port-storm adds player::has_blocking_power; switch to it once merged
// PHP: Models/Player.php:822
fn has_blocking_power(g: &mut Game, pid: PId, expedition: &str, is_gigantic: bool) -> R<bool> {
    for c in player::played_and_infinity(g, pid)? {
        if card::location(g, c)? != expedition && !gigantic(g, c)? && !is_gigantic {
            continue;
        }
        if card::is_blocking_power(g, c)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Players::getRegionsInfo()`
/// TEMPORARY: port-storm adds players::get_regions_info; switch to it once merged
// PHP: Managers/Players.php:876
fn get_regions_info(g: &mut Game) -> R<Value> {
    let mut visible = globals::get_visible_regions(g)?;
    let tiebreak = globals::is(g, "tieBreakerMode")?;
    for (vid, _) in php::entries(&visible.clone()) {
        let markers = meeples::get_of_type(g, &format!("storm-{}", vid), &json!(["ocean", "forest", "mountain"]))?;
        if !markers.is_empty() {
            for m in markers {
                let k = g.meeple(m)?.kind.clone();
                php::set(&mut visible, &vid, json!([k]));
            }
        }
    }
    for side in ["hero", "companion"] {
        for p in players::get_all(g) {
            let token = player::get_storm_token(g, p, side)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
            let mut location = g.meeple(token)?.location.split('-').nth(1).unwrap_or("").to_string();
            let expedition = if side == "hero" { STORM_LEFT } else { STORM_RIGHT };
            if globals::is(g, "tieBreakerMode")? {
                location = "4".into();
            }
            if php::isset(&visible, &location) {
                let mut nb = php::arr();
                for bm in php::vals(php::get(&visible, &location)) {
                    php::set(&mut nb, &php::key_of(bm), bm.clone());
                }
                players::biomes_modifier(g, &mut nb, p, expedition, tiebreak, true)?;
                php::set(&mut visible, &location, php::values(&nb));
            }
        }
    }
    Ok(visible)
}

/// `Player::isInBiome($storm, $biome, $excludeMove)`
// PHP: Models/Player.php:555
fn player_is_in_biome(g: &mut Game, pid: PId, storm: &str, biome: &Value, exclude_move: bool) -> R<bool> {
    let biomes = player::get_biomes(g, pid, storm, exclude_move)?;
    in_array_t(biome, &biomes)
}

/// `Utils::cartesian($input)` on lists of ids
// PHP: Helpers/Utils.php:294
fn cartesian(input: &[Vec<i64>]) -> Vec<Vec<i64>> {
    let mut result: Vec<Vec<i64>> = vec![vec![]];
    for values in input {
        let mut uniq: Vec<i64> = vec![];
        for v in values {
            if !uniq.contains(v) {
                uniq.push(*v);
            }
        }
        let mut append = vec![];
        for product in &result {
            for item in &uniq {
                let mut p = product.clone();
                p.push(*item);
                append.push(p);
            }
        }
        result = append;
    }
    result
}

// ---------------------------------------------------------------------------
// Predicates
// ---------------------------------------------------------------------------

/// `isFirstPassing($card, $event)`
// PHP: Helpers/Conditions.php:51
pub fn is_first_passing(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let fp = globals::get(g, "firstPass")?;
    let c = card_req(card)?;
    Ok(json!(php::loose_eq(&fp, &json!(g.card(c)?.pid))))
}

/// `isFromReserve($card, $event)`
// PHP: Helpers/Conditions.php:65
pub fn is_from_reserve(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(ev(event, "from"), &json!("reserve")))
}

/// `isFromHand($card, $event)`
// PHP: Helpers/Conditions.php:70
pub fn is_from_hand(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(ev(event, "from"), &json!("hand")))
}

/// `isToReserve($card, $event)`
// PHP: Helpers/Conditions.php:75
pub fn is_to_reserve(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(ev(event, "to"), &json!("reserve")))
}

/// `isToDiscard($card, $event)`
// PHP: Helpers/Conditions.php:80
pub fn is_to_discard(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(ev(event, "to"), &json!("discard")))
}

/// `isStillSameLocation($card, $event)`
// PHP: Helpers/Conditions.php:85
pub fn is_still_same_location(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::isset(event, "cardId") {
        return b(false);
    }
    let c = cards::get(g, ev(event, "cardId"))?;
    b(php::loose_eq(&locv(g, c)?, php::coalesce(ev(event, "to"), &json!(""))))
}

/// `isSourceSameLocation($card, $event)`
// PHP: Helpers/Conditions.php:93
pub fn is_source_same_location(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(php::loose_eq(&locv(g, c)?, php::coalesce(ev(event, "sourceLocation"), &json!(""))))
}

/// `hasDeckCards($card, $event)`
// PHP: Helpers/Conditions.php:98
pub fn has_deck_cards(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    b(player::get_deck_count(g, p)? > 0)
}

/// `isInStorms($card, $event)`
// PHP: Helpers/Conditions.php:103
pub fn is_in_storms(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(is_storm(&locv(g, c)?))
}

/// `isSource($card, $event)`
// PHP: Helpers/Conditions.php:108
pub fn is_source(_g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(php::loose_eq(&json!(c), php::coalesce(ev(event, "sourceId"), &json!(-1))))
}

/// `isSourceOrAugment($card, $event)`
// PHP: Helpers/Conditions.php:113
pub fn is_source_or_augment(_g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    if php::loose_eq(&json!(c), php::coalesce(ev(event, "sourceId"), &json!(-1))) {
        return b(true);
    }
    b(php::loose_eq(php::coalesce(ev(event, "augment"), &json!(false)), &json!(true)) && php::loose_eq(php::coalesce(ev(event, "cardId"), &json!(-1)), &json!(c)))
}

/// `isFacingSource($card, $event)`
// PHP: Helpers/Conditions.php:118
pub fn is_facing_source(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let target = cards::get(g, ev(event, "cardId"))?;
    let c = card_req(card)?;
    if php::loose_eq(&pidv(g, c)?, &pidv(g, target)?) {
        return b(false);
    }
    b(php::loose_eq(&locv(g, c)?, &locv(g, target)?) || gigantic(g, target)?)
}

/// `hasSameOwner($card, $event)`
// PHP: Helpers/Conditions.php:124
pub fn has_same_owner(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let card_id = ev(event, "cardId");
    if card_id.is_null() {
        return b(false);
    }
    if cards::get_single(g, card_id, false)?.is_none() {
        let c = card_req(card)?;
        return b(php::loose_eq(ev(event, "pId"), &pidv(g, c)?));
    }
    if php::truthy(php::coalesce(ev(event, "stealOwnership"), &json!(false))) {
        let x = cards::get(g, card_id)?;
        let c = card_req(card)?;
        if php::loose_ne(&pidv(g, x)?, &pidv(g, c)?) {
            return b(true);
        }
    }
    let x = cards::get(g, card_id)?;
    let c = card_req(card)?;
    b(php::loose_eq(&pidv(g, x)?, &pidv(g, c)?))
}

/// `hasSameEventController($card, $event)`
// PHP: Helpers/Conditions.php:156
pub fn has_same_event_controller(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let ctrl = php::coalesce(ev(event, "controller"), php::coalesce(ev(event, "pId"), &json!(-1))).clone();
    let c = card_req(card)?;
    b(php::loose_eq(&ctrl, &pidv(g, c)?))
}

/// `isDiscardByOwner($card, $event)`
// PHP: Helpers/Conditions.php:162
pub fn is_discard_by_owner(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let card_id = ev(event, "cardId");
    if card_id.is_null() {
        return b(false);
    }
    let discarded = cards::get_single(g, card_id, false)?;
    let Some(d) = discarded else { return b(false) };
    let c = card_req(card)?;
    if php::loose_ne(&pidv(g, d)?, &pidv(g, c)?) {
        return b(false);
    }
    let ctrl = php::coalesce(ev(event, "controller"), ev(event, "pId"));
    if php::loose_ne(ctrl, &pidv(g, c)?) {
        return b(false);
    }
    if php::truthy(ev(event, "sabotage")) {
        return b(php::loose_eq(ev(event, "from"), &json!("reserve")));
    }
    b(true)
}

/// `excludeSelf($card, $event)`
// PHP: Helpers/Conditions.php:186
pub fn exclude_self(_g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let other = php::coalesce(ev(event, "cardId"), php::coalesce(ev(event, "sourceId"), &json!(-1))).clone();
    b(php::loose_ne(&json!(c), &other))
}

/// `isCardPlayedSameOwner($card, $event)`
// PHP: Helpers/Conditions.php:191
pub fn is_card_played_same_owner(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let card_id = ev(event, "cardId");
    if card_id.is_null() {
        return b(false);
    }
    let c = card_req(card)?;
    let x = cards::get(g, card_id)?;
    b(php::loose_eq(&pidv(g, c)?, &pidv(g, x)?))
}

/// `isMe($card, $event)`
// PHP: Helpers/Conditions.php:210
pub fn is_me(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(php::loose_eq(ev(event, "pId"), &pidv(g, c)?))
}

fn is_me_b(g: &mut Game, card: Option<CardId>, event: &Value) -> R<bool> {
    Ok(php::truthy(&is_me(g, card, event, &[])?))
}

/// `isSelfPlayCardEvent($card, $event)`
// PHP: Helpers/Conditions.php:216
pub fn is_self_play_card_event(_g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::truthy(ev(event, "playCard")) {
        return b(false);
    }
    let c = card_req(card)?;
    b(php::intval(php::coalesce(ev(event, "cardId"), &json!(-1))) == c)
}

/// `isNotBlockedByOpponentPower($card, $event)`
// PHP: Helpers/Conditions.php:222
pub fn is_not_blocked_by_opponent_power(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let location = if ev(event, "to").is_null() { locv(g, c)? } else { ev(event, "to").clone() };
    let p = player_of(g, c)?;
    let gi = gigantic(g, c)?;
    b(!has_opponent_blocking_power(g, p, &php::strval(&location), gi)?)
}

/// `isNotMe($card, $event)`
// PHP: Helpers/Conditions.php:228
pub fn is_not_me(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(php::loose_ne(ev(event, "pId"), &pidv(g, c)?))
}

/// `isFirstPlayer($card, $event)`
// PHP: Helpers/Conditions.php:233
pub fn is_first_player(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !is_me_b(g, card, event)? {
        return b(false);
    }
    let c = card_req(card)?;
    let fp = globals::get(g, "firstPlayer")?;
    b(php::loose_eq(&pidv(g, c)?, &fp))
}

/// `isNotFirstPlayer($card, $event)`
// PHP: Helpers/Conditions.php:238
pub fn is_not_first_player(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !is_me_b(g, card, event)? {
        return b(false);
    }
    let c = card_req(card)?;
    let fp = globals::get(g, "firstPlayer")?;
    b(php::loose_ne(&pidv(g, c)?, &fp))
}

/// `isMyTurn($card, $event)`
// PHP: Helpers/Conditions.php:252
pub fn is_my_turn(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let a = globals::get(g, "activePId")?;
    let c = card_req(card)?;
    b(php::loose_eq(&a, &pidv(g, c)?))
}

/// `isAfternoon($card, $event)`
// PHP: Helpers/Conditions.php:257
pub fn is_afternoon(g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(globals::is(g, "dayPhase")?)
}

/// `isNotFirstTurn($card, $event)`
// PHP: Helpers/Conditions.php:262
pub fn is_not_first_turn(g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(globals::get_int(g, "day")? != 1)
}

/// `isNight($card, $event)`
// PHP: Helpers/Conditions.php:267
pub fn is_night(g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(globals::get_int(g, "phase")? == 4)
}

/// `isNotNight($card, $event)`
// PHP: Helpers/Conditions.php:272
pub fn is_not_night(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&is_night(g, card, event, &[])?))
}

/// The `$stormMoves[$card->getPId()][$storm]` guard of the movesStorm* predicates: the move, or
/// None when `!isset($stormMoves[$pId]) || $pId != $event['pId'] || !isset($stormMoves[$pId][$storm])`
fn own_storm_move(sm: &Value, pid: &Value, event: &Value, storm: &Value) -> Option<Value> {
    let pk = php::key_of(pid);
    if !php::isset(sm, &pk) || php::loose_ne(pid, ev(event, "pId")) || !php::isset(php::get(sm, &pk), &php::key_of(storm)) {
        return None;
    }
    Some(at(php::get(sm, &pk), storm).clone())
}

/// `movesStormsWithForest($card, $event)`
// PHP: Helpers/Conditions.php:277
pub fn moves_storms_with_forest(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let sm = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    let storm = if ev(event, "expedition").is_null() { locv(g, c)? } else { ev(event, "expedition").clone() };
    let Some(mv) = own_storm_move(&sm, &pidv(g, c)?, event, &storm) else { return b(false) };
    if in_array_t(&json!("forest"), ev(&mv, "biomes"))? && php::ge(ev(&mv, "moves"), &json!(1)) {
        return b(true);
    }
    b(false)
}

/// `movesStormDueToAscension($card, $event)`
// PHP: Helpers/Conditions.php:297
pub fn moves_storm_due_to_ascension(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let sm = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    let storm = if ev(event, "expedition").is_null() { locv(g, c)? } else { ev(event, "expedition").clone() };
    let Some(mv) = own_storm_move(&sm, &pidv(g, c)?, event, &storm) else { return b(false) };
    let p = player_of(g, c)?;
    if player::is_ascended(g, p, &php::strval(&storm))? && php::ge(ev(&mv, "moves"), &json!(1)) && php::truthy(ev(&mv, "hasMovedFromAscension")) {
        return b(true);
    }
    b(false)
}

/// `movesStormsDueToAscension($card, $event)`
// PHP: Helpers/Conditions.php:317
pub fn moves_storms_due_to_ascension(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let sm = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    for storm in STORMS {
        let Some(mv) = own_storm_move(&sm, &pidv(g, c)?, event, &json!(storm)) else { return b(false) };
        let p = player_of(g, c)?;
        if !player::is_ascended(g, p, storm)? || !php::truthy(ev(&mv, "hasMovedFromAscension")) {
            return b(false);
        }
    }
    b(true)
}

/// `movesAnyExpeditionDueToAscension($card, $event)`
// PHP: Helpers/Conditions.php:333
pub fn moves_any_expedition_due_to_ascension(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let sm = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    for storm in STORMS {
        let Some(mv) = own_storm_move(&sm, &pidv(g, c)?, event, &json!(storm)) else { continue };
        let p = player_of(g, c)?;
        if player::is_ascended(g, p, storm)? && php::truthy(ev(&mv, "hasMovedFromAscension")) {
            return b(true);
        }
    }
    b(false)
}

/// `movesAscendedAnyExpeditions($card, $event)`
// PHP: Helpers/Conditions.php:348
pub fn moves_ascended_any_expeditions(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let sm = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    for storm in STORMS {
        let Some(mv) = own_storm_move(&sm, &pidv(g, c)?, event, &json!(storm)) else { continue };
        let p = player_of(g, c)?;
        if player::is_ascended(g, p, storm)? && php::ge(ev(&mv, "moves"), &json!(1)) {
            return b(true);
        }
    }
    b(false)
}

/// Is there an opponent Character in `$storm` (or a Gigantic one in an expedition)?
fn opponent_character_facing(g: &mut Game, c: CardId, storm: &Value) -> R<bool> {
    let p = player_of(g, c)?;
    let n = next_of(g, p)?;
    for pc in player::get_played_cards(g, n, &Value::Null)? {
        if php::loose_ne(&type_of(g, pc)?, &json!("character")) {
            continue;
        }
        let l = locv(g, pc)?;
        if php::loose_eq(&l, storm) || (is_storm(&l) && gigantic(g, pc)?) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `zhenZephyr($card, $event)`
// PHP: Helpers/Conditions.php:369
pub fn zhen_zephyr(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let sm = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    for storm in STORMS {
        if opponent_character_facing(g, c, &json!(storm))? {
            continue;
        }
        let mv = php::get(php::get(&sm, &c_pid_key(g, c)?), storm).clone();
        if php::strict_eq(php::coalesce(ev(&mv, "ascended"), &json!(false)), &json!(false)) {
            continue;
        }
        if php::ge(php::coalesce(ev(&mv, "moves"), &json!(0)), &json!(1)) {
            return b(true);
        }
    }
    b(false)
}

fn c_pid_key(g: &Game, c: CardId) -> R<String> {
    Ok(g.card(c)?.pid.to_string())
}

/// `zhenZephyrMoveExpedition($card, $event)`
// PHP: Helpers/Conditions.php:405
pub fn zhen_zephyr_move_expedition(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let storm = ev(event, "expedition").clone();
    if php::lt(ev(event, "moveExpedition"), &json!(1)) || php::loose_eq(ev(event, "ascended"), &json!(false)) {
        return b(false);
    }
    let c = card_req(card)?;
    if opponent_character_facing(g, c, &storm)? {
        return b(false);
    }
    b(true)
}

/// `hasNotMoved($card, $event)`
// PHP: Helpers/Conditions.php:428
pub fn has_not_moved(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    if php::loose_ne(ev(event, "pId"), &pidv(g, c)?) {
        return b(false);
    }
    let sm = globals::get(g, "stormMoves")?;
    let pk = c_pid_key(g, c)?;
    if php::isset(&sm, &pk) {
        let mine = php::get(&sm, &pk);
        let l = php::floatval(php::coalesce(php::get(php::get(mine, STORM_LEFT), "moves"), &json!(0))).abs();
        let r = php::floatval(php::coalesce(php::get(php::get(mine, STORM_RIGHT), "moves"), &json!(0))).abs();
        if l + r != 0.0 {
            return b(false);
        }
    }
    b(!globals::is(g, "tieBreakerMode")?)
}

/// `$expedition == $card->getLocation()` part shared by the myExpedition*Moved predicates:
/// `!isset($event['expedition']) || $event['expedition'] == $exp`
fn event_expedition_is(event: &Value, exp: &Value) -> bool {
    !php::isset(event, "expedition") || php::loose_eq(ev(event, "expedition"), exp)
}

/// `myExpeditionHasNotMoved($card, $event)`
// PHP: Helpers/Conditions.php:437
pub fn my_expedition_has_not_moved(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let all = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    let loc = locv(g, c)?;
    let sm = at(php::get(&all, &c_pid_key(g, c)?), &loc).clone();
    if php::loose_ne(ev(event, "pId"), &pidv(g, c)?) {
        return b(false);
    }
    if !(sm.is_null() || php::loose_eq(php::coalesce(ev(&sm, "moves"), &json!(0)), &json!(0))) {
        return b(false);
    }
    if globals::is(g, "tieBreakerMode")? {
        return b(false);
    }
    b(event_expedition_is(event, &loc))
}

/// `myOtherExpeditionHasMoved($card, $event)`
// PHP: Helpers/Conditions.php:444
pub fn my_other_expedition_has_moved(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let all = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    let other = json!(other_exp(&card::location(g, c)?));
    let sm = at(php::get(&all, &c_pid_key(g, c)?), &other).clone();
    if php::loose_ne(ev(event, "pId"), &pidv(g, c)?) {
        return b(false);
    }
    if !(!sm.is_null() && php::ge(php::coalesce(ev(&sm, "moves"), &json!(0)), &json!(0))) {
        return b(false);
    }
    if globals::is(g, "tieBreakerMode")? {
        return b(false);
    }
    b(event_expedition_is(event, &other))
}

/// `myExpeditionHasMoved($card, $event)`
// PHP: Helpers/Conditions.php:452
pub fn my_expedition_has_moved(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let all = globals::get(g, "stormMoves")?;
    let c = card_req(card)?;
    let loc = locv(g, c)?;
    let sm = at(php::get(&all, &c_pid_key(g, c)?), &loc).clone();
    if php::loose_ne(ev(event, "pId"), &pidv(g, c)?) || sm.is_null() || !php::gt(php::coalesce(ev(&sm, "moves"), &json!(0)), &json!(0)) {
        return b(false);
    }
    if globals::is(g, "tieBreakerMode")? {
        return b(false);
    }
    b(event_expedition_is(event, &loc))
}

/// `anyOfMyExpeditionsHasMoved($card, $event)`
// PHP: Helpers/Conditions.php:459
pub fn any_of_my_expeditions_has_moved(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    if php::loose_ne(ev(event, "pId"), &pidv(g, c)?) || globals::is(g, "tieBreakerMode")? {
        return b(false);
    }
    let all = globals::get(g, "stormMoves")?;
    let sm = php::get(&all, &c_pid_key(g, c)?).clone();
    for storm in STORMS {
        if php::gt(php::coalesce(php::get(php::get(&sm, storm), "moves"), &json!(0)), &json!(0)) {
            return b(true);
        }
    }
    b(false)
}

/// `myExpeditionIsBehind($card, $event)`
// PHP: Helpers/Conditions.php:475
pub fn my_expedition_is_behind(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    is_my_expedition_ahead_or_behind(g, card, event, &[json!(false)])
}

/// `myExpeditionIsAhead($card, $event)`
// PHP: Helpers/Conditions.php:480
pub fn my_expedition_is_ahead(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    is_my_expedition_ahead_or_behind(g, card, event, &[json!(true)])
}

/// `isMyExpeditionAheadOrBehind($card, $event, bool $ahead)` (private; `$ahead` coerced to bool)
// PHP: Helpers/Conditions.php:485
pub fn is_my_expedition_ahead_or_behind(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ahead = php::truthy(&req(args, 0, "isMyExpeditionAheadOrBehind")?);
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let loc = card::location(g, c)?;
    if player::has_override_behind(g, p, &loc)? {
        return b(!ahead);
    }
    let cardloc = json!(loc);
    let location = if php::loose_ne(&json!(c), php::coalesce(ev(event, "cardId"), &json!(-1))) {
        cardloc
    } else {
        let to = php::coalesce(ev(event, "to"), &cardloc).clone();
        if php::in_array(&to, &json!(["limbo", "landmark"])) {
            cardloc
        } else {
            to
        }
    };
    let winners = players::get_winning_player_by_storms(g)?;
    let win = at(&winners, &location).clone();
    if globals::is(g, "tieBreakerMode")? || win.is_null() || php::loose_eq(&win, &json!(-1)) {
        return b(false);
    }
    let is_ahead = php::loose_eq(&win, &pidv(g, c)?);
    b(if ahead { is_ahead } else { !is_ahead })
}

/// `companionExpeditionIsBehind($card, $event)`
// PHP: Helpers/Conditions.php:505
pub fn companion_expedition_is_behind(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    expedition_is_behind(g, card, STORM_RIGHT)
}

/// `heroExpeditionIsBehind($card, $event)`
// PHP: Helpers/Conditions.php:515
pub fn hero_expedition_is_behind(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    expedition_is_behind(g, card, STORM_LEFT)
}

fn expedition_is_behind(g: &mut Game, card: Option<CardId>, exp: &str) -> R<Value> {
    let winners = players::get_winning_player_by_storms(g)?;
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    if player::has_override_behind(g, p, exp)? {
        return b(true);
    }
    let win = php::get(&winners, exp);
    b(!win.is_null() && php::loose_ne(win, &json!(-1)) && php::loose_ne(win, &pidv(g, c)?))
}

/// `myExpeditionIsNotBehind($card, $event)`
// PHP: Helpers/Conditions.php:525
pub fn my_expedition_is_not_behind(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&my_expedition_is_behind(g, card, event, &[])?))
}

/// `cardPlayedExpeditionIsBehind($card, $event)`
// PHP: Helpers/Conditions.php:530
pub fn card_played_expedition_is_behind(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let pc = cards::get(g, ev(event, "cardId"))?;
    my_expedition_is_behind(g, Some(pc), event, &[])
}

/// `allExpeditionsAreBehindOrTied($card, $event)`
// PHP: Helpers/Conditions.php:536
pub fn all_expeditions_are_behind_or_tied(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let winners = players::get_winning_player_by_storms(g)?;
    let c = card_req(card)?;
    let p = card::get_player(g, c)?;
    let p1 = player_of(g, c)?;
    if player::has_override_behind(g, p1, STORM_LEFT)? {
        return b(true);
    }
    let p = p.ok_or_else(|| PhpError::new("Error: Call to a member function hasOverrideBehind() on null"))?;
    let left = if player::has_override_behind(g, p, STORM_LEFT)? { json!(-1) } else { php::get(&winners, STORM_LEFT).clone() };
    let right = if player::has_override_behind(g, p, STORM_RIGHT)? { json!(-1) } else { php::get(&winners, STORM_RIGHT).clone() };
    let pid = pidv(g, c)?;
    b(!left.is_null() && !right.is_null() && php::loose_ne(&left, &pid) && php::loose_ne(&right, &pid))
}

/// `allExpeditionsAreNotBehindOrTied($card, $event)`
// PHP: Helpers/Conditions.php:548
pub fn all_expeditions_are_not_behind_or_tied(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&all_expeditions_are_behind_or_tied(g, card, event, &[])?))
}

/// `cardPlayedAscended($card, $event)`
// PHP: Helpers/Conditions.php:553
pub fn card_played_ascended(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let pc = cards::get(g, ev(event, "cardId"))?;
    is_card_expedition_ascended(g, Some(pc), event, &[])
}

/// `canPay($card, $event, $n)`
// PHP: Helpers/Conditions.php:567
pub fn can_pay(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "canPay")?;
    let p = player_of(g, card_req(card)?)?;
    b(php::ge(&json!(player::get_mana(g, p)?), &n))
}

/// `hasCardsInHand($card, $event)`
// PHP: Helpers/Conditions.php:572
pub fn has_cards_in_hand(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    b(hand_count(g, p)? > 0)
}

/// `$count <op> $n` with GTE / LTE / EQ, `die()` otherwise
fn cmp_gte_lte_eq(count: &Value, n: &Value, op: &Value, what: &str) -> R<Value> {
    if php::loose_eq(op, &json!("GTE")) {
        return b(php::ge(count, n));
    }
    if php::loose_eq(op, &json!("LTE")) {
        return b(php::le(count, n));
    }
    if php::loose_eq(op, &json!("EQ")) {
        return b(php::loose_eq(count, n));
    }
    Err(die(&format!("Unknown op for {}", what)))
}

/// `hasXCardsInHand($card, $event, $n, $op = 'GTE')`
// PHP: Helpers/Conditions.php:580
pub fn has_x_cards_in_hand(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "hasXCardsInHand")?;
    let op = opt(args, 1, json!("GTE"));
    let p = player_of(g, card_req(card)?)?;
    let count = json!(hand_count(g, p)?);
    cmp_gte_lte_eq(&count, &n, &op, "hasXCardsInHand")
}

/// `hasXCardsInHandExceptCurrentCard($card, $event, $n, $op = 'GTE')`
// PHP: Helpers/Conditions.php:599
pub fn has_x_cards_in_hand_except_current_card(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "hasXCardsInHandExceptCurrentCard")?;
    let op = opt(args, 1, json!("GTE"));
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let count = if card::location(g, c)? == "reserve" { hand_count(g, p)? as i64 } else { hand_count(g, p)? as i64 - 1 };
    cmp_gte_lte_eq(&json!(count), &n, &op, "hasXCardsInHandExceptCurrentCard")
}

/// `hasNoTokensInLandmarks($card, $event)`
// PHP: Helpers/Conditions.php:621
pub fn has_no_tokens_in_landmarks(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    let mut n = 0;
    for c in player::get_played_cards(g, p, &Value::Null)? {
        if card::location(g, c)? == "landmark" && card::is_token(g, c)? {
            n += 1;
        }
    }
    b(n == 0)
}

/// `has6HandCostLandmarks($card, $n)` (the second parameter is the event, unused)
// PHP: Helpers/Conditions.php:629
pub fn has6_hand_cost_landmarks(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    let mut total = 0;
    for c in player::get_landmarks(g, p)? {
        total += php::intval(&card::get(g, c, "costHand")?);
    }
    b(total >= 6)
}

/// `hasDiscardPileCards($card, $event, $n, $op = 'GTE')`
// PHP: Helpers/Conditions.php:639
pub fn has_discard_pile_cards(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "hasDiscardPileCards")?;
    let op = opt(args, 1, json!("GTE"));
    let c = card_req(card)?;
    let pid = g.card(c)?.pid;
    let count = json!(discard_pile(g, pid)?.len());
    cmp_gte_lte_eq(&count, &n, &op, "hasDiscardPileCards")
}

/// `hasEverReachXDiscardedCards($card, $event, $n, $op = 'GTE')`: remembers the first time
// PHP: Helpers/Conditions.php:656
pub fn has_ever_reach_x_discarded_cards(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "hasEverReachXDiscardedCards")?;
    let op = opt(args, 1, json!("GTE"));
    let c = card_req(card)?;
    let pid = g.card(c)?.pid;
    let mut tracker = globals::get(g, "samSpookEverDiscarded")?;
    let pk = pid.to_string();
    if php::isset(&tracker, &pk) && php::truthy(php::get(&tracker, &pk)) {
        return b(true);
    }
    let count = json!(discard_pile(g, pid)?.len());
    let met = (php::loose_eq(&op, &json!("GTE")) && php::ge(&count, &n))
        || (php::loose_eq(&op, &json!("LTE")) && php::le(&count, &n))
        || (php::loose_eq(&op, &json!("EQ")) && php::loose_eq(&count, &n));
    if met {
        php::set(&mut tracker, &pk, json!(true));
        globals::set(g, "samSpookEverDiscarded", tracker)?;
    }
    b(met)
}

/// `hasCardInDiscardPile($card, $event, $name)`
// PHP: Helpers/Conditions.php:678
pub fn has_card_in_discard_pile(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let name = req(args, 0, "hasCardInDiscardPile")?;
    if name.is_null() || php::strict_eq(&name, &json!("")) {
        return b(false);
    }
    let c = card_req(card)?;
    let pid = g.card(c)?.pid;
    for d in discard_pile(g, pid)? {
        if card_matching_search(g, d, &name)? {
            return b(true);
        }
    }
    b(false)
}

/// `hasCardInDiscardPileSourceName($card, $event)`
// PHP: Helpers/Conditions.php:694
pub fn has_card_in_discard_pile_source_name(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::gt(&count_cards_in_discard_pile_source_name(g, card, event, &[])?, &json!(0)))
}

/// `countCardsInDiscardPileSourceName($card, $event)`: an int
// PHP: Helpers/Conditions.php:699
pub fn count_cards_in_discard_pile_source_name(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    Ok(json!(count_cards_in_discard_pile_source_name_n(g, c)?))
}

/// `countCardsInDiscardPileSourceName($card, $event)` as a number (SpecialEffect)
// PHP: Helpers/Conditions.php:699
pub fn count_cards_in_discard_pile_source_name_n(g: &mut Game, c: CardId) -> R<i64> {
    let name = card_search_name(g, c)?;
    if name.is_empty() {
        return Ok(0);
    }
    let mut count = 0;
    let pid = g.card(c)?.pid;
    for d in discard_pile(g, pid)? {
        if card_matching_search(g, d, &json!(name))? {
            count += 1;
        }
    }
    Ok(count)
}

/// `getCardSearchName($card)`
// PHP: Helpers/Conditions.php:720
pub fn get_card_search_name(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    Ok(json!(card_search_name(g, c)?))
}

/// `getCardSearchName($card)`: the `XX_123` segment of `ALT_<set>_<x>_XX_123...`
/// (`/^ALT_[^_]+_[^_]+_([A-Z]{2}_\d+)/`)
// PHP: Helpers/Conditions.php:720
pub fn card_search_name(g: &mut Game, c: CardId) -> R<String> {
    let uid = card::get_str(g, c, "uid")?;
    if uid.is_empty() {
        return Ok(String::new());
    }
    Ok(search_name_of_uid(&uid).unwrap_or_default())
}

fn search_name_of_uid(uid: &str) -> Option<String> {
    let rest = uid.strip_prefix("ALT_")?;
    let i = rest.find('_')?;
    if i == 0 {
        return None;
    }
    let rest = &rest[i + 1..];
    let j = rest.find('_')?;
    if j == 0 {
        return None;
    }
    let rest = rest[j + 1..].as_bytes();
    if rest.len() < 4 || !rest[0].is_ascii_uppercase() || !rest[1].is_ascii_uppercase() || rest[2] != b'_' || !rest[3].is_ascii_digit() {
        return None;
    }
    let mut k = 3;
    while k < rest.len() && rest[k].is_ascii_digit() {
        k += 1;
    }
    Some(String::from_utf8_lossy(&rest[..k]).into_owned())
}

/// `isCardMatchingSearch($card, $name)` called as a condition: `$name` is the event
// PHP: Helpers/Conditions.php:738
pub fn is_card_matching_search(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if event.is_null() || php::strict_eq(event, &json!("")) {
        return b(false);
    }
    let c = card_req(card)?;
    b(card_matching_search(g, c, event)?)
}

/// `isCardMatchingSearch($card, $name)`: the uid contains `_<name>` followed by `_` or the end
/// (`/_<name>(_|$)/`)
// PHP: Helpers/Conditions.php:738
pub fn card_matching_search(g: &mut Game, c: CardId, name: &Value) -> R<bool> {
    if name.is_null() || php::strict_eq(name, &json!("")) {
        return Ok(false);
    }
    let uid = card::get_str(g, c, "uid")?;
    if uid.is_empty() {
        return Ok(false);
    }
    Ok(uid_matches(&uid, &php::strval(name)))
}

fn uid_matches(uid: &str, name: &str) -> bool {
    let needle = format!("_{}", name);
    let mut from = 0;
    while let Some(i) = uid[from..].find(&needle) {
        let end = from + i + needle.len();
        let tail = &uid[end..];
        // `$` also matches before a final newline
        if tail.is_empty() || tail.starts_with('_') || tail == "\n" {
            return true;
        }
        from = from + i + 1;
    }
    false
}

/// `hasCompletedFeat($card, $event, $n = 1, $op = 'GTE')`
// PHP: Helpers/Conditions.php:754
pub fn has_completed_feat(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    let op = opt(args, 1, json!("GTE"));
    has_control_feat_with_max_base_cost(g, card, event, &[n, json!(false), json!(99), json!("completed"), op])
}

/// `isThisFeatIncomplete($card, $event)`
// PHP: Helpers/Conditions.php:760
pub fn is_this_feat_incomplete(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(count_feat_completed(g, c)? < 1)
}

/// `isThisFeatCompleted($card, $event)`
// PHP: Helpers/Conditions.php:766
pub fn is_this_feat_completed(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(count_feat_completed(g, c)? >= 1)
}

/// `isTargetFeatCompleted($card, $event)`
// PHP: Helpers/Conditions.php:775
pub fn is_target_feat_completed(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let card_id = ev(event, "cardId");
    if card_id.is_null() {
        return b(false);
    }
    let c = cards::get(g, card_id)?;
    b(count_feat_completed(g, c)? >= 1)
}

/// `hasControlFeat($card, $event, $n = 1, $excludeMyself = 'false', $state = 'all', $op = 'GTE')`
// PHP: Helpers/Conditions.php:795
pub fn has_control_feat(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let n = json!(php::intval(&opt(args, 0, json!(1))));
    let ex = opt(args, 1, json!("false"));
    let state = opt(args, 2, json!("all"));
    let op = opt(args, 3, json!("GTE"));
    has_control(g, card, event, &[json!("feat"), n, ex, state, op, json!(false)])
}

/// `hasControlFeatWithMaxBaseCost($card, $event, $n = 1, $excludeMyself = 'false', $maxBaseCost = 99,
/// $state = 'all', $op = 'GTE')`
// PHP: Helpers/Conditions.php:804
pub fn has_control_feat_with_max_base_cost(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    let ex = opt(args, 1, json!("false"));
    let max_base_cost = opt(args, 2, json!(99));
    let state = opt(args, 3, json!("all"));
    let op = opt(args, 4, json!("GTE"));
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let types = json!(["character", "token", "permanent", "spell"]);
    let mut ids = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if is_of_types(g, x, &types)? {
            ids.push(x);
        }
    }
    let mut feats = vec![];
    for x in ids {
        if php::in_array(&json!("feat"), &card::get_subtypes(g, x)?) {
            feats.push(x);
        }
    }
    let max = php::intval(&max_base_cost);
    let mut ids = vec![];
    for x in feats {
        let ch = card::get(g, x, "costHand")?;
        let cr = card::get(g, x, "costReserve")?;
        let m = if php::lt(&cr, &ch) { cr } else { ch };
        if php::le(&m, &json!(max)) {
            ids.push(x);
        }
    }
    if php::strict_eq(&ex, &json!("true")) {
        ids.retain(|x| *x != c);
    }
    if php::loose_ne(&state, &json!("all")) {
        let mut out = vec![];
        if php::loose_eq(&state, &json!("exhausted")) {
            for x in ids {
                if card::is_tapped(g, x)? {
                    out.push(x);
                }
            }
        } else if php::loose_eq(&state, &json!("completed")) {
            for x in ids {
                if count_feat_completed(g, x)? > 0 {
                    out.push(x);
                }
            }
        } else if php::loose_eq(&state, &json!("notcompleted")) {
            for x in ids {
                if count_feat_completed(g, x)? == 0 {
                    out.push(x);
                }
            }
        } else {
            out = ids;
        }
        ids = out;
    }
    let m = ids.len() as i64;
    let n = php::intval(&n);
    if php::loose_eq(&op, &json!("GTE")) {
        return b(m >= n);
    }
    if php::loose_eq(&op, &json!("LTE")) {
        return b(m <= n);
    }
    if php::loose_eq(&op, &json!("EQ")) {
        return b(m == n);
    }
    if php::loose_eq(&op, &json!("LT")) {
        return b(m < n);
    }
    if php::loose_eq(&op, &json!("GT")) {
        return b(m > n);
    }
    Err(PhpError::new(format!("BgaVisibleSystemException: Unknown op for hasControlFeatWithMaxBaseCost: {}", php::strval(&op))))
}

/// `hasBiggerHand($card, $event)`
// PHP: Helpers/Conditions.php:857
pub fn has_bigger_hand(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    let mine = hand_count(g, p)?;
    let n = next_of(g, p)?;
    b(mine > hand_count(g, n)?)
}

/// `hasSmallerHand($card, $event)`
// PHP: Helpers/Conditions.php:865
pub fn has_smaller_hand(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    let mine = hand_count(g, p)?;
    let n = next_of(g, p)?;
    b(mine < hand_count(g, n)?)
}

/// `canSabotage($card, $event)`
// PHP: Helpers/Conditions.php:873
pub fn can_sabotage(g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(cards::count_in_location(g, "reserve")? > 0)
}

/// The cost filter of hasReserve: `GTE` → `>=`, `LTE` → `<=`, otherwise the closure returns null
fn cost_filter(g: &mut Game, ids: Vec<CardId>, prop: &str, cost: &Value, op: &Value) -> R<Vec<CardId>> {
    let mut out = vec![];
    for c in ids {
        let v = card::get(g, c, prop)?;
        let keep = if php::loose_eq(op, &json!("GTE")) {
            php::ge(&v, cost)
        } else if php::loose_eq(op, &json!("LTE")) {
            php::le(&v, cost)
        } else {
            false
        };
        if keep {
            out.push(c);
        }
    }
    Ok(out)
}

/// `hasReserve($card, $event, $type = null, $costHand = null, $costReserve = null, $op = 'GTE',
/// $state = 'all', $n = 0)`
// PHP: Helpers/Conditions.php:878
pub fn has_reserve(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    let cost_hand = opt(args, 1, Value::Null);
    let cost_reserve = opt(args, 2, Value::Null);
    let op = opt(args, 3, json!("GTE"));
    let state = opt(args, 4, json!("all"));
    let n = opt(args, 5, json!(0));
    let p = player_of(g, card_req(card)?)?;
    let mut ids = player::get_reserve_cards(g, p)?;
    if !ty.is_null() && php::loose_ne(&ty, &json!("")) && php::in_array(&ty, &php::str_list(TYPES)) {
        let mut out = vec![];
        for c in ids {
            if php::loose_eq(&type_of(g, c)?, &ty) || php::in_array(&ty, &card::get_additional_type(g, c)?) {
                out.push(c);
            }
        }
        ids = out;
    }
    if !ty.is_null() && php::loose_ne(&ty, &json!("")) && php::in_array(&ty, &subtypes()) {
        let mut out = vec![];
        for c in ids {
            if php::in_array(&ty, &card::get_subtypes(g, c)?) {
                out.push(c);
            }
        }
        ids = out;
    }
    if !cost_hand.is_null() && php::loose_ne(&cost_hand, &json!("")) {
        ids = cost_filter(g, ids, "costHand", &cost_hand, &op)?;
    }
    if !cost_reserve.is_null() && php::loose_ne(&cost_reserve, &json!("")) {
        ids = cost_filter(g, ids, "costReserve", &cost_reserve, &op)?;
    }
    let ids = filter_state(g, ids, &state)?;
    b(php::gt(&json!(ids.len()), &n))
}

/// Reserve + played cards of the card's player, other than the card, outside landmarks and heroes
fn other_reserve_or_expedition_cards(g: &mut Game, c: CardId) -> R<Vec<CardId>> {
    let p = player_of(g, c)?;
    let r = player::get_reserve_cards(g, p)?;
    let pl = player::get_played_cards(g, p, &Value::Null)?;
    let mut out = vec![];
    for x in collection::merge(&r, &pl) {
        if x == c {
            continue;
        }
        if card::location(g, x)? == "landmark" || php::loose_eq(&type_of(g, x)?, &json!("hero")) {
            continue;
        }
        out.push(x);
    }
    Ok(out)
}

/// `hasOtherSupportCardInReserveOrExpeditions($card, $event)`
// PHP: Helpers/Conditions.php:928
pub fn has_other_support_card_in_reserve_or_expeditions(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    for x in other_reserve_or_expedition_cards(g, c)? {
        if !php::empty(&card::get(g, x, "effectSupport")?) {
            return b(true);
        }
    }
    b(false)
}

/// `hasOtherAnimalInReserveOrExpeditions($card, $event)`
// PHP: Helpers/Conditions.php:946
pub fn has_other_animal_in_reserve_or_expeditions(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    for x in other_reserve_or_expedition_cards(g, c)? {
        if php::in_array(&json!("animal"), &card::get_subtypes(g, x)?) {
            return b(true);
        }
    }
    b(false)
}

/// `checkReserveCards($card, $event, $n, $op = 'GTE')` (null for other ops)
// PHP: Helpers/Conditions.php:964
pub fn check_reserve_cards(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "checkReserveCards")?;
    let op = opt(args, 1, json!("GTE"));
    let p = player_of(g, card_req(card)?)?;
    let count = json!(player::get_reserve_cards(g, p)?.len());
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&count, &n));
    }
    if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&count, &n));
    }
    Ok(Value::Null)
}

/// `checkAbilityActivatedThisTurn($card, $event, $type = 'any')`
// PHP: Helpers/Conditions.php:978
pub fn check_ability_activated_this_turn(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, json!("any"));
    let abilities = globals::get(g, "abilityActivatedThisTurn")?;
    let c = card_req(card)?;
    let mine = php::coalesce(php::get(&abilities, &c_pid_key(g, c)?), &php::arr()).clone();
    if php::loose_eq(&ty, &json!("any")) {
        return b(!php::empty(&mine));
    }
    b(!php::empty(at(&mine, &ty)))
}

/// `checkAbilityNotActivatedThisTurn($card, $event, $type = 'any')`
// PHP: Helpers/Conditions.php:988
pub fn check_ability_not_activated_this_turn(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, json!("any"));
    b(!php::truthy(&check_ability_activated_this_turn(g, card, event, &[ty])?))
}

/// `checkAbilityActivatedThisTurnTypeCount($card, $event, $type, $n = 1, $op = 'GTE')`
// PHP: Helpers/Conditions.php:993
pub fn check_ability_activated_this_turn_type_count(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "checkAbilityActivatedThisTurnTypeCount")?;
    let n = opt(args, 1, json!(1));
    let op = opt(args, 2, json!("GTE"));
    let abilities = globals::get(g, "abilityActivatedThisTurnTypeCount")?;
    let c = card_req(card)?;
    let mine = php::coalesce(php::get(&abilities, &c_pid_key(g, c)?), &php::arr()).clone();
    let count = php::coalesce(at(&mine, &ty), &json!(0)).clone();
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&count, &n));
    } else if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&count, &n));
    } else if php::loose_eq(&op, &json!("LT")) {
        return b(php::lt(&count, &n));
    } else if php::loose_eq(&op, &json!("GT")) {
        return b(php::gt(&count, &n));
    }
    b(false)
}

/// `hasLessReserveCards($card, $event)`
// PHP: Helpers/Conditions.php:1009
pub fn has_less_reserve_cards(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    let count = player::get_reserve_cards(g, p)?.len();
    let n = next_of(g, p)?;
    let opponent = player::get_reserve_cards(g, n)?.len();
    b(count < opponent)
}

/// `hasLess8Mana($card, $event)`
// PHP: Helpers/Conditions.php:1020
pub fn has_less8_mana(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    b(player::get_total_mana(g, p)? < 8)
}

/// `hasXMana($card, $event, $n, $op = 'GTE')` (null for other ops)
// PHP: Helpers/Conditions.php:1025
pub fn has_x_mana(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "hasXMana")?;
    let op = opt(args, 1, json!("GTE"));
    let p = player_of(g, card_req(card)?)?;
    let mana = json!(player::get_total_mana(g, p)?);
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&mana, &n));
    } else if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&mana, &n));
    } else if php::loose_eq(&op, &json!("LT")) {
        return b(php::lt(&mana, &n));
    } else if php::loose_eq(&op, &json!("GT")) {
        return b(php::gt(&mana, &n));
    }
    Ok(Value::Null)
}

/// `ownerOneReserve($card, $event)`
// PHP: Helpers/Conditions.php:1039
pub fn owner_one_reserve(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let x = cards::get(g, ev(event, "cardId"))?;
    let p = player_of(g, x)?;
    b(player::get_reserve_cards(g, p)?.len() == 1)
}

/// `hasControl($card, $event, $type, $n, $excludeMyself = 'false', $state = 'all', $op = 'GTE',
/// $opponent = false)` (null for other ops)
// PHP: Helpers/Conditions.php:1045
pub fn has_control(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "hasControl")?;
    let n = req(args, 1, "hasControl")?;
    let ex = opt(args, 2, json!("false"));
    let state = opt(args, 3, json!("all"));
    let op = opt(args, 4, json!("GTE"));
    let opponent = opt(args, 5, json!(false));

    let all4 = json!(["character", "token", "permanent", "spell"]);
    let mut types = json!(["character", "token"]);
    let mut sub_types: Option<Value> = None;
    if php::loose_eq(&ty, &json!("token")) {
        types = json!(["character", "permanent"]);
    }
    if php::loose_eq(&ty, &json!("permanent")) {
        types = json!(["permanent"]);
    }
    let ty_is_subtype = php::in_array(&ty, &subtypes());
    if ty_is_subtype {
        types = all4.clone();
    }
    let tys = php::strval(&ty);
    if tys.contains('|') {
        let maybe = php::str_list(&tys.split('|').collect::<Vec<_>>());
        let all_subtypes = php::count(&php::array_intersect(&maybe, &subtypes())) == php::count(&maybe);
        if all_subtypes {
            sub_types = Some(maybe);
            types = all4.clone();
        }
    }

    let c = card_req(card)?;
    let player = if php::truthy(&opponent) {
        let p = player_of(g, c)?;
        next_of(g, p)?
    } else {
        player_of(g, c)?
    };
    let mut ids = vec![];
    for x in player::get_played_cards(g, player, &Value::Null)? {
        if is_of_types(g, x, &types)? {
            ids.push(x);
        }
    }
    if php::loose_eq(&ty, &json!("token")) {
        let mut out = vec![];
        for x in ids {
            if card::is_token(g, x)? {
                out.push(x);
            }
        }
        ids = out;
    }
    if ty_is_subtype {
        let mut out = vec![];
        for x in ids {
            if php::in_array(&ty, &card::get_subtypes(g, x)?) {
                out.push(x);
            }
        }
        ids = out;
    }
    if let Some(st) = &sub_types {
        let mut out = vec![];
        for x in ids {
            if php::count(&php::array_intersect(st, &card::get_subtypes(g, x)?)) > 0 {
                out.push(x);
            }
        }
        ids = out;
    }
    if php::strict_eq(&ex, &json!("true")) {
        ids.retain(|x| *x != c);
    }
    let ids = filter_state(g, ids, &state)?;
    let m = json!(ids.len());
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&m, &n));
    }
    if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&m, &n));
    }
    Ok(Value::Null)
}

/// `hasOpponentControl($card, $event, $type, $n, $excludeMyself = 'false', $state = 'all', $op = 'GTE')`
// PHP: Helpers/Conditions.php:1110
pub fn has_opponent_control(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "hasOpponentControl")?;
    let n = req(args, 1, "hasOpponentControl")?;
    let ex = opt(args, 2, json!("false"));
    let state = opt(args, 3, json!("all"));
    let op = opt(args, 4, json!("GTE"));
    has_control(g, card, event, &[ty, n, ex, state, op, json!(true)])
}

/// `hasControlExhaustedPermanentOrReserve($card, $event)`
// PHP: Helpers/Conditions.php:1115
pub fn has_control_exhausted_permanent_or_reserve(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if php::truthy(&has_control(g, card, event, &[json!("permanent"), json!(1), json!(false), json!("exhausted")])?) {
        return b(true);
    }
    b(php::truthy(&has_reserve(g, card, event, &[Value::Null, Value::Null, Value::Null, json!("GTE"), json!("exhausted")])?))
}

/// `has4PermanentsOrReserveExhausted($card, $event)`
// PHP: Helpers/Conditions.php:1120
pub fn has4_permanents_or_reserve_exhausted(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let mut a = vec![];
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if is_of_types(g, x, &json!(["permanent"]))? && card::is_tapped(g, x)? {
            a.push(x);
        }
    }
    let p = player_of(g, c)?;
    let mut r = vec![];
    for x in player::get_reserve_cards(g, p)? {
        if card::is_tapped(g, x)? {
            r.push(x);
        }
    }
    b(collection::merge(&a, &r).len() >= 4)
}

/// noRobotnoPermanent / noPlantnoPermanent: no other permanent nor `$subtype` card in play
fn no_subtype_no_permanent(g: &mut Game, card: Option<CardId>, subtype: &str) -> R<Value> {
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let mut n = 0;
    for x in player::get_played_cards(g, p, &Value::Null)? {
        if x == c {
            continue;
        }
        if php::loose_eq(&type_of(g, x)?, &json!("permanent")) || php::in_array(&json!(subtype), &card::get_subtypes(g, x)?) || php::in_array(&json!("permanent"), &card::get_additional_type(g, x)?) {
            n += 1;
        }
    }
    b(n == 0)
}

/// `noRobotnoPermanent($card, $event)`
// PHP: Helpers/Conditions.php:1133
pub fn no_robotno_permanent(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    no_subtype_no_permanent(g, card, "robot")
}

/// `noPlantnoPermanent($card, $event)`
// PHP: Helpers/Conditions.php:1149
pub fn no_plantno_permanent(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    no_subtype_no_permanent(g, card, "plant")
}

/// `costCheck($card, $event, $cost, $op = 'GTE')` (null for other ops)
// PHP: Helpers/Conditions.php:1165
pub fn cost_check(g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let cost = req(args, 0, "costCheck")?;
    let op = opt(args, 1, json!("GTE"));
    let d = cards::get(g, ev(event, "cardId"))?;
    let cost_hand = card::get(g, d, "costHand")?;
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::le(&cost, &cost_hand));
    }
    if php::loose_eq(&op, &json!("LTE")) {
        return b(php::ge(&cost, &cost_hand));
    }
    Ok(Value::Null)
}

/// Number of 0 biome values among the Character / token cards
fn count_zero_stats(g: &mut Game, ids: &[CardId]) -> R<i64> {
    let mut n = 0;
    for x in ids {
        if !php::in_array(&type_of(g, *x)?, &json!(["token", "character"])) {
            continue;
        }
        for v in php::vals(&card::get_biomes(g, *x, false, false)?) {
            if php::loose_eq(v, &json!(0)) {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// `has3WithZeroStat($card, $event)`
// PHP: Helpers/Conditions.php:1178
pub fn has3_with_zero_stat(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let played = player::get_played_cards(g, p, &Value::Null)?;
    let has_zero = count_zero_stats(g, &played)?;
    let pid = pidv(g, c)?;
    b(php::loose_eq(&pid, php::coalesce(ev(event, "pId"), &pid)) && has_zero >= 3)
}

/// `hasXWithZeroStat($card, $event, $location = 'all', $n = 1, $op = 'GTE')`
// PHP: Helpers/Conditions.php:1197
pub fn has_x_with_zero_stat(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let location = opt(args, 0, json!("all"));
    let n = opt(args, 1, json!(1));
    let op = opt(args, 2, json!("GTE"));
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let mut played = player::get_played_cards(g, p, &Value::Null)?;
    if php::loose_eq(&location, &json!("all")) {
        let p = player_of(g, c)?;
        let r = player::get_reserve_cards(g, p)?;
        played = collection::merge(&played, &r);
    }
    let has_zero = json!(count_zero_stats(g, &played)?);
    if php::ge(&has_zero, &n) && php::loose_eq(&op, &json!("GTE")) {
        return b(true);
    } else if php::le(&has_zero, &n) && php::loose_eq(&op, &json!("LTE")) {
        return b(true);
    }
    b(false)
}

/// `canSacrifice($card, $event)`
// PHP: Helpers/Conditions.php:1225
pub fn can_sacrifice(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    has_control(g, card, event, &[json!("character"), json!(1)])
}

/// `hasControlFleetingAnchoredAsleep($card, $event)`: three different cards, one Asleep, one
/// Anchored, one Fleeting
// PHP: Helpers/Conditions.php:1230
pub fn has_control_fleeting_anchored_asleep(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let mut asleep = vec![];
    let mut anchored = vec![];
    let mut fleeting = vec![];
    let p = player_of(g, card_req(card)?)?;
    for x in player::get_played_cards(g, p, &json!(["token", "character"]))? {
        if card::has_token(g, x, "asleep")? {
            asleep.push(x);
        }
        if card::has_token(g, x, "anchored")? {
            anchored.push(x);
        }
        if card::has_token(g, x, "fleeting")? {
            fleeting.push(x);
        }
    }
    let mut combination = cartesian(&[asleep, anchored, fleeting]);
    combination.retain(|comb| {
        let mut u: Vec<i64> = vec![];
        for x in comb {
            if !u.contains(x) {
                u.push(*x);
            }
        }
        u.len() == 3
    });
    b(!combination.is_empty())
}

/// `isInContact($card, $event)`
// PHP: Helpers/Conditions.php:1255
pub fn is_in_contact(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    Ok(json!(is_in_contact_b(g, c)?))
}

/// `Conditions::isInContact($card, [])` as a bool (SpecialEffect)
// PHP: Helpers/Conditions.php:1255
pub fn is_in_contact_b(g: &mut Game, c: CardId) -> R<bool> {
    let loc = card::location(g, c)?;
    if !STORMS.contains(&loc.as_str()) {
        return Ok(false);
    }
    let p = player_of(g, c)?;
    if player::has_override_in_contact(g, p, &loc)? {
        return Ok(true);
    }
    let opponent = next_of(g, p)?;
    let opponent_tokens = meeples::get_storm_tokens(g, opponent)?;
    let mut token_f = vec![if loc == STORM_LEFT { "hero" } else { "companion" }];
    if gigantic(g, c)? {
        token_f = vec!["hero", "companion"];
    }
    for tok in token_f {
        let m = if tok == "hero" { player::get_hero_token(g, p)? } else { player::get_companion_token(g, p)? };
        let token = g.meeple(m)?.location.clone();
        for o in &opponent_tokens {
            if token == g.meeple(*o)?.location {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `allInContact($card, $event)`
// PHP: Helpers/Conditions.php:1284
pub fn all_in_contact(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    if !player::is_in_contact(g, p, Some(STORM_LEFT))? {
        return b(false);
    }
    let p = player_of(g, c)?;
    b(player::is_in_contact(g, p, Some(STORM_RIGHT))?)
}

/// `isNotInContact($card, $event)`
// PHP: Helpers/Conditions.php:1289
pub fn is_not_in_contact(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&is_in_contact(g, card, event, &[])?))
}

/// `hasOneContact($card, $event)`
// PHP: Helpers/Conditions.php:1294
pub fn has_one_contact(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let location = ev(event, "location");
    let c = card_req(card)?;
    if php::loose_eq(location, &json!(STORM_LEFT)) || php::loose_eq(location, &json!(STORM_RIGHT)) {
        let p = player_of(g, c)?;
        return b(player::is_in_contact(g, p, Some(&php::strval(location)))?);
    }
    let p = player_of(g, c)?;
    if player::is_in_contact(g, p, Some(STORM_LEFT))? {
        return b(true);
    }
    let p = player_of(g, c)?;
    b(player::is_in_contact(g, p, Some(STORM_RIGHT))?)
}

/// `notFleeting($card, $event)`
// PHP: Helpers/Conditions.php:1312
pub fn not_fleeting(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(!card::has_token(g, c, "fleeting")? && !php::truthy(ev(event, "fleeting")))
}

/// `isAsleep($card, $event)`
// PHP: Helpers/Conditions.php:1317
pub fn is_asleep(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(card::has_token(g, card_req(card)?, "asleep")?)
}

/// `isAnchored($card, $event)`
// PHP: Helpers/Conditions.php:1322
pub fn is_anchored(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(card::has_token(g, card_req(card)?, "anchored")?)
}

/// `isNotAnchored($card, $event)`
// PHP: Helpers/Conditions.php:1327
pub fn is_not_anchored(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&is_anchored(g, card, event, &[])?))
}

/// `hasFleeting($card, $event)`
// PHP: Helpers/Conditions.php:1332
pub fn has_fleeting(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(card::has_token(g, card_req(card)?, "fleeting")? || php::truthy(ev(event, "fleeting")))
}

/// `leaveAndFleeting($card, $event)`: the raw `$event['fleeting'] ?? false`
// PHP: Helpers/Conditions.php:1337
pub fn leave_and_fleeting(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    Ok(php::coalesce(ev(event, "fleeting"), &json!(false)).clone())
}

/// `notTapped($card, $event)`
// PHP: Helpers/Conditions.php:1342
pub fn not_tapped(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(!card::is_tapped(g, card_req(card)?)?)
}

/// `notUsed($card, $event)`
// PHP: Helpers/Conditions.php:1347
pub fn not_used(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let ed = card::get(g, card_req(card)?, "extraDatas")?;
    b(php::loose_eq(php::coalesce(php::get(&ed, "userPower"), &json!(false)), &json!(false)))
}

/// `isUsed($card, $event)`
// PHP: Helpers/Conditions.php:1352
pub fn is_used(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let ed = card::get(g, card_req(card)?, "extraDatas")?;
    b(php::loose_eq(php::coalesce(php::get(&ed, "userPower"), &json!(false)), &json!(true)))
}

/// The error of the Smoke Them Out helpers called as conditions: `$pId` is the card object,
/// used as an array key
fn illegal_card_offset() -> PhpError {
    PhpError::new("TypeError: Cannot access offset of type ALT\\Models\\Card on array")
}

/// `getSmokeThemOutCharges($pId, $cardId = null)` called as a condition (`$pId` = the card)
// PHP: Helpers/Conditions.php:1362
pub fn get_smoke_them_out_charges(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if card.is_some() {
        return Err(illegal_card_offset());
    }
    Ok(json!(smoke_them_out_charges(g, &Value::Null, event)?))
}

/// `getSmokeThemOutCharges($pId, $cardId = null)`: charges are stored per copy
/// (pId => [cardId => charges]); legacy saves stored a player-wide count
// PHP: Helpers/Conditions.php:1362
pub fn smoke_them_out_charges(g: &mut Game, pid: &Value, card_id: &Value) -> R<i64> {
    let armed = globals::get(g, "smokeThemOutArmed")?;
    let val = php::coalesce(at(&armed, pid), &json!(0)).clone();
    if php::is_arr(&val) {
        if card_id.is_null() {
            let s: f64 = php::vals(&val).into_iter().map(php::floatval).sum();
            return Ok(s.trunc() as i64);
        }
        return Ok(php::intval(php::coalesce(at(&val, card_id), &json!(0))));
    }
    Ok(php::intval(&val))
}

/// `isSmokeThemOutArmed($card, $event)`
// PHP: Helpers/Conditions.php:1374
pub fn is_smoke_them_out_armed(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let pid = pidv(g, c)?;
    b(smoke_them_out_charges(g, &pid, &json!(c))? > 0)
}

/// `armSmokeThemOut($pId, $cardId)` called as a condition (`$pId` = the card)
// PHP: Helpers/Conditions.php:1379
pub fn arm_smoke_them_out(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(illegal_card_offset())
}

/// `armSmokeThemOut($pId, $cardId)`
// PHP: Helpers/Conditions.php:1379
pub fn arm_smoke_them_out_pid(g: &mut Game, pid: i64, card_id: i64) -> R<()> {
    let mut armed = globals::get(g, "smokeThemOutArmed")?;
    let pk = pid.to_string();
    let ck = card_id.to_string();
    let cur = php::get(&armed, &pk).clone();
    let mut charges = if php::is_arr(&cur) { cur } else { php::arr() };
    let n = php::intval(php::coalesce(php::get(&charges, &ck), &json!(0))) + 1;
    php::set(&mut charges, &ck, json!(n));
    php::set(&mut armed, &pk, charges);
    globals::set(g, "smokeThemOutArmed", armed)?;
    Ok(())
}

/// `consumeSmokeThemOutCharge($pId, $cardId = null)` called as a condition (`$pId` = the card)
// PHP: Helpers/Conditions.php:1388
pub fn consume_smoke_them_out_charge(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(illegal_card_offset())
}

/// `consumeSmokeThemOutCharge($pId, $cardId = null)`
// PHP: Helpers/Conditions.php:1388
pub fn consume_smoke_them_out_charge_pid(g: &mut Game, pid: i64, card_id: Option<i64>) -> R<()> {
    let mut armed = globals::get(g, "smokeThemOutArmed")?;
    let pk = pid.to_string();
    let charges = php::get(&armed, &pk).clone();
    let cid = card_id.map(|c| json!(c)).unwrap_or(Value::Null);

    if !php::is_arr(&charges) {
        let remaining = smoke_them_out_charges(g, &json!(pid), &cid)? - 1;
        if remaining < 0 {
            return Ok(());
        }
        if remaining == 0 {
            php::unset(&mut armed, &pk);
        } else {
            php::set(&mut armed, &pk, json!(remaining));
        }
        globals::set(g, "smokeThemOutArmed", armed)?;
        return Ok(());
    }

    let Some(card_id) = card_id else { return Ok(()) };
    let ck = card_id.to_string();
    if php::le(php::coalesce(php::get(&charges, &ck), &json!(0)), &json!(0)) {
        return Ok(());
    }
    let mut charges = charges;
    let n = php::intval(php::get(&charges, &ck)) - 1;
    php::set(&mut charges, &ck, json!(n));
    if n <= 0 {
        php::unset(&mut charges, &ck);
    }
    if php::empty(&charges) {
        php::unset(&mut armed, &pk);
    } else {
        php::set(&mut armed, &pk, charges);
    }
    globals::set(g, "smokeThemOutArmed", armed)?;
    Ok(())
}

/// `disarmSmokeThemOut($pId)` called as a condition (`$pId` = the card)
// PHP: Helpers/Conditions.php:1423
pub fn disarm_smoke_them_out(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(illegal_card_offset())
}

/// `disarmSmokeThemOut($pId)`
// PHP: Helpers/Conditions.php:1423
pub fn disarm_smoke_them_out_pid(g: &mut Game, pid: i64) -> R<()> {
    let mut armed = globals::get(g, "smokeThemOutArmed")?;
    php::unset(&mut armed, &pid.to_string());
    globals::set(g, "smokeThemOutArmed", armed)?;
    Ok(())
}

/// `smokeThemOutLyTrigger($card, $event)`
// PHP: Helpers/Conditions.php:1436
pub fn smoke_them_out_ly_trigger(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !is_me_b(g, card, event)? || !php::truthy(&is_my_turn(g, card, event, &[])?) {
        return b(false);
    }
    let action = php::coalesce(ev(event, "action"), &json!("")).clone();
    let is_support_discard = php::loose_eq(&action, &json!("Discard")) && php::truthy(&is_support_effect(g, card, event, &[])?);
    let is_support_choose = php::loose_eq(&action, &json!("ChooseAssignment")) && php::truthy(&is_support_effect(g, card, event, &[])?);
    if !is_support_discard && !is_support_choose {
        return b(false);
    }
    if php::truthy(&is_this_feat_incomplete(g, card, event, &[])?) {
        return check_ability_activated_this_turn_type_count(g, card, event, &[json!("discard"), json!(2)]);
    }
    if !php::truthy(&is_smoke_them_out_armed(g, card, event, &[])?) {
        return b(false);
    }
    check_ability_activated_this_turn_type_count(g, card, event, &[json!("discard"), json!(1)])
}

/// `smokeThemOutAxTrigger($card, $event)`
// PHP: Helpers/Conditions.php:1464
pub fn smoke_them_out_ax_trigger(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !is_me_b(g, card, event)? || !php::truthy(&is_my_turn(g, card, event, &[])?) {
        return b(false);
    }
    if php::truthy(&is_this_feat_incomplete(g, card, event, &[])?) {
        return check_ability_activated_this_turn_type_count(g, card, event, &[json!("tap"), json!(3)]);
    }
    if !php::truthy(&is_smoke_them_out_armed(g, card, event, &[])?) {
        return b(false);
    }
    if !php::truthy(&check_ability_activated_this_turn_type_count(g, card, event, &[json!("tap"), json!(1)])?) {
        return b(false);
    }
    exclude_self(g, card, event, &[])
}

/// `hasNoBoost($card, $event)`
// PHP: Helpers/Conditions.php:1486
pub fn has_no_boost(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    has_boost(g, card, event, &[json!(0), json!("LTE")])
}

/// `hasBoost($card, $event, $n = 1, $op = 'GTE')`: the event's `boost` (status before a move)
/// wins over the card's
// PHP: Helpers/Conditions.php:1491
pub fn has_boost(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    let op = opt(args, 1, json!("GTE"));
    let mut m = json!(card::count_token(g, card_req(card)?, "boost")?);
    if php::isset(event, "boost") {
        m = ev(event, "boost").clone();
    }
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&m, &n));
    }
    if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&m, &n));
    }
    Err(die("Unknown op for hasBoost"))
}

/// `hasCounterOnCard($card, $event, $n = 1, $op = 'GTE')`
// PHP: Helpers/Conditions.php:1509
pub fn has_counter_on_card(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    let op = opt(args, 1, json!("GTE"));
    let ed = card::get(g, card_req(card)?, "extraDatas")?;
    let m = php::coalesce(php::get(&ed, "counter"), &json!(0)).clone();
    cmp_gte_lte_eq(&m, &n, &op, "hasCounterOnCard")
}

/// `heroHasCounter($card, $event, $n = 1, $op = 'GTE')`
// PHP: Helpers/Conditions.php:1524
pub fn hero_has_counter(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    let op = opt(args, 1, json!("GTE"));
    let p = player_of(g, card_req(card)?)?;
    let hero = player::get_hero(g, p)?;
    has_counter_on_card(g, hero, event, &[n, op])
}

/// `hasGainedBoost($card, $event, $n = 1)`
// PHP: Helpers/Conditions.php:1529
pub fn has_gained_boost(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    has_gained(g, card, event, &[json!("boost"), n])
}

/// `hasGained($card, $event, $type, $n = 1)`
// PHP: Helpers/Conditions.php:1534
pub fn has_gained(_g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "hasGained")?;
    if php::loose_ne(ev(event, "action"), &json!(GAIN)) {
        return b(false);
    }
    let gain = ev(event, "gain");
    let c = card_req(card)?;
    if php::loose_ne(ev(gain, "cardId"), &json!(c)) {
        return b(false);
    }
    if php::loose_ne(ev(gain, "type"), &ty) {
        return b(false);
    }
    b(true)
}

/// `isGain($card, $event, $type, $n = 1)`
// PHP: Helpers/Conditions.php:1550
pub fn is_gain(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "isGain")?;
    if php::loose_ne(ev(event, "action"), &json!(GAIN)) {
        return b(false);
    }
    if php::loose_ne(ev(ev(event, "gain"), "type"), &ty) {
        return b(false);
    }
    b(true)
}

/// `isMyGainInReserve($card, $event)`
// PHP: Helpers/Conditions.php:1563
pub fn is_my_gain_in_reserve(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if php::loose_ne(ev(event, "action"), &json!(GAIN)) || php::loose_ne(ev(event, "location"), &json!("reserve")) {
        return b(false);
    }
    let c = card_req(card)?;
    let x = cards::get(g, ev(ev(event, "gain"), "cardId"))?;
    b(php::loose_eq(&pidv(g, c)?, &pidv(g, x)?))
}

/// `isGainInStorms($card, $event)`
// PHP: Helpers/Conditions.php:1568
pub fn is_gain_in_storms(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(ev(event, "action"), &json!(GAIN)) && is_storm(php::coalesce(ev(event, "location"), &json!(""))))
}

/// `isGainCardType($card, $event, $type)`
// PHP: Helpers/Conditions.php:1573
pub fn is_gain_card_type(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "isGainCardType")?;
    b(type_check_v(&ty, ev(event, "cardType"), ev(event, "token"), &php::arr())?)
}

/// `hasGainedFleeting($card, $event)`
// PHP: Helpers/Conditions.php:1578
pub fn has_gained_fleeting(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if php::loose_ne(ev(event, "action"), &json!(GAIN)) {
        return b(false);
    }
    let gain = ev(event, "gain");
    if gain.is_null() {
        return b(false);
    }
    let x = cards::get(g, ev(gain, "cardId"))?;
    let c = card_req(card)?;
    if php::loose_ne(&pidv(g, x)?, &pidv(g, c)?) {
        return b(false);
    }
    let x = cards::get(g, ev(gain, "cardId"))?;
    if !php::in_array(&type_of(g, x)?, &json!(["token", "character"])) {
        return b(false);
    }
    if php::loose_ne(ev(gain, "type"), &json!("fleeting")) {
        return b(false);
    }
    b(true)
}

/// `hasPlayerGained($card, $event, $type)`
// PHP: Helpers/Conditions.php:1602
pub fn has_player_gained(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "hasPlayerGained")?;
    if php::loose_ne(ev(event, "action"), &json!(GAIN)) {
        return b(false);
    }
    let gain = ev(event, "gain");
    let x = cards::get(g, ev(gain, "cardId"))?;
    let c = card_req(card)?;
    if php::loose_ne(&pidv(g, x)?, &pidv(g, c)?) {
        return b(false);
    }
    if php::loose_ne(ev(gain, "type"), &ty) {
        return b(false);
    }
    b(true)
}

/// `isPlayedInSameLocation($card, $event)`
// PHP: Helpers/Conditions.php:1619
pub fn is_played_in_same_location(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    if php::loose_ne(&pidv(g, c)?, php::coalesce(ev(event, "locationPId"), &json!(-1))) {
        return b(false);
    }
    b(php::loose_eq(&locv(g, c)?, php::coalesce(ev(event, "to"), &json!(""))) || php::truthy(ev(event, "gigantic")) || gigantic(g, c)?)
}

/// `isNotMeInvoke($card, $event)`
// PHP: Helpers/Conditions.php:1626
pub fn is_not_me_invoke(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let pid = pidv(g, c)?;
    b(php::loose_ne(&pid, php::coalesce(ev(event, "locationPId"), &pid)))
}

/// `isMeInvoke($card, $event)`
// PHP: Helpers/Conditions.php:1631
pub fn is_me_invoke(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let pid = pidv(g, c)?;
    b(php::loose_eq(&pid, php::coalesce(ev(event, "locationPId"), &pid)))
}

/// `isNotPlayedInSameLocation($card, $event)`
// PHP: Helpers/Conditions.php:1636
pub fn is_not_played_in_same_location(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(php::loose_ne(&locv(g, c)?, php::coalesce(ev(event, "to"), &json!(""))))
}

/// `isAddedCardEvent($card, $event, $playedOnly = false, $allPlayers = false)`
// PHP: Helpers/Conditions.php:1650
pub fn is_added_card_event(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let played_only = opt(args, 0, json!(false));
    let all_players = opt(args, 1, json!(false));
    if !php::truthy(ev(event, "playCard")) {
        return b(false);
    }
    if !is_me_b(g, card, event)? && !php::truthy(&all_players) {
        return b(false);
    }
    if php::truthy(&played_only) && php::truthy(ev(event, "putAndNotPlayed")) {
        return b(false);
    }
    b(true)
}

/// `isExhaustedInLocation($card, $event, $location)`
// PHP: Helpers/Conditions.php:1668
pub fn is_exhausted_in_location(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let location = req(args, 0, "isExhaustedInLocation")?;
    b(php::loose_eq(php::coalesce(ev(event, "cardLocation"), &json!("")), &location))
}

/// `isCardOfType($card, $event, $type = null)`
// PHP: Helpers/Conditions.php:1673
pub fn is_card_of_type(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    if !type_check_v(&ty, ev(event, "cardType"), ev(event, "token"), ev(event, "additionalType"))? {
        return b(false);
    }
    if php::in_array(&ty, &subtypes()) && !in_array_t(&ty, ev(event, "cardSubtypes"))? {
        return b(false);
    }
    b(true)
}

/// `isCardOfTypeRobotOrToken($card, $event)`
// PHP: Helpers/Conditions.php:1687
pub fn is_card_of_type_robot_or_token(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if php::truthy(&is_card_of_type(g, card, event, &[json!("robot")])?) {
        return b(true);
    }
    b(php::truthy(&is_card_of_type(g, card, event, &[json!("token")])?))
}

/// `isCardAddedRobotOrToken($card, $event)`
// PHP: Helpers/Conditions.php:1692
pub fn is_card_added_robot_or_token(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if php::truthy(&is_card_added_any_player(g, card, event, &[json!("robot")])?) {
        return b(true);
    }
    b(php::truthy(&is_card_added_any_player(g, card, event, &[json!("token")])?))
}

/// The type, subtype and cost checks shared by isCardAdded* and isAddedCardOpponentEvent
fn played_card_checks(g: &mut Game, event: &Value, played: CardId, ty: &Value, cost: &Value, op: &Value) -> R<bool> {
    if !type_check_v(ty, ev(event, "cardType"), ev(event, "token"), ev(event, "additionalType"))? {
        return Ok(false);
    }
    if php::in_array(ty, &subtypes()) && !php::in_array(ty, &card::get_subtypes(g, played)?) {
        return Ok(false);
    }
    if !cost.is_null() {
        let cost_hand = card::get(g, played, "costHand")?;
        if php::loose_eq(op, &json!("GTE")) && php::lt(&cost_hand, cost) {
            return Ok(false);
        }
        if php::loose_eq(op, &json!("LTE")) && php::gt(&cost_hand, cost) {
            return Ok(false);
        }
        if php::loose_eq(op, &json!("E")) && php::loose_ne(&cost_hand, cost) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `isCardAddedAnyPlayer($card, $event, $type = null, $cost = null, $op = 'GTE',
/// $excludeMyself = '', $playedOnly = false)`
// PHP: Helpers/Conditions.php:1697
pub fn is_card_added_any_player(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    let cost = opt(args, 1, Value::Null);
    let op = opt(args, 2, json!("GTE"));
    let ex = opt(args, 3, json!(""));
    let played_only = opt(args, 4, json!(false));
    if !php::truthy(&is_added_card_event(g, card, event, &[played_only, json!(true)])?) {
        return b(false);
    }
    let played = cards::get(g, ev(event, "cardId"))?;
    if php::loose_eq(ev(event, "method"), &json!("MoveCard")) && php::loose_eq(php::coalesce(ev(event, "defect"), &json!(false)), &json!(false)) {
        return b(false);
    }
    let c = card_req(card)?;
    if php::loose_ne(&pidv(g, c)?, &pidv(g, played)?) {
        return b(false);
    }
    if php::loose_eq(&ex, &json!("true")) && php::loose_eq(&json!(c), ev(event, "cardId")) {
        return b(false);
    }
    b(played_card_checks(g, event, played, &ty, &cost, &op)?)
}

/// `isCardAdded($card, $event, $type = null, $cost = null, $op = 'GTE', $excludeMyself = '',
/// $playedOnly = false)` (`$playedOnly` is not passed to isAddedCardEvent)
// PHP: Helpers/Conditions.php:1747
pub fn is_card_added(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    let cost = opt(args, 1, Value::Null);
    let op = opt(args, 2, json!("GTE"));
    let ex = opt(args, 3, json!(""));
    if !php::truthy(&is_added_card_event(g, card, event, &[])?) {
        return b(false);
    }
    let played = cards::get(g, ev(event, "cardId"))?;
    let c = card_req(card)?;
    if php::loose_eq(&ex, &json!("true")) && php::loose_eq(&json!(c), ev(event, "cardId")) {
        return b(false);
    }
    b(played_card_checks(g, event, played, &ty, &cost, &op)?)
}

/// `isCardPlayed($card, $event, $type, $cost = null, $op = 'GTE', $excludeMyself = '')`
// PHP: Helpers/Conditions.php:1788
pub fn is_card_played(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "isCardPlayed")?;
    let cost = opt(args, 1, Value::Null);
    let op = opt(args, 2, json!("GTE"));
    let ex = opt(args, 3, json!(""));
    is_card_added(g, card, event, &[ty, cost, op, ex, json!(true)])
}

/// `isCardPlayedNotCharacter($card, $event)`
// PHP: Helpers/Conditions.php:1793
pub fn is_card_played_not_character(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&is_card_added(g, card, event, &[json!("character")])?))
}

/// `isCardPlayedWithZeroStat($card, $event)`
// PHP: Helpers/Conditions.php:1798
pub fn is_card_played_with_zero_stat(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::truthy(&is_card_played(g, card, event, &[json!("character")])?) {
        return b(false);
    }
    let played = cards::get(g, ev(event, "cardId"))?;
    for v in php::vals(&card::get_biomes(g, played, false, false)?) {
        if php::loose_eq(v, &json!(0)) {
            return b(true);
        }
    }
    b(false)
}

/// `cardPlayedCostCheck($card, $event, $cost, $check = 'base', $op = 'GTE')` (null for other ops)
// PHP: Helpers/Conditions.php:1815
pub fn card_played_cost_check(g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let cost = req(args, 0, "cardPlayedCostCheck")?;
    let check = opt(args, 1, json!("base"));
    let op = opt(args, 2, json!("GTE"));
    if !php::isset(event, "cardId") {
        return b(false);
    }
    let played = cards::get(g, ev(event, "cardId"))?;
    let compare = if php::loose_eq(&check, &json!("base")) {
        if php::loose_eq(ev(event, "from"), &json!("reserve")) {
            card::get(g, played, "costReserve")?
        } else {
            card::get(g, played, "costHand")?
        }
    } else if php::loose_eq(&check, &json!("hand")) {
        card::get(g, played, "costHand")?
    } else {
        card::get(g, played, "costReserve")?
    };
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&compare, &cost));
    }
    if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&compare, &cost));
    }
    Ok(Value::Null)
}

/// `isCharacterFromReserveNotBlocked($card, $event)`
// PHP: Helpers/Conditions.php:1841
pub fn is_character_from_reserve_not_blocked(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::truthy(&is_card_played(g, card, event, &[json!("character")])?) {
        return b(false);
    }
    let mut additional_effect = false;
    for e in php::vals(ev(event, "additionalEffects")) {
        if php::loose_eq(ev(e, "type"), ev(event, "cardType")) {
            additional_effect = true;
        }
    }
    if !(php::loose_eq(ev(event, "from"), &json!("reserve")) || additional_effect) {
        return b(false);
    }
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    let gi = gigantic(g, c)?;
    b(!has_opponent_blocking_power(g, p, &php::strval(ev(event, "to")), gi)?)
}

/// `isCharacterCostHigherThanCounter($card, $event)`
// PHP: Helpers/Conditions.php:1858
pub fn is_character_cost_higher_than_counter(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::truthy(&is_card_played(g, card, event, &[json!("character")])?) {
        return b(false);
    }
    let played = cards::get(g, ev(event, "cardId"))?;
    if !php::loose_eq(php::coalesce(ev(event, "reallyPlayed"), &json!(true)), &json!(true)) {
        return b(false);
    }
    let cost_hand = card::get(g, played, "costHand")?;
    let ed = card::get(g, card_req(card)?, "extraDatas")?;
    b(php::ge(&cost_hand, php::coalesce(php::get(&ed, "counter"), &json!(0))))
}

/// `isReallyPlayed($card, $event)`
// PHP: Helpers/Conditions.php:1868
pub fn is_really_played(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(php::coalesce(ev(event, "reallyPlayed"), &json!(true)), &json!(true)))
}

/// `specialEffect($card, $event, $effect)`
// PHP: Helpers/Conditions.php:1873
pub fn special_effect(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let effect = req(args, 0, "specialEffect")?;
    b(php::loose_eq(php::coalesce(ev(event, "specialEffect"), &json!("")), &effect))
}

/// `isPlayCard($card, $event)`
// PHP: Helpers/Conditions.php:1878
pub fn is_play_card(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::truthy(ev(event, "playCard")))
}

/// `isAddedCardAnyPlayer($card, $event, $type = null)`
// PHP: Helpers/Conditions.php:1886
pub fn is_added_card_any_player(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    if !php::truthy(ev(event, "playCard")) {
        return b(false);
    }
    b(type_check_v(&ty, ev(event, "cardType"), ev(event, "token"), ev(event, "additionalType"))?)
}

/// `isAddedCardOpponentEvent($card, $event, $type = null, $cost = null, $op = 'GTE',
/// $excludeMyself = '', $playedOnly = false)`
// PHP: Helpers/Conditions.php:1900
pub fn is_added_card_opponent_event(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    let cost = opt(args, 1, Value::Null);
    let op = opt(args, 2, json!("GTE"));
    let ex = opt(args, 3, json!(""));
    let played_only = opt(args, 4, json!(false));
    if !php::truthy(ev(event, "playCard")) {
        return b(false);
    }
    if is_me_b(g, card, event)? {
        return b(false);
    }
    if php::truthy(&played_only) && php::truthy(ev(event, "putAndNotPlayed")) {
        return b(false);
    }
    let played = cards::get(g, ev(event, "cardId"))?;
    let c = card_req(card)?;
    if php::loose_eq(&ex, &json!("true")) && php::loose_eq(&json!(c), ev(event, "cardId")) {
        return b(false);
    }
    b(played_card_checks(g, event, played, &ty, &cost, &op)?)
}

/// `hasMarkers($card, $event)`
// PHP: Helpers/Conditions.php:1952
pub fn has_markers(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(!meeples::get_of_type(g, &format!("card-{}", c), &json!(["ocean", "forest", "mountain"]))?.is_empty())
}

/// `isDiscardedEvent($card, $event, $meOnly = false)`
// PHP: Helpers/Conditions.php:1967
pub fn is_discarded_event(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let me_only = opt(args, 0, json!(false));
    if !php::truthy(ev(event, "discardCard")) {
        return b(false);
    }
    if php::truthy(&me_only) && !is_me_b(g, card, event)? {
        return b(false);
    }
    b(true)
}

/// `isDiscarded($card, $event, $from = null, $to = null, $type = null)`
// PHP: Helpers/Conditions.php:1980
pub fn is_discarded(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let from = opt(args, 0, Value::Null);
    let to = opt(args, 1, Value::Null);
    let ty = opt(args, 2, Value::Null);
    if !php::truthy(&is_discarded_event(g, card, event, &[])?) {
        return b(false);
    }
    if !from.is_null() && php::loose_ne(&from, &json!("")) && php::loose_ne(&from, ev(event, "from")) {
        return b(false);
    }
    if !to.is_null() && php::loose_ne(&to, &json!("")) && php::loose_ne(&to, ev(event, "to")) {
        return b(false);
    }
    if !ty.is_null() {
        let d = cards::get(g, ev(event, "cardId"))?;
        let dt = type_of(g, d)?;
        let tok = json!(card::is_token(g, d)?);
        let at = card::get_additional_type(g, d)?;
        if !type_check_v(&ty, &dt, &tok, &at)? {
            return b(false);
        }
    }
    b(true)
}

/// `isMyselfDiscarded($card, $event, $from = null, $to = null)`
// PHP: Helpers/Conditions.php:2003
pub fn is_myself_discarded(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let from = opt(args, 0, Value::Null);
    let to = opt(args, 1, Value::Null);
    let c = card_req(card)?;
    if php::loose_ne(ev(event, "cardId"), &json!(c)) {
        return b(false);
    }
    b(php::truthy(&is_discarded(g, card, event, &[from, to])?))
}

/// `notDiscarded($card, $event)`
// PHP: Helpers/Conditions.php:2008
pub fn not_discarded(_g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    b(php::loose_eq(ev(event, "cardId"), &json!(c)) && php::loose_eq(php::coalesce(ev(event, "to"), &json!("")), &json!("reserve")))
}

/// `notDestroyed($card, $event)`
// PHP: Helpers/Conditions.php:2013
pub fn not_destroyed(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(card::location(g, card_req(card)?)? != "destroy")
}

/// `notInDiscard($card, $event)`
// PHP: Helpers/Conditions.php:2018
pub fn not_in_discard(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(card::location(g, card_req(card)?)? != "discard")
}

/// `isSacrifice($card, $event, $type = null, $subType = null, $exclude = false)`
// PHP: Helpers/Conditions.php:2023
pub fn is_sacrifice(g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    let sub_type = opt(args, 1, Value::Null);
    let exclude = opt(args, 2, json!(false));
    if !php::truthy(ev(event, "sacrifice")) {
        return b(false);
    }
    if !ty.is_null() {
        let d = cards::get(g, ev(event, "cardId"))?;
        let dt = type_of(g, d)?;
        let tok = json!(card::is_token(g, d)?);
        let at = card::get_additional_type(g, d)?;
        if !type_check_v(&ty, &dt, &tok, &at)? {
            return b(false);
        }
    }
    if !sub_type.is_null() {
        let d = cards::get(g, ev(event, "cardId"))?;
        let st = card::get_subtypes(g, d)?;
        if !sub_type_check_v(&sub_type, &st, &exclude)? {
            return b(false);
        }
    }
    b(true)
}

/// `isSacrificeCharacterPermanent($card, $event)`
// PHP: Helpers/Conditions.php:2046
pub fn is_sacrifice_character_permanent(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if php::truthy(&is_sacrifice(g, card, event, &[json!("permanent")])?) {
        return b(true);
    }
    b(php::truthy(&is_sacrifice(g, card, event, &[json!("character")])?))
}

/// `isSacrificed($card, $event)`
// PHP: Helpers/Conditions.php:2051
pub fn is_sacrificed(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::truthy(&is_sacrifice(g, card, event, &[])?) {
        return b(false);
    }
    b(php::truthy(&is_myself_discarded(g, card, event, &[])?))
}

/// `isDiscardedType($card, $event, $type = null)`: the card's type, or the event's when the card
/// no longer exists
// PHP: Helpers/Conditions.php:2056
pub fn is_discarded_type(g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    if !ty.is_null() {
        let (dt, tok, at) = match cards::get_single(g, ev(event, "cardId"), false)? {
            None => (
                php::coalesce(ev(event, "cardType"), &json!("notgood")).clone(),
                php::coalesce(ev(event, "token"), &json!(false)).clone(),
                php::coalesce(ev(event, "additionalType"), &php::arr()).clone(),
            ),
            Some(d) => (type_of(g, d)?, json!(card::is_token(g, d)?), card::get_additional_type(g, d)?),
        };
        if !type_check_v(&ty, &dt, &tok, &at)? {
            return b(false);
        }
    }
    b(true)
}

/// `isNotToken($card, $event)`
// PHP: Helpers/Conditions.php:2076
pub fn is_not_token(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(ev(event, "token")))
}

/// `countOwnerBoosts($card, $event, $n = 1, $op = 'GTE')` (null for other ops)
// PHP: Helpers/Conditions.php:2081
pub fn count_owner_boosts(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    let op = opt(args, 1, json!("GTE"));
    let c = card_req(card)?;
    let boosts = json!(meeples::get_filtered(g, &pidv(g, c)?, &Value::Null, &json!("boost"))?.len());
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&boosts, &n));
    } else if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&boosts, &n));
    }
    Ok(Value::Null)
}

/// `isOpponentDraw($card, $event)`
// PHP: Helpers/Conditions.php:2100
pub fn is_opponent_draw(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::truthy(&is_not_me(g, card, event, &[])?) {
        return b(false);
    }
    b(php::loose_ne(ev(event, "location"), &json!("mana")))
}

/// `realResupply($card, $event)`
// PHP: Helpers/Conditions.php:2105
pub fn real_resupply(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(php::coalesce(ev(event, "notResupply"), &json!(false)), &json!(false)))
}

/// `amIResupplied($card, $event)`
// PHP: Helpers/Conditions.php:2110
pub fn am_i_resupplied(_g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if php::loose_ne(ev(event, "action"), &json!("Resupply")) {
        return b(false);
    }
    let c = card_req(card)?;
    b(in_array_t(&json!(c), ev(event, "cardIds"))?)
}

/// `isPermanentFromTarget($card, $event)`
// PHP: Helpers/Conditions.php:2115
pub fn is_permanent_from_target(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = cards::get(g, ev(event, "cardId"))?;
    b(php::loose_eq(&type_of(g, c)?, &json!("permanent")) || php::in_array(&json!("permanent"), &card::get_additional_type(g, c)?))
}

/// `isSpellFromTarget($card, $event)`
// PHP: Helpers/Conditions.php:2121
pub fn is_spell_from_target(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = cards::get(g, ev(event, "cardId"))?;
    b(php::loose_eq(&type_of(g, c)?, &json!("spell")) || php::in_array(&json!("spell"), &card::get_additional_type(g, c)?))
}

/// `isCharacterFromTarget($card, $event)`
// PHP: Helpers/Conditions.php:2127
pub fn is_character_from_target(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = cards::get(g, ev(event, "cardId"))?;
    b(php::in_array(&type_of(g, c)?, &json!(["character", "token"])))
}

/// `isHandEmpty($card, $event)`
// PHP: Helpers/Conditions.php:2133
pub fn is_hand_empty(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    b(hand_count(g, p)? == 0)
}

/// `isReserveEmpty($card, $event)`
// PHP: Helpers/Conditions.php:2138
pub fn is_reserve_empty(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    b(player::get_reserve_cards(g, p)?.is_empty())
}

/// `hasXExhaustedReserve($card, $event, $n, $op = 'GTE', $player = null)` (null for other ops)
// PHP: Helpers/Conditions.php:2143
pub fn has_x_exhausted_reserve(g: &mut Game, _card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "hasXExhaustedReserve")?;
    let op = opt(args, 1, json!("GTE"));
    let only = opt(args, 2, Value::Null);
    let mut count = 0;
    for p in players::get_all(g) {
        if !only.is_null() && php::loose_ne(&json!(p), &only) {
            continue;
        }
        for c in player::get_reserve_cards(g, p)? {
            if card::is_tapped(g, c)? {
                count += 1;
            }
        }
    }
    let count = json!(count);
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&count, &n));
    } else if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&count, &n));
    } else if php::loose_eq(&op, &json!("EQ")) {
        return b(php::loose_eq(&count, &n));
    }
    Ok(Value::Null)
}

/// `$excludeMove` of isInBiome / allExpeditionsNotIn: a string is true only for 'true'
fn exclude_move_arg(v: &Value) -> bool {
    if let Value::String(s) = v {
        return s == "true";
    }
    php::truthy(v)
}

/// `isInBiome($card, $event, $biome, $excludeMove = false)`
// PHP: Helpers/Conditions.php:2164
pub fn is_in_biome(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let biome = req(args, 0, "isInBiome")?;
    let exclude_move = exclude_move_arg(&opt(args, 1, json!(false)));
    let c = card_req(card)?;
    if gigantic(g, c)? {
        let p = player_of(g, c)?;
        if player_is_in_biome(g, p, STORM_LEFT, &biome, exclude_move)? {
            return b(true);
        }
        let p = player_of(g, c)?;
        return b(player_is_in_biome(g, p, STORM_RIGHT, &biome, exclude_move)?);
    }
    let p = player_of(g, c)?;
    let loc = card::location(g, c)?;
    b(player_is_in_biome(g, p, &loc, &biome, exclude_move)?)
}

/// `isNotInBiome($card, $event, $biome)`
// PHP: Helpers/Conditions.php:2177
pub fn is_not_in_biome(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let biome = req(args, 0, "isNotInBiome")?;
    b(!php::truthy(&is_in_biome(g, card, event, &[biome])?))
}

/// `allExpeditionsNotIn($card, $event, $biome, $excludeMove = false)`
// PHP: Helpers/Conditions.php:2182
pub fn all_expeditions_not_in(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let biome = req(args, 0, "allExpeditionsNotIn")?;
    let exclude_move = exclude_move_arg(&opt(args, 1, json!(false)));
    let c = card_req(card)?;
    let p = player_of(g, c)?;
    if player_is_in_biome(g, p, STORM_LEFT, &biome, exclude_move)? {
        return b(false);
    }
    let p = player_of(g, c)?;
    b(!player_is_in_biome(g, p, STORM_RIGHT, &biome, exclude_move)?)
}

/// `isInBiomeWithZeroStat($card, $event)`
// PHP: Helpers/Conditions.php:2194
pub fn is_in_biome_with_zero_stat(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::truthy(&is_card_played_with_zero_stat(g, card, event, &[])?) {
        return b(false);
    }
    let played = cards::get(g, ev(event, "cardId"))?;
    let biomes = card::get_biomes(g, played, false, false)?;
    for (biome, v) in php::entries(&biomes) {
        if php::loose_eq(v, &json!(0)) && php::truthy(&is_in_biome(g, Some(played), event, &[php::key_value(&biome)])?) {
            return b(true);
        }
    }
    b(false)
}

/// `isTargetAsleep($card, $event)`
// PHP: Helpers/Conditions.php:2210
pub fn is_target_asleep(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = cards::get(g, ev(event, "cardId"))?;
    b(card::has_token(g, c, "asleep")?)
}

/// `isTargetSameOwner($card, $event)`
// PHP: Helpers/Conditions.php:2215
pub fn is_target_same_owner(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let x = cards::get(g, ev(event, "cardId"))?;
    let c = card_req(card)?;
    b(php::loose_eq(&pidv(g, x)?, &pidv(g, c)?))
}

/// `isPlayedCardInBiome($card, $event, $biome)`
// PHP: Helpers/Conditions.php:2220
pub fn is_played_card_in_biome(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let biome = req(args, 0, "isPlayedCardInBiome")?;
    let p = player_of(g, card_req(card)?)?;
    let x = cards::get(g, ev(event, "cardId"))?;
    let loc = card::location(g, x)?;
    b(player_is_in_biome(g, p, &loc, &biome, false)?)
}

/// `isDiscardedCardInBiome($card, $event, $biome)`: a Gigantic card (when targeted) counts in
/// both expeditions
// PHP: Helpers/Conditions.php:2225
pub fn is_discarded_card_in_biome(g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let biome = req(args, 0, "isDiscardedCardInBiome")?;
    if !php::isset(event, "cardId") {
        return b(false);
    }
    let d = cards::get(g, ev(event, "cardId"))?;
    let player = card::get_player(g, d)?;
    let from = php::coalesce(ev(event, "from"), php::coalesce(ev(event, "cardFrom"), &json!(""))).clone();
    if !is_storm(&from) {
        return b(false);
    }
    let player = player.ok_or_else(|| PhpError::new("Error: Call to a member function isInBiome() on null"))?;
    let from = php::strval(&from);
    let was_gigantic = php::truthy(ev(event, "wasGigantic")) || php::truthy(ev(event, "gigantic"));
    if was_gigantic {
        let other = other_exp(&from);
        if player_is_in_biome(g, player, &from, &biome, false)? {
            return b(true);
        }
        return b(player_is_in_biome(g, player, other, &biome, false)?);
    }
    b(player_is_in_biome(g, player, &from, &biome, false)?)
}

/// `isDiscardedCardNotInBiome($card, $event, $biome)`
// PHP: Helpers/Conditions.php:2249
pub fn is_discarded_card_not_in_biome(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let biome = req(args, 0, "isDiscardedCardNotInBiome")?;
    b(!php::truthy(&is_discarded_card_in_biome(g, card, event, &[biome])?))
}

/// The Characters / tokens of the card's player in an expedition (or Gigantic)
fn in_expeditions(g: &mut Game, c: CardId) -> R<Vec<CardId>> {
    let p = player_of(g, c)?;
    let mut out = vec![];
    for x in player::get_played_cards(g, p, &json!(["character", "token"]))? {
        if gigantic(g, x)? || STORMS.contains(&card::location(g, x)?.as_str()) {
            out.push(x);
        }
    }
    Ok(out)
}

/// `gatherThePackPass($card, $event, $filter = 'animal')`: three different printed base costs
// PHP: Helpers/Conditions.php:2260
pub fn gather_the_pack_pass(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let filter = opt(args, 0, json!("animal"));
    let c = card_req(card)?;
    let mut ids = in_expeditions(g, c)?;
    if php::strict_eq(&filter, &json!("animal")) {
        let mut out = vec![];
        for x in ids {
            if php::in_array(&json!("animal"), &card::get_subtypes(g, x)?) {
                out.push(x);
            }
        }
        ids = out;
    }
    let mut distinct: Vec<String> = vec![];
    for x in ids {
        let printed = if card::has_token(g, x, "fleeting")? { card::get(g, x, "costReserve")? } else { card::get(g, x, "costHand")? };
        let k = php::key_of(&printed);
        if !distinct.contains(&k) {
            distinct.push(k);
        }
    }
    b(distinct.len() >= 3)
}

/// `soundTheHowlPass($card, $event)`: three different Forest values among Animals
// PHP: Helpers/Conditions.php:2281
pub fn sound_the_howl_pass(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let mut distinct: Vec<i64> = vec![];
    for x in in_expeditions(g, c)? {
        if !php::in_array(&json!("animal"), &card::get_subtypes(g, x)?) {
            continue;
        }
        let k = card::get_int(g, x, "forest")? + card::count_token(g, x, "boost")? as i64;
        if !distinct.contains(&k) {
            distinct.push(k);
        }
    }
    b(distinct.len() >= 3)
}

/// `XCharacterInExpedition($card, $event, $n, $op = 'GTE')` (the comparison is `$n >= $nCards`;
/// null for other ops)
// PHP: Helpers/Conditions.php:2295
pub fn x_character_in_expedition(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = req(args, 0, "XCharacterInExpedition")?;
    let op = opt(args, 1, json!("GTE"));
    let c = card_req(card)?;
    let loc = card::location(g, c)?;
    let p = player_of(g, c)?;
    let mut n_cards = 0;
    for x in player::get_played_cards(g, p, &json!(["token", "character"]))? {
        if gigantic(g, x)? || card::location(g, x)? == loc {
            n_cards += 1;
        }
    }
    let n_cards = json!(n_cards);
    if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&n, &n_cards));
    } else if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&n, &n_cards));
    } else if php::loose_eq(&op, &json!("E")) {
        return b(php::loose_eq(&n, &n_cards));
    }
    Ok(Value::Null)
}

/// `isOpponentExpeditionEmpty($card, $event)`
// PHP: Helpers/Conditions.php:2311
pub fn is_opponent_expedition_empty(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(&count_opponent_expedition(g, card, event, &[json!("character")])?, &json!(0)))
}

/// `countOpponentExpedition($card, $event, $type = null)` (private): the opponent's cards facing
/// the card (an int)
// PHP: Helpers/Conditions.php:2328
pub fn count_opponent_expedition(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, Value::Null);
    let c = card_req(card)?;
    let opponent = last_opponent(g, g.card(c)?.pid)?;
    let opponent_cards = player::get_played_cards(g, opponent, &ty)?;
    let loc = card::location(g, c)?;
    let opposite = other_exp(&loc);
    let mut n = 0;
    for o in opponent_cards {
        let ol = card::location(g, o)?;
        if ol == loc || (ol == opposite && gigantic(g, o)?) || (ol == opposite && gigantic(g, c)?) {
            n += 1;
        }
    }
    Ok(json!(n))
}

/// `hasPresenceInBothStorms($cards, $excludeCardId = null)` (private) called as a condition:
/// `$cards` is the card object, whose foreach sees no public property
// PHP: Helpers/Conditions.php:2353
pub fn has_presence_in_both_storms(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(false)
}

/// `hasPresenceInBothStorms($cards, $excludeCardId = null)`: a card in each expedition, or a
/// Gigantic one
// PHP: Helpers/Conditions.php:2353
pub fn presence_in_both_storms(g: &mut Game, ids: &[CardId], exclude: Option<CardId>) -> R<bool> {
    let mut left = false;
    let mut right = false;
    for &c in ids {
        if exclude == Some(c) {
            continue;
        }
        if gigantic(g, c)? {
            return Ok(true);
        }
        let l = card::location(g, c)?;
        if l == STORM_LEFT {
            left = true;
        }
        if l == STORM_RIGHT {
            right = true;
        }
        if left && right {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `controlInAllExpeditions($card, $event, $type = null, $includeSelf = 'false')`
// PHP: Helpers/Conditions.php:2383
pub fn control_in_all_expeditions(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let mut ty = opt(args, 0, Value::Null);
    let include_self = opt(args, 1, json!("false"));
    if ty.is_null() || php::strict_eq(&ty, &json!("")) || php::strict_eq(&ty, &json!("character")) {
        ty = json!(["character", "token"]);
    }
    let c = card_req(card)?;
    let exclude = if php::strict_eq(&include_self, &json!("true")) { None } else { Some(c) };
    let p = player_of(g, c)?;
    let ids = player::get_played_cards(g, p, &ty)?;
    b(presence_in_both_storms(g, &ids, exclude)?)
}

/// `hasOtherCharacterInAllExpeditions($card, $event, $op = 'GTE')`
// PHP: Helpers/Conditions.php:2397
pub fn has_other_character_in_all_expeditions(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let op = opt(args, 0, json!("GTE"));
    let has_both = php::truthy(&control_in_all_expeditions(g, card, event, &[])?);
    if php::loose_eq(&op, &json!("LTE")) {
        return b(!has_both);
    }
    b(has_both)
}

/// `isOpponentExpeditionNotEmpty($card, $event)`
// PHP: Helpers/Conditions.php:2407
pub fn is_opponent_expedition_not_empty(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&is_opponent_expedition_empty(g, card, event, &[])?))
}

/// `isOpponentExpeditionFilled($card, $event, $type = '', $n = 1, $op = 'EQ')` (null for other ops)
// PHP: Helpers/Conditions.php:2412
pub fn is_opponent_expedition_filled(g: &mut Game, card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let ty = opt(args, 0, json!(""));
    let n = opt(args, 1, json!(1));
    let op = opt(args, 2, json!("EQ"));
    if php::loose_eq(&op, &json!("EQ")) {
        return b(php::loose_eq(&count_opponent_expedition(g, card, event, &[ty])?, &n));
    } else if php::loose_eq(&op, &json!("GTE")) {
        return b(php::ge(&count_opponent_expedition(g, card, event, &[ty])?, &n));
    } else if php::loose_eq(&op, &json!("LTE")) {
        return b(php::le(&count_opponent_expedition(g, card, event, &[ty])?, &n));
    }
    Ok(Value::Null)
}

/// `isPlayedInOpponentExpedition($card, $event)`
// PHP: Helpers/Conditions.php:2423
pub fn is_played_in_opponent_expedition(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let played = cards::get(g, ev(event, "cardId"))?;
    let c = card_req(card)?;
    let to = ev(event, "to");
    let placed = php::loose_eq(php::coalesce(to, &json!("")), &locv(g, c)?)
        || (gigantic(g, played)? && is_storm(to))
        || (gigantic(g, c)? && is_storm(to));
    b(placed && php::loose_ne(&pidv(g, played)?, &pidv(g, c)?))
}

/// `isPlayedInOpponentOtherExp($card, $event)`
// PHP: Helpers/Conditions.php:2429
pub fn is_played_in_opponent_other_exp(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let played = cards::get(g, ev(event, "cardId"))?;
    let c = card_req(card)?;
    let other = other_exp(&card::location(g, c)?);
    if php::loose_ne(php::coalesce(ev(event, "to"), &json!("")), &json!(other)) || php::loose_eq(&pidv(g, played)?, &pidv(g, c)?) {
        return b(false);
    }
    let n = next_of(g, g.card(c)?.pid)?;
    b(player::count_cards_in_location(g, n, other, &json!(["character"]))? == 1)
}

/// `isPlayedInOwnOtherExpedition($card, $event)`
// PHP: Helpers/Conditions.php:2436
pub fn is_played_in_own_other_expedition(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let played = cards::get(g, ev(event, "cardId"))?;
    let c = card_req(card)?;
    let other = other_exp(&card::location(g, c)?);
    let to = ev(event, "to");
    let placed = php::loose_eq(php::coalesce(to, &json!("")), &json!(other)) || (gigantic(g, played)? && is_storm(to));
    b(placed && php::loose_eq(&pidv(g, played)?, &pidv(g, c)?))
}

/// `isNotGigantic($card, $event)`: the event card
// PHP: Helpers/Conditions.php:2443
pub fn is_not_gigantic(g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let played = cards::get(g, ev(event, "cardId"))?;
    b(!gigantic(g, played)?)
}

/// `isOpponentExpeditionIn($card, $event, $biome)`
// PHP: Helpers/Conditions.php:2449
pub fn is_opponent_expedition_in(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let biome = req(args, 0, "isOpponentExpeditionIn")?;
    let c = card_req(card)?;
    let opponent = last_opponent(g, g.card(c)?.pid)?;
    let loc = card::location(g, c)?;
    if player_is_in_biome(g, opponent, &loc, &biome, true)? {
        return b(true);
    }
    if !gigantic(g, c)? {
        return b(false);
    }
    b(player_is_in_biome(g, opponent, other_exp(&loc), &biome, true)?)
}

/// `isCardExpeditionAscended($card, $event)`
// PHP: Helpers/Conditions.php:2460
pub fn is_card_expedition_ascended(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let loc = card::location(g, c)?;
    if !STORMS.contains(&loc.as_str()) {
        return b(false);
    }
    let p = player_of(g, c)?;
    if player::is_ascended(g, p, &loc)? {
        return b(true);
    }
    if !gigantic(g, c)? {
        return b(false);
    }
    let p = player_of(g, c)?;
    b(player::is_ascended(g, p, other_exp(&loc))?)
}

/// `wasCardExpeditionAscended($card, $event)`: the raw `$event['sourceAscended'] ?? false`
// PHP: Helpers/Conditions.php:2469
pub fn was_card_expedition_ascended(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    Ok(php::coalesce(ev(event, "sourceAscended"), &json!(false)).clone())
}

/// `isNotCardExpeditionAscended($card, $event)`
// PHP: Helpers/Conditions.php:2474
pub fn is_not_card_expedition_ascended(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(!php::truthy(&is_card_expedition_ascended(g, card, event, &[])?))
}

/// `countSourceAscended($card, $event)`: an int
// PHP: Helpers/Conditions.php:2479
pub fn count_source_ascended(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Ok(json!(count_source_ascended_n(g, card_req(card)?)?))
}

/// `countSourceAscended($card, $event)` as a number (SpecialEffect)
// PHP: Helpers/Conditions.php:2479
pub fn count_source_ascended_n(g: &mut Game, c: CardId) -> R<i64> {
    let p = player_of(g, c)?;
    let l = player::is_ascended(g, p, STORM_LEFT)? as i64;
    let p = player_of(g, c)?;
    let r = player::is_ascended(g, p, STORM_RIGHT)? as i64;
    Ok(l + r)
}

/// `hasNoSourcePlayerAscended($card, $event)`
// PHP: Helpers/Conditions.php:2484
pub fn has_no_source_player_ascended(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(count_source_ascended_n(g, card_req(card)?)? == 0)
}

/// `hasSourcePlayerAscended($card, $event)`
// PHP: Helpers/Conditions.php:2489
pub fn has_source_player_ascended(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(count_source_ascended_n(g, card_req(card)?)? > 0)
}

/// `hasSourcePlayerAllAscended($card, $event)`
// PHP: Helpers/Conditions.php:2494
pub fn has_source_player_all_ascended(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    b(count_source_ascended_n(g, card_req(card)?)? == 2)
}

/// `isSupportEffect($card, $event)`
// PHP: Helpers/Conditions.php:2499
pub fn is_support_effect(_g: &mut Game, _card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    b(php::loose_eq(php::coalesce(ev(event, "isSupport"), &json!(false)), &json!(true)))
}

/// `hasAtLeastOneCardWithNoSupportInReserve($card, $event)`
// PHP: Helpers/Conditions.php:2504
pub fn has_at_least_one_card_with_no_support_in_reserve(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    for c in player::get_reserve_cards(g, p)? {
        if php::empty(&card::get(g, c, "effectSupport")?) {
            return b(true);
        }
    }
    b(false)
}

/// `isTargetForestLessOrEqualToSource($card, $event)`
// PHP: Helpers/Conditions.php:2517
pub fn is_target_forest_less_or_equal_to_source(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::isset(event, "cardId") {
        return b(false);
    }
    let target = cards::get(g, ev(event, "cardId"))?;
    let tf = php::intval(php::coalesce(php::get(&card::get_biomes(g, target, true, false)?, "forest"), &json!(0)));
    let sf = php::intval(php::coalesce(php::get(&card::get_biomes(g, card_req(card)?, true, false)?, "forest"), &json!(0)));
    b(tf <= sf)
}

/// `countMonoVisibleRegions($card, $event, $n = 1, $op = 'GTE')`
// PHP: Helpers/Conditions.php:2528
pub fn count_mono_visible_regions(g: &mut Game, _card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let n = opt(args, 0, json!(1));
    let op = opt(args, 1, json!("GTE"));
    let visible = get_regions_info(g)?;
    let mut count = 0;
    for r in php::vals(&visible) {
        if php::count(r) == 1 {
            count += 1;
        }
    }
    let count = json!(count);
    if php::loose_eq(&op, &json!("GTE")) && php::ge(&count, &n) {
        return b(true);
    } else if php::loose_eq(&op, &json!("LTE")) && php::le(&count, &n) {
        return b(true);
    }
    b(false)
}

/// `cardInMonoRegion($card, $event)`: the region of the card's expedition token shows one biome
// PHP: Helpers/Conditions.php:2545
pub fn card_in_mono_region(g: &mut Game, card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    let visible = get_regions_info(g)?;
    let c = card_req(card)?;
    let gi = gigantic(g, c)?;
    let loc = card::location(g, c)?;
    let side = if loc == STORM_LEFT { "hero" } else { "companion" };
    if mono_region_of(g, c, side, &visible)? {
        return b(true);
    }
    if gi {
        let side = if loc == STORM_LEFT { "companion" } else { "hero" };
        if mono_region_of(g, c, side, &visible)? {
            return b(true);
        }
    }
    b(false)
}

fn mono_region_of(g: &mut Game, c: CardId, side: &str, visible: &Value) -> R<bool> {
    let p = player_of(g, c)?;
    let m = if side == "hero" { player::get_hero_token(g, p)? } else { player::get_companion_token(g, p)? };
    let token = g.meeple(m)?.location.split('-').nth(1).map(|s| s.to_string());
    let Some(token) = token else {
        return Ok(false);
    };
    Ok(php::isset(visible, &token) && php::count(php::get(visible, &token)) == 1)
}

/// `isType($card, $_event, $type)`: `$type` is one type or several separated by `|`
// PHP: Helpers/Conditions.php:2578
pub fn is_type(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let ty = req(args, 0, "isType")?;
    let types = php::str_list(&php::explode("|", &php::strval(&ty)).iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let t = type_of(g, card_req(card)?)?;
    b(php::in_array(&t, &types))
}

/// `isSubtype($card, $_event, $subType)`: one subtype or several separated by `|`
// PHP: Helpers/Conditions.php:2588
pub fn is_subtype(g: &mut Game, card: Option<CardId>, _event: &Value, args: &[Value]) -> R<Value> {
    let st = req(args, 0, "isSubtype")?;
    let sub_types = php::str_list(&php::explode("|", &php::strval(&st)).iter().map(|s| s.as_str()).collect::<Vec<_>>());
    let mine = card::get_subtypes(g, card_req(card)?)?;
    b(php::count(&php::array_intersect(&mine, &sub_types)) > 0)
}

/// `typeCheck($type, ...)` called as a condition (`$type` = the card object)
// PHP: Helpers/Conditions.php:2600
pub fn type_check(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::new("typeCheck takes a type: use type_check_v"))
}

/// `typeCheck($type, $cardType, $isToken, $additionalType = [])`
// PHP: Helpers/Conditions.php:2600
pub fn type_check_v(ty: &Value, card_type: &Value, is_token: &Value, additional_type: &Value) -> R<bool> {
    if php::in_array(ty, &json!(["permanent", "spell"])) {
        if php::loose_ne(card_type, ty) && !in_array_t(ty, additional_type)? {
            return Ok(false);
        }
        if in_array_t(ty, additional_type)? {
            return Ok(true);
        }
    }
    if php::loose_eq(ty, &json!("character")) && !php::in_array(card_type, &json!(["character", "token"])) {
        return Ok(false);
    }
    if php::loose_eq(ty, &json!("characterOnly")) && (php::loose_ne(card_type, &json!("character")) || php::truthy(is_token)) {
        return Ok(false);
    }
    if php::loose_eq(ty, &json!("token")) && !php::truthy(is_token) {
        return Ok(false);
    }
    Ok(true)
}

/// `subTypeCheck($subType, ...)` called as a condition (`$subType` = the card object)
// PHP: Helpers/Conditions.php:2623
pub fn sub_type_check(_g: &mut Game, _card: Option<CardId>, _event: &Value, _args: &[Value]) -> R<Value> {
    Err(PhpError::new("subTypeCheck takes a subtype: use sub_type_check_v"))
}

/// `subTypeCheck($subType, $subTypes, $exclude = false)`
// PHP: Helpers/Conditions.php:2623
pub fn sub_type_check_v(sub_type: &Value, sub_types: &Value, exclude: &Value) -> R<bool> {
    if !php::truthy(exclude) {
        if in_array_t(sub_type, sub_types)? {
            return Ok(true);
        }
    } else if !in_array_t(sub_type, sub_types)? {
        return Ok(true);
    }
    Ok(false)
}

/// `hasRolled($card, $event, $result = 1, $op = 'GTE')`
// PHP: Helpers/Conditions.php:2639
pub fn has_rolled(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let result = opt(args, 0, json!(1));
    let op = opt(args, 1, json!("GTE"));
    let rolls = php::coalesce(ev(event, "rolls"), &php::arr()).clone();
    for roll in php::vals(&rolls) {
        if php::loose_eq(&op, &json!("GTE")) && php::ge(roll, &result) {
            return b(true);
        } else if php::loose_eq(&op, &json!("LTE")) && php::le(roll, &result) {
            return b(true);
        }
    }
    b(false)
}

/// `selectedRoll($card, $event, $result, $op = 'GTE')`
// PHP: Helpers/Conditions.php:2652
pub fn selected_roll(_g: &mut Game, _card: Option<CardId>, event: &Value, args: &[Value]) -> R<Value> {
    let result = req(args, 0, "selectedRoll")?;
    let op = opt(args, 1, json!("GTE"));
    let roll = php::coalesce(ev(event, "selectedRoll"), &json!(-1)).clone();
    if php::loose_eq(&op, &json!("GTE")) && php::ge(&roll, &result) {
        return b(true);
    } else if php::loose_eq(&op, &json!("LTE")) && php::le(&roll, &result) {
        return b(true);
    } else if php::loose_eq(&op, &json!("EQ")) && php::loose_eq(&roll, &result) {
        return b(true);
    }
    b(false)
}

/// `boostedByOtherCard($card, $event)`
// PHP: Helpers/Conditions.php:2676
pub fn boosted_by_other_card(_g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let gain = ev(event, "gain");
    b(php::loose_ne(ev(event, "sourceId"), &json!(c)) && php::loose_eq(ev(gain, "type"), &json!("boost")) && php::loose_eq(ev(gain, "cardId"), &json!(c)))
}

/// `isCharacterBoosted($card, $event)`
// PHP: Helpers/Conditions.php:2684
pub fn is_character_boosted(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let gain = ev(event, "gain");
    if php::loose_ne(ev(event, "pId"), &pidv(g, c)?) || php::loose_ne(ev(gain, "type"), &json!("boost")) {
        return b(false);
    }
    let x = cards::get(g, ev(gain, "cardId"))?;
    b(php::loose_eq(&pidv(g, x)?, &pidv(g, c)?))
}

/// `GainFirstBoost($card, $event)`
// PHP: Helpers/Conditions.php:2691
pub fn gain_first_boost(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let c = card_req(card)?;
    let gain = ev(event, "gain");
    if php::loose_ne(ev(event, "pId"), &pidv(g, c)?) || php::loose_ne(ev(gain, "type"), &json!("boost")) {
        return b(false);
    }
    let x = cards::get(g, ev(gain, "cardId"))?;
    if php::loose_ne(&pidv(g, x)?, &pidv(g, c)?) {
        return b(false);
    }
    b(php::loose_eq(php::coalesce(ev(event, "initialBoost"), &json!(-1)), &json!(0)))
}

/// `isControlledCharacterGain($card, $event)`: the gained card as the event card
// PHP: Helpers/Conditions.php:2699
pub fn is_controlled_character_gain(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let mut event = event.clone();
    let gc = ev(ev(&event, "gain"), "cardId").clone();
    php::set(&mut event, "cardId", gc);
    if !php::truthy(&has_same_owner(g, card, &event, &[])?) {
        return b(false);
    }
    b(php::truthy(&is_character_from_target(g, card, &event, &[])?))
}

/// `isCharacterBoostedAndUntap($card, $event)`
// PHP: Helpers/Conditions.php:2705
pub fn is_character_boosted_and_untap(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    if !php::isset(event, "gain") {
        return b(false);
    }
    let gain = ev(event, "gain");
    let gained = cards::get(g, ev(gain, "cardId"))?;
    let c = card_req(card)?;
    b(!card::is_tapped(g, c)?
        && php::loose_eq(ev(gain, "type"), &json!("boost"))
        && php::loose_eq(&pidv(g, gained)?, &pidv(g, c)?)
        && is_storm(&locv(g, gained)?))
}

/// `isRollEqualCostHand($card, $event)`: a revealed card costs the selected roll
// PHP: Helpers/Conditions.php:2717
pub fn is_roll_equal_cost_hand(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let p = player_of(g, card_req(card)?)?;
    let selected = php::coalesce(ev(event, "selectedRoll"), &json!(0)).clone();
    for d in cards::get_in_location(g, &format!("reveal-{}", p), None)? {
        if php::loose_eq(&selected, &card::get(g, d, "costHand")?) {
            return b(true);
        }
    }
    b(false)
}

/// `isNonTokenBoostedAndUntap($card, $event)`
// PHP: Helpers/Conditions.php:2732
pub fn is_non_token_boosted_and_untap(g: &mut Game, card: Option<CardId>, event: &Value, _args: &[Value]) -> R<Value> {
    let gain = ev(event, "gain");
    let x = cards::get(g, ev(gain, "cardId"))?;
    let c = card_req(card)?;
    b(!card::is_tapped(g, c)?
        && php::loose_eq(ev(gain, "type"), &json!("boost"))
        && is_storm(&locv(g, x)?)
        && php::loose_eq(&type_of(g, x)?, &json!("character"))
        && !card::is_token(g, x)?
        && php::loose_eq(&pidv(g, x)?, &pidv(g, c)?))
}
