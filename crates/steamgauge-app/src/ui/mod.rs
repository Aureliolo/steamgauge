//! The window.
//!
//! Everything here is a thin front for the same core the pipeline runs. A command in this
//! module may read the library, start a stage, and report what happened; it may not decide
//! anything a number depends on. Where the window and the terminal disagree about a figure,
//! one of them is calling the wrong function.

mod cockpit;
mod newer;
mod settings;
mod storage;
mod update;
mod who;
mod work;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use serde::Serialize;
use steamgauge_core::{
    DEFAULT_PACE, Listing, ReviewQuery, SteamClient, embed, reading_time::ReadingTimes, report,
};
use tauri::{AppHandle, Manager};

/// Where corpora live when nobody has said.
///
/// The pipeline defaults to `data` beside the working directory, which is right for a
/// terminal and wrong for an icon: an application opened from a menu has no working
/// directory worth writing gigabytes into.
fn library_dir(app: &AppHandle) -> PathBuf {
    static LIBRARY: OnceLock<PathBuf> = OnceLock::new();
    LIBRARY
        .get_or_init(|| {
            if let Some(chosen) = std::env::var_os("STEAMGAUGE_DATA") {
                return PathBuf::from(chosen);
            }
            let paths = app.path();
            match (paths.app_data_dir(), paths.app_local_data_dir()) {
                (Ok(roaming), Ok(local)) => {
                    settle_library(&roaming.join("data"), &local.join("data"))
                }
                (_, Ok(local)) => local.join("data"),
                _ => PathBuf::from("data"),
            }
        })
        .clone()
}

/// The library belongs in the local app data folder: on Windows the roaming one travels with a
/// roaming profile, which is no place for gigabytes of reviews. A library 0.1.3 or earlier made
/// in the roaming folder moves once, by rename, which on one drive is instant and all or
/// nothing; when it cannot, the library stays where it is and is used there, rather than the
/// app opening on an empty library beside the full one. On macOS and Linux the two folders are
/// one, and nothing moves.
fn settle_library(roaming: &Path, local: &Path) -> PathBuf {
    if roaming == local || local.exists() || !roaming.is_dir() {
        return local.to_path_buf();
    }
    let moved = local
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::rename(roaming, local));
    if moved.is_ok() {
        local.to_path_buf()
    } else {
        roaming.to_path_buf()
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
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

/// One client for everything the window asks the store between jobs, so a library's worth of
/// pictures and a search typed at the same time share one pace rather than each setting its own.
fn store() -> Result<&'static SteamClient, String> {
    static STORE: OnceLock<SteamClient> = OnceLock::new();
    if let Some(client) = STORE.get() {
        return Ok(client);
    }
    let client = SteamClient::new(DEFAULT_PACE)
        .map_err(text)?
        .with_patience(Duration::from_secs(30));
    Ok(STORE.get_or_init(|| client))
}

/// The games the store lists for the words typed into the box that adds one.
#[tauri::command]
async fn find_games(words: String) -> Result<Vec<Listing>, String> {
    store()?.search(&words).await.map_err(text)
}

/// How long a game the store had no picture for waits before it is asked again.
const ART_RETRY: Duration = Duration::from_hours(24 * 7);

