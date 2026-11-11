//! Orchestrating a sharded, resumable walk over a corpus.

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use tokio::{sync::Semaphore, task::JoinSet};

use crate::{
    CaptureWriter, Result,
    api::SteamClient,
    query::{MAX_PER_PAGE, ReviewQuery, SortOrder},
    shard::{self, CORPUS_EPOCH, DEFAULT_SHARD_TARGET, Shard},
    state::CrawlState,
};

#[derive(Debug, Clone)]
pub struct CrawlOptions {
    pub out_dir: PathBuf,
    /// Shards in flight at once. Request pacing is enforced globally by the client, so this
    /// changes how work is ordered, not how hard Valve is hit.
    pub concurrency: usize,
    pub shard_target: u64,
    pub resume: bool,
}

impl Default for CrawlOptions {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::from("data"),
            concurrency: 4,
            shard_target: DEFAULT_SHARD_TARGET,
            resume: true,
        }
    }
}

/// Why a shard's walk ended. Anything other than [`StopReason::Exhausted`] means the window
/// was not fully served and coverage should be read as a floor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Exhausted,
    /// The cursor stopped moving forward. Expected on helpfulness-ranked ordering, and a bug
    /// anywhere else.
    CursorRepeated,
    NoCursor,
}

impl StopReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exhausted => "Exhausted",
            Self::CursorRepeated => "CursorRepeated",
            Self::NoCursor => "NoCursor",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub shards_done: usize,
    pub shards_total: usize,
    pub unique: u64,
    pub valve_total: u64,
}

#[derive(Debug, Clone)]
pub struct CrawlReport {
    pub app_id: u32,
    /// The store's name for the app, or the one the crawl record holds where the store gives
    /// none. Empty where neither has one, and every caller falls back to the id.
    pub name: String,
    pub shards: usize,
    pub pages: u32,
    pub unique: u64,
    pub duplicates_this_run: u64,
    pub valve_total: u64,
    pub valve_positive: u64,
    pub valve_negative: u64,
    pub review_score_desc: String,
    pub elapsed: Duration,
    pub dir: PathBuf,
    pub complete: bool,
    pub resumed: bool,
    /// Windows Steam stopped serving early, which a second walk then completed.
    pub shards_restarted: usize,
    /// Windows still short of Valve's stated count after every walk. These stay unfinished.
    pub shards_short: usize,
}

