//! Immutable Parquet capture of what Valve served.
//!
//! Every row carries `raw_json`, the review object exactly as it arrived, alongside the
//! typed columns. That redundancy is deliberate: a corpus is expensive to rebuild and, for
//! reviews deleted since the crawl, impossible. Re-parsing must always be an option, and a
//! fixed set of typed columns would silently discard fields Valve adds later.
//!
//! Immutable means append-only, not frozen. A sweep fetches what was written or edited since
//! the capture and writes it beside the original rows, never over them, so a review that was
//! changed is held in both versions. Which version counts is decided once, in [`Newest`], and
//! every pass that reads the capture goes through it.

use std::{collections::HashMap, fs::File, path::Path, sync::Arc};

use arrow::{
    array::{
        Array, ArrayRef, BooleanBuilder, Float64Builder, Int64Array, Int64Builder, StringArray,
        StringBuilder, UInt32Builder,
    },
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use parquet::{
    arrow::{ArrowWriter, ProjectionMask, arrow_reader::ParquetRecordBatchReaderBuilder},
    basic::{Compression, ZstdLevel},
    file::properties::WriterProperties,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// Which copy of a review counts, where a sweep has fetched one that was already captured.
///
/// The newest copy of every id a sweep has written, by its last-edit time and the sweep that
/// wrote it. A row whose id is here counts only if it is that copy; a row whose id is not here
/// was captured once and counts as it is. The map holds one entry per swept review, which is
/// what changed since the crawl rather than the corpus, so it fits in memory where the corpus
/// would not.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Newest {
    /// Id to the last-edit time of the copy that counts and the sweep holding it.
    #[serde(default)]
    pub copies: HashMap<String, (i64, i64)>,
}

impl Newest {
    pub const FILE: &'static str = "newest.json";

    /// What the capture records, or nothing where no sweep has run.
    ///
    /// # Errors
    ///
    /// Fails if the file exists and cannot be parsed.
    pub fn load(snapshot: &Path) -> Result<Self> {
        match std::fs::read(snapshot.join(Self::FILE)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// # Errors
    ///
    /// Fails if the capture directory cannot be written to.
    pub fn save(&self, snapshot: &Path) -> Result<()> {
        std::fs::write(snapshot.join(Self::FILE), serde_json::to_vec(self)?)?;
        Ok(())
    }

    /// Notes a copy a sweep wrote. A later edit wins; the same edit fetched twice belongs to
    /// the later sweep, so exactly one copy counts however often it was fetched.
    pub fn record(&mut self, id: &str, updated: i64, sweep: i64) {
        match self.copies.get_mut(id) {
            Some(held) if (updated, sweep) <= *held => {}
            Some(held) => *held = (updated, sweep),
            None => {
                self.copies.insert(id.to_owned(), (updated, sweep));
            }
        }
    }

    /// Whether a row is the copy of its review that counts. `file` is the sweep that wrote
    /// the row, or zero for the crawl's own shards.
    #[must_use]
    pub fn counts(&self, id: &str, updated: i64, file: i64) -> bool {
        self.copies
            .get(id)
            .is_none_or(|&(newest, sweep)| (updated, file) == (newest, sweep))
    }
}

/// The rows of one batch that count, given the sweeps that came after the file they are in.
struct Kept<'a> {
    newest: &'a Newest,
    ids: &'a StringArray,
    updated: Option<&'a Int64Array>,
    file: i64,
}

impl Kept<'_> {
    fn row(&self, row: usize) -> bool {
        if self.newest.copies.is_empty() {
            return true;
        }
        let updated = self
            .updated
            .filter(|column| !column.is_null(row))
            .map_or(0, |column| column.value(row));
        self.newest.counts(self.ids.value(row), updated, self.file)
    }
}

/// Walks every batch of the capture, crawl shards first and then each sweep in order, with
/// the rows that count marked. Every reader of the capture comes through here, so there is
/// one place that knows what a capture is made of.
///
/// Only `columns` are decoded, beside the two that decide which rows count. Every row also
/// carries its `raw_json`, as large as the review itself, and no pass that walks the capture
/// reads it.
fn each_batch(
    snapshot: &Path,
    columns: &[&str],
    mut visit: impl FnMut(&RecordBatch, &Kept<'_>) -> Result<()>,
) -> Result<()> {
    let newest = Newest::load(snapshot)?;
    for (shard, file) in shards_of(snapshot)? {
        each_batch_of(&shard, file, &newest, columns, &mut visit)?;
    }
    Ok(())
}

fn each_batch_of(
    shard: &Path,
    file: i64,
    newest: &Newest,
    columns: &[&str],
    mut visit: impl FnMut(&RecordBatch, &Kept<'_>) -> Result<()>,
) -> Result<()> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(shard)?)?;
    let wanted = ProjectionMask::columns(
        builder.parquet_schema(),
        columns
            .iter()
            .copied()
            .chain(["recommendationid", "timestamp_updated"]),
    );
    let reader = builder
        .with_projection(wanted)
        .with_batch_size(8192)
        .build()?;
    for batch in reader {
        let batch = batch?;
        let ids = batch
            .column_by_name("recommendationid")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>())
            .ok_or(Error::MalformedPayload {
                field: "recommendationid",
            })?;
        let updated = batch
            .column_by_name("timestamp_updated")
            .and_then(|column| column.as_any().downcast_ref::<Int64Array>());
        visit(
            &batch,
            &Kept {
                newest,
                ids,
                updated,
                file,
            },
        )?;
    }
    Ok(())
}

/// Reviews per Parquet row group, which is what the writer buffers before flushing.
const REVIEWS_PER_ROW_GROUP: usize = 65_536;

