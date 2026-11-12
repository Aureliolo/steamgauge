//! Everything the app does that takes longer than a click, as jobs on three lanes.
//!
//! Downloads talk to Steam and wait on its pace. Reads and preparations hold the graphics card.
//! Each lane runs one job at a time, so a crawl that takes an hour never holds up a read and two
//! reads never fight over the card. A job never starts on a game another running job is
//! working on: a read walking a capture while an update writes to it would count a corpus that
//! is changing under it.
//!
//! The window is a view of this board. It asks for the board once and is sent it again whenever
//! a job moves, so leaving a page and coming back loses nothing, and a job keeps running
//! whichever page is open.
//!
//! Some jobs the app starts by itself: asking Steam about the library, and keeping its games up
//! to date. Those never run beside work the person started. While any of theirs waits or runs,
//! nothing of the app's own starts, and one of its own already running is stopped where it can
//! stop and waits again until theirs is done.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter as _, Manager as _};
use tokio::sync::Notify;

use super::{library_dir, now_unix, settings::Settings, text};

/// What a job is asked to do.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Task {
    /// Every review of a game. A download resumes where an earlier one stopped.
    Download { app_id: u32 },
    /// What was written or edited since a game was last downloaded, then a read of it again
    /// where it had been read.
    Update { app_id: u32 },
    /// Every review of a game read by the reader, in one language or all of them.
    Read {
        app_id: u32,
        language: Option<String>,
    },
    /// Counts a read game again from its saved readings, without the reader. Takes about a
    /// minute.
    Recount { app_id: u32 },
    /// A read game made searchable by meaning.
    Prepare { app_id: u32 },
    /// How many reviews each game has on Steam now, and whether newer models are published.
    Check,
    /// A report of these games, written as one page to `to`.
    Export {
        app_ids: Vec<u32>,
        to: std::path::PathBuf,
    },
    /// A read game's figures and claims, written as CSV and JSON into a new folder at `to`.
    ExportData { app_id: u32, to: std::path::PathBuf },
    /// A model fetched ahead of the work that needs it, by the name Storage lists it under: a
    /// reader size (`small`, `standard`), `search-encoder`, `search-reranker` or `writer`. A first
    /// game's reviews and its reader download at the same time this way, and a computer can hold
    /// every model before it goes offline.
    FetchModel { model: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Network,
    Machine,
    /// Writing pages from what is on disk, which waits on neither Steam nor the card.
    Files,
}

const LANES: [Lane; 3] = [Lane::Network, Lane::Machine, Lane::Files];

impl Task {
    const fn lane(&self) -> Lane {
        match self {
            Self::Download { .. } | Self::Update { .. } | Self::Check | Self::FetchModel { .. } => {
                Lane::Network
            }
            Self::Read { .. } | Self::Prepare { .. } => Lane::Machine,
            Self::Export { .. } | Self::ExportData { .. } | Self::Recount { .. } => Lane::Files,
        }
    }

    /// Every game the task reads or writes, a repeated one left in.
    pub(super) const fn games(&self) -> &[u32] {
        match self {
            Self::Download { app_id }
            | Self::Update { app_id }
            | Self::Read { app_id, .. }
            | Self::Recount { app_id }
            | Self::Prepare { app_id }
            | Self::ExportData { app_id, .. } => std::slice::from_ref(app_id),
            Self::Export { app_ids, .. } => app_ids.as_slice(),
            Self::Check | Self::FetchModel { .. } => &[],
        }
    }

    /// The game of a task that uses exactly one, which is what names it.
    pub(super) const fn one_game(&self) -> Option<u32> {
        match self.games() {
            [one] => Some(*one),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    Queued,
    Running,
    Done,
    Failed,
    Stopped,
}

/// What a job's count is a count of.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Unit {
    Bytes,
    Reviews,
    Points,
    Pages,
    Games,
}

/// One job as the window draws it.
#[derive(Debug, Clone, Serialize)]
pub(super) struct Job {
    pub id: u64,
    pub task: Task,
    /// The game's name, or what the job is about where it is about no one game.
    pub name: String,
    pub state: State,
    /// What it is doing now, in words.
    pub step: String,
    pub unit: Unit,
    pub done: f64,
    /// Where the count ends, where that is known before it gets there.
    pub total: Option<f64>,
    /// Units a second, over the last several seconds.
    pub rate: Option<f64>,
    /// Seconds to go at that rate, where there is a total.
    pub left: Option<f64>,
    /// How it ended, or what it is waiting on.
    pub note: Option<String>,
    pub queued: i64,
    pub started: Option<i64>,
    pub ended: Option<i64>,
    /// Started by the app rather than by the person, and so stepping aside for their work.
    pub background: bool,
}

/// How fast a count is moving, smoothed so one slow batch does not swing the time left by
/// minutes.
#[derive(Debug, Clone)]
pub(super) struct Meter {
    step: String,
    at: Instant,
    done: f64,
    rate: Option<f64>,
}

/// Samples closer together than this say more about when the progress callback fired than
/// about how fast the work goes.
const SAMPLE: Duration = Duration::from_secs(1);

/// The weight a new sample carries against the rate so far.
const NEWEST: f64 = 0.3;

impl Meter {
    pub(super) fn new(step: &str, done: f64, at: Instant) -> Self {
        Self {
            step: step.to_owned(),
            at,
            done,
            rate: None,
        }
    }

    /// The rate after seeing `done` at `now`. A new step, or a count that went backwards,
    /// starts the measurement again.
    pub(super) fn note(&mut self, step: &str, done: f64, now: Instant) -> Option<f64> {
        if step != self.step || done < self.done {
            *self = Self::new(step, done, now);
            return None;
        }
        let elapsed = now.saturating_duration_since(self.at);
        if elapsed < SAMPLE {
            return self.rate;
        }
        let sampled = (done - self.done) / elapsed.as_secs_f64();
        self.rate = Some(
            self.rate
                .map_or(sampled, |rate| rate * (1.0 - NEWEST) + sampled * NEWEST),
        );
        self.at = now;
        self.done = done;
        self.rate
    }
}

/// Seconds to go, where there is a total and the count is moving.
pub(super) fn left(done: f64, total: Option<f64>, rate: Option<f64>) -> Option<f64> {
    let (total, rate) = (total?, rate?);
    (rate > 0.0).then(|| (total - done).max(0.0) / rate)
}

/// Finished jobs kept on the board, newest first, so the cockpit can say what just happened.
const KEPT: usize = 30;

/// How often a moving job is sent to the window. Faster than this repaints a bar no eye
/// follows. Slower makes a download look stuck.
pub(super) const EVERY: Duration = Duration::from_millis(250);

#[derive(Default)]
struct Board {
    jobs: Vec<Job>,
    next: u64,
    meters: HashMap<u64, Meter>,
    stops: HashMap<u64, Arc<AtomicBool>>,
    /// Jobs of the app's own told to stop for the person's work, which wait again rather than
    /// ending.
    yielding: HashSet<u64>,
    /// Games whose files a removal is taking away. No job starts on one.
    held: Vec<u32>,
    sent: Option<Instant>,
}

/// A removal of these games' files under way. It holds them until dropped.
pub(super) struct Removing<'a> {
    work: &'a Work,
    games: Vec<u32>,
}

impl Removing<'_> {
    pub(super) fn games(&self) -> &[u32] {
        &self.games
    }
}

impl Drop for Removing<'_> {
    fn drop(&mut self) {
        self.work
            .lock()
            .held
            .retain(|game| !self.games.contains(game));
        self.work.ring();
    }
}

/// What a job of the app's own says while it waits.
const WAITS_FOR_YOURS: &str = "Started by the app, and gives way to any work you start.";

impl Board {
    /// Adds a task to wait its turn, or hands back the same task already waiting or running. A
    /// task the person asks for is theirs, even where the app had put it there first.
    fn put(&mut self, task: Task, name: String, background: bool, at: i64) -> u64 {
        if let Some(same) = self
            .jobs
            .iter_mut()
            .find(|job| job.task == task && matches!(job.state, State::Queued | State::Running))
        {
            if same.background && !background {
                same.background = false;
                if same.state == State::Queued {
                    same.note = None;
                }
            }
            return same.id;
        }
        self.next += 1;
        self.jobs.push(Job {
            id: self.next,
            task,
            name,
            state: State::Queued,
            step: "Waiting".to_owned(),
            unit: Unit::Reviews,
            done: 0.0,
            total: None,
            rate: None,
            left: None,
            note: background.then(|| WAITS_FOR_YOURS.to_owned()),
            queued: at,
            started: None,
            ended: None,
            background,
        });
        self.next
    }

    /// Records how a job ended at `at`, and gives the one game it used and whether the app had
    /// started it by itself.
    fn end(&mut self, id: u64, ended: &Result<Ended, String>, at: i64) -> (Option<u32>, bool) {
        self.meters.remove(&id);
        let stopped = self
            .stops
            .remove(&id)
            .is_some_and(|stop| stop.load(Ordering::Relaxed));
        let yielded = self.yielding.remove(&id);
        let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) else {
            return (None, false);
        };
        job.ended = Some(at);
        job.rate = None;
        job.left = None;
        match ended {
            Err(_) if yielded => job.wait_again(),
            Ok(ended) => {
                job.state = State::Done;
                job.note = Some(ended.note.clone());
                if let Some(total) = job.total {
                    job.done = total;
                }
            }
            Err(_) if stopped => {
                job.state = State::Stopped;
                job.note = Some(stopped_note(&job.task).to_owned());
            }
            Err(failure) => {
                job.state = State::Failed;
                job.note = Some(failure.clone());
            }
        }
        let said = (job.task.one_game(), job.background);
        // The newest finished jobs stay, so the cockpit can say what just happened. The job that
        // just ended is the newest, also where others ended in the same second.
        let mut finished: Vec<(bool, i64, u64)> = self
            .jobs
            .iter()
            .filter(|job| !matches!(job.state, State::Queued | State::Running))
            .map(|job| (job.id == id, job.ended.unwrap_or_default(), job.id))
            .collect();
        finished.sort_unstable_by(|a, b| b.cmp(a));
        let dropped: HashSet<u64> = finished.into_iter().skip(KEPT).map(|(.., id)| id).collect();
        self.jobs.retain(|job| !dropped.contains(&job.id));
        said
    }

    /// Gives a game's name to each of its jobs that had none.
    fn name(&mut self, app_id: u32, name: &str) {
        for job in &mut self.jobs {
            if job.name.is_empty() && job.task.one_game() == Some(app_id) {
                name.clone_into(&mut job.name);
            }
        }
    }
}

impl Job {
    /// Back to waiting, as a job of the app's own stopped for the person's work.
    fn wait_again(&mut self) {
        self.state = State::Queued;
        "Waiting".clone_into(&mut self.step);
        self.done = 0.0;
        self.total = None;
        self.started = None;
        self.ended = None;
        self.note = Some(WAITS_FOR_YOURS.to_owned());
    }
}

/// Whether a job is dropped where it stands when stopped: a crawl records each window only once
/// it is whole and resumes from those, a check writes nothing until it ends, and a model file
/// takes its name only once it is whole.
const fn droppable(task: &Task) -> bool {
    matches!(
        task,
        Task::Download { .. } | Task::Check | Task::FetchModel { .. }
    )
}

