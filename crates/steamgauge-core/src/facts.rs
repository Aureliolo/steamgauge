//! What the store says about a game that the reader and the labeller are both told.
//!
//! One fact, and on purpose only one. Everything a labeller is shown the reader is shown too,
//! or a label measures what the labeller was told rather than what the text says; and the one
//! thing the text cannot say, that a game is played only in a headset, is what two of the
//! sheet's rules turn on. "If you have a VR headset, get this" divides no readers of a game
//! nobody plays any other way, and a claim of motion sickness that names no screen means one
//! thing in a headset and another on a monitor.
//!
//! Kept beside the game's capture and beside its reference sets, as `game.json`, so a read and
//! a draw can each find it without the network. `steamgauge store-facts` writes both.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;

/// The file the facts are kept in, in a capture's game directory and in a reference set's.
pub const FILE: &str = "game.json";

/// What the store says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// Steam lists the game as VR Only: nobody plays it without a headset.
    pub headset_only: bool,
}

impl Facts {
    /// The facts kept in `dir`, if any are.
    #[must_use]
    pub fn load(dir: &Path) -> Option<Self> {
        serde_json::from_slice(&std::fs::read(dir.join(FILE)).ok()?).ok()
    }

    /// Keeps the facts in `dir`.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::write(dir.join(FILE), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// The facts kept in `dir` or up to three directories above it: a set's handout sits that
    /// far below the game it was drawn from, and no further up is any game's.
    #[must_use]
    pub fn around(dir: &Path) -> Option<Self> {
        dir.ancestors().take(4).find_map(Self::load)
    }
}

/// A game's facts, from beside its capture or beside its reference sets.
#[must_use]
pub fn of_game(captures: &Path, reference_game: &Path, app_id: u32) -> Option<Facts> {
    Facts::load(&captures.join(format!("appid={app_id}"))).or_else(|| Facts::load(reference_game))
}

/// The store's category for a game played only in a VR headset. Matched by id, because the
/// store answers in the language of whoever asks.
pub const VR_ONLY: u64 = 54;

/// Whether an `appdetails` answer with the categories filter lists the game as VR Only.
///
/// `None` where the answer holds no data for the app: a delisted or region-locked game is not
/// a game the store says is played on a screen.
#[must_use]
pub fn headset_only_in(body: &serde_json::Value, app_id: u32) -> Option<bool> {
    let data = body.get(app_id.to_string())?.get("data")?;
    Some(
        data.get("categories")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|categories| {
                categories.iter().any(|category| {
                    category.get("id").and_then(serde_json::Value::as_u64) == Some(VR_ONLY)
                })
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_store_says_headset_only_by_the_category_and_not_its_name() {
        // As the store answered for Beat Saber, asked from a machine set to Chinese.
        let beat_saber = serde_json::json!({
            "620980": {"success": true, "data": {"categories": [
                {"id": 2, "description": "单人"},
                {"id": 54, "description": "VR 独占"}
            ]}}
        });
        assert_eq!(headset_only_in(&beat_saber, 620_980), Some(true));
        let flat = serde_json::json!({"920210": {"success": true, "data": {"categories": [
            {"id": 53, "description": "VR Supported"}
        ]}}});
        assert_eq!(
            headset_only_in(&flat, 920_210),
            Some(false),
            "playable in a headset is not played only in one"
        );
        let delisted = serde_json::json!({"1": {"success": false}});
        assert_eq!(headset_only_in(&delisted, 1), None);
    }

    #[test]
    fn a_handout_finds_the_facts_of_the_game_it_was_drawn_from() {
        let game = std::env::temp_dir().join(format!("steamgauge-facts-{}", std::process::id()));
        let handout = game.join("retrieved").join("revisit");
        std::fs::create_dir_all(&handout).unwrap();
        Facts { headset_only: true }.save(&game).unwrap();
        assert_eq!(Facts::around(&handout), Some(Facts { headset_only: true }));
        let _ = std::fs::remove_dir_all(&game);
    }
}
