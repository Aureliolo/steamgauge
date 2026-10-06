//! What the app keeps on this computer, how much room each part of it takes, and giving the room
//! back.
//!
//! Every part is named by what removing it costs, which is what makes removing it safe to
//! offer: a search index is rebuilt when it is next needed, a read takes hours to redo, and
//! downloaded reviews come back from Steam in minutes.
//!
//! Sizes are counted by walking the folders each time the page asks. A game holds a few dozen
//! files and the models a handful, so the walk is milliseconds; a ledger kept beside every write
//! would cost more than it saves at this size.

use std::path::{Path, PathBuf};

use serde::Serialize;
use steamgauge_core::{embed, reader, search_models, state::CrawlState};
use tauri::AppHandle;

use super::{
    library_dir, shelf, text,
    work::{State, Task, Work},
};

/// The room a game's files take, by what removing each costs.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct Parts {
    /// The downloaded reviews of the newest download, fetched again from Steam in minutes.
    pub reviews: u64,
    /// The reads: hours of the reader's work to redo.
    pub reads: u64,
    /// Preparations for search by meaning, made again when next needed.
    pub search: u64,
    /// Earlier downloads a newer one replaced.
    pub earlier: u64,
    /// Downloads and reads that stopped part way, which are restarted rather than continued
    /// from these files.
    pub partial: u64,
    /// The library's own records: crawl progress, groups, choices.
    pub other: u64,
}

impl Parts {
    fn add(&mut self, part: Part, bytes: u64) {
        let slot = match part {
            Part::Reviews => &mut self.reviews,
            Part::Reads => &mut self.reads,
            Part::Search => &mut self.search,
            Part::Earlier => &mut self.earlier,
            Part::Partial => &mut self.partial,
            Part::Other => &mut self.other,
        };
        *slot = slot.saturating_add(bytes);
    }

    fn total(&self) -> u64 {
        [
            self.reviews,
            self.reads,
            self.search,
            self.earlier,
            self.partial,
            self.other,
        ]
        .into_iter()
        .fold(0, u64::saturating_add)
    }

    fn plus(mut self, other: &Self) -> Self {
        self.reviews += other.reviews;
        self.reads += other.reads;
        self.search += other.search;
        self.earlier += other.earlier;
        self.partial += other.partial;
        self.other += other.other;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Reviews,
    Reads,
    Search,
    Earlier,
    Partial,
    Other,
}

/// What a file in a game's newest download is, by the name the core writes it under.
fn part_of(name: &str) -> Part {
    let left_over = Path::new(name).extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("partial") || extension.eq_ignore_ascii_case("part")
    });
    if left_over || name.contains(".partial.") || name.contains(".recount.") {
        Part::Partial
    } else if name.contains("embeddings") {
        Part::Search
    } else if name.starts_with("readings") || name == "reading.json" || name == "claims.parquet" {
        Part::Reads
    } else if name.starts_with("shard-") || name.starts_with("sweep-") || name == "crawl.json" {
        Part::Reviews
    } else {
        Part::Other
    }
}

/// Every byte under a folder, or in a file, which is what removing it gives back.
pub fn bytes_under(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return std::fs::metadata(path)
            .map_or(0, |found| if found.is_file() { found.len() } else { 0 });
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => bytes_under(&entry.path()),
            Ok(_) => entry.metadata().map_or(0, |found| found.len()),
            Err(_) => 0,
        })
        .fold(0, u64::saturating_add)
}

/// Each file under a folder with its size, however deep.
fn files_under(dir: &Path, found: &mut Vec<(PathBuf, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => files_under(&entry.path(), found),
            Ok(_) => found.push((entry.path(), entry.metadata().map_or(0, |meta| meta.len()))),
            Err(_) => {}
        }
    }
}

fn game_dir(library: &Path, app_id: u32) -> PathBuf {
    library.join(format!("appid={app_id}"))
}

/// A game's files sorted into parts: the newest download's by name, earlier downloads whole.
fn game_parts(library: &Path, app_id: u32) -> Parts {
    let newest = embed::latest_snapshot(library, app_id).ok();
    let mut parts = Parts::default();
    let Ok(entries) = std::fs::read_dir(game_dir(library, app_id)) else {
        return parts;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_snapshot = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("snapshot="));
        if !is_snapshot {
            parts.add(Part::Other, bytes_under(&path));
        } else if newest.as_deref() == Some(path.as_path()) {
            let mut files = Vec::new();
            files_under(&path, &mut files);
            for (file, bytes) in files {
                let name = file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                parts.add(part_of(name), bytes);
            }
        } else {
            parts.add(Part::Earlier, bytes_under(&path));
        }
    }
    parts
}