/// The board and a bell for each lane.
#[derive(Default)]
pub(super) struct Work {
    board: Arc<Mutex<Board>>,
    network: Arc<Notify>,
    machine: Arc<Notify>,
    files: Arc<Notify>,
}

/// The names the store gave this session, for games whose download has not written one to disk
/// yet; `None` while the store is being asked.
fn store_names() -> std::sync::MutexGuard<'static, HashMap<u32, Option<String>>> {
    static NAMES: std::sync::OnceLock<Mutex<HashMap<u32, Option<String>>>> =
        std::sync::OnceLock::new();
    NAMES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What a job is about, in words: the game's name on disk or as the store gave it. None for a
/// game whose name nobody has yet, which the store is then asked for.
fn name_of(app: &AppHandle, task: &Task) -> Option<String> {
    name_in(&library_dir(app), task)
}

fn name_in(library: &std::path::Path, task: &Task) -> Option<String> {
    if let Task::FetchModel { model } = task {
        return Some(Fetchable::named(model).map_or_else(|_| model.clone(), Fetchable::title));
    }
    let Some(app_id) = task.one_game() else {
        return Some("Your library".to_owned());
    };
    steamgauge_core::report::crawl_facts(library, app_id)
        .ok()
        .map(|facts| facts.name)
        .filter(|name| !name.is_empty())
        .or_else(|| store_names().get(&app_id).cloned().flatten())
}

/// Asks the store for a game's name and gives it to every job of that game still without one.
/// A game's number is never shown in its place, so the store is asked again for a while when it
/// does not answer.
fn name_from_store(app: &AppHandle, app_id: u32) {
    if store_names().insert(app_id, None).is_some() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for wait in [0, 10, 60, 300, 1800] {
            tokio::time::sleep(Duration::from_secs(wait)).await;
            let Ok(store) = super::store() else { continue };
            if let Some(name) = store.name(app_id).await {
                store_names().insert(app_id, Some(name.clone()));
                let work = app.state::<Work>();
                work.lock().name(app_id, &name);
                work.send(&app, true);
                return;
            }
        }
        store_names().remove(&app_id);
    });
}

impl Work {
    const fn bell(&self, lane: Lane) -> &Arc<Notify> {
        match lane {
            Lane::Network => &self.network,
            Lane::Machine => &self.machine,
            Lane::Files => &self.files,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Board> {
        // A job that panicked while holding the board left it as it was. The board is a list
        // of what is happening, and carrying on with it is better than refusing every job.
        self.board
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Puts a task the person asked for on the board, unless the same task is already waiting or
    /// running, which then becomes theirs.
    pub(super) fn queue(&self, app: &AppHandle, task: Task, name: Option<String>) -> u64 {
        self.queue_as(app, task, name, false)
    }

    /// Puts a task the app starts by itself on the board, to wait behind the person's work.
    pub(super) fn queue_background(&self, app: &AppHandle, task: Task) -> u64 {
        self.queue_as(app, task, None, true)
    }

    fn queue_as(&self, app: &AppHandle, task: Task, name: Option<String>, background: bool) -> u64 {
        let lane = task.lane();
        let name = name.or_else(|| name_of(app, &task));
        let unnamed = name.is_none().then(|| task.one_game()).flatten();
        let id = self
            .lock()
            .put(task, name.unwrap_or_default(), background, now_unix());
        if let Some(app_id) = unnamed {
            name_from_store(app, app_id);
        }
        if !background {
            self.make_way();
        }
        self.bell(lane).notify_one();
        self.send(app, true);
        id
    }

    /// Stops every job of the app's own that is running, to wait again once the person's work
    /// is done. One already told to stop is left ending as it was told.
    fn make_way(&self) {
        let mut board = self.lock();
        let running: Vec<u64> = board
            .jobs
            .iter()
            .filter(|job| job.background && job.state == State::Running)
            .map(|job| job.id)
            .filter(|id| {
                !board
                    .stops
                    .get(id)
                    .is_some_and(|stop| stop.load(Ordering::Relaxed))
            })
            .collect();
        for id in running {
            board.yielding.insert(id);
            if let Some(stop) = board.stops.get(&id) {
                stop.store(true, Ordering::Relaxed);
            }
        }
    }

    /// Stops a job: a waiting one never starts, and a running one is told to stop where it
    /// can. It keeps what it finished.
    ///
    /// A download, a check and a model fetch are dropped where they stand, and a read or a
    /// preparation while it fetches its models. `droppable` gives the reasons.
    /// Everything else is only told, and stops at a point where what it leaves is consistent,
    /// letting go of its games only then, not before another job could start on the same files.
    /// A waiting job taken off lets go of its games at once, so every lane is rung. A job of the
    /// app's own that was stepping aside ends instead of waiting again.
    pub(super) fn stop(&self, id: u64) {
        let taken_off = {
            let mut board = self.lock();
            board.yielding.remove(&id);
            if let Some(stop) = board.stops.get(&id) {
                stop.store(true, Ordering::Relaxed);
            }
            if let Some(job) = board.jobs.iter_mut().find(|job| job.id == id)
                && job.state == State::Queued
            {
                job.state = State::Stopped;
                job.ended = Some(now_unix());
                job.note = Some("Taken off the list before it started.".to_owned());
                true
            } else {
                false
            }
        };
        if taken_off {
            self.ring();
        }
    }

    /// Takes finished jobs off the board.
    pub(super) fn clear(&self, app: &AppHandle) {
        self.lock()
            .jobs
            .retain(|job| matches!(job.state, State::Queued | State::Running));
        self.send(app, true);
    }

    pub(super) fn jobs(&self) -> Vec<Job> {
        self.lock().jobs.clone()
    }

    /// Holds these games for a removal of their files and says which may not be touched now, and
    /// why. Asking and holding are one step, so no job starts between them.
    pub(super) fn hold(&self, games: &[u32]) -> (Removing<'_>, Vec<(u32, String)>) {
        let mut board = self.lock();
        let mut held = Vec::new();
        let mut left = Vec::new();
        for &game in games {
            if held.contains(&game) {
                continue;
            }
            if let Some(job) = board
                .jobs
                .iter()
                .filter(|job| matches!(job.state, State::Queued | State::Running))
                .find(|job| job.task.games().contains(&game))
            {
                left.push((
                    game,
                    format!("This game is busy: {}.", job.step.to_lowercase()),
                ));
            } else if board.held.contains(&game) {
                left.push((
                    game,
                    "This game is busy: its files are being removed.".to_owned(),
                ));
            } else {
                held.push(game);
            }
        }
        board.held.extend(&held);
        drop(board);
        (
            Removing {
                work: self,
                games: held,
            },
            left,
        )
    }

    /// Sends the board to the window, at most every [`EVERY`] unless something changed state.
    fn send(&self, app: &AppHandle, now: bool) {
        let jobs = {
            let mut board = self.lock();
            if !now && board.sent.is_some_and(|sent| sent.elapsed() < EVERY) {
                return;
            }
            board.sent = Some(Instant::now());
            board.jobs.clone()
        };
        let _ = app.emit("work", jobs);
    }

    /// The first waiting job of this lane with no game in use and none a job queued before it
    /// waits to use, marked running. A read queued with its game's download waits for it. A job
    /// of no game waits for none. The person's work waits only for their own.
    fn take(&self, lane: Lane) -> Option<(u64, Task, Arc<AtomicBool>)> {
        let mut board = self.lock();
        let busy: Vec<u32> = board
            .jobs
            .iter()
            .filter(|job| job.state == State::Running)
            .flat_map(|job| job.task.games().iter().copied())
            .chain(board.held.iter().copied())
            .collect();
        let theirs = board
            .jobs
            .iter()
            .any(|job| !job.background && matches!(job.state, State::Queued | State::Running));
        let jobs = &board.jobs;
        let at = (0..jobs.len()).find(|&at| {
            let job = &jobs[at];
            job.state == State::Queued
                && job.task.lane() == lane
                && !(job.background && theirs)
                && job.task.games().iter().all(|game| {
                    !busy.contains(game)
                        && !jobs[..at].iter().any(|before| {
                            before.state == State::Queued
                                && before.task.games().contains(game)
                                && (job.background || !before.background)
                        })
                })
        })?;
        let job = &mut board.jobs[at];
        job.state = State::Running;
        job.started = Some(now_unix());
        "Starting".clone_into(&mut job.step);
        job.note = None;
        let (id, task) = (job.id, job.task.clone());
        let stop = Arc::new(AtomicBool::new(false));
        board.stops.insert(id, Arc::clone(&stop));
        Some((id, task, stop))
    }

    /// Records how far a running job has got.
    fn progress(
        &self,
        app: &AppHandle,
        id: u64,
        step: &str,
        unit: Unit,
        done: f64,
        total: Option<f64>,
    ) {
        {
            let mut board = self.lock();
            let now = Instant::now();
            let rate = board
                .meters
                .entry(id)
                .or_insert_with(|| Meter::new(step, done, now))
                .note(step, done, now);
            if let Some(job) = board.jobs.iter_mut().find(|job| job.id == id) {
                if job.step != step {
                    job.note = None;
                }
                step.clone_into(&mut job.step);
                job.unit = unit;
                job.done = done;
                job.total = total;
                job.rate = rate;
                job.left = left(done, total, rate);
            }
        }
        self.send(app, false);
    }

    /// Something the job is waiting on, shown under it until its next step.
    fn note(&self, app: &AppHandle, id: u64, note: String) {
        if let Some(job) = self.lock().jobs.iter_mut().find(|job| job.id == id) {
            job.note = Some(note);
        }
        self.send(app, true);
    }

    /// Records how a job ended, and says whether the app had started it by itself.
    fn finish(&self, app: &AppHandle, id: u64, ended: &Result<Ended, String>) -> bool {
        let (game, background) = self.lock().end(id, ended, now_unix());
        self.send(app, true);
        // The one game the job used, so a page showing another one is left as it is.
        if ended.is_ok() {
            let _ = app.emit("library", game);
        }
        // A finished job frees its games for the other lanes.
        self.ring();
        background
    }

    fn ring(&self) {
        for lane in LANES {
            self.bell(lane).notify_one();
        }
    }

    /// Where a finished report or a game's data was saved. Only what this app wrote can be opened
    /// from the window, never a path the window names.
    fn saved(&self, id: u64) -> Option<std::path::PathBuf> {
        self.lock().jobs.iter().find_map(|job| match &job.task {
            Task::Export { to, .. } | Task::ExportData { to, .. }
                if job.id == id && job.state == State::Done =>
            {
                Some(to.clone())
            }
            _ => None,
        })
    }
}

const fn stopped_note(task: &Task) -> &'static str {
    match task {
        Task::Download { .. } => {
            "Stopped. What was downloaded is kept, and downloading again carries on from there."
        }
        Task::Update { .. } => "Stopped. Updating again asks Steam again.",
        Task::Read { .. } => "Stopped. The reading from before is kept as it was.",
        Task::Recount { .. } => "Stopped. The counts from before are kept as they were.",
        Task::Prepare { .. } => "Stopped. What was done is kept, and preparing again carries on.",
        Task::Check => "Stopped.",
        Task::Export { .. } => "Stopped before the report was saved.",
        Task::ExportData { .. } => "Stopped. Nothing was saved.",
        Task::FetchModel { .. } => {
            "Stopped. Its files already fetched are kept, and the rest comes when it is first needed."
        }
    }
}

/// How a job ended well, and what it leaves to do next.
struct Ended {
    note: String,
    then: Vec<Task>,
}

impl Ended {
    fn said(note: impl Into<String>) -> Self {
        Self {
            note: note.into(),
            then: Vec::new(),
        }
    }
}

/// Starts every lane. Each waits for its bell, runs what it can, and waits again.
pub(super) fn start(app: &AppHandle) {
    for lane in LANES {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let next = app.state::<Work>().take(lane);
                let Some((id, task, stop)) = next else {
                    let bell = Arc::clone(app.state::<Work>().bell(lane));
                    bell.notified().await;
                    continue;
                };
                app.state::<Work>().send(&app, true);
                let read = match task {
                    Task::Read { app_id, .. } => Some(app_id),
                    _ => None,
                };
                let ended = run(&app, id, task, stop).await;
                let work = app.state::<Work>();
                let background = work.finish(&app, id, &ended);
                if let Ok(ended) = ended {
                    // What follows a job is started by whoever started the job.
                    for then in ended.then {
                        work.queue_as(&app, then, None, background);
                    }
                    if background && let Some(app_id) = read {
                        super::since::after_background_read(&app, app_id);
                    }
                }
            }
        });
    }
}

