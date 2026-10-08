//! What changed lately in what a game's reviewers say.
//!
//! The last three months of a capture against the twelve before them, from the months a reading
//! counted: the share of reviews recommending the game, and for each subject the share of
//! reviews praising it and the share complaining about it. A change is reported only where the
//! two windows hold enough reviews for a share to mean something and the gap is wider than the
//! reviews behind it can explain, so a quiet game does not lead with noise.

use serde::Serialize;

use crate::{read::ReadReport, taxonomy::SHEET};

/// Months in the recent window, counting the month the capture was made in.
pub const RECENT_MONTHS: i64 = 3;

/// Months in the window the recent one is held against, ending the month before it starts.
pub const BEFORE_MONTHS: i64 = 12;

/// Reviews each window needs before a share of it is compared. Below these a handful of
/// reviews moves a share by whole points, and a game reviewed twice a week would report a
/// change every month.
pub const ENOUGH_RECENT: u64 = 100;
pub const ENOUGH_BEFORE: u64 = 300;

/// How many standard errors a gap has to span: a library of seventy games tests some
/// thousands of shares, and at two of them a few dozen would clear by chance alone.
pub const CLEAR: f64 = 3.0;

/// The smallest change worth a line, in share points. A corpus of a million reviews makes a
/// change of a fifth of a point significant, and nobody acts on a fifth of a point.
pub const WORTH_SAYING: f64 = 0.02;

/// A share in each window, and how far apart they are in standard errors.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Shift {
    pub before: f64,
    pub recent: f64,
    pub before_reviews: u64,
    pub recent_reviews: u64,
    /// Positive where the recent share is the higher.
    pub z: f64,
}

impl Shift {
    fn of(before: (u64, u64), recent: (u64, u64)) -> Option<Self> {
        Self::between(before, recent, (ENOUGH_BEFORE, ENOUGH_RECENT))
    }

    /// Two shares as (reviews saying it, reviews) in each window, compared only where each
    /// window holds the reviews `enough` asks of it, earlier window first.
    #[expect(
        clippy::cast_precision_loss,
        reason = "review counts are far below 2^53"
    )]
    pub(crate) fn between(
        before: (u64, u64),
        recent: (u64, u64),
        enough: (u64, u64),
    ) -> Option<Self> {
        let ((hit_before, of_before), (hit_recent, of_recent)) = (before, recent);
        if of_before < enough.0 || of_recent < enough.1 {
            return None;
        }
        Some(Self {
            before: hit_before as f64 / of_before as f64,
            recent: hit_recent as f64 / of_recent as f64,
            before_reviews: of_before,
            recent_reviews: of_recent,
            z: standard_errors_apart(before, recent),
        })
    }

    /// Wider than chance and big enough to act on.
    #[must_use]
    pub fn clear(&self) -> bool {
        self.z.abs() >= CLEAR && (self.recent - self.before).abs() >= WORTH_SAYING
    }
}

/// How many standard errors of the pooled share separate two shares, each given as the reviews
/// it counts and the reviews it is a share of: positive where the second is the higher, and
/// nothing where neither side holds a review or every review is on the same side.
#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
pub(crate) fn standard_errors_apart(
    (hit_first, of_first): (u64, u64),
    (hit_second, of_second): (u64, u64),
) -> f64 {
    if of_first == 0 || of_second == 0 {
        return 0.0;
    }
    let share_first = hit_first as f64 / of_first as f64;
    let share_second = hit_second as f64 / of_second as f64;
    let pooled = (hit_first + hit_second) as f64 / (of_first + of_second) as f64;
    let spread =
        (pooled * (1.0 - pooled) * (1.0 / of_first as f64 + 1.0 / of_second as f64)).sqrt();
    if spread > 0.0 {
        (share_second - share_first) / spread
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Praise,
    Complaint,
}

/// One subject whose praise or complaints moved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Move {
    pub subject: &'static str,
    pub label: &'static str,
    pub side: Side,
    pub shift: Shift,
}

