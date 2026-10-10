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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
#[serde(default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is one switch in Settings; no two of them name one state"
)]
pub struct Settings {
    /// The share of the card's time a read or a preparation may take, one of [`SHARES`].
    pub gpu_share: f64,
    /// The reader size chosen, or none for the one this machine is recommended. A size the
    /// card's memory cannot hold is passed over for the recommended one.
    pub reader: Option<String>,
    /// The language a first reading counts, or none for every language.
    pub language: Option<String>,
    /// Whether the app asks Steam how many reviews each game has now, on opening and while open.
    pub check_steam: bool,
    /// Whether a game Steam has enough new reviews for is brought up to date and read again by
    /// itself while the app is open.
    pub keep_up_to_date: bool,
    /// Whether an update the app made by itself that finds a subject moved says so in a desktop
    /// notification.
    pub notify_moves: bool,
    /// Whether a downloaded game is read as soon as the download finishes.
    pub read_after_download: bool,
    /// Whether the app asks GitHub, at most once a day, whether a newer `SteamGauge` is out.
    pub check_newer_version: bool,
    /// Whether programs on this computer that connect to an MCP server over HTTP may steer the
    /// app, with the token Settings shows, on 127.0.0.1 and `http_port`.
    pub answer_over_http: bool,
    pub http_port: u16,
}

/// The port the HTTP server answers on until another is chosen.
pub const HTTP_PORT: u16 = 47_800;

impl Default for Settings {
    fn default() -> Self {
        Self {
            gpu_share: 1.0,
            reader: None,
            language: Some("english".to_owned()),
            check_steam: true,
            keep_up_to_date: true,
            notify_moves: false,
            read_after_download: true,
            check_newer_version: true,
            answer_over_http: false,
            http_port: HTTP_PORT,
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
        // The ports below 1024 are the system's to give out.
        if self.http_port < 1024 {
            self.http_port = HTTP_PORT;
        }
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
    /// What adds this copy to Claude Code.
    pub claude_command: String,
    /// How a client reaches the HTTP server, where it answers.
    pub http: super::mcp_http::Reach,
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
        claude_command: super::mcp::claude_command(),
        http: super::mcp_http::reach(&app),
    }
}

/// A new token for the HTTP server, which shuts out whoever held the old one.
#[tauri::command]
pub fn new_http_token(app: AppHandle) -> Result<Shown, String> {
    super::mcp_http::new_token(&app)?;
    Ok(settings(app))
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    settings: Settings,
    search_every_game: bool,
) -> Result<Shown, String> {
    settings.checked().save(&app)?;
    super::mcp_http::follow(&app);
    super::newer::check_in_background(&app);
    super::cockpit::check_when_due(&app);
    super::cockpit::keep_up_to_date(&app);
    let library = super::library_dir(&app);
    std::fs::create_dir_all(&library).map_err(|e| e.to_string())?;
    steamgauge_core::meaning::Choice {
        every_game: search_every_game,
    }
    .save(&library)
    .map_err(|e| e.to_string())?;
    Ok(self::settings(app))
}

/// One reader size as Settings offers it.
#[derive(Debug, Clone, Serialize)]
pub struct SizeOption {
    pub name: &'static str,
    /// The whole download, and what of it is still to fetch.
    pub download_bytes: u64,
    pub bytes_left: u64,
    pub published: bool,
    /// The card memory it reads in.
    pub needs: u64,
    /// Whether it can read on this machine: anywhere on a processor, on a card only if the
    /// card's memory holds it.
    pub runs_here: bool,
    /// How many times the fastest size's time it takes on a processor.
    pub times: f64,
}

/// This computer, the reader it is recommended, the one it reads with, and every size.
#[derive(Debug, Clone, Serialize)]
pub struct ReaderOptions {
    pub card: Option<String>,
    pub card_bytes: Option<u64>,
    pub on_processor: bool,
    /// Whether this build can put a reader on a card at all.
    pub reaches_card: bool,
    pub recommended: &'static str,
    pub reads_with: &'static str,
    pub sizes: Vec<SizeOption>,
    /// The room left on the drive the models are kept on, so a download can be weighed first.
    pub free_bytes: Option<u64>,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn reader_options(app: AppHandle) -> ReaderOptions {
    use steamgauge_core::reader::{SIZES, fits, on_the_processor};

    let card = steamgauge_core::card::largest();
    let reaches = steamgauge_core::model::REACHES_A_CARD;
    let fastest = SIZES[0].processor_seconds;
    ReaderOptions {
        card: steamgauge_core::card::name(),
        card_bytes: card.map(|card| card.bytes),
        on_processor: on_the_processor(card, reaches),
        reaches_card: reaches,
        recommended: fits(card, reaches).name,
        reads_with: super::work::reader_here(&Settings::load(&app)).name,
        free_bytes: {
            let cache = steamgauge_core::model::default_cache_dir();
            let at = cache.ancestors().find(|dir| dir.exists()).unwrap_or(&cache);
            fs4::available_space(at).ok()
        },
        sizes: SIZES
            .iter()
            .map(|size| {
                let published = size.published.is_pinned();
                SizeOption {
                    name: size.name,
                    download_bytes: size.published.total_bytes(),
                    bytes_left: if published {
                        size.published.bytes_left(&size.home())
                    } else {
                        0
                    },
                    published,
                    needs: size.needs,
                    runs_here: super::work::runs_here(size, card, reaches),
                    times: size.processor_seconds / fastest,
                }
            })
            .collect(),
    }
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
        assert!(loaded.keep_up_to_date);
        assert!(
            !loaded.notify_moves,
            "a notification is something a person asks for"
        );
        assert!(
            !loaded.answer_over_http,
            "a port other programs can reach is something a person asks for"
        );
        assert_eq!(loaded.http_port, HTTP_PORT);
    }

    #[test]
    fn a_port_the_system_gives_out_is_not_taken() {
        let checked = |http_port| {
            Settings {
                http_port,
                ..Settings::default()
            }
            .checked()
            .http_port
        };
        assert_eq!(checked(80), HTTP_PORT);
        assert_eq!(checked(0), HTTP_PORT);
        assert_eq!(checked(52_000), 52_000);
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
