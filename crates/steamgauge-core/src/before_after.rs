//! Each subject before and after one of a game's updates.
//!
//! The four weeks before an update was posted against the four weeks after it, counted from the
//! reviews written in each: the share recommending the game, and for each subject the share of
//! reviews praising it and the share complaining about it. The rules are those of what moved
//! lately ([`crate::moves`]): a share is compared only where both windows hold enough reviews for
//! it to mean something, and a difference is called a change only where it spans three standard
//! errors and two points. A change found here happened across the update; nothing here says the
//! update is why.

use std::{collections::HashMap, path::Path};

use serde::Serialize;

use crate::{
    Result,
    moves::Shift,
    read::{Month, ReadReport},
    reader::Polarity,
    taxonomy::SHEET,
    updates::Update,
    who::Membership,
};

/// Days in each window. Four whole weeks, so each window holds every day of the week four times
/// and neither leans on an extra weekend; close to the thirty days Steam's own recent reviews
/// cover, which players already read as lately; and short enough that a game patched every month
/// has most of each window to itself.
pub const WINDOW_DAYS: i64 = 28;

const WINDOW: i64 = WINDOW_DAYS * 86_400;

/// Reviews each window needs before a share of it is compared. The floor what moved lately holds
/// its shorter window to: here the two windows are the same length, so both are held to it.
pub const ENOUGH: u64 = crate::moves::ENOUGH_RECENT;

/// One review a reading counted: when it was written, whether it recommends the game, and the
/// subjects it praises and complains about, a bit each in taxonomy order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dated {
    pub created: i64,
    pub recommends: bool,
    pub praise: u64,
    pub complaint: u64,
    /// The kinds of reviewer its writer is, so one kind's reviews can be compared alone.
    pub kinds: Membership,
}

/// The reviews one kind of reviewer wrote, by its place in [`crate::who::SEGMENTS`].
#[must_use]
pub fn written_by(reviews: &[Dated], kind: usize) -> Vec<Dated> {
    reviews
        .iter()
        .filter(|review| review.kinds.positions().any(|at| at == kind))
        .copied()
        .collect()
}

/// Every review a game's reading counted, oldest first, with the sides its stored answers take.
///
/// The capture says when each review was written and the readings what each says, so an update
/// splits the reviews at the second it was posted rather than at a month's end. Counted as the
/// reading counts: the reviews in its language, each praising a subject where any of its claims
/// does, whether as the subject the claim is chiefly about or one named beside it.
///
/// # Errors
///
/// Fails if the capture or the readings cannot be read.
pub fn dated_reviews(snapshot: &Path, language: Option<&str>) -> Result<Vec<Dated>> {
    let written = crate::capture::rows_kept(snapshot, |row, _| {
        language
            .is_none_or(|wanted| wanted == row.language)
            .then(|| {
                (
                    row.recommendationid,
                    row.created,
                    row.voted_up,
                    Membership::of(&row.reviewer),
                )
            })
    })?;
    let at: HashMap<&str, usize> = written
        .iter()
        .enumerate()
        .map(|(index, (id, ..))| (id.as_str(), index))
        .collect();
    let mut sides = vec![(0_u64, 0_u64); written.len()];
    let slot_of = |name: &str| SHEET.iter().position(|row| row.id == name);
    crate::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, _, subject, _, polarity, also| {
            let Some(&index) = at.get(id) else {
                return;
            };
            let (praise, complaint) = &mut sides[index];
            let first = subject
                .and_then(slot_of)
                .map(|slot| (slot, Polarity::from_name(polarity)));
            for (slot, said) in first.into_iter().chain(also.iter()) {
                match said {
                    Polarity::Praise => *praise |= 1 << slot,
                    Polarity::Complaint => *complaint |= 1 << slot,
                    Polarity::Neutral => {}
                }
            }
        },
    )?;
    let mut dated: Vec<Dated> = written
        .iter()
        .zip(sides)
        .map(
            |((_, created, recommends, kinds), (praise, complaint))| Dated {
                created: *created,
                recommends: *recommends,
                praise,
                complaint,
                kinds: *kinds,
            },
        )
        .collect();
    dated.sort_by_key(|review| review.created);
    Ok(dated)
}

