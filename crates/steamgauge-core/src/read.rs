//! Reading a whole corpus, one claim at a time.
//!
//! One walk over the capture. Every distinct claim is asked about, and a review is added up as
//! soon as the model has answered the claims it holds. Distinct rather than every claim because
//! "Great game." is one claim written a thousand times, and reading it a thousand times is a
//! thousand times the electricity for the same answer.
//!
//! What comes out is deliberately three different shapes of number, and the difference
//! matters more than any of them:
//!
//! - **Mention rate**: the share of reviews raising a subject. A review counts once however
//!   many claims it makes about it, so nobody's verbosity moves it. This is the headline.
//! - **Polarity**: per review, per subject, as praised, criticised or mixed. A review that
//!   loves the art and hates the framerate says two things and is recorded as saying two.
//! - **Claim share**: the share of all claims. Verbosity-weighted, useful for reading,
//!   never a headline, and labelled wherever it appears.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use arrow::{
    array::{ArrayRef, Float32Builder, StringBuilder, UInt32Builder},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use parquet::{
    arrow::ArrowWriter,
    basic::{Compression, ZstdLevel},
    file::properties::WriterProperties,
};
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    reader::{Asked, ClaimReader, Polarity, Prepared, Reading},
    taxonomy::SHEET,
};

/// Claims per forward pass. Claims are short, so this is larger than the review-level default.
///
/// Measured rather than guessed, on a game of 216,778 claims, sizes run in a mirrored order and
/// the card watched throughout: 183s at 128 against 165s at both 256 and 512. The two larger
/// sizes are the same speed to within the spread of a single size, and 256 asks the card for
/// 2.5 GB where 512 asks for 3.9 GB, so the smaller of two equals wins. The window is sorted by
/// length before it is cut into batches, so a larger batch saves no padding; what it buys is a
/// card that is not waiting on the next launch, and by 256 it is no longer waiting.
pub const DEFAULT_READ_BATCH: usize = 256;

#[derive(Debug, Clone)]
pub struct ReadOptions {
    pub out_dir: PathBuf,
    pub top_helpful: usize,
    pub batch_size: usize,
    /// Which language to count. The capture is always the whole census; this decides what is
    /// counted from it, so the choice can change without re-downloading anything and every
    /// figure can say which reviews it is about.
    pub language: Option<String>,
    pub depth: Depth,
}

/// How closely each review is read.
///
/// Neither setting drops a review. What changes is how finely one is taken apart before the
/// model reads it, and therefore what a count is a count of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Depth {
    /// A review becomes the separate points it makes, and each point is read on its own.
    /// This is what makes a mention rate honest: a review that praises the art and damns the
    /// story counts once for each, rather than once for whichever the model noticed.
    #[default]
    Deep,
    /// The whole review is one point. A third of the work, and it systematically understates
    /// anyone who wrote more than a sentence, because the model answers about the review as a
    /// whole and a review about six things is about none of them clearly enough.
    Shallow,
}

impl Depth {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deep => "deep",
            Self::Shallow => "shallow",
        }
    }

    /// The points a review makes, at this depth.
    ///
    /// Every place the reading pass takes a review apart goes through here, so the pass and
    /// the readings it writes cannot disagree about which words a reading describes.
    #[must_use]
    pub fn claims_of(self, text: &str) -> Vec<std::borrow::Cow<'_, str>> {
        match self {
            Self::Deep => crate::claims::split(text),
            Self::Shallow => {
                let whole = text.trim();
                if whole.is_empty() {
                    Vec::new()
                } else {
                    vec![std::borrow::Cow::Borrowed(whole)]
                }
            }
        }
    }

    /// The same points, as byte ranges into the review, in the same order and number as
    /// [`Self::claims_of`] gives them.
    #[must_use]
    pub fn spans_of(self, text: &str) -> Vec<std::ops::Range<usize>> {
        match self {
            Self::Deep => crate::claims::spans(text),
            Self::Shallow => {
                let whole = text.trim();
                if whole.is_empty() {
                    Vec::new()
                } else {
                    let start = text.len() - text.trim_start().len();
                    std::iter::once(start..start + whole.len()).collect()
                }
            }
        }
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            out_dir: PathBuf::from("data"),
            top_helpful: crate::capture::DEFAULT_TOP_HELPFUL,
            batch_size: DEFAULT_READ_BATCH,
            language: None,
            depth: Depth::Deep,
        }
    }
}

/// How far a reading has got. One walk does both jobs, so both numbers move together and a
/// watcher never sees the corpus start again from nothing.
#[derive(Debug, Clone, Copy)]
pub struct ReadProgress {
    pub claims_read: u64,
    pub reviews_counted: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Tally {
    mention_reviews: u64,
    primary_reviews: u64,
    claims: u64,
    praised: u64,
    criticised: u64,
    mixed: u64,
    top_mention_reviews: u64,
    positive_mentions: u64,
}

/// One subject, counted over a corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectCount {
    pub id: String,
    pub label: String,
    /// Reviews raising this subject at least once. The headline denominator.
    pub mention_reviews: u64,
    /// Reviews whose main subject this is, which are exhaustive across subjects.
    pub primary_reviews: u64,
    /// Claims about this subject. Verbosity-weighted; never a headline.
    pub claims: u64,
    pub praised: u64,
    pub criticised: u64,
    pub mixed: u64,
    pub top_mention_reviews: u64,
    /// Of the reviews raising this, how many still recommended the game.
    pub positive_mentions: u64,
}

impl SubjectCount {
    /// Share of the reviews raising this subject that recommended the game.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    pub fn positive_share(&self) -> Option<f64> {
        (self.mention_reviews > 0)
            .then(|| self.positive_mentions as f64 / self.mention_reviews as f64)
    }

    /// Reviews that both praise and criticise this subject, as a share of those raising it.
    ///
    /// The figure the old report had no way to produce. A subject praised by half and damned
    /// by the other half, and one that every reviewer has mixed feelings about, are different
    /// findings that a single positive share renders identically.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    pub fn mixed_share(&self) -> Option<f64> {
        (self.mention_reviews > 0).then(|| self.mixed as f64 / self.mention_reviews as f64)
    }
}

/// One month of a corpus.
///
/// A subject raised steadily for two years and one raised furiously in a single week read
/// identically in a total, and they are not the same fact about anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Month {
    /// `2024-02`, which sorts as it reads.
    pub label: String,
    pub reviews: u64,
    pub positive: u64,
    /// Reviews raising each subject, in taxonomy order.
    pub subjects: Vec<u64>,
}

impl Month {
    /// Reviews a month needs before a rate of it is worth drawing. Below this a single review
    /// moves the figure by tens of points, and one such month would set the scale for every
    /// month that has something to say.
    pub const ENOUGH_FOR_A_RATE: u64 = 30;