/// Runs a job in a task of its own. A droppable job is dropped where it stands once it is told
/// to stop.
async fn run(app: &AppHandle, id: u64, task: Task, stop: Arc<AtomicBool>) -> Result<Ended, String> {
    let dropped_by = droppable(&task).then(|| Arc::clone(&stop));
    let app = app.clone();
    in_own_task(dropped_by, async move {
        match task {
            Task::Download { app_id } => download(&app, id, app_id).await,
            Task::Update { app_id } => update(&app, id, app_id, stop).await,
            Task::Read { app_id, language } => read(&app, id, app_id, language, stop).await,
            Task::Recount { app_id } => recount(&app, id, app_id, stop).await,
            Task::Prepare { app_id } => prepare(&app, id, app_id, stop).await,
            Task::Check => check(&app, id).await,
            Task::Export { app_ids, to } => export(&app, id, (app_ids, to), stop).await,
            Task::ExportData { app_id, to } => export_data(&app, id, app_id, to, stop).await,
            Task::FetchModel { model } => fetch_model(&app, id, &model).await,
        }
    })
    .await
}

/// Runs a job's work in a task of its own, so work that panics ends as failed and its lane goes
/// on. Given a stop flag, the work is dropped inside that task once the flag is set, and nothing
/// of it runs on.
async fn in_own_task<T: Send + 'static>(
    stop: Option<Arc<AtomicBool>>,
    work: impl Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    let job = tokio::spawn(async move {
        match stop {
            Some(stop) => unless_stopped(&stop, work).await,
            None => work.await,
        }
    });
    match job.await {
        Ok(ended) => ended,
        Err(joined) => Err(format!("the job failed: {joined}")),
    }
}

/// Runs work that may be dropped where it stands, and drops it once its job is told to stop.
async fn unless_stopped<T>(
    stop: &AtomicBool,
    work: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::select! {
        ended = work => ended,
        () = async {
            while !stop.load(Ordering::Relaxed) {
                tokio::time::sleep(EVERY).await;
            }
        } => Err("stopped".to_owned()),
    }
}

/// Reports a running job's progress.
fn tell(app: &AppHandle, id: u64, step: &str, unit: Unit, done: f64, total: Option<f64>) {
    app.state::<Work>()
        .progress(app, id, step, unit, done, total);
}

/// A Steam client that puts a refusal on the job rather than leaving a bar standing still.
fn steam(app: &AppHandle, id: u64) -> Result<steamgauge_core::SteamClient, String> {
    let app = app.clone();
    Ok(
        steamgauge_core::SteamClient::new(steamgauge_core::DEFAULT_PACE)
            .map_err(text)?
            .with_notice(move |wait, status| {
                app.state::<Work>().note(
                    &app,
                    id,
                    format!(
                        "Steam is refusing requests ({status}); asking again in {} s.",
                        wait.as_secs()
                    ),
                );
            }),
    )
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts shown on a bar are far below 2^53"
)]
const fn float(count: u64) -> f64 {
    count as f64
}

/// Shards fetched at once. Pacing is global, so this reorders work rather than leaning harder
/// on Valve, and matches what the pipeline uses when nobody says otherwise.
const SHARDS_AT_ONCE: usize = 4;

async fn download(app: &AppHandle, id: u64, app_id: u32) -> Result<Ended, String> {
    let out_dir = library_dir(app);
    std::fs::create_dir_all(&out_dir).map_err(text)?;
    let options = steamgauge_core::CrawlOptions {
        out_dir: out_dir.clone(),
        concurrency: SHARDS_AT_ONCE,
        shard_target: steamgauge_core::DEFAULT_SHARD_TARGET,
        resume: true,
    };
    let client = steam(app, id)?;
    let telling = app.clone();
    let report = steamgauge_core::crawl(&client, app_id, &options, move |progress| {
        tell(
            &telling,
            id,
            "Downloading reviews",
            Unit::Reviews,
            float(progress.unique),
            (progress.valve_total > 0).then(|| float(progress.valve_total)),
        );
    })
    .await
    .map_err(text)?;
    tell(
        app,
        id,
        "Asking Steam for its updates",
        Unit::Pages,
        0.0,
        None,
    );
    let mut ended = Ended::said(format!(
        "{} reviews downloaded.{}",
        thousands(report.unique),
        refresh_updates(&client, app_id, &out_dir).await
    ));
    let settings = Settings::load(app);
    if settings.read_after_download {
        ended.then.push(Task::Read {
            app_id,
            language: settings.language,
        });
    }
    Ok(ended)
}

async fn update(
    app: &AppHandle,
    id: u64,
    app_id: u32,
    stop: Arc<AtomicBool>,
) -> Result<Ended, String> {
    let out_dir = library_dir(app);
    let client = steam(app, id)?;
    let telling = app.clone();
    tell(app, id, "Asking Steam what changed", Unit::Pages, 0.0, None);
    let swept = steamgauge_core::crawl::sweep(&client, app_id, &out_dir, &stop, move |progress| {
        tell(
            &telling,
            id,
            "Fetching what changed",
            Unit::Pages,
            f64::from(progress.pages),
            None,
        );
    })
    .await
    .map_err(text)?;
    tell(
        app,
        id,
        "Asking Steam for its updates",
        Unit::Pages,
        0.0,
        None,
    );
    let updates = refresh_updates(&client, app_id, &out_dir).await;
    let since = steamgauge_core::time::day(swept.watermark);
    if swept.rows == 0 {
        return Ok(Ended::said(format!(
            "Nothing was written or edited since {since}.{updates}"
        )));
    }
    let mut ended = Ended::said(format!(
        "{} new and {} edited since {since}.{updates}",
        thousands(swept.new),
        thousands(swept.edited)
    ));
    // Counts over a corpus that has since changed are counts over a corpus nobody can open, so
    // a game that had been read is read again, in the language it was read in.
    if let Ok(snapshot) = steamgauge_core::embed::latest_snapshot(&out_dir, app_id)
        && let Ok(reading) = super::read_report(&snapshot)
    {
        ended.then.push(Task::Read {
            app_id,
            language: reading.language,
        });
    }
    Ok(ended)
}

/// Asks Steam for what the game's developer has posted and keeps it beside the reviews. The
/// reviews are the job: where Steam will not list the posts, what was kept before stays and the
/// job says so in a sentence of its own, which is empty when all went well.
async fn refresh_updates(
    client: &steamgauge_core::SteamClient,
    app_id: u32,
    out_dir: &std::path::Path,
) -> String {
    match steamgauge_core::updates::refresh(client, app_id, out_dir, now_unix()).await {
        Ok(_) => String::new(),
        Err(error) => format!(" Steam did not list its updates: {error}."),
    }
}

/// The reader size this machine reads with: the card decides where one is reached, and the
/// person's choice counts where the machine reads on its processor.
pub(super) fn reader_here(settings: &Settings) -> &'static steamgauge_core::reader::Size {
    use steamgauge_core::reader::{Size, fits};

    let card = steamgauge_core::card::largest();
    let reaches = steamgauge_core::model::REACHES_A_CARD;
    settings
        .reader
        .as_deref()
        .and_then(Size::named)
        .filter(|chosen| runs_here(chosen, card, reaches))
        .unwrap_or_else(|| fits(card, reaches))
}

/// Whether a size can read on this machine: any size on a processor, and on a card only a size
/// the card's memory holds, since a larger one would stop part way through a read.
pub(super) fn runs_here(
    size: &steamgauge_core::reader::Size,
    card: Option<steamgauge_core::card::Card>,
    reaches: bool,
) -> bool {
    use steamgauge_core::reader::{fits, on_the_processor};
    on_the_processor(card, reaches) || size.needs <= fits(card, reaches).needs
}

/// Downloads that add up across files: each file reports its own count from zero, and the bar
/// is of all of them together.
struct Across {
    base: u64,
    file: String,
    last: u64,
}

impl Across {
    const fn new() -> Self {
        Self {
            base: 0,
            file: String::new(),
            last: 0,
        }
    }

    fn add(&mut self, file: &str, downloaded: u64) -> u64 {
        if file != self.file {
            self.base += self.last;
            file.clone_into(&mut self.file);
            self.last = 0;
        }
        self.last = downloaded;
        self.base + downloaded
    }
}

/// Fetches the reader where it is not on this computer, on one bar across its files. A reader
/// in the working tree is never replaced: that is how one is tried before it is published.
async fn fetch_reader(
    app: &AppHandle,
    id: u64,
    size: &'static steamgauge_core::reader::Size,
) -> Result<(), String> {
    let model_dir = size.home();
    if size.here(&model_dir) {
        return Ok(());
    }
    if !size.published.is_pinned() {
        return Err(format!(
            "no {} reader is on this computer and none has been published yet",
            size.name
        ));
    }
    let telling = app.clone();
    let mut across = Across::new();
    let total = float(size.published.bytes_left(&model_dir));
    tell(
        app,
        id,
        "Fetching the reader",
        Unit::Bytes,
        0.0,
        Some(total),
    );
    steamgauge_core::reader::ensure(size, &model_dir, move |progress| {
        let done = float(across.add(progress.file, progress.downloaded));
        let saying = if done >= total {
            "Checking the reader"
        } else {
            "Fetching the reader"
        };
        tell(&telling, id, saying, Unit::Bytes, done, Some(total));
    })
    .await
    .map_err(text)
}

/// A model a job can fetch, by the name Storage lists it under.
#[derive(Debug, Clone, Copy)]
enum Fetchable {
    Reader(&'static steamgauge_core::reader::Size),
    Search(steamgauge_core::search_models::Model),
    Writer,
}

impl Fetchable {
    fn named(name: &str) -> Result<Self, String> {
        use steamgauge_core::{
            reader::Size,
            search_models::{ENCODER, RERANKER},
            writer::WRITER,
        };

        if let Some(size) = Size::named(name) {
            return Ok(Self::Reader(size));
        }
        [ENCODER, RERANKER]
            .into_iter()
            .find(|model| model.name == name)
            .map(Self::Search)
            .or_else(|| (name == WRITER.name).then_some(Self::Writer))
            .ok_or_else(|| format!("No model is called {name}."))
    }

