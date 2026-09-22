//! Drawing claims to be labelled, and handing them out in batches.
//!
//! A batch is a review with its claims numbered, never a heap of loose claims. Two reasons.
//! A claim on its own is often unreadable ("it doesn't", "same here", "this one too"), and
//! the labeller needs the review around it to know what it refers to. And sending the review
//! once with its claims enumerated costs a fraction of sending the review again for every
//! claim it contains.
//!
//! Selection is by hash of the review id, so it depends only on the review: a corpus that
//! gains reviews does not renumber the ones already labelled, and the same seed draws the
//! same sample on any machine.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;

/// One claim as it is handed to a labeller and recorded in the sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawnClaim {
    pub index: u16,
    /// Byte offsets into the review as captured. The published label set carries these
    /// rather than the text, so a labelled span can be recovered from Steam by anyone
    /// without this project redistributing a word anybody wrote.
    pub start: u32,
    pub end: u32,
    pub text: String,
}

/// One review, split, as it is handed out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawnReview {
    pub id: String,
    pub app_id: u32,
    pub language: String,
    /// `stratified` is training data and `random` is held back. A pilot draws only random,
    /// because the first question is what a corpus actually contains.
    pub subset: String,
    pub claims: Vec<DrawnClaim>,
    /// Which claims the labeller is asked about, where that is not all of them. The rest are
    /// still handed over, because they are the review: a claim reading "it doesn't" cannot be
    /// labelled without the sentence before it, and a draw that asks about one claim in a
    /// review must still show the review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<Vec<u16>>,
}

impl DrawnReview {
    /// Whether the labeller is asked about this claim, rather than shown it for context.
    #[must_use]
    pub fn asks(&self, index: u16) -> bool {
        self.asked
            .as_ref()
            .is_none_or(|asked| asked.contains(&index))
    }

    /// How many claims of this review are being asked about.
    #[must_use]
    pub fn asked_count(&self) -> usize {
        self.asked.as_ref().map_or(self.claims.len(), Vec::len)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrawReport {
    pub reviews: usize,
    pub claims: usize,
    pub batches: usize,
}

impl DrawReport {
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a sample is thousands of claims, not 2^53 of them"
    )]
    pub fn per_review(&self) -> f64 {
        if self.reviews == 0 {
            return 0.0;
        }
        self.claims as f64 / self.reviews as f64
    }
}

/// Draws reviews at random from a corpus and splits each into its claims.
///
/// Every claim of a drawn review is labelled, never a subset of them: a review labelled in
/// part cannot say what share of a corpus is contentless, which is the first thing the pilot
/// has to answer.
///
/// # Errors
///
/// Fails if there is no capture for the app.
pub fn draw(
    out_dir: &Path,
    app_id: u32,
    wanted: usize,
    english_share: f64,
    seed: u64,
    subset: &str,
) -> Result<Vec<DrawnReview>> {
    let snapshot = crate::embed::latest_snapshot(out_dir, app_id)?;
    // Two draws rather than one, so the mix is decided rather than inherited. A corpus is
    // whatever languages its players write in, and drawing straight from it would train the
    // model mostly on whichever one that happens to be. The reports default to English and
    // the model has to hold up in the rest, so the split is set here and recorded.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a sample is hundreds of reviews, not 2^53 of them"
    )]
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "english_share is a share and wanted is a count"
    )]
    let english_wanted = (wanted as f64 * english_share.clamp(0.0, 1.0)).round() as usize;
    let mut english: crate::bounded::Smallest<[u8; 32], DrawnReview> =
        crate::bounded::Smallest::new(english_wanted);
    let mut rest: crate::bounded::Smallest<[u8; 32], DrawnReview> =
        crate::bounded::Smallest::new(wanted.saturating_sub(english_wanted));

    crate::capture::for_each_body(&snapshot, |id, language, text| {
        let found = crate::claims::claims_of(text);
        if found.is_empty() {
            return Ok(());
        }
        let claims = found
            .into_iter()
            .enumerate()
            .map(|(index, (at, claim))| DrawnClaim {
                index: u16::try_from(index).unwrap_or(u16::MAX),
                start: u32::try_from(at.start).unwrap_or(u32::MAX),
                end: u32::try_from(at.end).unwrap_or(u32::MAX),
                text: claim.into_owned(),
            })
            .collect();
        let drawn = DrawnReview {
            id: id.to_owned(),
            app_id,
            language: language.to_owned(),
            subset: subset.to_owned(),
            claims,
            asked: None,
        };
        let key = crate::bounded::rank(seed, "claims", id);
        if language == "english" {
            english.offer(key, drawn);
        } else {
            rest.offer(key, drawn);
        }
        Ok(())
    })?;

    let mut drawn = english.take();
    drawn.extend(rest.take());
    drawn.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(drawn)
}

/// What a labeller is shown: one review, its claims numbered, and nothing else.
///
/// No app id, no rating, no language name, no prediction. The model reads the claim and the
/// review around it, so a label made from more than that measures what the labeller was told
/// rather than how well the text reads.
#[derive(Debug, Clone, Serialize)]
struct Handout<'a> {
    review_id: &'a str,
    review: String,
    claims: Vec<HandoutClaim<'a>>,
}

#[derive(Debug, Clone, Serialize)]
struct HandoutClaim<'a> {
    index: u16,
    text: &'a str,
}

/// Writes the drawn sample and the batches to hand out.
///
/// # Errors
///
/// Fails if the directory cannot be created or a file cannot be written.
pub fn write_set(
    dir: &Path,
    drawn: &[DrawnReview],
    reviews_per_batch: usize,
) -> Result<DrawReport> {
    write_set_handing_out(
        dir,
        drawn,
        reviews_per_batch,
        &std::collections::HashSet::new(),
    )
}

/// Writes the drawn sample, and batches for the reviews nobody has answered yet.
///
/// A reading that grows keeps the whole draw in its sample, so the labels already made stay
/// in question and the ingest keeps them, and hands out only the rest. The report counts what
/// is handed out, because that is the labeller's work.
///
/// # Errors
///
/// Fails if the directory cannot be created or a file cannot be written.
pub fn write_set_handing_out<S: std::hash::BuildHasher>(
    dir: &Path,
    drawn: &[DrawnReview],
    reviews_per_batch: usize,
    answered: &std::collections::HashSet<String, S>,
) -> Result<DrawReport> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("sample.json"), serde_json::to_vec_pretty(&drawn)?)?;
    let to_hand_out: Vec<&DrawnReview> = drawn
        .iter()
        .filter(|review| !answered.contains(&review.id))
        .collect();

    let batches = dir.join("batches");
    std::fs::create_dir_all(&batches)?;
    // A smaller draw than last time leaves the tail of the previous one on disk, and those
    // files look exactly like work to hand out.
    if let Ok(entries) = std::fs::read_dir(&batches) {
        for stale in entries.filter_map(std::result::Result::ok) {
            let path = stale.path();
            let named = path.extension().is_some_and(|kind| kind == "json")
                && path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("batch-"));
            if named {
                std::fs::remove_file(&path)?;
            }
        }
    }

    let mut written = 0;
    if reviews_per_batch > 0 {
        for (index, chunk) in to_hand_out.chunks(reviews_per_batch).enumerate() {
            let items: Vec<Handout<'_>> = chunk
                .iter()
                .map(|&review| Handout {
                    review_id: &review.id,
                    review: rejoined(review),
                    claims: review
                        .claims
                        .iter()
                        .filter(|claim| review.asks(claim.index))
                        .map(|claim| HandoutClaim {
                            index: claim.index,
                            text: &claim.text,
                        })
                        .collect(),
                })
                .collect();
            std::fs::write(
                batches.join(format!("batch-{index:03}.json")),
                serde_json::to_vec_pretty(&items)?,
            )?;
            written += 1;
        }
    }

    Ok(DrawReport {
        reviews: to_hand_out.len(),
        claims: to_hand_out.iter().map(|review| review.asked_count()).sum(),
        batches: written,
    })
}

/// The reviews of a reading whose every asked claim already has a label.
///
/// A review the labeller skipped a claim of is not answered: it goes out again whole, because
/// a claim cannot be read without the review around it.
///
/// # Errors
///
/// Fails if the reading's labels cannot be read.
pub fn answered_reviews(
    dir: &Path,
    drawn: &[DrawnReview],
) -> Result<std::collections::HashSet<String>> {
    let Ok(bytes) = std::fs::read(dir.join("labels.json")) else {
        return Ok(std::collections::HashSet::new());
    };
    let labels: Vec<ClaimLabel> = serde_json::from_slice(&bytes)?;
    let labelled: std::collections::HashSet<(&str, u16)> = labels
        .iter()
        .map(|label| (label.review_id.as_str(), label.index))
        .collect();
    Ok(drawn
        .iter()
        .filter(|review| {
            review
                .claims
                .iter()
                .filter(|claim| review.asks(claim.index))
                .all(|claim| labelled.contains(&(review.id.as_str(), claim.index)))
        })
        .map(|review| review.id.clone())
        .collect())
}