    /// Share of this month's reviews that recommended the game.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    pub fn positive_share(&self) -> Option<f64> {
        (self.reviews > 0).then(|| self.positive as f64 / self.reviews as f64)
    }

    /// The positive share, only where the month is big enough to carry one.
    #[must_use]
    pub fn positive_share_if_enough(&self) -> Option<f64> {
        (self.reviews >= Self::ENOUGH_FOR_A_RATE)
            .then(|| self.positive_share())
            .flatten()
    }

    /// Share of this month's reviews raising a subject, by its taxonomy position.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    pub fn rate(&self, slot: usize) -> Option<f64> {
        let raised = *self.subjects.get(slot)?;
        (self.reviews > 0).then(|| raised as f64 / self.reviews as f64)
    }
}

/// What reading a corpus found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadReport {
    pub app_id: u32,
    /// Reviews counted, which is every review in the capture unless a language was named.
    pub reviews: u64,
    /// Reviews in the capture, whatever the language.
    pub corpus_reviews: u64,
    pub language: Option<String>,
    /// How each review was taken apart. A draw that lists a review's claims beside its
    /// readings has to take the review apart the same way, so this is recorded rather than
    /// assumed.
    #[serde(default)]
    pub depth: Depth,
    /// Claims per forward pass. A batch is padded to its longest member, so its composition
    /// decides where half precision rounds, and each doubling moves about 75 answers of a
    /// game's 216,778. Three in ten thousand is not a reason to hold the size still, and it is
    /// a reason for a reading to say which size answered it.
    ///
    /// `None` on a reading written before this was recorded, which is not the same as zero.
    #[serde(default)]
    pub batch_size: Option<usize>,
    pub claims: u64,
    /// How many claims the model was actually asked about. A claim written a thousand times
    /// is one question, and this is what says how much of the corpus was repetition rather
    /// than reading. The whole case for reading a library locally is what it costs, so what
    /// it cost is recorded rather than argued.
    #[serde(default)]
    pub forward_passes: u64,
    /// Claims the model would not put a subject on. Reported rather than filed under
    /// whatever scored highest, which is the whole point of the rebuild.
    pub unclassified_claims: u64,
    /// Reviews where no claim got a subject at all.
    pub silent_reviews: u64,
    /// Reviews the splitter found no claim in: markup, a row of emoji, a single punctuation
    /// mark. They are reviews of the corpus and are counted as such, and they write no rows,
    /// so without this the counts and the rows beside them cannot be reconciled.
    #[serde(default)]
    pub claimless_reviews: u64,
    pub positive: u64,
    pub top_helpful: u64,
    pub model: String,
    /// Which labels the model was trained from. Two readers with the same backbone and the
    /// same threshold trained on different label sets give different numbers, and a reading
    /// that recorded only the backbone could not say which it was.
    #[serde(default)]
    pub trained_on: String,
    /// Which training run produced the reader. The backbone and the labels are shared by every
    /// candidate of one generation, so without this a library read with a new model skips every
    /// game the old one read and the corpus quietly holds two models' answers.
    #[serde(default)]
    pub read_with: String,
    /// What the reader is called, where it has a name; empty on a reading made before it did.
    #[serde(default)]
    pub reader: String,
    /// Which abstention rule the reader was carrying. The run id names the weights; the lines
    /// are drawn separately and can be redrawn without retraining, so two readings of one
    /// corpus by one run id can hold different answers and this is what says why.
    #[serde(default)]
    pub read_by_rule: String,
    /// What this model usually declines, on games it never saw, so this corpus's share can be
    /// read against something.
    #[serde(default)]
    pub usual_declined: Option<f32>,
    /// What the reader scored on games nobody involved in it had seen. Copied into the
    /// reading so a report can say what its rates are worth without the model being present.
    #[serde(default)]
    pub frozen: Option<crate::reader::Frozen>,
    /// Whether each claim was read with its review around it. The same model cannot be asked
    /// both ways, so this says which question the corpus was actually asked.
    #[serde(default)]
    pub context: bool,
    /// Which categories the model was trained against, so it cannot be read as
    /// answering a question it was never asked. Accepts the name the sheet used to
    /// carry, because every reader and reading already on disk records that.
    pub threshold: f32,
    pub device: String,
    /// When the capture this describes was last changed: its crawl, or the last sweep that
    /// brought it up to date. A capture swept since is one these counts no longer describe.
    #[serde(default)]
    pub captured_unix: i64,
    pub subjects: Vec<SubjectCount>,
    /// The terms that separate each subject's praise from its complaints, in taxonomy order.
    #[serde(default)]
    pub said: Vec<crate::said::SaidAbout>,
    pub languages: Vec<(String, u64)>,
    /// The languages in this corpus the reader has no line for, and the reviews written in
    /// them. Those reviews are read and declined in full, so without this the page shows a
    /// corpus declined far above the usual rate and offers no reason: the reason is that the
    /// reference set holds too few claims in that language to promise anything, which is a
    /// fact about the labels rather than about the game.
    #[serde(default)]
    pub unread_languages: Vec<(String, u64)>,
    /// The languages in this corpus the reader has to be surer about than English before it
    /// will answer, and the reviews written in them.
    ///
    /// A corpus declined far above the usual rate has two possible causes now and they want
    /// opposite work: it is about something the taxonomy lacks, or it is written in the
    /// languages the reference set covers worst. The first wants a category and the second
    /// wants labels. Silenced languages are usually a rounding error next to this one, so
    /// without it the page offers the rarer explanation for the commoner cause.
    #[serde(default)]
    pub strict_languages: Vec<(String, u64)>,
    /// What was said month by month, oldest first.
    pub months: Vec<Month>,
    #[serde(skip)]
    pub elapsed: Duration,
}

impl ReadReport {
    /// Share of claims the model declined to put a subject on.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "claim counts are far below 2^53"
    )]
    pub fn unclassified_share(&self) -> Option<f64> {
        (self.claims > 0).then(|| self.unclassified_claims as f64 / self.claims as f64)
    }

    /// How much more of this corpus the model declined than it usually does, as a ratio.
    ///
    /// `None` when the model carries no usual figure to compare against.
    #[must_use]
    pub fn declined_against_usual(&self) -> Option<f64> {
        let usual = f64::from(self.usual_declined?);
        let here = self.unclassified_share()?;
        (usual > 0.0).then(|| here / usual)
    }

    /// Whether this corpus is declined enough above the usual rate to be a finding about the
    /// game rather than a detail of the run.
    ///
    /// A ratio on its own stopped meaning what it meant. When the reader declined 45% of an
    /// ordinary corpus, 1.2 times that was nine points more; against a reader that declines
    /// 16%, the same ratio is three, which is the difference between one hard game and
    /// another. So the gap has to be wide in points as well as in proportion.
    #[must_use]
    pub fn declined_unusually(&self) -> bool {
        let (Some(ratio), Some(usual), Some(here)) = (
            self.declined_against_usual(),
            self.usual_declined,
            self.unclassified_share(),
        ) else {
            return false;
        };
        ratio >= UNUSUALLY_DECLINED && here - f64::from(usual) >= UNUSUALLY_DECLINED_BY
    }
}

