//! The window.
//!
//! Everything here is a thin front for the same core the pipeline runs. A command in this
//! module may read the library, start a stage, and report what happened; it may not decide
//! anything a number depends on. Where the window and the terminal disagree about a figure,
//! one of them is calling the wrong function.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::Serialize;
use steamgauge_core::{
    CrawlOptions, DEFAULT_PACE, DEFAULT_SHARD_TARGET, ReviewQuery, SteamClient, embed,
    reading_time::ReadingTimes, report,
};
use tauri::{AppHandle, Emitter, Manager};

/// Shards fetched at once. Pacing is global, so this reorders work rather than leaning
/// harder on Valve, and matches what the pipeline uses when nobody says otherwise.
const SHARDS_AT_ONCE: usize = 4;

/// Where corpora live when nobody has said.
///
/// The pipeline defaults to `data` beside the working directory, which is right for a
/// terminal and wrong for an icon: an application opened from a menu has no working
/// directory worth writing gigabytes into.
fn library_dir(app: &AppHandle) -> PathBuf {
    if let Some(chosen) = std::env::var_os("STEAMGAUGE_DATA") {
        return PathBuf::from(chosen);
    }
    app.path()
        .app_data_dir()
        .map_or_else(|_| PathBuf::from("data"), |dir| dir.join("data"))
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// One game as the library lists it.
#[derive(Debug, Clone, Serialize)]
struct Game {
    app_id: u32,
    name: String,
    reviews: u64,
    valve_total: u64,
    coverage: f64,
    verdict: String,
    snapshot: i64,
    stage: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct Shelf {
    path: String,
    games: Vec<Game>,
}

/// What Valve says about an app before a single review has been downloaded.
#[derive(Debug, Clone, Serialize)]
struct Found {
    app_id: u32,
    name: String,
    reviews: u64,
    positive: u64,
    negative: u64,
    verdict: String,
    /// Whether this game is already in the library, so the window can offer to continue a
    /// crawl rather than silently starting one that resumes.
    held: bool,
}

/// How far a game has been taken. Named after what exists on disk rather than what was
/// asked for, because an interrupted stage leaves the previous one intact.
fn stage_of(dir: &Path, app_id: u32) -> &'static str {
    let Ok(snapshot) = embed::latest_snapshot(dir, app_id) else {
        return "crawled";
    };
    if snapshot.join("reading.json").exists() {
        "read"
    } else if snapshot.join("embeddings.parquet").exists() {
        "embedded"
    } else {
        "crawled"
    }
}

fn shelf(dir: &Path) -> Shelf {
    let mut games = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let Some(app_id) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("appid="))
                .and_then(|digits| digits.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(facts) = report::crawl_facts(dir, app_id) else {
                continue;
            };
            games.push(Game {
                app_id,
                name: facts.title(),
                reviews: facts.rows_unique,
                valve_total: facts.valve_total_reviews,
                coverage: facts.coverage,
                verdict: facts.review_score_desc.clone(),
                snapshot: facts.snapshot_unix,
                stage: stage_of(dir, app_id),
            });
        }
    }
    games.sort_by_key(|game| game.name.to_lowercase());
    Shelf {
        path: dir.display().to_string(),
        games,
    }
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn library(app: AppHandle) -> Shelf {
    shelf(&library_dir(&app))
}

/// Steam refusing a download, and how long the client will wait before asking again.
#[derive(Debug, Clone, Serialize)]
struct SteamWait {
    app_id: u32,
    seconds: u64,
    status: u16,
}

/// A client for a download that tells the window whenever Steam refuses it. A crawl can wait
/// minutes for a refusal to lift, and a bar that stops for minutes with nothing said is a bar
/// that looks hung.
fn waiting_client(app: &AppHandle, app_id: u32) -> Result<SteamClient, String> {
    let window = app.clone();
    Ok(SteamClient::new(DEFAULT_PACE)
        .map_err(text)?
        .with_notice(move |wait, status| {
            let _ = window.emit(
                "steam-wait",
                SteamWait {
                    app_id,
                    seconds: wait.as_secs(),
                    status,
                },
            );
        }))
}

#[tauri::command]
async fn look_up(app: AppHandle, app_id: u32) -> Result<Found, String> {
    // A search waits seconds, not the half hour a crawl will: whoever typed the id would rather
    // hear that Steam is refusing than watch the box wait.
    let client = SteamClient::new(DEFAULT_PACE)
        .map_err(text)?
        .with_patience(std::time::Duration::from_secs(30));
    let page = client
        .fetch(&ReviewQuery::new(app_id).per_page(0), app_id)
        .await
        .map_err(text)?;
    let summary = page
        .query_summary
        .ok_or_else(|| format!("Steam serves no reviews for app {app_id}"))?;
    let held = embed::latest_snapshot(&library_dir(&app), app_id).is_ok();
    Ok(Found {
        app_id,
        name: client.name(app_id).await.unwrap_or_default(),
        reviews: summary.total_reviews,
        positive: summary.total_positive,
        negative: summary.total_negative,
        verdict: summary.review_score_desc,
        held,
    })
}

/// How far a crawl has got, as the window draws it.
#[derive(Debug, Clone, Serialize)]
struct Step {
    app_id: u32,
    shards_done: usize,
    shards_total: usize,
    unique: u64,
    valve_total: u64,
}

