//! What the app keeps on this computer, how much room each part of it takes, and giving the room
//! back.
//!
//! Every part is named by what removing it costs, which is what makes removing it safe to
//! offer: a search index is rebuilt when it is next needed, a read takes hours to redo, and
//! downloaded reviews come back from Steam in minutes.
//!
//! Sizes are counted by walking the folders each time the page asks. A game holds a few dozen
//! files and the models a handful, so the walk is milliseconds. A ledger kept beside every write
//! would cost more than it saves at this size.

use std::path::{Path, PathBuf};

use serde::Serialize;
use steamgauge_core::{embed, meaning, reader, search_models, state::CrawlState, writer};
use tauri::{AppHandle, Manager as _};

use super::{
    library_dir, shelf, text,
    work::{Job, State, Task, Work},
};

/// The room a game's files take, by what removing each costs.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub(super) struct Parts {
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
    const fn add(&mut self, part: Part, bytes: u64) {
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

    const fn plus(mut self, other: &Self) -> Self {
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

/// What a file in a game's newest download is, by its path within the download's folder: the
/// name the core writes it under, and the search's own folder.
///
/// `claims.parquet` is named although nothing writes it now: a download made by an earlier
/// build can hold it, and it goes with the game's readings.
fn part_of(within: &Path) -> Part {
    let name = within
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let left_over = within.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("partial") || extension.eq_ignore_ascii_case("part")
    });
    if left_over || name.contains(".partial.") || name.contains(".recount.") {
        Part::Partial
    } else if name.contains("embeddings") || within.starts_with(meaning::DIR) {
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
pub(super) fn bytes_under(path: &Path) -> u64 {
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
                parts.add(part_of(file.strip_prefix(&path).unwrap_or(&file)), bytes);
            }
        } else {
            parts.add(Part::Earlier, bytes_under(&path));
        }
    }
    parts
}

/// One game and the room it takes.
#[derive(Debug, Clone, Serialize)]
pub(super) struct GameRoom {
    pub app_id: u32,
    pub name: String,
    pub parts: Parts,
    pub total: u64,
}

/// One model in the cache and the room it takes.
#[derive(Debug, Clone, Serialize)]
pub(super) struct ModelRoom {
    /// What the model is called, as the cockpit names it.
    pub name: String,
    /// How the window asks for it to be removed.
    pub key: &'static str,
    pub role: String,
    pub bytes: u64,
    /// Whether this computer reads or searches with it, so removing it means fetching it again.
    pub used: bool,
}

/// A drive the app keeps something on.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Drive {
    pub name: String,
    pub total: u64,
    pub free: u64,
    /// What of the app's is on it: the library, the models, or both.
    pub holds: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct Storage {
    pub library: String,
    pub models: String,
    pub drives: Vec<Drive>,
    /// The library's parts across every game, its own records included.
    pub parts: Parts,
    pub games: Vec<GameRoom>,
    pub model_rooms: Vec<ModelRoom>,
    /// Games the removal that gave this answer left, each with why. Empty after any other call.
    pub spared: Vec<Spared>,
}

/// A game a removal left alone, and why.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct Spared {
    pub app_id: u32,
    pub why: String,
}

/// The models a computer can hold, each with the folder it lives in.
fn models(cache: &Path, reader_in_use: &str, writing: bool) -> Vec<(ModelRoom, PathBuf)> {
    let mut found: Vec<(ModelRoom, PathBuf)> = reader::SIZES
        .iter()
        .map(|size| {
            let home = size.home();
            (
                ModelRoom {
                    name: super::settings::reader_name(size),
                    key: size.name,
                    role: super::settings::reader_role(size),
                    bytes: bytes_under(&home),
                    used: size.name == reader_in_use,
                },
                home,
            )
        })
        .collect();
    for (model, role) in [
        (search_models::ENCODER, "finds what was said in other words"),
        (search_models::RERANKER, "orders search results by meaning"),
    ] {
        let home = model.dir(cache);
        found.push((
            ModelRoom {
                name: format!("SteamGauge {}", model.name.replace('-', " ")),
                key: model.name,
                role: role.to_owned(),
                bytes: bytes_under(&home),
                used: true,
            },
            home,
        ));
    }
    let home = writer::WRITER.dir(cache);
    found.push((
        ModelRoom {
            name: "SteamGauge writer".to_owned(),
            key: writer::WRITER.name,
            role: "writes what stands out".to_owned(),
            bytes: bytes_under(&home),
            used: writing,
        },
        home,
    ));
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
        // Two folders on one volume report the same totals. On Unix there is no letter to tell
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
        model_rooms: models(&cache, reader_in_use, writer::Choice::load(&library).write)
            .into_iter()
            .map(|(room, _)| room)
            .collect(),
        spared: Vec::new(),
    }
}