/// Above this ratio to the usual decline rate, and this many points above it, a corpus is
/// about something the taxonomy lacks rather than merely hard to read.
pub const UNUSUALLY_DECLINED: f64 = 1.2;
pub const UNUSUALLY_DECLINED_BY: f64 = 0.05;

impl ReadReport {
    /// Writes the counts beside the readings they were computed from.
    ///
    /// # Errors
    ///
    /// Fails if the snapshot cannot be written to.
    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

fn reading_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("recommendationid", DataType::Utf8, false),
        // Where the claim sits in the review, which is what a label names too. An ordinal
        // would name whatever sentence happens to sit in that position after the next
        // splitter change, and every reader of these rows would have to be told which
        // splitter wrote them before it could believe a single one.
        Field::new("start", DataType::UInt32, false),
        Field::new("end", DataType::UInt32, false),
        // Null where the model declined, which is a recorded answer rather than a gap.
        Field::new("subject", DataType::Utf8, true),
        Field::new("confidence", DataType::Float32, false),
        Field::new("polarity", DataType::Utf8, false),
    ]))
}

/// Reads every claim in the most recent capture.
///
/// # Errors
///
/// Fails if the capture is missing, or if reading, running the model or writing fails.
pub fn read_corpus(
    model: &mut ClaimReader,
    app_id: u32,
    options: &ReadOptions,
    mut on_progress: impl FnMut(ReadProgress),
) -> Result<ReadReport> {
    let started = Instant::now();
    let snapshot = crate::embed::latest_snapshot(&options.out_dir, app_id)?;

    let context = model.provenance().context;
    let (counted, forward_passes) =
        read_and_count(model, app_id, &snapshot, options, context, &mut on_progress)?;
    let captured = crate::report::crawl_facts(&options.out_dir, app_id)?;

    Ok(ReadReport {
        elapsed: started.elapsed(),
        forward_passes,
        device: model.device().to_owned(),
        threshold: model.provenance().threshold,
        model: model.provenance().trained_from.clone(),
        trained_on: model.provenance().data_fingerprint.clone(),
        read_with: model.provenance().run_id.clone(),
        reader: model.provenance().name.clone(),
        read_by_rule: model.provenance().lines_fingerprint.clone(),
        usual_declined: model.provenance().usual_declined,
        frozen: model.provenance().frozen,
        context,
        captured_unix: captured.changed_unix(),
        ..counted
    })
}

/// Counts a corpus again from the readings a model already wrote, without the model.
///
/// What a page shows is added up at read time, and a change to the adding up (which words
/// stand out, how a month is cut) would otherwise cost every game a reading again, five hours
/// of the card for a library. The answers do not change; only what is counted from them does,
/// so they are replayed from `readings.parquet` through the same counting a reading goes
/// through, and the counts land in `reading.json` as before. The readings themselves are
/// never rewritten: what the replay writes goes to a file beside them and is thrown away.
///
/// The provenance must be the reader that answered: the language lines it draws are part of
/// the counting, and a reading records which lines it was answered under.
///
/// # Errors
///
/// Fails if the capture, the readings or the reading are missing, if the readings were
/// answered under other lines than the reader named, or if this build takes a review apart
/// differently from the build that read it, so the answers no longer name its claims.
pub fn recount_corpus(
    out_dir: &Path,
    app_id: u32,
    top_helpful: usize,
    provenance: &crate::reader::Provenance,
    mut on_progress: impl FnMut(ReadProgress),
) -> Result<ReadReport> {
    let started = Instant::now();
    let snapshot = crate::embed::latest_snapshot(out_dir, app_id)?;
    let earlier: ReadReport =
        serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).map_err(|_| {
            Error::NoClassifications {
                path: snapshot.join("reading.json"),
            }
        })?)?;
    if earlier.read_by_rule != provenance.lines_fingerprint {
        return Err(Error::Refused(format!(
            "the readings of {app_id} were answered under the lines {} and the reader named \
             draws {}; a recount adds up what the reader that answered declined, so it needs \
             that reader",
            earlier.read_by_rule, provenance.lines_fingerprint
        )));
    }
    let options = ReadOptions {
        out_dir: out_dir.to_path_buf(),
        top_helpful,
        batch_size: earlier.batch_size.unwrap_or_default(),
        language: earlier.language.clone(),
        depth: earlier.depth,
    };
    let context = earlier.context;

    let position = |name: &str| SHEET.iter().position(|category| category.id == name);
    let mut stored: Stored = HashMap::new();
    for_each_reading(
        &snapshot.join("readings.parquet"),
        |id, at, subject, confidence, polarity| {
            let reading = Reading {
                subject: subject.and_then(position),
                confidence,
                polarity: Polarity::from_name(polarity),
            };
            stored.entry(id.to_owned()).or_default().push((at, reading));
        },
    )?;

    let replay = snapshot.join("readings.recount.parquet");
    let counted = recount_rows(
        &snapshot,
        &replay,
        &options,
        context,
        &stored,
        &mut on_progress,
    )
    .and_then(|counted| {
        // A claim the replay found no answer for is one this build cuts differently from
        // the build that read it, and the counts would quietly be about another corpus.
        if counted.claims != earlier.claims || counted.unclassified != earlier.unclassified_claims {
            return Err(Error::Refused(format!(
                "this build takes {app_id} apart into {} claims, {} of them unanswered, \
                     where the reading counted {} and {}; the readings no longer name this \
                     build's claims, so read the game again",
                counted.claims, counted.unclassified, earlier.claims, earlier.unclassified_claims
            )));
        }
        counted.finish(app_id, &options, provenance)
    });
    let _ = std::fs::remove_file(&replay);
    let counted = counted?;
    Ok(ReadReport {
        elapsed: started.elapsed(),
        forward_passes: earlier.forward_passes,
        device: earlier.device,
        threshold: earlier.threshold,
        model: earlier.model,
        trained_on: earlier.trained_on,
        read_with: earlier.read_with,
        // The name the reader carries now: the run id is the identity, and the recount has
        // already checked the reading is this reader's, so a reading made before it had a
        // name, or under an earlier one, is called what it is called today.
        reader: provenance.name.clone(),
        read_by_rule: earlier.read_by_rule,
        usual_declined: earlier.usual_declined,
        frozen: earlier.frozen,
        context,
        captured_unix: earlier.captured_unix,
        ..counted
    })
}

/// Every stored answer of a corpus, by review id and then by the span each names.
type Stored = HashMap<String, Vec<((u32, u32), Reading)>>;