    fn title(self) -> String {
        match self {
            Self::Reader(size) => format!("The {} reader", size.name),
            Self::Search(model) => format!("The {}", model.name.replace('-', " ")),
            Self::Writer => "The writer".to_owned(),
        }
    }
}

/// One bar across a model's files. It fetches until the last byte is in and checks after.
fn bar(
    app: &AppHandle,
    id: u64,
    total: f64,
    what: &str,
) -> impl FnMut(steamgauge_core::model::DownloadProgress<'_>) + use<> {
    let telling = app.clone();
    let mut across = Across::new();
    let (fetching, checking) = (format!("Fetching {what}"), format!("Checking {what}"));
    let total = a_total(total);
    match total {
        Some(total) => tell(app, id, &fetching, Unit::Bytes, 0.0, Some(total)),
        None => tell(app, id, &checking, Unit::Bytes, 0.0, None),
    }
    move |progress| {
        let (saying, done, of) = told(total, &mut across, progress, [&fetching, &checking]);
        tell(&telling, id, saying, Unit::Bytes, done, of);
    }
}

/// The total a bar counts towards. A total of nothing is none, so the bar tells no figure.
fn a_total(total: f64) -> Option<f64> {
    (total > 0.0).then_some(total)
}

/// Whether preparing a game fetches the search models, given whether a fetch has hashed each.
const fn search_models_wanted(encoder_hashed: bool, reranker_hashed: bool) -> bool {
    !(encoder_hashed && reranker_hashed)
}

/// Whether preparing a game fetches the writer.
const fn writer_wanted(writing: bool, hashed: bool) -> bool {
    writing && !hashed
}

/// The bytes the two search models would fetch, as a bar's total.
fn search_total(encoder_left: u64, reranker_left: u64) -> Option<f64> {
    a_total(float(encoder_left + reranker_left))
}

/// A report with the file named as the whole fetch counts it.
const fn named<'a>(
    file: &'a str,
    progress: steamgauge_core::model::DownloadProgress<'_>,
) -> steamgauge_core::model::DownloadProgress<'a> {
    steamgauge_core::model::DownloadProgress {
        file,
        downloaded: progress.downloaded,
        total: progress.total,
    }
}

/// What a bar says of one report from a fetch. With a total across the files, it counts every
/// file towards it. With none, bytes that arrive all the same are counted against that file's
/// own length.
fn told<'a>(
    total: Option<f64>,
    across: &mut Across,
    progress: steamgauge_core::model::DownloadProgress<'_>,
    [fetching, checking]: [&'a str; 2],
) -> (&'a str, f64, Option<f64>) {
    let Some(total) = total else {
        return (
            fetching,
            float(progress.downloaded),
            progress.total.map(float),
        );
    };
    let done = float(across.add(progress.file, progress.downloaded));
    (
        if done >= total { checking } else { fetching },
        done,
        Some(total),
    )
}

/// Drops the kept encoder and reranker after a fetch replaced a search model's files. Off the
/// async threads, since it waits for a search in flight.
async fn let_go_of_search_models(app: &AppHandle) -> Result<(), String> {
    let models = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        models.state::<super::Models>().let_go_of_search();
    })
    .await
    .map_err(text)
}

/// What follows a search model's fetch. When bytes came the kept models are let go, whether the
/// fetch ended well or not, and the fetch's own error is the one passed on.
async fn after_a_search_fetch(
    fetched: Result<(), String>,
    bytes_came: bool,
    let_go: impl Future<Output = Result<(), String>>,
) -> Result<(), String> {
    if !bytes_came {
        return fetched;
    }
    let let_go = let_go.await;
    fetched.and(let_go)
}

/// A flag a fetch's progress closure sets when it is told of bytes.
#[derive(Clone, Default)]
struct BytesCame(Arc<AtomicBool>);

impl BytesCame {
    fn set(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether bytes came since the last call.
    fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

/// Fetches one model where it is not on this computer yet. One already here is left as it is. A
/// search model it had to download is loaded afresh at its next use.
async fn fetch_model(app: &AppHandle, id: u64, name: &str) -> Result<Ended, String> {
    let model = Fetchable::named(name)?;
    let cache = steamgauge_core::model::default_cache_dir();
    let title = model.title();
    let what = title.to_lowercase();
    match model {
        Fetchable::Reader(size) => fetch_reader(app, id, size).await?,
        Fetchable::Search(search) => {
            let total = float(search.bytes_left(&cache));
            let came = BytesCame::default();
            let (seen, mut telling) = (came.clone(), bar(app, id, total, &what));
            let fetched = search
                .ensure(&cache, move |progress| {
                    seen.set();
                    telling(progress);
                })
                .await
                .map_err(text);
            after_a_search_fetch(fetched, came.take(), let_go_of_search_models(app)).await?;
        }
        Fetchable::Writer => {
            let writer = steamgauge_core::writer::WRITER;
            let total = float(writer.bytes_left(&cache));
            writer
                .ensure(&cache, bar(app, id, total, &what))
                .await
                .map_err(text)?;
        }
    }
    Ok(Ended::said(format!("{title} is on this computer.")))
}

/// The one fact about the game the reader is told, asked of the store once and kept.
async fn know_the_game(
    app: &AppHandle,
    id: u64,
    app_id: u32,
    out_dir: &std::path::Path,
) -> Result<(), String> {
    let game_dir = out_dir.join(format!("appid={app_id}"));
    if steamgauge_core::facts::Facts::load(&game_dir).is_some() {
        return Ok(());
    }
    tell(
        app,
        id,
        "Asking the store about the game",
        Unit::Reviews,
        0.0,
        None,
    );
    let headset_only = steam(app, id)?.headset_only(app_id).await.ok_or_else(|| {
        "the store would not say whether this game is played in a VR headset".to_owned()
    })?;
    steamgauge_core::facts::Facts { headset_only }
        .save(&game_dir)
        .map_err(text)
}

async fn read(
    app: &AppHandle,
    id: u64,
    app_id: u32,
    language: Option<String>,
    stop: Arc<AtomicBool>,
) -> Result<Ended, String> {
    let settings = Settings::load(app);
    let out_dir = library_dir(app);
    if steamgauge_core::embed::latest_snapshot(&out_dir, app_id).is_err() {
        return Err(
            "this game has no reviews downloaded yet: download it, then read it".to_owned(),
        );
    }
    let capture = float(
        steamgauge_core::read::finished_download(&out_dir, app_id)
            .map_err(text)?
            .rows_unique,
    );
    let size = reader_here(&settings);
    let model_dir = size.home();
    unless_stopped(&stop, fetch_reader(app, id, size)).await?;
    know_the_game(app, id, app_id, &out_dir).await?;
    let options = steamgauge_core::read::ReadOptions {
        out_dir: out_dir.clone(),
        language,
        card_share: settings.gpu_share,
        stop,
        ..steamgauge_core::read::ReadOptions::default()
    };
    let telling = app.clone();
    tell(
        app,
        id,
        "Loading the reader",
        Unit::Reviews,
        0.0,
        Some(capture),
    );
    let models = app.state::<super::Models>();
    let (reader, encoder) = (models.reader.clone(), models.encoder.clone());
    let report = tauri::async_runtime::spawn_blocking(move || {
        // From before the model loads: the estimate is of the wait, and loading is part of it.
        let started = Instant::now();
        // Reads and preparations take turns on the card, and one sized for the reader need not
        // hold the encoder beside it.
        encoder.let_go();
        let mut held = reader.lock()?;
        let model = held.get(&model_dir, steamgauge_core::reader::ClaimReader::load)?;
        let report = steamgauge_core::read::read_corpus(model, app_id, &options, |progress| {
            tell(
                &telling,
                id,
                "Reading every review",
                Unit::Reviews,
                float(progress.reviews_walked),
                Some(capture),
            );
        })
        .map_err(text)?;
        let snapshot =
            steamgauge_core::embed::latest_snapshot(&options.out_dir, app_id).map_err(text)?;
        report.save(&snapshot.join("reading.json")).map_err(text)?;
        // A read that kept answers took the time of the claims it asked, not of the game, and
        // would teach a pace no fresh read keeps.
        if model.device() == "cpu" && report.kept == 0 {
            let mut times = steamgauge_core::reading_time::ReadingTimes::load(&options.out_dir);
            times.note(
                size,
                options.language.as_deref(),
                started.elapsed().as_secs_f64(),
                report.corpus_reviews,
            );
            times.save(&options.out_dir).map_err(text)?;
        }
        Ok::<_, String>(report)
    })
    .await
    .map_err(text)??;

    let mut ended = Ended::said(format!(
        "{} reviews read, {} claims.",
        thousands(report.reviews),
        thousands(report.claims)
    ));
    if steamgauge_core::meaning::Choice::load(&out_dir).every_game {
        ended.then.push(Task::Prepare { app_id });
    }
    Ok(ended)
}

/// The reader on this computer that answered a reading, known by the lines it draws: a recount
/// adds up what that reader declined, so no other reader will do.
fn answering_reader(lines: &str) -> Option<steamgauge_core::reader::Provenance> {
    steamgauge_core::reader::SIZES
        .iter()
        .filter_map(|size| steamgauge_core::reader::Provenance::load(&size.home()).ok())
        .find(|provenance| provenance.lines_fingerprint == lines)
}

async fn recount(
    app: &AppHandle,
    id: u64,
    app_id: u32,
    stop: Arc<AtomicBool>,
) -> Result<Ended, String> {
    let out_dir = library_dir(app);
    let snapshot = steamgauge_core::embed::latest_snapshot(&out_dir, app_id).map_err(text)?;
    let earlier = super::read_report(&snapshot)?;
    let provenance = answering_reader(&earlier.read_by_rule).ok_or_else(|| {
        "the reader that read this game is no longer on this computer, so it has to be read again"
            .to_owned()
    })?;
    let total = float(earlier.corpus_reviews);
    tell(app, id, "Counting again", Unit::Reviews, 0.0, Some(total));
    let telling = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let report = steamgauge_core::read::recount_corpus(
            &out_dir,
            app_id,
            steamgauge_core::read::ReadOptions::default().top_helpful,
            &provenance,
            &stop,
            |progress| {
                tell(
                    &telling,
                    id,
                    "Counting again",
                    Unit::Reviews,
                    float(progress.reviews_walked),
                    Some(total),
                );
            },
        )
        .map_err(text)?;
        report.save(&snapshot.join("reading.json")).map_err(text)?;
        Ok::<_, String>(report)
    })
    .await
    .map_err(text)??;
    Ok(Ended::said(format!(
        "{} reviews counted again.",
        thousands(report.reviews)
    )))
}