#[must_use]
pub fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("recommendationid", DataType::Utf8, false),
        Field::new("appid", DataType::UInt32, false),
        Field::new("author_steamid", DataType::Utf8, true),
        Field::new("author_num_games_owned", DataType::UInt32, true),
        Field::new("author_num_reviews", DataType::UInt32, true),
        Field::new("author_playtime_forever", DataType::UInt32, true),
        Field::new("author_playtime_at_review", DataType::UInt32, true),
        Field::new("author_playtime_last_two_weeks", DataType::UInt32, true),
        Field::new("author_last_played", DataType::Int64, true),
        Field::new("language", DataType::Utf8, true),
        Field::new("review", DataType::Utf8, true),
        Field::new("timestamp_created", DataType::Int64, true),
        Field::new("timestamp_updated", DataType::Int64, true),
        Field::new("voted_up", DataType::Boolean, true),
        Field::new("votes_up", DataType::UInt32, true),
        Field::new("votes_funny", DataType::UInt32, true),
        Field::new("weighted_vote_score", DataType::Float64, true),
        Field::new("comment_count", DataType::UInt32, true),
        Field::new("steam_purchase", DataType::Boolean, true),
        Field::new("received_for_free", DataType::Boolean, true),
        Field::new("written_during_early_access", DataType::Boolean, true),
        Field::new("refunded", DataType::Boolean, true),
        Field::new("primarily_steam_deck", DataType::Boolean, true),
        Field::new("raw_json", DataType::Utf8, false),
    ]))
}

/// Writes reviews to a single Parquet file.
///
/// The file is written under a name no reader looks for and given its own only once closed,
/// so a crawl or a sweep that is stopped, fails or dies with the machine never leaves a file
/// without a footer where every read of the capture would trip over it.
#[derive(Debug)]
pub struct CaptureWriter {
    writer: ArrowWriter<File>,
    schema: Arc<Schema>,
    app_id: u32,
    rows: u64,
    partial: std::path::PathBuf,
    path: std::path::PathBuf,
}

/// Where a capture file is written until it is complete.
#[must_use]
pub fn partial_of(path: &Path) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".partial");
    std::path::PathBuf::from(name)
}

impl CaptureWriter {
    /// # Errors
    ///
    /// Fails if the parent directory cannot be created, the file cannot be opened, or the
    /// Parquet writer rejects the schema.
    pub fn create(path: &Path, app_id: u32) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let schema = schema();
        // Buffered whole before it reaches the disk, so this bounds what a shard holding
        // tens of thousands of reviews costs in memory.
        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(ZstdLevel::default()))
            .set_max_row_group_row_count(Some(REVIEWS_PER_ROW_GROUP))
            .build();
        let partial = partial_of(path);
        let writer =
            ArrowWriter::try_new(File::create(&partial)?, Arc::clone(&schema), Some(props))?;
        Ok(Self {
            writer,
            schema,
            app_id,
            rows: 0,
            partial,
            path: path.to_path_buf(),
        })
    }

    /// # Errors
    ///
    /// Fails if a review has no `recommendationid`, or if Arrow or Parquet reject the batch.
    pub fn write(&mut self, reviews: &[&Value]) -> Result<()> {
        if reviews.is_empty() {
            return Ok(());
        }
        let mut builders = RowBuilders::with_capacity(reviews.len());
        for review in reviews {
            builders.push(self.app_id, review)?;
        }
        self.writer.write(&builders.finish(&self.schema)?)?;
        self.rows += reviews.len() as u64;
        Ok(())
    }

    #[must_use]
    pub fn rows(&self) -> u64 {
        self.rows
    }

    /// # Errors
    ///
    /// Fails if the Parquet footer cannot be written or the file cannot be given its name.
    pub fn close(self) -> Result<u64> {
        self.writer.close()?;
        std::fs::rename(&self.partial, &self.path)?;
        Ok(self.rows)
    }
}

struct RowBuilders {
    recommendationid: StringBuilder,
    appid: UInt32Builder,
    author_steamid: StringBuilder,
    num_games_owned: UInt32Builder,
    num_reviews: UInt32Builder,
    playtime_forever: UInt32Builder,
    playtime_at_review: UInt32Builder,
    playtime_last_two_weeks: UInt32Builder,
    last_played: Int64Builder,
    language: StringBuilder,
    review: StringBuilder,
    created: Int64Builder,
    updated: Int64Builder,
    voted_up: BooleanBuilder,
    votes_up: UInt32Builder,
    votes_funny: UInt32Builder,
    weighted_vote_score: Float64Builder,
    comment_count: UInt32Builder,
    steam_purchase: BooleanBuilder,
    received_for_free: BooleanBuilder,
    early_access: BooleanBuilder,
    refunded: BooleanBuilder,
    steam_deck: BooleanBuilder,
    raw_json: StringBuilder,
}

impl RowBuilders {
    fn with_capacity(n: usize) -> Self {
        Self {
            recommendationid: StringBuilder::with_capacity(n, n * 12),
            appid: UInt32Builder::with_capacity(n),
            author_steamid: StringBuilder::with_capacity(n, n * 18),
            num_games_owned: UInt32Builder::with_capacity(n),
            num_reviews: UInt32Builder::with_capacity(n),
            playtime_forever: UInt32Builder::with_capacity(n),
            playtime_at_review: UInt32Builder::with_capacity(n),
            playtime_last_two_weeks: UInt32Builder::with_capacity(n),
            last_played: Int64Builder::with_capacity(n),
            language: StringBuilder::with_capacity(n, n * 8),
            review: StringBuilder::with_capacity(n, n * 200),
            created: Int64Builder::with_capacity(n),
            updated: Int64Builder::with_capacity(n),
            voted_up: BooleanBuilder::with_capacity(n),
            votes_up: UInt32Builder::with_capacity(n),
            votes_funny: UInt32Builder::with_capacity(n),
            weighted_vote_score: Float64Builder::with_capacity(n),
            comment_count: UInt32Builder::with_capacity(n),
            steam_purchase: BooleanBuilder::with_capacity(n),
            received_for_free: BooleanBuilder::with_capacity(n),
            early_access: BooleanBuilder::with_capacity(n),
            refunded: BooleanBuilder::with_capacity(n),
            steam_deck: BooleanBuilder::with_capacity(n),
            raw_json: StringBuilder::with_capacity(n, n * 700),
        }
    }

