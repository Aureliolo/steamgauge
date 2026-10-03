//! What a game's reviewers said in other words, and in other languages, than somebody typed.
//!
//! The word search counts what matches the words; this finds what is near them in meaning,
//! across the languages the reviews are written in, and ranks it. It never counts: a ranking
//! by meaning has no line at which "says this" ends, so any count of it would be a count of
//! where the line was drawn. What it offers is the claims a word search cannot reach, in the
//! order the reranker in [`crate::search_models`] puts the nearest of them.
//!
//! Every distinct claim of a reading is embedded once with the search encoder and kept beside
//! the reading at a byte a dimension. That is minutes on a card and hours on a processor, so it
//! is done only when somebody asks, in parts small enough that stopping loses little, and a
//! later preparation carries on from the parts already written.

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering as Atomic},
    },
    time::{Duration, Instant},
};

use arrow::{
    array::{
        Array, ArrayRef, FixedSizeBinaryArray, FixedSizeBinaryBuilder, FixedSizeListArray,
        FixedSizeListBuilder, Float32Array, Float32Builder, Int8Array, Int8Builder, StringArray,
        StringBuilder, UInt32Array, UInt32Builder,
    },
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use parquet::arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder};
use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    claims::Span,
    embed::sha256_bytes,
    reader::Polarity,
    search::DECLINED,
    search_models::{DIMENSIONS, ENCODER},
    taxonomy::SHEET,
};

/// Where a reading's vectors are kept, beside it in the snapshot.
const DIR: &str = "meaning";
const SIDECAR: &str = "meaning.json";

/// A part is written at least this often, so stopping a preparation on a processor, where a
/// claim takes the better part of a tenth of a second, loses a minute of it and no more.
const PART_EVERY: Duration = Duration::from_secs(60);
const PART_MOST: usize = 20_000;

/// Texts are sorted by length within a window before they are batched, so a batch is padded to
/// a length its members share rather than to its longest.
const WINDOW: usize = 1_024;
const BATCH: usize = 64;

/// Bytes on disk for each distinct claim, at most: a byte for each of the vector's dimensions,
/// the scale that restores it, the reader's confidence, the claim's key and where it was said.
pub const BYTES_PER_CLAIM: u64 = DIMENSIONS as u64 + 4 + 4 + 32 + 32;

/// How far a reading's vectors have got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Nothing prepared.
    None,
    /// Stopped part way; preparing again carries on.
    Partial,
    /// Prepared for a reading the game has since replaced; preparing again adds what is new.
    Stale,
    Ready,
}

/// What the directory's parts were prepared from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Sidecar {
    encoder: String,
    /// Whether a preparation walked every claim of the reading below.
    complete: bool,
    /// When the readings file it walked was last written, in seconds since the epoch.
    readings: Option<u64>,
}

fn readings_written(snapshot: &Path) -> Option<u64> {
    std::fs::metadata(snapshot.join("readings.parquet"))
        .and_then(|file| file.modified())
        .ok()
        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs())
}

fn sidecar(snapshot: &Path) -> Option<Sidecar> {
    serde_json::from_slice(&std::fs::read(snapshot.join(DIR).join(SIDECAR)).ok()?).ok()
}

fn parts(snapshot: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(snapshot.join(DIR))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "parquet"))
        .collect();
    found.sort();
    found
}

/// How far the vectors of the reading at `snapshot` have got.
#[must_use]
pub fn status(snapshot: &Path) -> Status {
    let side = sidecar(snapshot).filter(|side| side.encoder == ENCODER.name);
    match side {
        Some(side) if side.complete && side.readings == readings_written(snapshot) => Status::Ready,
        Some(side) if side.complete => Status::Stale,
        _ if parts(snapshot).is_empty() => Status::None,
        _ => Status::Partial,
    }
}

fn schema() -> Arc<Schema> {
    let dimensions = i32::try_from(DIMENSIONS).unwrap_or(0);
    Arc::new(Schema::new(vec![
        Field::new("key", DataType::FixedSizeBinary(32), false),
        Field::new("recommendationid", DataType::Utf8, false),
        Field::new("start", DataType::UInt32, false),
        Field::new("end", DataType::UInt32, false),
        Field::new("subject", DataType::Utf8, false),
        Field::new("polarity", DataType::Utf8, false),
        Field::new("confidence", DataType::Float32, false),
        Field::new("scale", DataType::Float32, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Int8, true)),
                dimensions,
            ),
            false,
        ),
    ]))
}

