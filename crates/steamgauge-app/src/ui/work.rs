//! Everything the app does that takes longer than a click, as jobs on two lanes.
//!
//! Downloads talk to Steam and wait on its pace; reads and preparations hold the graphics card.
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
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use super::{library_dir, now_unix, settings::Settings, text};

/// What a job is asked to do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Task {
    /// Every review of a game, resuming where an earlier download stopped.
    Download { app_id: u32 },
    /// What was written or edited since a game was last downloaded, then a read of it again
    /// where it had been read.
    Update { app_id: u32 },
    /// Every review of a game read by the reader, in one language or all of them.
    Read {
        app_id: u32,
        language: Option<String>,
    },
    /// A read game's answers added up again from disk, without the reader: how a reading made
    /// before the window counted something comes to count it, in a minute rather than a read.
    Recount { app_id: u32 },
    /// A read game made searchable by meaning.
    Prepare { app_id: u32 },
    /// How many reviews each game has on Steam now, and whether newer models are published.
    Check,
    /// A report of these games, written as one page to where the person chose.
    Export {
        app_ids: Vec<u32>,
        to: std::path::PathBuf,
    },
    /// A read game's figures and points, written as CSV and JSON into a new folder where the
    /// person chose.
    ExportData { app_id: u32, to: std::path::PathBuf },
    /// The reader this machine reads with, fetched ahead of a first read, so a first game's
    /// reviews and the reader download at the same time.
    FetchReader,
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
    fn lane(&self) -> Lane {
        match self {
            Self::Download { .. } | Self::Update { .. } | Self::Check | Self::FetchReader => {
                Lane::Network
            }
            Self::Read { .. } | Self::Prepare { .. } => Lane::Machine,
            Self::Export { .. } | Self::ExportData { .. } | Self::Recount { .. } => Lane::Files,
        }
    }

    pub(super) fn app_id(&self) -> Option<u32> {
        match self {
            Self::Download { app_id }
            | Self::Update { app_id }
            | Self::Read { app_id, .. }
            | Self::Recount { app_id }
            | Self::Prepare { app_id }
            | Self::ExportData { app_id, .. } => Some(*app_id),
            Self::Check | Self::Export { .. } | Self::FetchReader => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Queued,
    Running,
    Done,
    Failed,
    Stopped,
}

/// What a job's count is a count of.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Bytes,
    Reviews,
    Points,
    Pages,
    Games,
}