async fn prepare(
    app: &AppHandle,
    id: u64,
    app_id: u32,
    stop: Arc<AtomicBool>,
) -> Result<Ended, String> {
    use steamgauge_core::{
        meaning::{Times, prepare},
        search_models::SearchEncoder,
        themes,
    };

    let dir = library_dir(app);
    let writing = steamgauge_core::writer::Choice::load(&dir).write;
    unless_stopped(&stop, fetch_for_prepare(app, id, writing)).await?;

    let share = Settings::load(app).gpu_share;
    let models = app.state::<super::Models>();
    let (encoder, reader) = (models.encoder.clone(), models.reader.clone());
    let telling = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Ended, String> {
        let snapshot = steamgauge_core::embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let reading = super::read_report(&snapshot)?;
        let total = float(reading.claims);
        tell(
            &telling,
            id,
            "Loading the search model",
            Unit::Points,
            0.0,
            Some(total),
        );
        reader.let_go();
        let mut slot = encoder.lock()?;
        let encoder = slot.get(
            &steamgauge_core::model::default_cache_dir(),
            SearchEncoder::load,
        )?;
        let on_card = encoder.device() != "cpu";
        let started = Instant::now();
        let prepared = prepare(
            &snapshot,
            |texts| encoder.claims(texts),
            (&stop, share),
            |walked| {
                tell(
                    &telling,
                    id,
                    "Preparing for search",
                    Unit::Points,
                    float(walked),
                    Some(total),
                );
            },
        )
        .map_err(text)?;
        let mut times = Times::load(&dir);
        times.note(on_card, started.elapsed().as_secs_f64(), prepared.walked);
        times.save(&dir).map_err(text)?;
        drop(slot);
        if !prepared.finished {
            return Err("stopped".to_owned());
        }
        // From the vectors just made, so it runs no model and takes seconds beside the minutes
        // of preparing.
        tell(
            &telling,
            id,
            "Finding what players say",
            Unit::Points,
            total,
            Some(total),
        );
        let found = themes::find(&snapshot, themes::Closeness::USED).map_err(text)?;
        found.save(&snapshot).map_err(text)?;
        if !writing {
            return Ok(Ended::said(
                "Ready to search by meaning, with what players praise and complain about found.",
            ));
        }

        let game = steamgauge_core::report::crawl_facts(&dir, app_id)
            .map_err(text)?
            .title();
        write_paragraph(
            &telling,
            id,
            &snapshot,
            &game,
            reading.reviews,
            &found,
            &stop,
        )
    })
    .await
    .map_err(text)?
}

/// Every model preparing a game needs that no fetch has hashed yet, fetched and hashed against
/// its pins: the search models always, and the writer once somebody has asked for paragraphs.
async fn fetch_for_prepare(app: &AppHandle, id: u64, writing: bool) -> Result<(), String> {
    use steamgauge_core::{
        search_models::{ENCODER, RERANKER},
        writer::WRITER,
    };

    let cache = steamgauge_core::model::default_cache_dir();
    if search_models_wanted(ENCODER.fetched(&cache), RERANKER.fetched(&cache)) {
        let total = search_total(ENCODER.bytes_left(&cache), RERANKER.bytes_left(&cache));
        if total.is_none() {
            tell(
                app,
                id,
                "Checking the search models",
                Unit::Bytes,
                0.0,
                None,
            );
        }
        let mut across = Across::new();
        let came = BytesCame::default();
        for model in [ENCODER, RERANKER] {
            let telling = app.clone();
            let across = &mut across;
            let seen = came.clone();
            let fetched = model
                .ensure(&cache, move |progress| {
                    seen.set();
                    let file = format!("{}/{}", model.name, progress.file);
                    let progress = named(&file, progress);
                    let (saying, done, of) = told(
                        total,
                        across,
                        progress,
                        ["Fetching the search models", "Checking the search models"],
                    );
                    tell(&telling, id, saying, Unit::Bytes, done, of);
                })
                .await
                .map_err(text);
            after_a_search_fetch(fetched, came.take(), let_go_of_search_models(app)).await?;
        }
    }
    if writer_wanted(writing, WRITER.fetched(&cache)) {
        WRITER
            .ensure(
                &cache,
                bar(app, id, float(WRITER.bytes_left(&cache)), "the writer"),
            )
            .await
            .map_err(text)?;
    }
    Ok(())
}

/// Writes a game's paragraph from the themes just found and keeps what the check leaves of it.
fn write_paragraph(
    telling: &AppHandle,
    id: u64,
    snapshot: &std::path::Path,
    game: &str,
    reviews: u64,
    found: &steamgauge_core::themes::Themes,
    stop: &AtomicBool,
) -> Result<Ended, String> {
    use steamgauge_core::writer;

    let Some(brief) = writer::brief(game, reviews, found) else {
        return Ok(Ended::said(
            "Ready to search by meaning. Nothing is said in enough reviews to write about.",
        ));
    };
    tell(
        telling,
        id,
        "Writing what stands out",
        Unit::Points,
        0.0,
        None,
    );
    let mut loaded =
        writer::Writer::load(&steamgauge_core::model::default_cache_dir()).map_err(text)?;
    let Some(written) = loaded.answer(&brief.prompt, stop).map_err(text)? else {
        return Err("stopped".to_owned());
    };
    // A paragraph the check leaves nothing of is kept as none, so the page does not offer to
    // write one again for the same points.
    let kept = writer::checked(&written, &brief).unwrap_or_default();
    writer::Paragraph::new(kept, snapshot)
        .save(snapshot)
        .map_err(text)?;
    Ok(Ended::said(
        "Ready to search by meaning, with what players praise and complain about found and \
         written up.",
    ))
}

async fn check(app: &AppHandle, id: u64) -> Result<Ended, String> {
    let dir = library_dir(app);
    let games = super::shelf(&dir).games;
    let client = steam(app, id)?.with_patience(Duration::from_secs(60));
    let mut totals = super::cockpit::SteamTotals::load(&dir);
    let total = float(u64::try_from(games.len()).unwrap_or(u64::MAX));
    for (at, game) in (0_u64..).zip(&games) {
        tell(
            app,
            id,
            "Asking Steam how many reviews each game has",
            Unit::Games,
            float(at),
            Some(total),
        );
        if let Ok(summary) = client.summary(game.app_id).await {
            totals.games.insert(game.app_id, summary.total_reviews);
        }
        // A game downloaded before the crawler asked for names is listed by its number until
        // somebody asks. The store is being asked about it anyway.
        if game.name.starts_with("App ") {
            let _ = steamgauge_core::crawl::name_where_missing(&client, &dir, game.app_id).await;
        }
    }
    totals.checked = now_unix();
    totals.save(&dir).map_err(text)?;

    tell(
        app,
        id,
        "Asking whether newer models are published",
        Unit::Games,
        total,
        Some(total),
    );
    let releases = super::cockpit::Releases::fetch().await;
    releases.save(app).map_err(text)?;
    super::cockpit::keep_up_to_date(app);
    Ok(Ended::said("Checked every game against Steam."))
}

async fn export(
    app: &AppHandle,
    id: u64,
    (app_ids, to): (Vec<u32>, std::path::PathBuf),
    stop: Arc<AtomicBool>,
) -> Result<Ended, String> {
    let dir = library_dir(app);
    let games = float(u64::try_from(app_ids.len()).unwrap_or(u64::MAX));
    tell(app, id, "Writing the report", Unit::Games, 0.0, Some(games));
    let written = to.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let options = steamgauge_core::report::ReportOptions {
            out_dir: dir,
            ..steamgauge_core::report::ReportOptions::default()
        };
        let report = steamgauge_core::report::build(&app_ids, &options).map_err(text)?;
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".to_owned());
        }
        saved(&written, &steamgauge_core::html::render(&report))
    })
    .await
    .map_err(text)??;
    Ok(Ended::said(format!("Saved as {}.", to.display())))
}

/// Writes a file whole at `to`, by way of a `.partial` beside it, making the folders above it as
/// the path names them: an assistant queueing a report names a path rather than picking one that
/// exists in a dialog. What cannot be made or written is named.
fn saved(to: &std::path::Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{} cannot be made: {error}", parent.display()))?;
    }
    steamgauge_core::whole_file::write(to, contents.as_bytes())
        .map_err(|error| format!("{} cannot be written: {error}", to.display()))
}

async fn export_data(
    app: &AppHandle,
    id: u64,
    app_id: u32,
    to: std::path::PathBuf,
    stop: Arc<AtomicBool>,
) -> Result<Ended, String> {
    let dir = library_dir(app);
    tell(app, id, "Writing the data", Unit::Reviews, 0.0, None);
    let telling = app.clone();
    let exported = tauri::async_runtime::spawn_blocking(move || {
        steamgauge_core::export::game(&dir, app_id, &to, &stop, |walked, of| {
            tell(
                &telling,
                id,
                "Writing the claims",
                Unit::Reviews,
                float(walked),
                Some(float(of)),
            );
        })
        .map_err(text)
    })
    .await
    .map_err(text)??;
    let left_out = if exported.left_out > 0 {
        format!(
            " {} claims of reviews edited since they were read are left out.",
            thousands(exported.left_out)
        )
    } else {
        String::new()
    };
    Ok(Ended::said(format!(
        "Saved {} claims and the figures in {}.{left_out}",
        thousands(exported.points),
        exported.folder.display()
    )))
}

