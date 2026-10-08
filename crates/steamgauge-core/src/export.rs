//! A read game's figures and points, as files a spreadsheet or someone's own analysis reads.
//!
//! Every table is written twice, `<name>.csv` and `<name>.json`, into a new folder with a
//! `README.txt` that says what each column holds, which reader read the game, when, and how far
//! its figures can be trusted. The figures are the game page's, counted by the page's rules: one
//! the page will not show as a number is left empty, and a column says why.
//!
//! Review text is written by strangers, and a spreadsheet runs a cell that starts like a formula.
//! So in the CSV a text starting with one of [`FORMULA_STARTS`] carries a `'` before it, which is
//! the standard defence; the JSON holds every text as it was written.

use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Write as _,
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::{
    Error, Result,
    html::{language_name, percent, thousands},
    measure::ClaimAgreement,
    read::{Month, ReadReport},
    reader::{Frozen, Polarity, Reading},
    report::Measurement,
    taxonomy::{Category, SHEET},
    updates::Update,
    who::{self, SegmentCount},
};

/// What a spreadsheet reads a cell as a formula from: `=`, `+`, `-` and `@` start one, and some
/// importers skip a tab or a carriage return before them.
const FORMULA_STARTS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

/// Excel reads a CSV file without this in the system's legacy code page, where every letter
/// outside it comes out as two or three wrong ones.
const BYTE_ORDER_MARK: &[u8] = "\u{feff}".as_bytes();

/// Decimal places a share is written to, so a subject one review in a million raises is not
/// written as none.
const PLACES: usize = 6;

/// Below this share of its labelled points found, the game page marks a subject's rate as a floor.
const FOUND_TOO_FEW: f64 = 0.25;

/// The rows a sheet in Excel holds.
const SPREADSHEET_ROWS: u64 = 1_048_576;

/// Reviews walked between two reports of how far an export has got.
const TELLS_EVERY: u64 = 1_000;

/// What the rows of everyone's reviews are called.
const EVERYONE: &str = "everyone";

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub folder: PathBuf,
    /// Points of the reading written out.
    pub points: u64,
    /// Points left out because their review no longer holds them where the reading found them:
    /// it was edited since it was read, or this build takes it apart differently.
    pub left_out: u64,
}

/// Writes a read game's data into a new folder at `to`: subjects, months, points and updates, each
/// as CSV and JSON, and a README.txt saying what they hold. Says how many reviews of the capture
/// it has walked, and of how many, as it writes the points.
///
/// The files are written into `<to>.partial` and the folder takes its name only once all of them
/// are whole, so a stopped or failed export leaves nothing that looks finished.
///
/// # Errors
///
/// Refused where something is already at `to`, or a `.partial` folder is left beside it; fails
/// where the game has not been read, or a file cannot be read or written; and stops, leaving
/// nothing, once `stop` is set.
pub fn game(
    out_dir: &Path,
    app_id: u32,
    to: &Path,
    stop: &AtomicBool,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<Exported> {
    if to.exists() {
        return Err(Error::Refused(format!(
            "{} is there already; choose a name nothing has yet",
            to.display()
        )));
    }
    let Some(name) = to.file_name() else {
        return Err(Error::Refused(format!("{} names no folder", to.display())));
    };
    let snapshot = crate::embed::latest_snapshot(out_dir, app_id)?;
    let reading = snapshot.join("reading.json");
    let report: ReadReport = serde_json::from_slice(&std::fs::read(&reading).map_err(|_| {
        Error::NoClassifications {
            path: reading.clone(),
        }
    })?)?;
    let facts = crate::report::crawl_facts(out_dir, app_id)?;
    let measurement = crate::report::agreement_for(
        app_id,
        out_dir,
        &crate::claimset::default_reference_dir(app_id),
    );
    let game = Game {
        app_id,
        title: facts.title(),
        report: &report,
        measurement: &measurement,
        read_unix: modified(&reading),
        exported_unix: unix_now(),
        swept_since: facts
            .swept_unix
            .filter(|swept| report.captured_unix < *swept),
        updates: crate::updates::kept(out_dir, app_id).map(|kept| kept.updates()),
    };

    let mut partial_name = name.to_os_string();
    partial_name.push(".partial");
    let partial = to.with_file_name(partial_name);
    if let Err(error) = std::fs::create_dir(&partial) {
        return Err(if error.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Refused(format!(
                "{} is left from an export that did not finish; remove it and export again",
                partial.display()
            ))
        } else {
            error.into()
        });
    }
    let written = write(&game, &snapshot, &partial, stop, &mut on_progress).and_then(|written| {
        std::fs::rename(&partial, to)
            .map(|()| written)
            .map_err(Error::from)
    });
    match written {
        Ok(written) => Ok(Exported {
            folder: to.to_path_buf(),
            points: written.points,
            left_out: written.left_out,
        }),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&partial);
            Err(error)
        }
    }
}

fn modified(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|found| found.modified())
        .ok()
        .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0)
}

/// Everything about a game the files and the README are written from.
struct Game<'a> {
    app_id: u32,
    title: String,
    report: &'a ReadReport,
    measurement: &'a Measurement,
    read_unix: i64,
    exported_unix: i64,
    /// When the capture was brought up to date after the reading was counted, where it was.
    swept_since: Option<i64>,
    /// None where Steam was never asked about the game's posts.
    updates: Option<Vec<Update>>,
}

/// How many points went into `points`, in how many rows, and how many were left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Written {
    points: u64,
    rows: u64,
    left_out: u64,
}

fn write(
    game: &Game<'_>,
    snapshot: &Path,
    folder: &Path,
    stop: &AtomicBool,
    on_progress: &mut impl FnMut(u64, u64),
) -> Result<Written> {
    let agreement = game.measurement.report();
    let mut subjects = Table::create(folder, "subjects", &subject_columns(game.report))?;
    for row in subject_rows(game.report, agreement) {
        subjects.row(&row)?;
    }
    subjects.finish()?;

    let mut months = Table::create(folder, "months", &month_columns())?;
    for row in month_rows(game.report) {
        months.row(&row)?;
    }
    months.finish()?;

    let mut updates = Table::create(folder, "updates", &update_columns())?;
    for update in game.updates.iter().flatten() {
        updates.row(&[
            Cell::maybe_text(crate::time::iso_day(update.posted)),
            Cell::text(&update.title),
            Cell::text(&update.link),
        ])?;
    }
    updates.finish()?;

    let written = write_points(game, snapshot, folder, stop, on_progress)?;
    std::fs::write(folder.join("README.txt"), readme(game, written))?;
    Ok(written)
}

/// One field of a row, typed so the CSV and the JSON each write it their own way.
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Text(String),
    Count(u64),
    /// A share or a sureness, written to [`PLACES`].
    Decimal(f64),
    Flag(bool),
    /// A figure that does not exist, or one the game page does not show.
    Empty,
}

impl Cell {
    fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    fn maybe_text(text: Option<String>) -> Self {
        text.map_or(Self::Empty, Self::Text)
    }

    fn share(value: Option<f64>) -> Self {
        value
            .filter(|value| value.is_finite())
            .map_or(Self::Empty, Self::Decimal)
    }

    fn flag(value: Option<bool>) -> Self {
        value.map_or(Self::Empty, Self::Flag)
    }

    /// The field as it goes into the CSV.
    fn csv(&self) -> Cow<'_, str> {
        match self {
            Self::Text(text) => neutralised(text),
            Self::Count(count) => Cow::Owned(count.to_string()),
            Self::Decimal(value) => Cow::Owned(decimal(*value)),
            Self::Flag(true) => Cow::Borrowed("true"),
            Self::Flag(false) => Cow::Borrowed("false"),
            Self::Empty => Cow::Borrowed(""),
        }
    }
}

impl Serialize for Cell {
    fn serialize<S: Serializer>(&self, to: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Text(text) => to.serialize_str(text),
            Self::Count(count) => to.serialize_u64(*count),
            Self::Decimal(value) => to.serialize_f64(decimal(*value).parse().unwrap_or(*value)),
            Self::Flag(flag) => to.serialize_bool(*flag),
            Self::Empty => to.serialize_none(),
        }
    }
}

/// A text as a spreadsheet can be handed it: one that would start a formula starts with a `'`
/// instead, which every spreadsheet reads as "this is text".
fn neutralised(text: &str) -> Cow<'_, str> {
    if text.starts_with(FORMULA_STARTS) {
        Cow::Owned(format!("'{text}"))
    } else {
        Cow::Borrowed(text)
    }
}

/// A share to [`PLACES`], with the zeros that say nothing taken off: `0.25`, `1`, `0`.
fn decimal(value: f64) -> String {
    let fixed = format!("{value:.PLACES$}");
    fixed.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "review and point counts are far below 2^53"
)]
fn share(part: u64, whole: u64) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// One column of a table, and what it holds, as the README says it.
struct Column {
    name: String,
    meaning: String,
}

fn column(name: impl Into<String>, meaning: impl Into<String>) -> Column {
    Column {
        name: name.into(),
        meaning: meaning.into(),
    }
}

/// One table, written as `<name>.csv` and `<name>.json` side by side, a row at a time.
struct Table {
    csv: csv::Writer<BufWriter<File>>,
    json: BufWriter<File>,
    names: Vec<String>,
    rows: u64,
}

fn csv_failed(error: csv::Error) -> Error {
    Error::Io(error.into())
}