/// Walks the capture once, handing every review its stored answers, and returns the counting
/// before it is finished, so the walk's totals can be checked against the reading's.
fn recount_rows(
    snapshot: &Path,
    replay: &Path,
    options: &ReadOptions,
    context: bool,
    stored: &Stored,
    on_progress: &mut impl FnMut(ReadProgress),
) -> Result<Counting> {
    let mut counting = Counting::new(replay, options.top_helpful)?;
    let mut answers: HashMap<[u8; 32], Reading> = HashMap::new();
    let mut claims_seen: u64 = 0;
    crate::capture::for_each_row(snapshot, |row, text| {
        counting.note_corpus(&row);
        if options
            .language
            .as_ref()
            .is_some_and(|wanted| wanted != &row.language)
        {
            return Ok(());
        }
        let claims = options.depth.claims_of(text);
        let joined: Arc<str> = Arc::from(rejoined(&claims));
        let fingerprint = if context {
            review_key(&joined)
        } else {
            [0; 32]
        };
        let origin: Vec<(u32, u32)> = options
            .depth
            .spans_of(text)
            .into_iter()
            .map(|span| {
                (
                    u32::try_from(span.start).unwrap_or(u32::MAX),
                    u32::try_from(span.end).unwrap_or(u32::MAX),
                )
            })
            .collect();
        let filed = stored.get(&row.recommendationid);
        let mut spans = Vec::with_capacity(claims.len());
        let mut at = 0;
        answers.clear();
        for (index, claim) in claims.iter().enumerate() {
            let starts = at;
            at += claim.len() + 1;
            spans.push((starts, starts + claim.len()));
            let answered = filed.and_then(|filed| {
                let span = origin.get(index)?;
                filed.iter().find(|(where_, _)| where_ == span)
            });
            if let Some((_, reading)) = answered {
                answers.insert(
                    key(context, &fingerprint, index, claim, &row.language),
                    *reading,
                );
            }
        }
        claims_seen += claims.len() as u64;
        counting.count(
            &Pending {
                row,
                text: joined,
                spans,
                at: origin,
                fingerprint,
            },
            context,
            &answers,
        )?;
        if counting.reviews % 25_000 == 0 {
            on_progress(ReadProgress {
                claims_read: claims_seen,
                reviews_counted: counting.reviews,
            });
        }
        Ok(())
    })?;
    Ok(counting)
}

/// Where a reading is filed.
///
/// A claim read alone is the same question wherever it appears, so one answer serves every copy
/// of it and a corpus of a million reviews is a few hundred thousand forward passes.
///
/// A claim read with its review around it is a different question in a different review, but
/// the same question in every copy of the same review: the window is cut from the text, so the
/// same text at the same index gives the same window and the same answer. Filed by the text
/// rather than by the review's id, which keeps the saving wherever a review was written twice
/// and is where most of a corpus's repetition is: "Great game." is a whole review thousands of
/// times over.
fn key(context: bool, review: &[u8; 32], index: usize, claim: &str, language: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    if context {
        hasher.update(review);
        hasher.update(index.to_le_bytes());
    } else {
        hasher.update(claim.as_bytes());
    }
    // The language is part of the question now that the abstention line is drawn per language:
    // "10/10" in a Russian review and the same two characters in an English one clear different
    // bars, and a cache keyed on the text alone would hand the first answer to the second.
    hasher.update(language.as_bytes());
    hasher.finalize().into()
}

/// What two copies of one review have in common and two different reviews never do.
fn review_key(review: &str) -> [u8; 32] {
    crate::embed::sha256_bytes(review)
}

/// The claims of a review joined back together, which is what a labeller was shown.
///
/// Not the captured text: the splitter drops markup, list bullets and ticked boxes, and a
/// model trained on what the labeller read must be asked in the same form.
fn rejoined(claims: &[std::borrow::Cow<'_, str>]) -> String {
    claims.join(" ")
}

/// One claim queued for the model, with the review it sits in.
struct Queued {
    key: [u8; 32],
    claim: String,
    review: Arc<str>,
    at: usize,
    language: Arc<str>,
}

impl Queued {
    fn asked(&self) -> Asked<'_> {
        Asked {
            claim: &self.claim,
            review: &self.review,
            at: self.at,
            language: &self.language,
        }
    }
}

/// A review whose claims have been queued, waiting for the model to answer them.
///
/// Held by its claims rejoined rather than as it was captured: that is the form a labeller read
/// and the form the model is asked in, and slicing it gives the claims back without splitting
/// the review a second time.
struct Pending {
    row: crate::capture::Row,
    text: Arc<str>,
    spans: Vec<(usize, usize)>,
    /// The same claims as byte ranges into the review as it was captured, which is what a
    /// reading records and what a label names. `spans` above indexes the rejoined text the
    /// model was shown, and the two are different numbers for the same claim.
    at: Vec<(u32, u32)>,
    fingerprint: [u8; 32],
}

impl Pending {
    fn claims(&self) -> Vec<&str> {
        self.spans
            .iter()
            .map(|&(from, to)| &self.text[from..to])
            .collect()
    }
}

/// Reviews waiting on the model, beyond which they are counted whatever the window holds.
///
/// A corpus of a million copies of "Great game." queues one claim and drains nothing, so
/// without this the reviews waiting to be counted would be the whole corpus, in memory.
const PENDING_CAP: usize = 16_384;