/// A unit vector as bytes, and what to divide them by to get it back.
///
/// Scaled by the vector's own largest component rather than by one: the components of a unit
/// vector in a thousand dimensions are a few hundredths each, and at a fixed scale nearly all of them
/// would round to the same handful of values.
fn quantise(vector: &[f32]) -> (f32, Vec<i8>) {
    let largest = vector
        .iter()
        .fold(0.0_f32, |most, value| most.max(value.abs()));
    if largest == 0.0 {
        return (1.0, vec![0; vector.len()]);
    }
    let scale = 127.0 / largest;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "scaled into -127..=127 before the cast"
    )]
    let bytes = vector
        .iter()
        .map(|value| (value * scale).round().clamp(-127.0, 127.0) as i8)
        .collect();
    (scale, bytes)
}

/// What a claim's key is: its bytes hashed. The same words anywhere in the game are one key and
/// one vector, and a review edited since can be told apart by its bytes no longer hashing to it.
#[must_use]
pub fn key_of(claim: &str) -> [u8; 32] {
    sha256_bytes(claim)
}

/// A claim as the reading filed it.
struct Filed {
    at: Span,
    subject: &'static str,
    polarity: &'static str,
    confidence: f32,
}

/// Every claim of the reading at `snapshot`, by review.
fn filed(snapshot: &Path) -> Result<HashMap<String, Vec<Filed>>> {
    let mut filed: HashMap<String, Vec<Filed>> = HashMap::new();
    crate::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, subject, confidence, polarity, _| {
            let subject = subject
                .and_then(|name| SHEET.iter().find(|row| row.id == name))
                .map_or(DECLINED, |row| row.id);
            filed.entry(id.to_owned()).or_default().push(Filed {
                at,
                subject,
                polarity: Polarity::from_name(polarity).as_str(),
                confidence,
            });
        },
    )?;
    Ok(filed)
}

/// The keys of every claim the parts already hold a vector for.
fn embedded(snapshot: &Path) -> Result<HashSet<[u8; 32]>> {
    let mut done: HashSet<[u8; 32]> = HashSet::new();
    for part in parts(snapshot) {
        each_row(&part, |row| {
            done.insert(row.key);
        })?;
    }
    Ok(done)
}

/// One distinct claim waiting for its vector.
struct Waiting {
    key: [u8; 32],
    review: Arc<str>,
    at: Span,
    subject: &'static str,
    polarity: &'static str,
    confidence: f32,
    text: String,
}

/// Rows gathered for the next part.
struct Part {
    keys: FixedSizeBinaryBuilder,
    reviews: StringBuilder,
    starts: UInt32Builder,
    ends: UInt32Builder,
    subjects: StringBuilder,
    polarities: StringBuilder,
    confidences: Float32Builder,
    scales: Float32Builder,
    vectors: FixedSizeListBuilder<Int8Builder>,
    rows: usize,
    since: Instant,
}

impl Part {
    fn new() -> Self {
        let dimensions = i32::try_from(DIMENSIONS).unwrap_or(0);
        Self {
            keys: FixedSizeBinaryBuilder::new(32),
            reviews: StringBuilder::new(),
            starts: UInt32Builder::new(),
            ends: UInt32Builder::new(),
            subjects: StringBuilder::new(),
            polarities: StringBuilder::new(),
            confidences: Float32Builder::new(),
            scales: Float32Builder::new(),
            vectors: FixedSizeListBuilder::new(Int8Builder::new(), dimensions),
            rows: 0,
            since: Instant::now(),
        }
    }

