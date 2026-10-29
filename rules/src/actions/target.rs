//! Actions/Target.php: the player picks cards matching the filters (type, location, costs,
//! biomes, statuses...), pays their Tough, and each pick gets its own copy of `effect` (bound with
//! `updateCardId`) pushed as a parallel child, or a single copy with all the ids when `allIds`.
//! `stTarget` picks automatically when the choice is forced.

use crate::actions::discard_do::{check_selection, effect_desc, has_one_effect, has_token_v, in_arr};
use crate::core::node;
use crate::framework::ST_TARGET;
use crate::game::{CardId, Game, PId};
use crate::helpers::{collection, conditions, utils};
use crate::managers::{cards, players};
use crate::models::action::{get_arg, get_ctx_arg, get_source, get_source_id, param_req, push_parallel_child, resolve_action, update_card_id, ActionClass, Ctx};
use crate::models::{card, player};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};

pub struct Target;

const STORMS: [&str; 2] = ["stormLeft", "stormRight"];
const INFTY: i64 = 1000;

fn player_req(p: Option<PId>, method: &str) -> R<PId> {
    p.ok_or_else(|| PhpError::new(format!("Error: Call to a member function {}() on null", method)))
}

fn opposite(loc: &str) -> &'static str {
    if loc == "stormLeft" {
        "stormRight"
    } else {
        "stormLeft"
    }
}

/// `preg_match('/discard([+-]?\d+)/', $s, $m)` → `(int) $m[1]`, 0 when no match
fn discard_offset(s: &str) -> i64 {
    let mut from = 0;
    while let Some(i) = s[from..].find("discard") {
        let rest = &s[from + i + "discard".len()..];
        let b = rest.as_bytes();
        let mut j = 0;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > start {
            return php::intval(&json!(&rest[..j]));
        }
        from += i + 1;
    }
    0
}

/// `isCompareTargetBiomeSpecActive($spec)`
// PHP: Actions/Target.php:622
fn is_compare_target_biome_spec_active(spec: &Value) -> bool {
    php::is_arr(spec)
        && php::isset(spec, "biome")
        && php::isset(spec, "op")
        && php::isset(spec, "source")
        && php::strict_eq(php::get(spec, "op"), &json!("lte"))
        && (php::strict_eq(php::get(spec, "source"), &json!("source")) || php::strict_eq(php::get(spec, "source"), &json!("cardId")))
}

/// `resolveCompareTargetBiomeFilter()`: `(biome, cap)` or null
// PHP: Actions/Target.php:633
fn resolve_compare_target_biome_filter(a: &Target, g: &mut Game, ctx: Ctx) -> R<Option<(String, i64)>> {
    let spec = get_arg(a, g, ctx, "compareTargetBiome")?;
    if spec.is_null() {
        return Ok(None);
    }
    if !php::is_arr(&spec) {
        return Err(PhpError::new("compareTargetBiome must be null or an array"));
    }
    if !php::isset(&spec, "biome") || !php::isset(&spec, "op") {
        return Err(PhpError::new("compareTargetBiome requires keys biome and op (op must be lte)"));
    }
    if !php::key_exists(&spec, "source") {
        return Err(PhpError::new("compareTargetBiome requires key source (use source or cardId)"));
    }
    if !php::strict_eq(php::get(&spec, "op"), &json!("lte")) {
        return Err(PhpError::new("compareTargetBiome op must be lte"));
    }
    let biome = php::get(&spec, "biome").clone();
    if !php::in_array_strict(&biome, &json!(["forest", "mountain", "ocean"])) {
        return Err(PhpError::new("compareTargetBiome biome must be one of FOREST, MOUNTAIN, OCEAN"));
    }
    let ref_source = php::get(&spec, "source").clone();
    if !php::in_array_strict(&ref_source, &json!(["source", "cardId"])) {
        return Err(PhpError::new("compareTargetBiome source must be source or cardId"));
    }
    let ref_card = if php::strict_eq(&ref_source, &json!("source")) {
        if get_source_id(g, ctx)?.is_null() {
            return Err(PhpError::new("compareTargetBiome with source source requires ctx source id"));
        }
        let Some(c) = get_source(g, ctx)? else {
            return Err(PhpError::new("compareTargetBiome with source source requires a Card model at sourceId"));
        };
        if !php::in_array_strict(&card::get(g, c, "type")?, &json!(["character", "token"])) {
            return Err(PhpError::new("compareTargetBiome with source source requires source card type CHARACTER or TOKEN"));
        }
        c
    } else {
        let ctx_card_id = get_ctx_arg(g, ctx, "cardId");
        if ctx_card_id.is_null() {
            return Err(PhpError::new("compareTargetBiome with source cardId requires ctx cardId"));
        }
        match cards::get_raw(g, &ctx_card_id, true)? {
            cards::Got::One(c) => c,
            cards::Got::Many(_) => return Err(PhpError::new("compareTargetBiome with source cardId requires a valid Card at ctx cardId")),
        }
    };
    let ref_biomes = card::get_biomes(g, ref_card, true, false)?;
    let key = php::key_of(&biome);
    let cap = php::intval(php::coalesce(php::get(&ref_biomes, &key), &json!(0)));
    Ok(Some((key, cap)))
}