/// A stretch of time and the reviews written in it, from `from` up to but not including `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Window {
    pub from: i64,
    pub to: i64,
    pub reviews: u64,
}

/// One share in each window, and whether the difference is a change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Compared {
    pub before: f64,
    pub after: f64,
    /// How far apart the two are in standard errors, positive where the share rose.
    pub z: f64,
    /// Beyond chance and big enough to act on, by the rule of what moved lately.
    pub change: bool,
}

impl From<Shift> for Compared {
    fn from(shift: Shift) -> Self {
        Self {
            before: shift.before,
            after: shift.recent,
            z: shift.z,
            change: shift.clear(),
        }
    }
}

/// A subject's praise and complaints either side of an update.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Subject {
    pub subject: &'static str,
    pub label: &'static str,
    /// The share of each window's reviews praising it.
    pub praise: Compared,
    /// The share of each window's reviews complaining about it.
    pub complaint: Compared,
}

impl Subject {
    /// How many standard errors its clearest change spans, or none where neither side changed.
    fn clearest(&self) -> f64 {
        [self.praise, self.complaint]
            .iter()
            .filter(|side| side.change)
            .map(|side| side.z.abs())
            .fold(0.0, f64::max)
    }

    /// Both shares in both windows together, which is how much the subject is raised at all.
    fn raised(&self) -> f64 {
        self.praise.before + self.praise.after + self.complaint.before + self.complaint.after
    }
}

/// What changed across one update.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Around {
    pub update: Update,
    pub before: Window,
    pub after: Window,
    /// Whether the capture holds all four weeks after the update; a capture made soon after one
    /// holds only the days since.
    pub after_whole: bool,
    /// Whether both windows hold enough reviews to compare. Where they do not, nothing is
    /// compared and the subjects are left empty.
    pub enough: bool,
    pub recommended: Option<Compared>,
    /// Every subject either window raises, the clearest change first and then the most raised.
    pub subjects: Vec<Subject>,
    /// The subjects' shares that changed, each side counted once.
    pub changes: usize,
    /// Other updates posted within four weeks either side, which these windows hold too.
    pub nearby: usize,
}

/// Reviews, recommendations and each subject's sides, counted over one window.
struct Counts {
    reviews: u64,
    recommending: u64,
    praising: Vec<u64>,
    complaining: Vec<u64>,
}

fn count(reviews: &[Dated], from: i64, to: i64) -> Counts {
    let start = reviews.partition_point(|review| review.created < from);
    let end = reviews.partition_point(|review| review.created < to);
    let mut counts = Counts {
        reviews: 0,
        recommending: 0,
        praising: vec![0; SHEET.len()],
        complaining: vec![0; SHEET.len()],
    };
    for review in &reviews[start..end] {
        counts.reviews += 1;
        counts.recommending += u64::from(review.recommends);
        for slot in 0..SHEET.len() {
            counts.praising[slot] += (review.praise >> slot) & 1;
            counts.complaining[slot] += (review.complaint >> slot) & 1;
        }
    }
    counts
}