#[tauri::command]
async fn crawl(app: AppHandle, app_id: u32) -> Result<Shelf, String> {
    let out_dir = library_dir(&app);
    std::fs::create_dir_all(&out_dir).map_err(text)?;
    let options = CrawlOptions {
        out_dir: out_dir.clone(),
        concurrency: SHARDS_AT_ONCE,
        shard_target: DEFAULT_SHARD_TARGET,
        resume: true,
    };
    let client = waiting_client(&app, app_id)?;
    let window = app.clone();
    steamgauge_core::crawl(&client, app_id, &options, move |progress| {
        let _ = window.emit(
            "crawl",
            Step {
                app_id,
                shards_done: progress.shards_done,
                shards_total: progress.shards_total,
                unique: progress.unique,
                valve_total: progress.valve_total,
            },
        );
    })
    .await
    .map_err(text)?;
    Ok(shelf(&out_dir))
}

/// How far a sweep has got, as the window draws it.
#[derive(Debug, Clone, Serialize)]
struct SweepStep {
    app_id: u32,
    pages: u32,
    rows: u64,
}

/// What a sweep brought in, as the window reports it.
#[derive(Debug, Clone, Serialize)]
struct Swept {
    app_id: u32,
    rows: u64,
    new: u64,
    edited: u64,
    since: i64,
}

/// Brings a held capture up to date with what was written or edited since.
#[tauri::command]
async fn sweep(app: AppHandle, app_id: u32) -> Result<Swept, String> {
    let out_dir = library_dir(&app);
    let client = waiting_client(&app, app_id)?;
    let window = app.clone();
    let report = steamgauge_core::crawl::sweep(&client, app_id, &out_dir, move |progress| {
        let _ = window.emit(
            "sweep",
            SweepStep {
                app_id,
                pages: progress.pages,
                rows: progress.rows,
            },
        );
    })
    .await
    .map_err(text)?;
    Ok(Swept {
        app_id,
        rows: report.rows,
        new: report.new,
        edited: report.edited,
        since: report.watermark,
    })
}

/// How far a reading has got, as the window draws it.
#[derive(Debug, Clone, Serialize)]
struct ReadStep {
    app_id: u32,
    claims_read: u64,
    reviews_counted: u64,
}

/// How far a model download has got, as the window draws it.
#[derive(Debug, Clone, Serialize)]
struct Fetch {
    file: String,
    downloaded: u64,
    total: Option<u64>,
}

/// Reads a whole corpus with the trained model.
///
/// Runs off the window's thread: it is minutes of arithmetic on a million claims, and a
/// webview that stops answering is a webview a person force-quits.
///
/// `reader` names the size somebody chose, which counts only where the machine reads on its
/// processor: on a card the card decides, and a choice remembered from before the card was
/// fitted must not overrule it.
#[tauri::command]
async fn read_game(
    app: AppHandle,
    app_id: u32,
    language: Option<String>,
    reader: Option<String>,
) -> Result<(), String> {
    use steamgauge_core::reader::{Size, fits, on_the_processor};

    let out_dir = library_dir(&app);
    let options = steamgauge_core::read::ReadOptions {
        out_dir: out_dir.clone(),
        language,
        ..steamgauge_core::read::ReadOptions::default()
    };
    let window = app.clone();

    // A standard user has the binary and nothing else. The model is fetched by checksum the
    // first time it is needed, and the window is told how far the download has got, because
    // half a gigabyte with no progress shown is indistinguishable from a hang.
    let card = steamgauge_core::card::largest();
    let reaches = steamgauge_core::model::REACHES_A_CARD;
    // A name no size answers to is a choice remembered from a release that had it, and the
    // window offers only the sizes there are, so it falls back rather than refusing to read.
    let size = reader
        .as_deref()
        .filter(|_| on_the_processor(card, reaches))
        .and_then(Size::named)
        .unwrap_or_else(|| fits(card, reaches));
    let model_dir = size.home();
    if !model_dir.join("model.onnx").is_file() {
        if !size.published.is_pinned() {
            return Err(format!(
                "no {} claim reader is installed and none has been published yet",
                size.name
            ));
        }
        let fetching = app.clone();
        steamgauge_core::reader::ensure(size, &model_dir, |progress| {
            let _ = fetching.emit(
                "fetch",
                Fetch {
                    file: progress.file.to_owned(),
                    downloaded: progress.downloaded,
                    total: progress.total,
                },
            );
        })
        .await
        .map_err(text)?;
    }

    // The one fact about the game the reader is told, asked of the store once and kept.
    let game_dir = out_dir.join(format!("appid={app_id}"));
    if steamgauge_core::facts::Facts::load(&game_dir).is_none() {
        let headset_only = waiting_client(&app, app_id)?
            .headset_only(app_id)
            .await
            .ok_or_else(|| {
                "the store would not say whether this game is played in a VR headset".to_owned()
            })?;
        steamgauge_core::facts::Facts { headset_only }
            .save(&game_dir)
            .map_err(text)?;
    }

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // From before the model loads: the estimate is of the wait, and loading is part of it.
        let started = std::time::Instant::now();
        let mut model = steamgauge_core::reader::ClaimReader::load(&model_dir).map_err(text)?;
        let report = steamgauge_core::read::read_corpus(&mut model, app_id, &options, |progress| {
            let _ = window.emit(
                "read",
                ReadStep {
                    app_id,
                    claims_read: progress.claims_read,
                    reviews_counted: progress.reviews_counted,
                },
            );
        })
        .map_err(text)?;
        let snapshot = embed::latest_snapshot(&options.out_dir, app_id).map_err(text)?;
        report.save(&snapshot.join("reading.json")).map_err(text)?;
        if model.device() == "cpu" {
            let mut times = ReadingTimes::load(&options.out_dir);
            times.note(
                size,
                options.language.as_deref(),
                started.elapsed().as_secs_f64(),
                report.corpus_reviews,
            );
            times.save(&options.out_dir).map_err(text)?;
        }
        Ok(())
    })
    .await
    .map_err(text)?
}

