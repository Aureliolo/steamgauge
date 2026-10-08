//! Who wrote a game's reviews, and where what they say differs with who wrote them.
//!
//! Steam records beside every review how long its writer had played when they wrote it, whether
//! they played mostly on a Steam Deck, whether the game was still in early access, and whether
//! they got it free. A reading counts every subject again for each kind of reviewer those facts
//! make, so a page can show the figures of one kind, or of two side by side, and say where a
//! difference between them is wider than the reviews behind it can explain.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    html::percent,
    moves::{WORTH_SAYING, standard_errors_apart},
    read::ReadReport,
    taxonomy::{Category, SHEET},
};

/// What Steam records about the writer of a review, as far as a reading splits by it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reviewer {
    /// Minutes played when the review was written, where Steam said.
    pub played_minutes: Option<u32>,
    /// Played mostly on a Steam Deck.
    pub deck: bool,
    /// Written while the game was in early access.
    pub early_access: bool,
    /// Ticked by its writer as received for free.
    pub free: bool,
}

/// One fact reviewers are told apart by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Split {
    pub id: &'static str,
    pub label: &'static str,
}

pub const SPLITS: [Split; 4] = [
    Split {
        id: "played",
        label: "Time played when they wrote it",
    },
    Split {
        id: "deck",
        label: "Where they played",
    },
    Split {
        id: "early-access",
        label: "When they wrote it",
    },
    Split {
        id: "copy",
        label: "How they got the game",
    },
];

/// Which reviews a kind of reviewer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Holds {
    /// Played at least `from` minutes and fewer than `to`, where there is a `to`.
    Played {
        from: u32,
        to: Option<u32>,
    },
    Deck(bool),
    EarlyAccess(bool),
    Free(bool),
}

impl Holds {
    fn of(self, reviewer: &Reviewer) -> bool {
        match self {
            Self::Played { from, to } => reviewer
                .played_minutes
                .is_some_and(|played| played >= from && to.is_none_or(|to| played < to)),
            Self::Deck(wanted) => reviewer.deck == wanted,
            Self::EarlyAccess(wanted) => reviewer.early_access == wanted,
            Self::Free(wanted) => reviewer.free == wanted,
        }
    }
}

/// One kind of reviewer: the reviews whose writers share one answer to one [`Split`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub id: &'static str,
    /// The [`Split`] this is one answer to.
    pub split: &'static str,
    /// What a list of them calls this one.
    pub label: &'static str,
    /// What a sentence about them starts with.
    pub who: &'static str,
    #[serde(skip)]
    holds: Holds,
}

impl Segment {
    /// Whether a review's writer is this kind of reviewer.
    #[must_use]
    pub fn holds(&self, reviewer: &Reviewer) -> bool {
        self.holds.of(reviewer)
    }
}

const HOUR: u32 = 60;

/// Steam refunds a game played for under two hours, so a review written inside them is one its
/// writer could still take back with the game.
pub const REFUND_WINDOW: u32 = 2 * HOUR;

/// Every kind of reviewer a reading counts, a split's answers together and in order.
///
/// The bands of time played are the refund window and then four of roughly a fifth of reviewers
/// each, at round numbers: DECISIONS.md has the spread they were read from.
pub const SEGMENTS: [Segment; 11] = [
    Segment {
        id: "under-2-hours",
        split: "played",
        label: "Under 2 hours",
        who: "Reviewers with under 2 hours played",
        holds: Holds::Played {
            from: 0,
            to: Some(REFUND_WINDOW),
        },
    },
    Segment {
        id: "2-to-10-hours",
        split: "played",
        label: "2 to 10 hours",
        who: "Reviewers with 2 to 10 hours played",
        holds: Holds::Played {
            from: REFUND_WINDOW,
            to: Some(10 * HOUR),
        },
    },
    Segment {
        id: "10-to-30-hours",
        split: "played",
        label: "10 to 30 hours",
        who: "Reviewers with 10 to 30 hours played",
        holds: Holds::Played {
            from: 10 * HOUR,
            to: Some(30 * HOUR),
        },
    },
    Segment {
        id: "30-to-100-hours",
        split: "played",
        label: "30 to 100 hours",
        who: "Reviewers with 30 to 100 hours played",
        holds: Holds::Played {
            from: 30 * HOUR,
            to: Some(100 * HOUR),
        },
    },
    Segment {
        id: "100-hours-or-more",
        split: "played",
        label: "100 hours or more",
        who: "Reviewers with 100 hours or more played",
        holds: Holds::Played {
            from: 100 * HOUR,
            to: None,
        },
    },
    Segment {
        id: "steam-deck",
        split: "deck",
        label: "Mostly on a Steam Deck",
        who: "Reviewers playing mostly on a Steam Deck",
        holds: Holds::Deck(true),
    },
    Segment {
        id: "elsewhere",
        split: "deck",
        label: "Mostly elsewhere",
        who: "Reviewers playing mostly elsewhere",
        holds: Holds::Deck(false),
    },
    Segment {
        id: "early-access",
        split: "early-access",
        label: "During early access",
        who: "Reviewers writing during early access",
        holds: Holds::EarlyAccess(true),
    },
    Segment {
        id: "after-release",
        split: "early-access",
        label: "After release",
        who: "Reviewers writing after release",
        holds: Holds::EarlyAccess(false),
    },
    Segment {
        id: "got-it-free",
        split: "copy",
        label: "Got it free",
        who: "Reviewers who got it free",
        holds: Holds::Free(true),
    },
    Segment {
        id: "paid-for-it",
        split: "copy",
        label: "Paid for it",
        who: "Reviewers who paid for it",
        holds: Holds::Free(false),
    },
];

/// The kinds of reviewer one review's writer is, as a set of positions in [`SEGMENTS`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Membership(u16);

impl Membership {
    #[must_use]
    pub fn of(reviewer: &Reviewer) -> Self {
        Self(
            SEGMENTS
                .iter()
                .enumerate()
                .filter(|(_, segment)| segment.holds.of(reviewer))
                .map(|(at, _)| 1_u16 << at)
                .sum(),
        )
    }