impl Table {
    fn create(folder: &Path, name: &str, columns: &[Column]) -> Result<Self> {
        let mut file = BufWriter::new(File::create(folder.join(format!("{name}.csv")))?);
        file.write_all(BYTE_ORDER_MARK)?;
        let mut csv = csv::WriterBuilder::new()
            .terminator(csv::Terminator::CRLF)
            .from_writer(file);
        let names: Vec<String> = columns.iter().map(|column| column.name.clone()).collect();
        csv.write_record(&names).map_err(csv_failed)?;
        let mut json = BufWriter::new(File::create(folder.join(format!("{name}.json")))?);
        json.write_all(b"[")?;
        Ok(Self {
            csv,
            json,
            names,
            rows: 0,
        })
    }

    fn row(&mut self, cells: &[Cell]) -> Result<()> {
        let fields: Vec<Cow<'_, str>> = cells.iter().map(Cell::csv).collect();
        self.csv
            .write_record(fields.iter().map(|field| field.as_bytes()))
            .map_err(csv_failed)?;
        self.json
            .write_all(if self.rows == 0 { b"\n" } else { b",\n" })?;
        serde_json::to_writer(
            &mut self.json,
            &Object {
                names: &self.names,
                cells,
            },
        )?;
        self.rows += 1;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.csv.flush()?;
        self.json.write_all(b"\n]\n")?;
        self.json.flush()?;
        Ok(())
    }
}

/// A row as a JSON object, each cell under its column's name.
struct Object<'a> {
    names: &'a [String],
    cells: &'a [Cell],
}

impl Serialize for Object<'_> {
    fn serialize<S: Serializer>(&self, to: S) -> std::result::Result<S::Ok, S::Error> {
        let mut object = to.serialize_map(Some(self.names.len()))?;
        for (name, cell) in self.names.iter().zip(self.cells) {
            object.serialize_entry(name, cell)?;
        }
        object.end()
    }
}

/// Whose reviews a row counts: everyone's, or one kind of reviewer's.
struct Reviewers {
    id: &'static str,
    label: &'static str,
    /// The question the kind is one answer to; empty for everyone.
    question: &'static str,
    reviews: u64,
}

impl Reviewers {
    fn everyone(report: &ReadReport) -> Self {
        Self {
            id: EVERYONE,
            label: "Everyone",
            question: "",
            reviews: report.reviews,
        }
    }

    fn kind(segment: &who::Segment, count: &SegmentCount) -> Self {
        Self {
            id: segment.id,
            label: segment.label,
            question: who::SPLITS
                .iter()
                .find(|split| split.id == segment.split)
                .map_or("", |split| split.label),
            reviews: count.reviews,
        }
    }

    /// Why the game page shows no figure of these reviewers, where it shows none.
    fn withheld(&self) -> Option<String> {
        (self.id != EVERYONE && self.reviews < who::ENOUGH).then(|| {
            format!(
                "fewer than {} reviews by these reviewers, too few for any share of them to mean \
                 anything",
                who::ENOUGH
            )
        })
    }

    fn cells(&self) -> Vec<Cell> {
        vec![
            Cell::text(self.id),
            Cell::text(self.label),
            Cell::maybe_text((!self.question.is_empty()).then(|| self.question.to_owned())),
            Cell::Count(self.reviews),
            Cell::maybe_text(self.withheld()),
        ]
    }
}

fn reviewer_columns() -> Vec<Column> {
    vec![
        column(
            "reviewers",
            "Whose reviews the row counts: everyone, or the id of one kind of reviewer.",
        ),
        column("reviewers_label", "The same, as the game page names them."),
        column(
            "question",
            "What Steam records that the kind of reviewer is one answer to: time played when they \
             wrote it, where they played, when they wrote it, or how they got the game. The kinds \
             of one question share no review. Empty for everyone.",
        ),
        column("reviews", "Reviews by these reviewers that were counted."),
        column(
            "withheld",
            format!(
                "Why the figures of the row are empty, where they are: the game page shows no \
                 figure of a kind of reviewer with fewer than {} reviews.",
                who::ENOUGH
            ),
        ),
    ]
}

fn subject_columns(report: &ReadReport) -> Vec<Column> {
    let mut columns = reviewer_columns();
    columns.extend([
        column(
            "subject",
            "The subject's id, which stays the same across games and releases.",
        ),
        column("subject_label", "The subject as the game page names it."),
        column(
            "reviews_raising",
            "Reviews raising the subject at least once.",
        ),
        column(
            "mention_rate",
            "reviews_raising over reviews: the share of these reviews raising the subject. A \
             review counts once however often it raises it, and once for every subject it \
             raises, so the rates of all subjects add up to more than 1.",
        ),
        column(
            "mention_rate_low",
            "Where the mention rate would likely fall with more reviews like these: the lower end \
             of its 95% Wilson interval.",
        ),
        column("mention_rate_high", "The upper end of the same interval."),
        column(
            "praised",
            "Reviews praising the subject and not complaining about it.",
        ),
        column(
            "criticised",
            "Reviews complaining about the subject and not praising it.",
        ),
        column(
            "mixed",
            "Reviews both praising the subject and complaining about it.",
        ),
        column(
            "praised_share",
            "praised over reviews_raising. A review raising the subject with neither praise nor \
             complaint is in none of the three, so the three shares can add up to less than 1.",
        ),
        column("criticised_share", "criticised over reviews_raising."),
        column("mixed_share", "mixed over reviews_raising."),
        column(
            "recommending_share",
            "Of the reviews raising the subject, the share recommending the game.",
        ),
        column(
            "points",
            "Points about the subject in these reviews. A review making five points about it \
             counts five times here, so this weighs the wordy and is never a headline.",
        ),
        column(
            "point_share",
            "points over every point these reviews make, read or declined.",
        ),
        column(
            "corrected_point_share",
            "point_share with the reader's errors, as measured on this game's labelled points, \
             taken out: only where enough of the subject is labelled and the reader finds it \
             better than chance, which is no subject of most games.",
        ),
        column(
            "in_the_most_helpful",
            format!(
                "The mention rate among the {} reviews Steam ranks most helpful over the mention \
                 rate among all: above 1, the reviews most people read raise the subject more \
                 often than reviewers do. For everyone only.",
                report.top_helpful
            ),
        ),
        column(
            "rate_is_a_floor",
            "true where the reader is measured, on this game's labelled points, to find under a \
             quarter of the points about the subject, so its mention rate is a floor rather \
             than a count; empty where nothing is measured.",
        ),
    ]);
    columns
}

/// One subject over some reviewers.
#[derive(Debug, Clone, Copy, Default)]
struct Counted {
    raising: u64,
    praised: u64,
    criticised: u64,
    mixed: u64,
    recommending: u64,
    points: u64,
    /// Every point the reviewers made, which a share of points is of.
    of_points: u64,
    in_the_most_helpful: Option<f64>,
}

impl Counted {
    fn of_kind(count: &SegmentCount, slot: usize) -> Self {
        let at = |counts: &[u64]| counts.get(slot).copied().unwrap_or(0);
        Self {
            raising: at(&count.raised),
            praised: at(&count.praised),
            criticised: at(&count.criticised),
            mixed: at(&count.mixed),
            recommending: at(&count.recommending),
            points: at(&count.claims_about),
            of_points: count.claims,
            in_the_most_helpful: None,
        }
    }
}

fn subject_cells(
    reviewers: &Reviewers,
    (id, label): (&str, &str),
    counted: &Counted,
    agreement: Option<&ClaimAgreement>,
) -> Vec<Cell> {
    let band = crate::measure::wilson(counted.raising, reviewers.reviews);
    let figures = [
        Cell::Count(counted.raising),
        Cell::share(share(counted.raising, reviewers.reviews)),
        Cell::share(band.map(|(low, _)| low)),
        Cell::share(band.map(|(_, high)| high)),
        Cell::Count(counted.praised),
        Cell::Count(counted.criticised),
        Cell::Count(counted.mixed),
        Cell::share(share(counted.praised, counted.raising)),
        Cell::share(share(counted.criticised, counted.raising)),
        Cell::share(share(counted.mixed, counted.raising)),
        Cell::share(share(counted.recommending, counted.raising)),
        Cell::Count(counted.points),
        Cell::share(share(counted.points, counted.of_points)),
        Cell::share(
            agreement
                .and_then(|found| found.corrected_share(id, counted.points, counted.of_points)),
        ),
        Cell::share(counted.in_the_most_helpful),
        Cell::flag(
            agreement
                .and_then(|found| found.found(id))
                .map(|found| found < FOUND_TOO_FEW),
        ),
    ];
    let withheld = reviewers.withheld().is_some();
    let mut cells = reviewers.cells();
    cells.extend([Cell::text(id), Cell::text(label)]);
    cells.extend(
        figures
            .into_iter()
            .map(|figure| if withheld { Cell::Empty } else { figure }),
    );
    cells
}

/// Every subject for everyone, then every subject for each kind of reviewer the reading counted.
fn subject_rows(report: &ReadReport, agreement: Option<&ClaimAgreement>) -> Vec<Vec<Cell>> {
    let everyone = Reviewers::everyone(report);
    let mut rows: Vec<Vec<Cell>> = report
        .subjects
        .iter()
        .map(|subject| {
            let rate = share(subject.mention_reviews, report.reviews);
            let top = share(subject.top_mention_reviews, report.top_helpful);
            let counted = Counted {
                raising: subject.mention_reviews,
                praised: subject.praised,
                criticised: subject.criticised,
                mixed: subject.mixed,
                recommending: subject.positive_mentions,
                points: subject.claims,
                of_points: report.claims,
                // A subject nobody raised divides nothing by nothing, and a cell is never written
                // from that.
                in_the_most_helpful: rate.zip(top).map(|(rate, top)| top / rate),
            };
            subject_cells(
                &everyone,
                (&subject.id, &subject.label),
                &counted,
                agreement,
            )
        })
        .collect();
    for segment in &who::SEGMENTS {
        let Some(count) = report.who.iter().find(|count| count.id == segment.id) else {
            continue;
        };
        let reviewers = Reviewers::kind(segment, count);
        for (slot, category) in SHEET.iter().enumerate() {
            rows.push(subject_cells(
                &reviewers,
                (category.id, category.label),
                &Counted::of_kind(count, slot),
                agreement,
            ));
        }
    }
    rows
}