/// One job as the window draws it.
#[derive(Debug, Clone, Serialize)]
pub struct Job {
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
/// follows; slower makes a download look stuck.
pub(super) const EVERY: Duration = Duration::from_millis(250);

#[derive(Default)]
struct Board {
    jobs: Vec<Job>,
    next: u64,
    meters: HashMap<u64, Meter>,
    stops: HashMap<u64, Arc<AtomicBool>>,
    aborts: HashMap<u64, tokio::task::AbortHandle>,
    /// Jobs of the app's own told to stop for the person's work, which wait again rather than
    /// ending.
    yielding: HashSet<u64>,
    sent: Option<Instant>,
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
/// it is whole and resumes from those, and a check writes nothing until it ends.
fn droppable(task: &Task) -> bool {
    matches!(task, Task::Download { .. } | Task::Check)
}

/// The board and a bell for each lane.
#[derive(Default)]
pub struct Work {
    board: Arc<Mutex<Board>>,
    network: Arc<Notify>,
    machine: Arc<Notify>,
    files: Arc<Notify>,
}

/// What a job is about, in words: the game's name where it has one on disk.
fn name_of(app: &AppHandle, task: &Task) -> String {
    if *task == Task::FetchReader {
        return format!("The {} reader", reader_here(&Settings::load(app)).name);
    }
    match task.app_id() {
        Some(app_id) => steamgauge_core::report::crawl_facts(&library_dir(app), app_id)
            .map_or_else(|_| format!("App {app_id}"), |facts| facts.title()),
        None => "Your library".to_owned(),
    }
}

impl Work {
    fn bell(&self, lane: Lane) -> &Arc<Notify> {
        match lane {
            Lane::Network => &self.network,
            Lane::Machine => &self.machine,
            Lane::Files => &self.files,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Board> {
        // A job that panicked while holding the board left it as it was; the board is a list
        // of what is happening, and carrying on with it is better than refusing every job.
        self.board
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Puts a task the person asked for on the board, unless the same task is already waiting or
    /// running, which then becomes theirs.
    pub fn queue(&self, app: &AppHandle, task: Task, name: Option<String>) -> u64 {
        self.queue_as(app, task, name, false)
    }

    /// Puts a task the app starts by itself on the board, to wait behind the person's work.
    pub fn queue_background(&self, app: &AppHandle, task: Task) -> u64 {
        self.queue_as(app, task, None, true)
    }

    fn queue_as(&self, app: &AppHandle, task: Task, name: Option<String>, background: bool) -> u64 {
        let lane = task.lane();
        let name = name.unwrap_or_else(|| name_of(app, &task));
        let id = self.lock().put(task, name, background, now_unix());
        if !background {
            self.make_way();
        }
        self.bell(lane).notify_one();
        self.send(app, true);
        id
    }

    /// Stops every job of the app's own that is running, to wait again once the person's work
    /// is done.
    fn make_way(&self) {
        let mut board = self.lock();
        let running: Vec<(u64, bool)> = board
            .jobs
            .iter()
            .filter(|job| job.background && job.state == State::Running)
            .map(|job| (job.id, droppable(&job.task)))
            .collect();
        for (id, droppable) in running {
            board.yielding.insert(id);
            if let Some(stop) = board.stops.get(&id) {
                stop.store(true, Ordering::Relaxed);
            }
            if droppable && let Some(abort) = board.aborts.get(&id) {
                abort.abort();
            }
        }
    }

    /// Stops a job: a waiting one never starts, and a running one is told to stop where it
    /// can, keeping what it finished.
    ///
    /// A download and a check are dropped where they stand: a crawl records each window only
    /// once it is whole and resumes from those, and a check writes nothing until it ends.
    /// Everything else is only told, and stops at a point where what it leaves is consistent,
    /// letting go of its game only then, not before another job could start on the same files.
    pub fn stop(&self, app: &AppHandle, id: u64) {
        {
            let mut board = self.lock();
            if let Some(stop) = board.stops.get(&id) {
                stop.store(true, Ordering::Relaxed);
            }
            let droppable = board
                .jobs
                .iter()
                .any(|job| job.id == id && droppable(&job.task));
            if droppable && let Some(abort) = board.aborts.get(&id) {
                abort.abort();
            }
            if let Some(job) = board.jobs.iter_mut().find(|job| job.id == id)
                && job.state == State::Queued
            {
                job.state = State::Stopped;
                job.ended = Some(now_unix());
                job.note = Some("Taken off the list before it started.".to_owned());
            }
        }
        self.send(app, true);
    }

    /// Takes finished jobs off the board.
    pub fn clear(&self, app: &AppHandle) {
        self.lock()
            .jobs
            .retain(|job| matches!(job.state, State::Queued | State::Running));
        self.send(app, true);
    }

    pub fn jobs(&self) -> Vec<Job> {
        self.lock().jobs.clone()
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

    /// The first waiting job of this lane whose game no running job is working on, marked
    /// running.
    fn take(&self, lane: Lane) -> Option<(u64, Task, Arc<AtomicBool>)> {
        let mut board = self.lock();
        let busy: Vec<u32> = board
            .jobs
            .iter()
            .filter(|job| job.state == State::Running)
            .filter_map(|job| job.task.app_id())
            .collect();
        let theirs = board
            .jobs
            .iter()
            .any(|job| !job.background && matches!(job.state, State::Queued | State::Running));
        let job = board.jobs.iter_mut().find(|job| {
            job.state == State::Queued
                && job.task.lane() == lane
                && !(job.background && theirs)
                && job
                    .task
                    .app_id()
                    .is_none_or(|app_id| !busy.contains(&app_id))
        })?;
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
        let (game, background) = {
            let mut board = self.lock();
            board.meters.remove(&id);
            board.aborts.remove(&id);
            let stopped = board
                .stops
                .remove(&id)
                .is_some_and(|stop| stop.load(Ordering::Relaxed));
            let yielded = board.yielding.remove(&id);
            if let Some(job) = board.jobs.iter_mut().find(|job| job.id == id) {
                job.ended = Some(now_unix());
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
            }
            // The newest finished jobs stay, so the cockpit can say what just happened.
            let finished: Vec<u64> = board
                .jobs
                .iter()
                .rev()
                .filter(|job| !matches!(job.state, State::Queued | State::Running))
                .skip(KEPT)
                .map(|job| job.id)
                .collect();
            board.jobs.retain(|job| !finished.contains(&job.id));
            board
                .jobs
                .iter()
                .find(|job| job.id == id)
                .map_or((None, false), |job| (job.task.app_id(), job.background))
        };
        self.send(app, true);
        // Which game changed on disk, so a page showing another one is left as it is.
        if ended.is_ok() {
            let _ = app.emit("library", game);
        }
        // A finished job frees its game for the other lanes.
        for lane in LANES {
            self.bell(lane).notify_one();
        }
        background
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

fn stopped_note(task: &Task) -> &'static str {
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
        Task::FetchReader => "Stopped. The first read fetches the reader instead.",
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
pub fn start(app: &AppHandle) {
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

/// Runs a job in a task of its own, so stopping a download can drop it where it stands: a
/// download resumes from what it kept, and an update starts again from its watermark.
async fn run(app: &AppHandle, id: u64, task: Task, stop: Arc<AtomicBool>) -> Result<Ended, String> {
    let job = {
        let app = app.clone();
        tokio::spawn(async move {
            match task {
                Task::Download { app_id } => download(&app, id, app_id).await,
                Task::Update { app_id } => update(&app, id, app_id, stop).await,
                Task::Read { app_id, language } => read(&app, id, app_id, language, stop).await,
                Task::Recount { app_id } => recount(&app, id, app_id, stop).await,
                Task::Prepare { app_id } => prepare(&app, id, app_id, stop).await,
                Task::Check => check(&app, id).await,
                Task::Export { app_ids, to } => export(&app, id, (app_ids, to), stop).await,
                Task::ExportData { app_id, to } => export_data(&app, id, app_id, to, stop).await,
                Task::FetchReader => {
                    let size = reader_here(&Settings::load(&app));
                    fetch_reader(&app, id, size).await.map(|()| {
                        Ended::said(format!("The {} reader is on this computer.", size.name))
                    })
                }
            }
        })
    };
    app.state::<Work>()
        .lock()
        .aborts
        .insert(id, job.abort_handle());
    match job.await {
        Ok(ended) => ended,
        Err(joined) if joined.is_cancelled() => Err("stopped".to_owned()),
        Err(joined) => Err(format!("the job failed: {joined}")),
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
fn float(count: u64) -> f64 {
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
pub fn reader_here(settings: &Settings) -> &'static steamgauge_core::reader::Size {
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
pub fn runs_here(
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
    fn new() -> Self {
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
    if model_dir.join("model.onnx").is_file() {
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
    let size = reader_here(&settings);
    let model_dir = size.home();
    fetch_reader(app, id, size).await?;
    know_the_game(app, id, app_id, &out_dir).await?;

    let capture = float(
        steamgauge_core::report::crawl_facts(&out_dir, app_id)
            .map_err(text)?
            .rows_unique,
    );
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
    let report = tauri::async_runtime::spawn_blocking(move || {
        // From before the model loads: the estimate is of the wait, and loading is part of it.
        let started = Instant::now();
        let mut model = steamgauge_core::reader::ClaimReader::load(&model_dir).map_err(text)?;
        let report = steamgauge_core::read::read_corpus(&mut model, app_id, &options, |progress| {
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
        if model.device() == "cpu" {
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
        "{} reviews read, {} separate points.",
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
        search_models::{ENCODER, RERANKER, SearchEncoder},
    };

    let cache = steamgauge_core::model::default_cache_dir();
    let left = ENCODER.bytes_left(&cache) + RERANKER.bytes_left(&cache);
    if left > 0 {
        let total = float(left);
        let mut across = Across::new();
        for model in [ENCODER, RERANKER] {
            let telling = app.clone();
            let across = &mut across;
            model
                .ensure(&cache, move |progress| {
                    let file = format!("{}/{}", model.name, progress.file);
                    let done = float(across.add(&file, progress.downloaded));
                    let saying = if done >= total {
                        "Checking the search models"
                    } else {
                        "Fetching the search models"
                    };
                    tell(&telling, id, saying, Unit::Bytes, done, Some(total));
                })
                .await
                .map_err(text)?;
        }
    }

    let dir = library_dir(app);
    let share = Settings::load(app).gpu_share;
    let embedder = Arc::clone(&app.state::<super::Meaning>().embedder);
    let telling = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Ended, String> {
        let snapshot = steamgauge_core::embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let total = float(super::read_report(&snapshot)?.claims);
        tell(
            &telling,
            id,
            "Loading the search model",
            Unit::Points,
            0.0,
            Some(total),
        );
        let mut slot = embedder.lock().map_err(text)?;
        let encoder = super::loaded(&mut slot, SearchEncoder::load)?;
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
        if prepared.finished {
            Ok(Ended::said("Ready to search by meaning."))
        } else {
            Err("stopped".to_owned())
        }
    })
    .await
    .map_err(text)?
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
        let page = client
            .fetch(
                &steamgauge_core::ReviewQuery::new(game.app_id).per_page(0),
                game.app_id,
            )
            .await;
        if let Ok(summary) = page.map(|page| page.query_summary)
            && let Some(summary) = summary
        {
            totals.games.insert(game.app_id, summary.total_reviews);
        }
        // A game downloaded before the crawler asked for names is listed by its number until
        // somebody asks; the store is being asked about it anyway.
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
        let partial = written.with_extension("partial");
        std::fs::write(&partial, steamgauge_core::html::render(&report)).map_err(text)?;
        std::fs::rename(&partial, &written).map_err(text)
    })
    .await
    .map_err(text)??;
    Ok(Ended::said(format!("Saved as {}.", to.display())))
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
                "Writing the points",
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
            " {} points of reviews edited since they were read are left out.",
            thousands(exported.left_out)
        )
    } else {
        String::new()
    };
    Ok(Ended::said(format!(
        "Saved {} points and the figures in {}.{left_out}",
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
pub fn work(work: tauri::State<'_, Work>) -> Vec<Job> {
    work.jobs()
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn queue(app: AppHandle, work: tauri::State<'_, Work>, tasks: Vec<Task>) -> Vec<u64> {
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
pub fn stop_job(app: AppHandle, work: tauri::State<'_, Work>, id: u64) {
    work.stop(&app, id);
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn clear_finished(app: AppHandle, work: tauri::State<'_, Work>) {
    work.clear(&app);
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn open_report(app: AppHandle, work: tauri::State<'_, Work>, id: u64) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

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
        }
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
    fn only_a_download_and_a_check_are_dropped_where_they_stand() {
        assert!(droppable(&Task::Download { app_id: 1 }));
        assert!(droppable(&Task::Check));
        assert!(!droppable(&Task::Update { app_id: 1 }));
        assert!(!droppable(&read(1)));
    }

    #[test]
    fn counts_read_with_their_thousands_apart() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(25_242_117), "25,242,117");
    }
}