    pub fn positions(self) -> impl Iterator<Item = usize> {
        (0..SEGMENTS.len()).filter(move |at| self.0 & (1 << at) != 0)
    }
}

/// One month of one kind of reviewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentMonth {
    /// `2024-02`, which sorts as it reads.
    pub label: String,
    pub reviews: u64,
    pub positive: u64,
}

/// Every subject counted over one kind of reviewer. The vectors are in taxonomy order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentCount {
    pub id: String,
    pub reviews: u64,
    /// Of them, recommending the game.
    pub positive: u64,
    /// Claims in these reviews, answered or declined, which is what a share of claims is over.
    pub claims: u64,
    /// Reviews raising each subject.
    pub raised: Vec<u64>,
    /// Reviews praising it and not complaining about it, the reverse, and both.
    pub praised: Vec<u64>,
    pub criticised: Vec<u64>,
    pub mixed: Vec<u64>,
    /// Of the reviews raising each subject, how many recommended the game.
    pub recommending: Vec<u64>,
    /// Claims about each subject.
    pub claims_about: Vec<u64>,
    /// Oldest first.
    pub months: Vec<SegmentMonth>,
}

impl SegmentCount {
    fn empty(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            reviews: 0,
            positive: 0,
            claims: 0,
            raised: vec![0; SHEET.len()],
            praised: vec![0; SHEET.len()],
            criticised: vec![0; SHEET.len()],
            mixed: vec![0; SHEET.len()],
            recommending: vec![0; SHEET.len()],
            claims_about: vec![0; SHEET.len()],
            months: Vec::new(),
        }
    }

    /// Another kind's reviews added to these, which is how "everyone else" is made. The kinds
    /// of one split never share a review, so nothing is counted twice.
    fn add(&mut self, other: &Self) {
        self.reviews += other.reviews;
        self.positive += other.positive;
        self.claims += other.claims;
        for (mine, theirs) in [
            (&mut self.raised, &other.raised),
            (&mut self.praised, &other.praised),
            (&mut self.criticised, &other.criticised),
            (&mut self.mixed, &other.mixed),
            (&mut self.recommending, &other.recommending),
            (&mut self.claims_about, &other.claims_about),
        ] {
            for (slot, count) in mine.iter_mut().enumerate() {
                *count += theirs.get(slot).copied().unwrap_or(0);
            }
        }
    }

    fn at(counts: &[u64], slot: usize) -> u64 {
        counts.get(slot).copied().unwrap_or(0)
    }

    /// Reviews praising a subject, whether or not they also complain about it.
    fn praising(&self, slot: usize) -> u64 {
        Self::at(&self.praised, slot) + Self::at(&self.mixed, slot)
    }

    /// Reviews complaining about a subject, whether or not they also praise it.
    fn complaining(&self, slot: usize) -> u64 {
        Self::at(&self.criticised, slot) + Self::at(&self.mixed, slot)
    }
}

/// One review, as the counting of who wrote it needs it.
pub(crate) struct Reviewed<'a> {
    pub recommended: bool,
    pub month: &'a str,
    pub claims: u64,
    pub subjects: &'a [usize],
    pub praise: &'a [bool],
    pub complaint: &'a [bool],
}

/// What a reading adds up for every kind of reviewer as it walks a corpus.
#[derive(Debug, Clone)]
pub(crate) struct Tally {
    counts: Vec<SegmentCount>,
    months: Vec<BTreeMap<String, (u64, u64)>>,
}

impl Tally {
    pub(crate) fn new() -> Self {
        Self {
            counts: SEGMENTS
                .iter()
                .map(|segment| SegmentCount::empty(segment.id))
                .collect(),
            months: vec![BTreeMap::new(); SEGMENTS.len()],
        }
    }

    pub(crate) fn review(&mut self, member: Membership, review: &Reviewed<'_>) {
        let recommended = u64::from(review.recommended);
        for at in member.positions() {
            let count = &mut self.counts[at];
            count.reviews += 1;
            count.positive += recommended;
            count.claims += review.claims;
            for &subject in review.subjects {
                count.raised[subject] += 1;
                count.recommending[subject] += recommended;
                match (review.praise[subject], review.complaint[subject]) {
                    (true, true) => count.mixed[subject] += 1,
                    (true, false) => count.praised[subject] += 1,
                    (false, true) => count.criticised[subject] += 1,
                    (false, false) => {}
                }
            }
            let month = self.months[at].entry(review.month.to_owned()).or_default();
            month.0 += 1;
            month.1 += recommended;
        }
    }

    /// One claim about a subject, in a review whose writer is `member`.
    pub(crate) fn claim(&mut self, member: Membership, subject: usize) {
        for at in member.positions() {
            self.counts[at].claims_about[subject] += 1;
        }
    }

    pub(crate) fn finish(self) -> Vec<SegmentCount> {
        self.counts
            .into_iter()
            .zip(self.months)
            .map(|(mut count, months)| {
                count.months = months
                    .into_iter()
                    .map(|(label, (reviews, positive))| SegmentMonth {
                        label,
                        reviews,
                        positive,
                    })
                    .collect();
                count
            })
            .collect()
    }
}

/// Reviews a kind of reviewer needs before any share of it is shown or compared: the floor the
/// recent window of what moved is held to, below which a handful of reviews moves a share by
/// whole points.
pub const ENOUGH: u64 = 100;

/// How many standard errors a difference has to span. What moved tests some fifty shares a game
/// at three; a game's kinds of reviewer test some four hundred, and at three one of them would
/// clear by chance on every game. At four, fewer than one game in thirty shows one that chance
/// made.
pub const CLEAR: f64 = 4.0;

#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn ratio((part, whole): (u64, u64)) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

/// A share among some reviewers against the same share among others.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Gap {
    pub share: f64,
    pub against: f64,
    /// Standard errors of the pooled share apart, positive where `share` is the higher.
    pub z: f64,
    /// Wider than chance and wide enough to act on.
    pub clear: bool,
}