    fn push(&mut self, claim: &Waiting, vector: &[f32]) -> Result<()> {
        let (scale, bytes) = quantise(vector);
        self.keys.append_value(claim.key)?;
        self.reviews.append_value(&claim.review);
        self.starts.append_value(claim.at.0);
        self.ends.append_value(claim.at.1);
        self.subjects.append_value(claim.subject);
        self.polarities.append_value(claim.polarity);
        self.confidences.append_value(claim.confidence);
        self.scales.append_value(scale);
        self.vectors.values().append_slice(&bytes);
        self.vectors.append(true);
        self.rows += 1;
        Ok(())
    }

    fn due(&self) -> bool {
        self.rows >= PART_MOST || (self.rows > 0 && self.since.elapsed() >= PART_EVERY)
    }

    /// Writes the part aside and renames it into place, so a part on disk is always whole.
    fn write(&mut self, dir: &Path) -> Result<()> {
        if self.rows == 0 {
            return Ok(());
        }
        let columns: Vec<ArrayRef> = vec![
            Arc::new(self.keys.finish()),
            Arc::new(self.reviews.finish()),
            Arc::new(self.starts.finish()),
            Arc::new(self.ends.finish()),
            Arc::new(self.subjects.finish()),
            Arc::new(self.polarities.finish()),
            Arc::new(self.confidences.finish()),
            Arc::new(self.scales.finish()),
            Arc::new(self.vectors.finish()),
        ];
        let batch = RecordBatch::try_new(schema(), columns)?;
        let name = format!("part-{:05}.parquet", parts_in(dir));
        let partial = dir.join(format!("{name}.partial"));
        let mut writer = ArrowWriter::try_new(std::fs::File::create(&partial)?, schema(), None)?;
        writer.write(&batch)?;
        writer.close()?;
        std::fs::rename(&partial, dir.join(name))?;
        self.rows = 0;
        self.since = Instant::now();
        Ok(())
    }
}

fn parts_in(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "parquet"))
        .count()
}

/// What one preparation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prepared {
    /// Distinct claims embedded this time, not counting those a previous preparation did.
    pub embedded: u64,
    /// Claims walked, repeats included, which is what progress is counted in.
    pub walked: u64,
    /// Whether every claim of the reading now has its vector.
    pub finished: bool,
}

/// Embeds every distinct claim of the reading at `snapshot` that has no vector yet.
///
/// `embed` turns a batch of texts into unit vectors. `stop` is looked at between batches;
/// once it is set the part in hand is written and the preparation returns unfinished, and the
/// next one carries on. `share` is the share of the card's time it may take, as a read's is.
/// `on_progress` is told how many claims have been walked.
///
/// # Errors
///
/// Fails if the reading, the capture or the parts cannot be read or written, or if `embed`
/// fails.
pub fn prepare(
    snapshot: &Path,
    mut embed: impl FnMut(&[String]) -> Result<Vec<Vec<f32>>>,
    (stop, share): (&AtomicBool, f64),
    mut on_progress: impl FnMut(u64),
) -> Result<Prepared> {
    let dir = snapshot.join(DIR);
    std::fs::create_dir_all(&dir)?;
    // A reading replaced since the parts were written is carried on from rather than thrown
    // away: a claim is the bytes it covers, so a vector keyed by those bytes is still its
    // vector, and only what the new reading added is missing.
    let mut side = sidecar(snapshot).unwrap_or_default();
    // Vectors from another encoder live in another space, and a search asked in this one would
    // find nothing near them that is near in meaning.
    if side.encoder != ENCODER.name {
        for part in parts(snapshot) {
            std::fs::remove_file(part)?;
        }
    }
    ENCODER.name.clone_into(&mut side.encoder);
    side.complete = false;
    write_sidecar(&dir, &side)?;

    let mut done = embedded(snapshot)?;
    let filed = filed(snapshot)?;
    let mut prepared = Prepared {
        embedded: 0,
        walked: 0,
        finished: false,
    };
    let mut waiting: Vec<Waiting> = Vec::with_capacity(WINDOW);
    let mut part = Part::new();
    let mut flush = |waiting: &mut Vec<Waiting>, part: &mut Part, prepared: &mut Prepared| {
        waiting.sort_by_key(|claim| claim.text.len());
        for batch in waiting.chunks(BATCH) {
            if stop.load(Atomic::Relaxed) {
                return Err(Error::Stopped);
            }
            let texts: Vec<String> = batch.iter().map(|claim| claim.text.clone()).collect();
            let started = Instant::now();
            let vectors = embed(&texts)?;
            std::thread::sleep(crate::read::rest_for(started.elapsed(), share));
            for (claim, vector) in batch.iter().zip(vectors) {
                part.push(claim, &vector)?;
                prepared.embedded += 1;
            }
            if part.due() {
                part.write(&dir)?;
            }
        }
        waiting.clear();
        Ok(())
    };

    let walked = crate::capture::for_each_row(snapshot, |row, text| {
        let Some(claims) = filed.get(&row.recommendationid) else {
            return Ok(());
        };
        let review: Arc<str> = Arc::from(row.recommendationid.as_str());
        for filed in claims {
            prepared.walked += 1;
            let Some(claim) = text.get(filed.at.0 as usize..filed.at.1 as usize) else {
                continue;
            };
            let key = key_of(claim);
            if !done.insert(key) {
                continue;
            }
            waiting.push(Waiting {
                key,
                review: Arc::clone(&review),
                at: filed.at,
                subject: filed.subject,
                polarity: filed.polarity,
                confidence: filed.confidence,
                text: claim.to_owned(),
            });
        }
        if waiting.len() >= WINDOW {
            flush(&mut waiting, &mut part, &mut prepared)?;
            on_progress(prepared.walked);
        }
        Ok(())
    });
    let finished = match walked.and_then(|()| flush(&mut waiting, &mut part, &mut prepared)) {
        Ok(()) => true,
        Err(Error::Stopped) => false,
        Err(other) => return Err(other),
    };
    part.write(&dir)?;
    on_progress(prepared.walked);

    if finished {
        side.complete = true;
        side.readings = readings_written(snapshot);
        write_sidecar(&dir, &side)?;
    }
    prepared.finished = finished;
    Ok(prepared)
}

