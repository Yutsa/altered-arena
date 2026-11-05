//! Helpers/FlowConvertor.php: the effects of a Unique card built from its trigger / condition /
//! output ids. The tables (getTriggers, getConditions, getOutput) are PHP data: they are exported
//! as JSON (`rules/data/flow.min.json`, server/tools/uniques.php export); the logic is translated.

use crate::helpers::{ft, utils};
use crate::php;
use crate::{PhpError, R};
use serde_json::{json, Value};
use std::sync::OnceLock;

/// flow.min.json: `triggers`, `conditions`, `outputs` (the three tables), the id lists `TRIGGER`,
/// `CONDITION`, `OUTPUT` and `IN_PLAY` (array constants), `altArt` (Cards::getAltArt)
pub fn data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(include_str!("../../data/flow.min.json")).expect("flow.min.json"))
}

/// `FlowConvertor::getTriggers()`
pub fn get_triggers() -> &'static Value {
    php::get(data(), "triggers")
}

/// `FlowConvertor::getConditions()`
pub fn get_conditions() -> &'static Value {
    php::get(data(), "conditions")
}

/// `FlowConvertor::getOutput()`
pub fn get_output() -> &'static Value {
    php::get(data(), "outputs")
}

/// `array_merge(...)`, which throws a TypeError on an argument that is not an array
fn merge(arrays: &[&Value]) -> R<Value> {
    if let Some(a) = arrays.iter().find(|a| !php::is_arr(a)) {
        return Err(PhpError::new(format!("array_merge(): Argument must be of type array, {} given", php::type_name(a))));
    }
    Ok(php::array_merge(arrays))
}

/// `$x[$k] = $v` where `$x` must be an array (null becomes one; a string throws, as in PHP)
fn set_in(x: &mut Value, k: &str, v: Value) -> R<()> {
    if !x.is_null() && !php::is_arr(x) {
        return Err(PhpError::new(format!("Cannot access offset of type string on {}", php::type_name(x))));
    }
    php::set(x, k, v);
    Ok(())
}

/// `$x[] = $v` where `$x` must be an array (null becomes one)
fn push_in(x: &mut Value, v: Value) -> R<()> {
    if !x.is_null() && !php::is_arr(x) {
        return Err(PhpError::new(format!("[] operator not supported for {}", php::type_name(x))));
    }
    php::push(x, v);
    Ok(())
}

fn is_string_or_bool(v: &Value) -> bool {
    matches!(v, Value::String(_) | Value::Bool(_))
}

/// `$x ?? $default` for a key
fn get_or(v: &Value, k: &str, default: Value) -> Value {
    match php::get(v, k) {
        Value::Null => default,
        x => x.clone(),
    }
}