impl Gap {
    fn between(these: (u64, u64), others: (u64, u64)) -> Self {
        let (share, against) = (ratio(these), ratio(others));
        let z = standard_errors_apart(others, these);
        Self {
            share,
            against,
            z,
            clear: clear(share, against, z),
        }
    }
}

fn clear(share: f64, against: f64, z: f64) -> bool {
    z.abs() >= CLEAR && (share - against).abs() >= WORTH_SAYING
}

/// The counts of one kind of reviewer, where the reading has them.
fn stored<'a>(report: &'a ReadReport, id: &str) -> Option<&'a SegmentCount> {
    report.who.iter().find(|count| count.id == id)
}

/// The kind of reviewer an id names.
#[must_use]
pub fn segment(id: &str) -> Option<&'static Segment> {
    SEGMENTS.iter().find(|segment| segment.id == id)
}

fn kinds_of(split: &str) -> impl Iterator<Item = &'static Segment> {
    SEGMENTS
        .iter()
        .filter(move |segment| segment.split == split)
}

/// Every other kind of the same split, added together.
fn everyone_else(report: &ReadReport, these: &Segment) -> SegmentCount {
    let mut others = SegmentCount::empty(EVERYONE_ELSE);
    for other in kinds_of(these.split).filter(|other| other.id != these.id) {
        if let Some(count) = stored(report, other.id) {
            others.add(count);
        }
    }
    others
}

/// What a sentence calls the reviewers a kind is held against: the other answer where the fact
/// has two, and everybody else where it has more.
fn others_phrase(these: &Segment) -> String {
    let mut others = kinds_of(these.split).filter(|other| other.id != these.id);
    match (others.next(), others.next()) {
        (Some(only), None) => lowered_first(only.who),
        _ => "everyone else".to_owned(),
    }
}

fn lowered_first(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}

/// A subject's name in the middle of a sentence.
fn subject_phrase(category: &Category) -> String {
    if category.id == "verdict" {
        return "the game as a whole".to_owned();
    }
    mid_sentence(category.label)
}

/// A label in lower case, except a word that is an acronym.
fn mid_sentence(label: &str) -> String {
    label
        .split(' ')
        .map(|word| {
            if word.len() > 1 && word.chars().all(|c| c.is_ascii_uppercase()) {
                word.to_owned()
            } else {
                word.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The id the reviewers outside a kind go by, where they are what it is held against.
pub const EVERYONE_ELSE: &str = "everyone-else";

/// One side of a comparison, as a whole.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Head {
    pub id: String,
    pub label: String,
    /// What a sentence about them starts with.
    pub who: String,
    pub reviews: u64,
    pub claims: u64,
    /// The share recommending the game, and where it could lie given how many reviews it is of.
    pub recommending: f64,
    pub low: f64,
    pub high: f64,
}

impl Head {
    fn of(count: &SegmentCount, (label, who): (&str, &str)) -> Self {
        let (low, high) = crate::measure::wilson(count.positive, count.reviews).unwrap_or_default();
        Self {
            id: count.id.clone(),
            label: label.to_owned(),
            who: who.to_owned(),
            reviews: count.reviews,
            claims: count.claims,
            recommending: ratio((count.positive, count.reviews)),
            low,
            high,
        }
    }
}

/// One subject among one side's reviews.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Figures {
    /// Reviews raising it, and their share of the side's reviews.
    pub raised: u64,
    pub rate: f64,
    /// Where the rate could lie, given how many reviews it is over.
    pub low: f64,
    pub high: f64,
    pub praised: u64,
    pub criticised: u64,
    pub mixed: u64,
    /// The shares of all the side's reviews praising it and complaining about it, whether or
    /// not the same review does both.
    pub praising: f64,
    pub complaining: f64,
    /// Claims about it, which over [`Head::claims`] is the share a measured error corrects.
    pub claims: u64,
}

impl Figures {
    fn of(count: &SegmentCount, slot: usize) -> Self {
        let raised = SegmentCount::at(&count.raised, slot);
        let (low, high) = crate::measure::wilson(raised, count.reviews).unwrap_or_default();
        Self {
            raised,
            rate: ratio((raised, count.reviews)),
            low,
            high,
            praised: SegmentCount::at(&count.praised, slot),
            criticised: SegmentCount::at(&count.criticised, slot),
            mixed: SegmentCount::at(&count.mixed, slot),
            praising: ratio((count.praising(slot), count.reviews)),
            complaining: ratio((count.complaining(slot), count.reviews)),
            claims: SegmentCount::at(&count.claims_about, slot),
        }
    }
}

/// One subject, both sides of a comparison, and how far apart they are.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Beside {
    pub id: &'static str,
    pub label: &'static str,
    pub these: Figures,
    pub others: Figures,
    pub raised: Gap,
    pub praise: Gap,
    pub complaint: Gap,
}

/// One kind of reviewer beside another of the same split, or beside everyone else.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub split: &'static str,
    pub these: Head,
    pub others: Head,
    pub recommended: Gap,
    /// Every subject, in taxonomy order.
    pub subjects: Vec<Beside>,
    /// The months of the first side, oldest first.
    pub months: Vec<SegmentMonth>,
}