/// The review as the labeller reads it, rebuilt from its claims.
///
/// Rebuilt rather than stored a second time: what the labeller sees is exactly what was
/// split, so a claim that reads oddly is visibly the splitter's doing rather than a
/// discrepancy between two copies of the text.
fn rejoined(review: &DrawnReview) -> String {
    review
        .claims
        .iter()
        .map(|claim| claim.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Which sheet a labeller was working from, and which cut of the claims they were shown.
///
/// Not the versions this build holds. A labeller handed a set on Monday answers Monday's
/// sheet however long they take, and a revision that lands while they work does not reach
/// back and change what they were asked. Stamping the build's version at ingest is how a
/// set comes to claim it was labelled against rules its labeller never saw.
#[derive(Debug, Clone)]
pub struct Sheet {
    pub taxonomy: String,
    /// The labeller. Named rather than defaulted: the first set written by a second model is
    /// the one where a default would be wrong, and it is also the one nobody would think to
    /// check.
    pub produced_by: String,
}

impl Default for Sheet {
    /// What this build would hand out now, which is right for a set drawn and labelled
    /// without a revision in between.
    fn default() -> Self {
        Self {
            taxonomy: crate::taxonomy::sheet(),
            produced_by: String::new(),
        }
    }
}

/// One returned label, as a labeller writes it.
#[derive(Debug, Clone, Deserialize)]
pub struct ReturnedClaimLabel {
    pub review_id: String,
    pub index: u16,
    pub subject: String,
    pub polarity: String,
    pub ironic: bool,
    pub confidence: String,
    pub ambiguous: bool,
    #[serde(default)]
    pub split_wrong: bool,
}

/// One label as it is stored, joined back to what was drawn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimLabel {
    pub review_id: String,
    pub index: u16,
    pub app_id: u32,
    pub language: String,
    pub subset: String,
    pub start: u32,
    pub end: u32,
    /// The wording the labeller read, as a fingerprint of the whole brief. A label answers
    /// the sheet it was written against, boundary rules and all, and `revisit` is how the ones
    /// predating a clarification are found again. Which splitter cut the claim is not recorded:
    /// the label names the bytes, and whether this build still cuts a claim there is answered
    /// by asking the corpus rather than by comparing two version strings.
    #[serde(default)]
    pub taxonomy: String,
    /// Which labeller wrote it. A set labelled by two models is not one set: they disagree
    /// with each other about as often as they disagree with the truth, and pooling them
    /// without saying so turns a measurable difference into noise nobody can find again.
    #[serde(default)]
    pub produced_by: String,
    pub subject: String,
    pub polarity: String,
    pub ironic: bool,
    pub confidence: String,
    pub ambiguous: bool,
    pub split_wrong: bool,
}

/// What was wrong with a returned set, in the labeller's own terms.
#[derive(Debug, Clone, Default)]
pub struct ClaimIngest {
    pub accepted: usize,
    /// Labels already in the set for claims the returned files do not answer, kept as they
    /// were: a set is labelled in sittings, and a later sitting files beside the earlier one.
    pub kept: usize,
    /// Labels whose subject the revision moved. The measure of what a rule change was worth:
    /// a rule that moves nothing was already understood, and one that moves everything was
    /// not a clarification.
    pub moved: u32,
    /// Claims that were drawn and came back with no label. A partly labelled review cannot
    /// say what share of a corpus names no aspect, so this is a failure rather than a gap.
    pub missing: Vec<String>,
    /// Labels naming a claim that was never drawn.
    pub unknown: Vec<String>,
    /// Labels whose subject, polarity or confidence is not one the sheet offers.
    pub rejected: Vec<String>,
}

impl ClaimIngest {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.unknown.is_empty() && self.rejected.is_empty()
    }
}

/// Draws the share of an already-drawn set that a second labeller should read.
///
/// A set labelled once cannot say anything about its own reliability, so a tenth of it is
/// labelled again by a different labeller and the two are compared. The same reviews, drawn by
/// hash so that the choice depends on the review rather than on the order of the file: a set
/// redrawn after more games are added asks for a second opinion on the same reviews it already
/// has one for.
///
/// The purpose string differs from the one that drew the set, so this is not the front of the
/// original draw. Taking the first tenth of an ordering the set was already built from would
/// ask the second labeller about a systematically unrepresentative slice.
///
/// # Errors
///
/// Fails if the set has no drawn sample, or the sample cannot be read.
pub fn draw_second(dir: &Path, share: f64, seed: u64) -> Result<Vec<DrawnReview>> {
    let drawn: Vec<DrawnReview> =
        serde_json::from_slice(&std::fs::read(dir.join("sample.json")).map_err(|_| {
            crate::Error::NoReferenceSet {
                path: dir.join("sample.json"),
            }
        })?)?;

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a reference set is a few hundred reviews"
    )]
    let wanted = ((drawn.len() as f64) * share.clamp(0.0, 1.0)).round() as usize;

    let mut ranked: Vec<([u8; 32], DrawnReview)> = drawn
        .into_iter()
        .map(|review| {
            (
                crate::bounded::rank(seed, "second-opinion", &review.id),
                review,
            )
        })
        .collect();
    ranked.sort_by_key(|(key, _)| *key);
    Ok(ranked
        .into_iter()
        .take(wanted)
        .map(|(_, review)| review)
        .collect())
}

/// Draws the claims of a labelled set that a revision of the sheet puts back in question.
///
/// A taxonomy revision does not invalidate a set: the subjects that survive it mean what they
/// meant, and re-asking about all of them costs what the set cost. What it does is move
/// boundaries, and only claims near a moved one can change. This finds them by what they say,
/// because a claim about a subject the sheet has just learned to name almost always says its
/// name: eight games' worth of "modding" sat under `content` and `updates` without a single
/// one of them failing to use the word.
///
/// Matched by the same word cut that counts the terms a report shows, so "mod" finds "mod"
/// and "mods" and not "modern", and a caller need not know how a word is bounded in a script
/// that writes without spaces.
///
/// Narrowed further by which subjects a claim is currently filed under, where the caller
/// names any: a rule moves a boundary between two rows, and a claim on neither side of it
/// cannot cross. "Worth" appears in claims about eleven subjects and the rule about what a
/// thing is worth paying touches two of them.
///
/// With subjects and no words, every claim under them is asked about. A revision that narrows
/// a row rather than teaching the sheet a new name puts the whole row back in question, and
/// there are no words for that: `accessibility` was redefined from a purpose to a test, and
/// what has to be re-asked is each of its claims, not the ones that happen to say "subtitle".
///
/// Returns the claims that matched, as a set the labeller reads exactly like a fresh one.
///
/// # Errors
///
/// Fails if the set has no drawn sample or labels, or they cannot be read.
pub fn draw_revisit(dir: &Path, words: &[String], subjects: &[String]) -> Result<Vec<DrawnReview>> {
    let drawn: Vec<DrawnReview> =
        serde_json::from_slice(&std::fs::read(dir.join("sample.json")).map_err(|_| {
            crate::Error::NoReferenceSet {
                path: dir.join("sample.json"),
            }
        })?)?;
    let labels: Vec<ClaimLabel> =
        serde_json::from_slice(&std::fs::read(dir.join("labels.json")).map_err(|_| {
            crate::Error::NoReferenceSet {
                path: dir.join("labels.json"),
            }
        })?)?;
    let labelled: std::collections::HashSet<(&str, u16)> = labels
        .iter()
        .filter(|label| subjects.is_empty() || subjects.contains(&label.subject))
        .map(|label| (label.review_id.as_str(), label.index))
        .collect();

    Ok(drawn
        .iter()
        .filter_map(|review| {
            // The whole review goes to the labeller, as it did the first time, but only the
            // claims in question are asked about: a claim shown without the rest of its
            // review is a claim nobody can label, and a claim nobody asked about is one
            // whose existing label stands.
            let asked: Vec<u16> = review
                .claims
                .iter()
                .filter(|claim| labelled.contains(&(review.id.as_str(), claim.index)))
                .filter(|claim| {
                    words.is_empty()
                        || words
                            .iter()
                            .any(|word| crate::said::mentions(&claim.text, word))
                })
                .map(|claim| claim.index)
                .collect();
            (!asked.is_empty()).then(|| DrawnReview {
                asked: Some(asked),
                ..review.clone()
            })
        })
        .collect())
}