fn write_sidecar(dir: &Path, side: &Sidecar) -> Result<()> {
    let partial = dir.join(format!("{SIDECAR}.partial"));
    std::fs::write(&partial, serde_json::to_vec_pretty(side)?)?;
    std::fs::rename(partial, dir.join(SIDECAR))?;
    Ok(())
}

/// One claim near what was asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Near {
    pub similarity: f32,
    pub review_id: String,
    pub at: Span,
    pub subject: String,
    pub polarity: String,
    /// How sure the reader was of the claim's subject, as the reading records it.
    pub confidence: f32,
    /// The claim's bytes hashed, so a caller can check the review still says it there.
    pub key: [u8; 32],
}

struct Row<'a> {
    key: [u8; 32],
    review: &'a str,
    at: Span,
    subject: &'a str,
    polarity: &'a str,
    confidence: f32,
    scale: f32,
    vector: &'a [i8],
}

fn each_row(part: &Path, mut visit: impl FnMut(Row<'_>)) -> Result<()> {
    let reader = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(part)?)?.build()?;
    let malformed = |field| Error::MalformedPayload { field };
    for batch in reader {
        let batch = batch?;
        let column = |name: &'static str| batch.column_by_name(name).ok_or(malformed(name));
        let keys = column("key")?
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .ok_or(malformed("key"))?;
        let reviews = column("recommendationid")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(malformed("recommendationid"))?;
        let starts = column("start")?
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or(malformed("start"))?;
        let ends = column("end")?
            .as_any()
            .downcast_ref::<UInt32Array>()
            .ok_or(malformed("end"))?;
        let subjects = column("subject")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(malformed("subject"))?;
        let polarities = column("polarity")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(malformed("polarity"))?;
        let confidences = column("confidence")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or(malformed("confidence"))?;
        let scales = column("scale")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or(malformed("scale"))?;
        let vectors = column("vector")?
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .ok_or(malformed("vector"))?;
        let values = vectors
            .values()
            .as_any()
            .downcast_ref::<Int8Array>()
            .ok_or(malformed("vector"))?
            .values();
        let width = usize::try_from(vectors.value_length()).unwrap_or(0);
        for row in 0..batch.num_rows() {
            let key: [u8; 32] = keys.value(row).try_into().map_err(|_| malformed("key"))?;
            let from = (vectors.offset() + row) * width;
            visit(Row {
                key,
                review: reviews.value(row),
                at: (starts.value(row), ends.value(row)),
                subject: subjects.value(row),
                polarity: polarities.value(row),
                confidence: confidences.value(row),
                scale: scales.value(row),
                vector: &values[from..from + width],
            });
        }
    }
    Ok(())
}