#[tauri::command]
pub(super) async fn storage(app: AppHandle) -> Storage {
    tauri::async_runtime::spawn_blocking(move || measure(&app))
        .await
        .unwrap_or_else(|_| Storage {
            library: String::new(),
            models: String::new(),
            drives: Vec::new(),
            parts: Parts::default(),
            games: Vec::new(),
            model_rooms: Vec::new(),
            spared: Vec::new(),
        })
}

/// What a person can ask to have removed from a game.
#[derive(Debug, Clone, Copy, serde::Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Removal {
    /// Its readings, which a read makes again.
    Reads,
    /// Its preparation for search by meaning.
    Search,
    /// Its downloads from before the newest one.
    Earlier,
    /// What a stopped download, read or recount left behind.
    Partial,
    /// The whole game, with its crawl records and its place in groups.
    Game,
}

const fn active(job: &Job) -> bool {
    matches!(job.state, State::Queued | State::Running)
}

/// Why a model may not be removed now, if it may not.
fn model_busy(jobs: &[Job], key: &str) -> Option<String> {
    let mut active_jobs = jobs.iter().filter(|job| active(job));
    if active_jobs
        .clone()
        .any(|job| matches!(job.task, Task::Read { .. } | Task::Prepare { .. }))
    {
        return Some(
            "A read or a preparation is running or waiting. Models are removed once it is done."
                .to_owned(),
        );
    }
    active_jobs
        .any(|job| matches!(&job.task, Task::FetchModel { model } if model == key))
        .then(|| {
            "A fetch of this model is waiting or running. Models are removed once it is done."
                .to_owned()
        })
}

/// Whether a file is one that programs lock. They stay, since removing one would let a program
/// that has it open lock a file no other program finds.
fn is_lock(file: &Path) -> bool {
    file.extension()
        .is_some_and(|extension| extension == "lock")
}

/// Removes every file under a folder but the lock files. Folders stay.
fn remove_files_under(dir: &Path) -> std::io::Result<()> {
    let mut files = Vec::new();
    files_under(dir, &mut files);
    for (file, _) in files {
        if !is_lock(&file) {
            std::fs::remove_file(file)?;
        }
    }
    Ok(())
}

/// A refusal to take a game's folder, worded for the storage page.
fn refusal(refused: steamgauge_core::Error) -> std::io::Error {
    match refused {
        steamgauge_core::Error::Refused(_) => {
            std::io::Error::other("Another job is writing in this game.")
        }
        other => std::io::Error::other(other),
    }
}

/// Removes the chosen part of one game, or of every game when none is named.
fn remove_from_game(library: &Path, app_id: u32, what: Removal) -> std::io::Result<()> {
    let _alone = steamgauge_core::crawl::lock_for_removal(library, app_id).map_err(refusal)?;
    let dir = game_dir(library, app_id);
    let newest = embed::latest_snapshot(library, app_id).ok();
    match what {
        Removal::Game => {
            remove_files_under(&dir)?;
            if let Ok(state) = CrawlState::open(&library.join("state.sqlite")) {
                state.forget(app_id).map_err(std::io::Error::other)?;
            }
            super::cockpit::drop_from_groups(library, app_id)?;
            super::since::forget(library, app_id).map_err(std::io::Error::other)?;
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
                    "Its newest download is unfinished, so the earlier one is kept until that download completes.",
                ));
            }
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let path = entry.path();
                let is_snapshot = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("snapshot="));
                if is_snapshot && newest.as_deref() != Some(path.as_path()) {
                    remove_files_under(&path)?;
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
                if part_of(file.strip_prefix(&newest).unwrap_or(&file)) == wanted {
                    std::fs::remove_file(file)?;
                }
            }
        }
    }
    Ok(())
}

