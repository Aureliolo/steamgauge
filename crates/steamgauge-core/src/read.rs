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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
    reader::{Asked, ClaimReader, Polarity, Provenance, Reading},
    taxonomy::SHEET,
};

/// What a reading asks of a model: the half that tokenises, which runs on a thread of its own
/// beside the card, and the half that runs on the card.
///
/// The trained reader is the only one that ships. A model that answers from a table stands in
/// for it in the tests, which is how the walk, the batching and the counting are held to what
/// they do without a model on disk.
pub(crate) trait Model {
    type Encoder: Send + Sync;
    type Prepared: Send;

    fn encoder(&self) -> Arc<Self::Encoder>;
    fn prepare(encoder: &Self::Encoder, asked: &[Asked<'_>]) -> Result<Self::Prepared>;
    fn run(&mut self, prepared: Self::Prepared) -> Result<Vec<Reading>>;
    fn provenance(&self) -> &Provenance;
    fn device(&self) -> &str;
}

impl Model for ClaimReader {
    type Encoder = crate::reader::Encoder;
    type Prepared = crate::reader::Prepared;

    fn encoder(&self) -> Arc<Self::Encoder> {
        Self::encoder(self)
    }

    fn prepare(encoder: &Self::Encoder, asked: &[Asked<'_>]) -> Result<Self::Prepared> {
        encoder.prepare(asked)
    }

    fn run(&mut self, prepared: Self::Prepared) -> Result<Vec<Reading>> {
        Self::run(self, prepared)
    }

    fn provenance(&self) -> &Provenance {
        Self::provenance(self)
    }

    fn device(&self) -> &str {
        Self::device(self)
    }
}

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
    /// The share of the card's time the read may take, for a card other work is using: after
    /// each batch it rests as long again as the batch kept the card busy, times
    /// (1 - share) / share. What it reads is the same; only how long it takes changes.
    pub card_share: f64,
    /// Set by somebody who wants the read to end now. The reading already on disk is left as
    /// it was, because the new one is written beside it and only takes its place on finishing.
    pub stop: Arc<AtomicBool>,
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
            card_share: 1.0,
            stop: Arc::default(),
        }
    }
}

/// How far a reading has got. One walk does both jobs, so both numbers move together and a
/// watcher never sees the corpus start again from nothing.
#[derive(Debug, Clone, Copy)]
pub struct ReadProgress {
    pub claims_read: u64,
    pub reviews_counted: u64,
    /// Reviews of the capture walked so far, in any language. The capture's size is known
    /// before the walk starts, so this is the one figure a share of the way through can be
    /// taken of: how many claims a corpus holds is only known once it has been walked.
    pub reviews_walked: u64,
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
    /// Reviews praising each subject, in taxonomy order, whether or not they also complain
    /// about it. Empty on a reading counted before months carried sides.
    #[serde(default)]
    pub praising: Vec<u64>,
    /// Reviews complaining about each subject, in taxonomy order, whether or not they also
    /// praise it. Empty on a reading counted before months carried sides.
    #[serde(default)]
    pub complaining: Vec<u64>,
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
    /// Every subject counted again for each kind of reviewer, in the order of
    /// [`crate::who::SEGMENTS`]. Empty on a reading counted before reviewers were told apart.
    #[serde(default)]
    pub who: Vec<crate::who::SegmentCount>,
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
        // Every other subject the claim covers, as "audio:praise,controls:complaint"; null for
        // none, and absent from readings written by a reader that named one subject per claim,
        // which read as none.
        Field::new("also", DataType::Utf8, true),
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
    on_progress: impl FnMut(ReadProgress),
) -> Result<ReadReport> {
    read_with(model, app_id, options, on_progress)
}