fn month_columns() -> Vec<Column> {
    let mut columns = vec![
        column(
            "reviewers",
            "Whose reviews the row counts: everyone, or the id of one kind of reviewer.",
        ),
        column("reviewers_label", "The same, as the game page names them."),
        column(
            "month",
            "The calendar month the reviews were written in, UTC, as year-month.",
        ),
        column("reviews", "Reviews written that month that were counted."),
        column("recommended", "Of them, how many recommend the game."),
        column(
            "recommended_share",
            "recommended over reviews, where the month holds enough reviews for a share.",
        ),
        column(
            "withheld",
            format!(
                "Why recommended_share is empty, where it is: the game page draws no share of a \
                 month with fewer than {} reviews.",
                Month::ENOUGH_FOR_A_RATE
            ),
        ),
    ];
    for category in SHEET {
        columns.push(column(
            format!("{}_raising", category.id),
            format!(
                "Reviews that month raising {}. Counted for everyone only.",
                category.label
            ),
        ));
        columns.push(column(
            format!("{}_praising", category.id),
            format!(
                "Reviews that month praising {}, whether or not they also complain about it.",
                category.label
            ),
        ));
        columns.push(column(
            format!("{}_complaining", category.id),
            format!(
                "Reviews that month complaining about {}, whether or not they also praise it.",
                category.label
            ),
        ));
    }
    columns
}

fn month_head(reviewers: &Reviewers, label: &str, reviews: u64, recommended: u64) -> Vec<Cell> {
    let enough = reviews >= Month::ENOUGH_FOR_A_RATE;
    vec![
        Cell::text(reviewers.id),
        Cell::text(reviewers.label),
        Cell::text(label),
        Cell::Count(reviews),
        Cell::Count(recommended),
        Cell::share(share(recommended, reviews).filter(|_| enough)),
        Cell::maybe_text((!enough).then(|| {
            format!(
                "fewer than {} reviews this month, too few for a share",
                Month::ENOUGH_FOR_A_RATE
            )
        })),
    ]
}

/// Everyone's months, then the months of each kind of reviewer the page can show.
fn month_rows(report: &ReadReport) -> Vec<Vec<Cell>> {
    let everyone = Reviewers::everyone(report);
    let mut rows: Vec<Vec<Cell>> = report
        .months
        .iter()
        .map(|month| {
            let mut cells = month_head(&everyone, &month.label, month.reviews, month.positive);
            let at = |counts: &[u64], slot: usize| {
                counts
                    .get(slot)
                    .map_or(Cell::Empty, |count| Cell::Count(*count))
            };
            for slot in 0..SHEET.len() {
                cells.extend([
                    at(&month.subjects, slot),
                    at(&month.praising, slot),
                    at(&month.complaining, slot),
                ]);
            }
            cells
        })
        .collect();
    for segment in &who::SEGMENTS {
        let Some(count) = report.who.iter().find(|count| count.id == segment.id) else {
            continue;
        };
        let reviewers = Reviewers::kind(segment, count);
        if reviewers.withheld().is_some() {
            continue;
        }
        for month in &count.months {
            let mut cells = month_head(&reviewers, &month.label, month.reviews, month.positive);
            cells.resize(cells.len() + 3 * SHEET.len(), Cell::Empty);
            rows.push(cells);
        }
    }
    rows
}

fn update_columns() -> Vec<Column> {
    vec![
        column(
            "posted",
            "The day the developer posted the update on Steam, UTC.",
        ),
        column("title", "The post's title, as the developer wrote it."),
        column("link", "The post on Steam."),
    ]
}

fn point_columns() -> Vec<Column> {
    vec![
        column("review_id", "Steam's id for the review the point is in."),
        column(
            "start",
            "Where the point starts in the review's text, in bytes of UTF-8 from its first byte.",
        ),
        column(
            "end",
            "Where the point ends, the same way: start to end is the point.",
        ),
        column(
            "subject",
            "The id of a subject the reader filed the point under; empty where it declined to \
             name one rather than file it under whatever came closest.",
        ),
        column("subject_label", "The same, as the game page names it."),
        column(
            "side",
            "What the point does about the subject: praise, complaint or neutral. Empty where \
             the reader declined the point.",
        ),
        column(
            "first_subject",
            "true on the subject the reader named first for the point, false on each other \
             subject it also covers, each of which has a row of its own; empty where it declined \
             the point.",
        ),
        column(
            "sureness",
            "How sure the reader was of the point's first subject, from 0 to 1. A point under \
             several subjects carries the same figure on each of its rows, and a declined point \
             the sureness of its best guess, which fell short of the reader's line.",
        ),
        column("text", "The point as its reviewer wrote it."),
        column(
            "review_link",
            "The review on Steam; empty where Steam did not say who wrote it.",
        ),
        column("written", "The day the review was written, UTC."),
        column(
            "language",
            "The language Steam files the review under, by Steam's own name for it.",
        ),
        column("recommended", "true where the review recommends the game."),
        column(
            "minutes_played_when_written",
            "Minutes of the game its writer had played when they wrote the review, as Steam \
             reports it; empty where Steam did not say.",
        ),
        column(
            "mostly_on_steam_deck",
            "true where Steam says its writer played mostly on a Steam Deck, which Steam has \
             recorded only since the Deck came out in 2022.",
        ),
        column(
            "written_during_early_access",
            "true where the review was written while the game was in early access.",
        ),
        column(
            "got_it_free",
            "true where its writer ticked that they received the game for free.",
        ),
        column(
            "helpful_votes",
            "People on Steam who marked the review helpful.",
        ),
    ]
}

/// A subject a point is filed under, the side it takes on it, and whether it is the first the
/// reader named; all none for a point the reader declined.
type Filed = (Option<&'static Category>, Option<Polarity>, Option<bool>);

fn filed_under(reading: &Reading) -> Vec<Filed> {
    let first = reading
        .subject
        .and_then(|slot| SHEET.get(slot))
        .map(|category| (Some(category), Some(reading.polarity), Some(true)));
    let others = reading.also.iter().filter_map(|(slot, side)| {
        SHEET
            .get(slot)
            .map(|category| (Some(category), Some(side), Some(false)))
    });
    let mut filed: Vec<Filed> = first.into_iter().chain(others).collect();
    if filed.is_empty() {
        filed.push((None, None, None));
    }
    filed
}

/// The cells every row of one review's points ends with.
fn review_cells(row: &crate::capture::Row, author: &str, app_id: u32) -> Vec<Cell> {
    vec![
        Cell::maybe_text((!author.is_empty()).then(|| {
            format!("https://steamcommunity.com/profiles/{author}/recommended/{app_id}/")
        })),
        Cell::maybe_text(crate::time::iso_day(row.created)),
        Cell::text(&row.language),
        Cell::Flag(row.voted_up),
        row.reviewer
            .played_minutes
            .map_or(Cell::Empty, |minutes| Cell::Count(u64::from(minutes))),
        Cell::Flag(row.reviewer.deck),
        Cell::Flag(row.reviewer.early_access),
        Cell::Flag(row.reviewer.free),
        Cell::Count(u64::from(row.votes_up)),
    ]
}

/// Where a point sits in its review, and what the reader said about it.
type Answered = ((u32, u32), Reading);

/// Every point of the reading, a row for each subject it is filed under, in the order the capture
/// holds the reviews. A point is written only where its review still holds it where it was read.
fn write_points(
    game: &Game<'_>,
    snapshot: &Path,
    folder: &Path,
    stop: &AtomicBool,
    on_progress: &mut impl FnMut(u64, u64),
) -> Result<Written> {
    let mut filed: HashMap<String, Vec<Answered>> = HashMap::new();
    crate::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, subject, confidence, polarity, also| {
            let reading = Reading {
                subject: subject.and_then(|id| SHEET.iter().position(|row| row.id == id)),
                confidence,
                polarity: Polarity::from_name(polarity),
                also,
            };
            filed.entry(id.to_owned()).or_default().push((at, reading));
        },
    )?;

    let mut table = Table::create(folder, "points", &point_columns())?;
    let total = game.report.corpus_reviews;
    let mut walked: u64 = 0;
    let mut written = Written::default();
    crate::capture::for_each_row_with_author(snapshot, |row, author, text| {
        if stop.load(Ordering::Relaxed) {
            return Err(Error::Stopped);
        }
        walked += 1;
        if walked.is_multiple_of(TELLS_EVERY) {
            on_progress(walked, total);
        }
        let Some(points) = filed.get(&row.recommendationid) else {
            return Ok(());
        };
        let cut = game.report.depth.spans_of(text);
        let review = review_cells(&row, author, game.app_id);
        for (at, reading) in points {
            let span = at.0 as usize..at.1 as usize;
            let Some(said) = cut
                .contains(&span)
                .then(|| text.get(span.clone()))
                .flatten()
            else {
                written.left_out += 1;
                continue;
            };
            written.points += 1;
            for (category, side, first) in filed_under(reading) {
                let mut cells = vec![
                    Cell::text(&row.recommendationid),
                    Cell::Count(u64::from(at.0)),
                    Cell::Count(u64::from(at.1)),
                    Cell::maybe_text(category.map(|category| category.id.to_owned())),
                    Cell::maybe_text(category.map(|category| category.label.to_owned())),
                    Cell::maybe_text(side.map(|side| side.as_str().to_owned())),
                    Cell::flag(first),
                    Cell::share(Some(f64::from(reading.confidence))),
                    Cell::text(said),
                ];
                cells.extend(review.iter().cloned());
                table.row(&cells)?;
                written.rows += 1;
            }
        }
        Ok(())
    })?;
    on_progress(walked, total);
    table.finish()?;
    Ok(written)
}