/// Removes a part from each game. A game that refuses is left and listed, and the rest are done.
fn remove_from_each(library: &Path, games: &[u32], what: Removal) -> Vec<Spared> {
    games
        .iter()
        .filter_map(|&app_id| {
            remove_from_game(library, app_id, what)
                .err()
                .map(|failure| Spared {
                    app_id,
                    why: failure.to_string(),
                })
        })
        .collect()
}

/// A refused removal from one named game is an error. Across every game, the games left are the
/// answer.
fn spared_or_refused(named: Option<u32>, spared: Vec<Spared>) -> Result<Vec<Spared>, String> {
    match (named, spared.first()) {
        (Some(_), Some(first)) => Err(first.why.clone()),
        _ => Ok(spared),
    }
}

#[tauri::command]
pub(super) async fn free_room(
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
        if let Some(why) = work.busy(app_id) {
            return Err(why);
        }
    }
    let removing = targets.clone();
    let spared =
        tauri::async_runtime::spawn_blocking(move || remove_from_each(&library, &removing, what))
            .await
            .map_err(text)?;
    let spared = spared_or_refused(app_id, spared)?;
    Ok(Storage {
        spared,
        ..storage(app).await
    })
}

#[tauri::command]
pub(super) async fn remove_model(
    app: AppHandle,
    work: tauri::State<'_, Work>,
    key: String,
) -> Result<Storage, String> {
    if let Some(why) = model_busy(&work.jobs(), &key) {
        return Err(why);
    }
    let cache = steamgauge_core::model::default_cache_dir();
    let reader_in_use = super::work::reader_here(&super::settings::Settings::load(&app)).name;
    let library = library_dir(&app);
    let Some((_, home)) = models(&cache, reader_in_use, false)
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
    let writer_goes = is_the_writer(&key);
    let models = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // A model still loaded holds its files open, and Windows removes no file held open.
        models.state::<super::Models>().let_go();
        take_away(&home, &library, writer_goes)
    })
    .await
    .map_err(text)??;
    Ok(storage(app).await)
}

/// Whether the model called `key` is the writer, whose removal turns writing off.
fn is_the_writer(key: &str) -> bool {
    key == writer::WRITER.name
}