    fn push(&mut self, app_id: u32, r: &Value) -> Result<()> {
        let id = text(r.get("recommendationid")).ok_or(Error::MalformedPayload {
            field: "recommendationid",
        })?;
        self.recommendationid.append_value(id);
        self.appid.append_value(app_id);

        let author = r.get("author");
        self.author_steamid
            .append_option(text(author.and_then(|a| a.get("steamid"))));
        self.num_games_owned
            .append_option(number(author.and_then(|a| a.get("num_games_owned"))));
        self.num_reviews
            .append_option(number(author.and_then(|a| a.get("num_reviews"))));
        self.playtime_forever
            .append_option(number(author.and_then(|a| a.get("playtime_forever"))));
        self.playtime_at_review
            .append_option(number(author.and_then(|a| a.get("playtime_at_review"))));
        self.playtime_last_two_weeks.append_option(number(
            author.and_then(|a| a.get("playtime_last_two_weeks")),
        ));
        self.last_played.append_option(
            author
                .and_then(|a| a.get("last_played"))
                .and_then(Value::as_i64),
        );

        self.language.append_option(text(r.get("language")));
        self.review.append_option(text(r.get("review")));
        self.created
            .append_option(r.get("timestamp_created").and_then(Value::as_i64));
        self.updated
            .append_option(r.get("timestamp_updated").and_then(Value::as_i64));
        self.voted_up
            .append_option(r.get("voted_up").and_then(Value::as_bool));
        self.votes_up.append_option(number(r.get("votes_up")));
        self.votes_funny.append_option(number(r.get("votes_funny")));
        self.weighted_vote_score
            .append_option(decimal(r.get("weighted_vote_score")));
        self.comment_count
            .append_option(number(r.get("comment_count")));
        self.steam_purchase
            .append_option(r.get("steam_purchase").and_then(Value::as_bool));
        self.received_for_free
            .append_option(r.get("received_for_free").and_then(Value::as_bool));
        self.early_access.append_option(
            r.get("written_during_early_access")
                .and_then(Value::as_bool),
        );
        self.refunded
            .append_option(r.get("refunded").and_then(Value::as_bool));
        self.steam_deck
            .append_option(r.get("primarily_steam_deck").and_then(Value::as_bool));

        self.raw_json.append_value(r.to_string());
        Ok(())
    }

    fn finish(mut self, schema: &Arc<Schema>) -> Result<RecordBatch> {
        let columns: Vec<ArrayRef> = vec![
            Arc::new(self.recommendationid.finish()),
            Arc::new(self.appid.finish()),
            Arc::new(self.author_steamid.finish()),
            Arc::new(self.num_games_owned.finish()),
            Arc::new(self.num_reviews.finish()),
            Arc::new(self.playtime_forever.finish()),
            Arc::new(self.playtime_at_review.finish()),
            Arc::new(self.playtime_last_two_weeks.finish()),
            Arc::new(self.last_played.finish()),
            Arc::new(self.language.finish()),
            Arc::new(self.review.finish()),
            Arc::new(self.created.finish()),
            Arc::new(self.updated.finish()),
            Arc::new(self.voted_up.finish()),
            Arc::new(self.votes_up.finish()),
            Arc::new(self.votes_funny.finish()),
            Arc::new(self.weighted_vote_score.finish()),
            Arc::new(self.comment_count.finish()),
            Arc::new(self.steam_purchase.finish()),
            Arc::new(self.received_for_free.finish()),
            Arc::new(self.early_access.finish()),
            Arc::new(self.refunded.finish()),
            Arc::new(self.steam_deck.finish()),
            Arc::new(self.raw_json.finish()),
        ];
        Ok(RecordBatch::try_new(Arc::clone(schema), columns)?)
    }
}

/// Reads back the text of specific reviews, by id.
///
/// Takes the ids it wants rather than returning the corpus, because a caller that needs a
/// few hundred reviews out of a million should not pay for the other million.
///
/// # Errors
///
/// Fails if a shard cannot be read.
pub fn texts_for<S: std::hash::BuildHasher + Sync>(
    snapshot: &Path,
    ids: &std::collections::HashSet<String, S>,
) -> Result<HashMap<String, String>> {
    let found = side_by_side(snapshot, &["review"], |batch, kept, into| {
        let column = |name: &'static str| -> Result<&StringArray> {
            batch
                .column_by_name(name)
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or(Error::MalformedPayload { field: name })
        };
        let review_ids = column("recommendationid")?;
        let bodies = column("review")?;
        for row in 0..batch.num_rows() {
            if bodies.is_null(row) || !kept.row(row) {
                continue;
            }
            let id = review_ids.value(row);
            if ids.contains(id) {
                into.push((id.to_owned(), bodies.value(row).to_owned()));
            }
        }
        Ok(())
    })?;
    Ok(found.into_iter().collect())
}

/// Reviews taken as "the top of the pile" when measuring helpfulness bias. Steam's own
/// default view shows a page of this order, which is what most people actually read.
pub const DEFAULT_TOP_HELPFUL: usize = 50;

/// One captured review, without its text, for passes that walk the whole corpus.
///
/// The text arrives beside this rather than inside it, because the passes that hold rows in
/// memory hold hundreds of thousands of them and the text is the only field big enough to
/// matter.
#[derive(Debug, Clone)]
pub struct Row {
    pub recommendationid: String,
    /// Steam's own helpfulness score, which is what orders the page people read.
    pub helpfulness: f64,
    pub votes_up: u32,
    pub voted_up: bool,
    pub language: String,
    pub created: i64,
    pub reviewer: crate::who::Reviewer,
}

/// Streams every review with the fields a counting pass needs, and its text.
///
/// # Errors
///
/// Fails if a shard cannot be read, or if the visitor does.
pub fn for_each_row(snapshot: &Path, mut visit: impl FnMut(Row, &str) -> Result<()>) -> Result<()> {
    each_batch(snapshot, &ROW_COLUMNS, |batch, kept| {
        rows_of(batch, kept, &mut |_, row, text| visit(row, text))
    })
}

/// [`for_each_row`], with the Steam id of each review's writer beside it, which is what a link
/// to the review on Steam is made from; empty where Steam did not say.
///
/// # Errors
///
/// Fails if a shard cannot be read, or if the visitor does.
pub fn for_each_row_with_author(
    snapshot: &Path,
    mut visit: impl FnMut(Row, &str, &str) -> Result<()>,
) -> Result<()> {
    let mut columns = ROW_COLUMNS.to_vec();
    columns.push("author_steamid");
    each_batch(snapshot, &columns, |batch, kept| {
        let authors = batch
            .column_by_name("author_steamid")
            .and_then(|column| column.as_any().downcast_ref::<StringArray>());
        rows_of(batch, kept, &mut |at, row, text| {
            let author = authors
                .filter(|column| !column.is_null(at))
                .map_or("", |column| column.value(at));
            visit(row, author, text)
        })
    })
}

