//! Actions/*.php: one module per atomic action.
//!
//! Each action is a unit struct implementing `models::action::ActionClass`: override exactly the
//! methods the PHP class defines (the trait defaults are the base class of Models/Action.php).
//! - `ctx` is the action's engine node (`Option<NodeId>`); `NodeId`s follow in-place replacements
//!   (insertAsChild / pushParallelChilds turn a leaf into a SEQ / PARALLEL in the same slot).
//! - `getArg('x')` = `action::get_arg(self, g, ctx, "x")` (ctx arg, else `default_args()`).
//! - `st<Class>`: `st()` returns `Some(args)` to have `act<Class>(...args)` run through takeAction,
//!   `None` when PHP returns null (or calls `$this->resolveAction(...)` itself).
//! - act methods: `act(method, args)`, `args` is a list (positional) or a map (named arguments);
//!   use `action::param(args, i, "name")`; return the PHP return value (Null for void: it is the
//!   checkpoint flag of takeAction).
//! - `$this->resolveAction($args, $checkpoint)` = `action::resolve_action(self, g, ctx, args, cp)`
//!   (it proceeds); the other base methods are the free functions of `models::action`.
//! - Errors: PHP exceptions are `Err(PhpError)`, propagated with `?`.

pub mod activate_card;
pub mod activate_effect;
pub mod after_you;
pub mod block_expedition;
pub mod boost_exchange;
pub mod check_condition;
pub mod choose_assignment;
pub mod complete_feat;
pub mod discard;
pub mod discard_do;
pub mod discard_from_deck;
pub mod draw;
pub mod draw_mana;
pub mod end_afternoon;
pub mod exchange;
pub mod exhaust;
pub mod gain;
pub mod interupt_reveal;
pub mod invoke_token;
pub mod loose;
pub mod mark_region;
pub mod move_card;
pub mod move_expedition;
pub mod move_region_marker;
pub mod night_cleanup;
pub mod pay;
pub mod play_card;
pub mod ready;
pub mod resupply;
pub mod roll_die;
pub mod shuffle;
pub mod special_effect;
pub mod spell_cleanup;
pub mod spend;
pub mod tap;
pub mod target;
pub mod target_expedition;
pub mod target_player;
pub mod use_counter;