/// `getCompareBiomeDescriptionLabel($biome)`
// PHP: Actions/Target.php:708
fn get_compare_biome_description_label(biome: &Value) -> Value {
    match php::strval(biome).as_str() {
        "forest" => json!("Forest"),
        "mountain" => json!("Mountain"),
        "ocean" => json!("Ocean"),
        _ => json!(php::strval(biome)),
    }
}

/// `resolveSingleSubType()`: the subtype when the filter is a single one
// PHP: Actions/Target.php:725
fn resolve_single_sub_type(a: &Target, g: &Game, ctx: Ctx) -> R<Value> {
    let sub_type = get_arg(a, g, ctx, "subType")?;
    if php::strict_eq(&sub_type, &json!("disabled")) || sub_type.is_null() {
        return Ok(Value::Null);
    }
    if php::is_arr(&sub_type) {
        return Ok(if php::count(&sub_type) == 1 { php::geti(&sub_type, 0).clone() } else { Value::Null });
    }
    Ok(sub_type)
}

/// `canTargetSourceAsBoostSwapPair($sourceId, $previousTargetId)`
// PHP: Actions/Target.php:742
fn can_target_source_as_boost_swap_pair(a: &Target, g: &Game, ctx: Ctx, source_id: &Value, previous_target_id: &Value) -> R<bool> {
    let effect = get_arg(a, g, ctx, "effect")?;
    if !php::is_arr(&effect) {
        return Ok(false);
    }
    let action = php::get(&effect, "action").clone();
    let is_boost_swap = php::strict_eq(&action, &json!("BoostExchange"))
        || (php::strict_eq(&action, &json!("SpecialEffect")) && php::strict_eq(php::get(php::get(&effect, "args"), "effect"), &json!("swapBoostsWithSource")));
    if !is_boost_swap {
        return Ok(false);
    }
    if is_resolved_target_card_id(previous_target_id) && php::loose_ne(previous_target_id, source_id) {
        return Ok(true);
    }
    let mut parent = ctx.and_then(|n| node::parent(g, n));
    while let Some(p) = parent {
        let args = php::coalesce(&node::get_args(g, p), &php::arr()).clone();
        let cid = php::get(&args, "cardId").clone();
        if is_resolved_target_card_id(&cid) && php::loose_ne(&cid, source_id) {
            return Ok(true);
        }
        parent = node::parent(g, p);
    }
    Ok(false)
}

/// `isResolvedTargetCardId($cid)`
// PHP: Actions/Target.php:772
fn is_resolved_target_card_id(cid: &Value) -> bool {
    !cid.is_null() && !php::strict_eq(cid, &json!("EFFECT")) && !php::strict_eq(cid, &json!("me")) && !php::is_arr(cid) && php::is_numeric(cid)
}

/// `getSubtypeTargetLabel($defaultLabel)`
// PHP: Actions/Target.php:780
fn get_subtype_target_label(a: &Target, g: &Game, ctx: Ctx, default: &str) -> R<Value> {
    let sub_type = resolve_single_sub_type(a, g, ctx)?;
    if sub_type.is_null() {
        return Ok(json!(default));
    }
    Ok(match php::key_of(&sub_type).as_str() {
        "feat" => json!("feat"),
        "landmark" => json!("landmark"),
        _ => json!(default),
    })
}