/// `FlowConvertor::constructEffect($trinity, &$properties)`
// PHP: Helpers/FlowConvertor.php:6180
pub fn construct_effect(trinity: &Value, properties: &mut Value) -> R<()> {
    let mut calculated = php::arr();

    if php::isset(trinity, "trigger") {
        compute_trigger(php::get(trinity, "trigger"), &mut calculated)?;
    }
    if php::isset(trinity, "condition") {
        compute_conditions(php::get(trinity, "condition"), &mut calculated)?;
    }
    if php::isset(trinity, "output") {
        compute_output(php::get(trinity, "output"), &mut calculated)?;
    }

    if !php::isset(&calculated, "type") {
        return Err(PhpError::new("constructEffect: no type"));
    }

    let mut key = php::strval(php::get(&calculated, "type"));
    let mut node = php::arr();
    // `$tmp` lives in the whole function (PHP keeps it from one use to the next)
    let mut tmp = Value::Null;
    if php::isset(&calculated, "noTrigger") && php::strict_eq(php::get(&calculated, "noTrigger"), &json!(true)) {
        // DynamicAttributes wll be used
        // if it exists, condition must be added at the end of the dynamic attribute
        let unless_conditions = get_or(&calculated, "conditionConditions", php::arr());
        let dynamic_attribute_conditions = get_dynamic_attribute_condition_list(trinity, &calculated, &unless_conditions)?;
        let dynamic_attribute_condition_str =
            if !php::empty(&dynamic_attribute_conditions) { format!(":{}", php::implode("|", &dynamic_attribute_conditions)) } else { String::new() };

        // if the no trigger is linked to infinite reserve effect
        if php::isset(&calculated, "type") && php::loose_eq(php::get(&calculated, "type"), &json!("effectInfinity")) {
            tmp = php::get(&calculated, "outputAttributes").clone();
            php::unset(&mut calculated, "outputAttributes");
            set_in(php::get_mut(&mut calculated, "outputAttributes"), "effectInfinity", tmp.clone())?;
        }

        if php::isset(&calculated, "conditionConditions") {
            let tc = merge(&[&get_or(&calculated, "triggerConditions", php::arr()), php::get(&calculated, "conditionConditions")])?;
            php::set(&mut calculated, "triggerConditions", tc);
            let td = merge(&[&json!([php::get(&calculated, "triggerDescription")]), &json!([php::get(&calculated, "conditionDescription")])])?;
            php::set(&mut calculated, "triggerDescription", td);
            php::unset(&mut calculated, "conditionConditions");
            php::unset(&mut calculated, "conditionDescription");
        }
        let attributes = get_or(&calculated, "outputAttributes", php::arr());
        if php::isset(&calculated, "triggerConditions") {
            for (key_attribute, attribute) in php::entries(&attributes) {
                if key_attribute == "excludeUniversalTough" {
                    set_in(properties, &key_attribute, attribute.clone())?;
                    continue;
                }
                if !php::isset(properties, &key_attribute) {
                    if is_string_or_bool(attribute) {
                        set_in(properties, &key_attribute, json!(format!("{}{}", php::strval(attribute), dynamic_attribute_condition_str)))?;
                    }
                } else {
                    if !php::is_arr(php::get(properties, &key_attribute)) {
                        tmp = json!([php::get(properties, &key_attribute)]);
                    }
                    if is_string_or_bool(attribute) {
                        push_in(&mut tmp, json!(format!("{}{}", php::strval(attribute), dynamic_attribute_condition_str)))?;
                    }
                    set_in(properties, &key_attribute, tmp.clone())?;
                }
            }
        } else {
            // no condition on the trigger
            for (key_attribute, attribute) in php::entries(&attributes) {
                if !php::isset(properties, &key_attribute) {
                    set_in(properties, &key_attribute, attribute.clone())?;
                } else {
                    if !php::is_arr(php::get(properties, &key_attribute)) {
                        tmp = json!([php::get(properties, &key_attribute)]);
                    }
                    if is_string_or_bool(attribute) {
                        push_in(&mut tmp, attribute.clone())?;
                    }
                    set_in(properties, &key_attribute, tmp.clone())?;
                }
            }
        }
    } else if key == "manInTheMazeUnique" {
        let first = php::geti(php::get(&calculated, "triggerConditions"), 0).clone();
        let output = php::get(&calculated, "output").clone();
        if php::loose_eq(&first, &json!("4+")) {
            *properties = utils::update_tree(properties, &json!("OUTPUT4"), &output, &[]);
        } else if php::loose_eq(&first, &json!("9+")) {
            *properties = utils::update_tree(properties, &json!("OUTPUT9"), &output, &[]);
        }
    } else if key != "effectPassive" && key != "effectInfinity" {
        // no natural condition check, we need to insert CheckConditions
        if php::isset(&calculated, "triggerConditions") {
            let description = json!([php::get(&calculated, "conditionDescription"), php::get(&calculated, "outputDescription")]);
            let extra = if !php::empty(php::get(&calculated, "ignoreDeck")) { json!({"ignoreDeck": true}) } else { php::arr() };
            insert_check_condition(php::get(&calculated, "triggerConditions"), &mut node, description, &extra)?;
        }
        if php::isset(&calculated, "conditionEffect") {
            add_effect_to_condition(php::get(&calculated, "conditionEffect"), &mut node);
        }
    } else if php::isset(&calculated, "trigger") {
        if !php::is_arr(php::get(&calculated, "trigger")) {
            let t = json!([php::get(&calculated, "trigger")]);
            php::set(&mut calculated, "trigger", t);
        }

        let mut template = php::arr();
        if php::isset(&calculated, "triggerConditions") {
            php::set(&mut template, "conditions", php::get(&calculated, "triggerConditions").clone());
        }
        if php::isset(&calculated, "triggerListeningConditions") {
            php::set(&mut template, "listeningConditions", php::get(&calculated, "triggerListeningConditions").clone());
        }

        if php::isset(&calculated, "oppositeOutput") {
            // if it's a "if you do " with otherwise, it cannot be a check condition as an action from the user is expected
            if php::truthy(php::get(&calculated, "ifYouDo")) {
                // put a XOR node
                php::set(
                    &mut template,
                    "output",
                    ft::xor(vec![get_or(&calculated, "conditionEffect", json!("OUTPUT")), php::get(&calculated, "oppositeOutput").clone()]),
                );
            } else {
                // management of Otherwise effect
                php::set(
                    &mut template,
                    "output",
                    ft::action(
                        "CheckCondition",
                        json!({
                            "conditions": get_or(&calculated, "conditionConditions", php::arr()),
                            "effect": get_or(&calculated, "conditionEffect", json!("OUTPUT")),
                            "oppositeEffect": php::get(&calculated, "oppositeOutput"),
                            "description": [php::get(&calculated, "conditionDescription"), php::get(&calculated, "outputDescription")],
                        }),
                        json!({}),
                    ),
                );
            }
        } else {
            if php::isset(&calculated, "conditionEffect") {
                php::set(&mut template, "output", php::get(&calculated, "conditionEffect").clone());
            } else {
                php::set(&mut template, "output", json!("OUTPUT"));
            }
            // If there is a condition on condition, we add it
            if php::isset(&calculated, "conditionConditions") {
                let c = merge(&[&get_or(&template, "conditions", php::arr()), php::get(&calculated, "conditionConditions")])?;
                php::set(&mut template, "conditions", c);
            }
        }
        if php::isset(&calculated, "conditionN") {
            php::set(&mut template, "n", php::get(&calculated, "conditionN").clone());
        }

        // Management of AfterRest effects (must happen after cleanup but the cards have been cleanup)
        if php::loose_eq(&get_or(&calculated, "afterRest", json!("")), &json!(true)) {
            let new_output = json!({"action": "SpecialEffect", "args": {"effect": "afterRest", "args": php::get(&calculated, "output")}});
            php::set(&mut calculated, "output", new_output);
        }

        for trig in php::vals(php::get(&calculated, "trigger")) {
            // add conditions + effect + output
            php::set(&mut node, &php::key_of(trig), template.clone());
        }
    }

    if php::in_array(php::get(trinity, "condition"), &json!([642, 643])) && php::isset(&calculated, "output") {
        let o = utils::tag_tree(php::get(&calculated, "output"), &json!({"pId": "owner"}), false);
        php::set(&mut calculated, "output", o);
    }

    // Commit the "if you do" cost only. Do this before substituting OUTPUT so sabotage
    // (and the otherwise) keep their own optional / upTo.
    if php::truthy(php::get(&calculated, "ifYouDo")) && php::isset(&calculated, "oppositeOutput") {
        node = utils::update_tree(&node, &json!(true), &json!(false), &["optional"]);
        node = utils::update_tree(&node, &json!(true), &json!(false), &["upTo"]);
    }

    if php::isset(&calculated, "output") {
        add_output_to_node(php::get(&calculated, "output"), &mut node);
    }
    if php::isset(&calculated, "oppositeOutput") {
        // if "opposite" is already defined we update it
        if utils::search_tree(&node, &json!("OPPOSITE")) {
            add_opposite_to_node(php::get(&calculated, "oppositeOutput"), &mut node);
        } else if key != "effectPassive" {
            // we nest the actual node in a XOR
            node = ft::xor(vec![node, php::get(&calculated, "oppositeOutput").clone()]);
        }
    }

    // Flagged outputs (see getOutput 'ignoreDeck') are sequenced effects whose deck-dependent
    // part must not block the rest of the effect: make every generated check ignore the deck.
    if !php::empty(php::get(&calculated, "ignoreDeck")) && php::is_arr(&node) {
        tag_ignore_deck(&mut node);
    }

    // Specific interaction
    if php::is_arr(&node) {
        for tr in php::keys_str(&node) {
            let eff = php::get_mut(&mut node, &tr);
            // specific case for "When an opponent draws one or more cards or does Resupply "
            if tr == "Morning" {
                if php::loose_eq(php::get(eff, "conditions"), &json!(["isOpponentDraw", "realResupply"])) {
                    set_in(eff, "conditions", json!(["isMe"]))?;
                }
                if php::loose_eq(php::get(eff, "conditions"), &json!(["isOpponentDraw"])) {
                    set_in(eff, "conditions", json!(["isMe"]))?;
                }
            }
            // Management of defect
            if php::isset(&calculated, "pId") {
                set_in(eff, "pId", php::get(&calculated, "pId").clone())?;
            }
        }
    }

    // needed for passive effects in reaction to a classical output (171 for example)
    if php::isset(&calculated, "passiveEffect") {
        if php::isset(&calculated, "output") {
            let output = php::get(&calculated, "output").clone();
            add_output_to_node(&output, php::get_mut(&mut calculated, "passiveEffect"));
        }

        if php::isset(properties, "effectPassive") {
            let p = merge(&[php::get(properties, "effectPassive"), php::get(&calculated, "passiveEffect")])?;
            set_in(properties, "effectPassive", p)?;
        } else {
            set_in(properties, "effectPassive", php::get(&calculated, "passiveEffect").clone())?;
        }
    }

    if (!php::isset(&calculated, "noTrigger") || php::strict_eq(php::get(&calculated, "noTrigger"), &json!(false)))
        && php::isset(&calculated, "outputAttributes")
    {
        *properties = merge(&[properties, php::get(&calculated, "outputAttributes")])?;
    }

    // edge cases:
    let trigger = php::get(trinity, "trigger");
    let condition = php::get(trinity, "condition");
    let output = php::get(trinity, "output");
    let is = |v: &Value, n: i64| php::loose_eq(v, &json!(n));
    if php::in_array(trigger, &json!([8, 231])) && is(condition, 190) && is(output, 44) {
        set_in(properties, "sacrificeAndNotFleetingGoToReserve", json!(true))?;
        node = php::arr();
    } else if is(trigger, 239) {
        // Edge case for When an opponent draws one or more
        // bug #140378
        set_in(php::get_mut(&mut node, "Morning"), "conditions", json!(["isMe"]))?;
        push_in(php::get_mut(php::get_mut(&mut node, "Resupply"), "conditions"), json!("realResupply"))?;
    } else if is(trigger, 250) {
        set_in(php::get_mut(&mut node, "Resupply"), "conditions", json!(["isMe"]))?;
    } else if is(output, 529) {
        // The hunger
        set_in(properties, "effectHand", ft::action("SpecialEffect", json!({"effect": "hunger"}), json!({})))?;
        key = "effectPassive".into();
        node = php::arr();
        php::set(&mut node, "Discard", json!({"conditions": ["isDiscarded::discard"], "output": gain_me_boost(json!(1))}));
        php::set(&mut node, "ChooseAssignment", json!({"conditions": ["isAfternoon", "isFromReserve", "isSupportEffect"], "output": gain_me_boost(json!(1))}));
    } else if is(trigger, 797) && php::isset(&node, "ChooseAssignment") && !php::isset(&node, "Discard") {
        let c = php::get(&node, "ChooseAssignment").clone();
        php::set(&mut node, "Discard", c);
    } else if is(trigger, 446) {
        // We need to add the special effect
        let effect = php::get(php::get(&node, "LeaveExpedition"), "output").clone();
        let o = ft::action("SpecialEffect", json!({"effect": "doEachBoost", "args": {"effect": effect}}), json!({}));
        set_in(php::get_mut(&mut node, "LeaveExpedition"), "output", o)?;
    } else if is(trigger, 26) && !php::in_array(output, &json!([56, 97])) {
        // #147483: "Unique lyra - Timing limbo effect/cleanup
        // We add a flag to force listening except for power I gain 1 boost/2 boost
        set_in(php::get_mut(&mut node, "RollDie"), "forceListening", json!(true))?;
    } else if php::in_array(trigger, &json!([419, 253])) {
        // #170850: "Amarok and hooked token "
        // needs to manage movecard with Amarok
        let move_card_output = match php::get(php::get(&node, "InvokeToken"), "output") {
            Value::Null => php::get(php::get(&node, "ChooseAssignment"), "output").clone(),
            x => x.clone(),
        };
        if !move_card_output.is_null() {
            set_in(php::get_mut(&mut node, "MoveCard"), "conditions", json!(["isCharacterFromTarget", "isPlayedInSameLocation", "excludeSelf"]))?;
            set_in(php::get_mut(&mut node, "MoveCard"), "output", move_card_output)?;
        }
    } else if is(output, 568) {
        // Zaratan uniques
        set_in(properties, "gigantic", json!(true))?;
        set_in(properties, "effectPlayed", ft::action("SpecialEffect", json!({"effect": "boostXOpponentExpedition"}), json!({})))?;
    } else if is(output, 593) {
        // Lucan uniques
        set_in(properties, "gigantic", json!(true))?;
        set_in(properties, "effectPlayed", ft::action("DiscardDo", json!({"effect": gain_me_boost(json!("X"))}), json!({})))?;
    } else if is(output, 607) {
        // Lucan uniques
        set_in(properties, "gigantic", json!(true))?;
        set_in(
            properties,
            "effectPlayed",
            ft::action(
                "DiscardDo",
                json!({
                    "sacrifice": true,
                    "targetType": ["permanent"],
                    "targetLocation": php::get(data(), "IN_PLAY"),
                    "effect": gain_me_boost(json!("X")),
                }),
                json!({}),
            ),
        )?;
    } else if is(output, 579) {
        // DoppelGanger
        set_in(
            properties,
            "effectHand",
            ft::action(
                "Target",
                json!({
                    "effect": ft::action("SpecialEffect", json!({"effect": "doppelganger"}), json!({})),
                    "targetLocation": ["opponentSource"],
                }),
                json!({}),
            ),
        )?;
        set_in(properties, "effectPassive", json!({"LeaveExpedition": {"output": ft::action("SpecialEffect", json!({"effect": "resetCard"}), json!({}))}}))?;
        set_in(properties, "seasoned", json!(true))?;
    } else if is(output, 646) {
        let slot = if is(trigger, 22) {
            Some("effectHand")
        } else if is(trigger, 24) {
            Some("effectPlayed")
        } else if is(trigger, 1) {
            Some("effectReserve")
        } else {
            None
        };
        if let Some(slot) = slot {
            let asleep = ft::gain(json!("EFFECT"), "asleep", json!(1), json!(99));
            set_in(properties, slot, ft::action("Target", json!({"upTo": true, "effect": asleep}), json!({})))?;
            set_in(php::get_mut(properties, "effectPassive"), "Gain", json!({"conditions": ["isSource", "isFacingSource"], "output": ft::sabotage()}))?;
        }
    } else if is(output, 114) && is(condition, 357) {
        set_in(properties, "costReductionIfEmpty", json!(2))?;
        php::unset(properties, "dynamicCostReduction");
    } else if is(trigger, 258) {
        push_in(php::get_mut(php::get_mut(&mut node, "MoveExpedition"), "conditions"), json!("isNotNight"))?;
    } else if is(output, 752) || is(output, 753) || is(output, 754) {
        set_in(properties, "effectPlayed", ft::action("EndAfternoon", json!([]), json!({})))?;
    } else if is(trigger, 542) {
        set_in(php::get_mut(&mut node, "EatMeEnergyBars"), "conditions", json!(["isPlayedInOpponentOtherExp"]))?;
        php::unset(php::get_mut(&mut node, "EatMeEnergyBars"), "listeningConditions");
    } else if php::in_array(trigger, &json!([688, 689])) {
        set_in(php::get_mut(&mut node, "InvokeToken"), "listeningConditions", json!(["isMyTurn", "isAfternoon", "isNotMeInvoke", "notTapped"]))?;
    } else if is(trigger, 5) {
        push_in(php::get_mut(php::get_mut(&mut node, "InvokeToken"), "conditions"), json!("isMeInvoke"))?;
    }

    // dynamic attributes generate empty node
    if !php::empty(&node) {
        if php::isset(properties, &key) {
            // parallel node for non passive effects
            if key != "effectPassive" && key != "effectInfinity" {
                // there is already an effect, check if there is an PAR node, to add the node
                if php::loose_eq(&get_or(php::get(properties, &key), "type", json!("")), &json!("parallel")) {
                    push_in(php::get_mut(php::get_mut(properties, &key), "childs"), node)?;
                } else {
                    // Merge as PARALLEL — type must be set so consumers flatten instead of nesting another PAR
                    let old_node = php::get(properties, &key).clone();
                    php::unset(properties, &key);
                    set_in(properties, &key, json!({"type": "parallel", "childs": [old_node, node]}))?;
                }
            } else if key == "effectInfinity" {
                // We need to merge everything depending on triggers or not.
                // If there are triggers, it must be wrapped in EffectPassive, else as it is.
                if php::isset(&calculated, "trigger") {
                    if php::isset(php::get(properties, &key), "effectPassive") {
                        merge_per_trigger(php::get_mut(php::get_mut(properties, &key), "effectPassive"), &mut node)?;
                    }
                    if !php::empty(&node) {
                        let p = merge(&[php::get(php::get(properties, &key), "effectPassive"), &node])?;
                        set_in(php::get_mut(properties, &key), "effectPassive", p)?;
                    }
                } else {
                    let p = merge(&[php::get(properties, &key), &node])?;
                    set_in(properties, &key, p)?;
                }
            } else {
                // PassiveEffects
                // we need to merge per trigger if needed
                merge_per_trigger(php::get_mut(properties, &key), &mut node)?;
                if !php::empty(&node) {
                    let p = merge(&[php::get(properties, &key), &node])?;
                    set_in(properties, &key, p)?;
                }
            }
        } else if key == "effectInfinity" && php::isset(&calculated, "trigger") {
            set_in(php::get_mut(properties, &key), "effectPassive", node)?;
        } else {
            set_in(properties, &key, node)?;
        }
    }

    if php::isset(&calculated, "outputPassive") {
        // add the condition to the passive effect, if it exists
        let trigger_conditions = get_or(&calculated, "triggerConditions", php::arr());
        let condition_conditions = get_or(&calculated, "conditionConditions", php::arr());
        let output_passive = php::get_mut(&mut calculated, "outputPassive");
        for t in php::keys_str(output_passive) {
            let passive = php::get_mut(output_passive, &t);
            let mut conditions = php::arr();
            if php::isset(passive, "condition") {
                php::push(&mut conditions, php::get(passive, "condition").clone());
                php::unset(passive, "condition");
            }
            let c = merge(&[&get_or(passive, "conditions", php::arr()), &conditions, &trigger_conditions, &condition_conditions])?;
            set_in(passive, "conditions", c)?;
        }
        if php::isset(properties, &key) {
            let existing = php::get_mut(properties, &key);
            for existing_trigger in php::keys_str(existing) {
                // Nothing to merge as it doesn't exist
                if !php::isset(php::get(&calculated, "outputPassive"), &existing_trigger) {
                    continue;
                }
                let passive = php::get(php::get(&calculated, "outputPassive"), &existing_trigger).clone();
                let existing_node = php::get_mut(existing, &existing_trigger);

                // update of existing node as there is still an output
                if is(output, 478) {
                    add_effect_to_condition(php::get(&passive, "output"), existing_node);
                    continue;
                }
                // we already have childs
                if php::isset(existing_node, "childs") {
                    push_in(php::get_mut(existing_node, "childs"), passive)?;
                } else {
                    *existing_node = json!({"childs": php::array_merge(&[&json!([existing_node.clone()]), &json!([passive])])});
                }
                php::unset(php::get_mut(&mut calculated, "outputPassive"), &existing_trigger);
            }
        }
        let p = merge(&[&get_or(properties, "effectPassive", php::arr()), php::get(&calculated, "outputPassive")])?;
        set_in(properties, "effectPassive", p)?;
    }

    // Description
    let key_desc = if key == "effectSupport" || key == "effectInfinity" { "supportDesc" } else { "effectDesc" };
    if !php::empty(php::get(properties, key_desc)) {
        push_in(php::get_mut(properties, key_desc), json!("<BR>"))?;
    }
    for d in ["triggerDescription", "conditionDescription", "outputDescription"] {
        push_in(php::get_mut(properties, key_desc), get_or(&calculated, d, json!("")))?;
    }

    if key == "effectSupport" {
        if php::loose_eq(php::get(&calculated, "triggerDescription"), &json!("{D}")) {
            set_in(properties, "supportIcon", json!("discard"))?;
        }
    } else if key == "effectInfinity" {
        set_in(properties, "supportIcon", json!("infinity"))?;
    }
    Ok(())
}