/// One size somebody on a processor can choose, and about how long it would take them.
#[derive(Debug, Clone, Serialize)]
struct ReaderChoice {
    name: &'static str,
    /// About how long this game would take here, where anything has been read here in the
    /// language asked for.
    seconds: Option<f64>,
    /// How many times the fastest size's time this one takes, which is known before anything
    /// has been read here.
    times: f64,
}

/// The sizes to offer for reading a game, smallest first; none where a card is reached,
/// because there the card decides.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn reader_choices(
    app: AppHandle,
    app_id: u32,
    language: Option<String>,
) -> Result<Vec<ReaderChoice>, String> {
    use steamgauge_core::reader::{SIZES, on_the_processor};

    if !on_the_processor(
        steamgauge_core::card::largest(),
        steamgauge_core::model::REACHES_A_CARD,
    ) {
        return Ok(Vec::new());
    }
    let dir = library_dir(&app);
    let reviews = report::crawl_facts(&dir, app_id).map_err(text)?.rows_unique;
    let times = ReadingTimes::load(&dir);
    let fastest = SIZES[0].processor_seconds;
    Ok(SIZES
        .iter()
        .map(|size| ReaderChoice {
            name: size.name,
            seconds: times.seconds(size, language.as_deref(), reviews),
            times: size.processor_seconds / fastest,
        })
        .collect())
}

/// One subject, as the window draws it after a corpus has been read.
#[derive(Debug, Clone, Serialize)]
struct Subject {
    id: String,
    label: String,
    reviews: u64,
    rate: Option<f64>,
    praised: u64,
    criticised: u64,
    mixed: u64,
    claims: u64,
    top_rate: Option<f64>,
    bias: Option<f64>,
    positive: Option<f64>,
    /// What the share of claims would be with the model's measured errors taken out, where
    /// this game has labels and the model finds the subject better than chance.
    corrected: Option<f64>,
    /// Of the labelled claims about this subject, the share the model found. A subject it
    /// misses most of is a row whose rate is a floor, and the window marks it.
    found: Option<f64>,
    /// The words the praise uses and the complaints do not, and the reverse, each with the
    /// reviewers behind it. Empty where nothing stands out, which is what a thin side shows.
    praised_terms: Vec<steamgauge_core::said::Term>,
    criticised_terms: Vec<steamgauge_core::said::Term>,
}

/// How often the model is measured to be wrong on this game, where it has been measured.
#[derive(Debug, Clone, Serialize)]
struct Measured {
    answered: u64,
    declined: u64,
    agreement: Option<f64>,
    low: Option<f64>,
    high: Option<f64>,
    /// Labelled claims this build cuts differently from the build they were labelled under,
    /// so the figure beside them is over fewer claims than the set holds.
    unjoined: u64,
}

/// What reading a corpus found, ready for the window.
#[derive(Debug, Clone, Serialize)]
struct Reading {
    app_id: u32,
    name: String,
    reviews: u64,
    corpus_reviews: u64,
    language: Option<String>,
    claims: u64,
    unclassified_claims: u64,
    silent_reviews: u64,
    top_of_the_pile: u64,
    positive_baseline: Option<f64>,
    model: String,
    threshold: f32,
    /// The game in a paragraph, assembled from the same counts the table shows.
    in_short: String,
    /// When the capture was last brought up to date after these counts were made, so the
    /// window can say the counts describe the corpus as it was.
    swept_since: Option<i64>,
    /// Whether this build takes reviews apart differently from the pass that made these
    /// readings. The counts stand; a claim quoted by its index does not, until the game is
    /// read again.
    subjects: Vec<Subject>,
    measured: Option<Measured>,
    /// Whether this game's labels are in the model's weights, which is why it is not measured.
    learned: bool,
    /// What the reader scored on games it had never seen. The window says this where the game
    /// on screen has no labels of its own, which is most games anybody will ever open: telling
    /// them only that nothing is measured invites them to distrust everything or to trust
    /// everything, and the model does have a measurement.
    frozen: Option<steamgauge_core::reader::Frozen>,
    /// Oldest first. Fewer than two and there is no line to draw.
    months: Vec<MonthOut>,
    /// Commonest first, over the whole capture rather than the counted language.
    languages: Vec<Language>,
}