/// `getTargetableCards($player, $checkTough)`
// PHP: Actions/Target.php:213
pub(crate) fn get_targetable_cards(a: &Target, g: &mut Game, ctx: Ctx, player: Option<PId>, check_tough: bool) -> R<Vec<CardId>> {
    // Who is the target ?
    let pid = player_req(player, "getId")?;
    let mut pids = json!(players::get_all(g));
    let target_player = get_arg(a, g, ctx, "targetPlayer")?;
    if php::loose_eq(&target_player, &json!("me")) {
        pids = json!([pid]);
    }
    if php::loose_eq(&target_player, &json!("opponent")) {
        pids = php::array_diff(&pids, &json!([pid]));
    }

    let mut max_hand_cost = get_arg(a, g, ctx, "maxHandCost")?;
    let is_int = max_hand_cost.is_i64();
    if !is_int && php::loose_eq(&max_hand_cost, &json!("controlledCharacter")) {
        max_hand_cost = json!(player::get_played_cards(g, pid, &json!("character"))?.len() + player::get_played_cards(g, pid, &json!("token"))?.len());
    } else if !is_int && php::loose_eq(&max_hand_cost, &json!("maxCharacter")) {
        let mut m = json!(-1);
        for c in player::get_played_cards(g, pid, &json!("character"))? {
            m = php::max_v(m, card::get(g, c, "costHand")?);
        }
        max_hand_cost = m;
    } else if !is_int && php::loose_eq(&max_hand_cost, &json!("sourceCounter2")) {
        let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getExtraDatas() on null"))?;
        let counter = php::coalesce(php::get(&card::get(g, s, "extraDatas")?, "counter"), &json!(0)).clone();
        max_hand_cost = crate::actions::discard::num_add(&counter, &json!(2))?;
    } else if !is_int && php::strval(&max_hand_cost).starts_with("discard") {
        let previous_card_id = get_ctx_arg(g, ctx, "cardId");
        let offset = discard_offset(&php::strval(&max_hand_cost));
        if previous_card_id.is_null() {
            max_hand_cost = json!(offset);
        } else {
            let pc = cards::get(g, &previous_card_id)?;
            max_hand_cost = crate::actions::discard::num_add(&json!(offset), &card::get(g, pc, "costHand")?)?;
        }
    }

    // What cards ?
    let target_type = get_arg(a, g, ctx, "targetType")?;
    let mut target_location = get_arg(a, g, ctx, "targetLocation")?;
    let source_gigantic = |g: &mut Game| -> R<bool> {
        let sid = get_source_id(g, ctx)?;
        let s = cards::get(g, &sid)?;
        Ok(php::truthy(&card::is_gigantic(g, s, false)?))
    };
    if php::loose_eq(&target_location, &json!(["source"])) {
        let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
        target_location = json!([card::location(g, s)?]);
        if source_gigantic(g)? {
            target_location = json!(STORMS);
        }
    } else if php::loose_eq(&target_location, &json!(["oppositeSource"])) {
        let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
        target_location = json!([if card::location(g, s)? == "stormRight" { "stormLeft" } else { "stormRight" }]);
        if source_gigantic(g)? {
            target_location = json!(STORMS);
        }
    } else if php::loose_eq(&target_location, &json!(["opponentSource"])) {
        let s = get_source(g, ctx)?.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
        target_location = json!([card::location(g, s)?]);
        if source_gigantic(g)? {
            target_location = json!(STORMS);
        }
        pids = php::array_diff(&pids, &json!([pid]));
    }
    let exclude_tokens = get_arg(a, g, ctx, "excludeToken")?;
    let only_tokens = get_arg(a, g, ctx, "onlyToken")?;
    let ascended_only = get_arg(a, g, ctx, "ascendedOnly")?;

    let arg_cards = get_arg(a, g, ctx, "cards")?;
    let mut cs = vec![];
    if !php::empty(&arg_cards) {
        for c in cards::get_many(g, &arg_cards, true)? {
            let loc_ok = in_arr(&json!(card::location(g, c)?), &target_location)? || (php::in_array(&target_location, &json!(STORMS)) && php::truthy(&card::is_gigantic(g, c, false)?));
            if loc_ok && (in_arr(&card::get(g, c, "type")?, &target_type)? || php::count(&php::array_intersect(&target_type, &card::get_additional_type(g, c)?)) > 0) {
                cs.push(c);
            }
        }
    } else {
        for c in cards::get_filtered(g, &pids, &Value::Null, &target_type, true)? {
            let loc = json!(card::location(g, c)?);
            let loc_ok = in_arr(&loc, &target_location)?
                || ((in_arr(&json!("stormLeft"), &target_location)? || in_arr(&json!("stormRight"), &target_location)?)
                    && php::in_list(&loc, &STORMS)
                    && php::truthy(&card::is_gigantic(g, c, false)?))
                || (php::loose_eq(&card::get(g, c, "type")?, &json!("hero")) && in_arr(&json!("hero"), &target_type)?);
            let keep = loc_ok
                // Token exclusion
                && (php::strict_eq(&exclude_tokens, &json!(false)) || (php::strict_eq(&exclude_tokens, &json!(true)) && !card::is_token(g, c)?))
                && (php::loose_eq(&only_tokens, &json!(false)) || (php::strict_eq(&only_tokens, &json!(true)) && card::is_token(g, c)?));
            if keep {
                cs.push(c);
            }
        }
    }

    let exclude_self = get_arg(a, g, ctx, "excludeSelf")?;
    let source_id = get_source_id(g, ctx)?;
    let exclude_previous_target = get_arg(a, g, ctx, "excludePreviousTarget")?;
    let previous_target_id = if php::truthy(&exclude_previous_target) { get_ctx_arg(g, ctx, "cardId") } else { Value::Null };
    let sub_type = get_arg(a, g, ctx, "subType")?;
    let not_sub_types = get_arg(a, g, ctx, "notSubTypes")?;
    let expedition_attributes = get_arg(a, g, ctx, "expeditionAttributes")?;
    let filtered_biomes = players::filter_biomes(g, &expedition_attributes)?;
    let excluded_biomes = if php::truthy(&get_arg(a, g, ctx, "excludeBiomes")?) { players::exclude_biomes(g, &expedition_attributes)? } else { Value::Null };
    let is_tapped = php::truthy(&get_arg(a, g, ctx, "isTapped")?);
    let is_not_tapped = php::truthy(&get_arg(a, g, ctx, "isNotTapped")?);
    let max_statistic = get_arg(a, g, ctx, "maxStatistic")?;
    let augment_only = php::truthy(&get_arg(a, g, ctx, "augmentOnly")?);
    let mono_biome = php::truthy(&get_arg(a, g, ctx, "monoBiome")?);
    // Duster
    let max_base_cost = get_arg(a, g, ctx, "maxBaseCost")?;
    let min_base_cost = get_arg(a, g, ctx, "minBaseCost")?;
    // Eole
    let no_boost_if_boosted = php::truthy(&get_arg(a, g, ctx, "noBoostIfBoosted")?);
    let compare_filter = resolve_compare_target_biome_filter(a, g, ctx)?;

    // Which criteria ?
    let allow_self_as_second_boost_swap = can_target_source_as_boost_swap_pair(a, g, ctx, &source_id, &previous_target_id)?;
    let mut out = vec![];
    for c in cs {
        if target_filter(a, g, ctx, c, pid, &FilterArgs {
            exclude_self: php::truthy(&exclude_self),
            exclude_previous_target: php::truthy(&exclude_previous_target),
            previous_target_id: &previous_target_id,
            source_id: &source_id,
            allow_self_as_second_boost_swap,
            max_hand_cost: &max_hand_cost,
            sub_type: &sub_type,
            not_sub_types: &not_sub_types,
            check_tough,
            filtered_biomes: &filtered_biomes,
            excluded_biomes: &excluded_biomes,
            is_tapped,
            is_not_tapped,
            max_statistic: &max_statistic,
            augment_only,
            ascended_only: php::truthy(&ascended_only),
            mono_biome,
            max_base_cost: &max_base_cost,
            min_base_cost: &min_base_cost,
            compare_filter: &compare_filter,
            no_boost_if_boosted,
        })? {
            out.push(c);
        }
    }
    Ok(out)
}