/// `FT::GAIN(ME, BOOST, $n)`
fn gain_me_boost(n: Value) -> Value {
    ft::gain(json!("me"), "boost", n, json!(99))
}

/// The loop that merges a node per trigger into existing passive effects: an effect on a trigger
/// already there joins it (`childs`) and leaves `node`
fn merge_per_trigger(existing: &mut Value, node: &mut Value) -> R<()> {
    for existing_trigger in php::keys_str(existing) {
        // Nothing to merge as it doesn't exist
        if !php::isset(node, &existing_trigger) {
            continue;
        }
        let new = php::get(node, &existing_trigger).clone();
        let existing_node = php::get_mut(existing, &existing_trigger);
        // we already have childs
        if php::isset(existing_node, "childs") {
            push_in(php::get_mut(existing_node, "childs"), new)?;
        } else {
            *existing_node = json!({"childs": php::array_merge(&[&json!([existing_node.clone()]), &json!([new])])});
        }
        php::unset(node, &existing_trigger);
    }
    Ok(())
}

/// `FlowConvertor::computeTrigger($effect, &$calculated)`
// PHP: Helpers/FlowConvertor.php:6704
pub fn compute_trigger(effect: &Value, calculated: &mut Value) -> R<()> {
    let trigger = php::get(get_triggers(), &php::key_of(effect));
    if trigger.is_null() {
        return Err(PhpError::new(format!("Unique trigger not implemented.{}", php::strval(effect))));
    }

    php::set(calculated, "type", get_or(trigger, "type", json!("effectPassive")));
    if php::isset(trigger, "trigger") && php::loose_ne(php::get(trigger, "trigger"), &json!("")) {
        php::set(calculated, "trigger", php::get(trigger, "trigger").clone());
    }
    if php::isset(trigger, "condition") {
        let c = php::get(trigger, "condition");
        let c = if php::is_arr(c) { c.clone() } else { json!([c]) };
        let tc = merge(&[&get_or(calculated, "triggerConditions", php::arr()), &c])?;
        php::set(calculated, "triggerConditions", tc);
    }
    if php::isset(trigger, "description") {
        php::set(calculated, "triggerDescription", php::get(trigger, "description").clone());
    }
    php::set(calculated, "afterRest", json!(php::isset(trigger, "afterRest")));
    if php::isset(trigger, "n") {
        php::set(calculated, "conditionN", php::get(trigger, "n").clone());
    }
    if php::isset(trigger, "pId") {
        php::set(calculated, "pId", php::get(trigger, "pId").clone());
    }
    if php::isset(trigger, "listeningConditions") {
        php::set(calculated, "triggerListeningConditions", php::get(trigger, "listeningConditions").clone());
    }
    Ok(())
}