/// One month of the corpus, as the window draws it.
#[derive(Debug, Clone, Serialize)]
struct MonthOut {
    /// `2024-02`, which sorts.
    label: String,
    /// `February 2024`, which reads.
    name: String,
    reviews: u64,
    /// Share recommending the game, or none for a month too small to carry a rate.
    positive: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct Language {
    name: String,
    reviews: u64,
    share: Option<f64>,
}

#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn share_of(part: u64, whole: u64) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// One subject's row, with its measured error where the game has labels for it.
fn subject_row(
    found: &steamgauge_core::read::ReadReport,
    subject: &steamgauge_core::read::SubjectCount,
    agreement: Option<&steamgauge_core::ClaimAgreement>,
) -> Subject {
    let rate = share_of(subject.mention_reviews, found.reviews);
    let top_rate = share_of(subject.top_mention_reviews, found.top_helpful);
    let measured = agreement
        .and_then(|found| found.subjects.iter().find(|s| s.id == subject.id))
        .filter(|s| s.labelled >= steamgauge_core::measure::ENOUGH_TO_JUDGE_A_ROW);
    let said = found.said.iter().find(|said| said.subject == subject.id);
    Subject {
        id: subject.id.clone(),
        label: subject.label.clone(),
        reviews: subject.mention_reviews,
        rate,
        praised: subject.praised,
        criticised: subject.criticised,
        mixed: subject.mixed,
        claims: subject.claims,
        top_rate,
        bias: match (rate, top_rate) {
            (Some(overall), Some(top)) if overall > 0.0 => Some(top / overall),
            _ => None,
        },
        positive: share_of(subject.positive_mentions, subject.mention_reviews),
        corrected: measured
            .filter(|s| s.labelled >= steamgauge_core::measure::ENOUGH_TO_CORRECT_A_ROW)
            .and_then(|s| {
                share_of(subject.claims, found.claims).and_then(|observed| s.corrected(observed))
            }),
        found: measured.and_then(steamgauge_core::measure::SubjectAgreement::recall),
        praised_terms: said.map(|said| said.praised.clone()).unwrap_or_default(),
        criticised_terms: said.map(|said| said.criticised.clone()).unwrap_or_default(),
    }
}

/// What the reading pass wrote beside a snapshot's readings.
fn read_report(snapshot: &std::path::Path) -> Result<steamgauge_core::read::ReadReport, String> {
    serde_json::from_slice(
        &std::fs::read(snapshot.join("reading.json")).map_err(|_| {
            "this game has not been read yet; run the reading pass first".to_owned()
        })?,
    )
    .map_err(text)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn reading(app: AppHandle, app_id: u32) -> Result<Reading, String> {
    let dir = library_dir(&app);
    let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
    let found = read_report(&snapshot)?;
    let facts = report::crawl_facts(&dir, app_id).map_err(text)?;

    // The measurement, where this game has labelled claims the model never trained on. A game
    // without them still renders; it just cannot say how often it is wrong, and the window
    // says that instead. On a game it learned from the model reproduces its labels at 99%,
    // and that figure would advertise its memory.
    let reference = steamgauge_core::claimset::default_reference_dir(app_id);
    let labelled = reference.join("labels.json").is_file();
    let learned = labelled
        && steamgauge_core::measure::role(app_id, steamgauge_core::measure::SPLIT_SEED)
            == steamgauge_core::measure::Role::Train;
    let agreement = (labelled && !learned)
        .then(|| steamgauge_core::measure::agreement(&dir, app_id, &reference).ok())
        .flatten();
    let subjects = found
        .subjects
        .iter()
        .map(|subject| subject_row(&found, subject, agreement.as_ref()))
        .collect();

    let measured = agreement.as_ref().map(|found| {
        let interval = found.interval();
        Measured {
            answered: found.answered,
            declined: found.declined,
            agreement: found.rate(),
            low: interval.map(|(low, _)| low),
            high: interval.map(|(_, high)| high),
            unjoined: found.unjoined,
        }
    });

    Ok(Reading {
        app_id,
        name: facts.title(),
        reviews: found.reviews,
        corpus_reviews: found.corpus_reviews,
        language: found.language.clone(),
        claims: found.claims,
        unclassified_claims: found.unclassified_claims,
        silent_reviews: found.silent_reviews,
        top_of_the_pile: found.top_helpful,
        positive_baseline: share_of(found.positive, found.reviews),
        model: found.model.clone(),
        threshold: found.threshold,
        in_short: steamgauge_core::picture::in_short(&found),
        swept_since: facts
            .swept_unix
            .filter(|swept| found.captured_unix < *swept),
        subjects,
        measured,
        learned,
        frozen: found.frozen,
        months: found
            .months
            .iter()
            .map(|month| MonthOut {
                label: month.label.clone(),
                name: steamgauge_core::time::month_name(&month.label),
                reviews: month.reviews,
                positive: month.positive_share_if_enough(),
            })
            .collect(),
        languages: found
            .languages
            .iter()
            .map(|(name, reviews)| Language {
                name: name.clone(),
                reviews: *reviews,
                share: share_of(*reviews, found.corpus_reviews),
            })
            .collect(),
    })
}

/// A review quoted as it was written, with no reading attached, because the model that counts
/// the table was never asked about it.
#[derive(Debug, Clone, Serialize)]
struct Quoted {
    review_id: String,
    voted_up: bool,
    votes_up: u32,
    created: i64,
    language: String,
    url: Option<String>,
    review: String,
}

/// One subject found in this game's own reviews, with the reviews it rests on.
#[derive(Debug, Clone, Serialize)]
struct InducedOut {
    id: String,
    label: String,
    description: String,
    /// The label of the sheet subject this is a form of, when it is one.
    refines: Option<String>,
    /// How many handout reviews it was found in, which is more than are quoted.
    found_in: usize,
    reviews: Vec<Quoted>,
}

/// What this game's players talk about that the fixed subjects do not name, where the
/// induction has been run. Separate from the reading because it walks the capture for the
/// quoted reviews, and a game is selected far more often than this section is read.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn induced(app: AppHandle, app_id: u32) -> Result<Vec<InducedOut>, String> {
    let dir = library_dir(&app);
    let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
    let found = report::induced_for(app_id, &snapshot, report::DEFAULT_EXAMPLES).map_err(text)?;
    Ok(found
        .into_iter()
        .map(|evidence| InducedOut {
            refines: evidence.subject.refines.as_deref().and_then(|id| {
                steamgauge_core::SHEET
                    .iter()
                    .find(|category| category.id == id)
                    .map(|category| category.label.to_owned())
            }),
            found_in: evidence.subject.evidence.len(),
            id: evidence.subject.id,
            label: evidence.subject.label,
            description: evidence.subject.description,
            reviews: evidence
                .reviews
                .into_iter()
                .map(|review| Quoted {
                    url: (!review.author_steamid.is_empty()).then(|| {
                        format!(
                            "https://steamcommunity.com/profiles/{}/recommended/{app_id}/",
                            review.author_steamid
                        )
                    }),
                    review_id: review.id,
                    voted_up: review.voted_up,
                    votes_up: review.votes_up,
                    created: review.created,
                    language: review.language,
                    review: review.text,
                })
                .collect(),
        })
        .collect())
}

/// One claim shown as evidence, with the review it came from.
#[derive(Debug, Clone, Serialize)]
struct Evidence {
    review_id: String,
    claim: String,
    polarity: String,
    confidence: f32,
    voted_up: bool,
    votes_up: u32,
    created: i64,
    language: String,
    url: Option<String>,
    /// The rest of the review, so a claim can be read where it was written.
    review: String,
}

#[derive(Debug, Clone, Serialize)]
struct ClaimsBehind {
    subject: String,
    total: u64,
    from: usize,
    claims: Vec<Evidence>,
}

/// A claim chosen for a page, before the review it belongs to has been fetched.
type Wanted = (String, steamgauge_core::claims::Span, String, f32);

/// What the readings say about one claim of a review, before the capture is asked what it says.
type Filed = (steamgauge_core::claims::Span, String, f32);

/// Every claim filed under a subject, most helpful review first, a page at a time.
///
/// Narrowed to one side when `side` is given, and to the claims using a term when `term` is,
/// which is how a word that stands out opens onto the reviews it was counted from. Off the
/// window's thread, because a term is found by walking the whole capture, and a window that
/// stops answering for that long is a window a person force-quits.
#[tauri::command]
async fn claims_behind(
    app: AppHandle,
    app_id: u32,
    subject: String,
    side: Option<String>,
    term: Option<String>,
    from: usize,
    count: usize,
) -> Result<ClaimsBehind, String> {
    let dir = library_dir(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let _ = read_report(&snapshot)?;

        let (total, wanted) = match term
            .as_deref()
            .map(str::trim)
            .filter(|term| !term.is_empty())
        {
            Some(term) => claims_using(&snapshot, &subject, side.as_deref(), term, from, count),
            None => claims_under(&snapshot, &subject, side.as_deref(), from, count),
        }
        .map_err(text)?;

        Ok(ClaimsBehind {
            subject,
            total,
            from,
            claims: evidence(&snapshot, app_id, wanted)?,
        })
    })
    .await
    .map_err(text)?
}