impl CrawlReport {
    /// Share of Valve's own stated total that was actually retrieved.
    ///
    /// This is the figure that makes the census claim checkable rather than asserted. It is
    /// only meaningful against a total reported for the same request parameters.
    #[must_use]
    pub fn coverage(&self) -> Option<f64> {
        coverage_of(self.unique, self.valve_total)
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn coverage_of(unique: u64, valve_total: u64) -> Option<f64> {
    (valve_total > 0).then(|| unique as f64 / valve_total as f64)
}

/// Downloads every review Valve will serve for `app_id`, written up to the newest edit Steam
/// served as the crawl began, into an immutable Parquet capture.
///
/// # Errors
///
/// Propagates transport, throttling, database and Parquet failures. An interrupted crawl
/// leaves its completed shards on disk and resumes from them on the next run.
/// A crawl whose record could not be written is resumed as well, and the next run writes it.
/// [`crate::Error::Refused`] where another job holds the capture's lock when the crawl comes to
/// write its record. The crawl is resumed by the next run, which writes it.
pub async fn crawl(
    client: &SteamClient,
    app_id: u32,
    options: &CrawlOptions,
    mut on_progress: impl FnMut(Progress),
) -> Result<CrawlReport> {
    let started = Instant::now();
    let state = Arc::new(CrawlState::open(&options.out_dir.join("state.sqlite"))?);

    let summary = client.summary(app_id).await?;
    let valve_total = summary.total_reviews;

    let (crawl_id, snapshot, resumed) =
        prepare(client, app_id, options, &state, valve_total).await?;

    let dir = options
        .out_dir
        .join(format!("appid={app_id}"))
        .join(format!("snapshot={snapshot}"));
    std::fs::create_dir_all(&dir)?;
    let newest_edit = state.newest_edit(crawl_id)?;

    let pending = state.pending_shards(crawl_id)?;
    let shards_total = pending.len();
    let Tally {
        duplicates,
        shards_restarted,
        shards_short,
    } = run_shards(
        ShardRun {
            client,
            app_id,
            state: &state,
            crawl_id,
            dir: &dir,
            concurrency: options.concurrency,
            keep_until: newest_edit,
        },
        pending,
        |shards_done, unique| {
            on_progress(Progress {
                shards_done,
                shards_total,
                unique,
                valve_total,
            });
        },
    )
    .await?;

    let complete = state.all_shards_done(crawl_id)?;
    let (windows, pages) = state.completed_totals(crawl_id)?;
    // Asked before the lock, since a sweep begun while the store answers would be refused.
    let asked_name = client.name(app_id).await;
    // Held in a binding until the record is written: `_` would let go at once.
    let record_lock = sweep_lock(&dir, app_id)?;
    let recorded = recorded_fields(&dir)?;
    let unique = match recorded.get("swept_unix").and_then(Value::as_i64) {
        Some(swept) => windows.saturating_add(crate::capture::only_swept(&dir, swept)?),
        None => windows,
    };
    let name = match asked_name {
        Some(name) => name,
        None => recorded
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    };

    let report = CrawlReport {
        app_id,
        name,
        shards: shards_total,
        pages,
        unique,
        duplicates_this_run: duplicates,
        valve_total: summary.total_reviews,
        valve_positive: summary.total_positive,
        valve_negative: summary.total_negative,
        review_score_desc: summary.review_score_desc,
        elapsed: started.elapsed(),
        dir,
        complete,
        resumed,
        shards_restarted,
        shards_short,
    };
    write_sidecar(
        &report,
        snapshot,
        newest_edit.map(|edit| edit.saturating_add(1)),
        recorded,
    )?;
    // Finishing last keeps a crawl whose record is missing resumable.
    if complete {
        state.finish_crawl(crawl_id)?;
    }
    drop(record_lock);
    Ok(report)
}

/// The fields `crawl.json` holds, or none where the record is missing, cut off or no object.
/// A first run has no record, and a write in place that died can leave half of one.
fn recorded_fields(dir: &std::path::Path) -> Result<serde_json::Map<String, Value>> {
    match std::fs::read(dir.join("crawl.json")) {
        Ok(bytes) => Ok(match serde_json::from_slice(&bytes) {
            Ok(Value::Object(fields)) => fields,
            _ => serde_json::Map::new(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::Map::new()),
        Err(error) => Err(error.into()),
    }
}

/// Continues an unfinished crawl where one exists, otherwise plans a new one.
///
/// Resuming reuses the original snapshot timestamp so every shard of one corpus lands in
/// the same directory, however many runs it took to finish. A new crawl first asks Steam for
/// its newest edit and keeps it, and a resumed one finds it in the state.
async fn prepare(
    client: &SteamClient,
    app_id: u32,
    options: &CrawlOptions,
    state: &CrawlState,
    valve_total: u64,
) -> Result<(i64, i64, bool)> {
    if options.resume
        && let Some(found) = state.resumable(app_id)?
    {
        return Ok((found.id, found.snapshot_unix, true));
    }
    let snapshot = now_unix();
    let first = client
        .fetch(
            &ReviewQuery::new(app_id)
                .order(SortOrder::Updated)
                .window(CORPUS_EPOCH, snapshot),
            app_id,
        )
        .await?;
    let edit = newest_on(&first.reviews);
    // The newest edit Steam serves now is a stamp from the clock that stamps every review. The
    // windows run to the later of it and this computer's clock, so they hold everything written
    // before it and no other crawl has asked them.
    let shards = shard::plan(
        client,
        app_id,
        CORPUS_EPOCH,
        snapshot.max(edit),
        options.shard_target,
    )
    .await?;
    let crawl_id = state.begin_crawl(
        app_id,
        snapshot,
        &ReviewQuery::new(app_id).to_url(),
        valve_total,
    )?;
    if edit >= CORPUS_EPOCH {
        state.record_newest_edit(crawl_id, edit)?;
    }
    state.record_shards(crawl_id, &shards)?;
    Ok((crawl_id, snapshot, false))
}

/// What the shard run added up to, beyond what the state database already records.
struct Tally {
    duplicates: u64,
    shards_restarted: usize,
    shards_short: usize,
}

/// Everything a shard walk needs that is the same for every shard.
struct ShardRun<'a> {
    client: &'a SteamClient,
    app_id: u32,
    state: &'a Arc<CrawlState>,
    crawl_id: i64,
    dir: &'a std::path::Path,
    concurrency: usize,
    /// The newest creation time a shard keeps, where the crawl kept a newest edit.
    keep_until: Option<i64>,
}

/// Walks every outstanding window, up to `concurrency` at a time.
async fn run_shards(
    run: ShardRun<'_>,
    pending: Vec<crate::state::ShardRecord>,
    mut on_progress: impl FnMut(usize, u64),
) -> Result<Tally> {
    let ShardRun {
        client,
        app_id,
        state,
        crawl_id,
        dir,
        concurrency,
        keep_until,
    } = run;
    let permits = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut tasks = JoinSet::new();
    for record in pending {
        let client = client.clone();
        let permits = Arc::clone(&permits);
        let state = Arc::clone(state);
        let path = dir.join(format!("shard-{:04}.parquet", record.idx));
        tasks.spawn(async move {
            let _permit = permits.acquire_owned().await;
            state.mark_running(crawl_id, record.idx)?;
            let outcome = crawl_shard(&client, app_id, record.shard, &path, keep_until).await?;
            // A window still short after every walk is left unfinished on purpose. Marking
            // it done would fold a known undercount into the corpus and report it as
            // complete. Left as it is, the crawl reports itself incomplete and the next run
            // walks it again.
            if !outcome.short {
                state.mark_done(
                    crawl_id,
                    record.idx,
                    outcome.rows,
                    outcome.pages,
                    outcome.stop.as_str(),
                    outcome.max_created,
                )?;
            }
            Ok::<_, crate::Error>(outcome)
        });
    }

    let mut tally = Tally {
        duplicates: 0,
        shards_restarted: 0,
        shards_short: 0,
    };
    let mut done = 0;
    while let Some(joined) = tasks.join_next().await {
        // A panicking shard task must not be reported as a completed crawl.
        let outcome = joined.map_err(|e| crate::Error::ShardPanicked {
            detail: e.to_string(),
        })??;
        tally.duplicates += outcome
            .fetched
            .saturating_sub(outcome.rows.saturating_add(outcome.past));
        if outcome.walks > 1 {
            tally.shards_restarted += 1;
        }
        if outcome.short {
            tally.shards_short += 1;
        }
        done += 1;
        let (unique, _) = state.completed_totals(crawl_id)?;
        on_progress(done, unique);
    }
    Ok(tally)
}

/// Walks allowed for one window before its shortfall is treated as real rather than a stall.
///
/// Steam intermittently stops serving a window early. The cursor advances normally, pages
/// come back full, and then an empty page arrives long before the window is exhausted, which
/// is indistinguishable from a genuine end. Measured on a 380,000-review corpus, three of
/// fourteen windows stopped between 69% and 75% of their expected count, and re-walking an
/// identical window immediately afterwards returned 18,159 of 18,160.
///
/// Accepting the first walk is what made that corpus 4.5% short while reporting every shard
/// as finished, which is the one failure this project cannot tolerate quietly: it undercounts
/// the corpus and calls it a census.
const MAX_SHARD_WALKS: u32 = 3;

/// How far below Valve's stated count for a window a walk may land before it is walked again.
///
/// Windows that genuinely finish land within a handful of reviews of expectation, the drift
/// being reviews written or deleted between planning and walking. The largest gap seen across
/// eleven honest shards was eight reviews in 39,278. Windows cut short by a stall miss a
/// quarter or more. Nothing observed falls between, so both bounds have wide margins.
const SHORTFALL_RATIO: f64 = 0.95;

/// Shortfalls smaller than this are drift rather than evidence, however small the window.
const SHORTFALL_FLOOR: u64 = 20;

/// Whether a walk ended so far below expectation that Steam is more likely to have stalled
/// than the window to have run out.
#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn fell_short(rows: u64, expected: u64) -> bool {
    if expected == 0 {
        return false;
    }
    expected.saturating_sub(rows) > SHORTFALL_FLOOR
        && (rows as f64) < expected as f64 * SHORTFALL_RATIO
}

#[derive(Debug)]
struct ShardOutcome {
    rows: u64,
    fetched: u64,
    pages: u32,
    stop: StopReason,
    max_created: Option<i64>,
    /// Reviews left out as written after the crawl's newest edit.
    past: u64,
    /// Walks taken. More than one means Steam stopped early at least once.
    walks: u32,
    /// Still short after every walk, so the window is not known to be complete.
    short: bool,
}

/// Walks a window and repeats it while it comes back short of what Valve says it holds.
///
/// Each walk asks for one review fewer a page than the walk before. Steam keeps what it
/// answered a caller without a key for up to ten minutes, and an identical walk asked again
/// inside that time is served the same pages, the early end included. A different page size
/// is a different query, walked afresh.
async fn crawl_shard(
    client: &SteamClient,
    app_id: u32,
    shard: Shard,
    path: &std::path::Path,
    keep_until: Option<i64>,
) -> Result<ShardOutcome> {
    let mut last = walk_shard(client, app_id, shard, path, MAX_PER_PAGE, keep_until).await?;
    for walk in 2..=MAX_SHARD_WALKS {
        // The count held the reviews left out as too new, so they stand for rows here.
        if !fell_short(last.rows + last.past, shard.expected) {
            return Ok(last);
        }
        let per_page = MAX_PER_PAGE.saturating_sub(u16::try_from(walk - 1).unwrap_or(0));
        last = walk_shard(client, app_id, shard, path, per_page, keep_until).await?;
        last.walks = walk;
    }
    let short = fell_short(last.rows + last.past, shard.expected);
    Ok(ShardOutcome { short, ..last })
}

async fn walk_shard(
    client: &SteamClient,
    app_id: u32,
    shard: Shard,
    path: &std::path::Path,
    per_page: u16,
    keep_until: Option<i64>,
) -> Result<ShardOutcome> {
    let mut writer = CaptureWriter::create(path, app_id)?;
    let mut cursor = "*".to_owned();
    let mut seen: HashSet<String> = HashSet::new();
    let mut pages: u32 = 0;
    let mut fetched: u64 = 0;
    let mut past: u64 = 0;
    let mut max_created: Option<i64> = None;

    let stop = loop {
        let query = ReviewQuery::new(app_id)
            .cursor(cursor.clone())
            .per_page(per_page)
            .window(shard.start_date, shard.end_date);
        let page = client.fetch(&query, app_id).await?;
        pages = pages.saturating_add(1);

        if page.reviews.is_empty() {
            break StopReason::Exhausted;
        }
        fetched = fetched.saturating_add(count(page.reviews.len()));

        let mut fresh: Vec<&Value> = Vec::new();
        for review in &page.reviews {
            let new = review
                .get("recommendationid")
                .and_then(Value::as_str)
                .is_some_and(|id| seen.insert(id.to_owned()));
            if !new {
                continue;
            }
            // Written after the newest edit Steam served as the crawl began. The first sweep
            // walks from the second after that edit and brings it in, so a shard that kept it
            // would have it counted twice.
            let created = review.get("timestamp_created").and_then(Value::as_i64);
            if keep_until
                .zip(created)
                .is_some_and(|(until, created)| created > until)
            {
                past = past.saturating_add(1);
            } else {
                fresh.push(review);
            }
        }
        for review in &fresh {
            if let Some(ts) = review.get("timestamp_created").and_then(Value::as_i64) {
                max_created = Some(max_created.map_or(ts, |current: i64| current.max(ts)));
            }
        }
        writer.write(&fresh)?;

        let Some(next) = page.cursor else {
            break StopReason::NoCursor;
        };
        if next == cursor {
            break StopReason::CursorRepeated;
        }
        cursor = next;
    };

    let rows = writer.close()?;
    Ok(ShardOutcome {
        rows,
        fetched,
        pages,
        stop,
        max_created,
        past,
        walks: 1,
        short: false,
    })
}

/// A snapshot that does not record the parameters it was gathered under cannot be compared
/// to any other snapshot, so the metadata travels with the Parquet files.
///
/// The crawl sets its own fields and keeps the rest, since a sweep may have run between two
/// runs of one crawl. A sweep field of a type the sweep itself would not read is dropped, so
/// the record still reads as [`crate::report::CrawlFacts`]. It sets `walk_from_unix`, one second
/// past the crawl's newest edit, only where the record holds none, since a sweep moves it on.
fn write_sidecar(
    report: &CrawlReport,
    snapshot: i64,
    walk_from: Option<i64>,
    mut recorded: serde_json::Map<String, Value>,
) -> Result<()> {
    let meta = json!({
        "app_id": report.app_id,
        "name": report.name,
        "snapshot_unix": snapshot,
        "tool_version": env!("CARGO_PKG_VERSION"),
        "request_url_template": ReviewQuery::new(report.app_id).to_url(),
        "shards": report.shards,
        "pages": report.pages,
        "rows_unique": report.unique,
        "duplicates_this_run": report.duplicates_this_run,
        "valve_total_reviews": report.valve_total,
        "valve_total_positive": report.valve_positive,
        "valve_total_negative": report.valve_negative,
        "review_score_desc": report.review_score_desc,
        "coverage": report.coverage(),
        "complete": report.complete,
        "resumed": report.resumed,
        "shards_restarted": report.shards_restarted,
        "shards_short": report.shards_short,
        "elapsed_secs": report.elapsed.as_secs_f64(),
    });
    if let Value::Object(set) = meta {
        recorded.extend(set);
    }
    if let Some(from) = walk_from
        && recorded
            .get("walk_from_unix")
            .and_then(Value::as_i64)
            .is_none()
    {
        recorded.insert("walk_from_unix".to_owned(), json!(from));
    }
    if recorded
        .get("swept_unix")
        .is_some_and(|v| v.as_i64().is_none())
    {
        recorded.remove("swept_unix");
    }
    for key in ["sweeps", "rows_swept"] {
        if recorded.get(key).is_some_and(|v| v.as_u64().is_none()) {
            recorded.remove(key);
        }
    }
    write_facts(&report.dir, &Value::Object(recorded))
}

/// How far a sweep has got.
#[derive(Debug, Clone, Copy)]
pub struct SweepProgress {
    pub pages: u32,
    pub rows: u64,
}

/// What a sweep brought in.
#[derive(Debug, Clone)]
pub struct SweepReport {
    pub app_id: u32,
    pub dir: PathBuf,
    /// When the sweep started: this computer's clock, or one second past the last recorded sweep
    /// and the newest sweep file where the clock has not passed them. It names the file the sweep
    /// wrote.
    pub started: i64,
    /// The moment this sweep walked back towards: the record's `walk_from_unix`, or the last
    /// sweep's start where the record holds none as a whole number, or the crawl's start where it
    /// was never swept.
    pub watermark: i64,
    pub pages: u32,
    /// Rows written: reviews new since the watermark plus reviews edited since it.
    pub rows: u64,
    /// Of those, reviews created since the watermark that no recorded sweep held.
    pub new: u64,
    /// Of those, copies of reviews the crawl or a recorded sweep already held.
    pub edited: u64,
    /// The capture's unique reviews after the sweep, against Valve's total now.
    pub unique: u64,
    pub valve_total: u64,
    pub stop: StopReason,
    pub elapsed: Duration,
}

impl SweepReport {
    /// Share of Valve's total the capture now holds.
    #[must_use]
    pub fn coverage(&self) -> Option<f64> {
        coverage_of(self.unique, self.valve_total)
    }
}

/// How far past the watermark a sweep reads before trusting that it has seen everything.
///
/// Valve orders by last edit but not exactly: a review can arrive a few places later than
/// its time says. A day of slack costs a few pages and misses nothing a cursor can reach.
const SWEEP_SLACK: i64 = 24 * 60 * 60;

/// Brings a capture up to date: every review written or edited since it was last brought
/// up to date, or since it was crawled, is fetched again and written beside what is there.
///
/// One walk in last-edit order serves for both. A new review's last edit is its creation,
/// so the same walk that finds edits finds arrivals, and it stops at the watermark rather
/// than at the end of the corpus, which is what makes a sweep a few pages rather than a
/// crawl. The rows land in a sweep file of their own and [`crate::capture::Newest`] records
/// which copy of each review now counts, so nothing already captured is touched.
///
/// `stop` is looked at between pages. A sweep told to stop returns [`crate::Error::Stopped`]
/// having changed nothing: its file is left unfinished, under a name no reader looks for, and
/// the next sweep walks again from the same watermark. Once the walk is done it finishes,
/// because what follows is a single answer from Steam and the records it updates.
///
/// # Errors
///
/// Fails if the capture is missing, the network does, or the files cannot be written.
/// [`crate::Error::Refused`] while another job holds the capture's lock, in this program or
/// another.
pub async fn sweep(
    client: &SteamClient,
    app_id: u32,
    out_dir: &std::path::Path,
    stop: &std::sync::atomic::AtomicBool,
    mut on_progress: impl FnMut(SweepProgress),
) -> Result<SweepReport> {
    let begun = Instant::now();
    let dir = crate::embed::latest_snapshot(out_dir, app_id)?;
    // Held in a binding until the record is written: `_` would let go at once.
    let lock = sweep_lock(&dir, app_id)?;
    let mut facts: Value = serde_json::from_slice(&std::fs::read(dir.join("crawl.json"))?)?;
    let watermark = facts
        .get("walk_from_unix")
        .and_then(Value::as_i64)
        .or_else(|| facts.get("swept_unix").and_then(Value::as_i64))
        .or_else(|| facts.get("snapshot_unix").and_then(Value::as_i64))
        .ok_or(crate::Error::MalformedPayload {
            field: "snapshot_unix",
        })?;
    forget_unfinished_sweeps(&dir)?;
    // One past the last recorded sweep and the newest sweep file where the clock has not passed
    // them, as when two sweeps run in one second. The file is named for the start, and each of
    // those files may be counted.
    let floor = crate::capture::shards_of(&dir)?
        .into_iter()
        .map(|(_, start)| start)
        .filter(|start| *start > 0)
        .chain(facts.get("swept_unix").and_then(Value::as_i64))
        .max();
    let started = floor.map_or_else(now_unix, |floor| now_unix().max(floor.saturating_add(1)));

    let mut newest = crate::capture::Newest::load(&dir)?;
    let recorded = match facts.get("swept_unix").and_then(Value::as_i64) {
        Some(swept) => crate::capture::Newest::swept_by(&dir, swept)?,
        None => crate::capture::Newest::default(),
    };
    let path = dir.join(crate::capture::sweep_file(started));
    let walked = walk_since(
        client,
        app_id,
        (watermark, started),
        &path,
        (&mut newest, &recorded),
        (stop, &mut on_progress),
    )
    .await?;
    // Valve's total moves with the corpus, so coverage is re-read against today's figure
    // rather than the one the crawl saw. Asked before anything is recorded, so the records
    // below are written together or not at all.
    let summary = client.summary(app_id).await?;
    if walked.rows == 0 {
        std::fs::remove_file(&path)?;
    } else {
        newest.save(&dir)?;
    }
    let unique = facts
        .get("rows_unique")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(walked.new);
    let valve_total = summary.total_reviews;
    let sweeps = facts.get("sweeps").and_then(Value::as_u64).unwrap_or(0) + 1;
    let swept_rows = facts
        .get("rows_swept")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(walked.rows);
    if let Some(object) = facts.as_object_mut() {
        object.insert("swept_unix".to_owned(), json!(started));
        // The later of this sweep's watermark and the earlier of its start and the newest edit on
        // the first page. The walk asked only up to its start, and a value behind the watermark
        // would bring a shard's review in again as an edit.
        object.insert(
            "walk_from_unix".to_owned(),
            json!(watermark.max(started.min(walked.top))),
        );
        object.insert("sweeps".to_owned(), json!(sweeps));
        object.insert("rows_swept".to_owned(), json!(swept_rows));
        object.insert("rows_unique".to_owned(), json!(unique));
        object.insert("valve_total_reviews".to_owned(), json!(valve_total));
        object.insert(
            "valve_total_positive".to_owned(),
            json!(summary.total_positive),
        );
        object.insert(
            "valve_total_negative".to_owned(),
            json!(summary.total_negative),
        );
        object.insert(
            "review_score_desc".to_owned(),
            json!(summary.review_score_desc),
        );
        object.insert(
            "coverage".to_owned(),
            json!(coverage_of(unique, valve_total)),
        );
    }
    write_facts(&dir, &facts)?;
    // The lookup takes the lock itself, and a second handle in one program is refused as another
    // program's would be.
    drop(lock);
    // The store is being asked about the game anyway. A name it will not give is not a reason
    // to fail a sweep that has already landed.
    let _ = name_where_missing(client, out_dir, app_id).await;

    Ok(SweepReport {
        app_id,
        dir,
        started,
        watermark,
        pages: walked.pages,
        rows: walked.rows,
        new: walked.new,
        edited: walked.edited,
        unique,
        valve_total,
        stop: walked.stop,
        elapsed: begun.elapsed(),
    })
}

/// The file a sweep, a crawl's record step and a name lookup lock. It stays in the capture,
/// since removing it would let a program that has it open lock a file no other program can find.
const SWEEP_LOCK: &str = "sweep.lock";

/// Takes the lock on a capture, or refuses where it is held. A sweep holds it until its record is
/// written. A crawl and a name lookup hold it from their read of the record to their write. The
/// system lets go when the file is dropped or the program ends.
///
/// The file is opened to write, never to append, because Windows refuses to lock a handle
/// opened to append.
fn sweep_lock(dir: &std::path::Path, app_id: u32) -> Result<std::fs::File> {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join(SWEEP_LOCK))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(crate::Error::Refused(format!(
            "app {app_id}: another job is writing to its download, in this program or another. \
             Try again once it ends."
        ))),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
}

/// Deletes what sweeps that never finished left behind. Each sweep writes a file named for when
/// it started, so a stopped one leaves a file no later sweep will write over, and nothing in it
/// was ever counted: `Newest` is saved only once a sweep's file is whole. Called under the
/// sweep's lock, so nothing it deletes is a file a running sweep is writing.
fn forget_unfinished_sweeps(dir: &std::path::Path) -> Result<()> {
    for entry in std::fs::read_dir(dir)?.filter_map(std::result::Result::ok) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("sweep-") && name.ends_with(".parquet.partial") {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Writes a capture's crawl record beside itself and moves it over the old one, so a record is
/// never half one version and half the next.
fn write_facts(dir: &std::path::Path, facts: &Value) -> Result<()> {
    let partial = dir.join("crawl.json.partial");
    std::fs::write(&partial, serde_json::to_vec_pretty(facts)?)?;
    std::fs::rename(partial, dir.join("crawl.json"))?;
    Ok(())
}

/// Asks the store for a game's name where its capture recorded none, as captures made before
/// the crawler asked for it did, and keeps it. Returns the name where one was found and kept.
///
/// # Errors
///
/// Fails if there is no capture or its crawl record cannot be read or written.
/// [`crate::Error::Refused`] where another job holds the capture's lock once the store has
/// answered.
pub async fn name_where_missing(
    client: &SteamClient,
    out_dir: &std::path::Path,
    app_id: u32,
) -> Result<Option<String>> {
    let dir = crate::embed::latest_snapshot(out_dir, app_id)?;
    let first: Value = serde_json::from_slice(&std::fs::read(dir.join("crawl.json"))?)?;
    if has_name(&first) {
        return Ok(None);
    }
    let Some(name) = client.name(app_id).await.filter(|name| !name.is_empty()) else {
        return Ok(None);
    };
    // The store is asked first, so no request runs under the lock. A sweep may have landed
    // meanwhile, and the record is read again to keep what it wrote.
    let _lock = sweep_lock(&dir, app_id)?;
    let mut facts: Value = serde_json::from_slice(&std::fs::read(dir.join("crawl.json"))?)?;
    if has_name(&facts) {
        return Ok(None);
    }
    if let Some(object) = facts.as_object_mut() {
        object.insert("name".to_owned(), json!(name));
    }
    write_facts(&dir, &facts)?;
    Ok(Some(name))
}

/// Whether a crawl record holds a name that is not empty.
fn has_name(facts: &Value) -> bool {
    facts
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| !name.is_empty())
}

/// The newest `timestamp_updated` among the reviews with an id, 0 for one without the stamp and
/// `i64::MIN` where no review has an id.
fn newest_on(reviews: &[Value]) -> i64 {
    reviews
        .iter()
        .filter(|review| review.get("recommendationid").is_some_and(Value::is_string))
        .map(|review| {
            review
                .get("timestamp_updated")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(i64::MIN)
}

/// What one walk in last-edit order brought in.
struct Walked {
    pages: u32,
    rows: u64,
    new: u64,
    edited: u64,
    stop: StopReason,
    /// The newest edit on the first page, or `i64::MIN` where that page held no review with an id.
    top: i64,
}

/// Walks the corpus newest edit first, writing every review touched since `watermark` that no
/// recorded sweep holds already to `path` and noting each in `newest` and counting each new or
/// edited against `recorded`, the copies the recorded sweeps hold, until it is safely past the
/// watermark or `halt` is set.
///
/// The walk asks for the reviews written up to `started`, which is every review there is when
/// it starts. Steam keeps each answer it gives a caller without a key for ten minutes and
/// gives it to anyone who asks the same thing, so a sweep asking what any other caller asked
/// could be served pages from before the latest edits, and would miss them until the next
/// sweep. Bounded by its own start, every sweep asks
/// something nobody has asked before.
async fn walk_since(
    client: &SteamClient,
    app_id: u32,
    (watermark, started): (i64, i64),
    path: &std::path::Path,
    (newest, recorded): (&mut crate::capture::Newest, &crate::capture::Newest),
    (halt, on_progress): (
        &std::sync::atomic::AtomicBool,
        &mut impl FnMut(SweepProgress),
    ),
) -> Result<Walked> {
    let mut writer = CaptureWriter::create(path, app_id)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut cursor = "*".to_owned();
    let mut pages: u32 = 0;
    let (mut new, mut edited) = (0_u64, 0_u64);
    let mut top = i64::MIN;

    let stop = loop {
        if halt.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(crate::Error::Stopped);
        }
        let query = ReviewQuery::new(app_id)
            .order(SortOrder::Updated)
            .window(CORPUS_EPOCH, started)
            .cursor(cursor.clone());
        let page = client.fetch(&query, app_id).await?;
        pages = pages.saturating_add(1);
        if page.reviews.is_empty() {
            break StopReason::Exhausted;
        }

        let newest_on_page = newest_on(&page.reviews);
        let mut fresh: Vec<&Value> = Vec::new();
        for review in &page.reviews {
            let Some(id) = review.get("recommendationid").and_then(Value::as_str) else {
                continue;
            };
            let stamp = |field: &str| review.get(field).and_then(Value::as_i64).unwrap_or(0);
            let updated = stamp("timestamp_updated");
            if updated < watermark {
                continue;
            }
            // A copy a recorded sweep holds at this edit or a later one is already counted, and
            // still shows how far back this page reaches.
            if recorded
                .copies
                .get(id)
                .is_some_and(|&(held, _)| held >= updated)
            {
                continue;
            }
            if !seen.insert(id.to_owned()) {
                continue;
            }
            // Created since the watermark and in no recorded sweep file is a review the record
            // has not counted. Anything else is a copy of one it has.
            if stamp("timestamp_created") >= watermark && !recorded.copies.contains_key(id) {
                new += 1;
            } else {
                edited += 1;
            }
            newest.record(id, updated, started);
            fresh.push(review);
        }
        if pages == 1 {
            top = newest_on_page;
        }
        writer.write(&fresh)?;
        on_progress(SweepProgress {
            pages,
            rows: writer.rows(),
        });

        if newest_on_page < watermark - SWEEP_SLACK {
            break StopReason::Exhausted;
        }
        let Some(next) = page.cursor else {
            break StopReason::NoCursor;
        };
        if next == cursor {
            break StopReason::CursorRepeated;
        }
        cursor = next;
    };

    Ok(Walked {
        pages,
        rows: writer.close()?,
        new,
        edited,
        stop,
        top,
    })
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn now_unix() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in;

    fn report(unique: u64, total: u64) -> CrawlReport {
        CrawlReport {
            app_id: 1,
            shards: 1,
            pages: 1,
            unique,
            duplicates_this_run: 0,
            valve_total: total,
            valve_positive: 0,
            valve_negative: 0,
            name: String::new(),
            review_score_desc: String::new(),
            elapsed: Duration::from_secs(1),
            dir: PathBuf::new(),
            complete: true,
            resumed: false,
            shards_restarted: 0,
            shards_short: 0,
        }
    }

    #[test]
    fn a_window_that_stalls_a_quarter_of_the_way_short_is_not_accepted() {
        // The shard that exposed this returned 12,600 of an expected 18,184 and was recorded
        // as finished. Walking the identical window again returned all of them.
        assert!(fell_short(12_600, 18_184));
        assert!(fell_short(18_297, 24_340));
        assert!(fell_short(12_699, 17_991));
    }

    #[test]
    fn ordinary_drift_between_planning_and_walking_is_not_a_stall() {
        // Reviews are written and deleted while a crawl runs, so expectation is never exact.
        // The widest honest gap observed was eight reviews in 39,278.
        assert!(!fell_short(39_276, 39_278));
        assert!(!fell_short(29_048, 29_050));
        assert!(!fell_short(2_909, 2_909));
        assert!(!fell_short(35_856, 35_860));
    }

    #[test]
    fn a_small_window_is_judged_by_reviews_missed_rather_than_by_share() {
        // Losing three of twenty reviews is 15% and means nothing. The floor stops a tiny
        // window from being walked three times over noise.
        assert!(!fell_short(17, 20));
        assert!(fell_short(60, 200), "a real shortfall must still be caught");
    }

    #[test]
    fn a_window_valve_reports_nothing_for_can_never_be_short() {
        assert!(!fell_short(0, 0));
    }

    #[test]
    fn coverage_is_the_share_of_valves_own_total() {
        assert_eq!(report(2909, 2909).coverage(), Some(1.0));
        let partial = report(21, 2909);
        assert!((partial.coverage().unwrap() - 0.007_218).abs() < 1e-6);
    }

    #[test]
    fn coverage_is_unknown_rather_than_perfect_when_valve_reports_nothing() {
        assert_eq!(report(100, 0).coverage(), None);
    }

    #[test]
    fn stop_reasons_round_trip_to_the_strings_stored_in_state() {
        assert_eq!(StopReason::Exhausted.as_str(), "Exhausted");
        assert_eq!(StopReason::CursorRepeated.as_str(), "CursorRepeated");
        assert_eq!(StopReason::NoCursor.as_str(), "NoCursor");
    }

    #[test]
    fn a_shortfall_of_exactly_the_floor_is_drift() {
        assert!(!fell_short(80, 100));
        assert!(fell_short(79, 100));
    }

    #[test]
    fn a_walk_landing_exactly_on_the_ratio_is_not_short() {
        assert!(!fell_short(950, 1000));
        assert!(fell_short(949, 1000));
    }

    const APP: u32 = 7;
    const DAY: i64 = 24 * 60 * 60;
    /// What the stand-in serves a page of, whatever is asked for: Valve caps a page too.
    const PAGE: usize = 10;

    fn review(id: &str, created: i64, updated: i64) -> Value {
        json!({
            "recommendationid": id,
            "timestamp_created": created,
            "timestamp_updated": updated,
            "review": format!("review {id}"),
            "language": "english",
            "voted_up": true,
        })
    }

    fn summary_of(total: usize) -> stand_in::Answer {
        stand_in::Answer::json(&json!({"response": {
            "query_summary": {
                "num_reviews": 0,
                "total_reviews": total,
                "total_positive": total,
                "total_negative": 0,
                "review_score_desc": "Very Positive",
            },
            "cursor": "*",
        }}))
    }

    /// Whether the client is asking only for Valve's totals, which it does with a page of one.
    fn counting(asked: &stand_in::Asked) -> bool {
        asked.input()["num_per_page"] == 1
    }

    fn name_of_the_game(name: &str) -> stand_in::Answer {
        stand_in::Answer::json(&json!({
            APP.to_string(): {"success": true, "data": {"name": name}}
        }))
    }

    /// Where a cursor the stand-in handed out starts: `*` at the beginning, `at{n}` after `n`.
    fn offset(asked: &stand_in::Asked) -> usize {
        asked.input()["cursor"]
            .as_str()
            .and_then(|cursor| cursor.strip_prefix("at"))
            .and_then(|at| at.parse().ok())
            .unwrap_or(0)
    }

    fn page_of(served: &[Value], from: usize) -> stand_in::Answer {
        let to = (from + PAGE).min(served.len());
        let next = if from < served.len() { to } else { from };
        let reviews = served.get(from..to).unwrap_or(&[]);
        stand_in::Answer::json(&json!({"response": {
            "query_summary": {"num_reviews": reviews.len()},
            "reviews": reviews,
            "cursor": format!("at{next}"),
        }}))
    }

    /// Valve as far as a crawl can tell: `reviews` newest first, a page at a time, counted and
    /// served by window, with the name of the game beside them. The first `stalls` walks of the
    /// window starting at `stalled` end after their first page, as Steam's do now and then. It
    /// also serves the first page in last-edit order, which a crawl asks before it plans.
    fn valve(
        reviews: Vec<Value>,
        stalled: Option<i64>,
        stalls: usize,
    ) -> impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static {
        let walks = std::sync::Mutex::new(0_usize);
        move |asked| {
            if asked.path() == "/api/appdetails" {
                return name_of_the_game("A Game");
            }
            let input = asked.input();
            let (start, end) = (
                input["date_range_start"].as_i64().unwrap_or(i64::MIN),
                input["date_range_end"].as_i64().unwrap_or(i64::MAX),
            );
            let mut served: Vec<Value> = reviews
                .iter()
                .filter(|review| {
                    (start..=end).contains(&review["timestamp_created"].as_i64().unwrap())
                })
                .cloned()
                .collect();
            let by_edit = input["filter"] == 2;
            let order = if by_edit {
                "timestamp_updated"
            } else {
                "timestamp_created"
            };
            served.sort_by_key(|review| std::cmp::Reverse(review[order].as_i64()));
            if counting(asked) {
                let distinct: HashSet<&str> = served
                    .iter()
                    .filter_map(|review| review["recommendationid"].as_str())
                    .collect();
                return summary_of(distinct.len());
            }
            let from = offset(asked);
            if stalled == Some(start) && !by_edit {
                let mut walks = walks.lock().unwrap();
                if from == 0 {
                    *walks += 1;
                }
                if *walks <= stalls && from > 0 {
                    return page_of(&served, served.len());
                }
            }
            page_of(&served, from)
        }
    }

    /// Forty reviews in 2014 and thirty in 2024, so that a target of forty plans the two years
    /// as two windows, and one review of 2024 served twice, as a page boundary moving under a
    /// cursor serves one.
    fn corpus() -> Vec<Value> {
        let year = |at: i64, prefix: &'static str, count: i64| {
            (0..count).map(move |n| {
                let when = at + n * DAY;
                review(&format!("{prefix}{n}"), when, when)
            })
        };
        let mut reviews: Vec<Value> = year(1_400_000_000, "early-", 40)
            .chain(year(1_710_000_000, "late-", 30))
            .collect();
        reviews.push(review(
            "late-4",
            1_710_000_000 + 4 * DAY,
            1_710_000_000 + 4 * DAY,
        ));
        reviews
    }

    fn client_of(server: &stand_in::Server) -> SteamClient {
        SteamClient::new(Duration::ZERO)
            .unwrap()
            .with_store(&server.origin())
            .with_api(&server.origin())
    }

    fn options(out_dir: &std::path::Path) -> CrawlOptions {
        CrawlOptions {
            out_dir: out_dir.to_path_buf(),
            concurrency: 2,
            shard_target: 40,
            resume: true,
        }
    }

    fn ids_in(snapshot: &std::path::Path) -> HashSet<String> {
        let mut ids = HashSet::new();
        crate::capture::for_each_body(snapshot, |id, _, _| {
            ids.insert(id.to_owned());
            Ok(())
        })
        .unwrap();
        ids
    }

    fn facts(dir: &std::path::Path) -> Value {
        serde_json::from_slice(&std::fs::read(dir.join("crawl.json")).unwrap()).unwrap()
    }

    #[tokio::test]
    async fn a_crawl_captures_every_review_once_and_records_what_it_found() {
        let out = crate::tempdir::Dir::new();
        let server = stand_in::Server::new(valve(corpus(), None, 0));
        let mut done = Vec::new();
        let before = now_unix();
        let report = crawl(&client_of(&server), APP, &options(out.path()), |progress| {
            done.push((progress.shards_done, progress.shards_total));
        })
        .await
        .unwrap();
        let after = now_unix();

        assert_eq!(report.name, "A Game");
        assert_eq!(
            (report.shards, report.unique, report.valve_total),
            (2, 70, 70)
        );
        assert_eq!(report.duplicates_this_run, 1);
        assert!(report.complete && !report.resumed);
        assert_eq!((report.shards_restarted, report.shards_short), (0, 0));
        assert_eq!(done, [(1, 2), (2, 2)]);

        let stamp: i64 = report
            .dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("snapshot="))
            .and_then(|stamp| stamp.parse().ok())
            .unwrap();
        assert!((before..=after).contains(&stamp), "{stamp}");
        assert_eq!(
            report.dir,
            out.path()
                .join(format!("appid={APP}"))
                .join(format!("snapshot={stamp}"))
        );
        assert_eq!(ids_in(&report.dir).len(), 70);
        let facts = facts(&report.dir);
        assert_eq!(facts["rows_unique"], 70);
        assert_eq!(facts["complete"], true);
        assert_eq!(facts["name"], "A Game");
    }

    #[tokio::test]
    async fn a_window_steam_stops_serving_early_is_walked_again() {
        let out = crate::tempdir::Dir::new();
        let server = stand_in::Server::new(valve(corpus(), Some(CORPUS_EPOCH), 1));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert_eq!((report.shards_restarted, report.shards_short), (1, 0));
        assert!(report.complete);
        assert_eq!(report.unique, 70);
    }

    type Held = Arc<std::sync::Mutex<Vec<Value>>>;

    fn held(reviews: Vec<Value>) -> Held {
        Arc::new(std::sync::Mutex::new(reviews))
    }

    /// Valve over a corpus the test can change: counted and served by creation window, in
    /// last-edit order where asked (`filter` 2), else newest written first.
    fn changing_steam(
        corpus: &Held,
    ) -> impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static {
        let corpus = Arc::clone(corpus);
        move |asked| {
            if asked.path() == "/api/appdetails" {
                return name_of_the_game("A Game");
            }
            let input = asked.input();
            let (start, end) = (
                input["date_range_start"].as_i64().unwrap_or(i64::MIN),
                input["date_range_end"].as_i64().unwrap_or(i64::MAX),
            );
            let order = if input["filter"] == 2 {
                "timestamp_updated"
            } else {
                "timestamp_created"
            };
            let mut served: Vec<Value> = corpus
                .lock()
                .unwrap()
                .iter()
                .filter(|review| {
                    (start..=end).contains(&review["timestamp_created"].as_i64().unwrap())
                })
                .cloned()
                .collect();
            served.sort_by_key(|review| std::cmp::Reverse(review[order].as_i64()));
            if counting(asked) {
                return summary_of(served.len());
            }
            page_of(&served, offset(asked))
        }
    }

    /// The newest `date_range_end` among the queries that are not in last-edit order.
    fn newest_window_end(server: &stand_in::Server) -> Option<i64> {
        server
            .asked()
            .iter()
            .filter(|asked| asked.path() != "/api/appdetails")
            .map(stand_in::Asked::input)
            .filter(|input| input["filter"] != 2)
            .filter_map(|input| input["date_range_end"].as_i64())
            .max()
    }

    fn newest_edit_kept(out: &std::path::Path) -> Option<i64> {
        CrawlState::open(&out.join("state.sqlite"))
            .unwrap()
            .newest_edit(1)
            .unwrap()
    }

    #[tokio::test]
    async fn a_crawl_keeps_steams_newest_edit_and_ends_its_windows_at_its_own_clock() {
        let out = crate::tempdir::Dir::new();
        let stamped = now_unix() - 3600;
        let corpus = held(vec![
            review("r1", stamped - 1000, stamped - 1000),
            review("r2", stamped - 500, stamped - 500),
        ]);
        let server = stand_in::Server::new(changing_steam(&corpus));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();

        assert_eq!(report.unique, 2);
        assert_eq!(newest_edit_kept(out.path()), Some(stamped - 500));
        assert_eq!(
            newest_window_end(&server),
            facts(&report.dir)["snapshot_unix"].as_i64()
        );
    }

    #[tokio::test]
    async fn a_crawl_with_the_clock_behind_the_newest_edit_plans_up_to_that_edit() {
        let out = crate::tempdir::Dir::new();
        let now = now_unix();
        let corpus = held(vec![
            review("r1", now - 1000, now + 1800),
            review("r2", now + 900, now + 900),
        ]);
        let server = stand_in::Server::new(changing_steam(&corpus));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();

        assert_eq!(report.unique, 2);
        assert_eq!(newest_window_end(&server), Some(now + 1800));
        assert_eq!(newest_edit_kept(out.path()), Some(now + 1800));
    }

    #[tokio::test]
    async fn a_crawl_served_reviews_with_no_edit_time_keeps_no_newest_edit() {
        let out = crate::tempdir::Dir::new();
        let now = now_unix();
        let mut reviews = vec![
            review("r1", now - 1000, now - 1000),
            review("r2", now - 500, now - 500),
        ];
        for review in &mut reviews {
            review.as_object_mut().unwrap().remove("timestamp_updated");
        }
        let server = stand_in::Server::new(changing_steam(&held(reviews)));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();

        assert_eq!(report.unique, 2);
        assert_eq!(newest_edit_kept(out.path()), None);
    }

    #[tokio::test]
    async fn a_crawl_whose_newest_windows_hold_no_review_still_keeps_the_newest_edit() {
        let out = crate::tempdir::Dir::new();
        let now = now_unix();
        let early = CORPUS_EPOCH + 400 * DAY;
        let corpus = held(vec![
            review("r1", early, now - 100),
            review("r2", early + DAY, early + DAY),
        ]);
        let server = stand_in::Server::new(changing_steam(&corpus));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();

        assert_eq!(report.unique, 2);
        assert!(report.complete);
        assert_eq!(newest_edit_kept(out.path()), Some(now - 100));
    }

    #[tokio::test]
    async fn a_resumed_crawl_asks_steam_no_first_page_and_keeps_the_edit_it_began_with() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let kept = newest_edit_kept(out.path());
        assert_eq!(kept, Some(1_710_000_000 + 29 * DAY));

        let server = stand_in::Server::new(valve(corpus(), None, 0));
        let resumed = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert!(resumed.resumed && resumed.complete);
        assert_eq!(resumed.dir, short.dir);
        assert!(
            server
                .asked()
                .iter()
                .all(|asked| asked.input()["filter"] != 2)
        );
        assert_eq!(newest_edit_kept(out.path()), kept);
    }