/// What `keep` keeps of each review, in capture order, with the shards walked side by side.
///
/// For a pass somebody is waiting on: the largest game's million reviews take seconds one
/// shard after another and a fraction of that on every core. `keep` sees every row
/// [`for_each_row`] would, from several threads and in no particular order.
///
/// # Errors
///
/// Fails if a shard cannot be read.
pub fn rows_kept<T: Send>(
    snapshot: &Path,
    keep: impl Fn(Row, &str) -> Option<T> + Sync,
) -> Result<Vec<T>> {
    side_by_side(snapshot, &ROW_COLUMNS, |batch, kept, into| {
        rows_of(batch, kept, &mut |_, row, text| {
            into.extend(keep(row, text));
            Ok(())
        })
    })
}

/// What `take` takes from every batch, in capture order, with the shards read side by side:
/// [`each_batch`] for a pass somebody is waiting on.
fn side_by_side<T: Send>(
    snapshot: &Path,
    columns: &[&str],
    take: impl Fn(&RecordBatch, &Kept<'_>, &mut Vec<T>) -> Result<()> + Sync,
) -> Result<Vec<T>> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let newest = Newest::load(snapshot)?;
    let shards = shards_of(snapshot)?;
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(shards.len())
        .max(1);
    let mut walked: Vec<(usize, Result<Vec<T>>)> = std::thread::scope(|scope| {
        let running: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let order = next.fetch_add(1, Ordering::Relaxed);
                        let Some((shard, file)) = shards.get(order) else {
                            return mine;
                        };
                        let mut taken = Vec::new();
                        let read = each_batch_of(shard, *file, &newest, columns, |batch, kept| {
                            take(batch, kept, &mut taken)
                        });
                        mine.push((order, read.map(|()| taken)));
                    }
                })
            })
            .collect();
        running
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });
    walked.sort_by_key(|(order, _)| *order);
    let mut kept = Vec::new();
    for (_, shard) in walked {
        kept.extend(shard?);
    }
    Ok(kept)
}

/// The columns a [`Row`] and its text are made from.
const ROW_COLUMNS: [&str; 10] = [
    "review",
    "language",
    "weighted_vote_score",
    "votes_up",
    "voted_up",
    "timestamp_created",
    "author_playtime_at_review",
    "primarily_steam_deck",
    "written_during_early_access",
    "received_for_free",
];

/// Hands `visit` each kept row of a batch with its position in the batch, so a caller that read
/// a further column can find the row's value in it.
fn rows_of(
    batch: &RecordBatch,
    kept: &Kept<'_>,
    visit: &mut impl FnMut(usize, Row, &str) -> Result<()>,
) -> Result<()> {
    use arrow::array::{BooleanArray, Float64Array, UInt32Array};

    let field = |name: &'static str| -> Result<&dyn Array> {
        batch
            .column_by_name(name)
            .map(AsRef::as_ref)
            .ok_or(Error::MalformedPayload { field: name })
    };
    let cast = |name: &'static str| -> Result<&StringArray> {
        field(name)?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or(Error::MalformedPayload { field: name })
    };
    let ids = cast("recommendationid")?;
    let texts = cast("review")?;
    let languages = cast("language")?;
    let helpful = field("weighted_vote_score")?
        .as_any()
        .downcast_ref::<Float64Array>()
        .ok_or(Error::MalformedPayload {
            field: "weighted_vote_score",
        })?;
    let votes = field("votes_up")?
        .as_any()
        .downcast_ref::<UInt32Array>()
        .ok_or(Error::MalformedPayload { field: "votes_up" })?;
    let recommended = field("voted_up")?
        .as_any()
        .downcast_ref::<BooleanArray>()
        .ok_or(Error::MalformedPayload { field: "voted_up" })?;
    let created = field("timestamp_created")?
        .as_any()
        .downcast_ref::<Int64Array>()
        .ok_or(Error::MalformedPayload {
            field: "timestamp_created",
        })?;
    // Optional rather than required: a capture written by hand for a test, or by a build that
    // never asked Steam for them, still counts, with its reviewers unknown.
    let played = batch
        .column_by_name("author_playtime_at_review")
        .and_then(|column| column.as_any().downcast_ref::<UInt32Array>());
    let flag = |name: &str| {
        batch
            .column_by_name(name)
            .and_then(|column| column.as_any().downcast_ref::<BooleanArray>())
    };
    let (deck, early, free) = (
        flag("primarily_steam_deck"),
        flag("written_during_early_access"),
        flag("received_for_free"),
    );
    let set = |column: Option<&BooleanArray>, row: usize| {
        column.is_some_and(|column| !column.is_null(row) && column.value(row))
    };

    for row in 0..batch.num_rows() {
        if texts.is_null(row) || texts.value(row).trim().is_empty() || !kept.row(row) {
            continue;
        }
        let body = texts.value(row);
        visit(
            row,
            Row {
                recommendationid: ids.value(row).to_owned(),
                helpfulness: if helpful.is_null(row) {
                    0.0
                } else {
                    helpful.value(row)
                },
                votes_up: if votes.is_null(row) {
                    0
                } else {
                    votes.value(row)
                },
                voted_up: !recommended.is_null(row) && recommended.value(row),
                language: if languages.is_null(row) {
                    String::new()
                } else {
                    languages.value(row).to_owned()
                },
                created: if created.is_null(row) {
                    0
                } else {
                    created.value(row)
                },
                reviewer: crate::who::Reviewer {
                    played_minutes: played
                        .filter(|column| !column.is_null(row))
                        .map(|column| column.value(row)),
                    deck: set(deck, row),
                    early_access: set(early, row),
                    free: set(free, row),
                },
            },
            body,
        )?;
    }
    Ok(())
}

/// Streams every review's id, language and text, in capture order.
///
/// Unlike [`texts_for`] this holds nothing: a corpus of a million reviews is several
/// gigabytes of text, and every pass that walks all of it has to be able to walk away from
/// what it has already seen.
///
/// # Errors
///
/// Fails if a shard cannot be read, or if the visitor does.
pub fn for_each_body(
    snapshot: &Path,
    mut visit: impl FnMut(&str, &str, &str) -> Result<()>,
) -> Result<()> {
    each_batch(snapshot, &["language", "review"], |batch, kept| {
        let column = |name: &'static str| -> Result<&StringArray> {
            batch
                .column_by_name(name)
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or(Error::MalformedPayload { field: name })
        };
        let ids = column("recommendationid")?;
        let languages = column("language")?;
        let bodies = column("review")?;
        for row in 0..batch.num_rows() {
            if bodies.is_null(row) || !kept.row(row) {
                continue;
            }
            let language = if languages.is_null(row) {
                ""
            } else {
                languages.value(row)
            };
            visit(ids.value(row), language, bodies.value(row))?;
        }
        Ok(())
    })
}