/// Asks the model about every distinct claim, and adds up each review as its answers arrive.
///
/// One walk rather than two. The counting pass used to open the capture again, split every
/// review a second time and tally with the card idle; now a review is counted at the first
/// drain after its last claim was queued, which is the same arithmetic in the same order
/// against answers that are already final.
fn read_and_count(
    model: &mut ClaimReader,
    app_id: u32,
    snapshot: &Path,
    options: &ReadOptions,
    context: bool,
    on_progress: &mut impl FnMut(ReadProgress),
) -> Result<(ReadReport, u64)> {
    let mut answers: HashMap<[u8; 32], Reading> = HashMap::new();
    let mut window: Vec<Queued> = Vec::with_capacity(LENGTH_WINDOW);
    let mut pending: Vec<Pending> = Vec::new();
    let mut counting = Counting::new(&snapshot.join("readings.parquet"), options.top_helpful)?;
    // One allocation for every review a model that reads claims alone will ever queue.
    let nothing: Arc<str> = Arc::from("");

    crate::capture::for_each_row(snapshot, |row, text| {
        counting.note_corpus(&row);
        // Reading a claim nothing will count is a forward pass for nothing, and on a corpus
        // where the named language is a third of the reviews it is most of the work.
        if options
            .language
            .as_ref()
            .is_some_and(|wanted| wanted != &row.language)
        {
            return Ok(());
        }
        let claims = options.depth.claims_of(text);
        let joined: Arc<str> = Arc::from(rejoined(&claims));
        // Only a context reading files by the text, and hashing a review nothing will look up
        // is work on every review of the corpus.
        let fingerprint = if context {
            review_key(&joined)
        } else {
            [0; 32]
        };
        let review = if context {
            Arc::clone(&joined)
        } else {
            Arc::clone(&nothing)
        };
        let language: Arc<str> = Arc::from(row.language.as_str());
        // Where each claim sits in the review as captured. A reading is joined to a label by
        // this and by nothing else, so it is taken from the same splitter run that produced
        // the claims rather than recovered later from a second one.
        let origin: Vec<(u32, u32)> = options
            .depth
            .spans_of(text)
            .into_iter()
            .map(|span| {
                (
                    u32::try_from(span.start).unwrap_or(u32::MAX),
                    u32::try_from(span.end).unwrap_or(u32::MAX),
                )
            })
            .collect();
        let mut spans = Vec::with_capacity(claims.len());
        let mut at = 0;
        for (index, claim) in claims.iter().enumerate() {
            let starts = at;
            at += claim.len() + 1;
            spans.push((starts, starts + claim.len()));
            let key = key(context, &fingerprint, index, claim, &language);
            if answers.contains_key(&key) {
                continue;
            }
            // Reserved immediately, so a claim repeated later in the same window is not
            // queued twice. The reading is filled in when the window drains.
            answers.insert(key, Reading::default());
            window.push(Queued {
                key,
                claim: claim.to_string(),
                review: Arc::clone(&review),
                at: starts,
                language: Arc::clone(&language),
            });
            if window.len() >= LENGTH_WINDOW {
                drain(model, options.batch_size, &mut window, &mut answers)?;
                // Every review already waiting had all of its claims queued before that
                // drain, so every answer it needs is final. This one does not: its remaining
                // claims are queued after this, and it waits for the next drain.
                settle(&mut pending, &mut counting, context, &answers, on_progress)?;
            }
        }
        pending.push(Pending {
            row,
            text: joined,
            spans,
            at: origin,
            fingerprint,
        });
        if pending.len() >= PENDING_CAP {
            drain(model, options.batch_size, &mut window, &mut answers)?;
            settle(&mut pending, &mut counting, context, &answers, on_progress)?;
        }
        Ok(())
    })?;

    drain(model, options.batch_size, &mut window, &mut answers)?;
    settle(&mut pending, &mut counting, context, &answers, on_progress)?;
    let forward_passes = answers.len() as u64;
    Ok((
        counting.finish(app_id, options, model.provenance())?,
        forward_passes,
    ))
}

/// Counts every review whose answers are in, and says how far the walk has got.
fn settle(
    pending: &mut Vec<Pending>,
    counting: &mut Counting,
    context: bool,
    answers: &HashMap<[u8; 32], Reading>,
    on_progress: &mut impl FnMut(ReadProgress),
) -> Result<()> {
    for review in pending.drain(..) {
        counting.count(&review, context, answers)?;
    }
    on_progress(ReadProgress {
        claims_read: answers.len() as u64,
        reviews_counted: counting.reviews,
    });
    Ok(())
}

/// Claims held back before a run of batches, so they can be sorted by length first.
///
/// Every batch pads to its longest member, so a batch holding one long claim and a hundred
/// two-word ones costs as much as a hundred long ones. Sorting a window before cutting it
/// into batches puts claims of a size together, and on a corpus of mostly short claims that
/// is most of the arithmetic. The embedding pass has done this since a million-review game
/// took a day; this pass was missing it.
const LENGTH_WINDOW: usize = 16_384;

/// Asks the model about a window of claims, a batch at a time.
///
/// The batch after next is tokenised while the card works on the one in hand. Tokenising is
/// most of what a reading spends its processor on, and taking turns with the card left both at
/// about half duty; the batches, their order and their answers are the same either way.
fn drain(
    model: &mut ClaimReader,
    batch_size: usize,
    window: &mut Vec<Queued>,
    answers: &mut HashMap<[u8; 32], Reading>,
) -> Result<()> {
    if window.is_empty() {
        return Ok(());
    }
    window.sort_unstable_by_key(|queued| queued.claim.len() + queued.review.len());
    let size = batch_size.max(1);
    let encoder = model.encoder();
    let queued: &[Queued] = window;

    std::thread::scope(|scope| -> Result<()> {
        let (send, receive) = std::sync::mpsc::sync_channel::<Result<Prepared>>(1);
        scope.spawn(move || {
            for chunk in queued.chunks(size) {
                let asked: Vec<Asked<'_>> = chunk.iter().map(Queued::asked).collect();
                // A closed channel is the reader having given up on this window, which is not
                // this thread's error to report.
                if send.send(encoder.prepare(&asked)).is_err() {
                    return;
                }
            }
        });

        for chunk in queued.chunks(size) {
            let prepared = receive
                .recv()
                .map_err(|_| Error::Tokenizer("the batch being tokenised was lost".to_owned()))??;
            for (queued, reading) in chunk.iter().zip(model.run(prepared)?) {
                answers.insert(queued.key, reading);
            }
        }
        Ok(())
    })?;

    window.clear();
    Ok(())
}

/// What one review turned out to be about.
struct Verdict {
    subjects: Vec<usize>,
    primary: Option<usize>,
    praise: Vec<bool>,
    complaint: Vec<bool>,
    claims: usize,
    unclassified: usize,
}

fn judge(
    claims: &[&str],
    review: &[u8; 32],
    context: bool,
    language: &str,
    answers: &HashMap<[u8; 32], Reading>,
) -> Verdict {
    let mut praise = vec![false; SHEET.len()];
    let mut complaint = vec![false; SHEET.len()];
    let mut seen = vec![false; SHEET.len()];
    let mut primary = None;
    let mut best = f32::NEG_INFINITY;
    let mut unclassified = 0;

    for (index, claim) in claims.iter().enumerate() {
        let Some(reading) = answers.get(&key(context, review, index, claim, language)) else {
            unclassified += 1;
            continue;
        };
        let Some(subject) = reading.subject else {
            unclassified += 1;
            continue;
        };
        seen[subject] = true;
        match reading.polarity {
            Polarity::Praise => praise[subject] = true,
            Polarity::Complaint => complaint[subject] = true,
            Polarity::Neutral => {}
        }
        // The review's main subject is whichever claim the model was surest about. A review
        // is most about the thing it says most clearly, not the thing it says first.
        if reading.confidence > best {
            best = reading.confidence;
            primary = Some(subject);
        }
    }

    Verdict {
        subjects: (0..SHEET.len()).filter(|index| seen[*index]).collect(),
        primary,
        praise,
        complaint,
        claims: claims.len(),
        unclassified,
    }
}

/// Everything one walk adds up, and the file of readings it writes as it goes.
struct Counting {
    writer: ArrowWriter<std::fs::File>,
    schema: Arc<Schema>,
    tallies: Vec<Tally>,
    languages: HashMap<String, u64>,
    calendar: HashMap<String, Month>,
    top: crate::bounded::Smallest<std::cmp::Reverse<u64>, Vec<usize>>,
    rows: ReadingRows,
    said: crate::said::Said,
    reviews: u64,
    corpus_reviews: u64,
    claims: u64,
    unclassified: u64,
    silent: u64,
    claimless: u64,
    positive: u64,
}