/// `FlowConvertor::computeConditions($effect, &$calculated)`
// PHP: Helpers/FlowConvertor.php:6741
pub fn compute_conditions(effect: &Value, calculated: &mut Value) -> R<()> {
    let conditions = php::get(get_conditions(), &php::key_of(effect));
    if conditions.is_null() {
        return Err(PhpError::new(format!("Unique conditions not implemented.{}", php::strval(effect))));
    }
    if php::isset(conditions, "description") {
        php::set(calculated, "conditionDescription", php::get(conditions, "description").clone());
    }

    if php::isset(conditions, "condition") {
        let c = php::get(conditions, "condition");
        let c = if php::is_arr(c) { c.clone() } else { json!([c]) };
        if php::loose_ne(php::get(calculated, "type"), &json!("effectPassive")) {
            let tc = merge(&[&get_or(calculated, "triggerConditions", php::arr()), &c])?;
            php::set(calculated, "triggerConditions", tc);
        } else {
            // changed due to otherwise effect
            let cc = merge(&[&get_or(calculated, "conditionConditions", php::arr()), &c])?;
            php::set(calculated, "conditionConditions", cc);
        }
    }

    if php::isset(conditions, "effect") {
        php::set(calculated, "conditionEffect", php::get(conditions, "effect").clone());
    }

    if php::isset(conditions, "passiveEffect") {
        php::set(calculated, "passiveEffect", php::get(conditions, "passiveEffect").clone());
    }
    php::set(calculated, "ifYouDo", get_or(conditions, "ifYouDo", json!(false)));
    Ok(())
}