/// The variables captured by the filter closure of `getTargetableCards`
struct FilterArgs<'a> {
    exclude_self: bool,
    exclude_previous_target: bool,
    previous_target_id: &'a Value,
    source_id: &'a Value,
    allow_self_as_second_boost_swap: bool,
    max_hand_cost: &'a Value,
    sub_type: &'a Value,
    not_sub_types: &'a Value,
    check_tough: bool,
    filtered_biomes: &'a Value,
    excluded_biomes: &'a Value,
    is_tapped: bool,
    is_not_tapped: bool,
    max_statistic: &'a Value,
    augment_only: bool,
    ascended_only: bool,
    mono_biome: bool,
    max_base_cost: &'a Value,
    min_base_cost: &'a Value,
    compare_filter: &'a Option<(String, i64)>,
    no_boost_if_boosted: bool,
}

/// The filter closure of `getTargetableCards`
// PHP: Actions/Target.php:321
fn target_filter(a: &Target, g: &mut Game, ctx: Ctx, c: CardId, pid: PId, f: &FilterArgs) -> R<bool> {
    let cid = json!(c);
    if f.exclude_self && php::loose_eq(&cid, f.source_id) && !f.allow_self_as_second_boost_swap {
        return Ok(false);
    }
    if f.exclude_previous_target && php::truthy(f.previous_target_id) && php::loose_eq(&cid, f.previous_target_id) {
        return Ok(false);
    }
    let loc = card::location(g, c)?;
    let in_storms = STORMS.contains(&loc.as_str());
    let other_location = if in_storms { json!(opposite(&loc)) } else { Value::Null };
    let cpid = card::pid(g, c)?.to_string();
    let locv = json!(loc);
    // if we need to filter by location & attributes
    if f.excluded_biomes.is_null() && in_storms && !in_arr(&locv, php::get(f.filtered_biomes, &cpid))? && !php::truthy(&card::is_gigantic(g, c, false)?) {
        return Ok(false);
    }
    if f.excluded_biomes.is_null()
        && php::truthy(&card::is_gigantic(g, c, false)?)
        && in_storms
        && !in_arr(&other_location, php::get(f.filtered_biomes, &cpid))?
        && !in_arr(&locv, php::get(f.filtered_biomes, &cpid))?
    {
        return Ok(false);
    }
    if !f.excluded_biomes.is_null() && in_storms && !in_arr(&locv, php::coalesce(php::get(f.excluded_biomes, &cpid), &php::arr()))? && php::truthy(&card::is_gigantic(g, c, false)?) {
        return Ok(false);
    }
    if f.is_tapped && !card::is_tapped(g, c)? {
        return Ok(false);
    }
    if f.is_not_tapped && card::is_tapped(g, c)? {
        return Ok(false);
    }
    // Only card with a boost or a counter can be augmented
    if f.augment_only && !card::has_counters(g, c)? {
        return Ok(false);
    }
    if f.ascended_only && !card::is_in_ascended(g, c)? {
        return Ok(false);
    }
    if f.mono_biome && !php::truthy(&conditions::card_in_mono_region(g, Some(c), &php::arr(), &[])?) {
        return Ok(false);
    }
    if f.no_boost_if_boosted && card::count_token(g, c, "boost")? > 0 {
        return Ok(false);
    }

    let hand_cost = card::get(g, c, "costHand")?;
    let reserve_cost = card::get(g, c, "costReserve")?;
    let statuses = get_arg(a, g, ctx, "statuses")?;
    let excluded_statuses = get_arg(a, g, ctx, "excludedStatuses")?;
    let effects = get_arg(a, g, ctx, "hasEffects")?;
    if php::loose_ne(&effects, &json!("disabled")) && !has_one_effect(g, c, &effects)? {
        return Ok(false);
    }

    let subtypes = card::get_subtypes(g, c)?;
    // `A && B || C`: `(A && B) || C`
    if (php::loose_ne(f.sub_type, &json!("disabled")) && (!php::is_arr(f.sub_type) && !in_arr(f.sub_type, &subtypes)?))
        || (php::is_arr(f.sub_type) && php::count(&php::array_intersect(f.sub_type, &subtypes)) == 0)
    {
        return Ok(false);
    }

    if php::is_arr(f.not_sub_types) && php::count(f.not_sub_types) > 0 {
        let candidate = card::get_subtypes(g, c)?;
        for st in php::vals(&candidate) {
            // "non-Animal character" means the target cannot have subtype Animal as any of its subtype
            if php::in_array(st, f.not_sub_types) {
                return Ok(false);
            }
        }
    }

    if f.check_tough && card::pid(g, c)? != pid && card::get_tough(g, c)? > player::get_mana(g, pid)? as i64 {
        return Ok(false);
    }

    let biomes = card::get_biomes(g, c, true, false)?;
    for v in php::vals(&biomes) {
        if php::gt(v, f.max_statistic) {
            return Ok(false);
        }
    }

    if let Some((biome, cap)) = f.compare_filter {
        let target_val = php::intval(php::coalesce(php::get(&biomes, biome), &json!(0)));
        if target_val > *cap {
            return Ok(false);
        }
    }

    for s in php::vals(&excluded_statuses) {
        if has_token_v(g, c, s)? {
            return Ok(false);
        }
    }

    let fleeting = card::has_token(g, c, "fleeting")?;
    let cost_check = php::le(&get_arg(a, g, ctx, "minHandCost")?, &hand_cost)
        && php::le(&hand_cost, f.max_hand_cost)
        && php::le(&get_arg(a, g, ctx, "minReserveCost")?, &reserve_cost)
        && php::le(&reserve_cost, &get_arg(a, g, ctx, "maxReserveCost")?)
        && ((fleeting && php::le(&reserve_cost, f.max_base_cost)) || (!card::has_token(g, c, "fleeting")? && php::le(&hand_cost, f.max_base_cost)))
        && ((card::has_token(g, c, "fleeting")? && php::ge(&reserve_cost, f.min_base_cost)) || (!card::has_token(g, c, "fleeting")? && php::ge(&hand_cost, f.min_base_cost)));

    if php::loose_eq(&statuses, &json!("disabled")) || php::loose_eq(&card::get(g, c, "type")?, &json!("permanent")) {
        Ok(cost_check)
    } else {
        Ok(cost_check && has_token_v(g, c, &statuses)?)
    }
}