/// Ordered by similarity alone, so a heap of them keeps the nearest.
struct Ranked(Near);

impl PartialEq for Ranked {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Ranked {}
impl PartialOrd for Ranked {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ranked {
    // Reversed, so the heap's top is the least similar kept and is the one a nearer claim
    // pushes out.
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.similarity.total_cmp(&self.0.similarity)
    }
}

/// The `most` claims nearest `query`, a unit vector, nearest first, leaving out any `skip` says
/// to: the claims a word search already found.
///
/// No closeness is too far here. What this gathers is handed to the reranker, which reads each
/// one against the search and is what decides whether it is shown: the judged comparison that
/// chose the pair took the encoder's hundred nearest as they came.
///
/// # Errors
///
/// Fails if a part cannot be read.
pub fn nearest(
    snapshot: &Path,
    query: &[f32],
    most: usize,
    skip: impl Fn(&str, Span) -> bool,
) -> Result<Vec<Near>> {
    let mut kept: BinaryHeap<Ranked> = BinaryHeap::with_capacity(most + 1);
    for part in parts(snapshot) {
        each_row(&part, |row| {
            let dot: f32 = row
                .vector
                .iter()
                .zip(query)
                .map(|(&byte, &value)| f32::from(byte) * value)
                .sum();
            let similarity = dot / row.scale;
            if kept
                .peek()
                .is_some_and(|least| kept.len() >= most && similarity <= least.0.similarity)
                || skip(row.review, row.at)
            {
                return;
            }
            kept.push(Ranked(Near {
                similarity,
                review_id: row.review.to_owned(),
                at: row.at,
                subject: row.subject.to_owned(),
                polarity: row.polarity.to_owned(),
                confidence: row.confidence,
                key: row.key,
            }));
            if kept.len() > most {
                kept.pop();
            }
        })?;
    }
    let mut nearest: Vec<Near> = kept.into_iter().map(|ranked| ranked.0).collect();
    nearest.sort_by(|a, b| b.similarity.total_cmp(&a.similarity));
    Ok(nearest)
}

/// Distinct claims with vectors, and the bytes they take on disk.
#[must_use]
pub fn held(snapshot: &Path) -> (u64, u64) {
    parts(snapshot)
        .iter()
        .fold((0, 0), |(claims, bytes), part| {
            let rows = std::fs::File::open(part)
                .ok()
                .and_then(|file| ParquetRecordBatchReaderBuilder::try_new(file).ok())
                .map_or(0, |reader| {
                    u64::try_from(reader.metadata().file_metadata().num_rows()).unwrap_or(0)
                });
            let size = std::fs::metadata(part).map_or(0, |file| file.len());
            (claims + rows, bytes + size)
        })
}

/// How long preparing has taken on this machine, per kind of device, so an estimate is this
/// machine's own once it has prepared anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Times {
    taken: Vec<Taken>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Taken {
    /// `card` or `processor`.
    on: String,
    /// Which encoder took it: another's pace says nothing about this one's.
    #[serde(default)]
    encoder: String,
    seconds: f64,
    /// Claims walked, repeats included: what is known of a game before it is prepared.
    walked: u64,
}

/// The file the times are kept in, in the library directory.
pub const TIMES_FILE: &str = "meaning-times.json";

/// Seconds a claim takes on this project's machine, for an estimate before a machine has
/// measured its own: 3,000 claims of 1466860 through the exported encoder in the app's batches,
/// on the processor and on a card a training run held half of.
pub const PROCESSOR_SECONDS_PER_CLAIM: f64 = 0.118;
pub const CARD_SECONDS_PER_CLAIM: f64 = 0.0087;

impl Times {
    /// The times kept in `dir`, or none where there is no file or it cannot be read.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(TIMES_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Keeps the times in `dir`.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::write(dir.join(TIMES_FILE), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// Adds one preparation on a card or on the processor.
    pub fn note(&mut self, on_card: bool, seconds: f64, walked: u64) {
        if walked == 0 || !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let on = if on_card { "card" } else { "processor" };
        if let Some(taken) = self
            .taken
            .iter_mut()
            .find(|taken| taken.on == on && taken.encoder == ENCODER.name)
        {
            taken.seconds += seconds;
            taken.walked += walked;
        } else {
            self.taken.push(Taken {
                on: on.to_owned(),
                encoder: ENCODER.name.to_owned(),
                seconds,
                walked,
            });
        }
    }

