//! The library as a whole: every game in a row, the cockpit's overview of them, the groups a
//! person sorts them into, and the comparisons and reports made from several at once.
//!
//! Nothing here counts anything. Every figure comes from what a crawl, a sweep or a read wrote
//! beside the capture, or from what Steam said when the library was last checked against it.

use std::{collections::HashMap, path::Path};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use super::{library_dir, read_report, settings::Settings, text, work};

/// How many reviews Steam had for each game when the library was last checked against it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SteamTotals {
    pub checked: i64,
    pub games: HashMap<u32, u64>,
}

const TOTALS_FILE: &str = "steam-totals.json";

impl SteamTotals {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(TOTALS_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        write_whole(&dir.join(TOTALS_FILE), &serde_json::to_vec_pretty(self)?)
    }
}

/// The newest release each published repository carries, when that was last asked.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Releases {
    pub checked: i64,
    pub newest: HashMap<String, String>,
}

/// Every repository this build fetches a model from, where it has been published.
fn published() -> Vec<&'static str> {
    use steamgauge_core::search_models::{ENCODER, RERANKER};

    steamgauge_core::reader::SIZES
        .iter()
        .map(|size| size.published.repository)
        .chain([ENCODER.repository, RERANKER.repository])
        .filter(|repository| !repository.is_empty())
        .collect()
}

fn releases_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("releases.json"))
}

impl Releases {
    pub async fn fetch() -> Self {
        let mut newest = HashMap::new();
        for repository in published() {
            // A repository the Hub will not list today is left out rather than reported as
            // having nothing newer, which would be a claim made from no answer.
            if let Ok(Some(tag)) =
                steamgauge_core::model::newest_release(steamgauge_core::model::HUB, repository)
                    .await
            {
                newest.insert(repository.to_owned(), tag);
            }
        }
        Self {
            checked: super::now_unix(),
            newest,
        }
    }

    pub fn load(app: &AppHandle) -> Self {
        releases_file(app)
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app: &AppHandle) -> std::io::Result<()> {
        let Some(path) = releases_file(app) else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_whole(&path, &serde_json::to_vec_pretty(self)?)
    }

    /// The release newer than `pinned` that `repository` carries, if there is one.
    fn newer(&self, repository: &str, pinned: &str) -> Option<String> {
        let newest = self.newest.get(repository)?;
        let (newest_number, pinned_number) = (
            steamgauge_core::model::release_number(newest)?,
            steamgauge_core::model::release_number(pinned)?,
        );
        (newest_number > pinned_number).then(|| newest.clone())
    }
}

/// The groups a person sorts their games into.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Groups {
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub app_ids: Vec<u32>,
}

const GROUPS_FILE: &str = "groups.json";

impl Groups {
    fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(GROUPS_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Names trimmed, empty and repeated ones dropped, and each game listed once per group,
    /// so the file only ever holds what the window can show.
    fn tidied(self) -> Self {
        let mut seen = std::collections::HashSet::new();
        let groups = self
            .groups
            .into_iter()
            .filter_map(|group| {
                let name = group.name.trim().to_owned();
                if name.is_empty() || !seen.insert(name.to_lowercase()) {
                    return None;
                }
                let mut app_ids = group.app_ids;
                app_ids.sort_unstable();
                app_ids.dedup();
                Some(Group { name, app_ids })
            })
            .collect();
        Self { groups }
    }
}

/// Takes a removed game out of every group, so no group names a game the library no longer has.
pub fn drop_from_groups(dir: &Path, app_id: u32) -> std::io::Result<()> {
    let mut groups = Groups::load(dir);
    let before = groups.clone();
    for group in &mut groups.groups {
        group.app_ids.retain(|&id| id != app_id);
    }
    if groups == before {
        return Ok(());
    }
    let bytes = serde_json::to_vec_pretty(&groups.tidied()).map_err(std::io::Error::other)?;
    write_whole(&dir.join(GROUPS_FILE), &bytes)
}

/// Written beside its place and moved over it, so a file is never half one version.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let partial = path.with_extension("partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(partial, path)
}