/// One kind of reviewer beside `others`, or beside everyone else of its split where none is
/// named.
///
/// # Errors
///
/// Refused where a kind is unknown, where the reading was counted before reviewers were told
/// apart, where the two are answers to different facts and so share reviews, and where either
/// side holds fewer than [`ENOUGH`] reviews.
pub fn compare(
    report: &ReadReport,
    these: &str,
    others: Option<&str>,
) -> crate::Result<Comparison> {
    let refused = |why: String| crate::Error::Refused(why);
    let unknown = |id: &str| refused(format!("no kind of reviewer is called {id}"));
    let first = segment(these).ok_or_else(|| unknown(these))?;
    let counted = stored(report, first.id).ok_or_else(|| {
        refused(
            "this game was counted before its reviewers were told apart; reading it again \
             counts them"
                .to_owned(),
        )
    })?;
    let (against, (label, who)) = match others {
        Some(id) => {
            let second = segment(id).ok_or_else(|| unknown(id))?;
            if second.split != first.split || second.id == first.id {
                return Err(refused(format!(
                    "{} and {} are not two answers to one question, so their reviews overlap",
                    first.label, second.label
                )));
            }
            (
                stored(report, second.id)
                    .cloned()
                    .unwrap_or_else(|| SegmentCount::empty(second.id)),
                (second.label, second.who),
            )
        }
        None => (
            everyone_else(report, first),
            ("Everyone else", "Everyone else"),
        ),
    };
    for (side, name) in [(counted, first.label), (&against, label)] {
        if side.reviews < ENOUGH {
            return Err(refused(format!(
                "{name}: {} reviews, fewer than the {ENOUGH} a share needs to mean anything",
                side.reviews
            )));
        }
    }

    let subjects = SHEET
        .iter()
        .enumerate()
        .map(|(slot, category)| Beside {
            id: category.id,
            label: category.label,
            these: Figures::of(counted, slot),
            others: Figures::of(&against, slot),
            raised: Gap::between(
                (SegmentCount::at(&counted.raised, slot), counted.reviews),
                (SegmentCount::at(&against.raised, slot), against.reviews),
            ),
            praise: Gap::between(
                (counted.praising(slot), counted.reviews),
                (against.praising(slot), against.reviews),
            ),
            complaint: Gap::between(
                (counted.complaining(slot), counted.reviews),
                (against.complaining(slot), against.reviews),
            ),
        })
        .collect();
    Ok(Comparison {
        split: first.split,
        these: Head::of(counted, (first.label, first.who)),
        others: Head::of(&against, (label, who)),
        recommended: Gap::between(
            (counted.positive, counted.reviews),
            (against.positive, against.reviews),
        ),
        subjects,
        months: counted.months.clone(),
    })
}

/// What a finding is a difference in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Said {
    Recommends,
    Praises,
    Complains,
}

/// One kind of reviewer saying something more or less often than everyone else, by more than
/// chance.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub segment: &'static str,
    pub label: &'static str,
    /// None where the finding is about the share recommending the game.
    pub subject: Option<&'static str>,
    pub said: Said,
    pub gap: Gap,
    pub sentence: String,
}

fn sentence(these: &Segment, said: Said, category: Option<&Category>, gap: &Gap) -> String {
    let does = match (said, category) {
        (Said::Praises, Some(category)) => format!("praise {}", subject_phrase(category)),
        (Said::Complains, Some(category)) => format!("complain about {}", subject_phrase(category)),
        _ => "recommend the game".to_owned(),
    };
    format!(
        "{} {does} in {} of their reviews, against {} of {}.",
        these.who,
        percent(gap.share),
        percent(gap.against),
        others_phrase(these)
    )
}

/// The kinds of a split that are each held against the rest: all of them, except where there
/// are two, when the second against the first is the first against the second again.
fn tested(split: &str) -> impl Iterator<Item = &'static Segment> {
    let kinds: Vec<&'static Segment> = kinds_of(split).collect();
    let take = if kinds.len() == 2 { 1 } else { kinds.len() };
    kinds.into_iter().take(take)
}

/// Every kind of reviewer that praises, complains about or recommends something more or less
/// often than everyone else of its split, by more than chance and by enough to act on; clearest
/// first.
///
/// A subject that says nothing about the game has nothing to praise or complain about, and is
/// left out.
#[must_use]
pub fn findings(report: &ReadReport) -> Vec<Finding> {
    let mut found = Vec::new();
    for split in &SPLITS {
        for these in tested(split.id) {
            let Some(counted) = stored(report, these.id) else {
                continue;
            };
            let others = everyone_else(report, these);
            if counted.reviews < ENOUGH || others.reviews < ENOUGH {
                continue;
            }
            let mut note = |said: Said, category: Option<&'static Category>, gap: Gap| {
                if gap.clear {
                    found.push(Finding {
                        segment: these.id,
                        label: these.label,
                        subject: category.map(|category| category.id),
                        said,
                        sentence: sentence(these, said, category, &gap),
                        gap,
                    });
                }
            };
            note(
                Said::Recommends,
                None,
                Gap::between(
                    (counted.positive, counted.reviews),
                    (others.positive, others.reviews),
                ),
            );
            for (slot, category) in SHEET.iter().enumerate() {
                if category.id == "offtopic" {
                    continue;
                }
                note(
                    Said::Praises,
                    Some(category),
                    Gap::between(
                        (counted.praising(slot), counted.reviews),
                        (others.praising(slot), others.reviews),
                    ),
                );
                note(
                    Said::Complains,
                    Some(category),
                    Gap::between(
                        (counted.complaining(slot), counted.reviews),
                        (others.complaining(slot), others.reviews),
                    ),
                );
            }
        }
    }
    found.sort_by(|left, right| right.gap.z.abs().total_cmp(&left.gap.z.abs()));
    found
}

/// One kind of reviewer as a list of them shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Kind {
    pub id: &'static str,
    pub label: &'static str,
    pub reviews: u64,
    /// Whether it holds enough reviews for any share of it to be shown.
    pub enough: bool,
    /// The share recommending the game against everyone else of its split, where both hold
    /// enough reviews.
    pub recommended: Option<Gap>,
}

/// One fact and the kinds of reviewer it makes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Kinds {
    pub id: &'static str,
    pub label: &'static str,
    pub kinds: Vec<Kind>,
}