/// One captured review, with everything a reader needs to judge it for themselves.
#[derive(Debug, Clone)]
pub struct CapturedReview {
    pub id: String,
    pub text: String,
    pub language: String,
    /// Steam's own id for the author, which is what a link back to the review needs.
    pub author_steamid: String,
    pub voted_up: bool,
    pub votes_up: u32,
    pub votes_funny: u32,
    pub playtime_at_review_minutes: u32,
    pub created: i64,
}

/// Fetches whole reviews by id, for the handful a report actually shows.
///
/// Takes the ids it wants for the same reason [`texts_for`] does: a report shows a few
/// hundred reviews out of a million, and reading the million to find them costs the memory
/// the streaming passes were built to avoid.
///
/// # Errors
///
/// Fails if a shard cannot be read.
pub fn reviews_for<S: std::hash::BuildHasher + Sync>(
    snapshot: &Path,
    ids: &std::collections::HashSet<String, S>,
) -> Result<HashMap<String, CapturedReview>> {
    use arrow::array::{BooleanArray, UInt32Array};

    let columns = [
        "review",
        "language",
        "author_steamid",
        "votes_up",
        "votes_funny",
        "author_playtime_at_review",
        "voted_up",
        "timestamp_created",
    ];
    let found = side_by_side(snapshot, &columns, |batch, kept, into| {
        let strings = |name: &'static str| -> Result<&StringArray> {
            batch
                .column_by_name(name)
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .ok_or(Error::MalformedPayload { field: name })
        };
        let counts = |name: &'static str| -> Result<&UInt32Array> {
            batch
                .column_by_name(name)
                .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
                .ok_or(Error::MalformedPayload { field: name })
        };
        let review_ids = strings("recommendationid")?;
        let bodies = strings("review")?;
        let languages = strings("language")?;
        let authors = strings("author_steamid")?;
        let votes_up = counts("votes_up")?;
        let votes_funny = counts("votes_funny")?;
        let playtime = counts("author_playtime_at_review")?;
        let recommended = batch
            .column_by_name("voted_up")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
            .ok_or(Error::MalformedPayload { field: "voted_up" })?;
        let created = batch
            .column_by_name("timestamp_created")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
            .ok_or(Error::MalformedPayload {
                field: "timestamp_created",
            })?;

        for row in 0..batch.num_rows() {
            if bodies.is_null(row) || !kept.row(row) {
                continue;
            }
            let id = review_ids.value(row);
            if !ids.contains(id) {
                continue;
            }
            let string = |array: &StringArray| {
                if array.is_null(row) {
                    String::new()
                } else {
                    array.value(row).to_owned()
                }
            };
            let count = |array: &UInt32Array| {
                if array.is_null(row) {
                    0
                } else {
                    array.value(row)
                }
            };
            into.push((
                id.to_owned(),
                CapturedReview {
                    id: id.to_owned(),
                    text: bodies.value(row).to_owned(),
                    language: string(languages),
                    author_steamid: string(authors),
                    voted_up: !recommended.is_null(row) && recommended.value(row),
                    votes_up: count(votes_up),
                    votes_funny: count(votes_funny),
                    playtime_at_review_minutes: count(playtime),
                    created: if created.is_null(row) {
                        0
                    } else {
                        created.value(row)
                    },
                },
            ));
        }
        Ok(())
    })?;
    Ok(found.into_iter().collect())
}

/// What a published dataset says about one review: everything Steam shows about it except its
/// words and its author, and the fingerprint of the words that were labelled.
///
/// The text is the reviewer's and the author is a person; neither is ours to publish. The
/// fingerprint is what lets anybody who fetches the review from Steam know whether it is still
/// the text the labels were written against, since a review edited since may have moved every
/// byte offset a label names.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Steam's own fields, kept as Steam names them"
)]
pub struct ReviewFacts {
    pub review_id: String,
    pub language: String,
    pub created: i64,
    pub updated: i64,
    pub voted_up: bool,
    pub votes_up: u32,
    pub votes_funny: u32,
    pub weighted_vote_score: f64,
    pub comment_count: u32,
    pub steam_purchase: bool,
    pub received_for_free: bool,
    pub written_during_early_access: bool,
    pub refunded: bool,
    pub primarily_steam_deck: bool,
    /// Shown on the review itself; the account's other playtimes are not.
    pub playtime_at_review_minutes: u32,
    /// SHA-256 of the review text as UTF-8, the bytes every label's offsets count.
    pub text_sha256: String,
    pub text_bytes: usize,
}