/// What one game's reading says, as a row needs it.
#[derive(Debug, Clone, Serialize)]
pub struct Read {
    pub language: Option<String>,
    pub reviews: u64,
    pub claims: u64,
    /// Whether the reader that would read here now is the one that made this reading.
    pub current: bool,
    pub reader: String,
    pub recommended: Option<f64>,
}

/// One game as the library lists it.
#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub app_id: u32,
    pub name: String,
    pub reviews: u64,
    pub valve_total: u64,
    pub coverage: f64,
    pub verdict: String,
    /// The share of Steam's reviews recommending the game, at the last download or update.
    pub recommended: Option<f64>,
    pub downloaded: i64,
    pub updated: Option<i64>,
    /// Reviews Steam has gained since the last download or update, where it has been asked.
    pub new_on_steam: Option<u64>,
    pub read: Option<Read>,
    /// What moved lately, where the game has been read with months to compare.
    pub recent: Option<steamgauge_core::moves::Recent>,
    pub groups: Vec<String>,
    /// The reading itself, for what moved since a look; the window is sent what it needs of it.
    #[serde(skip)]
    pub report: Option<steamgauge_core::read::ReadReport>,
}

/// The run id of the reader this machine would read with now, where one is on it.
fn reader_run(settings: &Settings) -> Option<String> {
    let size = work::reader_here(settings);
    steamgauge_core::reader::Provenance::load(&size.home())
        .ok()
        .map(|provenance| provenance.run_id)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn share(part: u64, whole: u64) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// Every game in the library, as rows.
pub fn rows(app: &AppHandle) -> Vec<Row> {
    let dir = library_dir(app);
    let settings = Settings::load(app);
    let current = reader_run(&settings);
    let totals = SteamTotals::load(&dir);
    let groups = Groups::load(&dir);
    super::shelf(&dir)
        .games
        .into_iter()
        .filter_map(|game| {
            let facts = steamgauge_core::report::crawl_facts(&dir, game.app_id).ok()?;
            let reading = steamgauge_core::embed::latest_snapshot(&dir, game.app_id)
                .ok()
                .and_then(|snapshot| read_report(&snapshot).ok());
            Some(Row {
                app_id: game.app_id,
                name: game.name,
                reviews: game.reviews,
                valve_total: game.valve_total,
                coverage: game.coverage,
                verdict: game.verdict,
                recommended: share(facts.valve_total_positive, facts.valve_total_reviews),
                downloaded: facts.snapshot_unix,
                updated: facts.swept_unix,
                new_on_steam: totals
                    .games
                    .get(&game.app_id)
                    .map(|now| now.saturating_sub(facts.valve_total_reviews)),
                recent: reading.as_ref().and_then(steamgauge_core::moves::recent),
                read: reading.as_ref().map(|reading| Read {
                    current: current.as_deref() == Some(reading.read_with.as_str()),
                    reader: reading.reader.clone(),
                    recommended: share(reading.positive, reading.reviews),
                    language: reading.language.clone(),
                    reviews: reading.reviews,
                    claims: reading.claims,
                }),
                report: reading,
                groups: groups
                    .groups
                    .iter()
                    .filter(|group| group.app_ids.contains(&game.app_id))
                    .map(|group| group.name.clone())
                    .collect(),
            })
        })
        .collect()
}

#[tauri::command]
pub async fn games(app: AppHandle) -> Result<Vec<Row>, String> {
    tauri::async_runtime::spawn_blocking(move || rows(&app))
        .await
        .map_err(text)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn groups(app: AppHandle) -> Groups {
    Groups::load(&library_dir(&app))
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn save_groups(app: AppHandle, groups: Groups) -> Result<Groups, String> {
    let dir = library_dir(&app);
    std::fs::create_dir_all(&dir).map_err(text)?;
    let groups = groups.tidied();
    write_whole(
        &dir.join(GROUPS_FILE),
        &serde_json::to_vec_pretty(&groups).map_err(text)?,
    )
    .map_err(text)?;
    Ok(groups)
}

/// A game, named.
#[derive(Debug, Clone, Serialize)]
pub struct Named {
    pub app_id: u32,
    pub name: String,
}

/// One subject that moved in one game.
#[derive(Debug, Clone, Serialize)]
pub struct GameMove {
    pub app_id: u32,
    pub name: String,
    pub from: String,
    pub to: String,
    pub since: String,
    #[serde(flatten)]
    pub moved: steamgauge_core::moves::Move,
}

/// A game whose share recommending it moved.
#[derive(Debug, Clone, Serialize)]
pub struct GameShift {
    pub app_id: u32,
    pub name: String,
    pub from: String,
    pub to: String,
    pub since: String,
    pub shift: steamgauge_core::moves::Shift,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewOnSteam {
    pub app_id: u32,
    pub name: String,
    pub new: u64,
}

/// A model this build uses, and where it stands on this machine.
#[derive(Debug, Clone, Serialize)]
pub struct ModelState {
    pub name: String,
    /// What it is for, in words.
    pub role: &'static str,
    pub here: bool,
    pub bytes_left: u64,
    pub release: Option<&'static str>,
    pub newer: Option<String>,
    /// Whether this machine reads with it.
    pub used: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Machine {
    pub card: Option<String>,
    pub card_bytes: Option<u64>,
    /// Whether this build can put a model on a graphics card at all.
    pub reaches_card: bool,
    pub on_processor: bool,
    pub reader: &'static str,
    pub gpu_share: f64,
    pub models: Vec<ModelState>,
    pub releases_checked: i64,
}

fn machine(app: &AppHandle) -> Machine {
    use steamgauge_core::{
        reader::{SIZES, on_the_processor},
        search_models::{ENCODER, RERANKER},
    };

    let settings = Settings::load(app);
    let card = steamgauge_core::card::largest();
    let reaches = steamgauge_core::model::REACHES_A_CARD;
    let reader = work::reader_here(&settings);
    let releases = Releases::load(app);
    let release = |pinned: &'static str| (!pinned.is_empty()).then_some(pinned);
    let cache = steamgauge_core::model::default_cache_dir();
    let mut models: Vec<ModelState> = SIZES
        .iter()
        .map(|size| {
            let left = size.published.bytes_left(&size.home());
            ModelState {
                name: format!("Game Review Reader ({})", size.name),
                role: "reads every review",
                here: size.home().join("model.onnx").is_file()
                    && (left == 0 || !size.published.is_pinned()),
                bytes_left: if size.published.is_pinned() { left } else { 0 },
                release: release(size.published.release),
                newer: releases.newer(size.published.repository, size.published.release),
                used: size.name == reader.name,
            }
        })
        .collect();
    models.extend(
        [
            (ENCODER, "finds what was said in other words"),
            (RERANKER, "orders what it finds"),
        ]
        .map(|(model, role)| ModelState {
            name: format!("SteamGauge {}", model.name.replace('-', " ")),
            role,
            here: model.fetched(&cache),
            bytes_left: model.bytes_left(&cache),
            release: release(model.release),
            newer: releases.newer(model.repository, model.release),
            used: true,
        }),
    );
    Machine {
        card: steamgauge_core::card::name(),
        card_bytes: card.map(|card| card.bytes),
        reaches_card: reaches,
        on_processor: on_the_processor(card, reaches),
        reader: reader.name,
        gpu_share: settings.gpu_share,
        models,
        releases_checked: releases.checked,
    }
}

/// Everything the cockpit shows.
#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub games: usize,
    pub read: usize,
    pub reviews: u64,
    pub claims: u64,
    pub disk_bytes: u64,
    pub library: String,
    pub not_read: Vec<Named>,
    pub older_reader: Vec<Named>,
    pub new_on_steam: Vec<NewOnSteam>,
    pub checked: i64,
    pub moves: Vec<GameMove>,
    pub recommended: Vec<GameShift>,
    /// What moved since the person last looked at the cockpit.
    pub since: super::since::Lately,
    pub machine: Machine,
    pub jobs: Vec<work::Job>,
}

/// The moves the cockpit leads with, clearest first across every game.
const MOVES_SHOWN: usize = 12;

fn overview_of(app: &AppHandle) -> Overview {
    let dir = library_dir(app);
    let rows = rows(app);
    let named = |row: &Row| Named {
        app_id: row.app_id,
        name: row.name.clone(),
    };
    let mut moves: Vec<GameMove> = rows
        .iter()
        .filter_map(|row| row.recent.as_ref().map(|recent| (row, recent)))
        .flat_map(|(row, recent)| {
            recent.moves.iter().map(move |moved| GameMove {
                app_id: row.app_id,
                name: row.name.clone(),
                from: recent.from.clone(),
                to: recent.to.clone(),
                since: recent.since.clone(),
                moved: moved.clone(),
            })
        })
        .collect();
    moves.sort_by(|left, right| {
        right
            .moved
            .shift
            .z
            .abs()
            .total_cmp(&left.moved.shift.z.abs())
    });
    moves.truncate(MOVES_SHOWN);
    let mut recommended: Vec<GameShift> = rows
        .iter()
        .filter_map(|row| {
            let recent = row.recent.as_ref()?;
            let shift = recent
                .recommended
                .filter(steamgauge_core::moves::Shift::clear)?;
            Some(GameShift {
                app_id: row.app_id,
                name: row.name.clone(),
                from: recent.from.clone(),
                to: recent.to.clone(),
                since: recent.since.clone(),
                shift,
            })
        })
        .collect();
    recommended.sort_by(|left, right| right.shift.z.abs().total_cmp(&left.shift.z.abs()));
    let mut new_on_steam: Vec<NewOnSteam> = rows
        .iter()
        .filter_map(|row| {
            Some(NewOnSteam {
                app_id: row.app_id,
                name: row.name.clone(),
                new: row.new_on_steam.filter(|new| *new > 0)?,
            })
        })
        .collect();
    new_on_steam.sort_by_key(|game| std::cmp::Reverse(game.new));
    Overview {
        games: rows.len(),
        read: rows.iter().filter(|row| row.read.is_some()).count(),
        reviews: rows.iter().map(|row| row.reviews).sum(),
        claims: rows
            .iter()
            .filter_map(|row| row.read.as_ref().map(|read| read.claims))
            .sum(),
        disk_bytes: super::storage::bytes_under(&dir),
        library: dir.display().to_string(),
        not_read: rows
            .iter()
            .filter(|row| row.read.is_none())
            .map(named)
            .collect(),
        older_reader: rows
            .iter()
            .filter(|row| row.read.as_ref().is_some_and(|read| !read.current))
            .map(named)
            .collect(),
        new_on_steam,
        checked: SteamTotals::load(&dir).checked,
        moves,
        recommended,
        since: super::since::lately(app, &rows),
        machine: machine(app),
        jobs: app.state::<work::Work>().jobs(),
    }
}

#[tauri::command]
pub async fn overview(app: AppHandle) -> Result<Overview, String> {
    tauri::async_runtime::spawn_blocking(move || overview_of(&app))
        .await
        .map_err(text)
}

/// How long the library's counts from Steam are taken as current before the app asks again.
const CHECK_EVERY: i64 = 6 * 60 * 60;

/// How often the open app looks at whether the counts have aged past [`CHECK_EVERY`]. Asking
/// whether is a read of two small files; asking Steam is what the six hours are for.
pub const LOOK_AGAIN_EVERY: std::time::Duration = std::time::Duration::from_mins(10);

/// Asks Steam about the library, on opening and while the app stays open, where the person
/// wants that and it has not been asked lately. The question is the app's own, so it waits for
/// the person's work.
pub fn check_when_due(app: &AppHandle) {
    let dir = library_dir(app);
    if Settings::load(app).check_steam
        && super::now_unix() - SteamTotals::load(&dir).checked > CHECK_EVERY
        && !super::shelf(&dir).games.is_empty()
    {
        app.state::<work::Work>()
            .queue_background(app, work::Task::Check);
    }
}

/// Whether a game Steam has `new` reviews for that the library does not is brought up to date by
/// itself, given the `held` reviews the library has of it. An update reads the whole game again,
/// so it waits until Steam has a hundredth more than the library holds: the card is never spent
/// re-reading a game for less than a hundredth of it, and a small game, cheap to read, is kept
/// up to date review by review.
pub(super) fn due(held: u64, new: u64) -> bool {
    new > 0 && new >= held / 100
}

/// Brings every game with enough new reviews on Steam up to date as work of the app's own, from
/// what Steam last said, where the person wants that. The update reads the game again where it
/// had been read, in the language it was read in.
pub fn keep_up_to_date(app: &AppHandle) {
    let settings = Settings::load(app);
    if !(settings.check_steam && settings.keep_up_to_date) {
        return;
    }
    let dir = library_dir(app);
    let totals = SteamTotals::load(&dir);
    let work = app.state::<work::Work>();
    for game in super::shelf(&dir).games {
        let new = totals
            .games
            .get(&game.app_id)
            .map_or(0, |now| now.saturating_sub(game.valve_total));
        if due(game.reviews, new) {
            work.queue_background(
                app,
                work::Task::Update {
                    app_id: game.app_id,
                },
            );
        }
    }
}

/// Reads these games, each in the language it was last read in, or in the one chosen for first
/// readings where it has not been read.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn queue_reads(app: AppHandle, app_ids: Vec<u32>) -> Vec<u64> {
    let dir = library_dir(&app);
    let first = Settings::load(&app).language;
    let work = app.state::<work::Work>();
    app_ids
        .into_iter()
        .map(|app_id| {
            let language = steamgauge_core::embed::latest_snapshot(&dir, app_id)
                .ok()
                .and_then(|snapshot| read_report(&snapshot).ok())
                .map_or_else(|| first.clone(), |reading| reading.language);
            work.queue(&app, work::Task::Read { app_id, language }, None)
        })
        .collect()
}

/// Updates these games: what was written or edited since, then a read where one was read.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn queue_updates(app: AppHandle, app_ids: Option<Vec<u32>>) -> Vec<u64> {
    let app_ids = app_ids.unwrap_or_else(|| {
        super::shelf(&library_dir(&app))
            .games
            .into_iter()
            .map(|game| game.app_id)
            .collect()
    });
    let work = app.state::<work::Work>();
    app_ids
        .into_iter()
        .map(|app_id| work.queue(&app, work::Task::Update { app_id }, None))
        .collect()
}

/// One game side by side with others: its subjects as counts, so the window can show each as a
/// share of the game's own reviews.
#[derive(Debug, Clone, Serialize)]
pub struct Compared {
    pub app_id: u32,
    pub name: String,
    pub reviews: u64,
    pub language: Option<String>,
    pub recommended: Option<f64>,
    pub subjects: Vec<steamgauge_core::read::SubjectCount>,
}

#[tauri::command]
pub async fn compare(app: AppHandle, app_ids: Vec<u32>) -> Result<Vec<Compared>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = library_dir(&app);
        app_ids
            .into_iter()
            .map(|app_id| {
                let snapshot =
                    steamgauge_core::embed::latest_snapshot(&dir, app_id).map_err(text)?;
                let reading = read_report(&snapshot)?;
                let facts = steamgauge_core::report::crawl_facts(&dir, app_id).map_err(text)?;
                Ok(Compared {
                    app_id,
                    name: facts.title(),
                    reviews: reading.reviews,
                    language: reading.language.clone(),
                    recommended: share(reading.positive, reading.reviews),
                    subjects: reading.subjects,
                })
            })
            .collect()
    })
    .await
    .map_err(text)?
}