/// Every kind of reviewer the reading counted, by the fact that makes it; nothing where the
/// reading was counted before reviewers were told apart.
#[must_use]
pub fn kinds(report: &ReadReport) -> Vec<Kinds> {
    if report.who.is_empty() {
        return Vec::new();
    }
    SPLITS
        .iter()
        .map(|split| Kinds {
            id: split.id,
            label: split.label,
            kinds: kinds_of(split.id)
                .map(|these| {
                    let reviews = stored(report, these.id).map_or(0, |count| count.reviews);
                    let others = everyone_else(report, these);
                    Kind {
                        id: these.id,
                        label: these.label,
                        reviews,
                        enough: reviews >= ENOUGH,
                        recommended: (reviews >= ENOUGH && others.reviews >= ENOUGH).then(|| {
                            let counted = stored(report, these.id)
                                .map_or((0, 0), |count| (count.positive, count.reviews));
                            Gap::between(counted, (others.positive, others.reviews))
                        }),
                    }
                })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reviewer(played_minutes: Option<u32>) -> Reviewer {
        Reviewer {
            played_minutes,
            ..Reviewer::default()
        }
    }

    fn kinds_holding(reviewer: &Reviewer) -> Vec<&'static str> {
        Membership::of(reviewer)
            .positions()
            .map(|at| SEGMENTS[at].id)
            .collect()
    }

    fn slot(id: &str) -> usize {
        SHEET.iter().position(|category| category.id == id).unwrap()
    }

    /// A reading whose kinds of reviewer were counted as given, the rest empty.
    fn reading(counts: Vec<SegmentCount>) -> ReadReport {
        let mut report: ReadReport = serde_json::from_value(serde_json::json!({
            "app_id": 1, "reviews": 1, "corpus_reviews": 1, "language": null, "claims": 1,
            "unclassified_claims": 0, "silent_reviews": 0, "positive": 0, "top_helpful": 0,
            "model": "m", "threshold": 0.5, "device": "cpu", "context": false,
            "subjects": [], "languages": [], "months": []
        }))
        .unwrap();
        let mut given: std::collections::HashMap<String, SegmentCount> = counts
            .into_iter()
            .map(|count| (count.id.clone(), count))
            .collect();
        report.who = SEGMENTS
            .iter()
            .map(|segment| {
                given
                    .remove(segment.id)
                    .unwrap_or_else(|| SegmentCount::empty(segment.id))
            })
            .collect();
        report
    }

    /// `reviews` reviews of one kind, `positive` of them recommending, and on one subject
    /// `praising` reviews praising it alone and `complaining` complaining about it alone.
    fn kind(
        id: &str,
        reviews: u64,
        positive: u64,
        subject: &str,
        (praising, complaining): (u64, u64),
    ) -> SegmentCount {
        let mut count = SegmentCount::empty(id);
        count.reviews = reviews;
        count.positive = positive;
        count.claims = reviews * 3;
        let at = slot(subject);
        count.praised[at] = praising;
        count.criticised[at] = complaining;
        count.raised[at] = praising + complaining;
        count.recommending[at] = praising;
        count.claims_about[at] = (praising + complaining) * 2;
        count
    }

    #[test]
    fn time_played_falls_in_one_band_with_the_refund_window_first() {
        let band = |minutes: u32| kinds_holding(&reviewer(Some(minutes)))[0];
        assert_eq!(band(0), "under-2-hours");
        assert_eq!(band(119), "under-2-hours");
        assert_eq!(band(120), "2-to-10-hours");
        assert_eq!(band(599), "2-to-10-hours");
        assert_eq!(band(600), "10-to-30-hours");
        assert_eq!(band(1_799), "10-to-30-hours");
        assert_eq!(band(1_800), "30-to-100-hours");
        assert_eq!(band(5_999), "30-to-100-hours");
        assert_eq!(band(6_000), "100-hours-or-more");
        assert_eq!(band(u32::MAX), "100-hours-or-more");
        assert_eq!(REFUND_WINDOW, 120, "Steam's refund window is two hours");
    }

    #[test]
    fn a_reviewer_is_one_answer_to_every_question_and_no_band_where_steam_gave_no_time() {
        let everything = Reviewer {
            played_minutes: Some(700),
            deck: true,
            early_access: true,
            free: true,
        };
        assert_eq!(
            kinds_holding(&everything),
            [
                "10-to-30-hours",
                "steam-deck",
                "early-access",
                "got-it-free"
            ]
        );
        assert_eq!(
            kinds_holding(&reviewer(None)),
            ["elsewhere", "after-release", "paid-for-it"]
        );
        assert!(segment("steam-deck").unwrap().holds(&everything));
        assert!(!segment("elsewhere").unwrap().holds(&everything));
        for split in &SPLITS {
            for minutes in [None, Some(0), Some(130), Some(9_000)] {
                let held = kinds_holding(&reviewer(minutes))
                    .into_iter()
                    .filter(|id| segment(id).unwrap().split == split.id)
                    .count();
                let expected = usize::from(split.id != "played" || minutes.is_some());
                assert_eq!(held, expected, "{} at {minutes:?}", split.id);
            }
        }
    }

    #[test]
    fn a_review_is_counted_in_every_kind_its_writer_is_and_no_other() {
        let mut tally = Tally::new();
        let member = Membership::of(&Reviewer {
            played_minutes: Some(30),
            deck: true,
            ..Reviewer::default()
        });
        let (bugs, story, audio) = (slot("bugs"), slot("story"), slot("audio"));
        let mut praise = vec![false; SHEET.len()];
        let mut complaint = vec![false; SHEET.len()];
        praise[story] = true;
        complaint[story] = true;
        complaint[bugs] = true;
        praise[audio] = true;
        tally.review(
            member,
            &Reviewed {
                recommended: true,
                month: "2024-03",
                claims: 4,
                subjects: &[bugs, story, audio, slot("graphics")],
                praise: &praise,
                complaint: &complaint,
            },
        );
        tally.review(
            member,
            &Reviewed {
                recommended: false,
                month: "2024-01",
                claims: 2,
                subjects: &[bugs],
                praise: &vec![false; SHEET.len()],
                complaint: &complaint,
            },
        );
        tally.claim(member, bugs);
        tally.claim(member, bugs);
        tally.claim(member, story);
        let counts = tally.finish();

        for count in &counts {
            let held = [
                "under-2-hours",
                "steam-deck",
                "after-release",
                "paid-for-it",
            ]
            .contains(&count.id.as_str());
            assert_eq!(count.reviews, if held { 2 } else { 0 }, "{}", count.id);
        }
        let newcomers = &counts[0];
        assert_eq!((newcomers.positive, newcomers.claims), (1, 6));
        assert_eq!(newcomers.raised[bugs], 2);
        assert_eq!(newcomers.recommending[bugs], 1);
        assert_eq!(
            (
                newcomers.praised[bugs],
                newcomers.criticised[bugs],
                newcomers.mixed[bugs]
            ),
            (0, 2, 0)
        );
        assert_eq!(
            (
                newcomers.praised[story],
                newcomers.criticised[story],
                newcomers.mixed[story]
            ),
            (0, 0, 1)
        );
        assert_eq!(newcomers.praised[audio], 1);
        assert_eq!(
            newcomers.raised[slot("graphics")],
            1,
            "raised and neither praised nor complained about"
        );
        assert_eq!(
            newcomers.mixed[slot("graphics")] + newcomers.praised[slot("graphics")],
            0
        );
        assert_eq!(
            (newcomers.claims_about[bugs], newcomers.claims_about[story]),
            (2, 1)
        );
        assert_eq!(
            newcomers.months,
            [
                SegmentMonth {
                    label: "2024-01".to_owned(),
                    reviews: 1,
                    positive: 0
                },
                SegmentMonth {
                    label: "2024-03".to_owned(),
                    reviews: 1,
                    positive: 1
                },
            ],
            "oldest first"
        );
        assert_eq!(counts[1].months, []);
    }

    #[test]
    fn a_gap_is_measured_as_what_moved_measures_one_and_clears_only_both_bars() {
        let gap = Gap::between((30, 100), (60, 400));
        assert!((gap.share - 0.30).abs() < 1e-12 && (gap.against - 0.15).abs() < 1e-12);
        let spread = (0.18_f64 * 0.82 * (1.0 / 400.0 + 1.0 / 100.0)).sqrt();
        assert!((gap.z - 0.15 / spread).abs() < 1e-9, "z was {}", gap.z);
        assert!(!gap.clear, "3.5 standard errors is not four");

        let wide = Gap::between((60, 200), (60, 1_000));
        assert!(wide.z >= CLEAR && wide.clear);
        let lower = Gap::between((60, 1_000), (60, 200));
        assert!(
            lower.z <= -CLEAR && lower.clear,
            "fewer is as much a finding as more"
        );
        let small = Gap::between((415_000, 1_000_000), (400_000, 1_000_000));
        assert!(
            small.z > CLEAR && !small.clear,
            "a point and a half is beyond chance in a million reviews and not worth a line"
        );
        let two_and_a_half = Gap::between((425_000, 1_000_000), (400_000, 1_000_000));
        assert!(two_and_a_half.clear);
        let nothing = Gap::between((0, 0), (0, 0));
        assert!(nothing.z.abs() < f64::EPSILON && !nothing.clear);
        assert!(
            Gap::between((20_000, 1_000_000), (0, 1_000_000)).clear,
            "two points exactly is worth saying"
        );
    }

    #[test]
    fn a_difference_is_clear_from_four_standard_errors_either_way() {
        assert!(clear(0.40, 0.45, CLEAR));
        assert!(clear(0.45, 0.40, -CLEAR));
        assert!(!clear(0.40, 0.60, CLEAR - 0.01));
        assert!(!clear(0.40, 0.60, -(CLEAR - 0.01)));
        assert!(!clear(0.400, 0.405, 12.0));
    }

    #[test]
    fn one_kind_is_held_against_everyone_else_of_its_question_added_together() {
        let report = reading(vec![
            kind("100-hours-or-more", 400, 360, "content", (10, 120)),
            kind("under-2-hours", 300, 150, "content", (5, 5)),
            kind("10-to-30-hours", 500, 400, "content", (40, 20)),
            kind("steam-deck", 10_000, 9_000, "content", (0, 0)),
        ]);
        let compared = compare(&report, "100-hours-or-more", None).unwrap();
        assert_eq!(compared.split, "played");
        assert_eq!(
            (compared.others.id.as_str(), compared.others.label.as_str()),
            (EVERYONE_ELSE, "Everyone else")
        );
        assert_eq!(compared.others.reviews, 800, "the other bands, and no Deck");
        assert_eq!(compared.others.who, "Everyone else");
        assert_eq!(
            compared.these.who,
            "Reviewers with 100 hours or more played"
        );
        assert_eq!(compared.these.label, "100 hours or more");
        assert_eq!(
            (compared.these.reviews, compared.these.claims),
            (400, 1_200)
        );
        assert!((compared.these.recommending - 0.9).abs() < 1e-12);
        let (low, high) = crate::measure::wilson(360, 400).unwrap();
        assert_eq!((compared.these.low, compared.these.high), (low, high));
        assert!((compared.others.recommending - 550.0 / 800.0).abs() < 1e-12);
        assert_eq!(compared.recommended, Gap::between((360, 400), (550, 800)));

        assert_eq!(compared.subjects.len(), SHEET.len());
        let content = &compared.subjects[slot("content")];
        assert_eq!(content.id, "content");
        assert_eq!(content.label, SHEET[slot("content")].label);
        assert_eq!(content.these.raised, 130);
        assert!((content.these.rate - 130.0 / 400.0).abs() < 1e-12);
        let (low, high) = crate::measure::wilson(130, 400).unwrap();
        assert_eq!((content.these.low, content.these.high), (low, high));
        assert_eq!(
            (
                content.these.praised,
                content.these.criticised,
                content.these.mixed
            ),
            (10, 120, 0)
        );
        assert!((content.these.complaining - 0.3).abs() < 1e-12);
        assert!((content.others.complaining - 25.0 / 800.0).abs() < 1e-12);
        assert!((content.others.praising - 45.0 / 800.0).abs() < 1e-12);
        assert_eq!(content.these.claims, 260);
        assert_eq!(content.others.raised, 70);
        assert!(content.complaint.clear && content.complaint.z > 0.0);
        assert_eq!(
            content.raised,
            Gap::between((130, 400), (70, 800)),
            "raising it is compared too"
        );
        assert_eq!(content.praise, Gap::between((10, 400), (45, 800)));
        assert_eq!(compared.months, report.who[4].months);
    }

    #[test]
    fn a_mixed_review_is_both_praise_and_complaint_in_a_comparison() {
        let mut veterans = kind("100-hours-or-more", 200, 100, "story", (0, 0));
        veterans.mixed[slot("story")] = 50;
        veterans.raised[slot("story")] = 50;
        let report = reading(vec![
            veterans,
            kind("under-2-hours", 200, 100, "story", (0, 0)),
        ]);
        let story = &compare(&report, "100-hours-or-more", None)
            .unwrap()
            .subjects[slot("story")];
        assert!((story.these.praising - 0.25).abs() < 1e-12);
        assert!((story.these.complaining - 0.25).abs() < 1e-12);
        assert!(story.praise.clear && story.complaint.clear);
    }

    #[test]
    fn two_answers_to_one_question_are_compared_with_each_other() {
        let report = reading(vec![
            kind("steam-deck", 150, 140, "performance", (0, 60)),
            kind("elsewhere", 5_000, 4_000, "performance", (100, 300)),
            kind("under-2-hours", 5_000, 4_000, "performance", (0, 0)),
        ]);
        let named = compare(&report, "steam-deck", Some("elsewhere")).unwrap();
        let unnamed = compare(&report, "steam-deck", None).unwrap();
        assert_eq!(named.others.label, "Mostly elsewhere");
        assert_eq!(named.others.who, "Reviewers playing mostly elsewhere");
        assert_eq!(named.others.id, "elsewhere");
        assert_eq!(named.subjects, unnamed.subjects);
        assert_eq!(named.others.reviews, 5_000);
    }

    #[test]
    fn a_comparison_is_refused_where_it_would_mean_nothing() {
        let report = reading(vec![
            kind("steam-deck", 99, 90, "performance", (0, 0)),
            kind("elsewhere", 5_000, 4_000, "performance", (0, 0)),
            kind("got-it-free", 100, 50, "price", (0, 0)),
            kind("paid-for-it", 99, 50, "price", (0, 0)),
            kind("under-2-hours", 100, 50, "price", (0, 0)),
            kind("2-to-10-hours", 100, 50, "price", (0, 0)),
        ]);
        let why = |these: &str, others: Option<&str>| match compare(&report, these, others) {
            Err(crate::Error::Refused(why)) => why,
            other => panic!("{these} against {others:?} was not refused: {other:?}"),
        };
        assert!(why("veterans", None).contains("no kind of reviewer is called veterans"));
        assert!(why("under-2-hours", Some("nobody")).contains("nobody"));
        assert!(why("under-2-hours", Some("steam-deck")).contains("overlap"));
        assert!(why("under-2-hours", Some("under-2-hours")).contains("overlap"));
        assert!(why("steam-deck", None).contains("99 reviews"));
        assert!(
            why("got-it-free", None).starts_with("Everyone else: 99 reviews"),
            "the side that is too small is the one named"
        );
        assert!(compare(&report, "under-2-hours", Some("2-to-10-hours")).is_ok());
        assert!(
            compare(&report, "under-2-hours", Some("30-to-100-hours")).is_err(),
            "a kind nobody wrote in is too few, not missing"
        );

        let mut before = reading(Vec::new());
        before.who.clear();
        assert!(matches!(
            compare(&before, "under-2-hours", None),
            Err(crate::Error::Refused(why)) if why.contains("reading it again")
        ));
    }

    #[test]
    fn veterans_complaining_about_the_endgame_is_found_and_said_in_plain_words() {
        let report = reading(vec![
            kind("100-hours-or-more", 400, 360, "content", (10, 120)),
            kind("under-2-hours", 300, 270, "tutorial", (90, 0)),
            kind("10-to-30-hours", 500, 450, "content", (40, 20)),
        ]);
        let found = findings(&report);
        let veterans = found
            .iter()
            .find(|finding| finding.segment == "100-hours-or-more")
            .expect("a complaint three times as common among veterans");
        assert_eq!(veterans.subject, Some("content"));
        assert_eq!(veterans.said, Said::Complains);
        assert_eq!(veterans.label, "100 hours or more");
        assert_eq!(
            veterans.sentence,
            "Reviewers with 100 hours or more played complain about amount of content in 30.0% \
             of their reviews, against 2.5% of everyone else."
        );
        let newcomers = found
            .iter()
            .find(|finding| finding.segment == "under-2-hours")
            .expect("newcomers praising the tutorial");
        assert_eq!(newcomers.said, Said::Praises);
        assert_eq!(newcomers.subject, Some("tutorial"));
        assert!(newcomers.sentence.starts_with(
            "Reviewers with under 2 hours played praise tutorial and learning in 30.0%"
        ));
        assert!(
            found
                .windows(2)
                .all(|pair| pair[0].gap.z.abs() >= pair[1].gap.z.abs()),
            "clearest first"
        );
        assert!(found.iter().all(|finding| finding.gap.clear));
    }

    #[test]
    fn a_difference_chance_could_make_is_not_a_finding() {
        let report = reading(vec![
            kind("100-hours-or-more", 400, 360, "content", (10, 22)),
            kind("under-2-hours", 400, 360, "content", (10, 20)),
        ]);
        assert_eq!(findings(&report), []);
    }

    #[test]
    fn a_kind_with_too_few_reviews_is_never_compared() {
        let report = reading(vec![
            kind("100-hours-or-more", ENOUGH - 1, 10, "content", (0, 90)),
            kind("under-2-hours", 5_000, 4_500, "content", (0, 0)),
            kind("got-it-free", 5_000, 1_000, "price", (0, 4_000)),
            kind("paid-for-it", ENOUGH - 1, 90, "price", (0, 0)),
        ]);
        let found = findings(&report);
        assert!(
            found
                .iter()
                .all(|finding| finding.segment != "100-hours-or-more"),
            "{found:?}"
        );
        assert!(
            found.iter().all(|finding| finding.segment != "got-it-free"),
            "everyone else holds too few to set it against: {found:?}"
        );

        let enough = reading(vec![
            kind("100-hours-or-more", ENOUGH, 10, "content", (0, 90)),
            kind("under-2-hours", 5_000, 4_500, "content", (0, 0)),
        ]);
        let found = findings(&enough);
        assert!(
            found
                .iter()
                .any(|finding| finding.segment == "100-hours-or-more")
        );
        assert!(
            found
                .iter()
                .any(|finding| finding.segment == "under-2-hours"),
            "everyone else at exactly enough is enough to set newcomers against"
        );
    }

    #[test]
    fn two_answers_to_one_question_are_one_finding_not_two_mirrored() {
        let report = reading(vec![
            kind("steam-deck", 400, 360, "performance", (0, 160)),
            kind("elsewhere", 4_000, 3_600, "performance", (0, 400)),
        ]);
        let found = findings(&report);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].segment, "steam-deck");
        assert_eq!(
            found[0].sentence,
            "Reviewers playing mostly on a Steam Deck complain about performance in 40.0% of \
             their reviews, against 10.0% of reviewers playing mostly elsewhere."
        );
    }

    #[test]
    fn the_share_recommending_is_held_to_the_same_rule() {
        let report = reading(vec![
            kind("under-2-hours", 400, 200, "content", (0, 0)),
            kind("10-to-30-hours", 4_000, 3_600, "content", (0, 0)),
        ]);
        let found = findings(&report);
        let newcomers = found
            .iter()
            .find(|finding| finding.segment == "under-2-hours")
            .unwrap();
        assert_eq!(newcomers.said, Said::Recommends);
        assert_eq!(newcomers.subject, None);
        assert_eq!(
            newcomers.sentence,
            "Reviewers with under 2 hours played recommend the game in 50.0% of their reviews, \
             against 90.0% of everyone else."
        );
    }

    #[test]
    fn a_subject_that_says_nothing_about_the_game_has_nothing_to_differ_on() {
        let report = reading(vec![
            kind("under-2-hours", 400, 360, "offtopic", (0, 200)),
            kind("10-to-30-hours", 4_000, 3_600, "offtopic", (0, 0)),
        ]);
        assert_eq!(findings(&report), []);
    }

    #[test]
    fn a_subject_is_named_in_a_sentence_as_a_reader_would_name_it() {
        let named = |id: &str| subject_phrase(&SHEET[slot(id)]);
        assert_eq!(named("bugs"), "bugs and crashes");
        assert_eq!(named("vr"), "VR and headsets");
        assert_eq!(named("verdict"), "the game as a whole");
        assert_eq!(mid_sentence("A Word of VR"), "a word of VR");
        assert_eq!(lowered_first(""), "");
        assert_eq!(lowered_first("Reviewers who"), "reviewers who");
        let free = segment("got-it-free").unwrap();
        assert_eq!(others_phrase(free), "reviewers who paid for it");
        assert_eq!(
            others_phrase(segment("2-to-10-hours").unwrap()),
            "everyone else"
        );
    }

    #[test]
    fn every_kind_is_listed_under_its_question_with_whether_it_can_be_shown() {
        let report = reading(vec![
            kind("steam-deck", ENOUGH - 1, 90, "content", (0, 0)),
            kind("elsewhere", 5_000, 4_000, "content", (0, 0)),
            kind("got-it-free", ENOUGH, 20, "content", (0, 0)),
            kind("paid-for-it", 5_000, 4_500, "content", (0, 0)),
        ]);
        let listed = kinds(&report);
        assert_eq!(
            listed.iter().map(|split| split.id).collect::<Vec<_>>(),
            ["played", "deck", "early-access", "copy"]
        );
        assert_eq!(listed[0].label, "Time played when they wrote it");
        assert_eq!(listed[0].kinds.len(), 5);
        let deck = &listed[1].kinds;
        assert_eq!(
            (deck[0].id, deck[0].label),
            ("steam-deck", "Mostly on a Steam Deck")
        );
        assert_eq!((deck[0].reviews, deck[0].enough), (ENOUGH - 1, false));
        assert_eq!(deck[0].recommended, None);
        assert!(deck[1].enough);
        assert_eq!(
            deck[1].recommended, None,
            "the Deck is too few to set the rest against"
        );
        let free = &listed[3].kinds[0];
        assert!(free.enough);
        assert_eq!(
            free.recommended,
            Some(Gap::between((20, ENOUGH), (4_500, 5_000)))
        );
        assert!(free.recommended.unwrap().clear);
        assert!(
            listed[3].kinds[1].recommended.is_some(),
            "everyone else at exactly enough is enough"
        );

        let mut before = reading(Vec::new());
        before.who.clear();
        assert_eq!(kinds(&before), []);
    }

    #[test]
    fn everyone_else_adds_every_count_of_the_other_kinds() {
        let mut one = kind("under-2-hours", 10, 4, "story", (1, 2));
        one.mixed[slot("story")] = 3;
        one.claims = 7;
        let mut other = kind("2-to-10-hours", 20, 5, "story", (4, 8));
        other.mixed[slot("story")] = 1;
        other.claims = 9;
        let report = reading(vec![
            one,
            other,
            kind("10-to-30-hours", 1, 1, "story", (0, 0)),
        ]);
        let rest = everyone_else(&report, segment("100-hours-or-more").unwrap());
        let story = slot("story");
        assert_eq!((rest.reviews, rest.positive, rest.claims), (31, 10, 19));
        assert_eq!(
            (
                rest.raised[story],
                rest.praised[story],
                rest.criticised[story],
                rest.mixed[story]
            ),
            (15, 5, 10, 4)
        );
        assert_eq!(rest.recommending[story], 5);
        assert_eq!(rest.claims_about[story], 30);
        assert_eq!((rest.praising(story), rest.complaining(story)), (9, 14));
    }
}