impl Counting {
    fn new(readings: &Path, top_helpful: usize) -> Result<Self> {
        let schema = reading_schema();
        let writer = ArrowWriter::try_new(
            std::fs::File::create(readings)?,
            Arc::clone(&schema),
            Some(
                WriterProperties::builder()
                    .set_compression(Compression::ZSTD(ZstdLevel::default()))
                    .build(),
            ),
        )?;
        Ok(Self {
            writer,
            schema,
            tallies: vec![Tally::default(); SHEET.len()],
            languages: HashMap::new(),
            calendar: HashMap::new(),
            top: crate::bounded::Smallest::new(top_helpful),
            rows: ReadingRows::default(),
            said: crate::said::Said::new(SHEET.len()),
            reviews: 0,
            corpus_reviews: 0,
            claims: 0,
            unclassified: 0,
            silent: 0,
            claimless: 0,
            positive: 0,
        })
    }

    /// What a review is worth to the corpus whether or not it is in the language being read.
    fn note_corpus(&mut self, row: &crate::capture::Row) {
        self.corpus_reviews += 1;
        *self.languages.entry(row.language.clone()).or_default() += 1;
    }

    fn count(
        &mut self,
        review: &Pending,
        context: bool,
        answers: &HashMap<[u8; 32], Reading>,
    ) -> Result<()> {
        let row = &review.row;
        self.reviews += 1;
        if row.voted_up {
            self.positive += 1;
        }

        let pieces = review.claims();
        let verdict = judge(
            &pieces,
            &review.fingerprint,
            context,
            &row.language,
            answers,
        );
        self.claims += verdict.claims as u64;
        self.unclassified += verdict.unclassified as u64;
        if verdict.subjects.is_empty() {
            self.silent += 1;
        }
        if pieces.is_empty() {
            self.claimless += 1;
        }
        if let Some(primary) = verdict.primary {
            self.tallies[primary].primary_reviews += 1;
        }
        let month = self
            .calendar
            .entry(crate::time::year_month(row.created))
            .or_insert_with(|| Month {
                label: crate::time::year_month(row.created),
                reviews: 0,
                positive: 0,
                subjects: vec![0; SHEET.len()],
            });
        month.reviews += 1;
        if row.voted_up {
            month.positive += 1;
        }
        for &subject in &verdict.subjects {
            month.subjects[subject] += 1;
        }

        for &subject in &verdict.subjects {
            let tally = &mut self.tallies[subject];
            tally.mention_reviews += 1;
            if row.voted_up {
                tally.positive_mentions += 1;
            }
            match (verdict.praise[subject], verdict.complaint[subject]) {
                (true, true) => tally.mixed += 1,
                (true, false) => tally.praised += 1,
                (false, true) => tally.criticised += 1,
                (false, false) => {}
            }
        }
        for (index, claim) in pieces.iter().enumerate() {
            let reading = answers.get(&key(
                context,
                &review.fingerprint,
                index,
                claim,
                &row.language,
            ));
            if let Some(reading) = reading
                && let Some(subject) = reading.subject
            {
                self.tallies[subject].claims += 1;
                self.said.note(subject, reading.polarity, claim);
            }
            self.rows.push(
                &row.recommendationid,
                review.at.get(index).copied().unwrap_or((0, 0)),
                reading,
            );
        }
        self.said.next_review(&row.language);

        // Helpfulness ranks descending, and the bounded keeper takes the smallest key.
        self.top.offer(
            std::cmp::Reverse(row.helpfulness.to_bits()),
            verdict.subjects,
        );

        if self.rows.len() >= 16_384 {
            let batch = self.rows.take(&self.schema)?;
            self.writer.write(&batch)?;
        }
        Ok(())
    }

    fn finish(
        mut self,
        app_id: u32,
        options: &ReadOptions,
        provenance: &crate::reader::Provenance,
    ) -> Result<ReadReport> {
        if self.rows.len() > 0 {
            let batch = self.rows.take(&self.schema)?;
            self.writer.write(&batch)?;
        }
        self.writer.close()?;

        let top_reviews = self.top.take();
        for subjects in &top_reviews {
            for &subject in subjects {
                self.tallies[subject].top_mention_reviews += 1;
            }
        }

        let mut ranked: Vec<(String, u64)> = self.languages.into_iter().collect();
        ranked.sort_by_key(|(name, count)| (std::cmp::Reverse(*count), name.clone()));
        let unread: Vec<(String, u64)> = ranked
            .iter()
            .filter(|(name, _)| provenance.line_for_language(name).is_infinite())
            .cloned()
            .collect();
        // English is the anchor because it is what every other figure in this project is
        // compared against and what 71% of the reference set is written in. A reader with no
        // English line has nothing to anchor to and reports nothing rather than a bar drawn
        // from whichever language happened to sort first.
        let english = provenance.line_for_language("english");
        let strict: Vec<(String, u64)> = if english.is_finite() {
            ranked
                .iter()
                .filter(|(name, _)| {
                    let line = provenance.line_for_language(name);
                    line.is_finite() && line > english
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let mut months: Vec<Month> = self.calendar.into_values().collect();
        months.sort_by(|left, right| left.label.cmp(&right.label));

        Ok(ReadReport {
            app_id,
            reviews: self.reviews,
            corpus_reviews: self.corpus_reviews,
            language: options.language.clone(),
            depth: options.depth,
            batch_size: Some(options.batch_size),
            claims: self.claims,
            forward_passes: 0,
            unclassified_claims: self.unclassified,
            silent_reviews: self.silent,
            claimless_reviews: self.claimless,
            positive: self.positive,
            top_helpful: top_reviews.len() as u64,
            model: String::new(),
            trained_on: String::new(),
            read_with: String::new(),
            reader: String::new(),
            read_by_rule: String::new(),
            usual_declined: None,
            frozen: None,
            context: false,
            threshold: 0.0,
            device: String::new(),
            captured_unix: 0,
            subjects: SHEET
                .iter()
                .zip(&self.tallies)
                .map(|(category, tally)| SubjectCount {
                    id: category.id.to_owned(),
                    label: category.label.to_owned(),
                    mention_reviews: tally.mention_reviews,
                    primary_reviews: tally.primary_reviews,
                    claims: tally.claims,
                    praised: tally.praised,
                    criticised: tally.criticised,
                    mixed: tally.mixed,
                    top_mention_reviews: tally.top_mention_reviews,
                    positive_mentions: tally.positive_mentions,
                })
                .collect(),
            said: self
                .said
                .finish(&SHEET.iter().map(|c| (c.id, c.label)).collect::<Vec<_>>()),
            languages: ranked,
            unread_languages: unread,
            strict_languages: strict,
            months,
            elapsed: Duration::default(),
        })
    }
}

/// Streams every stored reading, one claim at a time, as review id and the span it covers.
///
/// # Errors
///
/// Fails if the file is missing or was written by another build.
pub fn for_each_reading(
    path: &Path,
    mut visit: impl FnMut(&str, (u32, u32), Option<&str>, f32, &str),
) -> Result<()> {
    use arrow::array::{Array, Float32Array, StringArray, UInt32Array};
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let file = std::fs::File::open(path).map_err(|_| crate::Error::NoClassifications {
        path: path.to_path_buf(),
    })?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?
        .with_batch_size(8192)
        .build()?;

    let another_build = |field: &'static str| crate::Error::StaleClassifications {
        path: path.to_path_buf(),
        field,
    };
    for batch in reader {
        let batch = batch?;
        let column = |name: &'static str| -> Result<&dyn Array> {
            batch
                .column_by_name(name)
                .map(AsRef::as_ref)
                .ok_or_else(|| another_build(name))
        };
        let ids = column("recommendationid")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| another_build("recommendationid"))?;
        let starts = column("start")?
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or_else(|| another_build("start"))?;
        let ends = column("end")?
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or_else(|| another_build("end"))?;
        let subjects = column("subject")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| another_build("subject"))?;
        let confidences = column("confidence")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or_else(|| another_build("confidence"))?;
        let polarities = column("polarity")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| another_build("polarity"))?;

        for row in 0..batch.num_rows() {
            visit(
                ids.value(row),
                (starts.value(row), ends.value(row)),
                (!subjects.is_null(row)).then(|| subjects.value(row)),
                confidences.value(row),
                polarities.value(row),
            );
        }
    }
    Ok(())
}