/// What moved in one game, clearest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Recent {
    /// First and last month of the recent window, as `2024-02`.
    pub from: String,
    pub to: String,
    /// First month of the window it is held against.
    pub since: String,
    /// The share recommending the game, where both windows hold enough reviews.
    pub recommended: Option<Shift>,
    /// Only the clear ones, most standard errors first.
    pub moves: Vec<Move>,
}

/// A month label as a count of months, so windows can be stepped through.
fn index_of(label: &str) -> Option<i64> {
    let (year, month) = label.split_once('-')?;
    let year: i64 = year.parse().ok()?;
    let month: i64 = month.parse().ok()?;
    (1..=12).contains(&month).then_some(year * 12 + month - 1)
}

fn label_of(index: i64) -> String {
    format!(
        "{:04}-{:02}",
        index.div_euclid(12),
        index.rem_euclid(12) + 1
    )
}

/// What moved lately in a game, or nothing where its reading has no months to compare.
///
/// The recent window ends with the month the capture was made in, so a capture a year old
/// says what was moving a year ago rather than reporting an empty quarter as a collapse.
#[must_use]
pub fn recent(report: &ReadReport) -> Option<Recent> {
    let last = if report.captured_unix > 0 {
        index_of(&crate::time::year_month(report.captured_unix))?
    } else {
        index_of(&report.months.last()?.label)?
    };
    let first_recent = last - RECENT_MONTHS + 1;
    let first_before = first_recent - BEFORE_MONTHS;

    let mut reviews = [0_u64; 2];
    let mut positive = [0_u64; 2];
    let mut praising = [vec![0_u64; SHEET.len()], vec![0_u64; SHEET.len()]];
    let mut complaining = [vec![0_u64; SHEET.len()], vec![0_u64; SHEET.len()]];
    let mut sided = true;
    for month in &report.months {
        let Some(at) = index_of(&month.label) else {
            continue;
        };
        let window = if (first_recent..=last).contains(&at) {
            1
        } else if (first_before..first_recent).contains(&at) {
            0
        } else {
            continue;
        };
        reviews[window] += month.reviews;
        positive[window] += month.positive;
        // A month counted before months carried sides holds none, and a window missing some of
        // its months' sides would compare a part against a whole.
        if month.praising.len() != SHEET.len() || month.complaining.len() != SHEET.len() {
            sided = false;
            continue;
        }
        for slot in 0..SHEET.len() {
            praising[window][slot] += month.praising[slot];
            complaining[window][slot] += month.complaining[slot];
        }
    }

    let mut moves = Vec::new();
    if sided {
        for (slot, category) in SHEET.iter().enumerate() {
            for (side, counts) in [(Side::Praise, &praising), (Side::Complaint, &complaining)] {
                let Some(shift) =
                    Shift::of((counts[0][slot], reviews[0]), (counts[1][slot], reviews[1]))
                else {
                    continue;
                };
                if shift.clear() {
                    moves.push(Move {
                        subject: category.id,
                        label: category.label,
                        side,
                        shift,
                    });
                }
            }
        }
    }
    moves.sort_by(|left, right| right.shift.z.abs().total_cmp(&left.shift.z.abs()));

    Some(Recent {
        from: label_of(first_recent),
        to: label_of(last),
        since: label_of(first_before),
        recommended: Shift::of((positive[0], reviews[0]), (positive[1], reviews[1])),
        moves,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::Month;

    fn month(label: &str, reviews: u64, positive: u64, bugs_complaints: u64) -> Month {
        let mut complaining = vec![0; SHEET.len()];
        let bugs = SHEET.iter().position(|c| c.id == "bugs").unwrap();
        complaining[bugs] = bugs_complaints;
        Month {
            label: label.to_owned(),
            reviews,
            positive,
            subjects: complaining.clone(),
            praising: vec![0; SHEET.len()],
            complaining,
        }
    }

    /// 15 March 2024, midday UTC.
    const MID_MARCH_2024: i64 = 1_710_504_000;

    fn reading(months: Vec<Month>) -> ReadReport {
        let mut report: ReadReport = serde_json::from_value(serde_json::json!({
            "app_id": 1, "reviews": 1, "corpus_reviews": 1, "language": null, "claims": 1,
            "unclassified_claims": 0, "silent_reviews": 0, "positive": 0, "top_helpful": 0,
            "model": "m", "threshold": 0.5, "device": "cpu", "context": false,
            "subjects": [], "languages": [], "months": []
        }))
        .unwrap();
        report.months = months;
        report.captured_unix = MID_MARCH_2024;
        report
    }

    #[test]
    fn a_complaint_that_tripled_lately_is_reported_and_a_steady_one_is_not() {
        let mut months: Vec<Month> = (1..=12)
            .map(|m| month(&format!("2023-{m:02}"), 100, 80, 5))
            .collect();
        months.extend((1..=3).map(|m| month(&format!("2024-{m:02}"), 100, 60, 15)));
        let found = recent(&reading(months)).unwrap();
        assert_eq!(
            (found.from.as_str(), found.to.as_str()),
            ("2024-01", "2024-03")
        );
        assert_eq!(found.since, "2023-01");
        let bugs = &found.moves[0];
        assert_eq!((bugs.subject, bugs.side), ("bugs", Side::Complaint));
        assert!((bugs.shift.before - 0.05).abs() < 1e-9);
        assert!((bugs.shift.recent - 0.15).abs() < 1e-9);
        assert!(bugs.shift.z > CLEAR);
        assert_eq!(found.moves.len(), 1, "nothing else moved");
        let recommended = found.recommended.unwrap();
        assert!(recommended.clear() && recommended.z < 0.0);
    }

    #[test]
    fn a_quiet_game_says_nothing_rather_than_reporting_noise() {
        let mut months: Vec<Month> = (1..=12)
            .map(|m| month(&format!("2023-{m:02}"), 10, 8, 1))
            .collect();
        months.extend((1..=3).map(|m| month(&format!("2024-{m:02}"), 10, 2, 9)));
        let found = recent(&reading(months)).unwrap();
        assert_eq!(found.moves, [] as [Move; 0]);
        assert_eq!(found.recommended, None);
    }

    #[test]
    fn months_counted_before_sides_were_kept_give_no_subject_moves() {
        let mut months: Vec<Month> = (1..=12)
            .map(|m| month(&format!("2023-{m:02}"), 100, 80, 5))
            .collect();
        months.extend((1..=3).map(|m| month(&format!("2024-{m:02}"), 100, 60, 15)));
        for one in &mut months {
            one.praising.clear();
            one.complaining.clear();
        }
        let found = recent(&reading(months)).unwrap();
        assert_eq!(found.moves, [] as [Move; 0]);
        assert!(
            found.recommended.is_some(),
            "the recommendation share needs no sides"
        );
    }

    #[test]
    fn a_month_missing_either_side_gives_no_subject_moves() {
        let mut months: Vec<Month> = (1..=12)
            .map(|m| month(&format!("2023-{m:02}"), 100, 80, 5))
            .collect();
        months.extend((1..=3).map(|m| month(&format!("2024-{m:02}"), 100, 60, 15)));
        months[0].complaining.clear();
        assert_eq!(recent(&reading(months)).unwrap().moves, [] as [Move; 0]);
    }

    #[test]
    fn a_share_is_compared_only_once_both_windows_hold_enough_reviews() {
        let compared = |before: u64, recent: u64| Shift::of((0, before), (0, recent)).is_some();
        assert!(compared(ENOUGH_BEFORE, ENOUGH_RECENT));
        assert!(!compared(ENOUGH_BEFORE - 1, ENOUGH_RECENT));
        assert!(!compared(ENOUGH_BEFORE, ENOUGH_RECENT - 1));
        assert!(!compared(ENOUGH_BEFORE - 1, ENOUGH_RECENT + 1_000));
        assert!(!compared(ENOUGH_BEFORE + 1_000, ENOUGH_RECENT - 1));
    }

    #[test]
    fn the_gap_is_measured_in_standard_errors_of_the_pooled_share() {
        // 60 of 400 before and 30 of 100 lately: pooled 90 of 500, so the spread is
        // sqrt(0.18 * 0.82 * (1/400 + 1/100)) and the gap of 0.15 is 3.49 of it.
        let shift = Shift::of((60, 400), (30, 100)).unwrap();
        assert!((shift.before - 0.15).abs() < 1e-12);
        assert!((shift.recent - 0.30).abs() < 1e-12);
        assert_eq!((shift.before_reviews, shift.recent_reviews), (400, 100));
        let spread = (0.18_f64 * 0.82 * (1.0 / 400.0 + 1.0 / 100.0)).sqrt();
        assert!((shift.z - 0.15 / spread).abs() < 1e-9, "z was {}", shift.z);
        assert!(shift.clear());
    }

    #[test]
    fn a_share_of_no_reviews_is_no_distance_from_anything() {
        for (first, second) in [((0, 0), (5, 10)), ((5, 10), (0, 0)), ((0, 0), (0, 0))] {
            let apart = standard_errors_apart(first, second);
            assert!(
                apart.abs() < f64::EPSILON,
                "{first:?} {second:?} gave {apart}"
            );
        }
    }

    #[test]
    fn a_share_that_never_left_nothing_has_not_moved() {
        let shift = Shift::of((0, 400), (0, 100)).unwrap();
        assert!(shift.z.abs() < 1e-12, "z was {}", shift.z);
        assert!(!shift.clear());
    }

    #[test]
    fn a_change_is_clear_only_when_it_is_both_beyond_chance_and_worth_saying() {
        let shift = |before: f64, recent: f64, z: f64| Shift {
            before,
            recent,
            before_reviews: 1_000_000,
            recent_reviews: 1_000_000,
            z,
        };
        assert!(shift(0.40, 0.45, CLEAR).clear());
        assert!(shift(0.45, 0.40, -CLEAR).clear());
        assert!(
            !shift(0.400, 0.405, 12.0).clear(),
            "half a point is significant in a million reviews and still not worth a line"
        );
        assert!(!shift(0.40, 0.60, CLEAR - 0.1).clear());
    }

    #[test]
    fn praise_is_counted_as_well_as_complaint() {
        let mut months: Vec<Month> = (1..=12)
            .map(|m| month(&format!("2023-{m:02}"), 100, 80, 5))
            .collect();
        months.extend((1..=3).map(|m| month(&format!("2024-{m:02}"), 100, 60, 15)));
        for one in &mut months {
            one.praising.clone_from(&one.complaining);
        }
        let found = recent(&reading(months)).unwrap();
        assert!(
            found
                .moves
                .iter()
                .any(|moved| (moved.subject, moved.side) == ("bugs", Side::Praise)),
            "{found:?}"
        );
    }

    #[test]
    fn the_recent_window_ends_where_the_capture_was_made_or_else_at_the_last_month() {
        let months: Vec<Month> = (1..=12)
            .map(|m| month(&format!("2023-{m:02}"), 100, 80, 5))
            .chain((1..=3).map(|m| month(&format!("2024-{m:02}"), 100, 60, 15)))
            .collect();
        let mut a_year_on = reading(months.clone());
        a_year_on.captured_unix = MID_MARCH_2024 + 365 * 86_400;
        let found = recent(&a_year_on).unwrap();
        assert_eq!(
            (found.from.as_str(), found.to.as_str()),
            ("2025-01", "2025-03")
        );
        assert!(
            found.moves.is_empty(),
            "a quarter with no reviews in it moved nothing"
        );

        let mut undated = reading(months);
        undated.captured_unix = 0;
        assert_eq!(recent(&undated).unwrap().to, "2024-03");
    }

    #[test]
    fn month_labels_step_across_a_year() {
        let january = index_of("2024-01").unwrap();
        assert_eq!(label_of(january - 1), "2023-12");
        assert_eq!(label_of(january + 11), "2024-12");
        assert_eq!(index_of("2024-13"), None);
    }
}