/// One game and the room it takes.
#[derive(Debug, Clone, Serialize)]
pub struct GameRoom {
    pub app_id: u32,
    pub name: String,
    pub parts: Parts,
    pub total: u64,
}

/// One model in the cache and the room it takes.
#[derive(Debug, Clone, Serialize)]
pub struct ModelRoom {
    /// What the model is called, as the cockpit names it.
    pub name: String,
    /// How the window asks for it to be removed.
    pub key: &'static str,
    pub role: &'static str,
    pub bytes: u64,
    /// Whether this computer reads or searches with it, so removing it means fetching it again.
    pub used: bool,
}

/// A drive the app keeps something on.
#[derive(Debug, Clone, Serialize)]
pub struct Drive {
    pub name: String,
    pub total: u64,
    pub free: u64,
    /// What of the app's is on it: the library, the models, or both.
    pub holds: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Storage {
    pub library: String,
    pub models: String,
    pub drives: Vec<Drive>,
    /// The library's parts across every game, its own records included.
    pub parts: Parts,
    pub games: Vec<GameRoom>,
    pub model_rooms: Vec<ModelRoom>,
}

/// The models a computer can hold, each with the folder it lives in.
fn models(cache: &Path, reader_in_use: &str) -> Vec<(ModelRoom, PathBuf)> {
    let mut found: Vec<(ModelRoom, PathBuf)> = reader::SIZES
        .iter()
        .map(|size| {
            let home = size.home();
            (
                ModelRoom {
                    name: format!("Game Review Reader ({})", size.name),
                    key: size.name,
                    role: "reads every review",
                    bytes: bytes_under(&home),
                    used: size.name == reader_in_use,
                },
                home,
            )
        })
        .collect();
    for (model, role) in [
        (search_models::ENCODER, "finds what was said in other words"),
        (search_models::RERANKER, "orders what it finds"),
    ] {
        let home = model.dir(cache);
        found.push((
            ModelRoom {
                name: format!("SteamGauge {}", model.name.replace('-', " ")),
                key: model.name,
                role,
                bytes: bytes_under(&home),
                used: true,
            },
            home,
        ));
    }
    found
}

/// The nearest folder that exists, since a library not yet made still has a drive.
fn existing(path: &Path) -> &Path {
    path.ancestors().find(|dir| dir.exists()).unwrap_or(path)
}

/// What a person calls the drive a folder is on: its letter on Windows, the folder elsewhere.
fn drive_name(path: &Path) -> String {
    match path.components().next() {
        Some(std::path::Component::Prefix(prefix)) => {
            prefix.as_os_str().to_string_lossy().into_owned()
        }
        _ => path.display().to_string(),
    }
}

fn drives(library: &Path, models: &Path) -> Vec<Drive> {
    let mut drives: Vec<Drive> = Vec::new();
    for (path, holds) in [(library, "library"), (models, "models")] {
        let at = existing(path);
        let (Ok(total), Ok(free)) = (fs4::total_space(at), fs4::available_space(at)) else {
            continue;
        };
        let name = drive_name(at);
        // Two folders on one volume report the same totals; on Unix there is no letter to tell
        // them apart by, so the totals are what does.
        if let Some(same) = drives
            .iter_mut()
            .find(|drive| drive.total == total && drive.free == free)
        {
            same.holds.push(holds);
        } else {
            drives.push(Drive {
                name,
                total,
                free,
                holds: vec![holds],
            });
        }
    }
    drives
}

fn measure(app: &AppHandle) -> Storage {
    let library = library_dir(app);
    let cache = steamgauge_core::model::default_cache_dir();
    let reader_in_use = super::work::reader_here(&super::settings::Settings::load(app)).name;

    let mut games: Vec<GameRoom> = shelf(&library)
        .games
        .into_iter()
        .map(|game| {
            let parts = game_parts(&library, game.app_id);
            GameRoom {
                app_id: game.app_id,
                name: game.name,
                total: parts.total(),
                parts,
            }
        })
        .collect();
    games.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));

    let in_games = games
        .iter()
        .fold(Parts::default(), |sum, game| sum.plus(&game.parts));
    let mut parts = in_games;
    parts.add(
        Part::Other,
        bytes_under(&library).saturating_sub(in_games.total()),
    );

    Storage {
        library: library.display().to_string(),
        models: cache.display().to_string(),
        drives: drives(&library, &cache),
        parts,
        games,
        model_rooms: models(&cache, reader_in_use)
            .into_iter()
            .map(|(room, _)| room)
            .collect(),
    }
}