/// The claims chosen for a page, each with the review it came from.
fn evidence(snapshot: &Path, app_id: u32, wanted: Vec<Wanted>) -> Result<Vec<Evidence>, String> {
    let ids: std::collections::HashSet<String> =
        wanted.iter().map(|(id, _, _, _)| id.clone()).collect();
    let fetched = steamgauge_core::capture::reviews_for(snapshot, &ids).map_err(text)?;

    Ok(wanted
        .into_iter()
        .filter_map(|(id, at, polarity, confidence)| {
            let review = fetched.get(&id)?;
            let claim = review.text.get(at.0 as usize..at.1 as usize)?.to_owned();
            let url = (!review.author_steamid.is_empty()).then(|| {
                format!(
                    "https://steamcommunity.com/profiles/{}/recommended/{app_id}/",
                    review.author_steamid
                )
            });
            Some(Evidence {
                review_id: id,
                claim,
                polarity,
                confidence,
                voted_up: review.voted_up,
                votes_up: review.votes_up,
                created: review.created,
                language: review.language.clone(),
                url,
                review: review.text.clone(),
            })
        })
        .collect())
}

/// One subject the claims that say a phrase were filed under.
#[derive(Debug, Clone, Serialize)]
struct SaidUnder {
    /// The subject's id, or `declined` for the claims the reader put no subject on.
    id: &'static str,
    label: &'static str,
    claims: u64,
}

/// What a game's reviewers said in the words searched for.
#[derive(Debug, Clone, Serialize)]
struct Searched {
    query: String,
    reviews: u64,
    /// Of the reviews the reading counted, so the share is of the same reviews every other
    /// rate on the page is of.
    share: Option<f64>,
    claims: u64,
    praise: u64,
    complaint: u64,
    neutral: u64,
    subjects: Vec<SaidUnder>,
    forms: Vec<(String, u64)>,
    narrowed: u64,
    from: usize,
    page: Vec<Evidence>,
}