/// Asks where to save a report of these games, then puts writing it on the board.
#[tauri::command]
pub async fn export_report(app: AppHandle, app_ids: Vec<u32>) -> Result<Option<u64>, String> {
    if app_ids.is_empty() {
        return Err("choose at least one game to report on".to_owned());
    }
    let dir = library_dir(&app);
    let suggested = match app_ids.as_slice() {
        [one] => steamgauge_core::report::crawl_facts(&dir, *one)
            .map_or_else(|_| format!("app-{one}"), |facts| facts.title()),
        several => format!("{} games compared", several.len()),
    };
    let mut asking = rfd::AsyncFileDialog::new()
        .set_title("Save the report")
        .add_filter("Web page", &["html"])
        .set_file_name(format!("{} - SteamGauge.html", file_safe(&suggested)));
    if let Some(window) = app.get_webview_window("main") {
        asking = asking.set_parent(&window);
    }
    let Some(chosen) = asking.save_file().await else {
        return Ok(None);
    };
    let to = chosen.path().to_path_buf();
    let name = if app_ids.len() == 1 {
        None
    } else {
        Some(format!("Report on {} games", app_ids.len()))
    };
    Ok(Some(app.state::<work::Work>().queue(
        &app,
        work::Task::Export { app_ids, to },
        name,
    )))
}