#[derive(Default)]
struct ReadingRows {
    ids: Vec<String>,
    starts: Vec<u32>,
    ends: Vec<u32>,
    subjects: Vec<Option<&'static str>>,
    confidences: Vec<f32>,
    polarities: Vec<&'static str>,
}

impl ReadingRows {
    fn len(&self) -> usize {
        self.ids.len()
    }

    fn push(&mut self, id: &str, at: (u32, u32), reading: Option<&Reading>) {
        self.ids.push(id.to_owned());
        self.starts.push(at.0);
        self.ends.push(at.1);
        self.subjects.push(
            reading
                .and_then(|reading| reading.subject)
                .and_then(|subject| SHEET.get(subject))
                .map(|category| category.id),
        );
        self.confidences
            .push(reading.map_or(0.0, |reading| reading.confidence));
        self.polarities.push(
            reading
                .map_or(Polarity::Neutral, |reading| reading.polarity)
                .as_str(),
        );
    }

    fn take(&mut self, schema: &Arc<Schema>) -> Result<RecordBatch> {
        let mut ids = StringBuilder::new();
        let mut starts = UInt32Builder::new();
        let mut ends = UInt32Builder::new();
        let mut subjects = StringBuilder::new();
        let mut confidences = Float32Builder::new();
        let mut polarities = StringBuilder::new();

        for row in 0..self.len() {
            ids.append_value(&self.ids[row]);
            starts.append_value(self.starts[row]);
            ends.append_value(self.ends[row]);
            subjects.append_option(self.subjects[row]);
            confidences.append_value(self.confidences[row]);
            polarities.append_value(self.polarities[row]);
        }
        self.ids.clear();
        self.starts.clear();
        self.ends.clear();
        self.subjects.clear();
        self.confidences.clear();
        self.polarities.clear();

        let columns: Vec<ArrayRef> = vec![
            Arc::new(ids.finish()),
            Arc::new(starts.finish()),
            Arc::new(ends.finish()),
            Arc::new(subjects.finish()),
            Arc::new(confidences.finish()),
            Arc::new(polarities.finish()),
        ];
        Ok(RecordBatch::try_new(Arc::clone(schema), columns)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_claim_sits_where_the_reader_says_it_sits_in_the_rejoined_review() {
        // The window is cut around a byte offset into the rejoined review, and the offset is
        // arithmetic rather than a search. If it drifts, the model is handed the wrong
        // sentences and every output still looks exactly as plausible as before.
        let reviews = [
            "Looks incredible. Runs like a slideshow. The story is the best in the series.",
            "[h1]Verdict[/h1]\n- great art\n- terrible netcode\n- worth it on sale",
            "10/10 would lose my save again. i.e. it crashes. But the combat! 神作。",
            "   ",
            "One point only",
            "Multibyte: 教学纯靠自己领悟。战斗手感极好。",
            "Mixed\ttabs\nand\r\nnewlines. Second point here.",
        ];

        for text in reviews {
            let claims = Depth::Deep.claims_of(text);
            let review = rejoined(&claims);
            let mut at = 0;
            for claim in &claims {
                assert_eq!(
                    &review[at..at + claim.len()],
                    claim.as_ref(),
                    "claim {claim:?} is not at {at} of {review:?}"
                );
                at += claim.len() + 1;
            }
            assert!(
                claims.is_empty() || at == review.len() + 1,
                "the walk must end exactly one separator past the end of {review:?}"
            );
        }
    }

    /// The walk holds a review by its rejoined text and the spans it queued, so that counting
    /// it later does not split it a second time. Those spans have to give the claims back
    /// exactly, or a review is counted as saying something it never said.
    #[test]
    fn a_held_review_gives_back_the_claims_it_was_queued_as() {
        let reviews = [
            "Looks incredible. Runs like a slideshow. The story is the best in the series.",
            "[h1]Verdict[/h1]\n- great art\n- terrible netcode\n- worth it on sale",
            "Multibyte: 教学纯靠自己领悟。战斗手感极好。",
            "One point only",
            "   ",
        ];

        for text in reviews {
            let claims = Depth::Deep.claims_of(text);
            let mut spans = Vec::new();
            let mut at = 0;
            for claim in &claims {
                spans.push((at, at + claim.len()));
                at += claim.len() + 1;
            }
            let held = Pending {
                at: Vec::new(),
                row: crate::capture::Row {
                    recommendationid: "1".to_owned(),
                    helpfulness: 0.0,
                    votes_up: 0,
                    voted_up: true,
                    language: "english".to_owned(),
                    created: 0,
                },
                text: Arc::from(rejoined(&claims)),
                spans,
                fingerprint: [0; 32],
            };
            let recovered: Vec<&str> = held.claims();
            let expected: Vec<&str> = claims.iter().map(AsRef::as_ref).collect();
            assert_eq!(
                recovered, expected,
                "held review {text:?} came back changed"
            );
        }
    }

    #[test]
    fn the_same_claim_is_one_question_alone_and_one_per_review_in_context() {
        let (short, long) = (review_key("Great game."), review_key("Great game. Buy it."));
        let (a, b) = (
            key(false, &short, 0, "Great game.", "english"),
            key(false, &long, 3, "Great game.", "english"),
        );
        assert_eq!(a, b, "read alone, a repeated claim is asked once");

        let (a, b) = (
            key(true, &short, 0, "Great game.", "english"),
            key(true, &long, 3, "Great game.", "english"),
        );
        assert_ne!(
            a, b,
            "read in context, the same words in two different reviews are two questions"
        );
        assert_eq!(
            key(true, &short, 0, "Great game.", "english"),
            key(
                true,
                &review_key("Great game."),
                0,
                "Great game.",
                "english"
            ),
            "two copies of one review give one window, so they are one question"
        );
        assert_eq!(
            a,
            key(
                true,
                &short,
                0,
                "whatever the splitter now calls it",
                "english"
            ),
            "filed by the review rather than the claim, so both passes agree however it reads"
        );
    }

    #[test]
    fn one_claim_in_two_languages_is_two_questions() {
        let alone = review_key("10/10");
        assert_ne!(
            key(false, &alone, 0, "10/10", "english"),
            key(false, &alone, 0, "10/10", "russian"),
            "the abstention line is drawn per language, so the same two characters clear \
             different bars and a cache keyed on the text alone would answer one with the other"
        );
        assert_ne!(
            key(true, &alone, 0, "10/10", "english"),
            key(true, &alone, 0, "10/10", "russian"),
            "a window read in two languages is two questions for the same reason"
        );
    }

    #[test]
    fn shallow_reads_a_review_as_one_point_and_deep_as_several() {
        let text = "The art is stunning. The story is a mess. Runs fine on a 3070.";
        assert_eq!(Depth::Shallow.claims_of(text).len(), 1);
        assert_eq!(Depth::Deep.claims_of(text).len(), 3);
    }

    #[test]
    fn neither_depth_invents_a_point_from_nothing() {
        assert!(Depth::Shallow.claims_of("   \n  ").is_empty());
        assert!(Depth::Deep.claims_of("   \n  ").is_empty());
    }

    #[test]
    fn a_reading_written_before_depth_existed_reads_back_as_deep() {
        // Every reading on disk before this field was deep, and a missing field must say so
        // rather than fail, or every existing corpus would need re-reading to open.
        let stored = serde_json::json!({
            "app_id": 1, "reviews": 1, "corpus_reviews": 1, "language": null, "claims": 1,
            "unclassified_claims": 0, "silent_reviews": 0, "positive": 1, "top_helpful": 1,
            "model": "m", "threshold": 0.5, "device": "cpu",
            "subjects": [], "languages": [], "months": []
        });
        let found: ReadReport = serde_json::from_value(stored).expect("an older reading opens");
        assert_eq!(found.depth, Depth::Deep);
        assert!(found.trained_on.is_empty());
    }

    #[test]
    fn a_corpus_declined_far_above_usual_is_a_corpus_about_something_missing() {
        let mut found = ReadReport {
            app_id: 1,
            reviews: 100,
            corpus_reviews: 100,
            language: None,
            depth: Depth::Deep,
            batch_size: None,
            claims: 1_000,
            forward_passes: 1_000,
            unclassified_claims: 900,
            silent_reviews: 0,
            claimless_reviews: 0,
            positive: 50,
            top_helpful: 10,
            model: String::new(),
            trained_on: String::new(),
            read_with: String::new(),
            reader: String::new(),
            read_by_rule: String::new(),
            usual_declined: Some(0.73),
            frozen: None,
            context: false,
            threshold: 0.5,
            device: String::new(),
            captured_unix: 0,
            subjects: Vec::new(),
            said: Vec::new(),
            languages: Vec::new(),
            unread_languages: Vec::new(),
            strict_languages: Vec::new(),
            months: Vec::new(),
            elapsed: Duration::ZERO,
        };
        let ratio = found
            .declined_against_usual()
            .expect("a usual figure is carried");
        assert!(
            ratio >= UNUSUALLY_DECLINED,
            "90% against a usual 73% is {ratio}"
        );

        assert!(found.declined_unusually(), "and seventeen points above it");

        found.unclassified_claims = 700;
        assert!(found.declined_against_usual().unwrap() < UNUSUALLY_DECLINED);

        // A reader that declines little is above its usual rate by proportion long before the
        // difference is worth telling anybody about: three points is one hard game, not a
        // corpus about something the taxonomy cannot name.
        found.usual_declined = Some(0.158);
        found.unclassified_claims = 190;
        assert!(found.declined_against_usual().unwrap() >= UNUSUALLY_DECLINED);
        assert!(!found.declined_unusually(), "three points is not a finding");
        found.unclassified_claims = 260;
        assert!(found.declined_unusually(), "ten points is");

        // A reader exported before the figure existed cannot make the comparison, and says
        // nothing rather than comparing against zero.
        found.usual_declined = None;
        assert_eq!(found.declined_against_usual(), None);
        assert!(!found.declined_unusually());
    }

    #[test]
    fn the_depth_a_reading_was_made_at_travels_with_it() {
        let stored = serde_json::json!({
            "app_id": 1, "reviews": 1, "corpus_reviews": 1, "language": null, "claims": 1,
            "depth": "shallow",
            "unclassified_claims": 0, "silent_reviews": 0, "positive": 1, "top_helpful": 1,
            "model": "m", "threshold": 0.5, "device": "cpu",
            "subjects": [], "languages": [], "months": []
        });
        let found: ReadReport = serde_json::from_value(stored).expect("a shallow reading opens");
        assert_eq!(found.depth, Depth::Shallow);
    }

    /// A readings file that names its claims some other way cannot be joined to anything, and
    /// the refusal has to say which file and what to do, not that a review payload is short of
    /// a field, which sends a reader to the crawler.
    #[test]
    fn readings_keyed_by_another_build_are_refused_by_name() {
        let dir =
            std::env::temp_dir().join(format!("steamgauge-old-readings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("readings.parquet");

        let schema = Arc::new(Schema::new(vec![
            Field::new("recommendationid", DataType::Utf8, false),
            Field::new("claim_index", DataType::UInt32, false),
            Field::new("subject", DataType::Utf8, true),
            Field::new("confidence", DataType::Float32, false),
            Field::new("polarity", DataType::Utf8, false),
        ]));
        let mut ids = StringBuilder::new();
        ids.append_value("1");
        let mut indexes = UInt32Builder::new();
        indexes.append_value(0);
        let mut subjects = StringBuilder::new();
        subjects.append_value("gameplay");
        let mut confidences = Float32Builder::new();
        confidences.append_value(0.9);
        let mut polarities = StringBuilder::new();
        polarities.append_value("praise");
        let columns: Vec<ArrayRef> = vec![
            Arc::new(ids.finish()),
            Arc::new(indexes.finish()),
            Arc::new(subjects.finish()),
            Arc::new(confidences.finish()),
            Arc::new(polarities.finish()),
        ];
        let batch = RecordBatch::try_new(Arc::clone(&schema), columns).unwrap();
        let mut writer =
            ArrowWriter::try_new(std::fs::File::create(&path).unwrap(), schema, None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();

        let refused = for_each_reading(&path, |_, _, _, _, _| {})
            .expect_err("readings with no span were streamed as though they had one");
        let why = refused.to_string();
        assert!(
            why.contains("another build")
                && why.contains("`start`")
                && why.contains("steamgauge read"),
            "the refusal does not say what the file is or what to do: {why}"
        );
        assert!(
            why.contains(&path.display().to_string()),
            "the refusal does not name the file: {why}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