/// Each subject before and after `update`, from `reviews` (oldest first) held up to
/// `held_until`. `updates` are the game's others, named so the windows can say what else they
/// hold.
#[must_use]
pub fn around(update: &Update, reviews: &[Dated], held_until: i64, updates: &[Update]) -> Around {
    let posted = update.posted;
    let ends = (posted + WINDOW).min(held_until).max(posted);
    let (before, after) = (
        count(reviews, posted - WINDOW, posted),
        count(reviews, posted, ends),
    );
    let enough = before.reviews >= ENOUGH && after.reviews >= ENOUGH;
    let compare = |was: u64, is: u64| {
        Shift::between((was, before.reviews), (is, after.reviews), (ENOUGH, ENOUGH))
            .map(Compared::from)
    };
    let mut subjects: Vec<Subject> = SHEET
        .iter()
        .enumerate()
        .filter(|(slot, _)| {
            before.praising[*slot]
                + before.complaining[*slot]
                + after.praising[*slot]
                + after.complaining[*slot]
                > 0
        })
        .filter_map(|(slot, row)| {
            Some(Subject {
                subject: row.id,
                label: row.label,
                praise: compare(before.praising[slot], after.praising[slot])?,
                complaint: compare(before.complaining[slot], after.complaining[slot])?,
            })
        })
        .collect();
    subjects.sort_by(|left, right| {
        right
            .clearest()
            .total_cmp(&left.clearest())
            .then_with(|| right.raised().total_cmp(&left.raised()))
    });
    Around {
        update: update.clone(),
        before: Window {
            from: posted - WINDOW,
            to: posted,
            reviews: before.reviews,
        },
        after: Window {
            from: posted,
            to: ends,
            reviews: after.reviews,
        },
        after_whole: posted + WINDOW <= held_until,
        enough,
        recommended: compare(before.recommending, after.recommending),
        changes: subjects
            .iter()
            .map(|subject| {
                usize::from(subject.praise.change) + usize::from(subject.complaint.change)
            })
            .sum(),
        subjects,
        nearby: updates
            .iter()
            .filter(|other| {
                other.gid != update.gid
                    && other.posted >= posted - WINDOW
                    && other.posted < posted + WINDOW
            })
            .count(),
    }
}

/// When the reviews a reading counted were last added to: the capture's own time where the
/// reading recorded it, and otherwise the newest review it holds.
#[must_use]
pub fn held_until(reading: &ReadReport, reviews: &[Dated]) -> i64 {
    if reading.captured_unix > 0 {
        reading.captured_unix
    } else {
        reviews.last().map_or(0, |newest| newest.created + 1)
    }
}

/// A game's updates, each with its subjects either side.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Updates {
    /// When Steam was last asked for the game's announcements, or none where it never was.
    pub asked: Option<i64>,
    /// Oldest first.
    pub around: Vec<Around>,
}

/// Every update a library keeps for a game, each with its subjects either side; nothing is
/// walked where Steam has not been asked about the game or named no update.
///
/// # Errors
///
/// Fails if the capture or the readings cannot be read.
pub fn of_game(out_dir: &Path, snapshot: &Path, reading: &ReadReport) -> Result<Updates> {
    let Some(kept) = crate::updates::kept(out_dir, reading.app_id) else {
        return Ok(Updates::default());
    };
    let updates = kept.updates();
    if updates.is_empty() {
        return Ok(Updates {
            asked: Some(kept.asked),
            around: Vec::new(),
        });
    }
    let reviews = dated_reviews(snapshot, reading.language.as_deref())?;
    let held = held_until(reading, &reviews);
    Ok(Updates {
        asked: Some(kept.asked),
        around: updates
            .iter()
            .map(|update| around(update, &reviews, held, &updates))
            .collect(),
    })
}

/// The updates the most reviews followed, up to `how_many`, oldest first. Only updates with
/// enough reviews either side are taken, and none within four weeks of one already taken, so a
/// launch and its week of hotfixes are one moment rather than four.
#[must_use]
pub fn biggest(arounds: &[Around], how_many: usize) -> Vec<&Around> {
    let mut ranked: Vec<&Around> = arounds.iter().filter(|one| one.enough).collect();
    ranked.sort_by_key(|one| std::cmp::Reverse(one.after.reviews));
    let mut chosen: Vec<&Around> = Vec::new();
    for one in ranked {
        if chosen.len() == how_many {
            break;
        }
        if chosen
            .iter()
            .all(|taken| (taken.update.posted - one.update.posted).abs() >= WINDOW)
        {
            chosen.push(one);
        }
    }
    chosen.sort_by_key(|one| one.update.posted);
    chosen
}

/// Where a moment falls on a timeline of `months` (oldest first, a column each), in columns
/// from its left edge: the month it falls in and how far through that month. A month with no
/// reviews has no column, so a moment in one sits where the next column starts. None outside
/// the months drawn.
#[must_use]
pub fn position(at: i64, months: &[Month]) -> Option<f64> {
    let label = crate::time::year_month(at);
    let (first, last) = (months.first()?, months.last()?);
    if label < first.label || label > last.label {
        return None;
    }
    let column = months.partition_point(|month| month.label < label);
    let through = if months[column].label == label {
        through_month(at)
    } else {
        0.0
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a corpus spans hundreds of months at most"
    )]
    let column = column as f64;
    Some(column + through)
}