fn read_with(
    model: &mut impl Model,
    app_id: u32,
    options: &ReadOptions,
    mut on_progress: impl FnMut(ReadProgress),
) -> Result<ReadReport> {
    let started = Instant::now();
    let snapshot = crate::embed::latest_snapshot(&options.out_dir, app_id)?;

    let context = model.provenance().context;
    // A reader told whether a game is played in a headset has to be told for every game: read
    // without the fact, a headset game's claims are asked as a screen game's, and every answer
    // still looks like one.
    let headset_only = if model.provenance().headset_marker {
        crate::facts::Facts::load(&options.out_dir.join(format!("appid={app_id}")))
            .ok_or_else(|| {
                crate::Error::Refused(format!(
                    "app {app_id}: this reader is told whether a game is played only in a \
                     headset, and nobody has asked the store about this one; \
                     `steamgauge store-facts {app_id}` does"
                ))
            })?
            .headset_only
    } else {
        false
    };
    // Written beside the reading it replaces and moved over it only once complete, so a read
    // that is stopped, fails or dies with the machine leaves the last one standing.
    let partial = snapshot.join("readings.partial.parquet");
    let (counted, forward_passes) = match read_and_count(
        model,
        app_id,
        (&snapshot, &partial),
        options,
        (context, headset_only),
        &mut on_progress,
    ) {
        Ok(read) => {
            std::fs::rename(&partial, snapshot.join("readings.parquet"))?;
            read
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            return Err(error);
        }
    };
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
/// differently from the build that read it, so the answers no longer name its claims; and
/// stops, leaving the reading as it was, once `stop` is set.
pub fn recount_corpus(
    out_dir: &Path,
    app_id: u32,
    top_helpful: usize,
    provenance: &crate::reader::Provenance,
    stop: &AtomicBool,
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
        card_share: 1.0,
        stop: Arc::default(),
    };
    let context = earlier.context;

    let position = |name: &str| SHEET.iter().position(|category| category.id == name);
    let mut stored: Stored = HashMap::new();
    for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, subject, confidence, polarity, also| {
            let reading = Reading {
                subject: subject.and_then(position),
                confidence,
                polarity: Polarity::from_name(polarity),
                also,
            };
            stored.entry(id.to_owned()).or_default().push((at, reading));
        },
    )?;

    let replay = snapshot.join("readings.recount.parquet");
    let counted = recount_rows(
        &snapshot,
        &replay,
        (&options, stop),
        context,
        &stored,
        (RECOUNT_TELLS_EVERY, &mut on_progress),
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

/// Reviews a recount counts between two reports of how far it has got.
const RECOUNT_TELLS_EVERY: u64 = 25_000;

/// Walks the capture once, handing every review its stored answers, and returns the counting
/// before it is finished, so the walk's totals can be checked against the reading's. Says how
/// far it has got every `every` reviews.
fn recount_rows(
    snapshot: &Path,
    replay: &Path,
    (options, stop): (&ReadOptions, &AtomicBool),
    context: bool,
    stored: &Stored,
    (every, on_progress): (u64, &mut impl FnMut(ReadProgress)),
) -> Result<Counting> {
    let mut counting = Counting::new(replay, options.top_helpful)?;
    let mut answers: HashMap<[u8; 32], Reading> = HashMap::new();
    let mut claims_seen: u64 = 0;
    crate::capture::for_each_row(snapshot, |row, text| {
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Stopped);
        }
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
        if counting.reviews % every == 0 {
            on_progress(ReadProgress {
                claims_read: claims_seen,
                reviews_counted: counting.reviews,
                reviews_walked: counting.corpus_reviews,
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
    headset_only: bool,
}

impl Queued {
    fn asked(&self) -> Asked<'_> {
        Asked {
            claim: &self.claim,
            review: &self.review,
            at: self.at,
            language: &self.language,
            headset_only: self.headset_only,
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
    model: &mut impl Model,
    app_id: u32,
    (snapshot, readings): (&Path, &Path),
    options: &ReadOptions,
    (context, headset_only): (bool, bool),
    on_progress: &mut impl FnMut(ReadProgress),
) -> Result<(ReadReport, u64)> {
    let mut answers: HashMap<[u8; 32], Reading> = HashMap::new();
    let mut window: Vec<Queued> = Vec::with_capacity(LENGTH_WINDOW);
    let mut pending: Vec<Pending> = Vec::new();
    let mut counting = Counting::new(readings, options.top_helpful)?;
    // One allocation for every review a model that reads claims alone will ever queue.
    let nothing: Arc<str> = Arc::from("");

    crate::capture::for_each_row(snapshot, |row, text| {
        if options.stop.load(Ordering::Relaxed) {
            return Err(Error::Stopped);
        }
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
                headset_only,
            });
            if window.len() >= LENGTH_WINDOW {
                drain(model, options, &mut window, &mut answers)?;
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
            drain(model, options, &mut window, &mut answers)?;
            settle(&mut pending, &mut counting, context, &answers, on_progress)?;
        }
        Ok(())
    })?;

    drain(model, options, &mut window, &mut answers)?;
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
        reviews_walked: counting.corpus_reviews,
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
fn drain<M: Model>(
    model: &mut M,
    options: &ReadOptions,
    window: &mut Vec<Queued>,
    answers: &mut HashMap<[u8; 32], Reading>,
) -> Result<()> {
    if window.is_empty() {
        return Ok(());
    }
    window.sort_unstable_by_key(|queued| queued.claim.len() + queued.review.len());
    let size = options.batch_size.max(1);
    let encoder = model.encoder();
    let queued: &[Queued] = window;

    std::thread::scope(|scope| -> Result<()> {
        let (send, receive) = std::sync::mpsc::sync_channel::<Result<M::Prepared>>(1);
        scope.spawn(move || {
            for chunk in queued.chunks(size) {
                let asked: Vec<Asked<'_>> = chunk.iter().map(Queued::asked).collect();
                // A closed channel is the reader having given up on this window, which is not
                // this thread's error to report.
                if send.send(M::prepare(&encoder, &asked)).is_err() {
                    return;
                }
            }
        });

        for chunk in queued.chunks(size) {
            let prepared = receive
                .recv()
                .map_err(|_| Error::Tokenizer("the batch being tokenised was lost".to_owned()))??;
            // A run returns once the card has answered, so its time is the card's time.
            let started = std::time::Instant::now();
            let readings = model.run(prepared)?;
            std::thread::sleep(rest_for(started.elapsed(), options.card_share));
            for (queued, reading) in chunk.iter().zip(readings) {
                answers.insert(queued.key, reading);
            }
        }
        Ok(())
    })?;

    window.clear();
    Ok(())
}

/// How long to leave the card idle after keeping it busy this long, for a read to take this
/// share of its time. The card cannot be asked to favour anyone else, so a read that shares it
/// does so by resting.
pub(crate) fn rest_for(busy: std::time::Duration, share: f64) -> std::time::Duration {
    if share >= 1.0 || share <= 0.0 {
        return std::time::Duration::ZERO;
    }
    busy.mul_f64((1.0 - share) / share)
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

    let mut note = |subject: usize, polarity: Polarity| {
        seen[subject] = true;
        match polarity {
            Polarity::Praise => praise[subject] = true,
            Polarity::Complaint => complaint[subject] = true,
            Polarity::Neutral => {}
        }
    };
    for (index, claim) in claims.iter().enumerate() {
        let Some(reading) = answers.get(&key(context, review, index, claim, language)) else {
            unclassified += 1;
            continue;
        };
        // A subject named beside another cleared its own line, so it counts whether or not the
        // one the claim is chiefly about cleared its own.
        for (subject, polarity) in reading.also.iter() {
            note(subject, polarity);
        }
        let Some(subject) = reading.subject else {
            unclassified += 1;
            continue;
        };
        note(subject, reading.polarity);
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
    who: crate::who::Tally,
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
            who: crate::who::Tally::new(),
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

    /// A review added to the month it was written in and to every kind of reviewer its writer
    /// is, which the claims it makes are then counted under too.
    fn by_month_and_writer(
        &mut self,
        row: &crate::capture::Row,
        verdict: &Verdict,
    ) -> crate::who::Membership {
        let label = crate::time::year_month(row.created);
        let member = crate::who::Membership::of(&row.reviewer);
        self.who.review(
            member,
            &crate::who::Reviewed {
                recommended: row.voted_up,
                month: &label,
                claims: verdict.claims as u64,
                subjects: &verdict.subjects,
                praise: &verdict.praise,
                complaint: &verdict.complaint,
            },
        );
        let month = self.calendar.entry(label.clone()).or_insert_with(|| Month {
            label,
            reviews: 0,
            positive: 0,
            subjects: vec![0; SHEET.len()],
            praising: vec![0; SHEET.len()],
            complaining: vec![0; SHEET.len()],
        });
        month.reviews += 1;
        if row.voted_up {
            month.positive += 1;
        }
        for &subject in &verdict.subjects {
            month.subjects[subject] += 1;
            month.praising[subject] += u64::from(verdict.praise[subject]);
            month.complaining[subject] += u64::from(verdict.complaint[subject]);
        }
        member
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
        let member = self.by_month_and_writer(row, &verdict);

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
            if let Some(reading) = reading {
                for (subject, polarity) in reading
                    .subject
                    .map(|subject| (subject, reading.polarity))
                    .into_iter()
                    .chain(reading.also.iter())
                {
                    self.tallies[subject].claims += 1;
                    self.who.claim(member, subject);
                    self.said.note(subject, polarity, claim);
                }
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
        // ArrowWriter writes nothing for a batch of no rows.
        let batch = self.rows.take(&self.schema)?;
        self.writer.write(&batch)?;
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
            who: self.who.finish(),
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
    for_each_full_reading(path, |id, at, subject, confidence, polarity, _| {
        visit(id, at, subject, confidence, polarity);
    })
}

/// What a stored reading says about one subject: the polarity it takes on it where it names it,
/// first or beside another, and None where it does not name it at all.
#[must_use]
pub fn polarity_on(
    subject: &str,
    first: Option<&str>,
    polarity: &str,
    also: crate::reader::Also,
) -> Option<&'static str> {
    if first == Some(subject) {
        return Some(Polarity::from_name(polarity).as_str());
    }
    let at = SHEET.iter().position(|row| row.id == subject)?;
    also.iter()
        .find(|(other, _)| *other == at)
        .map(|(_, said)| said.as_str())
}

/// [`for_each_reading`], with every other subject the claim covers as a stored reading wrote
/// it, for the passes that count them.
///
/// Readings written by a reader that named one subject per claim carry no such column, and
/// read as covering nothing else, which is what that reader said.
///
/// # Errors
///
/// Fails if the file is missing or was written by another build.
pub fn for_each_full_reading(
    path: &Path,
    mut visit: impl FnMut(&str, (u32, u32), Option<&str>, f32, &str, crate::reader::Also),
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
        let also = match batch.column_by_name("also") {
            Some(found) => Some(
                found
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| another_build("also"))?,
            ),
            None => None,
        };

        for row in 0..batch.num_rows() {
            visit(
                ids.value(row),
                (starts.value(row), ends.value(row)),
                (!subjects.is_null(row)).then(|| subjects.value(row)),
                confidences.value(row),
                polarities.value(row),
                also.filter(|column| !column.is_null(row))
                    .map(|column| crate::reader::Also::from_text(column.value(row)))
                    .unwrap_or_default(),
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
    also: Vec<Option<String>>,
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
        self.also
            .push(reading.and_then(|reading| reading.also.to_text()));
    }

    fn take(&mut self, schema: &Arc<Schema>) -> Result<RecordBatch> {
        let mut ids = StringBuilder::new();
        let mut starts = UInt32Builder::new();
        let mut ends = UInt32Builder::new();
        let mut subjects = StringBuilder::new();
        let mut confidences = Float32Builder::new();
        let mut polarities = StringBuilder::new();
        let mut also = StringBuilder::new();

        for row in 0..self.len() {
            ids.append_value(&self.ids[row]);
            starts.append_value(self.starts[row]);
            ends.append_value(self.ends[row]);
            subjects.append_option(self.subjects[row]);
            confidences.append_value(self.confidences[row]);
            polarities.append_value(self.polarities[row]);
            also.append_option(self.also[row].as_deref());
        }
        self.ids.clear();
        self.starts.clear();
        self.ends.clear();
        self.subjects.clear();
        self.confidences.clear();
        self.polarities.clear();
        self.also.clear();

        let columns: Vec<ArrayRef> = vec![
            Arc::new(ids.finish()),
            Arc::new(starts.finish()),
            Arc::new(ends.finish()),
            Arc::new(subjects.finish()),
            Arc::new(confidences.finish()),
            Arc::new(polarities.finish()),
            Arc::new(also.finish()),
        ];
        Ok(RecordBatch::try_new(Arc::clone(schema), columns)?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A model that answers from what a claim says. "bug-free" is praise of the bugs, "bug" a
    /// complaint about them, "music" praise of the audio, and "story" praise of the story with
    /// a complaint about the controls beside it. "great" is praise of the audio in a review
    /// that mentions music, which only a model reading in context can see. Anything else is
    /// declined.
    pub(crate) struct Table {
        provenance: Provenance,
        /// Batches run on the card, and claims asked in them.
        pub(crate) runs: usize,
        pub(crate) asked: usize,
        /// How long each question was, its claim and the review it was read in, in the order
        /// they were asked.
        pub(crate) lengths: Vec<usize>,
    }

    impl Table {
        pub(crate) fn new(context: bool) -> Self {
            Self {
                provenance: serde_json::from_value(serde_json::json!({
                    "subjects": crate::taxonomy::categories(),
                    "threshold": 0.5,
                    "max_tokens": 128,
                    "context": context,
                    "trained_from": "a table",
                    "data_fingerprint": "labels-1",
                    "run_id": "run-1",
                    "name": "Table Reader",
                    "lines_fingerprint": "lines-1",
                    "usual_declined": 0.25,
                    "frozen": {"games": 3, "claims": 300, "coverage": 0.8, "accuracy": 0.9,
                               "macro_f1": 0.7},
                }))
                .unwrap(),
                runs: 0,
                asked: 0,
                lengths: Vec::new(),
            }
        }

        fn answer(asked: &Asked<'_>) -> Reading {
            let at = |id: &str| SHEET.iter().position(|row| row.id == id);
            let said = |subject: &str, polarity, confidence| Reading {
                subject: at(subject),
                confidence,
                polarity,
                also: crate::reader::Also::default(),
            };
            let claim = asked.claim.to_lowercase();
            if claim.contains("bug-free") {
                said("bugs", Polarity::Praise, 0.7)
            } else if claim.contains("bug") {
                said("bugs", Polarity::Complaint, 0.9)
            } else if claim.contains("music") {
                said("audio", Polarity::Praise, 0.8)
            } else if claim.contains("story") {
                let mut reading = said("story", Polarity::Praise, 0.95);
                reading
                    .also
                    .insert(at("controls").unwrap(), Polarity::Complaint);
                reading
            } else if claim.contains("great") && asked.review.to_lowercase().contains("music") {
                said("audio", Polarity::Praise, 0.6)
            } else {
                Reading {
                    confidence: 0.3,
                    ..Reading::default()
                }
            }
        }
    }

    impl Model for Table {
        type Encoder = ();
        type Prepared = Vec<(usize, Reading)>;

        fn encoder(&self) -> Arc<()> {
            Arc::new(())
        }

        fn prepare((): &(), asked: &[Asked<'_>]) -> Result<Vec<(usize, Reading)>> {
            Ok(asked
                .iter()
                .map(|one| (one.claim.len() + one.review.len(), Self::answer(one)))
                .collect())
        }

        fn run(&mut self, prepared: Vec<(usize, Reading)>) -> Result<Vec<Reading>> {
            self.runs += 1;
            self.asked += prepared.len();
            Ok(prepared
                .into_iter()
                .map(|(length, reading)| {
                    self.lengths.push(length);
                    reading
                })
                .collect())
        }

        fn provenance(&self) -> &Provenance {
            &self.provenance
        }

        fn device(&self) -> &'static str {
            "table"
        }
    }

    /// When the capture was crawled and when it was last swept.
    pub(crate) const CRAWLED: i64 = 1_700_000_000;
    pub(crate) const SWEPT: i64 = 1_712_000_000;

    /// Seven reviews of app 1 under `out`, with the crawl's record beside them: "Bugs
    /// everywhere." three times, a review that is only punctuation, one in Chinese, and one
    /// that both damns and praises the bugs.
    pub(crate) fn corpus(out: &Path) -> PathBuf {
        corpus_of(out, 1)
    }

    /// [`corpus`] read by the table at the default options, with its reading saved beside it.
    pub(crate) fn read_corpus_of(out: &Path, app_id: u32) -> PathBuf {
        let snapshot = corpus_of(out, app_id);
        let options = options(out);
        read_with(&mut Table::new(false), app_id, &options, |_| {})
            .unwrap()
            .save(&snapshot.join("reading.json"))
            .unwrap();
        snapshot
    }

    pub(crate) fn corpus_of(out: &Path, app_id: u32) -> PathBuf {
        // (id, review, language, recommends, day of 2024 written, helpfulness)
        let reviews = [
            (
                "1",
                "Bugs everywhere. The music is lovely.",
                "english",
                false,
                "2024-03-15",
                0.9,
            ),
            (
                "2",
                "The story is gripping. It is great.",
                "english",
                true,
                "2024-03-20",
                0.5,
            ),
            ("3", "Bugs everywhere.", "english", true, "2024-04-01", 0.1),
            ("4", "\u{597D}\u{73A9}", "schinese", true, "2024-04-02", 0.2),
            ("5", "...", "english", false, "2024-04-03", 0.0),
            (
                "6",
                "Bugs everywhere. Mostly bug-free now.",
                "english",
                true,
                "2024-04-04",
                0.3,
            ),
            (
                "7",
                "The music swells. It is great.",
                "english",
                true,
                "2024-04-05",
                0.4,
            ),
        ];
        let snapshot = out
            .join(format!("appid={app_id}"))
            .join(format!("snapshot={CRAWLED}"));
        let rows: Vec<serde_json::Value> = reviews
            .iter()
            .map(|(id, text, language, recommends, day, helpful)| {
                let (month, date) = (&day[5..7], &day[8..10]);
                // Midday on the day, counted from the first of March 2024.
                let days = if month == "03" { 0 } else { 31 } + date.parse::<i64>().unwrap() - 1;
                serde_json::json!({
                    "recommendationid": id, "review": text, "language": language,
                    "voted_up": recommends, "weighted_vote_score": helpful.to_string(),
                    // The two most helpful drew as many votes as each other.
                    "votes_up": match *id { "1" | "2" => 50, "4" => 20, "6" => 30, "7" => 40,
                                            _ => 10 },
                    "timestamp_created": 1_709_294_400 + days * 86_400,
                    // The first reviewer played an hour and a half, on a Deck; the rest fifty.
                    "author": {"steamid": format!("7656{id}"),
                               "playtime_at_review": if *id == "1" { 90 } else { 3_000 }},
                    "primarily_steam_deck": *id == "1",
                })
            })
            .collect();
        let mut writer =
            crate::capture::CaptureWriter::create(&snapshot.join("shard-0000.parquet"), app_id)
                .unwrap();
        writer.write(&rows.iter().collect::<Vec<_>>()).unwrap();
        writer.close().unwrap();
        std::fs::write(
            snapshot.join("crawl.json"),
            serde_json::json!({
                "app_id": app_id, "name": "Test Game", "review_score_desc": "Mixed",
                "rows_unique": 7, "valve_total_reviews": 7, "valve_total_positive": 5,
                "valve_total_negative": 2, "coverage": 1.0, "snapshot_unix": CRAWLED,
                "shards": 1, "swept_unix": SWEPT,
            })
            .to_string(),
        )
        .unwrap();
        snapshot
    }

    /// These reviews of `app_id` under `out`, as Steam's API serves them, read by the table at the
    /// default options with the reading saved beside them.
    pub(crate) fn read_these(out: &Path, app_id: u32, reviews: &[serde_json::Value]) -> PathBuf {
        let snapshot = out
            .join(format!("appid={app_id}"))
            .join(format!("snapshot={CRAWLED}"));
        let mut writer =
            crate::capture::CaptureWriter::create(&snapshot.join("shard-0000.parquet"), app_id)
                .unwrap();
        writer.write(&reviews.iter().collect::<Vec<_>>()).unwrap();
        writer.close().unwrap();
        std::fs::write(
            snapshot.join("crawl.json"),
            serde_json::json!({
                "app_id": app_id, "name": "Test Game", "review_score_desc": "Mixed",
                "rows_unique": reviews.len(), "valve_total_reviews": reviews.len(),
                "coverage": 1.0, "snapshot_unix": CRAWLED, "shards": 1,
            })
            .to_string(),
        )
        .unwrap();
        read_with(&mut Table::new(false), app_id, &options(out), |_| {})
            .unwrap()
            .save(&snapshot.join("reading.json"))
            .unwrap();
        snapshot
    }

    pub(crate) fn options(out: &Path) -> ReadOptions {
        ReadOptions {
            out_dir: out.to_path_buf(),
            top_helpful: 2,
            ..ReadOptions::default()
        }
    }

    /// Each subject anybody raised, as (mentions, primary, claims, praised, criticised, mixed,
    /// top of the pile, recommending).
    fn tallies(report: &ReadReport) -> Vec<(&str, [u64; 8])> {
        report
            .subjects
            .iter()
            .filter(|subject| subject.mention_reviews > 0)
            .map(|subject| {
                (
                    subject.id.as_str(),
                    [
                        subject.mention_reviews,
                        subject.primary_reviews,
                        subject.claims,
                        subject.praised,
                        subject.criticised,
                        subject.mixed,
                        subject.top_mention_reviews,
                        subject.positive_mentions,
                    ],
                )
            })
            .collect()
    }

    #[test]
    fn a_corpus_is_read_once_per_distinct_claim_and_every_review_counted() {
        let out = crate::tempdir::Dir::new();
        corpus(out.path());
        let mut model = Table::new(false);
        let mut told = Vec::new();
        let report = read_with(&mut model, 1, &options(out.path()), |progress| {
            told.push((
                progress.claims_read,
                progress.reviews_counted,
                progress.reviews_walked,
            ));
        })
        .unwrap();

        assert_eq!(
            (report.reviews, report.corpus_reviews, report.positive),
            (7, 7, 5)
        );
        assert_eq!(
            (
                report.claims,
                report.unclassified_claims,
                report.silent_reviews,
                report.claimless_reviews
            ),
            (10, 3, 2, 1)
        );
        assert_eq!(
            report.forward_passes, 7,
            "\"Bugs everywhere.\" is one question however many reviews say it"
        );
        assert_eq!((model.runs, model.asked), (1, 7), "asked in one batch");
        assert_eq!(told, [(7, 7, 7)]);
        assert_eq!(report.top_helpful, 2);
        assert_eq!(
            tallies(&report),
            [
                ("bugs", [3, 3, 4, 0, 2, 1, 1, 2]),
                ("story", [1, 1, 1, 1, 0, 0, 1, 1]),
                ("audio", [2, 1, 2, 2, 0, 0, 1, 1]),
                ("controls", [1, 0, 1, 0, 1, 0, 1, 1]),
            ]
        );
        assert_eq!(
            report.languages,
            [("english".to_owned(), 6), ("schinese".to_owned(), 1)]
        );
        let months: Vec<(&str, u64, u64)> = report
            .months
            .iter()
            .map(|month| (month.label.as_str(), month.reviews, month.positive))
            .collect();
        assert_eq!(months, [("2024-03", 2, 1), ("2024-04", 5, 4)]);
        let bugs = SHEET.iter().position(|row| row.id == "bugs").unwrap();
        let april = &report.months[1];
        assert_eq!(
            (
                april.subjects[bugs],
                april.praising[bugs],
                april.complaining[bugs]
            ),
            (2, 1, 2)
        );
    }

    #[test]
    fn a_reading_carries_what_read_it_and_a_row_for_every_claim() {
        let out = crate::tempdir::Dir::new();
        let snapshot = corpus(out.path());
        let report = read_with(&mut Table::new(false), 1, &options(out.path()), |_| {}).unwrap();

        assert_eq!(report.device, "table");
        assert_eq!(
            (
                report.model.as_str(),
                report.trained_on.as_str(),
                report.read_with.as_str(),
                report.reader.as_str(),
                report.read_by_rule.as_str()
            ),
            ("a table", "labels-1", "run-1", "Table Reader", "lines-1")
        );
        assert!((report.threshold - 0.5).abs() < f32::EPSILON);
        assert_eq!(report.usual_declined, Some(0.25));
        assert_eq!(report.frozen.map(|frozen| frozen.games), Some(3));
        assert!(!report.context);
        assert_eq!(
            report.captured_unix, SWEPT,
            "the capture last changed when it was swept"
        );
        assert!(report.elapsed > Duration::ZERO);

        // A row per claim, the declined ones too, and every subject a claim names.
        let mut rows = Vec::new();
        for_each_full_reading(
            &snapshot.join("readings.parquet"),
            |id, at, subject, _, polarity, also| {
                rows.push((
                    id.to_owned(),
                    at,
                    subject.map(str::to_owned),
                    polarity.to_owned(),
                    also.to_text(),
                ));
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 10);
        assert!(rows.contains(&(
            "2".to_owned(),
            (0, 22),
            Some("story".to_owned()),
            "praise".to_owned(),
            Some("controls:complaint".to_owned())
        )));
        assert!(rows.contains(&("2".to_owned(), (23, 35), None, "neutral".to_owned(), None)));
        assert!(!snapshot.join("readings.partial.parquet").exists());
    }

    #[test]
    fn a_read_of_one_language_counts_only_it_and_still_walks_the_whole_capture() {
        let out = crate::tempdir::Dir::new();
        corpus(out.path());
        let options = ReadOptions {
            language: Some("english".to_owned()),
            ..options(out.path())
        };
        let mut model = Table::new(false);
        let report = read_with(&mut model, 1, &options, |_| {}).unwrap();
        assert_eq!((report.reviews, report.corpus_reviews), (6, 7));
        assert_eq!((report.claims, report.unclassified_claims), (9, 2));
        assert_eq!(model.asked, 6, "the Chinese review is never asked about");
    }

    #[test]
    fn claims_are_asked_a_batch_at_a_time() {
        let out = crate::tempdir::Dir::new();
        corpus(out.path());
        let options = ReadOptions {
            batch_size: 2,
            ..options(out.path())
        };
        let mut model = Table::new(false);
        read_with(&mut model, 1, &options, |_| {}).unwrap();
        assert_eq!((model.runs, model.asked), (4, 7));
        assert!(
            model.lengths.is_sorted(),
            "shortest first, so a batch pads to a length near its own: {:?}",
            model.lengths
        );
    }

    #[test]
    fn in_context_a_claim_is_a_question_about_its_own_review() {
        let out = crate::tempdir::Dir::new();
        corpus(out.path());
        let mut model = Table::new(true);
        let report = read_with(&mut model, 1, &options(out.path()), |_| {}).unwrap();
        assert!(report.context);
        // Every claim of every review is its own question now, "Bugs everywhere." included.
        assert_eq!(report.forward_passes, 10);
        // "It is great." beside the music is about the audio; beside the story it is not.
        assert_eq!((report.claims, report.unclassified_claims), (10, 2));
        let audio = report.subjects.iter().find(|s| s.id == "audio").unwrap();
        assert_eq!((audio.mention_reviews, audio.claims), (2, 3));
    }

    #[test]
    fn a_recount_of_a_readings_own_answers_gives_the_counts_it_gave() {
        let out = crate::tempdir::Dir::new();
        let snapshot = corpus(out.path());
        for context in [false, true] {
            let mut model = Table::new(context);
            let read = read_with(&mut model, 1, &options(out.path()), |_| {}).unwrap();
            read.save(&snapshot.join("reading.json")).unwrap();

            let mut told = 0;
            let again = recount_corpus(
                out.path(),
                1,
                2,
                &model.provenance,
                &AtomicBool::new(false),
                |_| told += 1,
            )
            .unwrap();
            assert_eq!(
                told, 0,
                "a corpus this small is counted before there is anything to tell"
            );
            assert_eq!(tallies(&again), tallies(&read));
            assert_eq!(again.who, read.who);
            assert_eq!(
                (
                    again.reviews,
                    again.corpus_reviews,
                    again.claims,
                    again.unclassified_claims
                ),
                (
                    read.reviews,
                    read.corpus_reviews,
                    read.claims,
                    read.unclassified_claims
                )
            );
            assert_eq!(
                (
                    again.silent_reviews,
                    again.claimless_reviews,
                    again.positive,
                    again.top_helpful
                ),
                (
                    read.silent_reviews,
                    read.claimless_reviews,
                    read.positive,
                    read.top_helpful
                )
            );
            assert_eq!(again.languages, read.languages);
            assert_eq!(again.months.len(), read.months.len());

            // The reading's own record of how it was read, kept as it was.
            assert_eq!(
                (
                    again.forward_passes,
                    again.device.as_str(),
                    again.model.as_str()
                ),
                (read.forward_passes, "table", "a table")
            );
            assert_eq!(
                (
                    again.read_with.as_str(),
                    again.reader.as_str(),
                    again.read_by_rule.as_str()
                ),
                ("run-1", "Table Reader", "lines-1")
            );
            assert_eq!(again.trained_on, "labels-1");
            assert!((again.threshold - 0.5).abs() < f32::EPSILON);
            assert_eq!(again.usual_declined, Some(0.25));
            assert_eq!(again.frozen.map(|frozen| frozen.claims), Some(300));
            assert_eq!(again.context, context);
            assert_eq!(again.captured_unix, SWEPT);
            assert!(again.elapsed > Duration::ZERO);
            assert!(!snapshot.join("readings.recount.parquet").exists());
        }
    }

    #[test]
    fn a_recount_of_one_language_counts_what_the_read_of_it_counted() {
        let out = crate::tempdir::Dir::new();
        let snapshot = corpus(out.path());
        let mut model = Table::new(false);
        let options = ReadOptions {
            language: Some("english".to_owned()),
            ..options(out.path())
        };
        let read = read_with(&mut model, 1, &options, |_| {}).unwrap();
        read.save(&snapshot.join("reading.json")).unwrap();
        let again = recount_corpus(
            out.path(),
            1,
            2,
            &model.provenance,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!((again.reviews, again.corpus_reviews), (6, 7));
        assert_eq!(tallies(&again), tallies(&read));
    }

    #[test]
    fn a_recount_asked_to_stop_stops_and_leaves_the_reading_as_it_was() {
        let out = crate::tempdir::Dir::new();
        let snapshot = corpus(out.path());
        let mut model = Table::new(false);
        let read = read_with(&mut model, 1, &options(out.path()), |_| {}).unwrap();
        read.save(&snapshot.join("reading.json")).unwrap();
        let before = std::fs::read(snapshot.join("reading.json")).unwrap();
        let stopped = recount_corpus(
            out.path(),
            1,
            2,
            &model.provenance,
            &AtomicBool::new(true),
            |_| {},
        );
        assert!(matches!(stopped, Err(Error::Stopped)), "{stopped:?}");
        assert_eq!(
            std::fs::read(snapshot.join("reading.json")).unwrap(),
            before
        );
        assert!(!snapshot.join("readings.recount.parquet").exists());
    }

    #[test]
    fn every_subject_is_counted_again_for_each_kind_of_reviewer() {
        let out = crate::tempdir::Dir::new();
        corpus(out.path());
        let report = read_with(&mut Table::new(false), 1, &options(out.path()), |_| {}).unwrap();
        let kind = |id: &str| {
            report
                .who
                .iter()
                .find(|count| count.id == id)
                .unwrap_or_else(|| panic!("no count of {id}"))
        };
        assert_eq!(
            report
                .who
                .iter()
                .map(|count| count.id.as_str())
                .collect::<Vec<_>>(),
            crate::who::SEGMENTS.map(|segment| segment.id)
        );
        let bugs = SHEET.iter().position(|c| c.id == "bugs").unwrap();
        let (newcomer, deck) = (kind("under-2-hours"), kind("steam-deck"));
        assert_eq!(
            (newcomer.reviews, &newcomer.raised, &newcomer.months),
            (deck.reviews, &deck.raised, &deck.months),
            "the one reviewer under two hours played on a Deck"
        );
        assert_eq!((newcomer.reviews, newcomer.positive), (1, 0));
        assert_eq!(newcomer.raised[bugs], 1);
        assert_eq!(newcomer.criticised[bugs], 1);
        assert_eq!(newcomer.claims_about[bugs], 1);
        assert_eq!(newcomer.claims, 2);
        assert_eq!(newcomer.months.len(), 1);
        let rest = kind("30-to-100-hours");
        assert_eq!(rest.reviews, report.reviews - 1);
        assert_eq!(rest.positive, report.positive);
        assert_eq!(kind("elsewhere").reviews, report.reviews - 1);
        assert_eq!(kind("paid-for-it").reviews, report.reviews);
        assert_eq!(kind("got-it-free").reviews, 0);
        assert_eq!(
            kind("after-release").claims + kind("early-access").claims,
            report.claims
        );
    }

    #[test]
    fn a_recount_refuses_readings_it_cannot_reconcile() {
        let out = crate::tempdir::Dir::new();
        let snapshot = corpus(out.path());
        let mut model = Table::new(false);
        let read = read_with(&mut model, 1, &options(out.path()), |_| {}).unwrap();
        let recount = |reading: &ReadReport, provenance: &Provenance| {
            reading.save(&snapshot.join("reading.json")).unwrap();
            recount_corpus(
                out.path(),
                1,
                2,
                provenance,
                &AtomicBool::new(false),
                |_| {},
            )
        };
        assert!(recount(&read, &model.provenance).is_ok());

        let mut redrawn = model.provenance.clone();
        redrawn.lines_fingerprint = "lines-2".to_owned();
        let refused = recount(&read, &redrawn).unwrap_err().to_string();
        assert!(
            refused.contains("lines-1") && refused.contains("lines-2"),
            "{refused}"
        );

        let more_claims = ReadReport {
            claims: read.claims + 1,
            ..read.clone()
        };
        assert!(recount(&more_claims, &model.provenance).is_err());
        let fewer_declined = ReadReport {
            unclassified_claims: read.unclassified_claims - 1,
            ..read.clone()
        };
        assert!(recount(&fewer_declined, &model.provenance).is_err());
    }

    #[test]
    fn a_recount_says_how_far_it_has_got_at_every_interval_of_reviews() {
        let out = crate::tempdir::Dir::new();
        let snapshot = corpus(out.path());
        let mut told = Vec::new();
        let counted = recount_rows(
            &snapshot,
            &out.path().join("replay.parquet"),
            (&options(out.path()), &AtomicBool::new(false)),
            false,
            &Stored::new(),
            (2, &mut |progress: ReadProgress| {
                told.push((
                    progress.claims_read,
                    progress.reviews_counted,
                    progress.reviews_walked,
                ));
            }),
        )
        .unwrap();
        assert_eq!(told, [(4, 2, 2), (6, 4, 4), (8, 6, 6)]);
        assert_eq!(
            (counted.claims, counted.unclassified),
            (10, 10),
            "a claim with no stored answer is a claim nobody answered"
        );
    }

    #[test]
    fn a_reading_of_no_claims_has_no_share_declined_rather_than_an_undefined_one() {
        let mut found: ReadReport = serde_json::from_value(serde_json::json!({
            "app_id": 1, "reviews": 0, "corpus_reviews": 0, "language": null, "claims": 0,
            "unclassified_claims": 0, "silent_reviews": 0, "positive": 0, "top_helpful": 0,
            "model": "m", "threshold": 0.5, "device": "cpu", "usual_declined": 0.25,
            "subjects": [], "languages": [], "months": []
        }))
        .unwrap();
        assert_eq!(found.unclassified_share(), None);
        assert_eq!(found.declined_against_usual(), None);
        found.claims = 10;
        found.unclassified_claims = 5;
        found.usual_declined = Some(0.0);
        assert_eq!(found.unclassified_share(), Some(0.5));
        assert_eq!(
            found.declined_against_usual(),
            None,
            "a reader that never declines gives nothing to compare against"
        );
    }

    #[test]
    fn the_languages_read_more_strictly_than_english_and_the_unread_are_named() {
        let dir = crate::tempdir::Dir::new();
        let mut counting = Counting::new(&dir.path().join("readings.parquet"), 1).unwrap();
        for (language, reviews) in [
            ("english", 5),
            ("german", 4),
            ("french", 3),
            ("spanish", 2),
            ("korean", 1),
        ] {
            for _ in 0..reviews {
                counting.note_corpus(&crate::capture::Row {
                    recommendationid: String::new(),
                    helpfulness: 0.0,
                    votes_up: 0,
                    voted_up: false,
                    language: language.to_owned(),
                    created: 0,
                    reviewer: crate::who::Reviewer::default(),
                });
            }
        }
        let provenance: Provenance = serde_json::from_value(serde_json::json!({
            "subjects": [], "threshold": 0.5, "max_tokens": 128,
            "language_thresholds": {"english": 0.5, "german": 0.7, "french": 0.4,
                                    "spanish": 0.5, "korean": null},
        }))
        .unwrap();
        let report = counting
            .finish(1, &ReadOptions::default(), &provenance)
            .unwrap();
        assert_eq!(report.corpus_reviews, 15);
        assert_eq!(report.strict_languages, [("german".to_owned(), 4)]);
        assert_eq!(report.unread_languages, [("korean".to_owned(), 1)]);
    }

    #[test]
    fn a_depth_is_named_as_a_reading_records_it() {
        assert_eq!(Depth::Deep.as_str(), "deep");
        assert_eq!(Depth::Shallow.as_str(), "shallow");
    }

    #[test]
    fn a_shallow_point_is_the_review_without_the_space_around_it() {
        let text = "  Great game, buy it.\n";
        let spans = Depth::Shallow.spans_of(text);
        assert_eq!((spans.len(), spans[0].clone()), (1, 2..21));
        assert_eq!(&text[2..21], Depth::Shallow.claims_of(text)[0]);
    }

    #[test]
    fn a_rate_needs_reviews_to_be_a_rate_of() {
        let month = |reviews, positive| Month {
            label: "2024-03".to_owned(),
            reviews,
            positive,
            subjects: vec![reviews / 2],
            praising: Vec::new(),
            complaining: Vec::new(),
        };
        assert_eq!(month(0, 0).rate(0), None);
        assert_eq!(month(10, 5).rate(0), Some(0.5));
        assert_eq!(month(10, 5).rate(1), None);
        assert_eq!(
            month(Month::ENOUGH_FOR_A_RATE, 15).positive_share_if_enough(),
            Some(0.5)
        );
        assert_eq!(
            month(Month::ENOUGH_FOR_A_RATE - 1, 15).positive_share_if_enough(),
            None
        );
    }

    #[test]
    fn a_review_is_chiefly_about_its_surest_claim_and_the_first_of_two_as_sure() {
        let at = |id: &str| SHEET.iter().position(|row| row.id == id).unwrap();
        let claims = ["Bugs everywhere.", "Lovely music."];
        let answered = |bugs: f32, audio: f32| {
            claims
                .iter()
                .enumerate()
                .map(|(index, claim)| {
                    let (subject, confidence) = if index == 0 {
                        (at("bugs"), bugs)
                    } else {
                        (at("audio"), audio)
                    };
                    (
                        key(false, &[0; 32], index, claim, "english"),
                        Reading {
                            subject: Some(subject),
                            confidence,
                            polarity: Polarity::Praise,
                            also: crate::reader::Also::default(),
                        },
                    )
                })
                .collect::<HashMap<_, _>>()
        };
        let primary = |bugs, audio| {
            judge(&claims, &[0; 32], false, "english", &answered(bugs, audio)).primary
        };
        assert_eq!(primary(0.9, 0.95), Some(at("audio")));
        assert_eq!(primary(0.9, 0.9), Some(at("bugs")));
    }

    #[test]
    fn the_mixed_share_is_of_the_reviews_that_raise_the_subject() {
        let subject = SubjectCount {
            id: "bugs".to_owned(),
            label: "Bugs and crashes".to_owned(),
            mention_reviews: 12,
            primary_reviews: 6,
            claims: 20,
            praised: 4,
            criticised: 5,
            mixed: 3,
            top_mention_reviews: 1,
            positive_mentions: 4,
        };
        assert_eq!(subject.mixed_share(), Some(0.25));
        let unraised = SubjectCount {
            mention_reviews: 0,
            ..subject
        };
        assert_eq!(unraised.mixed_share(), None);
    }

    #[test]
    fn a_read_given_part_of_the_card_rests_in_proportion_to_its_work() {
        let busy = std::time::Duration::from_millis(400);
        assert_eq!(
            rest_for(busy, 0.5),
            busy,
            "half the card rests as long as it worked"
        );
        assert_eq!(rest_for(busy, 0.25), busy * 3);
        assert_eq!(rest_for(busy, 1.0), std::time::Duration::ZERO);
        assert_eq!(
            rest_for(busy, 0.0),
            std::time::Duration::ZERO,
            "a share of nothing is refused where it is given, never slept on forever"
        );
    }

    #[test]
    fn a_claim_is_under_every_subject_it_names_with_the_polarity_it_takes_on_each() {
        let at = |id: &str| SHEET.iter().position(|row| row.id == id).unwrap();
        let mut also = crate::reader::Also::default();
        also.insert(at("controls"), Polarity::Complaint);
        assert_eq!(
            polarity_on("audio", Some("audio"), "praise", also),
            Some("praise")
        );
        assert_eq!(
            polarity_on("controls", Some("audio"), "praise", also),
            Some("complaint"),
            "beside the first subject, with its own polarity"
        );
        assert_eq!(polarity_on("story", Some("audio"), "praise", also), None);
        assert_eq!(
            polarity_on("controls", None, "neutral", also),
            Some("complaint"),
            "named beside a first subject the reader declined"
        );
    }

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
                    reviewer: crate::who::Reviewer::default(),
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
    fn a_month_counts_who_praised_and_who_complained_about_each_subject() {
        let dir = crate::tempdir::Dir::new();
        let mut counting = Counting::new(&dir.path().join("readings.parquet"), 10).unwrap();
        let at = |id: &str| SHEET.iter().position(|row| row.id == id).unwrap();
        let claims = Depth::Deep.claims_of("Bugs everywhere. Lovely music.");
        assert_eq!(claims.len(), 2);
        let mut spans = Vec::new();
        let mut answers = HashMap::new();
        let mut from = 0;
        for (index, claim) in claims.iter().enumerate() {
            spans.push((from, from + claim.len()));
            from += claim.len() + 1;
            let (subject, polarity) = if index == 0 {
                (at("bugs"), Polarity::Complaint)
            } else {
                (at("audio"), Polarity::Praise)
            };
            answers.insert(
                key(false, &[0; 32], index, claim, "english"),
                Reading {
                    subject: Some(subject),
                    confidence: if index == 0 { 0.9 } else { 0.95 },
                    polarity,
                    also: crate::reader::Also::default(),
                },
            );
        }
        let review = Pending {
            at: Vec::new(),
            row: crate::capture::Row {
                recommendationid: "1".to_owned(),
                helpfulness: 0.0,
                votes_up: 0,
                voted_up: false,
                language: "english".to_owned(),
                // 15 March 2024.
                created: 1_710_504_000,
                reviewer: crate::who::Reviewer::default(),
            },
            text: Arc::from(rejoined(&claims)),
            spans,
            fingerprint: [0; 32],
        };
        counting.count(&review, false, &answers).unwrap();
        // A recommending review the reader answered nothing about: counted, and silent.
        let unanswered = Depth::Deep.claims_of("Nothing anybody asked about.");
        let quiet = Pending {
            at: Vec::new(),
            row: crate::capture::Row {
                recommendationid: "2".to_owned(),
                voted_up: true,
                ..review.row.clone()
            },
            text: Arc::from(rejoined(&unanswered)),
            spans: unanswered
                .iter()
                .scan(0, |from, claim| {
                    let span = (*from, *from + claim.len());
                    *from += claim.len() + 1;
                    Some(span)
                })
                .collect(),
            fingerprint: [1; 32],
        };
        counting.count(&quiet, false, &answers).unwrap();
        let provenance: crate::reader::Provenance = serde_json::from_value(serde_json::json!({
            "subjects": [], "threshold": 0.5, "max_tokens": 128
        }))
        .unwrap();
        let report = counting
            .finish(1, &ReadOptions::default(), &provenance)
            .unwrap();

        let month = &report.months[0];
        assert_eq!(month.label, "2024-03");
        assert_eq!(
            (month.complaining[at("bugs")], month.praising[at("bugs")]),
            (1, 0)
        );
        assert_eq!(
            (month.praising[at("audio")], month.complaining[at("audio")]),
            (1, 0)
        );
        assert_eq!((month.reviews, month.positive), (2, 1));
        assert_eq!(
            (report.reviews, report.positive, report.silent_reviews),
            (2, 1, 1)
        );
        let primary = |id: &str| {
            report
                .subjects
                .iter()
                .find(|subject| subject.id == id)
                .map_or(0, |subject| subject.primary_reviews)
        };
        assert_eq!(
            (primary("audio"), primary("bugs")),
            (1, 0),
            "a review is chiefly about the claim the reader was surest of, not its first"
        );
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
        assert_eq!(
            Depth::Shallow.claims_of("   \n  "),
            [] as [std::borrow::Cow<'_, str>; 0]
        );
        assert_eq!(
            Depth::Deep.claims_of("   \n  "),
            [] as [std::borrow::Cow<'_, str>; 0]
        );
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
        assert_eq!(found.trained_on, "");
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
            who: Vec::new(),
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