    /// About how long preparing `claims` claims takes here: this machine's own pace where it
    /// has one on that kind of device, this project's otherwise.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "claim counts are far below 2^53"
    )]
    pub fn estimate(&self, on_card: bool, claims: u64) -> f64 {
        let reference = if on_card {
            CARD_SECONDS_PER_CLAIM
        } else {
            PROCESSOR_SECONDS_PER_CLAIM
        };
        self.per_claim(on_card).unwrap_or(reference) * claims as f64
    }

    /// Seconds a claim walked takes here, where this machine has prepared anything on that
    /// kind of device.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "claim counts are far below 2^53"
    )]
    pub fn per_claim(&self, on_card: bool) -> Option<f64> {
        let on = if on_card { "card" } else { "processor" };
        self.taken
            .iter()
            .find(|taken| taken.on == on && taken.encoder == ENCODER.name)
            .map(|taken| taken.seconds / taken.walked as f64)
    }
}

/// Whether somebody asked for every game they read to be prepared, kept in the library.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub every_game: bool,
}

/// The file the choice is kept in, in the library directory.
pub const CHOICE_FILE: &str = "meaning-choice.json";

impl Choice {
    /// The choice kept in `dir`, or none made where there is no file or it cannot be read.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(CHOICE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Keeps the choice in `dir`.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(self, dir: &Path) -> Result<()> {
        std::fs::write(dir.join(CHOICE_FILE), serde_json::to_vec_pretty(&self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    #[test]
    fn a_vector_survives_being_kept_at_a_byte_a_dimension() {
        // Folded before it is multiplied, so a thousand dimensions stay inside an i16.
        let raw: Vec<f32> = (0..1024_i16)
            .map(|at| f32::from(at % 101 * 37 % 101 - 50) / 7.0)
            .collect();
        let length = raw.iter().map(|value| value * value).sum::<f32>().sqrt();
        let unit: Vec<f32> = raw.iter().map(|value| value / length).collect();

        let (scale, bytes) = quantise(&unit);
        let back: f32 = bytes
            .iter()
            .zip(&unit)
            .map(|(&byte, &value)| f32::from(byte) * value)
            .sum::<f32>()
            / scale;

        assert!(
            (back - 1.0).abs() < 0.002,
            "a vector against itself came back as {back}"
        );
    }

    /// A fake encoder: a claim mentioning "deck" points one way, "price" another, and anything
    /// else a third, so what is near what is known.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "it stands in for an encoder, which can fail"
    )]
    fn embed(texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| {
                let mut vector = vec![0.0_f32; DIMENSIONS];
                let lowered = text.to_lowercase();
                let axis = if lowered.contains("deck") {
                    0
                } else if lowered.contains("price") {
                    1
                } else {
                    2
                };
                vector[axis] = 1.0;
                vector
            })
            .collect())
    }

    fn game(dir: &Path) {
        crate::search::tests::snapshot(dir);
    }

    #[test]
    fn a_prepared_reading_finds_the_claims_nearest_a_query_first() {
        let dir = crate::tempdir::Dir::new();
        game(dir.path());
        assert_eq!(status(dir.path()), Status::None);

        let prepared = prepare(dir.path(), embed, (&AtomicBool::new(false), 1.0), |_| {}).unwrap();
        assert!(prepared.finished);
        assert_eq!(prepared.walked, 5);
        assert_eq!(status(dir.path()), Status::Ready);

        let mut query = vec![0.0_f32; DIMENSIONS];
        query[0] = 1.0;
        let found = nearest(dir.path(), &query, 10, |_, _| false).unwrap();
        assert_eq!(
            found.len(),
            5,
            "every claim, the reranker being what leaves any out"
        );
        assert!(
            found[..3]
                .iter()
                .all(|near| (near.similarity - 1.0).abs() < 0.01)
        );
        assert!(found[3..].iter().all(|near| near.similarity < 0.01));
        let filed: Vec<&str> = found[..3]
            .iter()
            .map(|near| near.subject.as_str())
            .collect();
        assert_eq!(
            (
                filed
                    .iter()
                    .filter(|&&subject| subject == "performance")
                    .count(),
                filed.iter().filter(|&&subject| subject == DECLINED).count()
            ),
            (2, 1),
            "each claim carries the subject its reading filed it under: {filed:?}"
        );

        let skipped = nearest(dir.path(), &query, 3, |review, _| review == "1").unwrap();
        assert_eq!(
            skipped.iter().filter(|near| near.similarity > 0.99).count(),
            2,
            "a claim the words already found is left out"
        );
        assert_eq!(
            nearest(dir.path(), &query, 1, |_, _| false).unwrap().len(),
            1
        );
    }

    #[test]
    fn a_stopped_preparation_is_carried_on_and_nothing_is_embedded_twice() {
        let dir = crate::tempdir::Dir::new();
        game(dir.path());

        let stopped = prepare(dir.path(), embed, (&AtomicBool::new(true), 1.0), |_| {}).unwrap();
        assert!(!stopped.finished);
        assert_eq!(
            status(dir.path()),
            Status::None,
            "stopped before anything was written"
        );

        let first = prepare(dir.path(), embed, (&AtomicBool::new(false), 1.0), |_| {}).unwrap();
        let again = prepare(dir.path(), embed, (&AtomicBool::new(false), 1.0), |_| {}).unwrap();
        assert_eq!(first.embedded, 5);
        assert_eq!(again.embedded, 0, "every claim already had its vector");
        assert!(again.finished);
        assert_eq!(held(dir.path()).0, 5);
    }

    #[test]
    fn a_machine_times_are_its_own_once_it_has_any() {
        let mut times = Times::default();
        assert_eq!(times.per_claim(true), None);
        times.note(true, 10.0, 20_000);
        times.note(true, 0.0, 5);
        assert_eq!(times.per_claim(true), Some(0.0005));
        assert_eq!(times.per_claim(false), None);
    }

    #[test]
    fn the_card_and_the_processor_keep_their_own_pace_and_it_is_kept_on_disk() {
        let dir = crate::tempdir::Dir::new();
        let mut times = Times::default();
        assert!(
            (times.estimate(false, 1_000) - PROCESSOR_SECONDS_PER_CLAIM * 1_000.0).abs() < 1e-9,
            "a machine with no times of its own is given this project's"
        );
        times.note(true, 10.0, 1_000);
        times.note(false, 50.0, 1_000);
        times.note(true, 30.0, 1_000);
        assert_eq!(
            times.per_claim(true),
            Some(0.02),
            "both runs on the card, added"
        );
        assert_eq!(times.per_claim(false), Some(0.05));
        assert!((times.estimate(true, 500) - 10.0).abs() < 1e-9);

        times.save(dir.path()).unwrap();
        assert_eq!(Times::load(dir.path()), times);
    }

    #[test]
    fn the_nearest_claim_sorts_last_so_a_full_heap_gives_up_its_farthest() {
        let near = |similarity| {
            Ranked(Near {
                similarity,
                review_id: String::new(),
                at: (0, 0),
                subject: String::new(),
                polarity: String::new(),
                confidence: 0.0,
                key: [0; 32],
            })
        };
        assert_eq!(near(0.9).partial_cmp(&near(0.1)), Some(Ordering::Less));
        assert!(near(0.1) > near(0.9));
    }

    #[test]
    fn another_encoders_pace_is_not_this_ones() {
        let kept: Times = serde_json::from_str(
            r#"{"taken": [{"on": "card", "seconds": 361.0, "walked": 208912}]}"#,
        )
        .unwrap();
        assert_eq!(
            kept.per_claim(true),
            None,
            "times kept before they named their encoder were another encoder's"
        );
    }
}
