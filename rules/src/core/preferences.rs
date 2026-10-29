//! Core/Preferences.php: the `user_preferences` rows (not logged: undo leaves them as they are)

use crate::framework;
use crate::game::{Game, PrefRow};

/// `getLocalPrefsData()`: preference id and default value (`default`, else the first value)
const PREFS: &[(i64, &str)] = &[(103, "0"), (104, "2"), (152, "1")];

/// `Preferences::checkExistence()`: a row with the default value for each missing preference of
/// each player (run by `getAllDatas`, so by the first view of the game)
// PHP: Core/Preferences.php:88
pub fn check_existence(g: &mut Game) {
    let players = framework::players_in_order(g);
    for (id, default) in PREFS {
        for pid in &players {
            if !g.prefs.iter().any(|p| p.player_id == *pid && p.pref_id == *id && p.value.is_some()) {
                let row = g.prefs.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                g.prefs.push(PrefRow { id: row, player_id: *pid, pref_id: *id, value: Some(default.to_string()) });
            }
        }
    }
}