    #[tokio::test]
    async fn a_crawl_an_earlier_release_began_with_no_newest_edit_is_resumed() {
        let out = crate::tempdir::Dir::new();
        short_crawl(out.path()).await;
        rusqlite::Connection::open(out.path().join("state.sqlite"))
            .unwrap()
            .execute("DELETE FROM newest_edit", [])
            .unwrap();
        let resumed = resumed_crawl(out.path()).await.unwrap();
        assert!(resumed.resumed && resumed.complete);
        assert_eq!(resumed.unique, 70);
        assert_eq!(newest_edit_kept(out.path()), None);
    }

    async fn swept(server: &stand_in::Server, out: &std::path::Path) -> SweepReport {
        let stop = std::sync::atomic::AtomicBool::new(false);
        sweep(&client_of(server), APP, out, &stop, |_| {})
            .await
            .unwrap()
    }

    fn walk_from(dir: &std::path::Path) -> Option<i64> {
        facts(dir)["walk_from_unix"].as_i64()
    }

    /// `inner`, which then adds `late` to `corpus` once it has answered its first query for a page
    /// of reviews, as reviews written while a crawl plans are.
    fn arriving_after_the_first_page(
        inner: impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static,
        corpus: &Held,
        late: Vec<Value>,
    ) -> impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static {
        let arriving = Arc::clone(corpus);
        let arrived = std::sync::atomic::AtomicBool::new(false);
        move |asked| {
            let answer = inner(asked);
            if asked.path() != "/api/appdetails"
                && !counting(asked)
                && !arrived.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                arriving.lock().unwrap().extend(late.clone());
            }
            answer
        }
    }

    #[tokio::test]
    async fn a_sweep_after_a_crawl_brings_in_what_steam_stamped_after_the_crawls_newest_edit() {
        let out = crate::tempdir::Dir::new();
        let stamped = now_unix() - 3600;
        let corpus = held(vec![
            review("r1", stamped - 1000, stamped - 1000),
            review("r2", stamped - 500, stamped - 500),
        ]);
        let server = stand_in::Server::new(changing_steam(&corpus));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert_eq!(walk_from(&report.dir), Some(stamped - 499));

        corpus.lock().unwrap()[0] = review("r1", stamped - 1000, stamped + 6);
        corpus
            .lock()
            .unwrap()
            .push(review("r3", stamped + 5, stamped + 5));
        let swept = swept(&server, out.path()).await;
        assert_eq!((swept.rows, swept.new, swept.edited), (2, 1, 1));
        assert_eq!(facts(&report.dir)["rows_unique"], 3);
    }

    #[tokio::test]
    async fn a_sweep_straight_after_a_crawl_writes_no_row() {
        let out = crate::tempdir::Dir::new();
        let now = now_unix();
        let corpus = held(vec![
            review("r1", now - 1000, now - 1000),
            review("r2", now - 500, now - 500),
        ]);
        let server = stand_in::Server::new(changing_steam(&corpus));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        let swept = swept(&server, out.path()).await;
        assert_eq!((swept.rows, swept.new, swept.edited), (0, 0, 0));
        assert_eq!(facts(&report.dir)["rows_unique"], 2);
    }

    #[tokio::test]
    async fn a_review_that_arrives_while_a_crawl_plans_is_counted_once() {
        let out = crate::tempdir::Dir::new();
        let stamped = now_unix() - 3600;
        let corpus = held(vec![
            review("r1", stamped - 1000, stamped - 1000),
            review("r2", stamped - 500, stamped - 500),
        ]);
        let server = stand_in::Server::new(arriving_after_the_first_page(
            changing_steam(&corpus),
            &corpus,
            vec![review("r3", stamped + 1, stamped + 1)],
        ));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert_eq!(report.unique, 2);

        let swept = swept(&server, out.path()).await;
        assert_eq!((swept.rows, swept.new, swept.edited), (1, 1, 0));
        assert_eq!(facts(&report.dir)["rows_unique"], 3);
        let mut ids: Vec<String> = ids_in(&report.dir).into_iter().collect();
        ids.sort();
        assert_eq!(ids, ["r1", "r2", "r3"]);
    }

    #[tokio::test]
    async fn a_crawl_whose_newest_windows_hold_no_review_starts_the_first_walk_past_its_newest_edit()
     {
        let out = crate::tempdir::Dir::new();
        let early = CORPUS_EPOCH + 2 * 365 * DAY;
        let edit = now_unix() - 1000;
        let mut reviews: Vec<Value> = (0..50_i64)
            .map(|i| review(&format!("r{i}"), early + i * DAY, early + i * DAY))
            .collect();
        reviews[0] = review("r0", early, edit);
        let server = stand_in::Server::new(changing_steam(&held(reviews)));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert_eq!(report.unique, 50);
        assert_eq!(walk_from(&report.dir), Some(edit + 1));

        let swept = swept(&server, out.path()).await;
        assert_eq!((swept.rows, swept.new, swept.edited), (0, 0, 0));
    }

    #[tokio::test]
    async fn a_crawl_with_the_clock_behind_the_newest_edit_starts_the_first_walk_past_that_edit() {
        let out = crate::tempdir::Dir::new();
        let now = now_unix();
        let corpus = held(vec![
            review("r1", now - 1000, now + 1800),
            review("r2", now + 900, now + 900),
        ]);
        let server = stand_in::Server::new(changing_steam(&corpus));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert_eq!(walk_from(&report.dir), Some(now + 1801));

        let swept = swept(&server, out.path()).await;
        assert_eq!((swept.rows, swept.new, swept.edited), (0, 0, 0));
        assert_eq!(facts(&report.dir)["rows_unique"], 2);
    }

    #[tokio::test]
    async fn reviews_that_arrive_before_the_plan_counts_do_not_make_a_window_short() {
        let out = crate::tempdir::Dir::new();
        let stamped = now_unix() - 3600;
        let corpus = held(
            (0..5_i64)
                .map(|i| review(&format!("r{i}"), stamped - 1000 + i, stamped - 1000 + i))
                .collect(),
        );
        let late = (0..30_i64)
            .map(|i| review(&format!("late{i}"), stamped + 1 + i, stamped + 1 + i))
            .collect();
        let server = stand_in::Server::new(arriving_after_the_first_page(
            changing_steam(&corpus),
            &corpus,
            late,
        ));
        let report = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert!(report.complete);
        assert_eq!(report.unique, 5);
        assert_eq!(report.shards_short, 0);
        assert_eq!(report.duplicates_this_run, 0);

        let swept = swept(&server, out.path()).await;
        assert_eq!((swept.rows, swept.new, swept.edited), (30, 30, 0));
        assert_eq!(facts(&report.dir)["rows_unique"], 35);
    }

    #[tokio::test]
    async fn two_crawls_of_a_quiet_game_a_second_apart_ask_different_window_queries() {
        let now = now_unix();
        let early = CORPUS_EPOCH + 2 * 365 * DAY;
        let mut reviews: Vec<Value> = (0..50_i64)
            .map(|i| review(&format!("r{i}"), early + i * DAY, early + i * DAY))
            .collect();
        reviews[0] = review("r0", early, now - 1000);
        let corpus = held(reviews);
        let (first, second) = (
            stand_in::Server::new(changing_steam(&corpus)),
            stand_in::Server::new(changing_steam(&corpus)),
        );
        let (out_a, out_b) = (crate::tempdir::Dir::new(), crate::tempdir::Dir::new());
        crawl(&client_of(&first), APP, &options(out_a.path()), |_| {})
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1200)).await;
        crawl(&client_of(&second), APP, &options(out_b.path()), |_| {})
            .await
            .unwrap();

        let windows = |server: &stand_in::Server| -> Vec<(i64, i64)> {
            server
                .asked()
                .iter()
                .filter(|asked| asked.path() != "/api/appdetails")
                .map(stand_in::Asked::input)
                .filter(|input| input["filter"] != 2)
                .filter_map(|input| {
                    Some((
                        input["date_range_start"].as_i64()?,
                        input["date_range_end"].as_i64()?,
                    ))
                })
                .collect()
        };
        let before = windows(&first);
        assert!(
            windows(&second).iter().any(|query| !before.contains(query)),
            "a second crawl asks a window the first did not, so Steam's cache cannot serve it"
        );
    }

    #[tokio::test]
    async fn a_crawl_resumed_after_a_sweep_keeps_the_sweeps_walk_start() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let top = 1_710_000_000 + 29 * DAY;
        assert_eq!(walk_from(&short.dir), Some(top + 1));

        let pages = vec![vec![review("arrived", top + 100, top + 100)]];
        let server = stand_in::Server::new(edits(pages, 71));
        let stop = std::sync::atomic::AtomicBool::new(false);
        sweep(&client_of(&server), APP, out.path(), &stop, |_| {})
            .await
            .unwrap();
        assert_eq!(walk_from(&short.dir), Some(top + 100));

        let resumed = resumed_crawl(out.path()).await.unwrap();
        assert!(resumed.complete);
        assert_eq!(walk_from(&resumed.dir), Some(top + 100));
    }

    #[tokio::test]
    async fn a_resumed_crawl_with_no_record_still_records_its_walk_start() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        std::fs::remove_file(short.dir.join("crawl.json")).unwrap();
        let resumed = resumed_crawl(out.path()).await.unwrap();
        assert!(resumed.resumed);
        assert_eq!(walk_from(&resumed.dir), Some(1_710_000_000 + 29 * DAY + 1));
    }

    #[tokio::test]
    async fn a_resumed_crawl_that_kept_no_newest_edit_keeps_every_review_and_sets_no_walk_start() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        std::fs::remove_file(short.dir.join("crawl.json")).unwrap();
        rusqlite::Connection::open(out.path().join("state.sqlite"))
            .unwrap()
            .execute("DELETE FROM newest_edit", [])
            .unwrap();
        let resumed = resumed_crawl(out.path()).await.unwrap();
        assert!(resumed.complete);
        assert_eq!(resumed.unique, 70);
        assert_eq!(walk_from(&resumed.dir), None);
    }

    #[test]
    fn the_newest_edit_of_a_page_is_read_from_the_reviews_with_an_id() {
        assert_eq!(newest_on(&[]), i64::MIN);
        assert_eq!(newest_on(&[json!({"timestamp_updated": 9})]), i64::MIN);
        let mut stampless = review("r1", 1, 1);
        stampless
            .as_object_mut()
            .unwrap()
            .remove("timestamp_updated");
        assert_eq!(newest_on(std::slice::from_ref(&stampless)), 0);
        assert_eq!(
            newest_on(&[review("r2", 1, 5), stampless, review("r3", 1, 7)]),
            7
        );
    }

    #[tokio::test]
    async fn a_window_still_short_after_every_walk_is_left_for_the_next_run_to_finish() {
        let out = crate::tempdir::Dir::new();
        let stalling = stand_in::Server::new(valve(corpus(), Some(CORPUS_EPOCH), 3));
        let first = crawl(&client_of(&stalling), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert_eq!((first.shards_restarted, first.shards_short), (1, 1));
        assert!(!first.complete);
        assert!(!first.resumed);
        let walks_of_the_stalled_window: Vec<Value> = stalling
            .asked()
            .iter()
            .map(stand_in::Asked::input)
            .filter(|input| {
                input["date_range_start"] == CORPUS_EPOCH
                    && input["cursor"] == "*"
                    && input["num_per_page"] != 1
                    && input["filter"] != 2
            })
            .map(|input| input["num_per_page"].clone())
            .collect();
        assert_eq!(
            walks_of_the_stalled_window,
            [json!(100), json!(99), json!(98)],
            "a walk asked again as it was asked before is served from Steam's cache"
        );
        assert_eq!(facts(&first.dir)["complete"], false);

        let serving = stand_in::Server::new(valve(corpus(), None, 0));
        let second = crawl(&client_of(&serving), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert!(second.resumed && second.complete);
        assert_eq!(
            second.dir, first.dir,
            "a resumed crawl lands beside the first"
        );
        assert_eq!(
            second.shards, 1,
            "only the unfinished window is walked again"
        );
        assert_eq!(second.unique, 70);
        assert_eq!(ids_in(&second.dir).len(), 70);
    }

    const WATERMARK: i64 = 1_700_000_000;

    /// A capture of two reviews, crawled at [`WATERMARK`], as a sweep finds it.
    fn captured(out: &std::path::Path, facts: &Value) -> PathBuf {
        captured_of(out, APP, facts)
    }

    fn captured_of(out: &std::path::Path, app_id: u32, facts: &Value) -> PathBuf {
        let dir = out
            .join(format!("appid={app_id}"))
            .join(format!("snapshot={WATERMARK}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut writer = CaptureWriter::create(&dir.join("shard-0000.parquet"), app_id).unwrap();
        let (one, two) = (review("r1", 500, 500), review("r2", 600, 600));
        writer.write(&[&one, &two]).unwrap();
        writer.close().unwrap();
        write_facts(&dir, facts).unwrap();
        dir
    }

    /// A finished sweep file holding `reviews`.
    fn written_to(path: &std::path::Path, reviews: &[Value]) {
        let mut writer = CaptureWriter::create(path, APP).unwrap();
        let rows: Vec<&Value> = reviews.iter().collect();
        writer.write(&rows).unwrap();
        writer.close().unwrap();
    }

    /// Steam answering a sweep: `pages` in last-edit order, and `total` as the corpus's size.
    fn edits(
        pages: Vec<Vec<Value>>,
        total: usize,
    ) -> impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static {
        move |asked| {
            if asked.path() == "/api/appdetails" {
                return name_of_the_game("A Game");
            }
            if counting(asked) {
                return summary_of(total);
            }
            assert_eq!(asked.input()["filter"], 2, "the sweep walks by last edit");
            assert!(
                asked.input()["date_range_end"].as_i64() > Some(WATERMARK),
                "the sweep asks up to its own start, which no other caller has asked"
            );
            let at = offset(asked);
            let next = if at < pages.len() { at + 1 } else { at };
            let reviews = pages.get(at).cloned().unwrap_or_default();
            stand_in::Answer::json(&json!({"response": {
                "query_summary": {"num_reviews": reviews.len()},
                "reviews": reviews,
                "cursor": format!("at{next}"),
            }}))
        }
    }

    #[tokio::test]
    async fn a_sweep_brings_in_what_was_written_or_edited_since_the_watermark() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "swept_unix": WATERMARK, "rows_unique": 2, "name": "A Game"}),
        );
        // Written while the sweep before this one was walking, and fetched by it.
        let mut newest = crate::capture::Newest::default();
        newest.record("r9", WATERMARK + 2, WATERMARK);
        newest.save(&dir).unwrap();
        std::fs::write(dir.join("sweep-1.parquet.partial"), b"stopped").unwrap();
        let recorded = dir.join(crate::capture::sweep_file(WATERMARK));
        written_to(&recorded, &[review("r9", WATERMARK + 2, WATERMARK + 2)]);

        let w = WATERMARK;
        let pages = vec![
            vec![
                review("r3", w + 50, w + 50),
                review("r9", w + 2, w + 30),
                review("r1", 500, w + 10),
            ],
            vec![review("r4", w, w), review("r3", w + 50, w + 50)],
            // Exactly a day behind the watermark is still inside the slack.
            vec![review("r2", 600, w - DAY)],
            // Out of order, as Valve's last-edit order sometimes is.
            vec![review("r5", w + 5, w + 5), review("r6", 100, w - 2 * DAY)],
            vec![review("r7", 100, w - 3 * DAY)],
            vec![review("r8", w + 7, w + 7)],
        ];
        let server = stand_in::Server::new(edits(pages, 10));
        let stop = std::sync::atomic::AtomicBool::new(false);
        let before = now_unix();
        let report = sweep(&client_of(&server), APP, out.path(), &stop, |_| {})
            .await
            .unwrap();
        let after = now_unix();

        assert!((before..=after).contains(&report.started));
        assert_eq!(report.watermark, WATERMARK);
        assert_eq!(report.stop, StopReason::Exhausted);
        assert_eq!(
            report.pages, 5,
            "the walk stops once a page is a day behind"
        );
        assert_eq!((report.rows, report.new, report.edited), (5, 3, 2));
        assert_eq!((report.unique, report.valve_total), (5, 10));
        assert_eq!(report.coverage(), Some(0.5));

        let swept = dir.join(crate::capture::sweep_file(report.started));
        assert!(swept.exists());
        assert!(!dir.join("sweep-1.parquet.partial").exists());
        assert!(recorded.exists());
        assert!(
            crate::capture::Newest::load(&dir)
                .unwrap()
                .copies
                .contains_key("r3")
        );
        let facts = facts(&dir);
        assert_eq!(facts["swept_unix"], report.started);
        assert_eq!(facts["sweeps"], 1);
        assert_eq!(facts["rows_swept"], 5);
        assert_eq!(facts["rows_unique"], 5);
        assert_eq!(facts["valve_total_reviews"], 10);
        assert_eq!(facts["coverage"], 0.5);
    }

    #[tokio::test]
    async fn a_retry_after_a_failed_record_write_counts_what_the_first_try_fetched() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let w = WATERMARK;
        let page = vec![vec![
            review("r3", w + 50, w + 50),
            review("r1", 500, w + 40),
        ]];
        let blocker = dir.join("crawl.json.partial");
        std::fs::create_dir(&blocker).unwrap();
        let server = stand_in::Server::new(edits(page.clone(), 3));
        sweeping(&server, APP, out.path()).await.unwrap_err();
        std::fs::remove_dir(&blocker).unwrap();

        let retry = swept_once(out.path(), page, 3).await;
        assert_eq!((retry.rows, retry.new, retry.edited), (2, 1, 1));
        assert_eq!(retry.unique, 3);
        let facts = facts(&dir);
        assert_eq!(facts["rows_unique"], 3);
        assert_eq!(facts["rows_swept"], 2);
        assert_eq!(
            ids_in(&dir),
            ["r1", "r2", "r3"].map(str::to_owned).into_iter().collect()
        );
    }

    #[tokio::test]
    async fn a_retry_counts_an_edit_of_a_review_a_recorded_sweep_holds_as_an_edit() {
        let out = crate::tempdir::Dir::new();
        let swept = WATERMARK + 10;
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "swept_unix": swept, "rows_unique": 3}),
        );
        written_to(
            &dir.join(crate::capture::sweep_file(swept)),
            &[review("r3", swept, swept)],
        );
        let mut newest = crate::capture::Newest::default();
        newest.record("r3", swept, swept);
        newest.save(&dir).unwrap();
        let w = WATERMARK;
        let page = vec![vec![
            review("r4", w + 60, w + 60),
            review("r3", swept, w + 60),
        ]];
        let blocker = dir.join("crawl.json.partial");
        std::fs::create_dir(&blocker).unwrap();
        let server = stand_in::Server::new(edits(page.clone(), 4));
        sweeping(&server, APP, out.path()).await.unwrap_err();
        std::fs::remove_dir(&blocker).unwrap();

        let retry = swept_once(out.path(), page, 4).await;
        assert_eq!((retry.rows, retry.new, retry.edited), (2, 1, 1));
        assert_eq!(facts(&dir)["rows_unique"], 4);
    }

    #[tokio::test]
    async fn a_sweep_that_finds_nothing_new_leaves_no_file_but_counts_as_a_sweep() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": 1, "swept_unix": WATERMARK, "sweeps": 2, "name": "A Game"}),
        );
        let pages = vec![vec![review("r1", 500, WATERMARK - 2 * DAY)]];
        let server = stand_in::Server::new(edits(pages, 2));
        let stop = std::sync::atomic::AtomicBool::new(false);
        let report = sweep(&client_of(&server), APP, out.path(), &stop, |_| {})
            .await
            .unwrap();
        assert_eq!(
            report.watermark, WATERMARK,
            "the last sweep is the watermark"
        );
        assert_eq!(report.rows, 0);
        assert!(
            !dir.join(crate::capture::sweep_file(report.started))
                .exists()
        );
        assert!(!dir.join(crate::capture::Newest::FILE).exists());
        assert_eq!(facts(&dir)["sweeps"], 3);
        assert_eq!(facts(&dir)["walk_from_unix"], WATERMARK);
    }

    #[tokio::test]
    async fn a_sweep_walks_from_the_recorded_walk_start_where_the_last_sweep_started_later() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({
                "snapshot_unix": WATERMARK - DAY,
                "swept_unix": WATERMARK + 3600,
                "walk_from_unix": WATERMARK,
                "rows_unique": 2,
            }),
        );
        let page = vec![vec![review("r1", 500, WATERMARK + 60)]];
        let report = swept_once(out.path(), page, 2).await;
        assert_eq!(report.watermark, WATERMARK);
        assert_eq!((report.rows, report.edited), (1, 1));
        assert_eq!(facts(&dir)["walk_from_unix"], WATERMARK + 60);
    }

    #[tokio::test]
    async fn the_review_at_the_watermark_is_not_fetched_twice_and_one_stamped_beside_it_is() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let w = WATERMARK;
        let first = vec![vec![review("r3", w + 50, w + 50)]];
        swept_once(out.path(), first, 3).await;
        assert_eq!(facts(&dir)["walk_from_unix"], w + 50);

        let second = vec![vec![
            review("r4", w + 50, w + 50),
            review("r3", w + 50, w + 50),
        ]];
        let report = swept_once(out.path(), second, 4).await;
        assert_eq!((report.rows, report.new, report.edited), (1, 1, 0));
        assert_eq!(facts(&dir)["rows_unique"], 4);
        assert_eq!(
            ids_in(&dir),
            ["r1", "r2", "r3", "r4"]
                .map(str::to_owned)
                .into_iter()
                .collect()
        );
    }

    #[tokio::test]
    async fn a_later_page_does_not_move_the_walk_start_past_the_first() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let w = WATERMARK;
        let pages = vec![
            vec![review("r3", w + 50, w + 50)],
            vec![review("r4", w + 70, w + 70)],
        ];
        swept_once(out.path(), pages, 4).await;
        assert_eq!(facts(&dir)["walk_from_unix"], w + 50);

        let page = vec![vec![
            review("r5", w + 60, w + 60),
            review("r4", w + 70, w + 70),
            review("r3", w + 50, w + 50),
        ]];
        let report = swept_once(out.path(), page, 5).await;
        assert_eq!((report.rows, report.new), (1, 1));
    }

    #[tokio::test]
    async fn an_edit_stamped_after_the_sweeps_start_leaves_the_walk_start_at_that_start() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let far = now_unix() + 3600;
        let first = swept_once(out.path(), vec![vec![review("r1", 500, far)]], 2).await;
        assert_eq!(facts(&dir)["walk_from_unix"], first.started);

        let page = vec![vec![
            review("r1", 500, far),
            review("r3", first.started + 1, first.started + 1),
        ]];
        let second = swept_once(out.path(), page, 3).await;
        assert_eq!((second.rows, second.new), (1, 1));
    }

    #[tokio::test]
    async fn a_walk_start_that_is_no_whole_number_falls_back_to_the_last_sweep() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": 1, "swept_unix": WATERMARK, "walk_from_unix": "soon"}),
        );
        let page = vec![vec![review("r3", WATERMARK + 5, WATERMARK + 5)]];
        let report = swept_once(out.path(), page, 3).await;
        assert_eq!(report.watermark, WATERMARK);
        assert_eq!(report.rows, 1);
        assert_eq!(facts(&dir)["walk_from_unix"], WATERMARK + 5);
    }

    #[tokio::test]
    async fn a_page_of_copies_the_capture_holds_does_not_end_the_walk() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let w = WATERMARK;
        let held = vec![review("r3", w + 50, w + 50), review("r4", w + 50, w + 50)];
        swept_once(out.path(), vec![held.clone()], 4).await;

        let pages = vec![held, vec![review("r5", w + 52, w + 52)]];
        let report = swept_once(out.path(), pages, 5).await;
        assert_eq!(report.pages, 3);
        assert_eq!((report.rows, report.new), (1, 1));
        assert_eq!(
            ids_in(&dir),
            ["r1", "r2", "r3", "r4", "r5"]
                .map(str::to_owned)
                .into_iter()
                .collect()
        );
    }

    #[tokio::test]
    async fn a_sweep_refused_its_totals_leaves_the_walk_start_as_it_was() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let w = WATERMARK;
        let pages = vec![vec![
            review("r5", w + 60, w + 60),
            review("r3", w + 50, w + 50),
        ]];
        let inner = edits(pages.clone(), 4);
        let server = stand_in::Server::new(move |asked| {
            if counting(asked) {
                no_summary()
            } else {
                inner(asked)
            }
        });
        let refused = sweeping(&server, APP, out.path()).await;
        assert!(matches!(refused, Err(crate::Error::NoSuchCorpus { .. })));
        assert!(facts(&dir).get("walk_from_unix").is_none());

        let report = swept_once(out.path(), pages, 4).await;
        assert_eq!((report.rows, report.new), (2, 2));
        assert_eq!(facts(&dir)["rows_unique"], 4);
    }

    #[tokio::test]
    async fn a_retry_writes_again_the_copy_only_a_failed_sweep_holds_and_the_next_sweep_does_not() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let w = WATERMARK;
        let page = vec![vec![
            review("r3", w + 50, w + 50),
            review("r1", 500, w + 40),
        ]];
        let blocker = dir.join("crawl.json.partial");
        std::fs::create_dir(&blocker).unwrap();
        let server = stand_in::Server::new(edits(page.clone(), 3));
        sweeping(&server, APP, out.path()).await.unwrap_err();
        std::fs::remove_dir(&blocker).unwrap();

        let first = swept_once(out.path(), page.clone(), 3).await;
        assert_eq!((first.rows, first.new, first.edited), (2, 1, 1));
        assert_eq!(facts(&dir)["rows_unique"], 3);
        assert_eq!(facts(&dir)["rows_swept"], 2);

        let second = swept_once(out.path(), page, 3).await;
        assert_eq!((second.rows, second.new, second.edited), (0, 0, 0));
        assert_eq!(facts(&dir)["rows_swept"], 2);
    }

    async fn swept_once(out: &std::path::Path, page: Vec<Vec<Value>>, total: usize) -> SweepReport {
        let server = stand_in::Server::new(edits(page, total));
        sweeping(&server, APP, out).await.unwrap()
    }

    #[tokio::test]
    async fn a_sweep_starts_one_second_past_a_recorded_sweep_the_clock_has_not_passed() {
        let out = crate::tempdir::Dir::new();
        let ahead = now_unix() + 3600;
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "swept_unix": ahead, "rows_unique": 2, "name": "A Game"}),
        );
        let page = vec![vec![review("r3", ahead, ahead)]];

        let first = swept_once(out.path(), page.clone(), 3).await;
        assert_eq!(first.started, ahead + 1);
        assert_eq!((first.rows, first.new), (1, 1));
        assert!(dir.join(crate::capture::sweep_file(ahead + 1)).exists());

        let second = swept_once(out.path(), page, 3).await;
        assert_eq!(second.started, ahead + 2);
        assert!(dir.join(crate::capture::sweep_file(ahead + 1)).exists());
        assert_eq!(facts(&dir)["swept_unix"], ahead + 2);
        let ids = ids_in(&dir);
        assert!(["r1", "r2", "r3"].iter().all(|id| ids.contains(*id)));
    }

    #[tokio::test]
    async fn a_sweep_in_the_second_the_last_one_recorded_keeps_that_sweeps_file() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2, "name": "A Game"}),
        );
        // Both sweeps then start inside the next second.
        let turned = now_unix();
        while now_unix() == turned {
            std::thread::sleep(Duration::from_millis(5));
        }
        let page = vec![vec![review("r3", WATERMARK + 50, WATERMARK + 50)]];

        let first = swept_once(out.path(), page.clone(), 3).await;
        let second = swept_once(out.path(), page, 3).await;
        assert!(second.started > first.started);
        assert!(dir.join(crate::capture::sweep_file(first.started)).exists());
        let ids = ids_in(&dir);
        assert!(["r1", "r2", "r3"].iter().all(|id| ids.contains(*id)));
    }

    #[tokio::test]
    async fn a_sweep_starts_at_the_clock_once_it_has_passed_the_recorded_sweep() {
        let out = crate::tempdir::Dir::new();
        captured(
            out.path(),
            &json!({"snapshot_unix": 1, "swept_unix": WATERMARK}),
        );
        let page = vec![vec![review("r1", 500, WATERMARK - 2 * DAY)]];
        let before = now_unix();
        let report = swept_once(out.path(), page, 2).await;
        let after = now_unix();
        assert!((before..=after).contains(&report.started));

        let other = crate::tempdir::Dir::new();
        captured(
            other.path(),
            &json!({"snapshot_unix": WATERMARK, "swept_unix": "soon"}),
        );
        let page = vec![vec![review("r3", WATERMARK + 5, WATERMARK + 5)]];
        let before = now_unix();
        let report = swept_once(other.path(), page, 3).await;
        let after = now_unix();
        assert_eq!(report.rows, 1);
        assert!((before..=after).contains(&report.started));
    }

    #[tokio::test]
    async fn a_retry_after_a_failed_record_write_starts_past_the_file_the_index_counts() {
        let out = crate::tempdir::Dir::new();
        let ahead = now_unix() + 3600;
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "swept_unix": ahead, "rows_unique": 2, "name": "A Game"}),
        );
        let page = vec![vec![review("r3", ahead, ahead)]];
        let blocker = dir.join("crawl.json.partial");
        std::fs::create_dir(&blocker).unwrap();
        let server = stand_in::Server::new(edits(page.clone(), 3));
        sweeping(&server, APP, out.path()).await.unwrap_err();
        std::fs::remove_dir(&blocker).unwrap();
        let first = dir.join(crate::capture::sweep_file(ahead + 1));
        assert!(first.exists());

        let retry = swept_once(out.path(), page, 3).await;
        assert_eq!(retry.started, ahead + 2);
        assert!(first.exists());
        let ids = ids_in(&dir);
        assert!(["r1", "r2", "r3"].iter().all(|id| ids.contains(*id)));
    }

    fn sweeping(
        server: &stand_in::Server,
        app_id: u32,
        out: &std::path::Path,
    ) -> impl Future<Output = Result<SweepReport>> {
        let client = client_of(server);
        let out = out.to_path_buf();
        async move {
            let stop = std::sync::atomic::AtomicBool::new(false);
            sweep(&client, app_id, &out, &stop, |_| {}).await
        }
    }

    fn one_new_review() -> Vec<Vec<Value>> {
        vec![vec![review("r3", WATERMARK + 50, WATERMARK + 50)]]
    }

    #[tokio::test]
    async fn a_second_sweep_of_a_game_is_refused_while_the_first_runs() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let server = stand_in::Server::new(edits(one_new_review(), 3));
        let (first, second) = tokio::join!(
            sweeping(&server, APP, out.path()),
            sweeping(&server, APP, out.path())
        );
        let first = first.unwrap();
        assert_eq!((first.rows, first.new), (1, 1));
        let Err(crate::Error::Refused(said)) = second else {
            panic!("the second sweep ran: {second:?}");
        };
        assert!(said.contains("app 7"), "{said}");
        let facts = facts(&dir);
        assert_eq!(facts["sweeps"], 1);
        assert_eq!(facts["rows_unique"], 3);
        assert_eq!(
            ids_in(&dir),
            ["r1", "r2", "r3"].map(str::to_owned).into_iter().collect()
        );
    }

    #[tokio::test]
    async fn a_sweep_is_refused_while_another_program_holds_the_lock() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let server = stand_in::Server::new(edits(one_new_review(), 3));
        let before = std::fs::read(dir.join("crawl.json")).unwrap();
        let other = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(SWEEP_LOCK))
            .unwrap();
        other.try_lock().unwrap();
        for _ in 0..2 {
            let refused = sweeping(&server, APP, out.path()).await;
            assert!(
                matches!(refused, Err(crate::Error::Refused(_))),
                "{refused:?}"
            );
        }
        assert!(server.asked().is_empty());
        assert_eq!(std::fs::read(dir.join("crawl.json")).unwrap(), before);

        drop(other);
        let landed = sweeping(&server, APP, out.path()).await.unwrap();
        assert_eq!((landed.rows, landed.new), (1, 1));
        assert!(dir.join(SWEEP_LOCK).exists());
    }

    #[tokio::test]
    async fn a_lock_file_left_on_disk_by_a_sweep_that_ended_is_taken_freely() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        std::fs::write(dir.join(SWEEP_LOCK), b"").unwrap();
        let server = stand_in::Server::new(edits(one_new_review(), 3));
        let landed = sweeping(&server, APP, out.path()).await.unwrap();
        assert_eq!((landed.rows, landed.new), (1, 1));
        let again = sweeping(&server, APP, out.path()).await.unwrap();
        assert_eq!(again.rows, 0, "{again:?}");
    }

    #[tokio::test]
    async fn two_games_are_swept_at_once() {
        let out = crate::tempdir::Dir::new();
        let facts_of = json!({"snapshot_unix": WATERMARK, "rows_unique": 2});
        let seven = captured_of(out.path(), 7, &facts_of);
        let eight = captured_of(out.path(), 8, &facts_of);
        let server = stand_in::Server::new(edits(one_new_review(), 3));
        let (first, second) = tokio::join!(
            sweeping(&server, 7, out.path()),
            sweeping(&server, 8, out.path())
        );
        assert_eq!(first.unwrap().new, 1);
        assert_eq!(second.unwrap().new, 1);
        assert_eq!(facts(&seven)["sweeps"], 1);
        assert_eq!(facts(&eight)["sweeps"], 1);
    }

    /// Runs only as the child of the next test, which names the capture in `HOLD_SWEEP_LOCK`.
    #[test]
    fn holds_a_sweep_lock_for_the_test_that_starts_it() {
        let Some(dir) = std::env::var_os("HOLD_SWEEP_LOCK") else {
            return;
        };
        let _lock = sweep_lock(std::path::Path::new(&dir), APP).unwrap();
        println!("held");
        std::thread::sleep(Duration::from_secs(120));
    }

    #[tokio::test]
    async fn a_program_killed_midway_lets_go_of_the_lock_at_once() {
        use std::io::BufRead as _;
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let server = stand_in::Server::new(edits(one_new_review(), 3));
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crawl::tests::holds_a_sweep_lock_for_the_test_that_starts_it",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("HOLD_SWEEP_LOCK", &dir)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = std::io::BufReader::new(child.stdout.take().unwrap()).lines();
        let held = lines.any(|line| line.is_ok_and(|line| line.ends_with("... held")));
        assert!(held, "the other program never took the lock");

        let refused = sweeping(&server, APP, out.path()).await;
        assert!(
            matches!(refused, Err(crate::Error::Refused(_))),
            "{refused:?}"
        );
        child.kill().unwrap();
        child.wait().unwrap();
        let landed = sweeping(&server, APP, out.path()).await.unwrap();
        assert_eq!((landed.rows, landed.new), (1, 1));
    }

    #[tokio::test]
    async fn a_sweep_started_while_the_first_waits_for_its_totals_is_refused() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2, "name": "A Game"}),
        );
        let w = WATERMARK;
        let totals_asked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let walks = std::sync::atomic::AtomicUsize::new(0);
        let server = {
            let (totals_asked, released) = (Arc::clone(&totals_asked), Arc::clone(&released));
            stand_in::Server::new(move |asked| {
                use std::sync::atomic::Ordering::SeqCst;
                if asked.path() == "/api/appdetails" {
                    return name_of_the_game("A Game");
                }
                if counting(asked) {
                    if !totals_asked.swap(true, SeqCst) {
                        let began = Instant::now();
                        while !released.load(SeqCst) && began.elapsed() < Duration::from_secs(10) {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                    return summary_of(4);
                }
                let at = offset(asked);
                let walk = if at == 0 {
                    walks.fetch_add(1, SeqCst)
                } else {
                    walks.load(SeqCst) - 1
                };
                let pages = if walk == 0 {
                    vec![vec![review("r3", w + 50, w + 50)]]
                } else {
                    vec![vec![
                        review("r4", w + 60, w + 60),
                        review("r3", w + 50, w + 50),
                    ]]
                };
                let next = if at < pages.len() { at + 1 } else { at };
                let reviews = pages.get(at).cloned().unwrap_or_default();
                stand_in::Answer::json(&json!({"response": {
                    "query_summary": {"num_reviews": reviews.len()},
                    "reviews": reviews,
                    "cursor": format!("at{next}"),
                }}))
            })
        };
        let second = async {
            let began = Instant::now();
            while !totals_asked.load(std::sync::atomic::Ordering::SeqCst)
                && began.elapsed() < Duration::from_secs(10)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let second = sweeping(&server, APP, out.path()).await;
            released.store(true, std::sync::atomic::Ordering::SeqCst);
            second
        };
        let (first, second) = tokio::join!(sweeping(&server, APP, out.path()), second);
        assert!(
            matches!(second, Err(crate::Error::Refused(_))),
            "{second:?}"
        );
        let first = first.unwrap();
        assert_eq!((first.rows, first.new), (1, 1));
        let facts = facts(&dir);
        assert_eq!(facts["sweeps"], 1);
        assert_eq!(facts["rows_swept"], 1);
        assert_eq!(facts["rows_unique"], 3);
    }

    #[tokio::test]
    async fn a_sweep_takes_the_lock_before_it_reads_the_record() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let server = stand_in::Server::new(edits(one_new_review(), 3));
        let other = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(SWEEP_LOCK))
            .unwrap();
        other.try_lock().unwrap();
        std::fs::remove_file(dir.join("crawl.json")).unwrap();
        std::fs::create_dir(dir.join("crawl.json")).unwrap();
        let refused = sweeping(&server, APP, out.path()).await;
        assert!(
            matches!(refused, Err(crate::Error::Refused(_))),
            "{refused:?}"
        );
    }

    #[tokio::test]
    async fn a_name_is_asked_for_only_where_the_capture_has_none_and_kept() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(out.path(), &json!({"snapshot_unix": WATERMARK}));
        let store = stand_in::Server::new(|_| name_of_the_game("A Game"));
        let named = name_where_missing(&client_of(&store), out.path(), APP)
            .await
            .unwrap();
        assert_eq!(named.as_deref(), Some("A Game"));
        assert_eq!(facts(&dir)["name"], "A Game");

        let asked = store.asked().len();
        let again = name_where_missing(&client_of(&store), out.path(), APP)
            .await
            .unwrap();
        assert_eq!(again, None);
        assert_eq!(
            store.asked().len(),
            asked,
            "a capture with a name asks nobody"
        );
    }

    #[tokio::test]
    async fn a_blank_name_from_the_store_is_not_kept() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(out.path(), &json!({"snapshot_unix": WATERMARK}));
        let store = stand_in::Server::new(|_| name_of_the_game(" "));
        let named = name_where_missing(&client_of(&store), out.path(), APP)
            .await
            .unwrap();
        assert_eq!(named, None);
        assert!(facts(&dir).get("name").is_none());
    }

    /// The record a whole crawl of the two reviews [`captured`] writes leaves beside them.
    fn whole_record() -> Value {
        json!({
            "app_id": APP, "name": "A Game", "review_score_desc": "Very Positive",
            "rows_unique": 2, "valve_total_reviews": 2, "valve_total_positive": 2,
            "valve_total_negative": 0, "coverage": 1.0, "snapshot_unix": WATERMARK, "shards": 1,
        })
    }

    /// Steam's answer to a request for its totals that carries no totals at all.
    fn no_summary() -> stand_in::Answer {
        stand_in::Answer::json(&json!({"response": {"cursor": "*"}}))
    }

    /// Whether Valve's totals over the whole corpus are being asked for, rather than a window's.
    fn whole_corpus(asked: &stand_in::Asked) -> bool {
        counting(asked) && asked.input().get("date_range_start").is_none()
    }

    /// The error `report::crawl_facts` gives for the capture under `out`, where it gives one.
    fn unreadable(out: &std::path::Path) -> Option<String> {
        crate::report::crawl_facts(out, APP)
            .err()
            .map(|error| error.to_string())
    }

    #[tokio::test]
    async fn every_crawl_record_a_sweep_or_a_crawl_leaves_reads_back() {
        let pages = || {
            vec![
                vec![review("r3", WATERMARK + 50, WATERMARK + 50)],
                vec![review("r1", 500, WATERMARK - 2 * DAY)],
            ]
        };
        let stop = std::sync::atomic::AtomicBool::new(false);
        // A test thread has one scratch directory, so each library is a folder in it.
        let scratch = crate::tempdir::Dir::new();
        let library = |name: &str| {
            let out = scratch.path().join(name);
            std::fs::create_dir_all(&out).unwrap();
            out
        };

        let swept_without = library("swept-without-summary");
        captured(&swept_without, &whole_record());
        assert_eq!(unreadable(&swept_without), None);
        let walk = edits(pages(), 0);
        let server = stand_in::Server::new(move |asked| {
            if counting(asked) {
                no_summary()
            } else {
                walk(asked)
            }
        });
        let swept = sweep(&client_of(&server), APP, &swept_without, &stop, |_| {}).await;
        assert!(
            matches!(swept, Err(crate::Error::NoSuchCorpus { app_id: APP })),
            "{swept:?}"
        );

        let swept_zero = library("swept-total-zero");
        captured(&swept_zero, &whole_record());
        let server = stand_in::Server::new(edits(pages(), 0));
        sweep(&client_of(&server), APP, &swept_zero, &stop, |_| {})
            .await
            .unwrap();

        // Valve gives no total for the whole corpus while each window still counts its reviews.
        let crawled_zero = library("crawled-total-zero");
        let windows = valve(corpus(), None, 0);
        let server = stand_in::Server::new(move |asked| {
            if whole_corpus(asked) {
                summary_of(0)
            } else {
                windows(asked)
            }
        });
        crawl(&client_of(&server), APP, &options(&crawled_zero), |_| {})
            .await
            .unwrap();

        let crawled_without = library("crawled-without-summary");
        let pages_of_windows = valve(corpus(), None, 0);
        let server = stand_in::Server::new(move |asked| {
            if whole_corpus(asked) {
                no_summary()
            } else {
                pages_of_windows(asked)
            }
        });
        let crawled = crawl(&client_of(&server), APP, &options(&crawled_without), |_| {}).await;
        assert!(
            matches!(crawled, Err(crate::Error::NoSuchCorpus { app_id: APP })),
            "{crawled:?}"
        );
        crate::embed::latest_snapshot(&crawled_without, APP).unwrap_err();

        assert_eq!(
            [
                ("sweep, no summary", unreadable(&swept_without)),
                ("sweep, a total of 0", unreadable(&swept_zero)),
                ("crawl, a total of 0", unreadable(&crawled_zero)),
            ],
            [
                ("sweep, no summary", None),
                ("sweep, a total of 0", None),
                ("crawl, a total of 0", None),
            ]
        );
    }

    /// A crawl whose first window Steam keeps cutting short, so that one window is left.
    async fn short_crawl(out: &std::path::Path) -> CrawlReport {
        let server = stand_in::Server::new(valve(corpus(), Some(CORPUS_EPOCH), 3));
        let report = crawl(&client_of(&server), APP, &options(out), |_| {})
            .await
            .unwrap();
        assert!(!report.complete);
        report
    }

    async fn resumed_crawl(out: &std::path::Path) -> Result<CrawlReport> {
        let server = stand_in::Server::new(valve(corpus(), None, 0));
        crawl(&client_of(&server), APP, &options(out), |_| {}).await
    }

    /// `inner`, with a store that has no name for the game.
    fn nameless(
        inner: impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static,
    ) -> impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static {
        move |asked| {
            if asked.path() == "/api/appdetails" {
                stand_in::Answer::json(&json!({}))
            } else {
                inner(asked)
            }
        }
    }

    fn snapshot_of(dir: &std::path::Path) -> i64 {
        dir.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("snapshot="))
            .and_then(|stamp| stamp.parse().ok())
            .unwrap()
    }

    /// A sweep after the crawl that brings in one new review and one edit of a crawled one.
    async fn sweep_after(out: &std::path::Path, snapshot: i64) -> SweepReport {
        let pages = vec![vec![
            review("late-0", 1_710_000_000, snapshot + 1),
            review("arrived", snapshot + 1, snapshot + 1),
        ]];
        let server = stand_in::Server::new(edits(pages, 71));
        let stop = std::sync::atomic::AtomicBool::new(false);
        sweep(&client_of(&server), APP, out, &stop, |_| {})
            .await
            .unwrap()
    }

    fn amend(dir: &std::path::Path, change: impl FnOnce(&mut serde_json::Map<String, Value>)) {
        let mut record = facts(dir);
        change(record.as_object_mut().unwrap());
        write_facts(dir, &record).unwrap();
    }

    fn library_in(scratch: &crate::tempdir::Dir, name: &str) -> PathBuf {
        let out = scratch.path().join(name);
        std::fs::create_dir_all(&out).unwrap();
        out
    }

    #[tokio::test]
    async fn a_crawl_resumed_after_a_sweep_keeps_what_the_sweep_recorded() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let swept = sweep_after(out.path(), snapshot_of(&short.dir)).await;
        amend(&short.dir, |record| {
            record.insert("kept".to_owned(), json!(1));
        });

        let resumed = resumed_crawl(out.path()).await.unwrap();
        assert!(resumed.resumed && resumed.complete);
        assert_eq!(resumed.dir, short.dir);
        let record = facts(&resumed.dir);
        assert_eq!(record["swept_unix"], swept.started);
        assert_eq!(record["sweeps"], 1);
        assert_eq!(record["rows_swept"], 2);
        assert_eq!(record["kept"], 1);
        assert_eq!(record["rows_unique"], 71);
        assert_eq!(resumed.unique, 71);
        assert_eq!(ids_in(&resumed.dir).len(), 71);
        let share = 71.0 / 70.0;
        assert!((record["coverage"].as_f64().unwrap() - share).abs() < 1e-9);
        assert!((resumed.coverage().unwrap() - share).abs() < 1e-9);
    }

    #[tokio::test]
    async fn a_sweep_file_the_record_has_not_reached_adds_nothing_to_a_crawl() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let snapshot = snapshot_of(&short.dir);
        let swept = sweep_after(out.path(), snapshot).await;
        let unrecorded = review("unrecorded", snapshot + 2, snapshot + 2);
        let mut writer = CaptureWriter::create(
            &short
                .dir
                .join(crate::capture::sweep_file(swept.started + 10)),
            APP,
        )
        .unwrap();
        writer.write(&[&unrecorded]).unwrap();
        writer.close().unwrap();

        let resumed = resumed_crawl(out.path()).await.unwrap();
        assert_eq!(facts(&resumed.dir)["rows_unique"], 71);
        assert_eq!(resumed.unique, 71);
    }

    #[tokio::test]
    async fn a_crawl_the_store_gives_no_name_keeps_the_name_recorded() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        assert_eq!(facts(&short.dir)["name"], "A Game");

        let windows = valve(corpus(), None, 0);
        let server = stand_in::Server::new(nameless(move |asked| {
            if whole_corpus(asked) {
                summary_of(0)
            } else {
                windows(asked)
            }
        }));
        let resumed = crawl(&client_of(&server), APP, &options(out.path()), |_| {})
            .await
            .unwrap();
        assert_eq!(resumed.name, "A Game");
        let record = facts(&resumed.dir);
        assert_eq!(record["name"], "A Game");
        assert!(record["coverage"].is_null(), "{record}");
        assert_eq!(resumed.coverage(), None);
    }

    #[tokio::test]
    async fn a_crawl_with_no_name_from_the_store_or_the_record_writes_an_empty_one() {
        let scratch = crate::tempdir::Dir::new();
        let first = library_in(&scratch, "never-named");
        let server = stand_in::Server::new(nameless(valve(corpus(), None, 0)));
        let crawled = crawl(&client_of(&server), APP, &options(&first), |_| {})
            .await
            .unwrap();
        assert_eq!(crawled.name, "");
        assert_eq!(facts(&crawled.dir)["name"], "");
        assert_eq!(unreadable(&first), None);

        let second = library_in(&scratch, "named-with-a-number");
        let short = short_crawl(&second).await;
        amend(&short.dir, |record| {
            record.insert("name".to_owned(), json!(7));
        });
        let server = stand_in::Server::new(nameless(valve(corpus(), None, 0)));
        let resumed = crawl(&client_of(&server), APP, &options(&second), |_| {})
            .await
            .unwrap();
        assert!(resumed.resumed);
        assert_eq!(facts(&resumed.dir)["name"], "");
        assert_eq!(unreadable(&second), None);
    }

    #[tokio::test]
    async fn a_crawl_resumed_over_a_record_it_cannot_take_fields_from_writes_every_field() {
        let scratch = crate::tempdir::Dir::new();
        for name in ["removed", "not-json", "an-array", "cut-down"] {
            let out = library_in(&scratch, name);
            let short = short_crawl(&out).await;
            let record = short.dir.join("crawl.json");
            match name {
                "removed" => std::fs::remove_file(&record).unwrap(),
                "not-json" => std::fs::write(&record, b"not json").unwrap(),
                "an-array" => std::fs::write(&record, b"[]").unwrap(),
                _ => write_facts(
                    &short.dir,
                    &json!({"app_id": APP, "snapshot_unix": snapshot_of(&short.dir)}),
                )
                .unwrap(),
            }

            let resumed = resumed_crawl(&out).await.unwrap();
            assert!(resumed.resumed && resumed.complete, "{name}");
            assert_eq!(resumed.dir, short.dir, "{name}");
            let record = facts(&resumed.dir);
            assert_eq!(record["rows_unique"], 70, "{name}");
            assert_eq!(record["name"], "A Game", "{name}");
            assert_eq!(record["shards_short"], 0, "{name}");
            assert_eq!(unreadable(&out), None, "{name}");
        }
    }

    #[tokio::test]
    async fn a_sweep_field_of_the_wrong_type_adds_nothing_to_a_crawl_and_goes() {
        let scratch = crate::tempdir::Dir::new();
        let first = library_in(&scratch, "swept-unix-a-string");
        let short = short_crawl(&first).await;
        sweep_after(&first, snapshot_of(&short.dir)).await;
        amend(&short.dir, |record| {
            record.insert("swept_unix".to_owned(), json!("soon"));
        });
        let resumed = resumed_crawl(&first).await.unwrap();
        let record = facts(&resumed.dir);
        assert_eq!(record["rows_unique"], 70);
        assert_eq!(resumed.unique, 70);
        assert!(record.get("swept_unix").is_none(), "{record}");
        assert_eq!(record["sweeps"], 1);
        assert_eq!(unreadable(&first), None);

        let second = library_in(&scratch, "sweeps-a-string");
        let short = short_crawl(&second).await;
        let swept = sweep_after(&second, snapshot_of(&short.dir)).await;
        amend(&short.dir, |record| {
            record.insert("sweeps".to_owned(), json!("x"));
            record.insert("rows_swept".to_owned(), json!(-1));
        });
        let resumed = resumed_crawl(&second).await.unwrap();
        let record = facts(&resumed.dir);
        assert_eq!(record["rows_unique"], 71);
        assert_eq!(record["swept_unix"], swept.started);
        assert!(record.get("sweeps").is_none() && record.get("rows_swept").is_none());
        assert_eq!(unreadable(&second), None);
    }

    #[tokio::test]
    async fn a_crawl_whose_record_cannot_be_written_leaves_the_earlier_record_whole() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let before = std::fs::read(short.dir.join("crawl.json")).unwrap();
        std::fs::create_dir(short.dir.join("crawl.json.partial")).unwrap();

        resumed_crawl(out.path()).await.unwrap_err();
        assert_eq!(std::fs::read(short.dir.join("crawl.json")).unwrap(), before);
    }

    #[tokio::test]
    async fn a_crawl_whose_record_was_not_written_is_resumed_and_writes_it() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let blocker = short.dir.join("crawl.json.partial");
        std::fs::create_dir(&blocker).unwrap();
        resumed_crawl(out.path()).await.unwrap_err();
        std::fs::remove_dir(&blocker).unwrap();

        let again = resumed_crawl(out.path()).await.unwrap();
        assert!(again.resumed && again.complete);
        assert_eq!(again.dir, short.dir);
        assert_eq!(again.shards, 0);
        let record = facts(&again.dir);
        assert_eq!(record["complete"], true);
        assert_eq!(record["rows_unique"], 70);

        let after = resumed_crawl(out.path()).await.unwrap();
        assert!(!after.resumed);
    }

    #[tokio::test]
    async fn a_resumed_crawl_takes_its_record_from_its_own_folder() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let snapshot = snapshot_of(&short.dir);
        let newer = out
            .path()
            .join(format!("appid={APP}"))
            .join(format!("snapshot={}", snapshot + 1000));
        std::fs::create_dir_all(&newer).unwrap();
        let kept = review("elsewhere", 1, 1);
        let mut writer = CaptureWriter::create(&newer.join("shard-0000.parquet"), APP).unwrap();
        writer.write(&[&kept]).unwrap();
        writer.close().unwrap();
        write_facts(
            &newer,
            &json!({
                "app_id": APP, "snapshot_unix": snapshot + 1000,
                "swept_unix": snapshot + 500, "sweeps": 2, "rows_swept": 9,
            }),
        )
        .unwrap();

        let resumed = resumed_crawl(out.path()).await.unwrap();
        assert_eq!(resumed.dir, short.dir);
        let record = facts(&resumed.dir);
        for key in ["swept_unix", "sweeps", "rows_swept"] {
            assert!(record.get(key).is_none(), "{key}");
        }
    }

    #[tokio::test]
    async fn a_sweep_keeps_every_field_it_does_not_set() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({
                "snapshot_unix": WATERMARK, "rows_unique": 2, "name": "A Game",
                "complete": false, "kept": 1,
            }),
        );
        let pages = vec![vec![review("r3", WATERMARK + 5, WATERMARK + 5)]];
        let server = stand_in::Server::new(edits(pages, 3));
        let stop = std::sync::atomic::AtomicBool::new(false);
        sweep(&client_of(&server), APP, out.path(), &stop, |_| {})
            .await
            .unwrap();
        let record = facts(&dir);
        assert_eq!(record["name"], "A Game");
        assert_eq!(record["complete"], false);
        assert_eq!(record["kept"], 1);
        assert_eq!(record["rows_unique"], 3);
    }

    #[tokio::test]
    async fn a_name_set_after_a_write_that_died_before_its_rename_keeps_the_rest() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "swept_unix": WATERMARK + 9, "kept": 1}),
        );
        std::fs::write(dir.join("crawl.json.partial"), b"{\"name\": \"Cut").unwrap();
        let store = stand_in::Server::new(|_| name_of_the_game("A Game"));
        name_where_missing(&client_of(&store), out.path(), APP)
            .await
            .unwrap();
        let record = facts(&dir);
        assert_eq!(record["name"], "A Game");
        assert_eq!(record["swept_unix"], WATERMARK + 9);
        assert_eq!(record["kept"], 1);
        assert!(!dir.join("crawl.json.partial").exists());
    }

    /// The lock a capture's other program holds, taken on a handle of the test's own.
    fn lock_held_by_another_program(dir: &std::path::Path) -> std::fs::File {
        let other = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(SWEEP_LOCK))
            .unwrap();
        other.try_lock().unwrap();
        other
    }

    /// `inner`, except that the store's answer about the game's name sets `asked` and then waits
    /// until `released` is set, or ten seconds.
    fn name_held_back(
        inner: impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static,
        (asked, released): (
            Arc<std::sync::atomic::AtomicBool>,
            Arc<std::sync::atomic::AtomicBool>,
        ),
    ) -> impl Fn(&stand_in::Asked) -> stand_in::Answer + Send + Sync + 'static {
        move |request| {
            use std::sync::atomic::Ordering::SeqCst;
            if request.path() == "/api/appdetails" {
                asked.store(true, SeqCst);
                let began = Instant::now();
                while !released.load(SeqCst) && began.elapsed() < Duration::from_secs(10) {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            inner(request)
        }
    }

    /// Waits until `flag` is set, or ten seconds.
    async fn until_set(flag: &std::sync::atomic::AtomicBool) {
        let began = Instant::now();
        while !flag.load(std::sync::atomic::Ordering::SeqCst)
            && began.elapsed() < Duration::from_secs(10)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn a_crawl_writes_no_record_while_another_program_holds_the_lock() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let before = std::fs::read(short.dir.join("crawl.json")).unwrap();
        let other = lock_held_by_another_program(&short.dir);

        let refused = resumed_crawl(out.path()).await;
        assert!(
            matches!(refused, Err(crate::Error::Refused(_))),
            "{refused:?}"
        );
        assert_eq!(std::fs::read(short.dir.join("crawl.json")).unwrap(), before);

        drop(other);
        let again = resumed_crawl(out.path()).await.unwrap();
        assert!(again.resumed && again.complete);
        assert_eq!(again.dir, short.dir);
        assert_eq!(facts(&again.dir)["rows_unique"], 70);
    }

    #[tokio::test]
    async fn a_crawl_that_reads_its_record_before_the_lock_is_refused_by_it() {
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let record = short.dir.join("crawl.json");
        let bytes = std::fs::read(&record).unwrap();
        let other = lock_held_by_another_program(&short.dir);
        std::fs::remove_file(&record).unwrap();
        std::fs::create_dir(&record).unwrap();

        let refused = resumed_crawl(out.path()).await;
        assert!(
            matches!(refused, Err(crate::Error::Refused(_))),
            "{refused:?}"
        );

        std::fs::remove_dir(&record).unwrap();
        std::fs::write(&record, bytes).unwrap();
        drop(other);
        let again = resumed_crawl(out.path()).await.unwrap();
        assert!(again.resumed && again.complete);
    }

    #[tokio::test]
    async fn a_sweep_that_lands_while_a_crawl_asks_the_store_keeps_its_record() {
        use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
        let out = crate::tempdir::Dir::new();
        let short = short_crawl(out.path()).await;
        let (asked, released) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let server = stand_in::Server::new(name_held_back(
            valve(corpus(), None, 0),
            (Arc::clone(&asked), Arc::clone(&released)),
        ));
        let sweeping_meanwhile = async {
            until_set(&asked).await;
            let swept = sweep_after(out.path(), snapshot_of(&short.dir)).await;
            released.store(true, SeqCst);
            swept
        };
        let (client, settings) = (client_of(&server), options(out.path()));
        let (resumed, swept) =
            tokio::join!(crawl(&client, APP, &settings, |_| {}), sweeping_meanwhile);
        let resumed = resumed.unwrap();
        assert!(resumed.resumed && resumed.complete);
        let record = facts(&resumed.dir);
        assert_eq!(record["swept_unix"], swept.started);
        assert_eq!(record["sweeps"], 1);
        assert_eq!(record["rows_swept"], 2);
        assert_eq!(record["rows_unique"], 71);
    }

    #[tokio::test]
    async fn a_name_lookup_writes_nothing_while_another_program_holds_the_lock() {
        let out = crate::tempdir::Dir::new();
        let dir = captured(out.path(), &json!({"snapshot_unix": WATERMARK}));
        let store = stand_in::Server::new(|_| name_of_the_game("A Game"));
        let before = std::fs::read(dir.join("crawl.json")).unwrap();
        let other = lock_held_by_another_program(&dir);

        let refused = name_where_missing(&client_of(&store), out.path(), APP).await;
        assert!(
            matches!(refused, Err(crate::Error::Refused(_))),
            "{refused:?}"
        );
        assert_eq!(std::fs::read(dir.join("crawl.json")).unwrap(), before);

        drop(other);
        let named = name_where_missing(&client_of(&store), out.path(), APP)
            .await
            .unwrap();
        assert_eq!(named.as_deref(), Some("A Game"));
    }

    #[tokio::test]
    async fn a_sweep_that_lands_while_a_name_lookup_asks_the_store_keeps_its_record() {
        use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
        let out = crate::tempdir::Dir::new();
        let dir = captured(
            out.path(),
            &json!({"snapshot_unix": WATERMARK, "rows_unique": 2}),
        );
        let (asked, released) = (
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        let store = stand_in::Server::new(name_held_back(
            |_| name_of_the_game("Late Name"),
            (Arc::clone(&asked), Arc::clone(&released)),
        ));
        let walk = stand_in::Server::new(edits(one_new_review(), 3));
        let sweeping_meanwhile = async {
            until_set(&asked).await;
            let swept = sweeping(&walk, APP, out.path()).await;
            released.store(true, SeqCst);
            swept
        };
        let client = client_of(&store);
        let (looked_up, swept) = tokio::join!(
            name_where_missing(&client, out.path(), APP),
            sweeping_meanwhile
        );
        swept.unwrap();
        assert_eq!(looked_up.unwrap(), None);
        let record = facts(&dir);
        assert_eq!(record["name"], "A Game");
        assert_eq!(record["sweeps"], 1);
        assert_eq!(record["rows_unique"], 3);
    }
}