/// The [`ReviewFacts`] of the reviews named, from the copy of each that counts.
///
/// # Errors
///
/// Fails if a shard cannot be read.
pub fn facts_for<S: std::hash::BuildHasher + Sync>(
    snapshot: &Path,
    ids: &std::collections::HashSet<String, S>,
) -> Result<HashMap<String, ReviewFacts>> {
    use arrow::array::{BooleanArray, Float64Array, UInt32Array};

    let columns = [
        "review",
        "language",
        "timestamp_created",
        "voted_up",
        "votes_up",
        "votes_funny",
        "weighted_vote_score",
        "comment_count",
        "steam_purchase",
        "received_for_free",
        "written_during_early_access",
        "refunded",
        "primarily_steam_deck",
        "author_playtime_at_review",
    ];
    let found = side_by_side(snapshot, &columns, |batch, kept, into| {
        fn column<'a, T: 'static>(batch: &'a RecordBatch, name: &'static str) -> Result<&'a T> {
            batch
                .column_by_name(name)
                .and_then(|c| c.as_any().downcast_ref::<T>())
                .ok_or(Error::MalformedPayload { field: name })
        }
        let review_ids = column::<StringArray>(batch, "recommendationid")?;
        let bodies = column::<StringArray>(batch, "review")?;
        let languages = column::<StringArray>(batch, "language")?;
        let created = column::<Int64Array>(batch, "timestamp_created")?;
        let updated = column::<Int64Array>(batch, "timestamp_updated")?;
        let counts = |name| column::<UInt32Array>(batch, name);
        let flags = |name| column::<BooleanArray>(batch, name);
        let (votes_up, votes_funny, comments, playtime) = (
            counts("votes_up")?,
            counts("votes_funny")?,
            counts("comment_count")?,
            counts("author_playtime_at_review")?,
        );
        let (recommends, purchased, free, early, refunded, deck) = (
            flags("voted_up")?,
            flags("steam_purchase")?,
            flags("received_for_free")?,
            flags("written_during_early_access")?,
            flags("refunded")?,
            flags("primarily_steam_deck")?,
        );
        let score = column::<Float64Array>(batch, "weighted_vote_score")?;

        for row in 0..batch.num_rows() {
            if bodies.is_null(row) || !kept.row(row) {
                continue;
            }
            let id = review_ids.value(row);
            if !ids.contains(id) {
                continue;
            }
            let count = |array: &UInt32Array| (!array.is_null(row)).then(|| array.value(row));
            let flag = |array: &BooleanArray| !array.is_null(row) && array.value(row);
            let time = |array: &Int64Array| (!array.is_null(row)).then(|| array.value(row));
            let text = bodies.value(row);
            into.push((
                id.to_owned(),
                ReviewFacts {
                    review_id: id.to_owned(),
                    language: if languages.is_null(row) {
                        String::new()
                    } else {
                        languages.value(row).to_owned()
                    },
                    created: time(created).unwrap_or(0),
                    updated: time(updated).unwrap_or(0),
                    voted_up: flag(recommends),
                    votes_up: count(votes_up).unwrap_or(0),
                    votes_funny: count(votes_funny).unwrap_or(0),
                    weighted_vote_score: if score.is_null(row) {
                        0.0
                    } else {
                        score.value(row)
                    },
                    comment_count: count(comments).unwrap_or(0),
                    steam_purchase: flag(purchased),
                    received_for_free: flag(free),
                    written_during_early_access: flag(early),
                    refunded: flag(refunded),
                    primarily_steam_deck: flag(deck),
                    playtime_at_review_minutes: count(playtime).unwrap_or(0),
                    text_sha256: crate::embed::sha256_hex(text),
                    text_bytes: text.len(),
                },
            ));
        }
        Ok(())
    })?;
    Ok(found.into_iter().collect())
}

/// The name of the file a sweep writes, from when it started.
#[must_use]
pub fn sweep_file(started: i64) -> String {
    format!("sweep-{started}.parquet")
}

/// The capture's files in the order they count: the crawl's shards, then each sweep from the
/// earliest, each with the sweep that wrote it, or zero for the crawl's own.
fn shards_of(snapshot: &Path) -> Result<Vec<(std::path::PathBuf, i64)>> {
    let mut shards: Vec<(std::path::PathBuf, i64)> = std::fs::read_dir(snapshot)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let stem = name.strip_suffix(".parquet")?;
            if stem.starts_with("shard-") {
                Some((path, 0))
            } else {
                let started = stem.strip_prefix("sweep-")?.parse::<i64>().ok()?;
                Some((path, started))
            }
        })
        .collect();
    shards.sort();
    Ok(shards)
}

fn text(v: Option<&Value>) -> Option<&str> {
    v?.as_str()
}

fn number(v: Option<&Value>) -> Option<u32> {
    u32::try_from(v?.as_u64()?).ok()
}

