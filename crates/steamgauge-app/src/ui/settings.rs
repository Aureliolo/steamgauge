//! What a person chose about how the app runs, kept between sessions.
//!
//! Kept in the app's own configuration directory rather than in the library: the library can be
//! pointed elsewhere and shared, and how much of this machine's graphics card to take is a fact
//! about this machine.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// The shares of the graphics card a person can give the app's reading and preparing.
pub const SHARES: [f64; 4] = [0.25, 0.5, 0.75, 1.0];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// The share of the card's time a read or a preparation may take, one of [`SHARES`].
    pub gpu_share: f64,
    /// The reader size chosen where the machine reads on its processor; the card decides
    /// everywhere else.
    pub reader: Option<String>,
    /// The language a first reading counts, or none for every language.
    pub language: Option<String>,
    /// Whether opening the app asks Steam how many reviews each game has now.
    pub check_steam: bool,
    /// Whether a downloaded game is read as soon as the download finishes.
    pub read_after_download: bool,
    /// Whether the app asks GitHub, at most once a day, whether a newer `SteamGauge` is out.
    pub check_newer_version: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gpu_share: 1.0,
            reader: None,
            language: Some("english".to_owned()),
            check_steam: true,
            read_after_download: true,
            check_newer_version: true,
        }
    }
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("settings.json"))
}

impl Settings {
    /// What was chosen, or the defaults where nothing was or the file cannot be read.
    pub fn load(app: &AppHandle) -> Self {
        file(app)
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .map(Self::checked)
            .unwrap_or_default()
    }

    /// A share outside the offered ones is the nearest offered one, so a hand-edited file can
    /// neither starve a read to nothing nor ask for more than the whole card.
    fn checked(mut self) -> Self {
        self.gpu_share = SHARES
            .into_iter()
            .min_by(|a, b| {
                (a - self.gpu_share)
                    .abs()
                    .total_cmp(&(b - self.gpu_share).abs())
            })
            .unwrap_or(1.0);
        self.reader = self
            .reader
            .filter(|name| steamgauge_core::reader::Size::named(name).is_some());
        self
    }

    fn save(&self, app: &AppHandle) -> Result<(), String> {
        let path = file(app).ok_or("this system gives the app nowhere to keep settings")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let partial = path.with_extension("partial");
        std::fs::write(
            &partial,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(partial, path).map_err(|e| e.to_string())
    }
}

/// The settings, with whether every game read is also prepared for search, which the core keeps
/// in the library because a preparation is a fact about the library's games.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shown {
    #[serde(flatten)]
    pub settings: Settings,
    pub search_every_game: bool,
    pub shares: [f64; 4],
    /// Where the library is, so a person knows where the gigabytes went.
    pub library: String,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn settings(app: AppHandle) -> Shown {
    let library = super::library_dir(&app);
    Shown {
        settings: Settings::load(&app),
        search_every_game: steamgauge_core::meaning::Choice::load(&library).every_game,
        shares: SHARES,
        library: library.display().to_string(),
    }
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    settings: Settings,
    search_every_game: bool,
) -> Result<Shown, String> {
    settings.checked().save(&app)?;
    super::newer::check_in_background(&app);
    let library = super::library_dir(&app);
    std::fs::create_dir_all(&library).map_err(|e| e.to_string())?;
    steamgauge_core::meaning::Choice {
        every_game: search_every_game,
    }
    .save(&library)
    .map_err(|e| e.to_string())?;
    Ok(self::settings(app))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_written_by_hand_lands_on_the_nearest_one_offered() {
        let checked = |gpu_share| {
            Settings {
                gpu_share,
                ..Settings::default()
            }
            .checked()
            .gpu_share
        };
        assert!((checked(0.0) - 0.25).abs() < f64::EPSILON);
        assert!((checked(0.6) - 0.5).abs() < f64::EPSILON);
        assert!((checked(7.0) - 1.0).abs() < f64::EPSILON);
        assert!(
            SHARES.contains(&checked(f64::NAN)),
            "a share that is not a number still lands on one offered"
        );
    }

    #[test]
    fn a_file_written_before_a_setting_existed_takes_its_default() {
        let loaded: Settings = serde_json::from_str(r#"{"gpu_share": 0.5}"#).unwrap();
        assert!(loaded.check_newer_version);
        assert!(loaded.check_steam);
    }

    #[test]
    fn a_reader_no_size_answers_to_is_forgotten() {
        let checked = Settings {
            reader: Some("huge".to_owned()),
            ..Settings::default()
        }
        .checked();
        assert_eq!(checked.reader, None);
    }
}