/// `FlowConvertor::computeOutput($effect, &$calculated)`
// PHP: Helpers/FlowConvertor.php:6777
pub fn compute_output(effect: &Value, calculated: &mut Value) -> R<()> {
    let output = php::get(get_output(), &php::key_of(effect));
    if output.is_null() {
        return Err(PhpError::new(format!("Unique conditions not implemented.{}", php::strval(effect))));
    }
    for (from, to) in [("description", "outputDescription"), ("output", "output"), ("passive", "outputPassive"), ("attributes", "outputAttributes")] {
        if php::isset(output, from) {
            php::set(calculated, to, php::get(output, from).clone());
        }
    }
    // For No trigger effects,
    if php::isset(output, "noTrigger") {
        php::set(calculated, "noTrigger", json!(true));
    }
    if php::isset(output, "oppositeOutput") {
        php::set(calculated, "oppositeOutput", php::get(output, "oppositeOutput").clone());
    }
    // Outputs whose deck-dependent part is only a step of a sequenced effect:
    // the rest of the effect must still be playable on an empty deck.
    if php::isset(output, "ignoreDeck") {
        php::set(calculated, "ignoreDeck", json!(true));
    }
    Ok(())
}

/// `FlowConvertor::tagIgnoreDeck(&$node)`: `ignoreDeck` on every CheckCondition node
// PHP: Helpers/FlowConvertor.php:6817
fn tag_ignore_deck(node: &mut Value) {
    if !php::is_arr(node) {
        return;
    }
    if php::strict_eq(php::get(node, "action"), &json!("CheckCondition")) {
        php::set(php::get_mut(node, "args"), "ignoreDeck", json!(true));
    }
    for k in php::keys_str(node) {
        let value = php::get_mut(node, &k);
        if php::is_arr(value) {
            tag_ignore_deck(value);
        }
    }
}