/// `weighted_vote_score` arrives as a JSON string on populated reviews and as a number when
/// it is zero, so neither `as_f64` nor `as_str` alone is enough.
fn decimal(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn weighted_vote_score_accepts_both_shapes() {
        assert_eq!(decimal(Some(&json!("0.5238095"))), Some(0.523_809_5));
        assert_eq!(decimal(Some(&json!(0))), Some(0.0));
        assert_eq!(decimal(Some(&json!(null))), None);
        assert_eq!(decimal(None), None);
    }

    #[test]
    fn a_capture_file_is_only_read_once_it_is_whole() {
        let dir = crate::tempdir::Dir::new();
        let path = dir.path().join("sweep-17.parquet");
        let review = json!({ "recommendationid": "1", "review": "Fine.", "language": "english" });
        let mut writer = CaptureWriter::create(&path, 7).unwrap();
        writer.write(&[&review]).unwrap();
        assert!(
            shards_of(dir.path()).unwrap().is_empty(),
            "a file still being written, or left by a sweep that never finished, is no shard"
        );
        assert_eq!(writer.close().unwrap(), 1);
        assert_eq!(shards_of(dir.path()).unwrap(), vec![(path.clone(), 17)]);
        assert!(!partial_of(&path).exists());
    }

    #[test]
    fn oversized_counts_do_not_abort_a_row() {
        assert_eq!(number(Some(&json!(u64::from(u32::MAX) + 1))), None);
        assert_eq!(number(Some(&json!(42))), Some(42));
    }

    #[test]
    fn a_review_without_an_id_is_rejected() {
        let mut builders = RowBuilders::with_capacity(1);
        let err = builders
            .push(1, &json!({"review": "no id here"}))
            .unwrap_err();
        assert!(matches!(
            err,
            Error::MalformedPayload {
                field: "recommendationid"
            }
        ));
    }

    #[test]
    fn missing_fields_become_nulls_rather_than_failing() {
        let mut builders = RowBuilders::with_capacity(1);
        builders
            .push(1_091_500, &json!({"recommendationid": "1", "review": "ok"}))
            .unwrap();
        let batch = builders.finish(&schema()).unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), schema().fields().len());
    }

    #[test]
    fn raw_json_preserves_fields_the_schema_does_not_model() {
        let mut builders = RowBuilders::with_capacity(1);
        let review = json!({"recommendationid": "7", "some_future_field": [1, 2, 3]});
        builders.push(1, &review).unwrap();
        let batch = builders.finish(&schema()).unwrap();
        let raw = batch
            .column_by_name("raw_json")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap()
            .value(0);
        assert!(raw.contains("some_future_field"), "{raw}");
    }

    /// A scratch capture directory, removed on drop.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "steamgauge-capture-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn review(id: &str, text: &str, created: i64, updated: i64) -> Value {
        json!({
            "recommendationid": id,
            "review": text,
            "language": "english",
            "timestamp_created": created,
            "timestamp_updated": updated,
            "voted_up": true,
            "votes_up": 1,
        })
    }

    fn write(path: &Path, reviews: &[Value]) {
        let mut writer = CaptureWriter::create(path, 1).unwrap();
        let borrowed: Vec<&Value> = reviews.iter().collect();
        writer.write(&borrowed).unwrap();
        writer.close().unwrap();
    }

    #[test]
    fn every_reader_counts_the_newest_copy_of_a_swept_review_and_only_that() {
        let scratch = Scratch::new("newest");
        let dir = &scratch.0;
        write(
            &dir.join("shard-0000.parquet"),
            &[
                review("1", "crashes on launch", 100, 100),
                review("2", "great game", 100, 100),
            ],
        );
        // The first sweep fetches an edit of 1 and a new review 3; the second sweep fetches
        // review 3 again, unchanged, because it was written while the first was running.
        write(
            &dir.join(sweep_file(500)),
            &[
                review("1", "fixed now, runs fine", 100, 450),
                review("3", "arrived later", 420, 420),
            ],
        );
        write(
            &dir.join(sweep_file(600)),
            &[review("3", "arrived later", 420, 420)],
        );
        let mut newest = Newest::default();
        newest.record("1", 450, 500);
        newest.record("3", 420, 500);
        newest.record("3", 420, 600);
        newest.save(dir).unwrap();

        let mut seen: Vec<(String, String)> = Vec::new();
        for_each_row(dir, |row, text| {
            seen.push((row.recommendationid, text.to_owned()));
            Ok(())
        })
        .unwrap();
        seen.sort();
        assert_eq!(
            seen,
            [
                ("1".to_owned(), "fixed now, runs fine".to_owned()),
                ("2".to_owned(), "great game".to_owned()),
                ("3".to_owned(), "arrived later".to_owned()),
            ]
        );

        let mut bodies = 0;
        for_each_body(dir, |_, _, _| {
            bodies += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(bodies, 3);

        let wanted: std::collections::HashSet<String> = ["1", "3"].map(str::to_owned).into();
        let fetched = reviews_for(dir, &wanted).unwrap();
        assert_eq!(fetched["1"].text, "fixed now, runs fine");
        let texts = texts_for(dir, &wanted).unwrap();
        assert_eq!(texts["1"], "fixed now, runs fine");
        assert_eq!(texts["3"], "arrived later");

        let kept = rows_kept(dir, |row, text| {
            Some((row.recommendationid, text.to_owned()))
        })
        .unwrap();
        assert_eq!(
            kept,
            [
                ("2".to_owned(), "great game".to_owned()),
                ("1".to_owned(), "fixed now, runs fine".to_owned()),
                ("3".to_owned(), "arrived later".to_owned()),
            ],
            "the rows that count, in the order the capture holds them"
        );
    }

    #[test]
    fn a_row_says_who_wrote_it_as_far_as_steam_said() {
        let scratch = Scratch::new("reviewer");
        let mut told = review("1", "played it a lot", 1, 1);
        told["author"] = json!({"steamid": "7", "playtime_at_review": 1234});
        told["primarily_steam_deck"] = json!(true);
        told["written_during_early_access"] = json!(true);
        told["received_for_free"] = json!(true);
        let mut denied = review("3", "said no to all of it", 1, 1);
        denied["primarily_steam_deck"] = json!(false);
        denied["written_during_early_access"] = json!(false);
        denied["received_for_free"] = json!(false);
        write(
            &scratch.0.join("shard-0000.parquet"),
            &[told, review("2", "said nothing about myself", 1, 1), denied],
        );
        let mut seen = Vec::new();
        for_each_row(&scratch.0, |row, _| {
            seen.push((row.recommendationid, row.reviewer));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            seen,
            [
                (
                    "1".to_owned(),
                    crate::who::Reviewer {
                        played_minutes: Some(1234),
                        deck: true,
                        early_access: true,
                        free: true,
                    }
                ),
                ("2".to_owned(), crate::who::Reviewer::default()),
                ("3".to_owned(), crate::who::Reviewer::default()),
            ]
        );
    }

    #[test]
    fn a_capture_without_the_reviewer_columns_still_counts_with_its_reviewers_unknown() {
        use arrow::array::{BooleanArray, Float64Array, UInt32Array};

        let scratch = Scratch::new("no-reviewer");
        let schema = Arc::new(Schema::new(vec![
            Field::new("recommendationid", DataType::Utf8, false),
            Field::new("review", DataType::Utf8, true),
            Field::new("language", DataType::Utf8, true),
            Field::new("weighted_vote_score", DataType::Float64, true),
            Field::new("votes_up", DataType::UInt32, true),
            Field::new("voted_up", DataType::Boolean, true),
            Field::new("timestamp_created", DataType::Int64, true),
            Field::new("timestamp_updated", DataType::Int64, true),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["1"])),
                Arc::new(StringArray::from(vec!["fine"])),
                Arc::new(StringArray::from(vec!["english"])),
                Arc::new(Float64Array::from(vec![0.5])),
                Arc::new(UInt32Array::from(vec![1])),
                Arc::new(BooleanArray::from(vec![true])),
                Arc::new(Int64Array::from(vec![1])),
                Arc::new(Int64Array::from(vec![1])),
            ],
        )
        .unwrap();
        let file = File::create(scratch.0.join("shard-0000.parquet")).unwrap();
        let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();

        let mut seen = Vec::new();
        for_each_row(&scratch.0, |row, _| {
            seen.push(row.reviewer);
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, [crate::who::Reviewer::default()]);
    }

    #[test]
    fn shards_read_side_by_side_come_back_in_capture_order() {
        let scratch = Scratch::new("side-by-side");
        let mut wanted = Vec::new();
        for shard in 0..24 {
            let reviews: Vec<Value> = (0..5)
                .map(|row| {
                    let id = (shard * 5 + row).to_string();
                    wanted.push(id.clone());
                    review(&id, "said", 1, 1)
                })
                .collect();
            write(
                &scratch.0.join(format!("shard-{shard:04}.parquet")),
                &reviews,
            );
        }

        let kept = rows_kept(&scratch.0, |row, _| Some(row.recommendationid)).unwrap();

        assert_eq!(kept, wanted);
    }

    #[test]
    fn a_reviews_published_facts_carry_neither_its_words_nor_its_author() {
        let scratch = Scratch::new("facts");
        let mut full = review("1", "Crashes on launch. ünïcode", 100, 200);
        full["author"] = json!({
            "steamid": "76561198000000000",
            "num_games_owned": 400,
            "playtime_forever": 9000,
            "playtime_at_review": 1234,
        });
        full["votes_funny"] = json!(3);
        full["weighted_vote_score"] = json!(0.75);
        full["comment_count"] = json!(2);
        full["steam_purchase"] = json!(true);
        full["refunded"] = json!(false);
        full["primarily_steam_deck"] = json!(true);
        write(
            &scratch.0.join("shard-0000.parquet"),
            &[full, review("2", "not asked for", 1, 1)],
        );

        let wanted: std::collections::HashSet<String> = ["1".to_owned()].into();
        let facts = facts_for(&scratch.0, &wanted).unwrap();

        assert_eq!(facts.len(), 1, "only the reviews named are written");
        let one = &facts["1"];
        assert_eq!((one.created, one.updated), (100, 200));
        assert_eq!(one.playtime_at_review_minutes, 1234);
        assert!(one.voted_up && one.steam_purchase && one.primarily_steam_deck && !one.refunded);
        assert_eq!((one.votes_funny, one.comment_count), (3, 2));
        assert_eq!(
            one.text_sha256,
            crate::embed::sha256_hex("Crashes on launch. ünïcode")
        );
        assert_eq!(one.text_bytes, "Crashes on launch. ünïcode".len());
        let written = serde_json::to_string(one).unwrap();
        for private in ["Crashes", "76561198000000000", "9000", "400"] {
            assert!(
                !written.contains(private),
                "{private} leaked into {written}"
            );
        }
    }

    #[test]
    fn a_capture_nobody_has_swept_counts_every_row_it_holds() {
        let scratch = Scratch::new("unswept");
        write(
            &scratch.0.join("shard-0000.parquet"),
            &[review("1", "one", 1, 1), review("2", "two", 2, 2)],
        );
        let mut rows = 0;
        for_each_row(&scratch.0, |_, _| {
            rows += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(rows, 2);
        assert!(Newest::load(&scratch.0).unwrap().copies.is_empty());
    }

    #[test]
    fn a_writer_counts_the_reviews_it_has_written() {
        let scratch = Scratch::new("rows");
        let mut writer = CaptureWriter::create(&scratch.0.join("shard-0000.parquet"), 1).unwrap();
        assert_eq!(writer.rows(), 0);
        let (one, two) = (review("1", "one", 1, 1), review("2", "two", 2, 2));
        writer.write(&[&one, &two]).unwrap();
        writer.write(&[]).unwrap();
        assert_eq!(writer.rows(), 2);
        assert_eq!(writer.close().unwrap(), 2);
    }

    #[test]
    fn a_review_without_words_is_given_by_no_reader() {
        let scratch = Scratch::new("wordless");
        let mut silent = review("2", "", 1, 1);
        silent.as_object_mut().unwrap().remove("review");
        write(
            &scratch.0.join("shard-0000.parquet"),
            &[review("1", "fine", 1, 1), silent, review("3", "   ", 1, 1)],
        );
        let wanted: std::collections::HashSet<String> = ["1", "2"].map(str::to_owned).into();

        let mut rows = Vec::new();
        for_each_row(&scratch.0, |row, _| {
            rows.push(row.recommendationid);
            Ok(())
        })
        .unwrap();
        assert_eq!(rows, ["1"], "a blank review is no row");
        assert_eq!(
            texts_for(&scratch.0, &wanted)
                .unwrap()
                .into_keys()
                .collect::<Vec<_>>(),
            ["1"]
        );
        assert_eq!(
            reviews_for(&scratch.0, &wanted)
                .unwrap()
                .into_keys()
                .collect::<Vec<_>>(),
            ["1"]
        );
        assert_eq!(
            facts_for(&scratch.0, &wanted)
                .unwrap()
                .into_keys()
                .collect::<Vec<_>>(),
            ["1"]
        );
    }

    #[test]
    fn a_review_recommends_the_game_only_where_it_says_so() {
        let scratch = Scratch::new("recommends");
        let mut against = review("2", "no", 1, 1);
        against["voted_up"] = json!(false);
        let mut unsaid = review("3", "maybe", 1, 1);
        unsaid.as_object_mut().unwrap().remove("voted_up");
        write(
            &scratch.0.join("shard-0000.parquet"),
            &[review("1", "yes", 1, 1), against, unsaid],
        );

        let mut rows = Vec::new();
        for_each_row(&scratch.0, |row, _| {
            rows.push((row.recommendationid, row.voted_up));
            Ok(())
        })
        .unwrap();
        let expected = [
            ("1".to_owned(), true),
            ("2".to_owned(), false),
            ("3".to_owned(), false),
        ];
        assert_eq!(rows, expected);

        let wanted: std::collections::HashSet<String> = ["1", "2", "3"].map(str::to_owned).into();
        let fetched = reviews_for(&scratch.0, &wanted).unwrap();
        for (id, recommends) in expected {
            assert_eq!(fetched[&id].voted_up, recommends, "review {id}");
        }
    }

    #[test]
    fn a_record_of_copies_that_cannot_be_read_is_an_error_rather_than_none() {
        let scratch = Scratch::new("unreadable");
        std::fs::create_dir(scratch.0.join(Newest::FILE)).unwrap();
        assert!(Newest::load(&scratch.0).is_err());
    }

    #[test]
    fn a_later_edit_wins_and_the_same_edit_belongs_to_the_later_sweep() {
        let mut newest = Newest::default();
        newest.record("1", 300, 10);
        newest.record("1", 200, 20);
        assert_eq!(
            newest.copies["1"],
            (300, 10),
            "an older edit does not displace a newer"
        );
        newest.record("1", 300, 30);
        assert_eq!(
            newest.copies["1"],
            (300, 30),
            "the same edit fetched again moves file"
        );
        assert!(newest.counts("1", 300, 30));
        assert!(!newest.counts("1", 300, 10));
        assert!(!newest.counts("1", 100, 0));
        assert!(newest.counts("never swept", 0, 0));
    }
}