/// Empties a model's folder, and turns the writer off when it was the writer's. A fetch holding
/// the folder refuses the removal and leaves writing as it was.
fn take_away(home: &Path, library: &Path, writer_goes: bool) -> Result<(), String> {
    if !steamgauge_core::model::remove_fetched(home).map_err(text)? {
        return Err(
            "This model is being fetched, here or by a command in a terminal. \
            Models are removed once it is done."
                .to_owned(),
        );
    }
    // Still asked for, the writer would be fetched again by the next game prepared.
    if writer_goes {
        writer::Choice { write: false }
            .save(library)
            .map_err(text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Part, Parts, Removal, Spared, game_parts, is_the_writer, model_busy, part_of,
        remove_from_each, remove_from_game, spared_or_refused, take_away,
    };
    use crate::ui::work::{Job, State, Task, Unit};
    use std::path::{Path, PathBuf};
    use steamgauge_core::writer;

    #[test]
    fn only_the_writers_removal_turns_writing_off() {
        assert!(is_the_writer(writer::WRITER.name));
        assert!(!is_the_writer("search"));
    }

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
        file(&game.join("snapshot=200/meaning/part-00000.parquet"), 60);
        file(&game.join("snapshot=200/meaning/meaning.json"), 4);
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
            ("meaning/part-00000.parquet", Part::Search),
            ("meaning/meaning.json", Part::Search),
            ("readings.partial.parquet", Part::Partial),
            ("readings.recount.parquet", Part::Partial),
            ("sweep-1.parquet.partial", Part::Partial),
            ("meaning/part-00001.parquet.partial", Part::Partial),
            ("model.onnx.part", Part::Partial),
            ("part-00000.parquet", Part::Other),
            ("groups.json", Part::Other),
        ] {
            assert_eq!(part_of(Path::new(name)), part, "{name}");
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
                search: 467,
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
        assert_eq!((left.reads, left.reviews, left.search), (0, 111, 467));
    }

    #[test]
    fn removing_the_search_takes_its_preparation_and_leaves_the_rest() {
        let scratch = Scratch::new("search");
        game(&scratch.0);
        remove_from_game(&scratch.0, 7, Removal::Search).unwrap();
        let left = game_parts(&scratch.0, 7);
        assert_eq!(
            (left.search, left.reads, left.reviews, left.partial),
            (0, 302, 111, 25)
        );
        assert!(
            !scratch
                .0
                .join("appid=7/snapshot=200/meaning/part-00000.parquet")
                .exists()
        );
    }

    /// Every file under a folder, named from it, sorted.
    fn files_of(dir: &Path) -> Vec<String> {
        let mut found = Vec::new();
        super::files_under(dir, &mut found);
        let mut names: Vec<String> = found
            .into_iter()
            .map(|(path, _)| {
                path.strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        names.sort();
        names
    }

    /// Whether `groups.json` in the library names the game.
    fn grouped(library: &Path, app_id: u32) -> bool {
        let groups: serde_json::Value =
            serde_json::from_slice(&std::fs::read(library.join("groups.json")).unwrap()).unwrap();
        groups["groups"].as_array().unwrap().iter().any(|group| {
            group["app_ids"]
                .as_array()
                .unwrap()
                .contains(&app_id.into())
        })
    }

    fn name_game_in_groups(library: &Path, app_id: u32) {
        let groups = serde_json::json!({"groups": [{"name": "Kept", "app_ids": [app_id]}]});
        file(&library.join("groups.json"), 0);
        std::fs::write(library.join("groups.json"), groups.to_string()).unwrap();
    }

    #[test]
    fn removing_earlier_downloads_keeps_the_newest() {
        let scratch = Scratch::new("earlier");
        game(&scratch.0);
        remove_from_game(&scratch.0, 7, Removal::Earlier).unwrap();
        assert_eq!(earlier_files(&scratch.0, 7), 0);
        let left = game_parts(&scratch.0, 7);
        assert_eq!((left.reviews, left.earlier), (111, 0));

        let eight = scratch.0.join("appid=8");
        file(&eight.join("snapshot=5/crawl.json"), 1);
        file(&eight.join("snapshot=5/sweep.lock"), 0);
        file(&eight.join("snapshot=6/crawl.json"), 1);
        remove_from_game(&scratch.0, 8, Removal::Earlier).unwrap();
        assert_eq!(files_of(&eight), ["snapshot=5/sweep.lock", "writing.lock"]);
    }

    #[test]
    fn removing_a_game_leaves_only_its_lock_files() {
        let scratch = Scratch::new("game");
        game(&scratch.0);
        let seven = scratch.0.join("appid=7");
        file(&seven.join("crawl.lock"), 0);
        file(&seven.join("snapshot=200/sweep.lock"), 0);
        remove_from_game(&scratch.0, 7, Removal::Game).unwrap();
        assert_eq!(
            files_of(&seven),
            ["crawl.lock", "snapshot=200/sweep.lock", "writing.lock"]
        );
    }

    #[test]
    fn nothing_is_removed_from_a_game_another_job_writes_in() {
        let scratch = Scratch::new("writing");
        game(&scratch.0);
        let state =
            steamgauge_core::state::CrawlState::open(&scratch.0.join("state.sqlite")).unwrap();
        state.begin_crawl(7, 200, "", 0).unwrap();
        name_game_in_groups(&scratch.0, 7);
        let seven = scratch.0.join("appid=7");
        let other = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(seven.join("writing.lock"))
            .unwrap();
        other.try_lock_shared().unwrap();
        let before = files_of(&seven);
        let parts = game_parts(&scratch.0, 7);

        for what in [
            Removal::Game,
            Removal::Earlier,
            Removal::Reads,
            Removal::Search,
            Removal::Partial,
        ] {
            let refused = remove_from_game(&scratch.0, 7, what).unwrap_err();
            assert!(
                refused.to_string().contains("Another job is writing"),
                "{what:?}: {refused}"
            );
        }
        assert_eq!(files_of(&seven), before);
        assert_eq!(game_parts(&scratch.0, 7), parts);
        assert!(state.resumable(7).unwrap().is_some());
        assert!(grouped(&scratch.0, 7));

        drop(other);
        remove_from_game(&scratch.0, 7, Removal::Partial).unwrap();
        assert_eq!(game_parts(&scratch.0, 7).partial, 0);
    }

    #[test]
    fn a_state_that_cannot_be_opened_stops_no_removal() {
        let scratch = Scratch::new("no-state");
        game(&scratch.0);
        std::fs::create_dir_all(scratch.0.join("state.sqlite")).unwrap();
        name_game_in_groups(&scratch.0, 7);
        remove_from_game(&scratch.0, 7, Removal::Earlier).unwrap();
        assert_eq!(earlier_files(&scratch.0, 7), 0);
        remove_from_game(&scratch.0, 7, Removal::Game).unwrap();
        assert_eq!(files_of(&scratch.0.join("appid=7")), ["writing.lock"]);
        assert!(!grouped(&scratch.0, 7));
    }

    #[test]
    fn a_game_with_no_folder_is_still_forgotten() {
        let scratch = Scratch::new("no-folder");
        let state =
            steamgauge_core::state::CrawlState::open(&scratch.0.join("state.sqlite")).unwrap();
        state.begin_crawl(7, 200, "", 0).unwrap();
        name_game_in_groups(&scratch.0, 7);
        remove_from_game(&scratch.0, 7, Removal::Game).unwrap();
        assert!(state.resumable(7).unwrap().is_none());
        assert!(!grouped(&scratch.0, 7));
        assert_eq!(files_of(&scratch.0.join("appid=7")), ["writing.lock"]);
    }

    /// Games 7, 8 and 9 with an earlier and a newer download each, and a crawl open on game 8.
    fn three_games(library: &Path, unfinished: &[u32]) {
        for app_id in [7, 8, 9] {
            let game = library.join(format!("appid={app_id}"));
            file(&game.join("snapshot=100/shard-0000.parquet"), 50);
            file(&game.join("snapshot=200/shard-0000.parquet"), 100);
        }
        let state =
            steamgauge_core::state::CrawlState::open(&library.join("state.sqlite")).unwrap();
        for &app_id in unfinished {
            state.begin_crawl(app_id, 200, "", 0).unwrap();
        }
    }

    fn earlier_files(library: &Path, app_id: u32) -> usize {
        let mut found = Vec::new();
        super::files_under(
            &library.join(format!("appid={app_id}/snapshot=100")),
            &mut found,
        );
        found.len()
    }

    #[test]
    fn one_spared_game_does_not_stop_the_rest() {
        let scratch = Scratch::new("one-spared");
        three_games(&scratch.0, &[8]);
        let spared = remove_from_each(&scratch.0, &[7, 8, 9], Removal::Earlier);
        assert_eq!(spared.len(), 1);
        assert_eq!(spared[0].app_id, 8);
        assert!(spared[0].why.contains("is unfinished"));
        assert_eq!(earlier_files(&scratch.0, 7), 0);
        assert_eq!(earlier_files(&scratch.0, 8), 1);
        assert_eq!(earlier_files(&scratch.0, 9), 0);
    }

    #[test]
    fn no_game_left_gives_an_empty_list() {
        let scratch = Scratch::new("none-spared");
        three_games(&scratch.0, &[]);
        assert_eq!(
            remove_from_each(&scratch.0, &[7, 8, 9], Removal::Earlier),
            Vec::new()
        );
        assert_eq!(
            [7, 8, 9].map(|app_id| earlier_files(&scratch.0, app_id)),
            [0, 0, 0]
        );
    }

    #[test]
    fn every_game_left_is_listed_in_order() {
        let scratch = Scratch::new("all-spared");
        three_games(&scratch.0, &[7, 8, 9]);
        let spared = remove_from_each(&scratch.0, &[7, 8, 9], Removal::Earlier);
        assert_eq!(
            spared.iter().map(|one| one.app_id).collect::<Vec<_>>(),
            [7, 8, 9]
        );
        assert_eq!(
            [7, 8, 9].map(|app_id| earlier_files(&scratch.0, app_id)),
            [1, 1, 1]
        );
    }

    #[test]
    fn a_named_game_refused_is_an_error_and_every_game_gives_the_list() {
        let one = || {
            vec![Spared {
                app_id: 8,
                why: "x".to_owned(),
            }]
        };
        assert_eq!(spared_or_refused(Some(8), one()), Err("x".to_owned()));
        assert_eq!(spared_or_refused(None, one()), Ok(one()));
        assert_eq!(spared_or_refused(Some(8), Vec::new()), Ok(Vec::new()));
        assert_eq!(spared_or_refused(None, Vec::new()), Ok(Vec::new()));
    }

    fn job(task: Task, state: State) -> Job {
        Job {
            id: 0,
            task,
            name: String::new(),
            state,
            step: String::new(),
            unit: Unit::Reviews,
            done: 0.0,
            total: None,
            rate: None,
            left: None,
            note: None,
            queued: 0,
            started: None,
            ended: None,
            background: false,
        }
    }

    #[test]
    fn a_removal_refused_by_a_fetch_leaves_the_files_and_writing_on() {
        let scratch = Scratch::new("model-held");
        let home = scratch.0.join("model");
        let library = scratch.0.join("library");
        file(&home.join("model.onnx"), 5);
        std::fs::create_dir_all(&library).unwrap();
        writer::Choice { write: true }.save(&library).unwrap();
        let held = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(home.join("fetch.lock"))
            .unwrap();
        held.lock().unwrap();

        let refused = take_away(&home, &library, true).unwrap_err();
        assert!(refused.contains("being fetched"), "{refused}");
        assert!(home.join("model.onnx").is_file());
        assert!(writer::Choice::load(&library).write);

        drop(held);
        take_away(&home, &library, true).unwrap();
        assert!(!home.join("model.onnx").exists());
        assert!(home.join("fetch.lock").is_file());
        assert!(!writer::Choice::load(&library).write);
    }

    #[test]
    fn a_model_is_not_removed_while_a_fetch_of_it_waits_or_runs() {
        let fetch = |model: &str| Task::FetchModel {
            model: model.to_owned(),
        };
        let read = || Task::Read {
            app_id: 7,
            language: None,
        };
        let fetch_sentence =
            "A fetch of this model is waiting or running. Models are removed once it is done.";
        let read_sentence =
            "A read or a preparation is running or waiting. Models are removed once it is done.";
        for (state, busy) in [
            (State::Queued, true),
            (State::Running, true),
            (State::Done, false),
            (State::Failed, false),
            (State::Stopped, false),
        ] {
            let jobs = [job(fetch("writer"), state)];
            assert_eq!(
                model_busy(&jobs, "writer").as_deref(),
                busy.then_some(fetch_sentence),
                "{state:?}"
            );
            assert_eq!(model_busy(&jobs, "small"), None, "{state:?} other model");
        }

        for task in [read(), Task::Prepare { app_id: 7 }] {
            let state = if matches!(task, Task::Read { .. }) {
                State::Queued
            } else {
                State::Running
            };
            assert_eq!(
                model_busy(&[job(task, state)], "writer").as_deref(),
                Some(read_sentence)
            );
        }
        for state in [State::Done, State::Failed, State::Stopped] {
            assert_eq!(
                model_busy(&[job(read(), state)], "writer"),
                None,
                "{state:?}"
            );
        }
        let download = job(Task::Download { app_id: 7 }, State::Queued);
        assert_eq!(model_busy(&[download], "writer"), None);

        let reading = job(read(), State::Queued);
        let fetching = job(fetch("writer"), State::Queued);
        assert_eq!(
            model_busy(&[reading.clone(), fetching.clone()], "writer").as_deref(),
            Some(read_sentence)
        );
        assert_eq!(
            model_busy(&[fetching, reading], "writer").as_deref(),
            Some(read_sentence)
        );
    }
}