#[tauri::command]
pub async fn storage(app: AppHandle) -> Storage {
    tauri::async_runtime::spawn_blocking(move || measure(&app))
        .await
        .unwrap_or_else(|_| Storage {
            library: String::new(),
            models: String::new(),
            drives: Vec::new(),
            parts: Parts::default(),
            games: Vec::new(),
            model_rooms: Vec::new(),
        })
}

/// What a person can ask to have removed from a game.
#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Removal {
    Reads,
    Search,
    Earlier,
    Partial,
    /// The whole game: every file, its crawl records and its place in groups.
    Game,
}

fn active(job: &super::work::Job) -> bool {
    matches!(job.state, State::Queued | State::Running)
}

/// Why a game's files may not be touched now, if they may not.
fn busy_game(work: &Work, app_id: u32) -> Option<String> {
    work.jobs()
        .into_iter()
        .filter(active)
        .find_map(|job| match &job.task {
            Task::Export { app_ids, .. } if app_ids.contains(&app_id) => {
                Some("A report of this game is being written.".to_owned())
            }
            task if task.app_id() == Some(app_id) => Some(format!(
                "{} is working on this game: {}.",
                job.name,
                job.step.to_lowercase()
            )),
            _ => None,
        })
}

/// Removes the chosen part of one game, or of every game when none is named.
fn remove_from_game(library: &Path, app_id: u32, what: Removal) -> std::io::Result<()> {
    let dir = game_dir(library, app_id);
    let newest = embed::latest_snapshot(library, app_id).ok();
    match what {
        Removal::Game => {
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
            }
            if let Ok(state) = CrawlState::open(&library.join("state.sqlite")) {
                state.forget(app_id).map_err(std::io::Error::other)?;
            }
            super::cockpit::drop_from_groups(library, app_id)?;
        }
        Removal::Earlier => {
            // A crawl still running or interrupted writes into the newest folder, which may not
            // be complete yet: the earlier one is all that is whole until it finishes.
            let unfinished = CrawlState::open(&library.join("state.sqlite"))
                .ok()
                .and_then(|state| state.resumable(app_id).ok().flatten())
                .is_some();
            if unfinished {
                return Err(std::io::Error::other(
                    "a download of this game is unfinished; its earlier download is kept until it completes",
                ));
            }
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let path = entry.path();
                let is_snapshot = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("snapshot="));
                if is_snapshot && newest.as_deref() != Some(path.as_path()) {
                    std::fs::remove_dir_all(path)?;
                }
            }
        }
        Removal::Reads | Removal::Search | Removal::Partial => {
            let wanted = match what {
                Removal::Reads => Part::Reads,
                Removal::Search => Part::Search,
                _ => Part::Partial,
            };
            let Some(newest) = newest else {
                return Ok(());
            };
            let mut files = Vec::new();
            files_under(&newest, &mut files);
            for (file, _) in files {
                let name = file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                if part_of(name) == wanted {
                    std::fs::remove_file(file)?;
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn free_room(
    app: AppHandle,
    work: tauri::State<'_, Work>,
    app_id: Option<u32>,
    what: Removal,
) -> Result<Storage, String> {
    let library = library_dir(&app);
    let targets: Vec<u32> = match app_id {
        Some(app_id) => vec![app_id],
        None => shelf(&library)
            .games
            .into_iter()
            .map(|game| game.app_id)
            .collect(),
    };
    for &app_id in &targets {
        if let Some(why) = busy_game(&work, app_id) {
            return Err(why);
        }
    }
    let removing = targets.clone();
    tauri::async_runtime::spawn_blocking(move || {
        removing
            .into_iter()
            .try_for_each(|app_id| remove_from_game(&library, app_id, what))
    })
    .await
    .map_err(text)?
    .map_err(text)?;
    Ok(storage(app).await)
}

#[tauri::command]
pub async fn remove_model(
    app: AppHandle,
    work: tauri::State<'_, Work>,
    key: String,
) -> Result<Storage, String> {
    let reading = work
        .jobs()
        .into_iter()
        .filter(active)
        .any(|job| matches!(job.task, Task::Read { .. } | Task::Prepare { .. }));
    if reading {
        return Err(
            "A read or a preparation is running or waiting; models are removed once it is done."
                .to_owned(),
        );
    }
    let cache = steamgauge_core::model::default_cache_dir();
    let reader_in_use = super::work::reader_here(&super::settings::Settings::load(&app)).name;
    let Some((_, home)) = models(&cache, reader_in_use)
        .into_iter()
        .find(|(room, _)| room.key == key)
    else {
        return Err(format!("No model is called {key}."));
    };
    // Only ever a folder inside the cache, whatever a reader size's home says: a model being
    // tried from a working tree is not the app's to delete.
    if !home.starts_with(&cache) {
        return Err("That model is not in the app's model folder.".to_owned());
    }
    tauri::async_runtime::spawn_blocking(move || {
        if home.exists() {
            std::fs::remove_dir_all(&home)
        } else {
            Ok(())
        }
    })
    .await
    .map_err(text)?
    .map_err(text)?;
    Ok(storage(app).await)
}

#[cfg(test)]
mod tests {
    use super::{Part, Parts, Removal, game_parts, part_of, remove_from_game};
    use std::path::{Path, PathBuf};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("steamgauge-storage-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn file(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0_u8; bytes]).unwrap();
    }

    /// A game with an earlier download and a newer one holding every kind of file.
    fn game(library: &Path) {
        let game = library.join("appid=7");
        file(&game.join("snapshot=100/shard-0000.parquet"), 50);
        file(&game.join("snapshot=200/shard-0000.parquet"), 100);
        file(&game.join("snapshot=200/sweep-1.parquet"), 10);
        file(&game.join("snapshot=200/crawl.json"), 1);
        file(&game.join("snapshot=200/readings.parquet"), 300);
        file(&game.join("snapshot=200/reading.json"), 2);
        file(&game.join("snapshot=200/embeddings.parquet"), 400);
        file(&game.join("snapshot=200/embeddings.json"), 3);
        file(&game.join("snapshot=200/readings.partial.parquet"), 20);
        file(&game.join("snapshot=200/sweep-2.parquet.partial"), 5);
    }

    #[test]
    fn each_file_is_the_part_its_name_says() {
        for (name, part) in [
            ("shard-0012.parquet", Part::Reviews),
            ("sweep-3.parquet", Part::Reviews),
            ("crawl.json", Part::Reviews),
            ("readings.parquet", Part::Reads),
            ("reading.json", Part::Reads),
            ("claims.parquet", Part::Reads),
            ("embeddings.parquet", Part::Search),
            ("claim-embeddings.json", Part::Search),
            ("readings.partial.parquet", Part::Partial),
            ("readings.recount.parquet", Part::Partial),
            ("sweep-1.parquet.partial", Part::Partial),
            ("model.onnx.part", Part::Partial),
            ("groups.json", Part::Other),
        ] {
            assert_eq!(part_of(name), part, "{name}");
        }
    }

    #[test]
    fn a_game_is_sorted_into_its_parts_with_earlier_downloads_whole() {
        let scratch = Scratch::new("parts");
        game(&scratch.0);
        assert_eq!(
            game_parts(&scratch.0, 7),
            Parts {
                reviews: 111,
                reads: 302,
                search: 403,
                earlier: 50,
                partial: 25,
                other: 0
            }
        );
    }

    #[test]
    fn removing_reads_leaves_the_reviews_and_the_search() {
        let scratch = Scratch::new("reads");
        game(&scratch.0);
        remove_from_game(&scratch.0, 7, Removal::Reads).unwrap();
        let left = game_parts(&scratch.0, 7);
        assert_eq!((left.reads, left.reviews, left.search), (0, 111, 403));
    }

    #[test]
    fn removing_earlier_downloads_keeps_the_newest() {
        let scratch = Scratch::new("earlier");
        game(&scratch.0);
        remove_from_game(&scratch.0, 7, Removal::Earlier).unwrap();
        assert!(!scratch.0.join("appid=7/snapshot=100").exists());
        assert_eq!(game_parts(&scratch.0, 7).reviews, 111);
    }

    #[test]
    fn removing_a_game_leaves_nothing_of_it() {
        let scratch = Scratch::new("game");
        game(&scratch.0);
        remove_from_game(&scratch.0, 7, Removal::Game).unwrap();
        assert!(!scratch.0.join("appid=7").exists());
    }
}