/// `FlowConvertor::insertCheckCondition($conditions, &$node, $description, $extraArgs)`
// PHP: Helpers/FlowConvertor.php:6833
pub fn insert_check_condition(conditions: &Value, node: &mut Value, description: Value, extra_args: &Value) -> R<()> {
    if php::empty(node) {
        let args = merge(&[&json!({"conditions": conditions, "effect": "OUTPUT", "oppositeEffect": "OPPOSITE", "description": description}), extra_args])?;
        *node = ft::action("CheckCondition", args, json!({}));
    } else {
        // Utils::search: the first child that is a CheckCondition (false, key 0, if none)
        let n_key = php::entries(node)
            .into_iter()
            .find(|(_, child)| php::loose_eq(&get_or(child, "action", json!("")), &json!("CheckCondition")))
            .map(|(k, _)| k)
            .unwrap_or_else(|| "0".into());
        let current = php::get(php::get(node, &n_key), "conditions");
        let current = if php::is_arr(current) { current.clone() } else { json!([current]) };
        let c = merge(&[&current, conditions])?;
        set_in(php::get_mut(node, &n_key), "conditions", c)?;
    }
    Ok(())
}

/// `FlowConvertor::addEffectToCondition($effect, &$node)`
pub fn add_effect_to_condition(effect: &Value, node: &mut Value) {
    if php::empty(node) {
        *node = effect.clone();
    } else {
        *node = utils::update_tree(node, &json!("OUTPUT"), effect, &[]);
    }
}