/// How far the figures can be trusted, in the words the game page uses.
fn trust(measurement: &Measurement, frozen: Option<Frozen>) -> String {
    match measurement {
        Measurement::Measured(found) => match (found.rate(), found.interval()) {
            (Some(rate), Some((low, high))) => {
                let mut said = format!(
                    "Where this game has been labelled, the model named the same subject a \
                     separate labeller did {} of the time on the {} claims it answered, \
                     somewhere between {} and {}. That is agreement with another model, not \
                     accuracy.",
                    percent(rate),
                    thousands(found.answered),
                    percent(low),
                    percent(high)
                );
                if found.unjoined > 0 {
                    let _ = write!(
                        said,
                        " A further {} labelled claims are left out because this build takes \
                         their reviews apart differently from the build they were labelled under.",
                        thousands(found.unjoined)
                    );
                }
                said
            }
            _ => "The model declined every labelled claim on this game, so nothing here is \
                  measured."
                .to_owned(),
        },
        Measurement::Unscored(why) => format!(
            "This game has labelled claims, and nothing here is measured against them: {why}. \
             Treat every rate as provisional until it is."
        ),
        Measurement::Learned => unmeasured(
            "This game's labelled claims are in the model's training set, so how often it agrees \
             with them says how well it remembers them, not how it reads, and nothing here is \
             scored against them.",
            frozen,
        ),
        Measurement::Unlabelled => unmeasured(
            "Nobody has labelled this game's claims, so how often the model is wrong here has \
             not been measured.",
            frozen,
        ),
    }
}

fn unmeasured(why: &str, frozen: Option<Frozen>) -> String {
    frozen.map_or_else(
        || format!("{why} Treat every rate as provisional."),
        |frozen| {
            format!(
                "{why} What is measured is {} games it had never seen, over {} labelled claims: \
                 it answers {} of them and names the same subject a separate labeller did {} of \
                 the time when it does. Expect this game to be near that, and treat every rate \
                 as provisional.",
                frozen.games,
                thousands(u64::from(frozen.claims)),
                percent(frozen.coverage),
                percent(frozen.accuracy)
            )
        },
    )
}

fn day_or_unknown(unix: i64) -> String {
    crate::time::iso_day(unix).unwrap_or_else(|| "an unknown day".to_owned())
}

/// What each file holds, which reader read the game and when, and how far to trust it.
fn readme(game: &Game<'_>, written: Written) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{} (Steam app {})", game.title, game.app_id);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Exported by SteamGauge {} on {}. Every figure here is one the game's page in SteamGauge \
         shows, counted by the same rules.",
        env!("CARGO_PKG_VERSION"),
        day_or_unknown(game.exported_unix)
    );
    let _ = writeln!(out, "\nWhat was read\n");
    out.push_str(&what_was_read(game, written));
    let _ = writeln!(out, "\nHow far to trust it\n");
    let _ = writeln!(out, "{}", trust(game.measurement, game.report.frozen));
    let _ = writeln!(
        out,
        "These are a census, not a survey: every review Steam serves was counted, so there is \
         no sampling error to report. What they carry is the reader's error."
    );
    let _ = writeln!(out, "\nThe files");
    out.push_str(&the_files(game, written));
    let _ = writeln!(out, "\nReading the CSV files\n");
    out.push_str(&reading_the_csv(written));
    out
}

/// Which reviews were read, by what and when, and what to watch for in the figures.
fn what_was_read(game: &Game<'_>, written: Written) -> String {
    let report = game.report;
    let mut out = String::new();
    let counted = report.language.as_deref().map_or_else(
        || format!("{} reviews in every language", thousands(report.reviews)),
        |language| {
            format!(
                "{} {} reviews of the {} downloaded",
                thousands(report.reviews),
                language_name(language),
                thousands(report.corpus_reviews)
            )
        },
    );
    let _ = writeln!(
        out,
        "{counted}, taken apart into {} points. The reader put no subject on {} of them, which \
         are counted as declined rather than filed under whatever came closest, and named none \
         in {} reviews.",
        thousands(report.claims),
        thousands(report.unclassified_claims),
        thousands(report.silent_reviews)
    );
    let reader = if report.reader.is_empty() {
        &report.model
    } else {
        &report.reader
    };
    let _ = write!(
        out,
        "Read by {reader} on {}",
        day_or_unknown(game.read_unix)
    );
    if !report.read_with.is_empty() {
        let _ = write!(out, ", run {}", report.read_with);
    }
    if report.depth == crate::read::Depth::Shallow {
        let _ = write!(
            out,
            ", each review read whole as one point, which understates anyone who wrote more \
             than a sentence"
        );
    }
    let _ = writeln!(
        out,
        ". The reviews were downloaded up to {}.",
        day_or_unknown(report.captured_unix)
    );
    if let Some(swept) = game.swept_since {
        let _ = writeln!(
            out,
            "The game was brought up to date on {} and these were counted before that; reading \
             it again counts what arrived.",
            day_or_unknown(swept)
        );
    }
    if report
        .unclassified_share()
        .is_some_and(|share| share >= 0.5)
    {
        let _ = writeln!(
            out,
            "The reader would not commit to a subject for {} of the points, so every rate here \
             is a floor: what it was sure of, not everything that was said.",
            percent(report.unclassified_share().unwrap_or_default())
        );
    }
    if written.left_out > 0 {
        let _ = writeln!(
            out,
            "{} points are left out of points.csv because their reviews no longer hold them \
             where they were read: edited since, or taken apart differently by this version. \
             subjects.csv and months.csv count them, as the game page does.",
            thousands(written.left_out)
        );
    }
    out
}

/// Each file, what its rows are, and what every column holds.
fn the_files(game: &Game<'_>, written: Written) -> String {
    let report = game.report;
    let mut out = String::new();
    let mut subjects = String::from(
        "One row per subject for everyone, then one per subject for each kind of reviewer: by \
         time played when they wrote it, where they played, when they wrote it, and how they got \
         the game.",
    );
    if report.who.is_empty() {
        subjects.push_str(
            " This game was counted before reviewers were told apart, so only everyone's rows are \
             here; Count again on its page, or steamgauge recount, adds the rest without reading \
             again.",
        );
    }
    let updates = match &game.updates {
        None => {
            "Steam has not been asked for this game's posts, so there are none here; \
                 bringing the game up to date asks."
        }
        Some(found) if found.is_empty() => {
            "None of the posts its developer made on Steam is an \
                                            update."
        }
        Some(_) => {
            "Each post the game's developer made on Steam that is an update, oldest first: \
                    one Steam marks as patch notes, or titled as a patch, a hotfix, an update or \
                    a version, and not a test branch, a sale or a roadmap."
        }
    };
    let points = format!(
        "One row for each subject of each point the reader read, review by review: a point about \
         two subjects has two rows, and a point it declined has one with no subject. {} points in \
         {} rows.",
        thousands(written.points),
        thousands(written.rows)
    );
    for (files, about, columns) in [
        (
            "subjects.csv and subjects.json",
            subjects.as_str(),
            subject_columns(report),
        ),
        (
            "months.csv and months.json",
            "One row per month, oldest first, for everyone and then for each kind of reviewer \
             with enough reviews for the game page to show; a kind with fewer has no rows.",
            month_columns(),
        ),
        (
            "points.csv and points.json",
            points.as_str(),
            point_columns(),
        ),
        ("updates.csv and updates.json", updates, update_columns()),
    ] {
        let _ = writeln!(out, "\n{files}\n\n{about}\n");
        for column in columns {
            let _ = writeln!(out, "  {}: {}", column.name, column.meaning);
        }
    }
    out
}