/// `getTargetCosts($player)`: card id → Tough to pay, for the opponent's cards
// PHP: Actions/Target.php:440
fn get_target_costs(a: &Target, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<Value> {
    let cs = get_targetable_cards(a, g, ctx, player, false)?;
    let mut costs = php::arr();
    if php::loose_eq(&get_arg(a, g, ctx, "ignoreTough")?, &json!(true)) {
        return Ok(costs);
    }
    let pid = player_req(player, "getId")?;
    for c in cs {
        if card::pid(g, c)? == pid {
            continue;
        }
        let extra = get_additional_target_cost(g, c)?;
        if extra > 0 {
            php::set(&mut costs, &c.to_string(), json!(extra));
        }
    }
    Ok(costs)
}

/// `getAdditionalTargetCost($card)`: Tough, plus the completed Feats' reserve-character Tough
// PHP: Actions/Target.php:463
fn get_additional_target_cost(g: &mut Game, c: CardId) -> R<i64> {
    let mut extra = card::get_tough(g, c)?.max(0);
    if card::location(g, c)? == "reserve" {
        let at = card::get_additional_type(g, c)?;
        let at = if php::is_arr(&at) {
            at
        } else if at.is_null() {
            php::arr()
        } else {
            json!([at])
        };
        if php::loose_eq(&card::get(g, c, "type")?, &json!("character")) || php::in_array_strict(&json!("character"), &at) {
            let owner = player_req(card::get_player(g, c)?, "getCompletedFeatReserveCharacterTough")?;
            extra += player::get_completed_feat_reserve_character_tough(g, owner)?;
        }
    }
    Ok(extra)
}

/// `argsTarget()`
// PHP: Actions/Target.php:478
fn args_target(a: &Target, g: &mut Game, ctx: Ctx) -> R<Value> {
    let player = players::get_active(g);
    let cs = get_targetable_cards(a, g, ctx, player, false)?;
    Ok(json!({
        "n": get_arg(a, g, ctx, "n")?,
        "cardIds": collection::ids(&cs),
        "upTo": get_arg(a, g, ctx, "upTo")?,
        "description": a.get_description(g, ctx)?,
        "totalCost": get_arg(a, g, ctx, "totalCost")?,
        "totalMountain": get_arg(a, g, ctx, "totalMountain")?,
        "targetCosts": get_target_costs(a, g, ctx, player)?,
        "manaOrbs": php::loose_eq(&get_arg(a, g, ctx, "targetLocation")?, &json!(["mana"])),
    }))
}

/// `actTarget($cardIds)`
// PHP: Actions/Target.php:503
fn act_target(a: &Target, g: &mut Game, ctx: Ctx, card_ids: &Value) -> R<Value> {
    let p = players::get_active(g);
    let args = args_target(a, g, ctx)?;
    let mut total_cost = get_arg(a, g, ctx, "totalCost")?;
    let mut total_mountain = get_arg(a, g, ctx, "totalMountain")?;
    check_selection(card_ids, &args)?;

    // Tough
    let mut additional_cost = 0i64;
    for cid in php::vals(card_ids) {
        additional_cost += php::intval(php::coalesce(php::get(php::get(&args, "targetCosts"), &php::key_of(cid)), &json!(0)));
    }
    let pid = player_req(p, "getMana")?;
    if additional_cost > player::get_mana(g, pid)? as i64 {
        return Err(PhpError::new("You cannot pay the additional cost (Tough effect)"));
    }
    player::pay_mana(g, pid, additional_cost)?;
    let (exclude_increase_card, increase_other) = crate::models::player::has_increase_all_other_characters_biomes_highest(g, pid)?;

    let cs = cards::get_many(g, card_ids, true)?;
    let source = get_source(g, ctx)?;
    if php::strict_eq(&get_arg(a, g, ctx, "allIds")?, &json!(true)) {
        let effect = get_arg(a, g, ctx, "effect")?;
        if !effect.is_null() {
            let sid = get_source_id(g, ctx)?;
            let nd = update_card_id(g, &effect, card_ids, &json!(""), &sid, &json!(pid), false)?;
            push_parallel_child(g, ctx, &nd)?;
        }
        let cs = cards::get_many(g, card_ids, true)?;
        g.notify("targetCards", json!({"player_id": pid, "cardIds": cs, "additionalCost": additional_cost}));
    } else {
        for c in cs {
            let card_from = card::location(g, c)?;

            // Select untapped card in mana => untap another card if any
            if php::truthy(php::get(&args, "manaOrbs")) && !card::is_tapped(g, c)? {
                let tapped = player::get_mana_cards(g, pid, Some(true))?;
                if let Some(c2) = collection::first(&tapped) {
                    card::set_property(g, c2, "tapped", json!(false), true)?;
                }
            }

            let effect = get_arg(a, g, ctx, "effect")?;
            if !effect.is_null() {
                let sid = get_source_id(g, ctx)?;
                let owner = player_req(card::get_player(g, c)?, "getId")?;
                let mut nd = update_card_id(g, &effect, &json!(c), &json!(card_from), &sid, &json!(owner), false)?;
                if STORMS.contains(&card_from.as_str()) {
                    // in case of invoking token combined with a sacrifice
                    nd = utils::update_tree(&nd, &json!(["source"]), &json!([card_from]), &["targetLocation"]);
                    let s = source.ok_or_else(|| PhpError::new("Error: Call to a member function getLocation() on null"))?;
                    let opp = if card::location(g, s)? == "stormLeft" { "stormRight" } else { "stormLeft" };
                    nd = utils::update_tree(&nd, &json!(["oppositeSource"]), &json!([opp]), &["targetLocation"]);
                    let discard_source = format!("{}-{}", card_from, card::pid(g, c)?);
                    nd = utils::update_tree(&nd, &json!(["discardedSource"]), &json!([discard_source]), &["targetLocation"]);
                    // if the discarded card is the source, need to update the location
                    let uid = card::get(g, c, "uid")?;
                    if php::loose_eq(&json!(c), &sid) && php::in_list(&uid, &["ALT_CORE_B_OR_06_U_3184", "ALT_COREKS_B_AX_07_U_5235", "ALT_CORE_B_OR_06_U_3184", "ALT_COREKS_B_OR_17_U_1949"]) {
                        nd = utils::update_tree(&nd, &json!(["initialSource"]), &json!([discard_source]), &["targetLocation"]);
                    }
                }
                push_parallel_child(g, ctx, &nd)?;
            }
            total_cost = crate::actions::discard::num_add(&total_cost, &json!(-card::get_int(g, c, "costHand")?))?;
            let owner = player_req(card::get_player(g, c)?, "hasIncreaseBiomesHighest")?;
            let cloc = card::location(g, c)?;
            let mut gigantic_increase = false;
            if crate::models::player::has_increase_biomes_highest(g, owner, &cloc)? || (php::truthy(&card::is_gigantic(g, c, false)?) && crate::models::player::has_increase_biomes_highest(g, owner, opposite(&cloc))?) {
                gigantic_increase = true;
            }
            let mut increase_biome = gigantic_increase || increase_other;
            if php::loose_eq(&json!(exclude_increase_card), &json!(c)) {
                increase_biome = gigantic_increase;
            }
            let biomes = card::get_biomes(g, c, true, increase_biome)?;
            total_mountain = crate::actions::discard::num_add(&total_mountain, &json!(-php::intval(php::get(&biomes, "mountain"))))?;
        }

        if php::lt(&total_cost, &json!(0)) {
            return Err(PhpError::new("Total hand cost exceeds the limit of the effect"));
        }
        if php::lt(&total_mountain, &json!(0)) {
            return Err(PhpError::new("Total mountain attribute exceeds the limit of the effect"));
        }

        let cs = cards::get_many(g, card_ids, true)?;
        g.notify("targetCards", json!({"player_id": pid, "cardIds": cs, "additionalCost": additional_cost}));

        if php::loose_eq(&get_arg(a, g, ctx, "discardRemaining")?, &json!(true)) {
            let remaining = php::array_diff(&get_arg(a, g, ctx, "cards")?, card_ids);
            for cid in php::vals(&remaining) {
                cards::discard(g, cid, "discard", None)?;
            }
            let rem = cards::get_many(g, &php::array_diff(&get_arg(a, g, ctx, "cards")?, card_ids), true)?;
            g.notify("publicDiscard", json!({"player_id": pid, "cardIds": rem}));
        }
    }

    resolve_action(a, g, ctx, json!([card_ids]), json!(false))?;
    Ok(Value::Null)
}

impl ActionClass for Target {
    fn name(&self) -> &'static str {
        "Target"
    }

    // PHP: Actions/Target.php:25
    fn default_args(&self) -> Value {
        json!({
            "upTo": false,
            "targetPlayer": "all",
            "targetType": ["character", "token"],
            "targetLocation": ["stormLeft", "stormRight", "landmark"],
            "maxReserveCost": INFTY,
            "maxHandCost": INFTY,
            "minReserveCost": 0,
            "minHandCost": 0,
            "maxBaseCost": INFTY,
            "minBaseCost": 0,
            "n": 1,
            "statuses": "disabled",
            "excludedStatuses": [],
            "excludeSelf": false,
            "excludePreviousTarget": false,
            "totalCost": INFTY,
            "totalMountain": INFTY,
            "hasEffects": "disabled",
            "cards": [],
            "discardRemaining": false,
            "subType": "disabled",
            "notSubTypes": null,
            "expeditionAttributes": null,
            "excludeBiomes": false,
            "isTapped": false,
            "maxStatistic": 99,
            "augmentOnly": false,
            "effect": null,
            "allIds": false,
            "ignoreTough": false,
            "excludeToken": false,
            "onlyToken": false,
            "ascendedOnly": false,
            "monoBiome": false,
            "isNotTapped": false,
            "compareTargetBiome": null,
            "noBoostIfBoosted": false,
        })
    }

    // PHP: Actions/Target.php:20
    fn get_state(&self, _g: &mut Game, _ctx: Ctx) -> R<Value> {
        Ok(json!(ST_TARGET))
    }

    // PHP: Actions/Target.php:65
    fn get_description(&self, g: &mut Game, ctx: Ctx) -> R<Value> {
        let target_type = get_arg(self, g, ctx, "targetType")?;
        let up_to = php::truthy(php::coalesce(&get_ctx_arg(g, ctx, "upTo"), &json!(false)));
        let total_cost = get_arg(self, g, ctx, "totalCost")?;
        let total_mountain = get_arg(self, g, ctx, "totalMountain")?;
        let base_cost = get_arg(self, g, ctx, "maxBaseCost")?;
        let min_base_cost = get_arg(self, g, ctx, "minBaseCost")?;
        let infty = json!(INFTY);
        let n_infty = php::loose_eq(&get_arg(self, g, ctx, "n")?, &infty);
        let mut type_label = Value::Null;
        let count = php::count_strict(&target_type)?;
        let mut msg: String = if count == 1 && php::loose_eq(&target_type, &json!(["character"])) {
            if up_to {
                if php::loose_ne(&total_cost, &infty) {
                    "Target up to ${n} character(s) (of max hand cost of ${totalCost}) to ${effect_desc}"
                } else if php::loose_ne(&total_mountain, &infty) {
                    "Target up to ${n} character(s) (of max mountain attribute of ${totalMountain}) to ${effect_desc}"
                } else if n_infty {
                    "All valid characters ${effect_desc}"
                } else {
                    "Target up to ${n} character(s) to ${effect_desc}"
                }
            } else {
                "Target ${n} character(s) to ${effect_desc}"
            }
        } else if count == 1 && php::loose_eq(&target_type, &json!(["permanent"])) {
            type_label = get_subtype_target_label(self, g, ctx, "permanent")?;
            if up_to {
                if php::loose_ne(&total_cost, &infty) {
                    "Target up to ${n} ${type_label}(s) (of max hand cost of ${totalCost}) to ${effect_desc}"
                } else if php::loose_ne(&total_mountain, &infty) {
                    "Target up to ${n} ${type_label}(s) (of max mountain attribute of ${totalMountain}) to ${effect_desc}"
                } else {
                    "Target up to ${n} ${type_label}(s) to ${effect_desc}"
                }
            } else {
                "Target ${n} ${type_label}(s) to ${effect_desc}"
            }
        } else if count == 2 && (php::loose_eq(&target_type, &json!(["spell", "permanent"])) || php::loose_eq(&target_type, &json!(["permanent", "spell"]))) {
            if up_to {
                if php::loose_ne(&total_cost, &infty) {
                    "Target up to ${n} non-character card(s) (of max hand cost of ${totalCost}) to ${effect_desc}"
                } else if php::loose_ne(&total_mountain, &infty) {
                    "Target up to ${n} non-character card(s) (of max mountain attribute of ${totalMountain}) to ${effect_desc}"
                } else {
                    "Target up to ${n} non-character card(s) to ${effect_desc}"
                }
            } else {
                "Target ${n} non-character card(s) to ${effect_desc}"
            }
        } else if up_to {
            if php::loose_ne(&total_cost, &infty) {
                "Target up to ${n} card(s) (of max hand cost of ${totalCost}) to ${effect_desc}"
            } else if php::loose_ne(&total_mountain, &infty) {
                "Target up to ${n} card(s) (of max mountain attribute of ${totalMountain}) to ${effect_desc}"
            } else if php::loose_ne(&base_cost, &infty) {
                "Target up to ${n} card(s) (of max base cost of ${baseCost}) to ${effect_desc}"
            } else if n_infty {
                "All valid targets ${effect_desc}"
            } else {
                "Target up to ${n} card(s) to ${effect_desc}"
            }
        } else if n_infty {
            "All valid targets ${effect_desc}"
        } else if php::loose_ne(&base_cost, &infty) {
            "Target ${n} card(s) (of max base cost of ${baseCost}) to ${effect_desc}"
        } else if php::loose_ne(&min_base_cost, &json!(0)) {
            "Target ${n} card(s) (of min base cost of ${minBaseCost}) to ${effect_desc}"
        } else {
            "Target ${n} card(s) to ${effect_desc}"
        }
        .to_string();

        let compare_spec = get_arg(self, g, ctx, "compareTargetBiome")?;
        let mut i18n = vec![json!("effect_desc")];
        let mut biome_label = Value::Null;
        if is_compare_target_biome_spec_active(&compare_spec) {
            biome_label = get_compare_biome_description_label(php::get(&compare_spec, "biome"));
            if php::strict_eq(php::get(&compare_spec, "source"), &json!("cardId")) {
                msg.push_str(" (with ${biome_label} less than or equal to the chosen card's).");
            } else {
                msg.push_str(" (with ${biome_label} less than or equal to mine).");
            }
            i18n.push(json!("biome_label"));
        }

        let mut args = json!({
            "n": get_arg(self, g, ctx, "n")?,
            "effect_desc": effect_desc(g, ctx)?,
            "totalCost": total_cost,
            "totalMountain": total_mountain,
            "baseCost": base_cost,
            "minBaseCost": min_base_cost,
            "i18n": i18n.clone(),
        });
        if !type_label.is_null() {
            php::set(&mut args, "type_label", type_label);
            i18n.push(json!("type_label"));
            php::set(&mut args, "i18n", Value::Array(i18n));
        }
        if !biome_label.is_null() {
            php::set(&mut args, "biome_label", biome_label);
        }
        Ok(json!({"log": msg, "args": args}))
    }

    // PHP: Actions/Target.php:181
    fn is_doable(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        let target_cards = get_targetable_cards(self, g, ctx, player, false)?;
        let all_ids = php::truthy(&get_arg(self, g, ctx, "allIds")?);
        let up_to = php::truthy(&get_arg(self, g, ctx, "upTo")?);
        // Multi-pick: only require Tough mana for targets we must pay for.
        if all_ids && !up_to {
            let target_costs = get_target_costs(self, g, ctx, player)?;
            let free = target_cards.len() as i64 - php::count(&target_costs) as i64;
            let needed_paid = (php::intval(&get_arg(self, g, ctx, "n")?) - free).max(0);
            let mut costs: Vec<i64> = php::vals(&target_costs).into_iter().map(php::intval).collect();
            costs.sort();
            let total: i64 = costs.iter().take(needed_paid as usize).sum();
            let pid = player_req(player, "getMana")?;
            if total > player::get_mana(g, pid)? as i64 {
                return Ok(false);
            }
        }
        let n = get_arg(self, g, ctx, "n")?;
        Ok(php::truthy(&get_arg(self, g, ctx, "upTo")?)
            || (!php::truthy(&get_arg(self, g, ctx, "allIds")?) && !target_cards.is_empty())
            || (php::truthy(&get_arg(self, g, ctx, "allIds")?) && php::ge(&json!(target_cards.len()), &n)))
    }

    // PHP: Actions/Target.php:203
    fn is_optional(&self, g: &mut Game, ctx: Ctx, player: Option<PId>) -> R<bool> {
        if php::truthy(&get_arg(self, g, ctx, "upTo")?) {
            return Ok(true);
        }
        if php::loose_eq(&get_arg(self, g, ctx, "ignoreTough")?, &json!(false)) {
            let active = players::get_active(g);
            if get_targetable_cards(self, g, ctx, active, true)?.is_empty() {
                return Ok(true);
            }
        }
        if !self.is_doable(g, ctx, player)? {
            return Ok(true);
        }
        let n = ctx.ok_or_else(|| PhpError::new("Error: Call to a member function getOptional() on null"))?;
        if php::truthy(php::coalesce(&node::info(g, n, "optional"), &json!(false))) {
            return Ok(true);
        }
        let targetable = get_targetable_cards(self, g, ctx, player, false)?.len();
        let costs = get_target_costs(self, g, ctx, player)?;
        if targetable == php::count(&costs) {
            // `getTargetCosts($player) > 0`: an array is always greater than an int
            let _ = get_target_costs(self, g, ctx, player)?;
            return Ok(true);
        }
        Ok(false)
    }

    // PHP: Actions/Target.php:478
    fn args(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        Ok(Some(args_target(self, g, ctx)?))
    }

    // PHP: Actions/Target.php:495
    fn st(&self, g: &mut Game, ctx: Ctx) -> R<Option<Value>> {
        let args = args_target(self, g, ctx)?;
        if php::loose_eq(php::get(&args, "upTo"), &json!(false)) {
            let active = players::get_active(g);
            if !self.is_optional(g, ctx, active)? && php::le(&json!(php::count(php::get(&args, "cardIds"))), php::get(&args, "n")) {
                let ids = php::get(&args, "cardIds").clone();
                act_target(self, g, ctx, &ids)?;
            }
        }
        Ok(None)
    }

    fn act(&self, g: &mut Game, ctx: Ctx, method: &str, args: &Value) -> R<Value> {
        match method.to_lowercase().as_str() {
            "acttarget" => {
                let ids = param_req(args, 0, "cardIds", "actTarget")?;
                act_target(self, g, ctx, &ids)
            }
            _ => Err(PhpError::new(format!("Error: Call to undefined method ALT\\Actions\\Target::{}()", method))),
        }
    }
}