/// `FlowConvertor::addOppositeToNode($effect, &$node)`
pub fn add_opposite_to_node(effect: &Value, node: &mut Value) {
    if php::empty(node) {
        *node = effect.clone();
    } else {
        *node = utils::update_tree(node, &json!("OPPOSITE"), effect, &[]);
    }
}

/// `FlowConvertor::addOutputToNode($effect, &$node)`
pub fn add_output_to_node(effect: &Value, node: &mut Value) {
    if php::empty(node) {
        *node = effect.clone();
    } else {
        *node = utils::update_tree(node, &json!("OUTPUT"), effect, &[]);
    }
}

/// `FlowConvertor::getDynamicAttributeConditionList($trinity, $calculated, $unlessConditions)`:
/// dynamic attributes are checked without an event, so event-based trigger conditions become
/// state checks
// PHP: Helpers/FlowConvertor.php:6885
fn get_dynamic_attribute_condition_list(trinity: &Value, calculated: &Value, unless_conditions: &Value) -> R<Value> {
    let trigger_id = php::get(trinity, "trigger");
    if let Some(state_trigger_conditions) = get_state_trigger_conditions_for_dynamic_attribute(trigger_id) {
        return merge(&[&state_trigger_conditions, unless_conditions]);
    }
    merge(&[&get_or(calculated, "triggerConditions", php::arr()), unless_conditions])
}

/// `FlowConvertor::getStateTriggerConditionsForDynamicAttribute($triggerId)`
// PHP: Helpers/FlowConvertor.php:6899
fn get_state_trigger_conditions_for_dynamic_attribute(trigger_id: &Value) -> Option<Value> {
    // Another Character joined my Expedition — at least two Characters in this Expedition.
    if php::in_array(trigger_id, &json!([21, 253, 419, 517])) {
        return Some(json!(["XCharacterInExpedition:2:LTE"]));
    }
    None
}