/// Sets beside a game's random draw that add claims to train on rather than answers to
/// compare. Every one of them is labelled as its own `subset`, and no prevalence figure
/// counts a row from any of them.
pub const TEACHING_SETS: &[&str] = &["declined", "mined", "retrieved", "multilingual"];

/// The directories that hold another labeller's answers to claims a set already has, newest
/// first, because the first one holding a claim is the one that answers it.
///
/// Both are blind readings of the same drawn batches with no labels in them. `second` is what
/// `second-opinion` writes, a share of each set and the frozen games in full; `opus` is what a
/// second model wrote reading twenty-six games from end to end. Reading only the first of them
/// left 9,072 second opinions on training claims unused, 1,069 of them disagreements, which is
/// most of what a loss charged against two labellers has to work with.
pub const SECOND_READINGS: &[&str] = &["second", "opus"];

/// Draws the claims the reader would not answer, as a set to teach it on.
///
/// Every set so far is a random draw, which is what makes prevalence measurable and is the
/// right default: a set drawn for being hard cannot say what share of a corpus mentions
/// price. But once a reader exists, the claims it abstains on are worth several times a random
/// claim to label, because a random draw spends most of its budget confirming answers the
/// model already gets right.
///
/// So this is a teaching draw and it is marked as one. Every review it produces carries
/// `subset: "declined"`, which keeps it out of every prevalence figure and out of the
/// validation and frozen games entirely: a model measured on the claims it was known to find
/// hard would report an accuracy nobody can interpret.
///
/// Drawn uniformly from the declined claims rather than from the least confident of them. The
/// bottom of a confidence ordering is mostly text with nothing in it, and a set of that
/// teaches the model to say `offtopic` rather than to read anything.
///
/// A review already in the game's reference set is never drawn, so no review is labelled twice
/// under two different draws.
///
/// # Errors
///
/// Fails if there is no capture, no reading, or the reading was cut by another splitter.
pub fn draw_declined(
    out_dir: &Path,
    app_id: u32,
    dir: &Path,
    wanted: usize,
    seed: u64,
) -> Result<Vec<DrawnReview>> {
    let snapshot = crate::embed::latest_snapshot(out_dir, app_id)?;
    let reading: crate::read::ReadReport =
        serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).map_err(|_| {
            crate::Error::NoClassifications {
                path: snapshot.join("reading.json"),
            }
        })?)?;

    let already = already_drawn(dir);

    let mut chosen: crate::bounded::Smallest<[u8; 32], (String, crate::claims::Span)> =
        crate::bounded::Smallest::new(wanted);
    crate::read::for_each_reading(
        &snapshot.join("readings.parquet"),
        |id, at, subject, _, _| {
            if subject.is_some() || already.contains(id) {
                return;
            }
            let key =
                crate::bounded::rank(seed, "declined", &format!("{id}\u{0}{}\u{0}{}", at.0, at.1));
            chosen.offer(key, (id.to_owned(), at));
        },
    )?;

    let mut picks: Picks = std::collections::HashMap::new();
    for (id, at) in chosen.take() {
        picks.entry(id).or_default().push(at);
    }

    handouts(&snapshot, app_id, reading.depth, &picks, "declined")
}

/// What this build cuts from one review: its text as the capture holds it now, and the spans
/// of the claims cut from it. The text travels with the spans because a label's span is met
/// only once it is brought to its words, and that needs the review it was written about.
#[derive(Debug, Clone, Default)]
pub struct Cut {
    pub text: String,
    pub spans: std::collections::HashSet<crate::claims::Span>,
}

/// What this build cuts, keyed by review id, for the reviews that were asked about.
pub type CutSpans = std::collections::HashMap<String, Cut>;

/// Which claims of which reviews a draw wants, named by the bytes they cover.
pub(crate) type Picks = std::collections::HashMap<String, Vec<crate::claims::Span>>;

/// One line of a draw: the claims it caught, in the order it ranked them.
pub(crate) type Caught = Vec<(String, crate::claims::Span)>;

/// What this build's splitter cuts from the named reviews of one game's capture.
///
/// A stored label names a byte span, and the sets here were cut by four different splitters.
/// Where this build no longer cuts a label's span, the words it names are still in the review
/// but they are not a claim any more: usually a template heading the splitter has since joined
/// to the option it labels, sometimes a comma list it has since taken apart. That is not a hard
/// question, it is one with nothing to answer, and it costs whoever is asked the same as a real
/// one.
///
/// Only the reviews named are split. The capture is walked whole, which is cheap; splitting a
/// game of three million claims to check a few hundred labels is not.
///
/// # Errors
///
/// Fails if the capture cannot be read.
pub fn spans_cut_now<S: std::hash::BuildHasher>(
    out_dir: &Path,
    app_id: u32,
    ids: &std::collections::HashSet<String, S>,
) -> Result<CutSpans> {
    let snapshot = crate::embed::latest_snapshot(out_dir, app_id)?;
    let mut cut = CutSpans::new();
    crate::capture::for_each_body(&snapshot, |id, _, text| {
        if !ids.contains(id) {
            return Ok(());
        }
        let spans = crate::claims::claims_of(text)
            .into_iter()
            .filter_map(|(at, _)| {
                Some((u32::try_from(at.start).ok()?, u32::try_from(at.end).ok()?))
            })
            .collect();
        cut.insert(
            id.to_owned(),
            Cut {
                text: text.to_owned(),
                spans,
            },
        );
        Ok(())
    })?;
    Ok(cut)
}

/// Where this build cuts the claim a label names, or nothing where it cuts no claim there.
///
/// The label's span is brought to its words first: a span from an older cut may still carry
/// the tag or the bullet in front of them, and the words are what a claim is. This is the one
/// answer to whether a label still names a claim, and the measure joins its readings by the
/// same span, so what the draw holds back and what the measure leaves unjoined are the same
/// labels.
#[must_use]
pub fn cut_at(cut: &CutSpans, label: &ClaimLabel) -> Option<crate::claims::Span> {
    let review = cut.get(label.review_id.as_str())?;
    let words = crate::claims::words_at(&review.text, label.start, label.end)?;
    review.spans.contains(&words).then_some(words)
}

/// Draws claims that look like they belong to the subjects the labelled set is starved of.
///
/// The declined draw above asks the reader what it found hard. This asks a different question,
/// because for eight of the twenty-six subjects the reader's opinion is worthless: it has seen
/// thirty-two `licensing` claims and it does not know the row exists. A draw that waits for the
/// model to be uncertain about `vr` will wait forever, and a random draw over a corpus where
/// `vr` is two claims in a thousand spends its whole budget elsewhere.
///
/// So the claims are found by looking for them. [`crate::mine::PROBES`] says where each starved
/// subject tends to be written about, a claim goes to the first subject whose line it takes,
/// and the quota is filled round-robin so a game with plenty of `mods` claims and no `vr` ones
/// still returns a full draw without `mods` eating it.
///
/// **Nothing here is a sample of anything.** Every review carries `subset: "mined"`, which
/// keeps it out of every prevalence figure exactly as the declined draw is kept out. A set
/// selected for containing the word "headset" cannot say what share of a corpus is about
/// headsets, and the labels it produces are for training only.
///
/// A review already in the game's reference set is never drawn twice.
///
/// # Errors
///
/// Fails if there is no capture, no reading, or the reading was cut by another splitter.
pub fn draw_mined(
    out_dir: &Path,
    app_id: u32,
    dir: &Path,
    wanted: usize,
    seed: u64,
    only: &[String],
) -> Result<Mined> {
    let snapshot = crate::embed::latest_snapshot(out_dir, app_id)?;
    let reading: crate::read::ReadReport =
        serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).map_err(|_| {
            crate::Error::NoClassifications {
                path: snapshot.join("reading.json"),
            }
        })?)?;

    let already = already_drawn(dir);

    let depth = reading.depth;
    let mut lines: Vec<crate::bounded::Smallest<[u8; 32], (String, crate::claims::Span)>> =
        crate::mine::PROBES
            .iter()
            .map(|_| crate::bounded::Smallest::new(wanted))
            .collect();
    crate::capture::for_each_body(&snapshot, |id, _, text| {
        if already.contains(id) {
            return Ok(());
        }
        for ((index, claim), span) in depth
            .claims_of(text)
            .into_iter()
            .enumerate()
            .zip(depth.spans_of(text))
        {
            let Some(subject) = crate::mine::hooked_among(&claim, only) else {
                continue;
            };
            let at = crate::mine::PROBES
                .iter()
                .position(|probe| probe.subject == subject)
                .unwrap_or(0);
            let index = u16::try_from(index).unwrap_or(u16::MAX);
            let key = crate::bounded::rank(seed, "mined", &format!("{id}\u{0}{index}"));
            lines[at].offer(
                key,
                (
                    id.to_owned(),
                    (
                        u32::try_from(span.start).unwrap_or(u32::MAX),
                        u32::try_from(span.end).unwrap_or(u32::MAX),
                    ),
                ),
            );
        }
        Ok(())
    })?;

    let caught: Vec<Caught> = lines
        .into_iter()
        .map(crate::bounded::Smallest::take)
        .collect();
    let (picks, taken) = round_robin(&caught, wanted);
    Ok(Mined {
        drawn: handouts(&snapshot, app_id, depth, &picks, "mined")?,
        by_line: crate::mine::PROBES
            .iter()
            .zip(taken)
            .map(|(probe, count)| (probe.subject, count))
            .collect(),
    })
}