/// A game's name as a file name: what Windows, macOS and Linux all refuse in one is dropped.
pub(super) fn file_safe(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| {
            !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()
        })
        .collect();
    let trimmed = kept.trim().trim_end_matches('.').to_owned();
    if trimmed.is_empty() {
        "report".to_owned()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_keep_one_of_each_name_and_each_game_once() {
        let tidied = Groups {
            groups: vec![
                Group {
                    name: "  Rivals ".to_owned(),
                    app_ids: vec![3, 1, 3],
                },
                Group {
                    name: "rivals".to_owned(),
                    app_ids: vec![9],
                },
                Group {
                    name: "   ".to_owned(),
                    app_ids: vec![2],
                },
            ],
        }
        .tidied();
        assert_eq!(
            tidied.groups,
            vec![Group {
                name: "Rivals".to_owned(),
                app_ids: vec![1, 3],
            }]
        );
    }

    #[test]
    fn a_newer_release_is_one_numbered_past_the_pin() {
        let releases = Releases {
            checked: 0,
            newest: HashMap::from([("someone/reader".to_owned(), "v3".to_owned())]),
        };
        assert_eq!(
            releases.newer("someone/reader", "v2"),
            Some("v3".to_owned())
        );
        assert_eq!(releases.newer("someone/reader", "v3"), None);
        assert_eq!(
            releases.newer("someone/reader", ""),
            None,
            "nothing pinned, nothing newer"
        );
        assert_eq!(releases.newer("someone/else", "v1"), None);
    }

    #[test]
    fn a_game_is_brought_up_to_date_once_steam_has_a_hundredth_more() {
        assert!(!due(0, 0));
        assert!(!due(50_000, 0));
        assert!(due(0, 1));
        assert!(
            due(150, 1),
            "a small game is kept up to date review by review"
        );
        assert!(!due(50_000, 499));
        assert!(due(50_000, 500));
        assert!(due(50_000, 4_000));
    }

    #[test]
    fn a_name_becomes_a_file_name_on_every_system() {
        assert_eq!(file_safe("Half-Life: Alyx"), "Half-Life Alyx");
        assert_eq!(file_safe("What?*"), "What");
        assert_eq!(file_safe("..."), "report");
    }
}