/// Every claim of a read game that says what somebody typed, counted, with a page of them.
///
/// `side` and `subject` narrow the page and nothing else: the counts are always of every claim
/// that says it.
#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "tauri hands a command its arguments one by one"
)]
async fn search_game(
    app: AppHandle,
    last: tauri::State<'_, LastSearch>,
    app_id: u32,
    query: String,
    side: Option<String>,
    subject: Option<String>,
    from: usize,
    count: usize,
) -> Result<Searched, String> {
    use steamgauge_core::search::{Narrow, Phrase, Readings, search};

    let phrase = Phrase::new(&query).ok_or_else(|| "type a word to look for".to_owned())?;
    let dir = library_dir(&app);
    let last = Arc::clone(&last.0);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let report = read_report(&snapshot)?;
        let game: ReadingsKey = (
            snapshot.clone(),
            std::fs::metadata(snapshot.join("readings.parquet"))
                .and_then(|file| file.modified())
                .ok(),
        );
        let key: SearchKey = (game.clone(), phrase.clone());
        let (kept, readings) = {
            let held = last.lock().map_err(text)?;
            (
                held.said
                    .as_ref()
                    .filter(|(asked, _)| *asked == key)
                    .map(|(_, said)| Arc::clone(said)),
                held.readings
                    .as_ref()
                    .filter(|(loaded, _)| *loaded == game)
                    .map(|(_, readings)| Arc::clone(readings)),
            )
        };
        let said = if let Some(said) = kept {
            said
        } else {
            let readings = if let Some(readings) = readings {
                readings
            } else {
                let readings = Arc::new(Readings::load(&snapshot).map_err(text)?);
                last.lock().map_err(text)?.readings = Some((game, Arc::clone(&readings)));
                readings
            };
            let said = Arc::new(search(&snapshot, &readings, &phrase).map_err(text)?);
            last.lock().map_err(text)?.said = Some((key, Arc::clone(&said)));
            said
        };

        let narrow = Narrow {
            side: side.as_deref(),
            subject: subject.as_deref(),
        };
        let (narrowed, page) = said.page(narrow, from, count);
        let wanted = page
            .into_iter()
            .map(|hit| {
                (
                    hit.review_id.to_string(),
                    hit.at,
                    hit.polarity.to_owned(),
                    hit.confidence,
                )
            })
            .collect();
        Ok(Searched {
            query,
            reviews: said.reviews,
            share: share_of(said.reviews, report.reviews),
            claims: said.claims,
            praise: said.praise,
            complaint: said.complaint,
            neutral: said.neutral,
            subjects: said
                .subjects
                .iter()
                .map(|&(id, claims)| SaidUnder {
                    id,
                    label: steamgauge_core::taxonomy::SHEET
                        .iter()
                        .find(|row| row.id == id)
                        .map_or("No subject the reader would name", |row| row.label),
                    claims,
                })
                .collect(),
            forms: said.forms.clone(),
            narrowed,
            from,
            page: evidence(&snapshot, app_id, wanted)?,
        })
    })
    .await
    .map_err(text)?
}

/// A read game as it stands: the readings file's time is in it, so a game read again since is
/// searched again.
type ReadingsKey = (PathBuf, Option<std::time::SystemTime>);

type SearchKey = (ReadingsKey, steamgauge_core::search::Phrase);

/// What the window keeps between searches, one of each and not a history. The readings of the
/// game searched last, since loading them is most of a search of a large game and somebody
/// trying words asks several in a row. The last search and what it found, since a search is
/// asked again as its pages are turned and its sides chosen, and a game's every hit for a
/// common word is tens of megabytes.
#[derive(Default)]
struct Searching {
    readings: Option<(ReadingsKey, Arc<steamgauge_core::search::Readings>)>,
    said: Option<(SearchKey, Arc<steamgauge_core::search::Said>)>,
}

#[derive(Default)]
struct LastSearch(Arc<Mutex<Searching>>);