/// How the CSV files are laid out, and what a spreadsheet opening them should be told.
fn reading_the_csv(written: Written) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "UTF-8 with a byte order mark, which is what makes Excel read every language's letters; a \
         comma between fields and a CRLF after each row, as RFC 4180 sets out. A field is in \
         double quotes where it holds a comma, a double quote or a line break, and a double \
         quote inside one is written twice."
    );
    let _ = writeln!(
        out,
        "A text that begins with =, +, -, @, a tab or a carriage return has a ' put before it in \
         the CSV, so a spreadsheet shows it rather than running it as a formula: reviews are \
         written by strangers. The JSON files hold every text as it was written."
    );
    let _ = writeln!(
        out,
        "A share is a decimal from 0 to 1, to {PLACES} places. An empty cell, null in the JSON, is \
         a figure that does not exist, such as a share of no reviews, or one SteamGauge does not \
         show; where it is withheld, the withheld column says why."
    );
    if written.rows >= SPREADSHEET_ROWS {
        let _ = writeln!(
            out,
            "points.csv holds {} rows and its header, more than the {} a sheet in Excel holds: \
             Excel opens the first of them and says so. points.json, or any tool that reads CSV \
             without that limit, has every row.",
            thousands(written.rows),
            thousands(SPREADSHEET_ROWS)
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::tests::{CRAWLED, read_corpus_of, read_these};
    use serde_json::{Value, json};

    /// Midday on 1 March 2024.
    const MARCH: i64 = 1_709_294_400;

    struct Read {
        header: Vec<String>,
        csv: Vec<Vec<String>>,
        json: Vec<serde_json::Map<String, Value>>,
    }

    impl Read {
        /// The rows whose `column` is `value`, as JSON objects.
        fn rows_where(&self, column: &str, value: &str) -> Vec<&serde_json::Map<String, Value>> {
            self.json
                .iter()
                .filter(|row| row[column] == Value::String(value.to_owned()))
                .collect()
        }
    }

    /// A table read back from both of its files, after checking the two hold the same rows: the
    /// CSV parsed by the `csv` crate's reader rather than this module's writer, so a quoting
    /// mistake shows as a field the JSON does not have.
    fn read(folder: &Path, name: &str) -> Read {
        let bytes = std::fs::read(folder.join(format!("{name}.csv"))).unwrap();
        assert!(
            bytes.starts_with(BYTE_ORDER_MARK),
            "{name}.csv starts with a BOM"
        );
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(text.ends_with("\r\n"), "{name}.csv ends its last row");
        let mut reader = csv::ReaderBuilder::new().from_reader(&bytes[..]);
        let header: Vec<String> = reader
            .headers()
            .unwrap()
            .iter()
            .map(str::to_owned)
            .collect();
        let csv: Vec<Vec<String>> = reader
            .records()
            .map(|record| record.unwrap().iter().map(str::to_owned).collect())
            .collect();
        let json: Vec<Value> =
            serde_json::from_slice(&std::fs::read(folder.join(format!("{name}.json"))).unwrap())
                .unwrap();
        let json: Vec<serde_json::Map<String, Value>> = json
            .into_iter()
            .map(|row| row.as_object().unwrap().clone())
            .collect();
        assert_eq!(
            csv.len(),
            json.len(),
            "{name} has as many rows in each file"
        );
        for (fields, object) in csv.iter().zip(&json) {
            assert_eq!(fields.len(), header.len());
            assert_eq!(object.len(), header.len());
            for (name, field) in header.iter().zip(fields) {
                let agrees = match &object[name] {
                    Value::Null => field.is_empty(),
                    Value::Bool(flag) => *field == flag.to_string(),
                    Value::Number(number) => field.parse::<f64>().ok() == number.as_f64(),
                    Value::String(text) => *field == neutralised(text),
                    other => panic!("{name} holds {other}, which no cell writes"),
                };
                assert!(
                    agrees,
                    "{name}: the CSV says {field:?}, the JSON {}",
                    object[name]
                );
            }
        }
        Read { header, csv, json }
    }

    fn names(columns: &[Column]) -> Vec<String> {
        columns.iter().map(|column| column.name.clone()).collect()
    }

    fn export(out: &Path, app_id: u32) -> (Exported, Vec<(u64, u64)>) {
        let mut told = Vec::new();
        let exported = game(
            out,
            app_id,
            &out.join("Test Game data"),
            &AtomicBool::new(false),
            |walked, of| told.push((walked, of)),
        )
        .unwrap();
        (exported, told)
    }

    fn review(id: &str, text: &str) -> Value {
        json!({
            "recommendationid": id, "review": text, "language": "english", "voted_up": true,
            "weighted_vote_score": "0.5", "votes_up": 3, "timestamp_created": MARCH,
            "author": {"steamid": format!("7656{id}"), "playtime_at_review": 3_000},
        })
    }

    /// Reviews that start like formulas, one with a comma and quotes in it and nobody behind it,
    /// and enough written by people with 30 to 100 hours that the page shows their figures.
    fn strangers(out: &Path) -> PathBuf {
        let mut reviews = vec![
            review("1", "=1+1 bug count."),
            review("2", "+bug in every level."),
            review("3", "-10/10 bugs."),
            review("4", "@everyone the bugs are back."),
            json!({
                "recommendationid": "5", "review": "Bugs, \"so many\", everywhere.",
                "language": "english", "voted_up": false, "weighted_vote_score": "0.1",
                "votes_up": 0, "timestamp_created": MARCH,
            }),
        ];
        reviews.extend((0..who::ENOUGH).map(|at| review(&format!("9{at:03}"), "Bugs everywhere.")));
        let snapshot = read_these(out, 2, &reviews);
        let post =
            |gid: &str, title: &str, posted: i64, patch_notes: bool| crate::updates::Announcement {
                gid: gid.to_owned(),
                title: title.to_owned(),
                posted,
                patch_notes,
            };
        crate::updates::Announcements {
            asked: MARCH,
            posts: vec![
                post(
                    "11",
                    "=HYPERLINK(\"http://example.com\") Patch 1.1",
                    MARCH,
                    true,
                ),
                post("12", "-Hotfix 1.2", MARCH + 86_400, true),
                post("13", "Summer sale", MARCH + 2 * 86_400, false),
            ],
        }
        .save(&out.join("appid=2"))
        .unwrap();
        snapshot
    }

    #[test]
    fn a_text_that_would_start_a_formula_starts_with_a_quote_instead() {
        for start in FORMULA_STARTS {
            let text = format!("{start}HYPERLINK(\"http://example.com\")");
            assert_eq!(neutralised(&text), format!("'{text}"));
        }
        for safe in [
            "Bugs everywhere.",
            "",
            "a=b",
            "10/10",
            "'quoted already",
            " =spaced",
        ] {
            assert_eq!(neutralised(safe), safe);
        }
        assert_eq!(Cell::text("=1+1").csv(), "'=1+1");
        assert_eq!(
            serde_json::to_value(Cell::text("=1+1")).unwrap(),
            json!("=1+1"),
            "the JSON keeps the text as written"
        );
    }

    #[test]
    fn each_kind_of_cell_is_written_its_own_way() {
        assert_eq!(decimal(0.25), "0.25");
        assert_eq!(decimal(1.0), "1");
        assert_eq!(decimal(0.0), "0");
        assert_eq!(decimal(10.0), "10");
        assert_eq!(decimal(1.0 / 3.0), "0.333333");
        assert_eq!(decimal(0.000_001), "0.000001");
        assert_eq!(decimal(f64::from(0.9_f32)), "0.9");
        assert_eq!(Cell::Count(1_234).csv(), "1234");
        assert_eq!(Cell::Flag(true).csv(), "true");
        assert_eq!(Cell::Flag(false).csv(), "false");
        assert_eq!(Cell::Empty.csv(), "");
        assert_eq!(Cell::share(Some(f64::NAN)), Cell::Empty);
        assert_eq!(Cell::share(Some(f64::INFINITY)), Cell::Empty);
        assert_eq!(Cell::share(None), Cell::Empty);
        assert_eq!(Cell::flag(None), Cell::Empty);
        assert_eq!(Cell::maybe_text(None), Cell::Empty);
        assert_eq!(
            serde_json::to_value([
                Cell::Count(7),
                Cell::Decimal(1.0 / 3.0),
                Cell::Flag(false),
                Cell::Empty,
            ])
            .unwrap(),
            json!([7, 0.333_333, false, null])
        );
        assert_eq!(share(1, 4), Some(0.25));
        assert_eq!(share(0, 0), None, "a share of nothing is none, not NaN");
    }

    #[test]
    fn a_field_is_quoted_only_where_it_must_be_and_every_row_ends_in_crlf() {
        let out = crate::tempdir::Dir::new();
        let mut table = Table::create(
            out.path(),
            "quoted",
            &[column("text", "Text."), column("count", "A count.")],
        )
        .unwrap();
        table.row(&[Cell::text("plain"), Cell::Count(1)]).unwrap();
        table
            .row(&[Cell::text("a, \"b\"\nc"), Cell::Count(2)])
            .unwrap();
        table.finish().unwrap();
        let written = std::fs::read(out.path().join("quoted.csv")).unwrap();
        assert_eq!(
            std::str::from_utf8(&written).unwrap(),
            "\u{feff}text,count\r\nplain,1\r\n\"a, \"\"b\"\"\nc\",2\r\n"
        );
        let quoted = read(out.path(), "quoted");
        assert_eq!(quoted.csv[1][0], "a, \"b\"\nc");
        let json = std::fs::read_to_string(out.path().join("quoted.json")).unwrap();
        assert!(json.starts_with("[\n{") && json.contains("},\n{") && json.ends_with("}\n]\n"));
        assert_eq!(
            serde_json::from_str::<Value>(&json).unwrap(),
            json!([{"text": "plain", "count": 1}, {"text": "a, \"b\"\nc", "count": 2}])
        );

        let empty = Table::create(out.path(), "empty", &[column("text", "Text.")]).unwrap();
        empty.finish().unwrap();
        assert_eq!(
            std::fs::read_to_string(out.path().join("empty.json")).unwrap(),
            "[\n]\n"
        );
        assert_eq!(read(out.path(), "empty").json.len(), 0);
    }

    /// A game read and exported, with what the export said as it went.
    struct Fixture {
        out: crate::tempdir::Dir,
        report: ReadReport,
        snapshot: PathBuf,
        folder: PathBuf,
        exported: Exported,
        told: Vec<(u64, u64)>,
    }

    fn exported(app_id: u32, fixture: impl FnOnce(&Path) -> PathBuf) -> Fixture {
        let out = crate::tempdir::Dir::new();
        let snapshot = fixture(out.path());
        let report: ReadReport =
            serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).unwrap()).unwrap();
        let (exported, told) = export(out.path(), app_id);
        Fixture {
            folder: out.path().join("Test Game data"),
            out,
            report,
            snapshot,
            exported,
            told,
        }
    }

    /// The seven reviews of [`read_corpus_of`].
    fn seven() -> Fixture {
        exported(1, |out| read_corpus_of(out, 1))
    }

    #[test]
    fn a_read_game_is_written_whole_and_the_two_files_of_each_table_agree() {
        let Fixture {
            out,
            folder,
            exported,
            told,
            ..
        } = seven();
        assert_eq!(exported.folder, folder);
        assert_eq!((exported.points, exported.left_out), (10, 0));
        assert_eq!(
            told,
            vec![(7, 7)],
            "told once, at the end, of every review walked"
        );
        assert!(!out.path().join("Test Game data.partial").exists());
        let mut files: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        assert_eq!(
            files,
            [
                "README.txt",
                "months.csv",
                "months.json",
                "points.csv",
                "points.json",
                "subjects.csv",
                "subjects.json",
                "updates.csv",
                "updates.json"
            ]
        );
        for table in ["subjects", "months", "points", "updates"] {
            read(&folder, table);
        }
    }

    #[test]
    fn everyones_subjects_are_the_figures_the_page_shows_and_a_small_kind_is_withheld() {
        let Fixture {
            out: _out,
            report,
            folder,
            ..
        } = seven();
        let subjects = read(&folder, "subjects");
        assert_eq!(subjects.header, names(&subject_columns(&report)));
        assert_eq!(
            subjects.json.len(),
            SHEET.len() * (1 + who::SEGMENTS.len()),
            "every subject for everyone and for each kind of reviewer"
        );
        let bugs = subjects
            .rows_where("subject", "bugs")
            .into_iter()
            .find(|row| row["reviewers"] == "everyone")
            .unwrap();
        let counted = report.subjects.iter().find(|s| s.id == "bugs").unwrap();
        assert_eq!(bugs["reviewers_label"], "Everyone");
        assert_eq!(bugs["question"], Value::Null);
        assert_eq!(bugs["withheld"], Value::Null);
        assert_eq!(bugs["subject_label"], counted.label.as_str());
        assert_eq!(bugs["reviews"], json!(report.reviews));
        assert_eq!(bugs["reviews_raising"], json!(counted.mention_reviews));
        assert_eq!(bugs["praised"], json!(counted.praised));
        assert_eq!(bugs["criticised"], json!(counted.criticised));
        assert_eq!(bugs["mixed"], json!(counted.mixed));
        assert_eq!(bugs["points"], json!(counted.claims));
        let rounded = |part: u64, whole: u64| {
            json!(decimal(share(part, whole).unwrap()).parse::<f64>().unwrap())
        };
        assert_eq!(
            bugs["mention_rate"],
            rounded(counted.mention_reviews, report.reviews)
        );
        assert_eq!(
            bugs["praised_share"],
            rounded(counted.praised, counted.mention_reviews)
        );
        assert_eq!(
            bugs["criticised_share"],
            rounded(counted.criticised, counted.mention_reviews)
        );
        assert_eq!(
            bugs["mixed_share"],
            rounded(counted.mixed, counted.mention_reviews)
        );
        assert_eq!(
            bugs["recommending_share"],
            rounded(counted.positive_mentions, counted.mention_reviews)
        );
        assert_eq!(bugs["point_share"], rounded(counted.claims, report.claims));
        let (low, high) = crate::measure::wilson(counted.mention_reviews, report.reviews).unwrap();
        assert_eq!(
            bugs["mention_rate_low"],
            json!(decimal(low).parse::<f64>().unwrap())
        );
        assert_eq!(
            bugs["mention_rate_high"],
            json!(decimal(high).parse::<f64>().unwrap())
        );
        let top = share(counted.top_mention_reviews, report.top_helpful).unwrap()
            / share(counted.mention_reviews, report.reviews).unwrap();
        assert_eq!(
            bugs["in_the_most_helpful"],
            json!(decimal(top).parse::<f64>().unwrap())
        );
        assert_eq!(
            bugs["corrected_point_share"],
            Value::Null,
            "nothing is labelled here"
        );
        assert_eq!(bugs["rate_is_a_floor"], Value::Null);
        let unraised = subjects
            .rows_where("subject", "vr")
            .into_iter()
            .find(|row| row["reviewers"] == "everyone")
            .unwrap();
        assert_eq!(unraised["reviews_raising"], json!(0));
        assert_eq!(unraised["mention_rate"], json!(0.0));
        assert_eq!(unraised["praised_share"], Value::Null);
        assert_eq!(unraised["in_the_most_helpful"], Value::Null);
    }

    #[test]
    fn a_kind_of_reviewer_too_few_to_show_has_its_figures_withheld() {
        // Seven reviews: every kind of reviewer is too few to show.
        let Fixture {
            out: _out,
            report,
            folder,
            ..
        } = seven();
        let subjects = read(&folder, "subjects");
        let kind = subjects
            .rows_where("reviewers", "30-to-100-hours")
            .into_iter()
            .find(|row| row["subject"] == "bugs")
            .unwrap();
        assert_eq!(kind["reviewers_label"], "30 to 100 hours");
        assert_eq!(kind["question"], "Time played when they wrote it");
        let theirs = report
            .who
            .iter()
            .find(|count| count.id == "30-to-100-hours")
            .unwrap();
        assert!(theirs.reviews > 0 && theirs.reviews < who::ENOUGH);
        assert_eq!(kind["reviews"], json!(theirs.reviews));
        assert!(
            kind["withheld"]
                .as_str()
                .unwrap()
                .starts_with("fewer than 100 reviews")
        );
        for figure in subject_columns(&report).iter().skip(7) {
            assert_eq!(
                kind[&figure.name],
                Value::Null,
                "{} is withheld",
                figure.name
            );
        }
    }

    #[test]
    fn months_and_points_are_written_as_the_reading_counted_them() {
        let Fixture {
            out: _out,
            report,
            folder,
            ..
        } = seven();
        let months = read(&folder, "months");
        assert_eq!(months.header, names(&month_columns()));
        assert_eq!(
            months.json.len(),
            report.months.len(),
            "no kind has months to show"
        );
        let march = &months.json[0];
        assert_eq!(march["month"], "2024-03");
        assert_eq!(march["reviews"], json!(report.months[0].reviews));
        assert_eq!(march["recommended"], json!(report.months[0].positive));
        assert_eq!(march["recommended_share"], Value::Null);
        assert!(
            march["withheld"]
                .as_str()
                .unwrap()
                .starts_with("fewer than 30 reviews")
        );
        let slot = SHEET.iter().position(|row| row.id == "bugs").unwrap();
        assert_eq!(
            march["bugs_raising"],
            json!(report.months[0].subjects[slot])
        );
        assert_eq!(
            march["bugs_praising"],
            json!(report.months[0].praising[slot])
        );
        assert_eq!(
            march["bugs_complaining"],
            json!(report.months[0].complaining[slot])
        );
    }

    #[test]
    fn every_point_is_written_under_each_subject_with_its_review() {
        let Fixture {
            out: _out, folder, ..
        } = seven();
        let points = read(&folder, "points");
        assert_eq!(points.header, names(&point_columns()));
        assert_eq!(
            points.json.len(),
            11,
            "ten points, one of them under two subjects"
        );
        let first = &points.json[0];
        assert_eq!(first["review_id"], "1");
        assert_eq!(
            (first["start"].clone(), first["end"].clone()),
            (json!(0), json!(16))
        );
        assert_eq!(first["text"], "Bugs everywhere.");
        assert_eq!(first["subject"], "bugs");
        assert_eq!(first["side"], "complaint");
        assert_eq!(first["first_subject"], true);
        assert_eq!(first["sureness"], json!(0.9));
        assert_eq!(
            first["review_link"],
            "https://steamcommunity.com/profiles/76561/recommended/1/"
        );
        assert_eq!(first["written"], "2024-03-15");
        assert_eq!(first["language"], "english");
        assert_eq!(first["recommended"], false);
        assert_eq!(first["minutes_played_when_written"], json!(90));
        assert_eq!(first["mostly_on_steam_deck"], true);
        assert_eq!(first["written_during_early_access"], false);
        assert_eq!(first["got_it_free"], false);
        assert_eq!(first["helpful_votes"], json!(50));
        let story = points.rows_where("review_id", "2");
        assert_eq!(story.len(), 3, "the story point twice and a declined one");
        assert_eq!(
            (
                story[0]["subject"].clone(),
                story[0]["side"].clone(),
                story[0]["first_subject"].clone()
            ),
            (json!("story"), json!("praise"), json!(true))
        );
        assert_eq!(
            (
                story[1]["subject"].clone(),
                story[1]["side"].clone(),
                story[1]["first_subject"].clone()
            ),
            (json!("controls"), json!("complaint"), json!(false))
        );
        assert_eq!(story[1]["text"], story[0]["text"]);
        assert_eq!(story[1]["sureness"], story[0]["sureness"]);
        assert_eq!(story[2]["text"], "It is great.");
        for declined in ["subject", "subject_label", "side", "first_subject"] {
            assert_eq!(
                story[2][declined],
                Value::Null,
                "a declined point has no {declined}"
            );
        }
        assert_eq!(story[2]["sureness"], json!(0.3));
        assert_eq!(
            points.rows_where("review_id", "5").len(),
            0,
            "\"...\" makes no point"
        );

        let updates = read(&folder, "updates");
        assert_eq!(updates.header, names(&update_columns()));
        assert_eq!(updates.json.len(), 0);
    }

    #[test]
    fn the_readme_says_what_every_column_holds_and_how_the_game_was_read() {
        let Fixture {
            out: _out,
            report,
            snapshot,
            folder,
            ..
        } = seven();
        let readme = std::fs::read_to_string(folder.join("README.txt")).unwrap();
        assert!(readme.starts_with("Test Game (Steam app 1)\n"));
        for column in subject_columns(&report)
            .iter()
            .chain(&month_columns())
            .chain(&point_columns())
            .chain(&update_columns())
        {
            assert!(
                readme.contains(&format!("  {}: {}\n", column.name, column.meaning)),
                "the README says what {} holds",
                column.name
            );
        }
        assert!(readme.contains("10 points in 11 rows"));
        assert!(readme.contains("Steam has not been asked for this game's posts"));
        assert!(readme.contains("What is measured is 3 games it had never seen"));
        assert!(readme.contains("7 reviews in every language, taken apart into"));
        let read_on = crate::time::iso_day(modified(&snapshot.join("reading.json"))).unwrap();
        assert!(readme.contains(&format!("Read by Table Reader on {read_on}, run run-1.")));
        assert!(readme.contains(&format!(
            "The reviews were downloaded up to {}.",
            crate::time::iso_day(crate::read::tests::SWEPT).unwrap()
        )));
        let today = |unix| crate::time::iso_day(unix).unwrap();
        let now = unix_now();
        assert!(
            readme.contains(&format!("on {}. Every figure", today(now)))
                || readme.contains(&format!("on {}. Every figure", today(now - 60))),
            "the README says when it was written"
        );
        assert!(!readme.contains("brought up to date on"));
        assert!(!readme.contains("left out of points.csv"));
        assert!(!readme.contains("a sheet in Excel holds"));
    }

    #[test]
    fn text_from_strangers_cannot_run_in_a_spreadsheet() {
        let Fixture {
            out: _out,
            folder,
            exported,
            told,
            ..
        } = exported(2, strangers);
        let walked = 5 + who::ENOUGH;
        assert_eq!(told, vec![(walked, walked)]);
        assert_eq!(exported.left_out, 0);

        let points = read(&folder, "points");
        let neutralised: Vec<&Vec<String>> = points
            .csv
            .iter()
            .filter(|fields| fields[8].starts_with('\''))
            .collect();
        // The splitter takes a leading + or - for a list's bullet, so only these two reach a point.
        assert_eq!(
            neutralised.len(),
            2,
            "every point starting like a formula is neutralised"
        );
        for (fields, start) in neutralised.iter().zip(['=', '@']) {
            assert!(
                fields[8].starts_with(&format!("'{start}")),
                "{:?} is written as text",
                fields[8]
            );
        }
        let raw = std::fs::read_to_string(folder.join("points.csv")).unwrap();
        assert!(!raw.contains(",=1+1"), "no cell starts with a bare formula");
        assert!(raw.contains(",'=1+1 bug count.,"));
        let json_text: Vec<&str> = points
            .json
            .iter()
            .filter_map(|row| row["text"].as_str())
            .collect();
        assert!(
            json_text.contains(&"=1+1 bug count."),
            "the JSON keeps it as written"
        );

        let quoted = points.rows_where("review_id", "5");
        assert_ne!(quoted.len(), 0);
        assert_eq!(quoted[0]["review_link"], Value::Null, "nobody is behind it");
        assert_eq!(quoted[0]["minutes_played_when_written"], Value::Null);
        assert_eq!(quoted[0]["helpful_votes"], json!(0));
        assert_eq!(quoted[0]["recommended"], false);
        assert!(
            quoted
                .iter()
                .any(|row| row["text"].as_str().unwrap().contains("\"so many\"")),
            "quotes survive the round trip"
        );
    }

    #[test]
    fn a_kind_of_reviewer_with_enough_reviews_is_shown_with_its_months() {
        let Fixture {
            out: _out,
            report,
            folder,
            ..
        } = exported(2, strangers);
        let subjects = read(&folder, "subjects");
        let shown = subjects
            .rows_where("reviewers", "30-to-100-hours")
            .into_iter()
            .find(|row| row["subject"] == "bugs")
            .unwrap();
        let counted = report
            .who
            .iter()
            .find(|count| count.id == "30-to-100-hours")
            .unwrap();
        let slot = SHEET.iter().position(|row| row.id == "bugs").unwrap();
        assert_eq!(shown["reviews"], json!(counted.reviews));
        assert!(counted.reviews >= who::ENOUGH);
        assert_eq!(shown["withheld"], Value::Null);
        assert_eq!(shown["reviews_raising"], json!(counted.raised[slot]));
        assert_eq!(shown["criticised"], json!(counted.criticised[slot]));
        assert_eq!(shown["points"], json!(counted.claims_about[slot]));
        assert_eq!(
            shown["point_share"],
            json!(
                decimal(share(counted.claims_about[slot], counted.claims).unwrap())
                    .parse::<f64>()
                    .unwrap()
            )
        );
        assert_eq!(
            shown["in_the_most_helpful"],
            Value::Null,
            "counted for everyone only"
        );
        let short = subjects
            .rows_where("reviewers", "under-2-hours")
            .into_iter()
            .find(|row| row["subject"] == "bugs")
            .unwrap();
        assert_eq!(short["reviews"], json!(0));
        assert_eq!(short["reviews_raising"], Value::Null);
        assert_eq!(short["mention_rate"], Value::Null);

        let months = read(&folder, "months");
        let everyone = months.rows_where("reviewers", "everyone");
        assert_eq!(everyone.len(), 1);
        assert_eq!(everyone[0]["withheld"], Value::Null);
        assert_eq!(
            everyone[0]["recommended_share"],
            json!(
                decimal(share(report.months[0].positive, report.months[0].reviews).unwrap())
                    .parse::<f64>()
                    .unwrap()
            )
        );
        let theirs = months.rows_where("reviewers", "30-to-100-hours");
        assert_eq!(theirs.len(), 1, "a kind the page shows has its months");
        assert_eq!(theirs[0]["reviews"], json!(counted.months[0].reviews));
        assert_eq!(
            theirs[0]["bugs_raising"],
            Value::Null,
            "counted for everyone only"
        );
        assert_eq!(months.rows_where("reviewers", "under-2-hours").len(), 0);
        assert_eq!(months.rows_where("reviewers", "steam-deck").len(), 0);
    }

    #[test]
    fn an_update_titled_like_a_formula_cannot_run_in_a_spreadsheet_either() {
        let Fixture {
            out: _out, folder, ..
        } = exported(2, strangers);
        let updates = read(&folder, "updates");
        assert_eq!(updates.json.len(), 2, "the sale is not an update");
        assert_eq!(
            updates.csv[0],
            [
                "2024-03-01",
                "'=HYPERLINK(\"http://example.com\") Patch 1.1",
                &crate::updates::link("11")
            ]
        );
        assert_eq!(
            updates.json[0]["title"],
            "=HYPERLINK(\"http://example.com\") Patch 1.1"
        );
        assert_eq!(updates.csv[1][1], "'-Hotfix 1.2");
        assert_eq!(updates.json[1]["posted"], "2024-03-02");

        let readme = std::fs::read_to_string(folder.join("README.txt")).unwrap();
        assert!(!readme.contains("counted before reviewers were told apart"));
        assert!(readme.contains("Each post the game's developer made on Steam that is an update"));
    }

    #[test]
    fn a_month_carries_a_share_only_where_it_holds_enough_reviews() {
        let everyone = Reviewers {
            id: EVERYONE,
            label: "Everyone",
            question: "",
            reviews: 0,
        };
        let enough = Month::ENOUGH_FOR_A_RATE;
        let thin = month_head(&everyone, "2024-03", enough - 1, 10);
        assert_eq!(thin[5], Cell::Empty);
        assert!(matches!(&thin[6], Cell::Text(why) if why.starts_with("fewer than 30")));
        let full = month_head(&everyone, "2024-03", enough, 15);
        assert_eq!(full[5], Cell::Decimal(0.5));
        assert_eq!(full[6], Cell::Empty);
        assert_eq!(
            full[..5],
            [
                Cell::text("everyone"),
                Cell::text("Everyone"),
                Cell::text("2024-03"),
                Cell::Count(enough),
                Cell::Count(15)
            ]
        );
    }

    #[test]
    fn a_kind_of_reviewer_is_shown_from_the_hundredth_review() {
        let kind = |reviews| Reviewers {
            id: "steam-deck",
            label: "Mostly on a Steam Deck",
            question: "Where they played",
            reviews,
        };
        assert!(kind(who::ENOUGH - 1).withheld().is_some());
        assert_eq!(kind(who::ENOUGH).withheld(), None);
        let everyone = Reviewers {
            id: EVERYONE,
            label: "Everyone",
            question: "",
            reviews: 3,
        };
        assert_eq!(everyone.withheld(), None, "everyone is always shown");
        let counted = Counted {
            raising: 50,
            praised: 20,
            criticised: 10,
            mixed: 5,
            recommending: 30,
            points: 70,
            of_points: 700,
            in_the_most_helpful: None,
        };
        let withheld = subject_cells(&kind(who::ENOUGH - 1), ("bugs", "Bugs"), &counted, None);
        let shown = subject_cells(&kind(who::ENOUGH), ("bugs", "Bugs"), &counted, None);
        assert_eq!(withheld.len(), shown.len());
        assert!(withheld[7..].iter().all(|cell| *cell == Cell::Empty));
        assert_eq!(
            shown[7..18],
            [
                Cell::Count(50),
                Cell::Decimal(0.5),
                shown[9].clone(),
                shown[10].clone(),
                Cell::Count(20),
                Cell::Count(10),
                Cell::Count(5),
                Cell::Decimal(0.4),
                Cell::Decimal(0.2),
                Cell::Decimal(0.1),
                Cell::Decimal(0.6),
            ]
        );
        assert_eq!(shown[18], Cell::Count(70));
        assert_eq!(shown[19], Cell::Decimal(0.1));
    }

    fn labelled(id: &'static str, labelled: u64, agreed: u64) -> crate::measure::SubjectAgreement {
        crate::measure::SubjectAgreement {
            id,
            label: id,
            labelled,
            read: agreed + 10,
            agreed,
            seen: 1_000,
            mistaken_for: None,
        }
    }

    fn agreement(subjects: Vec<crate::measure::SubjectAgreement>) -> ClaimAgreement {
        ClaimAgreement {
            app_id: 1,
            matched: 200,
            unjoined: 0,
            answered: 160,
            agreed: 120,
            declined: 40,
            polarity_answered: 0,
            polarity_agreed: 0,
            clear_answered: 0,
            clear_agreed: 0,
            contested_answered: 0,
            contested_agreed: 0,
            subjects,
            beyond_the_first: crate::measure::Beyond::default(),
        }
    }

    #[test]
    fn a_measured_game_corrects_what_it_can_and_marks_a_rate_that_is_a_floor() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_corpus_of(out.path(), 1);
        let report: ReadReport =
            serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).unwrap()).unwrap();
        // Bugs found in a fifth of its labels, audio in exactly a quarter, story in most.
        let measured = agreement(vec![
            labelled("bugs", 100, 20),
            labelled("audio", 20, 5),
            labelled("story", 100, 80),
        ]);
        let rows = subject_rows(&report, Some(&measured));
        let row = |id: &str| {
            rows.iter()
                .find(|cells| cells[0] == Cell::text("everyone") && cells[5] == Cell::text(id))
                .unwrap()
        };
        assert_eq!(row("bugs")[22], Cell::Flag(true));
        assert_eq!(
            row("audio")[22],
            Cell::Flag(false),
            "a quarter found is not under a quarter"
        );
        assert_eq!(row("story")[22], Cell::Flag(false));
        assert_eq!(row("vr")[22], Cell::Empty, "nothing measured");

        let story = report.subjects.iter().find(|s| s.id == "story").unwrap();
        assert_eq!(
            row("story")[20],
            Cell::share(measured.corrected_share("story", story.claims, report.claims))
        );
        assert!(matches!(row("story")[20], Cell::Decimal(_)));
        assert_eq!(
            row("audio")[20],
            Cell::Empty,
            "too few labels to correct by"
        );
    }

    #[test]
    fn how_far_to_trust_it_is_said_as_the_game_page_says_it() {
        let frozen = Frozen {
            games: 8,
            claims: 1_234,
            coverage: 0.8,
            accuracy: 0.9,
            macro_f1: 0.7,
        };
        let measured = trust(
            &Measurement::Measured(Box::new(agreement(Vec::new()))),
            Some(frozen),
        );
        assert!(measured.starts_with(
            "Where this game has been labelled, the model named the same subject a separate \
             labeller did 75.0% of the time on the 160 claims it answered"
        ));
        assert!(measured.ends_with("That is agreement with another model, not accuracy."));
        let unjoined = trust(
            &Measurement::Measured(Box::new(ClaimAgreement {
                unjoined: 12,
                ..agreement(Vec::new())
            })),
            None,
        );
        assert!(unjoined.contains("A further 12 labelled claims are left out"));
        let declined = trust(
            &Measurement::Measured(Box::new(ClaimAgreement {
                answered: 0,
                agreed: 0,
                ..agreement(Vec::new())
            })),
            None,
        );
        assert!(declined.starts_with("The model declined every labelled claim"));
        let unscored = trust(
            &Measurement::Unscored("its labels cannot be read".to_owned()),
            None,
        );
        assert!(unscored.contains("measured against them: its labels cannot be read. Treat"));
        let learned = trust(&Measurement::Learned, Some(frozen));
        assert!(learned.starts_with("This game's labelled claims are in the model's training set"));
        assert!(learned.contains(
            "What is measured is 8 games it had never seen, over 1,234 labelled claims: it \
             answers 80.0% of them and names the same subject a separate labeller did 90.0%"
        ));
        let unlabelled = trust(&Measurement::Unlabelled, None);
        assert_eq!(
            unlabelled,
            "Nobody has labelled this game's claims, so how often the model is wrong here has \
             not been measured. Treat every rate as provisional."
        );
    }

    fn readme_of(report: &ReadReport, written: Written, swept_since: Option<i64>) -> String {
        readme(
            &Game {
                app_id: 9,
                title: "Nine".to_owned(),
                report,
                measurement: &Measurement::Unlabelled,
                read_unix: MARCH,
                exported_unix: MARCH + 86_400,
                swept_since,
                updates: Some(Vec::new()),
            },
            written,
        )
    }

    #[test]
    fn the_readme_says_what_the_reading_was_and_what_to_watch_for() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_corpus_of(out.path(), 1);
        let report: ReadReport =
            serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).unwrap()).unwrap();
        let plain = readme_of(&report, Written::default(), None);
        assert!(plain.contains("Exported by SteamGauge"));
        assert!(plain.contains("on 2024-03-02. Every figure"));
        assert!(plain.contains("Read by Table Reader on 2024-03-01, run run-1."));
        assert!(plain.contains("None of the posts its developer made on Steam is an update."));
        assert!(!plain.contains("every rate here is a floor"));
        assert!(!plain.contains("each review read whole"));

        let english = ReadReport {
            language: Some("english".to_owned()),
            reviews: 6,
            reader: String::new(),
            model: "a model".to_owned(),
            read_with: String::new(),
            depth: crate::read::Depth::Shallow,
            unclassified_claims: report.claims / 2,
            claims: report.claims / 2 * 2,
            who: Vec::new(),
            ..report.clone()
        };
        let said = readme_of(
            &english,
            Written {
                points: 3,
                rows: SPREADSHEET_ROWS,
                left_out: 2,
            },
            Some(MARCH + 2 * 86_400),
        );
        assert!(said.contains("6 English reviews of the 7 downloaded"));
        assert!(said.contains("Read by a model on 2024-03-01, each review read whole"));
        assert!(!said.contains(", run "));
        assert!(said.contains("brought up to date on 2024-03-03"));
        assert!(
            said.contains("every rate here is a floor"),
            "half declined is the line"
        );
        assert!(said.contains("2 points are left out of points.csv"));
        assert!(said.contains("a sheet in Excel holds"));
        assert!(said.contains("counted before reviewers were told apart"));

        let fewer = ReadReport {
            unclassified_claims: report.claims / 2 - 1,
            claims: report.claims / 2 * 2,
            ..report.clone()
        };
        let said = readme_of(
            &fewer,
            Written {
                rows: SPREADSHEET_ROWS - 1,
                ..Written::default()
            },
            None,
        );
        assert!(!said.contains("every rate here is a floor"));
        assert!(
            !said.contains("a sheet in Excel holds"),
            "the last row a sheet holds is still one"
        );
    }

    #[test]
    fn the_capture_brought_up_to_date_after_the_reading_is_said_and_a_moved_point_left_out() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(
            out.path(),
            3,
            &[
                review("1", "Bugs everywhere. The music is lovely."),
                review("2", "Bugs everywhere."),
            ],
        );
        let crawl = |swept: i64| {
            std::fs::write(
                snapshot.join("crawl.json"),
                json!({"app_id": 3, "name": "Three", "rows_unique": 2, "snapshot_unix": CRAWLED,
                       "swept_unix": swept})
                .to_string(),
            )
            .unwrap();
        };
        crawl(CRAWLED);
        let (_, _) = export(out.path(), 3);
        let folder = out.path().join("Test Game data");
        let readme = std::fs::read_to_string(folder.join("README.txt")).unwrap();
        assert!(
            !readme.contains("brought up to date on"),
            "swept as it was read"
        );
        std::fs::remove_dir_all(&folder).unwrap();

        // The first review edited since: its words no longer sit where they were read.
        let mut writer =
            crate::capture::CaptureWriter::create(&snapshot.join("shard-0000.parquet"), 3).unwrap();
        let edited = [
            review("1", "The music is lovely. Bugs everywhere."),
            review("2", "Bugs everywhere."),
        ];
        writer.write(&edited.iter().collect::<Vec<_>>()).unwrap();
        writer.close().unwrap();
        crawl(CRAWLED + 1);
        let (exported, _) = export(out.path(), 3);
        assert_eq!((exported.points, exported.left_out), (1, 2));
        let readme = std::fs::read_to_string(folder.join("README.txt")).unwrap();
        assert!(readme.contains("brought up to date on 2023-11-14"));
        assert!(readme.contains("2 points are left out of points.csv"));
        let points = read(&folder, "points");
        assert_eq!(points.json.len(), 1);
        assert_eq!(points.json[0]["review_id"], "2");
    }

    #[test]
    fn an_export_never_writes_over_anything_and_leaves_nothing_when_it_stops() {
        let out = crate::tempdir::Dir::new();
        read_corpus_of(out.path(), 1);
        let to = out.path().join("Test Game data");
        let partial = out.path().join("Test Game data.partial");
        let never = AtomicBool::new(false);

        std::fs::create_dir(&to).unwrap();
        std::fs::write(to.join("mine.txt"), "kept").unwrap();
        let refused = game(out.path(), 1, &to, &never, |_, _| {}).unwrap_err();
        assert!(matches!(refused, Error::Refused(ref why) if why.contains("is there already")));
        assert_eq!(
            std::fs::read_to_string(to.join("mine.txt")).unwrap(),
            "kept"
        );
        std::fs::remove_dir_all(&to).unwrap();

        std::fs::create_dir(&partial).unwrap();
        let refused = game(out.path(), 1, &to, &never, |_, _| {}).unwrap_err();
        assert!(matches!(refused, Error::Refused(ref why) if why.contains("did not finish")));
        assert!(partial.exists() && !to.exists());
        std::fs::remove_dir(&partial).unwrap();

        let stopped = game(out.path(), 1, &to, &AtomicBool::new(true), |_, _| {}).unwrap_err();
        assert!(matches!(stopped, Error::Stopped));
        assert!(
            !to.exists() && !partial.exists(),
            "a stopped export leaves nothing"
        );

        let unread = crate::read::tests::corpus_of(out.path(), 4);
        let refused = game(out.path(), 4, &to, &never, |_, _| {}).unwrap_err();
        assert!(
            matches!(refused, Error::NoClassifications { ref path } if *path == unread.join("reading.json"))
        );
        assert!(!partial.exists());
    }
}