/// How far through its month a moment falls, from 0 at the first second to just under 1.
#[must_use]
pub fn through_month(at: i64) -> f64 {
    let (year, month, day) = crate::time::civil(at);
    let into_day = at.rem_euclid(86_400);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a day of a month and a second of a day are both far below 2^53"
    )]
    let elapsed = f64::from(day - 1) + into_day as f64 / 86_400.0;
    elapsed / f64::from(days_in(year, month))
}

fn days_in(year: i64, month: u8) -> u8 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moves::{CLEAR, WORTH_SAYING};

    const DAY: i64 = 86_400;
    /// Midday on 1 March 2024, where the test corpus starts.
    const MARCH: i64 = 1_709_294_400;

    fn slot(id: &str) -> usize {
        SHEET.iter().position(|row| row.id == id).unwrap()
    }

    fn bit(id: &str) -> u64 {
        1 << slot(id)
    }

    #[test]
    fn each_review_takes_the_sides_its_claims_take_as_the_reading_counted_them() {
        let out = crate::tempdir::Dir::new();
        let snapshot = crate::read::tests::read_corpus_of(out.path(), 1);
        let reading: ReadReport =
            serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).unwrap()).unwrap();
        let reviews = dated_reviews(&snapshot, None).unwrap();
        assert_eq!(reviews.len(), 7);
        assert!(
            reviews
                .windows(2)
                .all(|pair| pair[0].created <= pair[1].created)
        );
        assert_eq!(
            reviews[0],
            Dated {
                created: MARCH + 14 * DAY,
                recommends: false,
                praise: bit("audio"),
                complaint: bit("bugs"),
                kinds: reviews[0].kinds,
            }
        );
        assert_eq!(
            (reviews[1].praise, reviews[1].complaint),
            (bit("story"), bit("controls")),
            "a subject named beside the first counts too"
        );
        assert_eq!(
            (reviews[5].praise, reviews[5].complaint),
            (bit("bugs"), bit("bugs")),
            "one review can praise and damn the same subject"
        );

        // Counted by month, the sides are the reading's own months, subject by subject.
        for month in &reading.months {
            let within: Vec<&Dated> = reviews
                .iter()
                .filter(|review| crate::time::year_month(review.created) == month.label)
                .collect();
            assert_eq!(within.len() as u64, month.reviews, "{}", month.label);
            assert_eq!(
                within.iter().filter(|review| review.recommends).count() as u64,
                month.positive
            );
            for at in 0..SHEET.len() {
                let praising = within.iter().filter(|r| (r.praise >> at) & 1 == 1).count();
                let complaining = within
                    .iter()
                    .filter(|r| (r.complaint >> at) & 1 == 1)
                    .count();
                assert_eq!(praising as u64, month.praising[at], "{} {at}", month.label);
                assert_eq!(
                    complaining as u64, month.complaining[at],
                    "{} {at}",
                    month.label
                );
            }
        }

        let english = dated_reviews(&snapshot, Some("english")).unwrap();
        assert_eq!(
            english.len(),
            6,
            "the review in Chinese is not counted in English"
        );
    }

    fn update(gid: &str, posted: i64) -> Update {
        Update {
            gid: gid.to_owned(),
            title: format!("Patch {gid}"),
            posted,
            link: crate::updates::link(gid),
        }
    }

    /// `count` reviews written half a minute apart from `from`, the first `complaining` of them
    /// complaining about bugs and the first `recommending` recommending the game.
    fn written(from: i64, count: u64, complaining: u64, recommending: u64) -> Vec<Dated> {
        (0..count)
            .map(|at| Dated {
                created: from + i64::try_from(at).unwrap() * 30,
                recommends: at < recommending,
                praise: bit("story"),
                complaint: if at < complaining { bit("bugs") } else { 0 },
                kinds: Membership::default(),
            })
            .collect()
    }

    /// Posted at day 100, with 400 reviews in the four weeks before, a tenth of them about bugs,
    /// and 300 in the four weeks after, a quarter of them.
    fn patched() -> (Update, Vec<Dated>) {
        let posted = 100 * DAY;
        let mut reviews = written(posted - WINDOW, 400, 40, 360);
        reviews.extend(written(posted, 300, 75, 210));
        (update("1", posted), reviews)
    }

    #[test]
    fn the_four_weeks_either_side_are_compared_as_what_moved_compares_them() {
        let (patch, reviews) = patched();
        let found = around(
            &patch,
            &reviews,
            patch.posted + 60 * DAY,
            std::slice::from_ref(&patch),
        );
        assert_eq!(
            (found.before.from, found.before.to, found.before.reviews),
            (patch.posted - WINDOW, patch.posted, 400)
        );
        assert_eq!(
            (found.after.from, found.after.to, found.after.reviews),
            (patch.posted, patch.posted + WINDOW, 300)
        );
        assert!(found.enough && found.after_whole);
        assert_eq!(found.nearby, 0);

        // 40 of 400 against 75 of 300: pooled 115 of 700.
        let bugs = found.subjects.iter().find(|s| s.subject == "bugs").unwrap();
        assert!((bugs.complaint.before - 0.10).abs() < 1e-12);
        assert!((bugs.complaint.after - 0.25).abs() < 1e-12);
        let pooled = 115.0 / 700.0;
        let spread = (pooled * (1.0 - pooled) * (1.0 / 400.0 + 1.0 / 300.0_f64)).sqrt();
        assert!((bugs.complaint.z - 0.15 / spread).abs() < 1e-9);
        assert!(bugs.complaint.change && bugs.complaint.z >= CLEAR);
        assert!(!bugs.praise.change, "nobody praised the bugs either side");
        assert_eq!(found.subjects[0].subject, "bugs", "the change comes first");

        let story = found
            .subjects
            .iter()
            .find(|s| s.subject == "story")
            .unwrap();
        assert!((story.praise.before - 1.0).abs() < 1e-12 && !story.praise.change);
        assert_eq!(
            found.subjects.len(),
            2,
            "a subject nobody raised is not listed"
        );

        let recommended = found.recommended.unwrap();
        assert!((recommended.before - 0.9).abs() < 1e-12);
        assert!((recommended.after - 0.7).abs() < 1e-12);
        assert!(recommended.change && recommended.z < 0.0);
        assert_eq!(
            found.changes, 1,
            "the recommendation share is not a subject's"
        );
    }

    #[test]
    fn a_subject_is_ranked_by_its_clearest_change_and_then_by_how_much_it_is_raised() {
        let side = |before: f64, after: f64, z: f64, change: bool| Compared {
            before,
            after,
            z,
            change,
        };
        let subject = |praise, complaint| Subject {
            subject: "bugs",
            label: "Bugs",
            praise,
            complaint,
        };
        // Ten standard errors that are half a point is not a change, and does not rank as one.
        let ranked = subject(side(0.40, 0.405, 10.0, false), side(0.1, 0.2, -4.0, true));
        assert!((ranked.clearest() - 4.0).abs() < 1e-12);
        let both = subject(side(0.1, 0.3, 5.0, true), side(0.3, 0.1, -6.0, true));
        assert!((both.clearest() - 6.0).abs() < 1e-12);
        let neither = subject(side(0.1, 0.1, 0.0, false), side(0.2, 0.3, 2.0, false));
        assert!(neither.clearest().abs() < 1e-12);
        assert!((neither.raised() - 0.7).abs() < 1e-12);
    }

    #[test]
    fn subjects_that_did_not_change_are_listed_most_raised_first() {
        let posted = 100 * DAY;
        let review = |created: i64, praise: u64| Dated {
            created,
            recommends: true,
            praise,
            complaint: 0,
            kinds: Membership::default(),
        };
        let mut reviews = Vec::new();
        for at in 0..200 {
            let story = if at % 2 == 0 { bit("story") } else { 0 };
            let both = bit("audio") | story;
            reviews.push(review(posted - WINDOW + at, both));
            reviews.push(review(posted + at, both));
        }
        reviews.sort_by_key(|one| one.created);
        let found = around(&update("1", posted), &reviews, posted + WINDOW, &[]);
        let order: Vec<&str> = found.subjects.iter().map(|s| s.subject).collect();
        assert_eq!(order, ["audio", "story"]);
    }

    #[test]
    fn one_kind_of_reviewer_is_compared_over_its_own_reviews_alone() {
        let kind = |id: &str| {
            crate::who::SEGMENTS
                .iter()
                .position(|segment| segment.id == id)
                .unwrap()
        };
        let free = Membership::of(&crate::who::Reviewer {
            free: true,
            ..crate::who::Reviewer::default()
        });
        let paid = Membership::of(&crate::who::Reviewer::default());
        let reviews: Vec<Dated> = (0..5)
            .map(|at| Dated {
                created: at,
                recommends: true,
                praise: 0,
                complaint: 0,
                kinds: if at < 2 { free } else { paid },
            })
            .collect();
        let created = |kept: Vec<Dated>| kept.iter().map(|one| one.created).collect::<Vec<_>>();
        assert_eq!(created(written_by(&reviews, kind("got-it-free"))), [0, 1]);
        assert_eq!(
            created(written_by(&reviews, kind("paid-for-it"))),
            [2, 3, 4]
        );
        assert_eq!(
            created(written_by(&reviews, kind("steam-deck"))),
            [] as [i64; 0]
        );
    }

    #[test]
    fn a_subject_raised_on_one_side_only_is_listed() {
        let posted = 100 * DAY;
        let mut reviews: Vec<Dated> = (0..200)
            .map(|at| Dated {
                created: posted - WINDOW + at,
                recommends: true,
                praise: if at < 50 { bit("audio") } else { 0 },
                complaint: 0,
                kinds: Membership::default(),
            })
            .collect();
        reviews.extend((0..200).map(|at| Dated {
            created: posted + at,
            recommends: true,
            praise: if at < 50 { bit("story") } else { 0 },
            complaint: if at >= 150 { bit("bugs") } else { 0 },
            kinds: Membership::default(),
        }));
        let found = around(&update("1", posted), &reviews, posted + WINDOW, &[]);
        let audio = found.subjects.iter().find(|s| s.subject == "audio");
        assert!(
            audio.is_some_and(|audio| audio.praise.change && audio.praise.after.abs() < 1e-12),
            "a subject nobody raises after an update is a change, not a gap"
        );
        let mut listed: Vec<&str> = found.subjects.iter().map(|s| s.subject).collect();
        listed.sort_unstable();
        assert_eq!(
            listed,
            ["audio", "bugs", "story"],
            "praised only before, complained about only after, and praised only after"
        );
    }

    #[test]
    fn a_window_holds_from_its_start_up_to_but_not_its_end() {
        let posted = 100 * DAY;
        let at = |created: i64| Dated {
            created,
            recommends: true,
            praise: 0,
            complaint: 0,
            kinds: Membership::default(),
        };
        let reviews = vec![
            at(posted - WINDOW - 1),
            at(posted - WINDOW),
            at(posted - 1),
            at(posted),
            at(posted + WINDOW - 1),
            at(posted + WINDOW),
        ];
        let found = around(&update("1", posted), &reviews, posted + 90 * DAY, &[]);
        assert_eq!((found.before.reviews, found.after.reviews), (2, 2));
    }

    #[test]
    fn too_few_reviews_either_side_compares_nothing_and_says_how_many_there_were() {
        let posted = 100 * DAY;
        let enough_after = |before: u64, after: u64| {
            let mut reviews = written(posted - WINDOW, before, before / 2, before);
            reviews.extend(written(posted, after, 0, 0));
            around(&update("1", posted), &reviews, posted + WINDOW, &[])
        };
        let thin = enough_after(ENOUGH, ENOUGH - 1);
        assert!(!thin.enough);
        assert_eq!(
            (thin.before.reviews, thin.after.reviews),
            (ENOUGH, ENOUGH - 1)
        );
        assert_eq!(thin.recommended, None);
        assert!(thin.subjects.is_empty() && thin.changes == 0);
        assert!(!enough_after(ENOUGH - 1, ENOUGH).enough);
        let just = enough_after(ENOUGH, ENOUGH);
        assert!(just.enough && !just.subjects.is_empty());
    }

    #[test]
    fn a_change_is_called_only_beyond_chance_and_only_when_worth_saying() {
        let posted = 100 * DAY;
        let with = |before: (u64, u64), after: (u64, u64)| {
            let mut reviews = written(posted - WINDOW, before.1, before.0, 0);
            reviews.extend(written(posted, after.1, after.0, 0));
            let found = around(&update("1", posted), &reviews, posted + WINDOW, &[]);
            found
                .subjects
                .iter()
                .find(|s| s.subject == "bugs")
                .unwrap()
                .complaint
        };
        // 20 of 200 against 30 of 200 is 1.8 standard errors: chance could do it.
        let noise = with((20, 200), (30, 200));
        assert!(noise.z.abs() < CLEAR && !noise.change);
        // 50,000 reviews either side make a point and a half significant, and it is still not
        // worth a line.
        let small = with((5_000, 50_000), (5_750, 50_000));
        assert!(small.z.abs() >= CLEAR && (small.after - small.before) < WORTH_SAYING);
        assert!(!small.change);
    }

    #[test]
    fn an_update_the_capture_holds_only_days_after_says_so() {
        let (patch, reviews) = patched();
        let soon = around(&patch, &reviews, patch.posted + 10 * DAY, &[]);
        assert!(!soon.after_whole);
        assert_eq!(soon.after.to, patch.posted + 10 * DAY);
        let later = around(&patch, &reviews, patch.posted + WINDOW, &[]);
        assert!(later.after_whole, "four weeks to the second is four weeks");
        let posted_after = around(&patch, &reviews, patch.posted - DAY, &[]);
        assert_eq!(
            (
                posted_after.after.from,
                posted_after.after.to,
                posted_after.after.reviews
            ),
            (patch.posted, patch.posted, 0)
        );
        assert!(!posted_after.enough);
    }

    #[test]
    fn the_updates_either_side_within_four_weeks_are_counted() {
        let (patch, reviews) = patched();
        let others = [
            patch.clone(),
            update("2", patch.posted - WINDOW),
            update("3", patch.posted - WINDOW - 1),
            update("4", patch.posted + WINDOW - 1),
            update("5", patch.posted + WINDOW),
        ];
        let found = around(&patch, &reviews, patch.posted + 60 * DAY, &others);
        assert_eq!(found.nearby, 2);
    }

    #[test]
    fn reviews_are_held_up_to_the_capture_or_else_past_the_newest() {
        let mut reading: ReadReport = serde_json::from_value(serde_json::json!({
            "app_id": 1, "reviews": 1, "corpus_reviews": 1, "language": null, "claims": 1,
            "unclassified_claims": 0, "silent_reviews": 0, "positive": 0, "top_helpful": 0,
            "model": "m", "threshold": 0.5, "device": "cpu", "context": false,
            "subjects": [], "languages": [], "months": []
        }))
        .unwrap();
        let reviews = written(500, 3, 0, 0);
        assert_eq!(held_until(&reading, &reviews), 500 + 60 + 1);
        assert_eq!(held_until(&reading, &[]), 0);
        reading.captured_unix = 9_000;
        assert_eq!(held_until(&reading, &reviews), 9_000);
    }

    fn after(gid: &str, posted: i64, reviews: u64, enough: bool) -> Around {
        let (patch, written) = patched();
        let mut found = around(&patch, &written, posted + WINDOW, &[]);
        found.update = update(gid, posted);
        found.after.reviews = reviews;
        found.enough = enough;
        found
    }

    #[test]
    fn the_biggest_updates_are_the_most_reviewed_after_and_four_weeks_apart() {
        let launched = MARCH;
        let arounds = [
            after("launch", launched, 9_000, true),
            after("hotfix", launched + DAY, 8_000, true),
            after("big", launched + WINDOW, 7_000, true),
            after("quiet", launched + 3 * WINDOW, 500, true),
            after("thin", launched + 5 * WINDOW, 9_500, false),
            after("small", launched + 7 * WINDOW, 400, true),
        ];
        let chosen: Vec<&str> = biggest(&arounds, 3)
            .iter()
            .map(|one| one.update.gid.as_str())
            .collect();
        assert_eq!(chosen, ["launch", "big", "quiet"], "oldest first");
        assert_eq!(biggest(&arounds, 0), Vec::<&Around>::new());
        let everything: Vec<&str> = biggest(&arounds, 10)
            .iter()
            .map(|one| one.update.gid.as_str())
            .collect();
        assert_eq!(everything, ["launch", "big", "quiet", "small"]);
    }

    fn months(labels: &[&str]) -> Vec<Month> {
        labels
            .iter()
            .map(|label| Month {
                label: (*label).to_owned(),
                reviews: 1,
                positive: 1,
                subjects: Vec::new(),
                praising: Vec::new(),
                complaining: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn an_update_sits_on_the_timeline_where_it_falls_in_its_month() {
        // 1 March 2024 at midday, 16 March at midday, 29 February at midnight.
        let chart = months(&["2024-02", "2024-03", "2024-05"]);
        assert!((position(MARCH, &chart).unwrap() - (1.0 + 0.5 / 31.0)).abs() < 1e-12);
        let mid = MARCH + 15 * DAY;
        assert!((position(mid, &chart).unwrap() - 1.5).abs() < 1e-12);
        let leap = MARCH - DAY / 2 - DAY;
        assert!((position(leap, &chart).unwrap() - 28.0 / 29.0).abs() < 1e-12);
        let april = MARCH + 40 * DAY;
        assert_eq!(position(april, &chart), Some(2.0), "a month with no column");
        let may = MARCH + 75 * DAY;
        assert!((position(may, &chart).unwrap() - (2.0 + 14.5 / 31.0)).abs() < 1e-12);
        assert_eq!(position(MARCH - 40 * DAY, &chart), None);
        assert_eq!(position(MARCH + 100 * DAY, &chart), None);
        assert_eq!(position(MARCH, &[]), None);
    }

    #[test]
    fn a_month_has_the_days_the_calendar_gives_it() {
        let days: Vec<u8> = (1..=12).map(|month| days_in(2023, month)).collect();
        assert_eq!(days, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]);
        assert_eq!(days_in(2024, 2), 29);
        assert_eq!(days_in(1900, 2), 28);
        assert_eq!(days_in(2000, 2), 29);
    }

    #[test]
    fn a_game_steam_was_never_asked_about_has_no_updates_and_walks_nothing() {
        let out = crate::tempdir::Dir::new();
        let snapshot = crate::read::tests::read_corpus_of(out.path(), 1);
        let reading: ReadReport =
            serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).unwrap()).unwrap();
        let nowhere = out.path().join("no capture here");
        assert_eq!(
            of_game(out.path(), &nowhere, &reading).unwrap(),
            Updates::default()
        );
        let game = out.path().join("appid=1");
        let news = |posts| crate::updates::Announcements { asked: 4, posts };
        news(Vec::new()).save(&game).unwrap();
        assert_eq!(
            of_game(out.path(), &nowhere, &reading).unwrap(),
            Updates {
                asked: Some(4),
                around: Vec::new()
            },
            "asked, and no update named"
        );

        news(vec![crate::updates::Announcement {
            gid: "9".to_owned(),
            title: "Patch 1.1".to_owned(),
            posted: MARCH + 31 * DAY,
            patch_notes: false,
        }])
        .save(&game)
        .unwrap();
        let found = of_game(out.path(), &snapshot, &reading).unwrap().around;
        assert_eq!(found.len(), 1);
        // The capture was last swept on the evening of 1 April, after only one of April's
        // reviews in this corpus could have been written.
        assert_eq!(reading.captured_unix, crate::read::tests::SWEPT);
        assert_eq!((found[0].before.reviews, found[0].after.reviews), (2, 1));
        assert!(!found[0].enough && !found[0].after_whole);
    }
}