/// The encoder that searches by meaning, loaded once and kept: it is six hundred megabytes, and
/// a search that loaded it every time would spend seconds before looking at anything. With a
/// flag that stops a preparation, and one that says a preparation is running, since two at
/// once would write the same parts.
#[derive(Default)]
struct Meaning {
    embedder: Arc<Mutex<Option<steamgauge_core::embed::Embedder>>>,
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

/// Whether this machine reads on a card, which is what decides how long preparing takes.
fn meaning_on_card() -> bool {
    !steamgauge_core::reader::on_the_processor(
        steamgauge_core::card::largest(),
        steamgauge_core::model::REACHES_A_CARD,
    )
}

/// What preparing a game for search by meaning would cost on this machine, and what the window
/// recommends from what it can see.
#[derive(Debug, Clone, Serialize)]
struct MeaningOffer {
    status: steamgauge_core::meaning::Status,
    on_card: bool,
    /// About how long what is left would take here.
    seconds: f64,
    /// About what what is left would take on disk, at most.
    disk_bytes: u64,
    /// What the encoder takes to fetch, where it is not here yet; nothing once it is.
    download_bytes: u64,
    every_game: bool,
    running: bool,
    /// `prepare` on a card, where it is minutes; `wait` on a processor, where it is hours.
    recommended: &'static str,
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn meaning_offer(
    app: AppHandle,
    meaning: tauri::State<'_, Meaning>,
    app_id: u32,
) -> Result<MeaningOffer, String> {
    use steamgauge_core::meaning::{
        BYTES_PER_CLAIM, Choice, DOWNLOAD_BYTES, ENCODER, PRECISION, Times, held, status,
    };

    let dir = library_dir(&app);
    let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
    let report = read_report(&snapshot)?;
    let on_card = meaning_on_card();
    let left = report.claims.saturating_sub(held(&snapshot).0);
    let cache = steamgauge_core::model::default_cache_dir();
    let fetched = steamgauge_core::model::model_path(&cache, ENCODER, PRECISION).is_file()
        && steamgauge_core::model::tokenizer_path(&cache, ENCODER).is_file();
    Ok(MeaningOffer {
        status: status(&snapshot),
        on_card,
        seconds: Times::load(&dir).estimate(on_card, left),
        disk_bytes: left.saturating_mul(BYTES_PER_CLAIM),
        download_bytes: if fetched { 0 } else { DOWNLOAD_BYTES },
        every_game: Choice::load(&dir).every_game,
        running: meaning.running.load(Ordering::Relaxed),
        recommended: if on_card { "prepare" } else { "wait" },
    })
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn choose_meaning(app: AppHandle, every_game: bool) -> Result<(), String> {
    steamgauge_core::meaning::Choice { every_game }
        .save(&library_dir(&app))
        .map_err(text)
}

#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn stop_meaning(meaning: tauri::State<'_, Meaning>) {
    meaning.stop.store(true, Ordering::Relaxed);
}

/// How far a preparation has got, as the window draws it.
#[derive(Debug, Clone, Serialize)]
struct MeaningStep {
    app_id: u32,
    walked: u64,
    total: u64,
}

/// Loads the encoder into `slot` unless it is there already.
fn loaded(
    slot: &mut Option<steamgauge_core::embed::Embedder>,
) -> Result<&mut steamgauge_core::embed::Embedder, String> {
    use steamgauge_core::meaning::{ENCODER, PRECISION};
    if slot.is_none() {
        *slot = Some(
            steamgauge_core::embed::Embedder::load(
                &steamgauge_core::model::default_cache_dir(),
                ENCODER,
                PRECISION,
            )
            .map_err(text)?,
        );
    }
    slot.as_mut()
        .ok_or_else(|| "the encoder did not load".to_owned())
}

/// Prepares a read game for search by meaning, fetching the encoder first if it is not here.
/// Returns whether it finished; a stopped preparation keeps what it did.
#[tauri::command]
async fn prepare_meaning(
    app: AppHandle,
    meaning: tauri::State<'_, Meaning>,
    app_id: u32,
) -> Result<bool, String> {
    use steamgauge_core::meaning::{ENCODER, PRECISION, Times, prepare};

    if meaning.running.swap(true, Ordering::SeqCst) {
        return Err("a game is already being prepared".to_owned());
    }
    meaning.stop.store(false, Ordering::Relaxed);
    let running = Arc::clone(&meaning.running);
    let outcome = async {
        let fetching = app.clone();
        steamgauge_core::model::ensure(
            &steamgauge_core::model::default_cache_dir(),
            ENCODER,
            PRECISION,
            |progress| {
                let _ = fetching.emit(
                    "meaning-fetch",
                    Fetch {
                        file: progress.file.to_owned(),
                        downloaded: progress.downloaded,
                        total: progress.total,
                    },
                );
            },
        )
        .await
        .map_err(text)?;

        let dir = library_dir(&app);
        let embedder = Arc::clone(&meaning.embedder);
        let stop = Arc::clone(&meaning.stop);
        let window = app.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
            let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
            let total = read_report(&snapshot)?.claims;
            let mut slot = embedder.lock().map_err(text)?;
            let encoder = loaded(&mut slot)?;
            let on_card = encoder.device() != "cpu";
            let started = std::time::Instant::now();
            let prepared = prepare(
                &snapshot,
                |texts| encoder.embed(texts),
                &stop,
                |walked| {
                    let _ = window.emit(
                        "meaning",
                        MeaningStep {
                            app_id,
                            walked,
                            total,
                        },
                    );
                },
            )
            .map_err(text)?;
            let mut times = Times::load(&dir);
            times.note(on_card, started.elapsed().as_secs_f64(), prepared.walked);
            times.save(&dir).map_err(text)?;
            Ok(prepared.finished)
        })
        .await
        .map_err(text)?
    }
    .await;
    running.store(false, Ordering::SeqCst);
    outcome
}

/// One claim near in meaning to what was searched, with the review it came from.
#[derive(Debug, Clone, Serialize)]
struct NearClaim {
    similarity: f32,
    #[serde(flatten)]
    evidence: Evidence,
}