/// A game's header picture as raw bytes, kept in the cache folder after the first time, so the
/// library draws from disk and the window itself never reaches the network. An empty file says
/// the store had none when last asked.
#[tauri::command]
async fn art(app: AppHandle, app_id: u32) -> Result<tauri::ipc::Response, String> {
    let folder = app.path().app_cache_dir().map_err(text)?.join("art");
    let kept = folder.join(app_id.to_string());
    if let Ok(found) = std::fs::metadata(&kept) {
        if found.len() > 0 {
            return std::fs::read(&kept)
                .map(tauri::ipc::Response::new)
                .map_err(text);
        }
        let recent = found
            .modified()
            .ok()
            .and_then(|when| when.elapsed().ok())
            .is_some_and(|since| since < ART_RETRY);
        if recent {
            return Err(format!("the store has no picture of app {app_id}"));
        }
    }
    let picture = store()?.header_art(app_id).await;
    std::fs::create_dir_all(&folder).map_err(text)?;
    std::fs::write(&kept, picture.as_deref().unwrap_or_default()).map_err(text)?;
    picture
        .map(tauri::ipc::Response::new)
        .ok_or_else(|| format!("the store has no picture of app {app_id}"))
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

/// What reading a game would take here, said before anything starts: which reader, what it
/// has to fetch first, and on a processor how long each size would take.
#[derive(Debug, Clone, Serialize)]
struct ReadOffer {
    reader: &'static str,
    /// What the reader still has to fetch, where it has been published.
    download_bytes: u64,
    /// Whether a reader that is not here could be fetched at all.
    published: bool,
    here: bool,
    /// The sizes to choose from, smallest first; none where a card is reached, because there
    /// the card decides.
    choices: Vec<ReaderChoice>,
}

/// What reading a game, or any game where none is named, would take on this machine.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
fn read_offer(
    app: AppHandle,
    app_id: Option<u32>,
    language: Option<String>,
) -> Result<ReadOffer, String> {
    use steamgauge_core::reader::{SIZES, on_the_processor};

    let dir = library_dir(&app);
    let size = work::reader_here(&settings::Settings::load(&app));
    let home = size.home();
    let published = size.published.is_pinned();
    let choices = if on_the_processor(
        steamgauge_core::card::largest(),
        steamgauge_core::model::REACHES_A_CARD,
    ) {
        let reviews = app_id
            .map(|app_id| report::crawl_facts(&dir, app_id).map(|facts| facts.rows_unique))
            .transpose()
            .map_err(text)?;
        let times = ReadingTimes::load(&dir);
        let fastest = SIZES[0].processor_seconds;
        SIZES
            .iter()
            .map(|size| ReaderChoice {
                name: size.name,
                seconds: reviews
                    .and_then(|reviews| times.seconds(size, language.as_deref(), reviews)),
                times: size.processor_seconds / fastest,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(ReadOffer {
        reader: size.name,
        download_bytes: if published {
            size.published.bytes_left(&home)
        } else {
            0
        },
        published,
        here: home.join("model.onnx").is_file(),
        choices,
    })
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
    /// The kinds of reviewer, and where one says something more or less often than the rest.
    who: who::Overview,
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
        corrected: corrected_share(agreement, &subject.id, subject.claims, found.claims),
        found: measured.and_then(steamgauge_core::measure::SubjectAgreement::recall),
        praised_terms: said.map(|said| said.praised.clone()).unwrap_or_default(),
        criticised_terms: said.map(|said| said.criticised.clone()).unwrap_or_default(),
    }
}

/// What the share of claims about a subject would be with the model's measured errors taken out,
/// where this game has labels enough to correct by and the model finds the subject better than
/// chance.
fn corrected_share(
    agreement: Option<&steamgauge_core::ClaimAgreement>,
    subject: &str,
    claims: u64,
    of_claims: u64,
) -> Option<f64> {
    agreement?
        .subjects
        .iter()
        .find(|s| s.id == subject)
        .filter(|s| s.labelled >= steamgauge_core::measure::ENOUGH_TO_CORRECT_A_ROW)
        .and_then(|s| share_of(claims, of_claims).and_then(|observed| s.corrected(observed)))
}

/// Whether the reader learned from this game's labels, and how often it agrees with them where it
/// did not. A game without them still renders; it just cannot say how often it is wrong, and the
/// window says that instead. On a game it learned from the model reproduces its labels at 99%,
/// and that figure would advertise its memory.
fn measurement(dir: &Path, app_id: u32) -> (bool, Option<steamgauge_core::ClaimAgreement>) {
    let reference = steamgauge_core::claimset::default_reference_dir(app_id);
    let labelled = reference.join("labels.json").is_file();
    let learned = labelled
        && steamgauge_core::measure::role(app_id, steamgauge_core::measure::SPLIT_SEED)
            == steamgauge_core::measure::Role::Train;
    let agreement = (labelled && !learned)
        .then(|| steamgauge_core::measure::agreement(dir, app_id, &reference).ok())
        .flatten();
    (learned, agreement)
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

    let (learned, agreement) = measurement(&dir, app_id);
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
        who: who::overview(&found),
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
    let found = report::induced_for(
        &steamgauge_core::induced::default_path(app_id),
        &snapshot,
        report::DEFAULT_EXAMPLES,
    )
    .map_err(text)?;
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
/// Narrowed to one side when `side` is given, to the claims using a term when `term` is, which
/// is how a word that stands out opens onto the reviews it was counted from, and to one kind of
/// reviewer's reviews when `who` is. Off the window's thread, because a term or a kind of
/// reviewer is found by walking the whole capture, and a window that stops answering for that
/// long is a window a person force-quits.
#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "a tauri command takes the window's arguments one by one"
)]
async fn claims_behind(
    app: AppHandle,
    app_id: u32,
    subject: String,
    side: Option<String>,
    term: Option<String>,
    who: Option<String>,
    from: usize,
    count: usize,
) -> Result<ClaimsBehind, String> {
    let dir = library_dir(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let _ = read_report(&snapshot)?;
        let only = who
            .as_deref()
            .map(|kind| who::written_by(&snapshot, kind))
            .transpose()?;
        let narrowed = Narrowed {
            subject: &subject,
            side: side.as_deref(),
            only: only.as_ref(),
        };

        let (total, wanted) = match term
            .as_deref()
            .map(str::trim)
            .filter(|term| !term.is_empty())
        {
            Some(term) => claims_using(&snapshot, &narrowed, term, from, count),
            None => claims_under(&snapshot, &narrowed, from, count),
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
    use steamgauge_core::search::{Narrow, Phrase};

    let phrase = Phrase::new(&query).ok_or_else(|| "type a word to look for".to_owned())?;
    let dir = library_dir(&app);
    let last = Arc::clone(&last.0);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let report = read_report(&snapshot)?;
        let said = words_found(&last, &snapshot, &phrase)?;

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

/// What the words of `phrase` find in the game read at `snapshot`, kept from before or searched
/// now. The window asks for the words and for the meaning at once, and the meaning leaves out
/// what the words found, so the lock is held while searching: whichever comes second waits and
/// finds the search the first one made, rather than making it again or finding nothing.
fn words_found(
    last: &Mutex<Searching>,
    snapshot: &Path,
    phrase: &steamgauge_core::search::Phrase,
) -> Result<Arc<steamgauge_core::search::Said>, String> {
    use steamgauge_core::search::{Readings, search};

    let mut held = last.lock().map_err(text)?;
    let game: ReadingsKey = (
        snapshot.to_path_buf(),
        std::fs::metadata(snapshot.join("readings.parquet"))
            .and_then(|file| file.modified())
            .ok(),
    );
    let key: SearchKey = (game.clone(), phrase.clone());
    if let Some((asked, said)) = &held.said
        && *asked == key
    {
        return Ok(Arc::clone(said));
    }
    let readings = match &held.readings {
        Some((loaded, readings)) if *loaded == game => Arc::clone(readings),
        _ => {
            let readings = Arc::new(Readings::load(snapshot).map_err(text)?);
            held.readings = Some((game, Arc::clone(&readings)));
            readings
        }
    };
    let said = Arc::new(search(snapshot, &readings, phrase).map_err(text)?);
    held.said = Some((key, Arc::clone(&said)));
    Ok(said)
}

/// The encoder that searches by meaning and the reranker that orders what it finds, each loaded
/// once and kept: together they are well over a gigabyte, and a search that loaded them every
/// time would spend seconds before looking at anything.
#[derive(Default)]
struct Meaning {
    embedder: Arc<Mutex<Option<steamgauge_core::search_models::SearchEncoder>>>,
    reranker: Arc<Mutex<Option<steamgauge_core::search_models::SearchReranker>>>,
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
    /// What the encoder and the reranker take to fetch, where they are not here yet; nothing
    /// once they are.
    download_bytes: u64,
    every_game: bool,
    /// The job preparing this game, where one is waiting or running.
    job: Option<u64>,
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
    work: tauri::State<'_, work::Work>,
    app_id: u32,
) -> Result<MeaningOffer, String> {
    use steamgauge_core::{
        meaning::{BYTES_PER_CLAIM, Choice, Times, held, status},
        search_models::{ENCODER, RERANKER},
    };

    let dir = library_dir(&app);
    let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
    let report = read_report(&snapshot)?;
    let on_card = meaning_on_card();
    let left = report.claims.saturating_sub(held(&snapshot).0);
    let cache = steamgauge_core::model::default_cache_dir();
    Ok(MeaningOffer {
        status: status(&snapshot),
        on_card,
        seconds: Times::load(&dir).estimate(on_card, left),
        disk_bytes: left.saturating_mul(BYTES_PER_CLAIM),
        download_bytes: ENCODER.bytes_left(&cache) + RERANKER.bytes_left(&cache),
        every_game: Choice::load(&dir).every_game,
        job: work.jobs().into_iter().find_map(|job| {
            (job.task == work::Task::Prepare { app_id }
                && matches!(job.state, work::State::Queued | work::State::Running))
            .then_some(job.id)
        }),
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

/// Loads a search model into `slot` unless it is there already.
fn loaded<T>(
    slot: &mut Option<T>,
    load: impl FnOnce(&Path) -> steamgauge_core::Result<T>,
) -> Result<&mut T, String> {
    if slot.is_none() {
        *slot = Some(load(&steamgauge_core::model::default_cache_dir()).map_err(text)?);
    }
    slot.as_mut()
        .ok_or_else(|| "the search model did not load".to_owned())
}

/// One claim near in meaning to what was searched, with the review it came from.
#[derive(Debug, Clone, Serialize)]
struct NearClaim {
    similarity: f32,
    #[serde(flatten)]
    evidence: Evidence,
}

/// The claims nearest in meaning to what was searched, leaving out those its words found, in the
/// order the reranker puts them.
#[tauri::command]
async fn search_by_meaning(
    app: AppHandle,
    meaning: tauri::State<'_, Meaning>,
    last: tauri::State<'_, LastSearch>,
    app_id: u32,
    query: String,
) -> Result<Vec<NearClaim>, String> {
    use steamgauge_core::{
        meaning::nearest,
        search_models::{SHOWN_FROM, SearchEncoder, SearchReranker, candidates},
    };

    /// Enough to read through, and past it the claims are the least near of the near.
    const MOST: usize = 50;

    let dir = library_dir(&app);
    let embedder = Arc::clone(&meaning.embedder);
    let reranker = Arc::clone(&meaning.reranker);
    let last = Arc::clone(&last.0);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let mut reranking = reranker.lock().map_err(text)?;
        let reranker = loaded(&mut reranking, SearchReranker::load)?;
        let wanted = {
            // A preparation holds the encoder for as long as it runs, which on a processor is
            // hours, and a search that waited for it would look like one that had hung.
            let mut slot = embedder.try_lock().map_err(|_| {
                "a game is being prepared; search by meaning is back when it finishes or is \
                 stopped"
                    .to_owned()
            })?;
            loaded(&mut slot, SearchEncoder::load)?
                .search(&query)
                .map_err(text)?
        };
        // The claims the words found are already on the page above, so they are left out here.
        let mut found: HashMap<String, Vec<steamgauge_core::claims::Span>> = HashMap::new();
        if let Some(phrase) = steamgauge_core::search::Phrase::new(&query) {
            let said = words_found(&last, &snapshot, &phrase)?;
            for hit in &said.hits {
                found
                    .entry(hit.review_id.to_string())
                    .or_default()
                    .push(hit.at);
            }
        }
        let near = nearest(
            &snapshot,
            &wanted,
            candidates(reranker.device()),
            |review, at| found.get(review).is_some_and(|spans| spans.contains(&at)),
        )
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
        let near: Vec<NearClaim> = evidence(&snapshot, app_id, wanted)?
            .into_iter()
            .filter_map(|found| {
                let key = steamgauge_core::meaning::key_of(&found.claim);
                Some(NearClaim {
                    similarity: *similarity.get(&(found.review_id.clone(), key))?,
                    evidence: found,
                })
            })
            .collect();

        let claims: Vec<String> = near.iter().map(|one| one.evidence.claim.clone()).collect();
        let scores = reranker.score(&query, &claims).map_err(text)?;
        let mut ranked: Vec<(f32, NearClaim)> = scores
            .into_iter()
            .zip(near)
            .filter(|(score, _)| *score >= SHOWN_FROM)
            .collect();
        // Stable, so claims it scores alike keep the embedding's order.
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        Ok(ranked.into_iter().take(MOST).map(|(_, one)| one).collect())
    })
    .await
    .map_err(text)?
}

/// Which claims a page of evidence is drawn from: those on a subject, on one side of it where a
/// side is named, in the reviews of one kind of reviewer where their ids are given.
struct Narrowed<'a> {
    subject: &'a str,
    side: Option<&'a str>,
    only: Option<&'a std::collections::HashSet<String>>,
}

impl Narrowed<'_> {
    /// The polarity a stored reading takes on the subject, where it is one of the claims wanted.
    fn polarity(
        &self,
        id: &str,
        found: Option<&str>,
        polarity: &str,
        also: steamgauge_core::reader::Also,
    ) -> Option<&'static str> {
        if self.only.is_some_and(|only| !only.contains(id)) {
            return None;
        }
        let polarity = steamgauge_core::read::polarity_on(self.subject, found, polarity, also)?;
        self.side
            .is_none_or(|side| side == polarity)
            .then_some(polarity)
    }
}

/// The page of claims under a subject, and how many there are, from the readings alone.
fn claims_under(
    snapshot: &std::path::Path,
    narrowed: &Narrowed<'_>,
    from: usize,
    count: usize,
) -> steamgauge_core::Result<(u64, Vec<Wanted>)> {
    let mut total: u64 = 0;
    let mut wanted: Vec<Wanted> = Vec::new();
    steamgauge_core::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, found, confidence, polarity, also| {
            let Some(polarity) = narrowed.polarity(id, found, polarity, also) else {
                return;
            };
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
    narrowed: &Narrowed<'_>,
    term: &str,
    from: usize,
    count: usize,
) -> steamgauge_core::Result<(u64, Vec<Wanted>)> {
    let mut filed: std::collections::HashMap<String, Vec<Filed>> = std::collections::HashMap::new();
    steamgauge_core::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, found, confidence, polarity, also| {
            let Some(polarity) = narrowed.polarity(id, found, polarity, also) else {
                return;
            };
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
        .manage(work::Work::default())
        .manage(update::Updating::default())
        .setup(|app| {
            update::tidy(app.handle());
            work::start(app.handle());
            cockpit::check_on_opening(app.handle());
            newer::check_in_background(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library,
            look_up,
            find_games,
            art,
            reading,
            who::who_wrote,
            claims_behind,
            induced,
            read_offer,
            search_game,
            meaning_offer,
            choose_meaning,
            search_by_meaning,
            work::work,
            work::queue,
            work::stop_job,
            work::clear_finished,
            work::open_report,
            settings::settings,
            settings::save_settings,
            settings::reader_options,
            storage::storage,
            storage::free_room,
            storage::remove_model,
            newer::newer_version,
            update::update_state,
            update::update_now,
            update::show_update_file,
            cockpit::overview,
            cockpit::games,
            cockpit::groups,
            cockpit::save_groups,
            cockpit::compare,
            cockpit::export_report,
            cockpit::queue_reads,
            cockpit::queue_updates
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::settle_library;
    use std::path::PathBuf;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("steamgauge-library-{name}-{}", std::process::id()));
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

    #[test]
    fn a_first_start_keeps_the_library_in_the_local_folder() {
        let scratch = Scratch::new("first");
        let (roaming, local) = (scratch.0.join("Roaming/data"), scratch.0.join("Local/data"));
        assert_eq!(settle_library(&roaming, &local), local);
        assert!(!roaming.exists());
    }

    #[test]
    fn a_library_in_the_roaming_folder_moves_whole_to_the_local_one() {
        let scratch = Scratch::new("moves");
        let (roaming, local) = (scratch.0.join("Roaming/data"), scratch.0.join("Local/data"));
        std::fs::create_dir_all(roaming.join("appid=1")).unwrap();
        std::fs::write(roaming.join("appid=1/reviews.parquet"), b"held").unwrap();
        assert_eq!(settle_library(&roaming, &local), local);
        assert_eq!(
            std::fs::read(local.join("appid=1/reviews.parquet")).unwrap(),
            b"held"
        );
        assert!(!roaming.exists());
    }

    #[test]
    fn a_library_already_in_the_local_folder_is_left_beside_an_old_one() {
        let scratch = Scratch::new("both");
        let (roaming, local) = (scratch.0.join("Roaming/data"), scratch.0.join("Local/data"));
        std::fs::create_dir_all(&roaming).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        assert_eq!(settle_library(&roaming, &local), local);
        assert!(roaming.exists());
    }

    #[test]
    fn a_library_that_cannot_move_is_used_where_it_is() {
        let scratch = Scratch::new("stuck");
        let roaming = scratch.0.join("Roaming/data");
        std::fs::create_dir_all(&roaming).unwrap();
        // A file where the local folder's parent has to be, so the move cannot happen.
        std::fs::write(scratch.0.join("Local"), b"").unwrap();
        assert_eq!(
            settle_library(&roaming, &scratch.0.join("Local/data")),
            roaming
        );
        assert!(roaming.exists());
    }

    #[test]
    fn one_folder_for_both_moves_nothing() {
        let scratch = Scratch::new("same");
        let both = scratch.0.join("data");
        std::fs::create_dir_all(&both).unwrap();
        assert_eq!(settle_library(&both, &both), both);
        assert!(both.exists());
    }
}