/// A count with thousands separated, as the window would print it.
pub(super) fn thousands(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub(super) fn work(work: tauri::State<'_, Work>) -> Vec<Job> {
    work.jobs()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub(super) fn queue(app: AppHandle, work: tauri::State<'_, Work>, tasks: Vec<Task>) -> Vec<u64> {
    tasks
        .into_iter()
        .map(|task| work.queue(&app, task, None))
        .collect()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub(super) fn stop_job(app: AppHandle, work: tauri::State<'_, Work>, id: u64) {
    work.stop(id);
    work.send(&app, true);
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub(super) fn clear_finished(app: AppHandle, work: tauri::State<'_, Work>) {
    work.clear(&app);
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub(super) fn open_report(
    app: AppHandle,
    work: tauri::State<'_, Work>,
    id: u64,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt as _;

    let path = work
        .saved(id)
        .ok_or("that report is not one this app saved, or it is not finished")?;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_search_fetch_lets_go_once_when_bytes_came_and_never_when_none_did() {
        let lets_go = |fetched: Result<(), String>, bytes_came| async move {
            let calls = std::sync::atomic::AtomicU32::new(0);
            let result = after_a_search_fetch(fetched, bytes_came, async {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            })
            .await;
            (result, calls.load(Ordering::Relaxed))
        };

        let (result, calls) = lets_go(Err("the second file failed".to_owned()), true).await;
        assert_eq!(result.unwrap_err(), "the second file failed");
        assert_eq!(calls, 1, "bytes then an error");

        let (result, calls) = lets_go(Err("no route".to_owned()), false).await;
        assert_eq!(result.unwrap_err(), "no route");
        assert_eq!(calls, 0, "no bytes");

        let (result, calls) = lets_go(Ok(()), false).await;
        result.unwrap();
        assert_eq!(calls, 0, "no bytes and no error");

        let (result, calls) = lets_go(Ok(()), true).await;
        result.unwrap();
        assert_eq!(calls, 1, "bytes then success");
    }

    #[test]
    fn bytes_that_came_are_told_once_per_file() {
        let came = BytesCame::default();
        assert!(!came.take(), "nothing came yet");
        let closure = came.clone();
        closure.set();
        assert!(came.take(), "a clone's progress closure told of bytes");
        assert!(!came.take(), "told once, so the next file starts afresh");
        assert!(!closure.take(), "the clone shares the one flag");
    }

    #[test]
    fn a_model_is_fetched_by_the_name_storage_lists_it_under_and_no_other() {
        let titled = |name: &str| Fetchable::named(name).map(Fetchable::title);
        assert_eq!(titled("small").unwrap(), "The small reader");
        assert_eq!(titled("standard").unwrap(), "The standard reader");
        assert_eq!(titled("search-encoder").unwrap(), "The search encoder");
        assert_eq!(titled("search-reranker").unwrap(), "The search reranker");
        assert_eq!(titled("writer").unwrap(), "The writer");
        assert_eq!(titled("huge").unwrap_err(), "No model is called huge.");
    }

    #[test]
    fn bytes_that_come_with_no_total_are_told_against_their_own_file() {
        let progress = |file, downloaded, total| steamgauge_core::model::DownloadProgress {
            file,
            downloaded,
            total,
        };
        let words = ["Fetching", "Checking"];
        let mut across = Across::new();
        assert_eq!(
            told(None, &mut across, progress("a", 40, Some(100)), words),
            ("Fetching", 40.0, Some(100.0))
        );
        assert_eq!(
            told(None, &mut across, progress("b", 7, None), words),
            ("Fetching", 7.0, None)
        );
        assert_eq!(
            told(
                Some(150.0),
                &mut across,
                progress("a", 100, Some(100)),
                words
            ),
            ("Fetching", 100.0, Some(150.0))
        );
        assert_eq!(
            told(Some(150.0), &mut across, progress("b", 50, Some(50)), words),
            ("Checking", 150.0, Some(150.0))
        );
    }

    #[test]
    fn a_preparation_fetches_what_no_fetch_has_hashed() {
        assert!(!search_models_wanted(true, true));
        assert!(search_models_wanted(true, false));
        assert!(search_models_wanted(false, true));
        assert!(search_models_wanted(false, false));
    }

    #[test]
    fn a_preparation_fetches_the_writer_only_when_asked_for_paragraphs() {
        assert!(writer_wanted(true, false));
        assert!(!writer_wanted(true, true));
        assert!(!writer_wanted(false, false));
        assert!(!writer_wanted(false, true));
    }

    #[test]
    fn a_total_of_nothing_is_told_as_none() {
        assert_eq!(a_total(0.0), None);
        assert_eq!(a_total(5.0), Some(5.0));
    }

    #[test]
    fn the_search_models_total_is_what_both_would_fetch() {
        assert_eq!(search_total(0, 0), None);
        assert_eq!(search_total(7, 3), Some(10.0));
        assert_eq!(search_total(0, 4), Some(4.0));
        assert_eq!(search_total(4, 0), Some(4.0));
    }

    #[test]
    fn a_report_is_counted_under_the_name_it_is_given() {
        let progress = steamgauge_core::model::DownloadProgress {
            file: "model.onnx",
            downloaded: 3,
            total: Some(9),
        };
        let named = named("search-encoder/model.onnx", progress);
        assert_eq!(
            (named.file, named.downloaded, named.total),
            ("search-encoder/model.onnx", 3, Some(9))
        );
    }

    #[test]
    fn fetching_a_model_waits_on_the_network_and_on_no_game() {
        let task = Task::FetchModel {
            model: "writer".to_owned(),
        };
        assert_eq!(task.lane(), Lane::Network);
        assert_eq!(task.games(), Vec::<u32>::new());
        assert_eq!(
            serde_json::to_value(&task).unwrap(),
            serde_json::json!({"kind": "fetch_model", "model": "writer"})
        );
    }

    #[test]
    fn a_steady_count_settles_on_its_rate_and_a_new_step_starts_again() {
        let start = Instant::now();
        let mut meter = Meter::new("Downloading", 0.0, start);
        assert_eq!(
            meter.note("Downloading", 50.0, start + Duration::from_millis(200)),
            None
        );
        let first = meter
            .note("Downloading", 100.0, start + Duration::from_secs(1))
            .unwrap();
        assert!((first - 100.0).abs() < 1e-9);
        let second = meter
            .note("Downloading", 300.0, start + Duration::from_secs(2))
            .unwrap();
        assert!(
            (second - (100.0 * 0.7 + 200.0 * 0.3)).abs() < 1e-9,
            "one fast second moves the rate a little, not all the way"
        );
        assert_eq!(
            meter.note("Reading", 0.0, start + Duration::from_secs(3)),
            None
        );
    }

    #[test]
    fn time_left_needs_a_total_and_a_moving_count() {
        assert_eq!(left(25.0, Some(100.0), Some(5.0)), Some(15.0));
        assert_eq!(left(25.0, None, Some(5.0)), None);
        assert_eq!(left(25.0, Some(100.0), Some(0.0)), None);
        assert_eq!(left(120.0, Some(100.0), Some(5.0)), Some(0.0));
    }

    #[test]
    fn files_downloaded_one_after_another_add_up_on_one_bar() {
        let mut across = Across::new();
        assert_eq!(across.add("tokenizer.json", 10), 10);
        assert_eq!(across.add("tokenizer.json", 30), 30);
        assert_eq!(across.add("model.onnx", 5), 35);
        assert_eq!(across.add("model.onnx", 500), 530);
        assert_eq!(across.add("reader.json", 2), 532);
    }

    fn read(app_id: u32) -> Task {
        Task::Read {
            app_id,
            language: None,
        }
    }

    /// Puts a task on a board, as the person or as the app.
    fn put(work: &Work, task: Task, background: bool) -> u64 {
        work.lock().put(task, String::new(), background, 0)
    }

    fn job(work: &Work, id: u64) -> Job {
        work.jobs().into_iter().find(|job| job.id == id).unwrap()
    }

    #[test]
    fn a_name_from_the_store_goes_to_that_game_s_unnamed_jobs_only() {
        let work = Work::default();
        let download = put(&work, Task::Download { app_id: 7 }, false);
        let named = work
            .lock()
            .put(Task::Update { app_id: 7 }, "Kept".to_owned(), false, 0);
        let other = put(&work, Task::Download { app_id: 8 }, false);
        let library = put(&work, Task::Check, false);
        work.lock().name(7, "Helldivers 2");
        assert_eq!(job(&work, download).name, "Helldivers 2");
        assert_eq!(job(&work, named).name, "Kept");
        assert_eq!(job(&work, other).name, "");
        assert_eq!(job(&work, library).name, "");
    }

    #[test]
    fn a_read_and_an_update_of_one_game_never_run_at_once() {
        let work = Work::default();
        put(&work, Task::Update { app_id: 7 }, false);
        assert!(work.take(Lane::Network).is_some());
        put(&work, read(7), false);
        let other = put(&work, read(8), false);
        let (id, _, _) = work.take(Lane::Machine).unwrap();
        assert_eq!(id, other, "the read of the game being updated waits");
        assert!(work.take(Lane::Machine).is_none());
    }

    #[test]
    fn a_report_is_saved_whole_into_folders_made_for_it_and_a_folder_it_cannot_make_is_named() {
        let scratch = std::env::temp_dir().join(format!("steamgauge-saved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let to = scratch.join("comparables").join("data").join("report.html");
        saved(&to, "<p>whole</p>").unwrap();
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "<p>whole</p>");
        assert!(!steamgauge_core::whole_file::beside(&to).exists());

        let a_file = scratch.join("a file");
        std::fs::write(&a_file, "").unwrap();
        let refused = saved(&a_file.join("report.html"), "").unwrap_err();
        assert!(refused.contains(&a_file.display().to_string()), "{refused}");
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn a_report_that_cannot_be_written_whole_leaves_the_one_before() {
        let scratch = std::env::temp_dir().join(format!("steamgauge-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        let to = scratch.join("report.html");
        saved(&to, "<p>before</p>").unwrap();
        std::fs::create_dir(steamgauge_core::whole_file::beside(&to)).unwrap();

        assert!(saved(&to, "<p>after</p>").is_err());
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "<p>before</p>");
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn a_read_queued_with_its_games_download_waits_for_it() {
        let work = Work::default();
        put(&work, Task::Download { app_id: 7 }, false);
        put(&work, Task::Download { app_id: 9 }, false);
        let first = put(&work, read(7), false);
        let other = put(&work, read(8), false);
        assert_eq!(
            work.take(Lane::Machine).map(|(id, _, _)| id),
            Some(other),
            "the read of a game still to be downloaded waits, though its own lane is free"
        );
        let (download, _, _) = work.take(Lane::Network).unwrap();
        work.lock().jobs.iter_mut().for_each(|job| {
            if job.id == download || job.id == other {
                job.state = State::Done;
            }
        });
        assert_eq!(work.take(Lane::Machine).map(|(id, _, _)| id), Some(first));
    }

    #[test]
    fn the_persons_work_does_not_wait_for_the_apps_own_queued_before_it() {
        let work = Work::default();
        put(&work, Task::Update { app_id: 7 }, true);
        let theirs = put(&work, read(7), false);
        assert_eq!(work.take(Lane::Machine).map(|(id, _, _)| id), Some(theirs));
    }

    #[test]
    fn the_apps_own_work_waits_while_the_persons_waits_or_runs() {
        let work = Work::default();
        let own = put(&work, read(8), true);
        let theirs = put(&work, Task::Download { app_id: 9 }, false);
        assert!(
            work.take(Lane::Machine).is_none(),
            "a download the person asked for is waiting, so the app's read does not start"
        );
        let (id, _, _) = work.take(Lane::Network).unwrap();
        assert_eq!(id, theirs);
        assert!(
            work.take(Lane::Machine).is_none(),
            "nor while it runs, on another lane"
        );
        work.lock().jobs.iter_mut().for_each(|job| {
            if job.id == theirs {
                job.state = State::Done;
            }
        });
        let (id, _, _) = work.take(Lane::Machine).unwrap();
        assert_eq!(id, own, "once theirs is done, the app's starts");
    }

    #[test]
    fn the_apps_own_work_runs_when_the_person_has_none() {
        let work = Work::default();
        let check = put(&work, Task::Check, true);
        let update = put(&work, Task::Update { app_id: 7 }, true);
        assert_eq!(work.take(Lane::Network).map(|(id, _, _)| id), Some(check));
        assert_eq!(job(&work, check).note, None, "running, it waits on nothing");
        assert_eq!(work.take(Lane::Network).map(|(id, _, _)| id), Some(update));
    }

    #[test]
    fn the_app_says_its_own_work_is_waiting_for_the_persons() {
        let work = Work::default();
        let own = put(&work, Task::Update { app_id: 7 }, true);
        let theirs = put(&work, Task::Update { app_id: 8 }, false);
        assert_eq!(job(&work, own).note.as_deref(), Some(WAITS_FOR_YOURS));
        assert!(job(&work, own).background);
        assert_eq!(job(&work, theirs).note, None);
        assert!(!job(&work, theirs).background);
    }

    #[test]
    fn a_task_the_person_asks_for_becomes_theirs() {
        let work = Work::default();
        let own = put(&work, Task::Update { app_id: 7 }, true);
        assert_eq!(put(&work, Task::Update { app_id: 7 }, false), own);
        let now = job(&work, own);
        assert!(!now.background);
        assert_eq!(now.note, None);
        assert_eq!(
            put(&work, Task::Update { app_id: 7 }, true),
            own,
            "and the app asking again does not take it back"
        );
        assert!(!job(&work, own).background);
        assert_eq!(work.jobs().len(), 1);
    }

    #[test]
    fn the_person_asking_for_work_stops_the_apps_own_and_only_that() {
        let work = Work::default();
        let own = put(&work, read(8), true);
        let (_, _, own_stop) = work.take(Lane::Machine).unwrap();
        let theirs = put(&work, Task::Update { app_id: 9 }, false);
        let (_, _, their_stop) = work.take(Lane::Network).unwrap();
        work.make_way();
        assert!(own_stop.load(Ordering::Relaxed));
        assert!(!their_stop.load(Ordering::Relaxed));
        let board = work.lock();
        assert!(board.yielding.contains(&own));
        assert!(!board.yielding.contains(&theirs));
    }

    #[test]
    fn a_job_that_stepped_aside_waits_again_from_the_start() {
        let work = Work::default();
        let own = put(&work, read(8), true);
        work.take(Lane::Machine).unwrap();
        {
            let mut board = work.lock();
            let job = board.jobs.iter_mut().find(|job| job.id == own).unwrap();
            job.done = 500.0;
            job.total = Some(900.0);
            job.ended = Some(5);
            job.wait_again();
        };
        let again = job(&work, own);
        assert_eq!(again.state, State::Queued);
        assert!(again.done.abs() < f64::EPSILON);
        assert_eq!(
            (again.total, again.started, again.ended),
            (None, None, None)
        );
        assert_eq!(again.note.as_deref(), Some(WAITS_FOR_YOURS));
        assert_eq!(work.take(Lane::Machine).map(|(id, _, _)| id), Some(own));
    }

    #[test]
    fn a_download_a_check_and_a_model_fetch_are_dropped_where_they_stand() {
        assert!(droppable(&Task::Download { app_id: 1 }));
        assert!(droppable(&Task::Check));
        assert!(droppable(&Task::FetchModel {
            model: "writer".to_owned()
        }));
        assert!(!droppable(&Task::Update { app_id: 1 }));
        assert!(!droppable(&read(1)));
        assert!(!droppable(&Task::Prepare { app_id: 1 }));
    }

    #[test]
    fn a_stopped_model_fetch_says_its_fetched_files_are_kept() {
        let note = stopped_note(&Task::FetchModel {
            model: "writer".to_owned(),
        });
        assert_eq!(
            note,
            "Stopped. Its files already fetched are kept, and the rest comes when it is first needed."
        );
        assert_eq!(stopped_note(&Task::Check), "Stopped.");
    }

    /// Sets a marker when dropped, as work holding a half-written file would let it go.
    struct Marks(Arc<AtomicBool>);

    impl Drop for Marks {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn a_job_told_to_stop_is_dropped_inside_its_own_task() {
        let flag = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let held = Marks(Arc::clone(&dropped));
        let work = async move {
            let _held = held;
            std::future::pending::<Result<(), String>>().await
        };
        let setter = {
            let flag = Arc::clone(&flag);
            async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                flag.store(true, Ordering::Relaxed);
            }
        };
        let (ended, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(in_own_task(Some(flag), work), setter)
        })
        .await
        .expect("the work was never told to stop");
        assert_eq!(ended, Err("stopped".to_owned()));
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn a_job_told_to_stop_before_its_work_starts_ends_stopped_with_nothing_kept() {
        let dropped = Arc::new(AtomicBool::new(false));
        let held = Marks(Arc::clone(&dropped));
        let flag = AtomicBool::new(true);
        let ended = tokio::time::timeout(
            Duration::from_secs(2),
            unless_stopped(&flag, async move {
                let _held = held;
                std::future::pending::<Result<(), String>>().await
            }),
        )
        .await
        .expect("the work outlived its stop");
        assert_eq!(ended, Err("stopped".to_owned()));
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn a_job_that_panics_ends_as_failed() {
        let ended = in_own_task(None, async {
            assert!(Duration::ZERO > Duration::from_secs(1), "work that panics");
            Ok::<(), String>(())
        })
        .await;
        assert!(ended.unwrap_err().starts_with("the job failed"));
    }

    #[tokio::test]
    async fn work_that_ends_is_handed_back_as_it_ended() {
        let flag = AtomicBool::new(false);
        assert_eq!(unless_stopped(&flag, async { Ok(3) }).await, Ok(3));
        assert_eq!(
            unless_stopped(&flag, async { Err::<u8, _>("no room".to_owned()) }).await,
            Err("no room".to_owned())
        );
    }

    /// Whether the lane's bell has been rung since it was last taken, which takes the ring.
    async fn rung(work: &Work, lane: Lane) -> bool {
        tokio::time::timeout(Duration::ZERO, work.bell(lane).notified())
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn taking_a_waiting_job_off_the_board_rings_every_lane() {
        let work = Work::default();
        put(&work, Task::Update { app_id: 9 }, false);
        assert!(work.take(Lane::Network).is_some());
        let download = put(&work, Task::Download { app_id: 7 }, false);
        let held = put(&work, read(7), false);
        assert!(work.take(Lane::Machine).is_none());
        assert!(!rung(&work, Lane::Machine).await);
        work.stop(download);
        assert_eq!(job(&work, download).state, State::Stopped);
        for lane in LANES {
            assert!(rung(&work, lane).await, "{lane:?} was not rung");
        }
        assert_eq!(work.take(Lane::Machine).map(|(id, _, _)| id), Some(held));
    }

    #[test]
    fn a_running_job_told_to_stop_keeps_its_game_until_it_ends() {
        let work = Work::default();
        let update = put(&work, Task::Update { app_id: 7 }, false);
        let (_, _, flag) = work.take(Lane::Network).unwrap();
        put(&work, read(7), false);
        work.stop(update);
        assert_eq!(job(&work, update).state, State::Running);
        assert!(flag.load(Ordering::Relaxed));
        assert!(work.take(Lane::Machine).is_none());
    }

    /// Puts one more than `KEPT` downloads of the person's on the board.
    fn crowd(work: &Work) -> Vec<u64> {
        (100..)
            .take(KEPT + 1)
            .map(|app_id| put(work, Task::Download { app_id }, false))
            .collect()
    }

    #[test]
    fn the_finished_jobs_kept_are_the_newest_to_end() {
        let work = Work::default();
        let own = put(&work, Task::Update { app_id: 7 }, true);
        assert!(work.take(Lane::Network).is_some());
        let others = crowd(&work);
        let mut board = work.lock();
        // The last queued ends first, so ending order and queue order disagree.
        for (at, &other) in (0..).zip(others.iter().rev()) {
            board.end(other, &Ok(Ended::said("Done.")), at);
        }
        assert_eq!(
            board.end(own, &Ok(Ended::said("Done.")), 100),
            (Some(7), true),
            "a job of the app's own stays the app's, so what follows it waits behind the person's"
        );
        let kept: Vec<u64> = board.jobs.iter().map(|job| job.id).collect();
        assert_eq!(kept.len(), KEPT);
        assert!(kept.contains(&own), "the job that just ended is shown");
        let gone: Vec<u64> = others.iter().rev().take(2).copied().collect();
        assert!(
            gone.iter().all(|id| !kept.contains(id)),
            "the two that ended first are taken off"
        );
    }

    #[test]
    fn the_job_that_just_ended_is_kept_where_others_ended_in_the_same_second() {
        let work = Work::default();
        let own = put(&work, Task::Update { app_id: 7 }, true);
        assert!(work.take(Lane::Network).is_some());
        let others = crowd(&work);
        let mut board = work.lock();
        for &other in &others {
            board.end(other, &Ok(Ended::said("Done.")), 100);
        }
        board.end(own, &Ok(Ended::said("Done.")), 100);
        assert!(board.jobs.iter().any(|job| job.id == own));
        assert_eq!(board.jobs.len(), KEPT);
    }

    #[test]
    fn the_persons_stop_of_a_job_stepping_aside_ends_it() {
        for stop_first in [false, true] {
            let work = Work::default();
            let own = put(&work, read(8), true);
            assert!(work.take(Lane::Machine).is_some());
            put(&work, Task::Update { app_id: 9 }, false);
            if stop_first {
                work.stop(own);
                work.make_way();
            } else {
                work.make_way();
                work.stop(own);
            }
            // The person asks for more work while the stopped job winds down.
            work.make_way();
            let mut board = work.lock();
            board.end(own, &Err("Stopped.".to_owned()), 1);
            let state = board.jobs.iter().find(|job| job.id == own).unwrap().state;
            assert_eq!(state, State::Stopped, "stopped first: {stop_first}");
        }
    }

    #[test]
    fn a_job_that_errs_is_stopped_only_where_it_was_told_to_stop() {
        for told in [false, true] {
            let work = Work::default();
            let update = put(&work, Task::Update { app_id: 7 }, false);
            assert!(work.take(Lane::Network).is_some());
            if told {
                work.stop(update);
            }
            let mut board = work.lock();
            board.end(update, &Err("Steam did not answer.".to_owned()), 1);
            let ended = board.jobs.iter().find(|job| job.id == update).unwrap();
            let expected = if told {
                (State::Stopped, stopped_note(&ended.task))
            } else {
                (State::Failed, "Steam did not answer.")
            };
            assert_eq!((ended.state, ended.note.as_deref().unwrap()), expected);
        }
    }

    #[test]
    fn a_job_stepping_aside_waits_again_when_it_ends() {
        let work = Work::default();
        let own = put(&work, read(8), true);
        assert!(work.take(Lane::Machine).is_some());
        put(&work, Task::Update { app_id: 9 }, false);
        work.make_way();
        let mut board = work.lock();
        assert_eq!(
            board.end(own, &Err("Stopped.".to_owned()), 1),
            (Some(8), true)
        );
        let state = board.jobs.iter().find(|job| job.id == own).unwrap().state;
        assert_eq!(state, State::Queued);
    }

    #[test]
    fn counts_read_with_their_thousands_apart() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(25_242_117), "25,242,117");
    }

    fn report(app_ids: Vec<u32>) -> Task {
        Task::Export {
            app_ids,
            to: std::path::PathBuf::from("report.html"),
        }
    }

    /// One task of each kind that uses game 7, a report of 8 and 7 among them.
    fn kinds_on_a_game() -> Vec<Task> {
        vec![
            Task::Download { app_id: 7 },
            Task::Update { app_id: 7 },
            read(7),
            Task::Recount { app_id: 7 },
            Task::Prepare { app_id: 7 },
            Task::ExportData {
                app_id: 7,
                to: std::path::PathBuf::from("data"),
            },
            report(vec![8, 7]),
        ]
    }

    fn kinds_of_no_game() -> Vec<Task> {
        vec![
            Task::Check,
            Task::FetchModel {
                model: "writer".to_owned(),
            },
            report(Vec::new()),
        ]
    }

    /// Sets a job's state by hand, as a board in the middle of its work has it.
    fn set_state(work: &Work, id: u64, state: State) {
        work.lock()
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .unwrap()
            .state = state;
    }

    fn taken(work: &Work, lane: Lane) -> Option<u64> {
        work.take(lane).map(|(id, _, _)| id)
    }

    #[test]
    fn a_report_and_an_update_of_a_game_in_it_never_run_at_once() {
        let updating = Work::default();
        put(&updating, Task::Update { app_id: 7 }, false);
        updating.take(Lane::Network).unwrap();
        put(&updating, report(vec![7, 8]), false);
        let report_during_update = taken(&updating, Lane::Files);

        let reporting = Work::default();
        put(&reporting, report(vec![7, 8]), false);
        reporting.take(Lane::Files).unwrap();
        put(&reporting, Task::Update { app_id: 7 }, false);
        let update_during_report = taken(&reporting, Lane::Network);

        assert_eq!(
            (report_during_update, update_during_report),
            (None, None),
            "a report reads every game in its list, so neither starts while the other runs"
        );
    }

    #[test]
    fn two_jobs_on_one_game_run_one_after_the_other_whatever_their_kinds() {
        let kinds = kinds_on_a_game();
        for (at, first) in kinds.iter().enumerate() {
            for (other, second) in kinds.iter().enumerate() {
                if at == other {
                    continue;
                }
                let board = |first_state: Option<State>| {
                    let work = Work::default();
                    let started = put(&work, first.clone(), false);
                    assert_eq!(taken(&work, first.lane()), Some(started));
                    if let Some(state) = first_state {
                        set_state(&work, started, state);
                    }
                    let id = put(&work, second.clone(), false);
                    (taken(&work, second.lane()), id)
                };
                assert_eq!(board(None).0, None, "{first:?} running, {second:?} put");
                for ended in [State::Done, State::Failed, State::Stopped] {
                    let (got, id) = board(Some(ended));
                    assert_eq!(got, Some(id), "{first:?} {ended:?}, {second:?} put");
                }
            }
        }
    }

    #[test]
    fn a_job_waits_for_every_waiting_job_queued_before_it_on_its_games() {
        let work = Work::default();
        put(&work, Task::Download { app_id: 9 }, false);
        put(&work, Task::Download { app_id: 7 }, false);
        put(&work, read(7), false);
        assert_eq!(taken(&work, Lane::Machine), None);
    }

    /// Whether two tasks have a game in common.
    fn share_a_game(a: &Task, b: &Task) -> bool {
        a.games().iter().any(|game| b.games().contains(game))
    }

    #[test]
    fn every_order_of_three_jobs_keeps_each_game_to_one_job_in_the_order_queued() {
        let mut all = kinds_on_a_game();
        all.extend(kinds_of_no_game());
        let mut orders = 0;
        for a in 0..all.len() {
            for b in (0..all.len()).filter(|&b| b != a) {
                for c in (0..all.len()).filter(|&c| c != a && c != b) {
                    orders += 1;
                    let tasks = [all[a].clone(), all[b].clone(), all[c].clone()];
                    walk(&tasks);
                }
            }
        }
        assert_eq!(orders, 720);
    }

    /// A board of these tasks put in order, each job in the state given.
    fn board_at(tasks: &[Task; 3], states: &[State]) -> (Work, Vec<u64>) {
        let work = Work::default();
        let ids: Vec<u64> = tasks
            .iter()
            .zip(states)
            .map(|(task, &state)| {
                let id = put(&work, task.clone(), false);
                set_state(&work, id, state);
                id
            })
            .collect();
        (work, ids)
    }

    fn states_of(work: &Work, ids: &[u64]) -> Vec<State> {
        ids.iter().map(|&id| job(work, id).state).collect()
    }

    /// Tries every move from every position three jobs can reach, each position on a fresh board.
    fn walk(tasks: &[Task; 3]) {
        let mut seen = Vec::new();
        let mut ahead = vec![vec![State::Queued; 3]];
        while let Some(states) = ahead.pop() {
            if seen.contains(&states) {
                continue;
            }
            seen.push(states.clone());
            let mut moves = Vec::new();
            for lane in LANES {
                let lane_busy = tasks
                    .iter()
                    .zip(&states)
                    .any(|(task, &state)| task.lane() == lane && state == State::Running);
                if lane_busy {
                    continue;
                }
                let (work, ids) = board_at(tasks, &states);
                let Some(started) = taken(&work, lane) else {
                    continue;
                };
                let at = ids.iter().position(|&id| id == started).unwrap();
                for (other, &state) in states.iter().enumerate().filter(|&(other, _)| other != at) {
                    let clash = share_a_game(&tasks[at], &tasks[other]);
                    assert!(
                        !(clash
                            && (state == State::Running || (other < at && state == State::Queued))),
                        "{:?} started beside {:?} ({state:?}) from {states:?}",
                        tasks[at],
                        tasks[other]
                    );
                }
                moves.push(states_of(&work, &ids));
            }
            for at in (0..3).filter(|&at| states[at] == State::Running) {
                let (work, ids) = board_at(tasks, &states);
                set_state(&work, ids[at], State::Done);
                moves.push(states_of(&work, &ids));
            }
            if moves.is_empty() {
                assert_eq!(states, vec![State::Done; 3], "stuck for {tasks:?}");
            }
            ahead.extend(moves);
        }
    }

    #[test]
    fn a_job_of_no_game_waits_for_no_game_and_holds_none() {
        let work = Work::default();
        for task in kinds_on_a_game() {
            let id = put(&work, task, false);
            set_state(&work, id, State::Running);
        }
        for task in kinds_of_no_game() {
            put(&work, task, false);
        }
        let started = [
            taken(&work, Lane::Network),
            taken(&work, Lane::Network),
            taken(&work, Lane::Files),
        ];
        assert!(started.iter().all(Option::is_some), "{started:?}");

        for kind in kinds_on_a_game() {
            let work = Work::default();
            for task in kinds_of_no_game() {
                let id = put(&work, task, false);
                set_state(&work, id, State::Running);
            }
            let id = put(&work, kind.clone(), false);
            assert_eq!(taken(&work, kind.lane()), Some(id), "{kind:?}");
        }
    }

    #[test]
    fn a_report_listing_a_game_twice_starts_and_holds_it() {
        let work = Work::default();
        let both = put(&work, report(vec![7, 7]), false);
        assert_eq!(taken(&work, Lane::Files), Some(both));
        put(&work, Task::Update { app_id: 7 }, false);
        assert_eq!(taken(&work, Lane::Network), None);
    }

    #[test]
    fn a_task_names_a_game_only_where_it_uses_exactly_one() {
        assert_eq!(read(7).one_game(), Some(7));
        assert_eq!(report(vec![7]).one_game(), Some(7));
        assert_eq!(report(vec![7, 8]).one_game(), None);
        assert_eq!(report(vec![7, 7]).one_game(), None);
        assert_eq!(report(Vec::new()).one_game(), None);
        assert_eq!(Task::Check.one_game(), None);

        let work = Work::default();
        let ids: Vec<u64> = [vec![7], vec![7, 8], vec![7, 7]]
            .into_iter()
            .map(|games| put(&work, report(games), false))
            .collect();
        work.lock().name(7, "Alpha");
        let names: Vec<String> = ids.iter().map(|&id| job(&work, id).name).collect();
        assert_eq!(names, ["Alpha", "", ""]);
    }

    #[test]
    fn a_job_of_several_games_is_named_for_the_library_and_one_of_an_unknown_game_for_nothing() {
        let library = std::env::temp_dir().join(format!("steamgauge-names-{}", std::process::id()));
        let model = Task::FetchModel {
            model: "no-such-model".to_owned(),
        };
        assert_eq!(name_in(&library, &model).as_deref(), Some("no-such-model"));
        for games in [vec![7, 8], vec![7, 7], Vec::new()] {
            assert_eq!(
                name_in(&library, &report(games)).as_deref(),
                Some("Your library")
            );
        }
        assert_eq!(name_in(&library, &read(4_000_000_007)), None);
    }

    #[test]
    fn a_waiting_report_taken_off_the_board_lets_go_of_every_game_in_it() {
        let work = Work::default();
        put(&work, Task::Update { app_id: 7 }, false);
        assert!(work.take(Lane::Network).is_some());
        let both = put(&work, report(vec![7, 8]), false);
        let held = put(&work, read(8), false);
        assert_eq!(taken(&work, Lane::Machine), None);
        work.stop(both);
        assert_eq!(taken(&work, Lane::Machine), Some(held));
    }

    /// The games a hold of `games` leaves, which it lets go of at once.
    fn left_by(work: &Work, games: &[u32]) -> Vec<u32> {
        work.hold(games)
            .1
            .into_iter()
            .map(|(game, _)| game)
            .collect()
    }

    const REMOVED: &str = "This game is busy: its files are being removed.";

    #[test]
    fn a_removal_leaves_each_game_a_job_waits_or_runs_on() {
        for task in kinds_on_a_game() {
            for ended in [State::Done, State::Failed, State::Stopped] {
                let work = Work::default();
                let id = put(&work, task.clone(), false);
                let (first, left) = work.hold(&[9, 7, 6]);
                assert_eq!(
                    left,
                    vec![(7, "This game is busy: waiting.".to_owned())],
                    "{task:?}"
                );
                let (second, left) = work.hold(&[9, 6]);
                assert_eq!(
                    left,
                    vec![(9, REMOVED.to_owned()), (6, REMOVED.to_owned())],
                    "{task:?}"
                );
                drop(second);
                drop(first);
                assert_eq!(taken(&work, task.lane()), Some(id));
                assert_eq!(
                    work.hold(&[9, 7, 6]).1,
                    vec![(7, "This game is busy: starting.".to_owned())],
                    "{task:?}"
                );
                set_state(&work, id, ended);
                assert_eq!(
                    left_by(&work, &[9, 7, 6]),
                    Vec::<u32>::new(),
                    "{task:?} {ended:?}"
                );
            }
        }
        let work = Work::default();
        put(&work, report(vec![8, 7]), false);
        assert_eq!(
            work.hold(&[8]).1,
            vec![(8, "This game is busy: waiting.".to_owned())]
        );

        let work = Work::default();
        for task in kinds_of_no_game() {
            put(&work, task, false);
        }
        assert_eq!(left_by(&work, &[7, 8]), Vec::<u32>::new());
        assert_eq!(left_by(&work, &[]), Vec::<u32>::new());
    }

    #[tokio::test]
    async fn no_job_starts_on_a_game_while_its_files_are_removed() {
        let work = Work::default();
        let (removal, left) = work.hold(&[7, 8]);
        assert_eq!(left, Vec::new());
        put(&work, read(8), false);
        for task in kinds_on_a_game() {
            put(&work, task, false);
        }
        let nine = put(&work, read(9), false);
        let started: Vec<u64> = LANES
            .into_iter()
            .filter_map(|lane| taken(&work, lane))
            .collect();
        assert_eq!(started, vec![nine]);
        drop(removal);
        for lane in LANES {
            assert!(rung(&work, lane).await, "{lane:?} was not rung");
        }
        let (_, task, _) = work.take(Lane::Network).unwrap();
        assert_eq!(task, Task::Download { app_id: 7 });
    }

    #[test]
    fn two_removals_never_hold_one_game_and_each_lets_go_of_all_its_own() {
        let sets: [&[u32]; 6] = [&[7], &[8], &[7, 8], &[8, 7], &[9, 8], &[]];
        for a in sets {
            for b in sets {
                let work = Work::default();
                let held_now = |work: &Work| {
                    [7, 8, 9]
                        .into_iter()
                        .filter(|&game| !left_by(work, &[game]).is_empty())
                        .collect::<Vec<u32>>()
                };
                let first = work.hold(a).0;
                let (second, left) = work.hold(b);
                let shared: Vec<u32> = b.iter().copied().filter(|game| a.contains(game)).collect();
                assert_eq!(
                    left.iter().map(|(game, _)| *game).collect::<Vec<_>>(),
                    shared,
                    "{a:?} then {b:?}"
                );
                assert!(left.iter().all(|(_, why)| why == REMOVED));
                let mut both: Vec<u32> = a.iter().chain(b).copied().collect();
                both.sort_unstable();
                both.dedup();
                assert_eq!(held_now(&work), both, "{a:?} then {b:?}");
                drop(first);
                let mut rest: Vec<u32> =
                    b.iter().copied().filter(|game| !a.contains(game)).collect();
                rest.sort_unstable();
                assert_eq!(held_now(&work), rest, "{a:?} then {b:?}");
                drop(second);
                assert_eq!(held_now(&work), Vec::<u32>::new(), "{a:?} then {b:?}");
            }
        }
    }
}