/// The claims nearest in meaning to what was searched, leaving out those its words found.
#[tauri::command]
async fn search_by_meaning(
    app: AppHandle,
    meaning: tauri::State<'_, Meaning>,
    last: tauri::State<'_, LastSearch>,
    app_id: u32,
    query: String,
) -> Result<Vec<NearClaim>, String> {
    use steamgauge_core::meaning::nearest;

    /// Enough to read through, and past it the claims are the least near of the near.
    const MOST: usize = 50;

    let dir = library_dir(&app);
    let embedder = Arc::clone(&meaning.embedder);
    let last = Arc::clone(&last.0);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let wanted = {
            // A preparation holds the encoder for as long as it runs, which on a processor is
            // hours, and a search that waited for it would look like one that had hung.
            let mut slot = embedder.try_lock().map_err(|_| {
                "a game is being prepared; search by meaning is back when it finishes or is \
                 stopped"
                    .to_owned()
            })?;
            let vectors = loaded(&mut slot)?
                .embed(std::slice::from_ref(&query))
                .map_err(text)?;
            vectors
                .into_iter()
                .next()
                .ok_or_else(|| "the encoder returned nothing".to_owned())?
        };
        // The claims the words found are already on the page above, so they are left out here.
        let kept = last.lock().map_err(text)?.said.clone();
        let mut found: HashMap<String, Vec<steamgauge_core::claims::Span>> = HashMap::new();
        if let Some((((held, _), phrase), said)) = kept
            && held == snapshot
            && steamgauge_core::search::Phrase::new(&query).is_some_and(|asked| asked == phrase)
        {
            for hit in &said.hits {
                found
                    .entry(hit.review_id.to_string())
                    .or_default()
                    .push(hit.at);
            }
        }
        let near = nearest(&snapshot, &wanted, MOST, |review, at| {
            found.get(review).is_some_and(|spans| spans.contains(&at))
        })
        .map_err(text)?;

        let similarity: HashMap<(String, [u8; 32]), f32> = near
            .iter()
            .map(|claim| ((claim.review_id.clone(), claim.key), claim.similarity))
            .collect();
        let wanted = near
            .into_iter()
            .map(|claim| (claim.review_id, claim.at, claim.polarity, claim.confidence))
            .collect();
        // A review edited since its vectors were made may no longer say at those bytes what
        // was embedded, and a claim quoted from the wrong bytes is worse than one not shown.
        Ok(evidence(&snapshot, app_id, wanted)?
            .into_iter()
            .filter_map(|found| {
                let key = steamgauge_core::meaning::key_of(&found.claim);
                Some(NearClaim {
                    similarity: *similarity.get(&(found.review_id.clone(), key))?,
                    evidence: found,
                })
            })
            .collect())
    })
    .await
    .map_err(text)?
}

/// The page of claims under a subject, and how many there are, from the readings alone.
fn claims_under(
    snapshot: &std::path::Path,
    subject: &str,
    side: Option<&str>,
    from: usize,
    count: usize,
) -> steamgauge_core::Result<(u64, Vec<Wanted>)> {
    let mut total: u64 = 0;
    let mut wanted: Vec<Wanted> = Vec::new();
    steamgauge_core::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, found, confidence, polarity, also| {
            let Some(polarity) = steamgauge_core::read::polarity_on(subject, found, polarity, also)
            else {
                return;
            };
            if side.is_some_and(|side| side != polarity) {
                return;
            }
            total += 1;
            if total > from as u64 && wanted.len() < count {
                wanted.push((id.to_owned(), at, polarity.to_owned(), confidence));
            }
        },
    )?;
    Ok((total, wanted))
}

/// The page of claims under a subject that use a term, and how many there are.
///
/// The readings say which claims are filed here; only the capture says what they say. So the
/// readings are gathered by review first, and the capture is walked once, taking each listed
/// review apart the way the reading pass did and keeping the claims that use the term. One
/// walk per page rather than one per review, which is the difference between a click and a
/// wait on a corpus of a million reviews.
fn claims_using(
    snapshot: &std::path::Path,
    subject: &str,
    side: Option<&str>,
    term: &str,
    from: usize,
    count: usize,
) -> steamgauge_core::Result<(u64, Vec<Wanted>)> {
    let mut filed: std::collections::HashMap<String, Vec<Filed>> = std::collections::HashMap::new();
    steamgauge_core::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, found, confidence, polarity, also| {
            let Some(polarity) = steamgauge_core::read::polarity_on(subject, found, polarity, also)
            else {
                return;
            };
            if side.is_some_and(|side| side != polarity) {
                return;
            }
            filed
                .entry(id.to_owned())
                .or_default()
                .push((at, polarity.to_owned(), confidence));
        },
    )?;

    let mut total: u64 = 0;
    let mut wanted: Vec<Wanted> = Vec::new();
    steamgauge_core::capture::for_each_row(snapshot, |row, text| {
        let Some(claims) = filed.get(&row.recommendationid) else {
            return Ok(());
        };
        for (at, polarity, confidence) in claims {
            let uses = text
                .get(at.0 as usize..at.1 as usize)
                .is_some_and(|claim| steamgauge_core::said::mentions(claim, term));
            if !uses {
                continue;
            }
            total += 1;
            if total > from as u64 && wanted.len() < count {
                wanted.push((
                    row.recommendationid.clone(),
                    *at,
                    polarity.clone(),
                    *confidence,
                ));
            }
        }
        Ok(())
    })?;
    Ok((total, wanted))
}

/// # Errors
///
/// Fails if the webview cannot be created, which on Linux means the system webview is
/// missing and on Windows means `WebView2` is not installed.
pub fn run() -> anyhow::Result<()> {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(LastSearch::default())
        .manage(Meaning::default())
        .invoke_handler(tauri::generate_handler![
            library,
            look_up,
            crawl,
            reading,
            claims_behind,
            induced,
            read_game,
            reader_choices,
            search_game,
            meaning_offer,
            choose_meaning,
            prepare_meaning,
            stop_meaning,
            search_by_meaning,
            sweep
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}