/// A mining draw and which line caught what.
///
/// The counts are the only way to tell a probe list that is working from one that is not,
/// short of labelling the result: a line that catches nothing across a whole library is
/// written wrong or aimed at a subject the corpus does not discuss, and either way the reader
/// will not learn that row from this draw.
#[derive(Debug)]
pub struct Mined {
    pub drawn: Vec<DrawnReview>,
    pub by_line: Vec<(&'static str, usize)>,
}

/// The reviews a game's reference set already holds, under any draw, so no review is drawn
/// twice under two different ones.
///
/// Every teaching set counts, not only the random draw. A review drawn as `mined` and again
/// as `retrieved` would be labelled twice and trained on twice, and where the two labellers
/// disagreed the model would be trained on both answers.
pub(crate) fn already_drawn(dir: &Path) -> std::collections::HashSet<String> {
    std::iter::once(dir.to_path_buf())
        .chain(TEACHING_SETS.iter().map(|name| dir.join(name)))
        .filter_map(|set| std::fs::read(set.join("sample.json")).ok())
        .filter_map(|bytes| serde_json::from_slice::<Vec<DrawnReview>>(&bytes).ok())
        .flatten()
        .map(|review| review.id)
        .collect()
}

/// Takes from each subject's line in turn until the quota is full or the lines run dry.
///
/// Taking the best `wanted / 8` from each instead would leave the draw short whenever a game
/// has none of a subject, which for `vr` is most games. Round-robin spends what one subject
/// cannot use on the subjects that can, while still giving the starved rows first refusal.
pub(crate) fn round_robin(caught: &[Caught], wanted: usize) -> (Picks, Vec<usize>) {
    let mut picks: Picks = std::collections::HashMap::new();
    let mut taken = vec![0; caught.len()];
    let deepest = caught.iter().map(Vec::len).max().unwrap_or(0);
    for round in 0..deepest {
        for (line, at) in caught.iter().zip(0..) {
            if taken.iter().sum::<usize>() >= wanted {
                return (picks, taken);
            }
            let Some((id, index)) = line.get(round) else {
                continue;
            };
            picks.entry(id.clone()).or_default().push(*index);
            taken[at] += 1;
        }
    }
    (picks, taken)
}

/// Builds the handout for a set of chosen claims: whole reviews, with the claims asked about
/// named by index.
///
/// The review is whole because a claim like "it doesn't" is unanswerable without it, and the
/// `asked` list is what keeps the labeller from being charged for the rest of it. An index the
/// corpus has outgrown is dropped here: a review edited between the read and the draw is cut
/// into different claims, and the index chosen then names a different sentence now.
pub(crate) fn handouts(
    snapshot: &Path,
    app_id: u32,
    depth: crate::read::Depth,
    picks: &Picks,
    subset: &str,
) -> Result<Vec<DrawnReview>> {
    let mut drawn = Vec::new();
    crate::capture::for_each_body(snapshot, |id, language, text| {
        let Some(wanted) = picks.get(id) else {
            return Ok(());
        };
        let spans = depth.spans_of(text);
        let claims: Vec<DrawnClaim> = depth
            .claims_of(text)
            .into_iter()
            .enumerate()
            .zip(spans)
            .map(|((index, claim), span)| DrawnClaim {
                index: u16::try_from(index).unwrap_or(u16::MAX),
                start: u32::try_from(span.start).unwrap_or(u32::MAX),
                end: u32::try_from(span.end).unwrap_or(u32::MAX),
                text: claim.into_owned(),
            })
            .collect();
        // The handout numbers the claims of this review as it is cut now, and a pick names the
        // bytes it wants rather than a place in that numbering, so the two are matched here.
        let mut asked: Vec<u16> = claims
            .iter()
            .filter(|claim| wanted.contains(&(claim.start, claim.end)))
            .map(|claim| claim.index)
            .collect();
        asked.sort_unstable();
        asked.dedup();
        if asked.is_empty() {
            return Ok(());
        }
        drawn.push(DrawnReview {
            id: id.to_owned(),
            app_id,
            language: language.to_owned(),
            subset: subset.to_owned(),
            claims,
            asked: Some(asked),
        });
        Ok(())
    })?;

    drawn.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(drawn)
}

/// Replaces the labels of the revisited claims, leaving every other label alone.
///
/// The returned labels are the answers under the sheet as it now reads, so they carry the
/// current taxonomy while the rest of the set carries the one it was labelled under. A set
/// where every row claimed the current version would be a set that had quietly relabelled
/// itself.
///
/// # Errors
///
/// Fails if the set or the returned files cannot be read or written.
pub fn ingest_revisit(dir: &Path, from: &Path, by: &str) -> Result<ClaimIngest> {
    let mut labels: Vec<ClaimLabel> =
        serde_json::from_slice(&std::fs::read(dir.join("labels.json")).map_err(|_| {
            crate::Error::NoReferenceSet {
                path: dir.join("labels.json"),
            }
        })?)?;
    let mut held: std::collections::HashMap<(String, u16), &mut ClaimLabel> = labels
        .iter_mut()
        .map(|label| ((label.review_id.clone(), label.index), label))
        .collect();

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(from)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .collect();
    files.sort();

    let mut report = ClaimIngest::default();
    let mut changed = 0;
    for file in files {
        let returned: Vec<ReturnedClaimLabel> = serde_json::from_slice(&std::fs::read(&file)?)?;
        for answer in returned {
            let Some(label) = held.get_mut(&(answer.review_id.clone(), answer.index)) else {
                report
                    .unknown
                    .push(format!("{}#{}", answer.review_id, answer.index));
                continue;
            };
            if !crate::taxonomy::SHEET
                .iter()
                .any(|category| category.id == answer.subject)
                || !crate::taxonomy::POLARITY.contains(&answer.polarity.as_str())
                || !crate::taxonomy::CONFIDENCE.contains(&answer.confidence.as_str())
            {
                report.rejected.push(format!(
                    "{}#{} {} / {} / {}",
                    answer.review_id,
                    answer.index,
                    answer.subject,
                    answer.polarity,
                    answer.confidence
                ));
                continue;
            }
            changed += u32::from(label.subject != answer.subject);
            label.subject = answer.subject;
            label.polarity = answer.polarity;
            label.ironic = answer.ironic;
            label.confidence = answer.confidence;
            label.ambiguous = answer.ambiguous;
            label.split_wrong = answer.split_wrong;
            crate::taxonomy::sheet().clone_into(&mut label.taxonomy);
            // A revisited label is a new answer from whoever gave it, not a correction of the
            // first labeller's, so it carries the second labeller's name.
            by.clone_into(&mut label.produced_by);
            report.accepted += 1;
        }
    }

    std::fs::write(dir.join("labels.json"), serde_json::to_vec_pretty(&labels)?)?;
    report.moved = changed;
    Ok(report)
}

/// Where the claim reference sets live.
#[must_use]
pub fn reference_root() -> std::path::PathBuf {
    std::path::PathBuf::from("reference").join("claims")
}

/// Where one game's claim reference set lives.
#[must_use]
pub fn default_reference_dir(app_id: u32) -> std::path::PathBuf {
    reference_root().join(app_id.to_string())
}

/// Merges returned label files into a claim reference set.
///
/// # Errors
///
/// Fails if the sample or the returned files cannot be read.
pub fn ingest(dir: &Path, from: &Path, sheet: &Sheet) -> Result<(Vec<ClaimLabel>, ClaimIngest)> {
    let drawn: Vec<DrawnReview> =
        serde_json::from_slice(&std::fs::read(dir.join("sample.json")).map_err(|_| {
            crate::Error::NoReferenceSet {
                path: dir.join("sample.json"),
            }
        })?)?;

    // Only the claims the draw asks about. The rest of a review is in the handout because a
    // claim cannot be read without it, and a label on one of those answers a question nobody
    // put: it would land in the set as though it had been drawn.
    let mut wanted: std::collections::HashMap<(String, u16), (&DrawnReview, &DrawnClaim)> =
        std::collections::HashMap::new();
    for review in &drawn {
        for claim in review
            .claims
            .iter()
            .filter(|claim| review.asks(claim.index))
        {
            wanted.insert((review.id.clone(), claim.index), (review, claim));
        }
    }

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(from)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .collect();
    files.sort();

    let mut report = ClaimIngest::default();
    let mut labels: Vec<ClaimLabel> = Vec::new();
    let mut seen: std::collections::HashSet<(String, u16)> = std::collections::HashSet::new();

    for file in files {
        let returned: Vec<ReturnedClaimLabel> = serde_json::from_slice(&std::fs::read(&file)?)?;
        for label in returned {
            let key = (label.review_id.clone(), label.index);
            let Some((review, claim)) = wanted.get(&key) else {
                report
                    .unknown
                    .push(format!("{}#{}", label.review_id, label.index));
                continue;
            };
            if !crate::taxonomy::SHEET
                .iter()
                .any(|category| category.id == label.subject)
                || !crate::taxonomy::POLARITY.contains(&label.polarity.as_str())
                || !crate::taxonomy::CONFIDENCE.contains(&label.confidence.as_str())
            {
                report.rejected.push(format!(
                    "{}#{} {} / {} / {}",
                    label.review_id, label.index, label.subject, label.polarity, label.confidence
                ));
                continue;
            }
            if !seen.insert(key) {
                continue;
            }
            labels.push(ClaimLabel {
                review_id: label.review_id,
                index: label.index,
                app_id: review.app_id,
                language: review.language.clone(),
                subset: review.subset.clone(),
                start: claim.start,
                end: claim.end,
                taxonomy: sheet.taxonomy.clone(),
                produced_by: sheet.produced_by.clone(),
                subject: label.subject,
                polarity: label.polarity,
                ironic: label.ironic,
                confidence: label.confidence,
                ambiguous: label.ambiguous,
                split_wrong: label.split_wrong,
            });
        }
    }

    report.accepted = labels.len();

    // A set is labelled in sittings: one labeller's five batches, then another's five a week
    // later. The claims these files do not answer keep the label they already have, sheet and
    // labeller and all, because a label written under an earlier wording is a fact about that
    // wording and not something a later sitting can re-stamp. Only a claim nobody has answered
    // is missing.
    if let Ok(bytes) = std::fs::read(dir.join("labels.json")) {
        let earlier: Vec<ClaimLabel> = serde_json::from_slice(&bytes)?;
        for label in earlier {
            let key = (label.review_id.clone(), label.index);
            if wanted.contains_key(&key) && seen.insert(key) {
                labels.push(label);
                report.kept += 1;
            }
        }
    }

    for (id, index) in wanted.keys() {
        if !seen.contains(&(id.clone(), *index)) {
            report.missing.push(format!("{id}#{index}"));
        }
    }
    report.missing.sort();
    labels.sort_by(|left, right| {
        (left.review_id.as_str(), left.index).cmp(&(right.review_id.as_str(), right.index))
    });

    // A merge that placed nothing is a merge aimed at the wrong set, and writing its result
    // replaces that set's labelling with the empty intersection. On 2026-09-18 this destroyed
    // the first labelling of four games, 2,068 labels, because the returned files belonged to
    // a draw beside the one being merged into and the default `--to` is the set itself. The
    // labels were recoverable from the training export; the record of which splitter cut them
    // and which sheet they answered was not, and cannot be, because most games mix both.
    if labels.is_empty() && !report.unknown.is_empty() {
        return Err(crate::Error::Refused(format!(
            "none of the {} labels name a claim {} drew, so this is another set's work: \
             nothing was written. Name the set they belong to with --to.",
            report.unknown.len(),
            dir.display()
        )));
    }

    std::fs::write(dir.join("labels.json"), serde_json::to_vec_pretty(&labels)?)?;
    Ok((labels, report))
}

/// A label joined back to the text it was written about.
///
/// The text is joined from the drawn sample rather than re-sliced from the capture, so
/// whatever reads it sees exactly the characters the labeller read. Holds review text, so it
/// never leaves the machine.
#[derive(Debug, Clone)]
pub struct LabelledClaim {
    pub label: ClaimLabel,
    /// The second labeller's answer to the same claim, where the set has been read twice.
    /// Carried beside the first rather than as a row of its own: a disagreement is a fact
    /// about the claim, and a trainer that sees both can learn that it is contested rather
    /// than learning one answer confidently.
    pub again: Option<ClaimLabel>,
    pub text: String,
    /// The review as the labeller was shown it, and where the claim starts in it. A labeller
    /// reads "it doesn't" with the sentence before it; a model given the claim alone is being
    /// asked a question nobody could answer, and the gap between what the two saw is
    /// measurable error attributed to the model.
    pub review: String,
    pub review_offset: usize,
}

/// What a second labeller said about the claims of one set, from whichever reading holds each.
///
/// [`SECOND_READINGS`] in order, first one wins: a claim read by both is answered by the
/// newer sheet's reading, and a claim only the older one covers is still answered.
fn read_again(set: &Path) -> Result<std::collections::HashMap<(String, u16), ClaimLabel>> {
    let mut again = std::collections::HashMap::new();
    for reading in SECOND_READINGS {
        let Ok(bytes) = std::fs::read(set.join(reading).join("labels.json")) else {
            continue;
        };
        for label in serde_json::from_slice::<Vec<ClaimLabel>>(&bytes)? {
            again
                .entry((label.review_id.clone(), label.index))
                .or_insert(label);
        }
    }
    Ok(again)
}

/// Every labelled claim under a reference root, with its text, in a fixed order.
///
/// A game's directory holds its random draw, and beside it the teaching draws named in
/// [`TEACHING_SETS`]. Named rather than "every subdirectory holding labels": `second/` holds a
/// second labeller's answers to claims the set already has, and sweeping those in would give
/// the same claim twice, on both answers wherever the two labellers disagreed.
///
/// # Errors
///
/// Fails if a reference set cannot be read.
pub fn labelled_claims(reference_root: &Path) -> Result<Vec<LabelledClaim>> {
    let mut sets: Vec<std::path::PathBuf> = std::fs::read_dir(reference_root)
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .flat_map(|game| {
            let teaching: Vec<std::path::PathBuf> =
                TEACHING_SETS.iter().map(|name| game.join(name)).collect();
            std::iter::once(game).chain(teaching)
        })
        .filter(|path| path.join("labels.json").exists() && path.join("sample.json").exists())
        .collect();
    sets.sort();

    let mut found = Vec::new();
    for set in sets {
        let labels: Vec<ClaimLabel> =
            serde_json::from_slice(&std::fs::read(set.join("labels.json"))?)?;
        let drawn: Vec<DrawnReview> =
            serde_json::from_slice(&std::fs::read(set.join("sample.json"))?)?;
        let mut again = read_again(&set)?;

        // The claim, and where it starts in the review around it. Searching for the text
        // instead would find the first copy of "Great game." in a review that says it twice,
        // and centre the window on the wrong half of the review.
        let mut text: std::collections::HashMap<(&str, u16), (&str, usize)> =
            std::collections::HashMap::new();
        let mut around: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
        for review in &drawn {
            let mut at = 0;
            for claim in &review.claims {
                text.insert((review.id.as_str(), claim.index), (claim.text.as_str(), at));
                at += claim.text.len() + 1;
            }
            around.insert(review.id.as_str(), rejoined(review));
        }

        for label in labels {
            let Some(&(claim, at)) = text.get(&(label.review_id.as_str(), label.index)) else {
                continue;
            };
            found.push(LabelledClaim {
                text: claim.to_owned(),
                review: around
                    .get(label.review_id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                review_offset: at,
                again: again.remove(&(label.review_id.clone(), label.index)),
                label,
            });
        }
    }
    Ok(found)
}

/// What the training export wrote, and what it would not write.
#[derive(Debug, Clone, Default)]
pub struct ExportReport {
    pub written: usize,
    /// Rows whose text carries no proposition.
    pub no_claim: usize,
    /// Teaching rows drawn from a game the model is measured on, and the games they sat on.
    pub measured_on: usize,
    pub measured_on_games: std::collections::BTreeSet<u32>,
    /// Rows naming bytes this build cuts no claim at.
    pub recut: usize,
    /// Claims two draws both handed out, folded into one row carrying both answers.
    pub twice: usize,
}

/// One row per claim, with a claim drawn twice keeping its second answer rather than both rows.
///
/// A teaching draw skips a review the game's sets already hold, so no claim should be labelled
/// under two of them. The draws made before that guard existed did not, and 616 claims sit in
/// two sets at once, 108 of them under two different subjects. Exported as two rows, the model
/// sees one claim twice an epoch and, on those 108, is taught both answers in the same pass.
///
/// The row from the random draw is the one kept, because that is the draw every figure is
/// measured on and the other is a teaching copy of it. The teaching label is not thrown away:
/// where the kept row has no second answer, it becomes one. Two labellers who read the same
/// claim without seeing each other's answer are a second reading, whichever draw handed it to
/// them, and the export already carries a column for exactly that.
fn read_once(found: Vec<LabelledClaim>, report: &mut ExportReport) -> Vec<LabelledClaim> {
    let teaching = |one: &LabelledClaim| TEACHING_SETS.contains(&one.label.subset.as_str());
    let mut place: std::collections::HashMap<(u32, String, u16), usize> =
        std::collections::HashMap::new();
    let mut kept: Vec<LabelledClaim> = Vec::with_capacity(found.len());

    for mut claim in found {
        let key = (
            claim.label.app_id,
            claim.label.review_id.clone(),
            claim.label.index,
        );
        let Some(&at) = place.get(&key) else {
            place.insert(key, kept.len());
            kept.push(claim);
            continue;
        };
        report.twice += 1;
        if teaching(&kept[at]) && !teaching(&claim) {
            std::mem::swap(&mut kept[at], &mut claim);
        }
        // Where the kept row already has a second answer, this third one is dropped: the
        // column holds one, and a blind reading of the whole set is the better one to hold.
        if kept[at].again.is_none() {
            kept[at].again = Some(claim.label);
        }
    }
    kept
}

/// Writes every labelled claim, with its text, as JSONL for training.
///
/// The file it writes holds review text and never leaves the machine: what gets published is
/// the label set, which carries ids and offsets and no text at all.
///
/// The captures are read because a label names bytes, and the sets were cut by several
/// splitters over the run. A row whose span this build cuts no claim at is not a label of any
/// claim the reader will ever be handed: at best it is a true label of a sentence that now
/// sits inside a larger claim, at worst one label over what are now two claims, taught under
/// one subject. The adjudication page has refused to ask about them since it was written;
/// training kept learning them, because the export joined a label to the text the draw stored
/// rather than to the text the splitter produces.
///
/// A game with no capture on this machine holds nothing back, exactly as in the adjudication
/// draw: the alternative is an export that silently shrinks depending on what was crawled.
///
/// # Errors
///
/// Fails if a reference set cannot be read or the destination cannot be written.
pub fn export_training(reference_root: &Path, captures: &Path, to: &Path) -> Result<ExportReport> {
    use std::io::Write as _;

    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::io::BufWriter::new(std::fs::File::create(to)?);
    let mut report = ExportReport::default();

    let found = read_once(labelled_claims(reference_root)?, &mut report);
    let mut wanted: std::collections::BTreeMap<u32, std::collections::HashSet<String>> =
        std::collections::BTreeMap::new();
    for claim in &found {
        wanted
            .entry(claim.label.app_id)
            .or_default()
            .insert(claim.label.review_id.clone());
    }
    let cuts: std::collections::HashMap<u32, CutSpans> = wanted
        .into_iter()
        .filter_map(|(app_id, ids)| Some((app_id, spans_cut_now(captures, app_id, &ids).ok()?)))
        .collect();

    for claim in found {
        // Most of the set was cut before the rules that recognise these, so it still holds
        // thousands of them. A row whose text carries no proposition has a label that could
        // not have been right, and training on it teaches the string rather than the task.
        if crate::claims::is_not_a_claim(&claim.text) {
            report.no_claim += 1;
            continue;
        }
        let label = &claim.label;
        // A teaching set is drawn for being hard or for holding a word, so a row of one on a
        // validation or frozen game is measured on claims chosen for the model rather than
        // drawn at random, and the coverage figure stops meaning what it says. The draw
        // commands refuse such a game, but the sets drawn before that guard existed are still
        // on disk, and the split reads whatever the export hands it.
        if TEACHING_SETS.contains(&label.subset.as_str())
            && crate::measure::role(label.app_id, crate::measure::SPLIT_SEED)
                != crate::measure::Role::Train
        {
            report.measured_on += 1;
            report.measured_on_games.insert(label.app_id);
            continue;
        }
        if cuts
            .get(&label.app_id)
            .is_some_and(|cut| cut_at(cut, label).is_none())
        {
            report.recut += 1;
            continue;
        }
        let row = serde_json::json!({
            "text": claim.text,
            "review": claim.review,
            "review_offset": claim.review_offset,
            "subject": label.subject,
            "polarity": label.polarity,
            "confidence": label.confidence,
            "ambiguous": label.ambiguous,
            "ironic": label.ironic,
            "split_wrong": label.split_wrong,
            "produced_by": label.produced_by,
            "language": label.language,
            "app_id": label.app_id,
            "review_id": label.review_id,
            "claim_index": label.index,
            "subset": label.subset,
            "second_subject": claim.again.as_ref().map(|again| &again.subject),
            "second_polarity": claim.again.as_ref().map(|again| &again.polarity),
            "second_confidence": claim.again.as_ref().map(|again| &again.confidence),
            "second_by": claim.again.as_ref().map(|again| &again.produced_by),
        });
        writeln!(out, "{row}")?;
        report.written += 1;
    }
    out.flush()?;
    Ok(report)
}

/// What a pool draw wrote for one game.
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolReport {
    pub reviews: usize,
    pub claims: usize,
    /// Reviews the draw reached that a reference set already holds.
    pub labelled: usize,
    /// Rows held back for carrying no claim.
    pub refused: usize,
}

/// Every review id a game's reference sets hold, whichever draw put it there.
///
/// Walks every `sample.json` under the game's directory rather than the named sets, so a
/// second reading and a revisit count too: the pool is for reviews nobody has read.
fn reviews_in_sets(game: &Path) -> Result<std::collections::HashSet<String>> {
    let mut ids = std::collections::HashSet::new();
    let mut dirs = vec![game.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.file_name().is_some_and(|name| name == "sample.json") {
                let drawn: Vec<DrawnReview> = serde_json::from_slice(&std::fs::read(&path)?)?;
                ids.extend(drawn.into_iter().map(|review| review.id));
            }
        }
    }
    Ok(ids)
}

/// Draws reviews no reference set holds and appends every claim of each, with its text, as
/// JSONL for a teacher model to read.
///
/// A labelled claim is already in the training set with its label; what a pool adds is claims
/// nobody has read, in the numbers a labeller never could, for a student to learn a teacher's
/// answers on. The rows are shaped like the training export without the label fields, and
/// hold review text, so the file never leaves the machine either.
///
/// The caller keeps the draw to games the model trains on. A frozen or validation game's
/// reviews, even unlabelled, carry the teacher's knowledge of that game into the student, and
/// the figure measured on it would stop being about a game nobody trained on.
///
/// # Errors
///
/// Fails if there is no capture for the app, a reference set cannot be read, or the
/// destination cannot be written.
pub fn export_pool(
    out_dir: &Path,
    app_id: u32,
    game_sets: &Path,
    wanted: usize,
    english_share: f64,
    seed: u64,
    out: &mut impl std::io::Write,
) -> Result<PoolReport> {
    let labelled = reviews_in_sets(game_sets)?;
    let drawn = draw(out_dir, app_id, wanted, english_share, seed, "pool")?;
    let mut report = PoolReport::default();
    for review in drawn {
        if labelled.contains(&review.id) {
            report.labelled += 1;
            continue;
        }
        let around = rejoined(&review);
        let mut at = 0;
        for claim in &review.claims {
            let offset = at;
            at += claim.text.len() + 1;
            if crate::claims::is_not_a_claim(&claim.text) {
                report.refused += 1;
                continue;
            }
            let row = serde_json::json!({
                "text": claim.text,
                "review": around,
                "review_offset": offset,
                "language": review.language,
                "app_id": review.app_id,
                "review_id": review.id,
                "claim_index": claim.index,
                "subset": review.subset,
            });
            writeln!(out, "{row}")?;
            report.claims += 1;
        }
        report.reviews += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_label(
        app_id: u32,
        review_id: &str,
        index: u16,
        subset: &str,
        subject: &str,
    ) -> ClaimLabel {
        ClaimLabel {
            review_id: review_id.to_owned(),
            index,
            app_id,
            language: "english".to_owned(),
            subset: subset.to_owned(),
            start: 0,
            end: 0,
            taxonomy: crate::taxonomy::sheet(),
            produced_by: "a-labeller".to_owned(),
            subject: subject.to_owned(),
            polarity: "praise".to_owned(),
            ironic: false,
            confidence: "high".to_owned(),
            ambiguous: false,
            split_wrong: false,
        }
    }

    fn review(id: &str, claims: &[&str]) -> DrawnReview {
        DrawnReview {
            id: id.to_owned(),
            app_id: 1,
            language: "english".to_owned(),
            subset: "random".to_owned(),
            claims: claims
                .iter()
                .enumerate()
                .map(|(index, text)| DrawnClaim {
                    index: u16::try_from(index).unwrap(),
                    start: 0,
                    end: 0,
                    text: (*text).to_owned(),
                })
                .collect(),
            asked: None,
        }
    }

    #[test]
    fn a_claim_only_the_older_reading_answered_still_has_a_second_answer() {
        let dir = std::env::temp_dir().join(format!("steamgauge-again-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let label = |id: &str, subject: &str| ClaimLabel {
            review_id: id.to_owned(),
            index: 0,
            app_id: 1,
            language: "english".to_owned(),
            subset: "random".to_owned(),
            start: 0,
            end: 0,
            taxonomy: crate::taxonomy::sheet(),
            produced_by: "a-labeller".to_owned(),
            subject: subject.to_owned(),
            polarity: "praise".to_owned(),
            ironic: false,
            confidence: "high".to_owned(),
            ambiguous: false,
            split_wrong: false,
        };
        for (reading, labels) in [
            ("second", vec![label("a", "gameplay")]),
            ("opus", vec![label("a", "story"), label("b", "performance")]),
        ] {
            std::fs::create_dir_all(dir.join(reading)).unwrap();
            std::fs::write(
                dir.join(reading).join("labels.json"),
                serde_json::to_vec(&labels).unwrap(),
            )
            .unwrap();
        }

        let again = read_again(&dir).unwrap();
        // The newer reading answers the claim both hold, and the older one is not thrown away
        // for the claim it alone read.
        assert_eq!(again[&("a".to_owned(), 0)].subject, "gameplay");
        assert_eq!(again[&("b".to_owned(), 0)].subject, "performance");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_teaching_set_on_a_game_the_model_is_measured_on_never_reaches_the_export() {
        let role_of = |app_id| crate::measure::role(app_id, crate::measure::SPLIT_SEED);
        let first = |wanted| {
            (1..u32::MAX)
                .find(|&app_id| role_of(app_id) == wanted)
                .unwrap()
        };
        let (trains_on, held_back) = (
            first(crate::measure::Role::Train),
            first(crate::measure::Role::Validation),
        );

        let root = std::env::temp_dir().join(format!("steamgauge-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (app_id, set, subset) in [
            (trains_on, "", "random"),
            (trains_on, "mined", "mined"),
            (held_back, "", "random"),
            (held_back, "mined", "mined"),
        ] {
            let dir = root.join(app_id.to_string()).join(set);
            std::fs::create_dir_all(&dir).unwrap();
            let id = format!("{app_id}-{subset}");
            let drawn = DrawnReview {
                id: id.clone(),
                app_id,
                subset: subset.to_owned(),
                ..review(&id, &["The combat is superb."])
            };
            std::fs::write(
                dir.join("sample.json"),
                serde_json::to_vec(&[drawn]).unwrap(),
            )
            .unwrap();
            std::fs::write(
                dir.join("labels.json"),
                serde_json::json!([{
                    "review_id": id, "index": 0, "app_id": app_id, "language": "english",
                    "subset": subset, "start": 0, "end": 0, "subject": "gameplay",
                    "polarity": "praise", "ironic": false, "confidence": "high",
                    "ambiguous": false, "split_wrong": false
                }])
                .to_string(),
            )
            .unwrap();
        }

        let to = root.join("claims.jsonl");
        // No captures, so nothing is held back for being cut differently: this test is about
        // which game a teaching row sits on.
        let report = export_training(&root, &root.join("no-captures"), &to).unwrap();
        let written = std::fs::read_to_string(&to).unwrap();

        assert_eq!(report.measured_on, 1, "the held-back game's mined row");
        assert_eq!(report.measured_on_games, [held_back].into_iter().collect());
        assert_eq!(report.written, 3);
        assert!(
            written.contains(&format!("{trains_on}-mined")),
            "a teaching row on a game the model trains on is what teaching sets are for"
        );
        // The random draw is the measurement, so it stays whatever else the game carries.
        assert!(written.contains(&format!("{held_back}-random")));
        assert!(!written.contains(&format!("{held_back}-mined")));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_claim_two_draws_both_handed_out_is_one_row_holding_both_answers() {
        let claim = |subset: &str, subject: &str, again: Option<&str>| LabelledClaim {
            text: "The combat is superb.".to_owned(),
            review: "The combat is superb.".to_owned(),
            review_offset: 0,
            again: again.map(|subject| ClaimLabel {
                subject: subject.to_owned(),
                ..a_label(1, "r1", 0, "second", subject)
            }),
            label: a_label(1, "r1", 0, subset, subject),
        };

        let mut report = ExportReport::default();
        let kept = read_once(
            vec![
                claim("multilingual", "audio", None),
                claim("random", "gameplay", None),
            ],
            &mut report,
        );
        assert_eq!(report.twice, 1);
        assert_eq!(kept.len(), 1, "one claim, one row");
        // The random draw is what every figure is measured on, whichever order the sets were
        // read in, and the teaching copy is a second reading of the same claim.
        assert_eq!(kept[0].label.subject, "gameplay");
        assert_eq!(kept[0].label.subset, "random");
        assert_eq!(
            kept[0].again.as_ref().map(|a| a.subject.as_str()),
            Some("audio")
        );

        let mut report = ExportReport::default();
        let kept = read_once(
            vec![
                claim("random", "gameplay", Some("story")),
                claim("multilingual", "audio", None),
            ],
            &mut report,
        );
        assert_eq!(
            kept[0].again.as_ref().map(|a| a.subject.as_str()),
            Some("story"),
            "a blind reading of the whole set outranks a teaching draw's copy"
        );
    }

    #[test]
    fn a_revisit_with_no_words_asks_about_every_claim_under_the_subjects() {
        let dir = std::env::temp_dir().join(format!("steamgauge-revisit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let drawn = DrawnReview {
            app_id: 1,
            ..review(
                "r1",
                &[
                    "No subtitle size setting.",
                    "The combat is superb.",
                    "It runs at nine frames.",
                ],
            )
        };
        std::fs::write(
            dir.join("sample.json"),
            serde_json::to_vec(&[drawn]).unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("labels.json"),
            serde_json::to_vec(&[
                a_label(1, "r1", 0, "random", "accessibility"),
                a_label(1, "r1", 1, "random", "accessibility"),
                a_label(1, "r1", 2, "random", "performance"),
            ])
            .unwrap(),
        )
        .unwrap();

        let by_word = draw_revisit(&dir, &["subtitle".to_owned()], &[]).unwrap();
        assert_eq!(by_word[0].asked, Some(vec![0]));

        // A row redefined rather than renamed puts every claim under it back in question, and
        // the claim that says nothing about the new wording is exactly the one to re-ask.
        let whole_row = draw_revisit(&dir, &[], &["accessibility".to_owned()]).unwrap();
        assert_eq!(whole_row[0].asked, Some(vec![0, 1]));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_merge_that_places_nothing_writes_nothing() {
        // The default target is the set itself, so pointing a returned batch at the wrong one
        // is a single missing flag away, and the labels that do not fit are exactly the labels
        // of another draw. Writing the empty intersection then replaces a labelling that is in
        // no version control with nothing at all. This is not hypothetical: it destroyed 2,068
        // labels across four games before the check existed.
        let dir = std::env::temp_dir().join(format!("steamgauge-refuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_set(&dir, &[review("a", &["The combat is superb."])], 1).unwrap();

        let sheet = Sheet {
            taxonomy: crate::taxonomy::sheet(),
            produced_by: "a-labeller".to_owned(),
        };
        let mine = dir.join("returned");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(
            mine.join("batch-000.json"),
            serde_json::json!([{
                "review_id": "somebody-elses-review", "index": 0, "subject": "gameplay",
                "polarity": "praise", "ironic": false, "confidence": "high",
                "ambiguous": false, "split_wrong": false
            }])
            .to_string(),
        )
        .unwrap();

        let refused = ingest(&dir, &mine, &sheet);
        assert!(
            refused.is_err(),
            "a merge naming only another set's claims was accepted"
        );
        let held: Vec<ClaimLabel> =
            serde_json::from_slice(&std::fs::read(dir.join("labels.json")).unwrap_or_default())
                .unwrap_or_default();
        assert!(
            held.is_empty(),
            "the set had no labels to lose here, but the file was written anyway"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_batch_holds_whole_reviews_so_a_claim_is_never_shown_alone() {
        let dir = std::env::temp_dir().join(format!("steamgauge-claimset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let drawn = vec![
            review("a", &["The combat is superb.", "It runs badly."]),
            review("b", &["gg"]),
        ];
        let report = write_set(&dir, &drawn, 1).unwrap();

        assert_eq!(report.reviews, 2);
        assert_eq!(report.claims, 3);
        assert_eq!(report.batches, 2);

        let first = std::fs::read_to_string(dir.join("batches").join("batch-000.json")).unwrap();
        assert!(first.contains("The combat is superb."));
        assert!(
            first.contains("It runs badly."),
            "a review's other claims are what make the first one readable"
        );
        assert!(
            !first.contains("app_id"),
            "a labeller told which game it is can infer what the model cannot"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reading_that_grows_hands_out_only_what_nobody_has_read() {
        let dir = std::env::temp_dir().join(format!("steamgauge-grows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let drawn = vec![
            review("a", &["The combat is superb.", "It runs badly."]),
            review("b", &["gg"]),
            review("c", &["The story drags."]),
        ];
        // "a" is answered in full, "b" had its one claim skipped: it goes out again whole.
        let label = |id: &str, index: u16| ClaimLabel {
            review_id: id.to_owned(),
            index,
            app_id: 1,
            language: "english".to_owned(),
            subset: "random".to_owned(),
            start: 0,
            end: 0,
            taxonomy: crate::taxonomy::sheet(),
            produced_by: "a-labeller".to_owned(),
            subject: "gameplay".to_owned(),
            polarity: "praise".to_owned(),
            ironic: false,
            confidence: "high".to_owned(),
            ambiguous: false,
            split_wrong: false,
        };
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("labels.json"),
            serde_json::to_vec(&[label("a", 0), label("a", 1)]).unwrap(),
        )
        .unwrap();

        let answered = answered_reviews(&dir, &drawn).unwrap();
        assert_eq!(answered.len(), 1);
        assert!(answered.contains("a"));

        let report = write_set_handing_out(&dir, &drawn, 1, &answered).unwrap();
        assert_eq!(report.reviews, 2);
        assert_eq!(report.claims, 2);
        assert_eq!(report.batches, 2);
        let handed: String = ["batch-000.json", "batch-001.json"]
            .iter()
            .map(|name| std::fs::read_to_string(dir.join("batches").join(name)).unwrap())
            .collect();
        assert!(!handed.contains("The combat is superb."));
        assert!(handed.contains("gg"));
        assert!(handed.contains("The story drags."));
        assert!(
            !dir.join("batches").join("batch-002.json").exists(),
            "the answered review still took a batch"
        );
        let sample: Vec<DrawnReview> =
            serde_json::from_slice(&std::fs::read(dir.join("sample.json")).unwrap()).unwrap();
        assert_eq!(
            sample.len(),
            3,
            "the sample must keep the answered review, or the ingest drops its labels"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_draw_that_asks_about_one_claim_still_hands_over_the_whole_review() {
        let dir = std::env::temp_dir().join(format!("steamgauge-asked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut drawn = review("a", &["Bought it for the mod support.", "It doesn't."]);
        drawn.asked = Some(vec![1]);
        let report = write_set(&dir, &[drawn], 8).unwrap();

        assert_eq!(report.claims, 1, "one claim was asked about, not two");
        let batch = std::fs::read_to_string(dir.join("batches").join("batch-000.json")).unwrap();
        assert!(
            batch.contains("Bought it for the mod support. It doesn't."),
            "the claim nobody is asked about is the only thing that makes the other readable"
        );
        let handed: serde_json::Value = serde_json::from_str(&batch).unwrap();
        let claims = handed[0]["claims"].as_array().unwrap();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0]["index"], 1, "the index is the one in the review");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_label_on_a_claim_nobody_asked_about_does_not_join_the_set() {
        let dir = std::env::temp_dir().join(format!("steamgauge-unasked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut drawn = review("a", &["Bought it for the mod support.", "It doesn't."]);
        drawn.asked = Some(vec![1]);
        write_set(&dir, &[drawn], 8).unwrap();

        let returned = dir.join("returned");
        std::fs::create_dir_all(&returned).unwrap();
        std::fs::write(
            returned.join("batch-000.json"),
            serde_json::to_vec(&serde_json::json!([
                {"review_id": "a", "index": 1, "subject": "mods", "polarity": "complaint",
                 "ironic": false, "confidence": "high", "ambiguous": false, "split_wrong": false},
                {"review_id": "a", "index": 0, "subject": "mods", "polarity": "praise",
                 "ironic": false, "confidence": "high", "ambiguous": false, "split_wrong": false},
            ]))
            .unwrap(),
        )
        .unwrap();

        let sheet = Sheet {
            produced_by: "a test".to_owned(),
            ..Default::default()
        };
        let (labels, report) = ingest(&dir, &returned, &sheet).unwrap();

        assert_eq!(labels.len(), 1, "only the claim that was asked about");
        assert_eq!(labels[0].index, 1);
        assert_eq!(report.unknown, vec!["a#0".to_owned()]);
        assert!(
            report.missing.is_empty(),
            "a claim shown for context is not a claim that came back unlabelled"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_later_sitting_files_beside_the_earlier_one_rather_than_over_it() {
        // A set is labelled five batches at a time, and the second sitting's files do not
        // hold the first's answers. Written from the returned files alone, the second ingest
        // replaced 675 labels with the 648 that came back, and stamped whatever survived with
        // the second sitting's sheet and labeller.
        let dir = std::env::temp_dir().join(format!("steamgauge-sittings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_set(
            &dir,
            &[
                review("a", &["The combat is superb."]),
                review("b", &["It runs badly."]),
            ],
            1,
        )
        .unwrap();
        let answer = |id: &str, subject: &str| {
            serde_json::json!([{
                "review_id": id, "index": 0, "subject": subject, "polarity": "praise",
                "ironic": false, "confidence": "high", "ambiguous": false, "split_wrong": false
            }])
            .to_string()
        };

        let first = dir.join("first");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::write(first.join("batch-000.json"), answer("a", "gameplay")).unwrap();
        let earlier = Sheet {
            taxonomy: "an-earlier-wording".to_owned(),
            produced_by: "the-first-labeller".to_owned(),
        };
        let (_, report) = ingest(&dir, &first, &earlier).unwrap();
        assert_eq!(report.missing, vec!["b#0".to_owned()]);

        let second = dir.join("second");
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(second.join("batch-001.json"), answer("b", "performance")).unwrap();
        let later = Sheet {
            taxonomy: crate::taxonomy::sheet(),
            produced_by: "the-second-labeller".to_owned(),
        };
        let (labels, report) = ingest(&dir, &second, &later).unwrap();

        assert_eq!(report.accepted, 1);
        assert_eq!(report.kept, 1);
        assert!(
            report.missing.is_empty(),
            "both sittings together answer the whole draw"
        );
        assert_eq!(labels.len(), 2);
        let first_again = labels.iter().find(|label| label.review_id == "a").unwrap();
        assert_eq!(first_again.subject, "gameplay");
        assert_eq!(
            (
                first_again.taxonomy.as_str(),
                first_again.produced_by.as_str()
            ),
            ("an-earlier-wording", "the-first-labeller"),
            "a label kept from an earlier sitting keeps the sheet and labeller it answered"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_drawn_sample_records_the_offsets_rather_than_only_the_text() {
        let dir = std::env::temp_dir().join(format!("steamgauge-offsets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut drawn = review("a", &["Great port.", "Runs at 4k60."]);
        drawn.claims[1].start = 12;
        drawn.claims[1].end = 25;
        write_set(&dir, &[drawn], 8).unwrap();

        let sample = std::fs::read_to_string(dir.join("sample.json")).unwrap();
        assert!(sample.contains("\"start\": 12") && sample.contains("\"end\": 25"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
