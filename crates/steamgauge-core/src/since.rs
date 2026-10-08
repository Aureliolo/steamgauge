//! What moved in a game since a person last looked at it.
//!
//! When a person looks at a game, what the library held of it and what its reading counted are
//! kept. On a later look, the reviews read in between are the difference between the reading then
//! and the reading now. That holds where both were made by the same reader under the same lines,
//! in the same language and at the same depth, because the reviews they share are answered alike
//! again. The share of those new reviews recommending the game, praising each subject and
//! complaining about it is held against the twelve months of reviews before the capture that was
//! seen, with what moved's floors and two-proportion test.
//!
//! The new reviews are a later kind of reviewer, and they are held to who wrote it's bar rather
//! than what moved's: a person looks again and again, and every look at every game tests some fifty
//! shares. At three standard errors a library of ten games each gathering a few hundred reviews
//! would show about one change chance made on every look; at four, on about one look in thirty.

use std::{collections::BTreeMap, io::Write, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    Result,
    moves::{self, Move, Shift, Side},
    read::{Depth, ReadReport},
    taxonomy::SHEET,
};

/// Standard errors a change in the new reviews has to span.
pub const CLEAR: f64 = 4.0;

/// The file in the library the looks are kept in.
pub const FILE: &str = "last-seen.json";

/// The subject that says nothing about the game, which nobody acts on a change in.
const SAYS_NOTHING: &str = "offtopic";

/// What a reading counted, kept as a person saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counted {
    pub read_with: String,
    pub read_by_rule: String,
    pub language: Option<String>,
    pub depth: Depth,
    /// When the capture the reading counted was last changed.
    pub captured: i64,
    pub reviews: u64,
    pub positive: u64,
    /// Reviews praising each subject, by its id, whether or not they also complain about it.
    pub praising: BTreeMap<String, u64>,
    /// Reviews complaining about each subject, by its id, whether or not they also praise it.
    pub complaining: BTreeMap<String, u64>,
}

impl Counted {
    #[must_use]
    pub fn of(report: &ReadReport) -> Self {
        Self {
            read_with: report.read_with.clone(),
            read_by_rule: report.read_by_rule.clone(),
            language: report.language.clone(),
            depth: report.depth,
            captured: report.captured_unix,
            reviews: report.reviews,
            positive: report.positive,
            praising: report
                .subjects
                .iter()
                .map(|subject| (subject.id.clone(), subject.praised + subject.mixed))
                .collect(),
            complaining: report
                .subjects
                .iter()
                .map(|subject| (subject.id.clone(), subject.criticised + subject.mixed))
                .collect(),
        }
    }

    /// Whether `other` asked the reviews the same question, so the ones both counted were
    /// answered alike and one count can be taken from the other.
    fn alike(&self, other: &Self) -> bool {
        self.read_with == other.read_with
            && self.read_by_rule == other.read_by_rule
            && self.language == other.language
            && self.depth == other.depth
    }
}

/// A game as a person last saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub at: i64,
    /// Reviews the library held of it, in every language.
    pub held: u64,
    pub reading: Option<Counted>,
}

impl Seen {
    #[must_use]
    pub fn of(at: i64, held: u64, reading: Option<&ReadReport>) -> Self {
        Self {
            at,
            held,
            reading: reading.map(Counted::of),
        }
    }
}

/// How far what changed since a look could be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// Nothing was added or read since.
    Nothing,
    /// Reviews were added and are not read yet.
    Unread,
    /// Read for the first time since, so nothing earlier stands beside it.
    FirstRead,
    /// Read again by another reader, under other lines, in another language or at another depth,
    /// so the earlier count cannot be taken from the new one.
    Reread,
    /// Fewer new reviews were read than a share of them needs.
    Few,
    /// The twelve months before the capture that was seen hold too few reviews to compare with.
    Unanchored,
    Compared,
}

/// What moved in one game since a look.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Since {
    pub looked: i64,
    /// Reviews the library holds now that it did not then, in every language.
    pub new: u64,
    /// Reviews counted now that were not then, in the language the game is read in, where the two
    /// readings asked the same question.
    pub read_new: u64,
    pub standing: Standing,
    /// First and last month of the year the new reviews are held against, as `2024-02`.
    pub from: Option<String>,
    pub to: Option<String>,
    /// The share recommending the game, where it moved by more than chance.
    pub recommended: Option<Shift>,
    /// The subjects that moved by more than chance, clearest first.
    pub moves: Vec<Move>,
}

impl Since {
    /// Whether anything moved by more than chance.
    #[must_use]
    pub fn moved(&self) -> bool {
        self.recommended.is_some() || !self.moves.is_empty()
    }
}

/// Wider than this module's bar and big enough to act on.
#[must_use]
pub fn clear(shift: &Shift) -> bool {
    shift.z.abs() >= CLEAR && (shift.recent - shift.before).abs() >= moves::WORTH_SAYING
}

/// What moved in a game since `seen`, given what the library holds of it now and its reading.
#[must_use]
pub fn since(seen: &Seen, held: u64, now: Option<&ReadReport>) -> Since {
    let mut found = Since {
        looked: seen.at,
        new: held.saturating_sub(seen.held),
        read_new: 0,
        standing: Standing::Nothing,
        from: None,
        to: None,
        recommended: None,
        moves: Vec::new(),
    };
    let added = if found.new > 0 {
        Standing::Unread
    } else {
        Standing::Nothing
    };
    found.standing = match (seen.reading.as_ref(), now) {
        (_, None) => added,
        (None, Some(_)) => Standing::FirstRead,
        (Some(then), Some(report)) => {
            let counted = Counted::of(report);
            if !then.alike(&counted) {
                Standing::Reread
            } else if counted.captured == then.captured {
                added
            } else {
                found.read_new = counted.reviews.saturating_sub(then.reviews);
                compare(&mut found, then, &counted, report)
            }
        }
    };
    found
}

#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn ratio(part: u64, whole: u64) -> f64 {
    part as f64 / whole as f64
}

/// How many more reviews `now` counts under a subject than `then` did. An edited review can take
/// one away, and a count that fell has gained nothing.
fn gained(now: &BTreeMap<String, u64>, then: &BTreeMap<String, u64>, id: &str) -> u64 {
    now.get(id)
        .copied()
        .unwrap_or(0)
        .saturating_sub(then.get(id).copied().unwrap_or(0))
}

/// The new reviews against the twelve months before the capture that was seen.
fn compare(found: &mut Since, then: &Counted, now: &Counted, report: &ReadReport) -> Standing {
    let new = found.read_new;
    if new < moves::ENOUGH_RECENT {
        return Standing::Few;
    }
    // The month the seen capture ends in is left out of both sides: its earlier reviews were
    // already counted, and its later ones are among the new.
    let Some(seen_in) = moves::index_of(&crate::time::year_month(then.captured)) else {
        return Standing::Unanchored;
    };
    let first = seen_in - moves::BEFORE_MONTHS;

    let mut reviews = 0;
    let mut positive = 0;
    let mut praising = vec![0_u64; SHEET.len()];
    let mut complaining = vec![0_u64; SHEET.len()];
    let mut sided = true;
    for month in &report.months {
        let Some(at) = moves::index_of(&month.label) else {
            continue;
        };
        if !(first..seen_in).contains(&at) {
            continue;
        }
        reviews += month.reviews;
        positive += month.positive;
        if month.praising.len() != SHEET.len() || month.complaining.len() != SHEET.len() {
            sided = false;
            continue;
        }
        for slot in 0..SHEET.len() {
            praising[slot] += month.praising[slot];
            complaining[slot] += month.complaining[slot];
        }
    }
    if reviews < moves::ENOUGH_BEFORE {
        return Standing::Unanchored;
    }
    found.from = Some(moves::label_of(first));
    found.to = Some(moves::label_of(seen_in - 1));

    let shift = |before: u64, after: u64| {
        let after = after.min(new);
        Shift {
            before: ratio(before, reviews),
            recent: ratio(after, new),
            before_reviews: reviews,
            recent_reviews: new,
            z: moves::standard_errors_apart((before, reviews), (after, new)),
        }
    };
    found.recommended =
        Some(shift(positive, now.positive.saturating_sub(then.positive))).filter(clear);
    if sided {
        for (slot, category) in SHEET.iter().enumerate() {
            if category.id == SAYS_NOTHING {
                continue;
            }
            for (side, before, counted_now, counted_then) in [
                (Side::Praise, &praising, &now.praising, &then.praising),
                (
                    Side::Complaint,
                    &complaining,
                    &now.complaining,
                    &then.complaining,
                ),
            ] {
                let moved = shift(before[slot], gained(counted_now, counted_then, category.id));
                if clear(&moved) {
                    found.moves.push(Move {
                        subject: category.id,
                        label: category.label,
                        side,
                        shift: moved,
                    });
                }
            }
        }
    }
    found
        .moves
        .sort_by(|left, right| right.shift.z.abs().total_cmp(&left.shift.z.abs()));
    Standing::Compared
}

/// The library looked at as a whole, and every game as it stood then.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Look {
    pub at: i64,
    pub games: BTreeMap<u32, Seen>,
}

/// When a person last looked at the cockpit and at each game's page, and what they saw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LastSeen {
    pub cockpit: Option<Look>,
    pub pages: BTreeMap<u32, Seen>,
    /// What a notification has already named for a game since the person last looked at it.
    pub told: BTreeMap<u32, Vec<String>>,
}

impl LastSeen {
    /// The looks kept in `dir`, or none where there is no file or it cannot be read: a lost
    /// record costs one visit's comparison, and the next look starts it again.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Written whole beside its place, flushed to the disk, and moved over it, so a crash at any
    /// point leaves either the record before or the record after.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written or moved into place.
    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join(FILE);
        let partial = dir.join(format!("{FILE}.partial"));
        {
            let mut file = std::fs::File::create(&partial)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
        }
        std::fs::rename(&partial, &path)?;
        Ok(())
    }

    /// The cockpit seen, with every game as it stands. Whatever a notification named has now
    /// been shown.
    pub fn cockpit_seen(&mut self, look: Look) {
        self.cockpit = Some(look);
        self.told.clear();
    }

    /// One game's page seen.
    pub fn page_seen(&mut self, app_id: u32, seen: Seen) {
        self.pages.insert(app_id, seen);
        self.told.remove(&app_id);
    }

    /// What a game's page last showed, or where the page was never opened, what the cockpit
    /// last showed of it.
    #[must_use]
    pub fn page(&self, app_id: u32) -> Option<&Seen> {
        self.pages
            .get(&app_id)
            .or_else(|| self.cockpit.as_ref()?.games.get(&app_id))
    }

    /// The most recent look at a game, on its page or on the cockpit.
    #[must_use]
    pub fn latest(&self, app_id: u32) -> Option<&Seen> {
        let page = self.pages.get(&app_id);
        let cockpit = self
            .cockpit
            .as_ref()
            .and_then(|look| look.games.get(&app_id));
        match (page, cockpit) {
            (Some(page), Some(cockpit)) if cockpit.at > page.at => Some(cockpit),
            (Some(page), _) => Some(page),
            (None, cockpit) => cockpit,
        }
    }

    /// Notes that a notification named `about` for a game, and whether it had not already.
    pub fn tell(&mut self, app_id: u32, about: &str) -> bool {
        let told = self.told.entry(app_id).or_default();
        if told.iter().any(|said| said == about) {
            return false;
        }
        told.push(about.to_owned());
        true
    }

    /// A game removed from the library leaves no look behind.
    pub fn forget(&mut self, app_id: u32) {
        self.pages.remove(&app_id);
        self.told.remove(&app_id);
        if let Some(look) = self.cockpit.as_mut() {
            look.games.remove(&app_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::Month;

    /// 15 March 2024, midday UTC.
    const MID_MARCH_2024: i64 = 1_710_504_000;
    /// 15 May 2024, midday UTC.
    const MID_MAY_2024: i64 = 1_715_774_400;

    fn slot(id: &str) -> usize {
        SHEET.iter().position(|category| category.id == id).unwrap()
    }

    fn month(label: &str, reviews: u64, positive: u64, bugs_complaints: u64) -> Month {
        let mut complaining = vec![0; SHEET.len()];
        complaining[slot("bugs")] = bugs_complaints;
        Month {
            label: label.to_owned(),
            reviews,
            positive,
            subjects: complaining.clone(),
            praising: vec![0; SHEET.len()],
            complaining,
        }
    }

    fn subject(id: &str, praised: u64, criticised: u64, mixed: u64) -> serde_json::Value {
        serde_json::json!({
            "id": id, "label": id, "mention_reviews": praised + criticised + mixed,
            "primary_reviews": 0, "claims": 0, "praised": praised, "criticised": criticised,
            "mixed": mixed, "top_mention_reviews": 0, "positive_mentions": 0
        })
    }

    /// A reading of `reviews` reviews, `positive` recommending, with bugs complained about in
    /// `bugs` of them, captured at `captured`, over a year of 1,200 reviews before March 2024.
    fn reading(captured: i64, reviews: u64, positive: u64, bugs: u64) -> ReadReport {
        let mut report: ReadReport = serde_json::from_value(serde_json::json!({
            "app_id": 1, "reviews": reviews, "corpus_reviews": reviews, "language": "english",
            "claims": 1, "unclassified_claims": 0, "silent_reviews": 0, "positive": positive,
            "top_helpful": 0, "model": "m", "threshold": 0.5, "device": "cpu", "context": false,
            "read_with": "run-a", "read_by_rule": "lines-a",
            "subjects": [subject("bugs", 0, bugs, 0), subject("story", 50, 0, 0)],
            "languages": [], "months": []
        }))
        .unwrap();
        report.captured_unix = captured;
        report.months = (3..=12)
            .map(|m| month(&format!("2023-{m:02}"), 100, 80, 5))
            .chain((1..=2).map(|m| month(&format!("2024-{m:02}"), 100, 80, 5)))
            .collect();
        report
    }

    fn seen_at_march() -> Seen {
        Seen::of(100, 1_500, Some(&reading(MID_MARCH_2024, 1_250, 1_000, 60)))
    }

    #[test]
    fn a_reading_is_kept_with_praise_and_complaint_each_counting_the_mixed() {
        let counted = Counted::of(&reading(MID_MARCH_2024, 1_250, 1_000, 60));
        let report: ReadReport = {
            let mut report = reading(MID_MARCH_2024, 1_250, 1_000, 0);
            report.subjects =
                serde_json::from_value(serde_json::json!([subject("bugs", 3, 7, 2)])).unwrap();
            report
        };
        let mixed = Counted::of(&report);
        assert_eq!(mixed.praising["bugs"], 5);
        assert_eq!(mixed.complaining["bugs"], 9);
        assert_eq!(counted.complaining["bugs"], 60);
        assert_eq!(counted.praising["story"], 50);
        assert_eq!(
            (counted.reviews, counted.positive, counted.captured),
            (1_250, 1_000, MID_MARCH_2024)
        );
        assert_eq!(counted.language.as_deref(), Some("english"));
        assert_eq!(
            (counted.read_with.as_str(), counted.read_by_rule.as_str()),
            ("run-a", "lines-a")
        );
    }

    #[test]
    fn nothing_added_and_nothing_read_is_nothing() {
        let found = since(
            &seen_at_march(),
            1_500,
            Some(&reading(MID_MARCH_2024, 1_250, 1_000, 60)),
        );
        assert_eq!(found.standing, Standing::Nothing);
        assert_eq!((found.looked, found.new, found.read_new), (100, 0, 0));
        assert!(!found.moved());
    }

    #[test]
    fn reviews_added_and_not_read_are_counted_and_not_compared() {
        let found = since(
            &seen_at_march(),
            1_900,
            Some(&reading(MID_MARCH_2024, 1_250, 1_000, 60)),
        );
        assert_eq!(found.standing, Standing::Unread);
        assert_eq!(found.new, 400);
        let unread = Seen::of(5, 300, None);
        assert_eq!(since(&unread, 450, None).standing, Standing::Unread);
        assert_eq!(since(&unread, 450, None).new, 150);
        assert_eq!(since(&unread, 300, None).standing, Standing::Nothing);
        assert_eq!(
            since(&unread, 200, None).new,
            0,
            "a library holding fewer reviews than it did gained none"
        );
    }

    #[test]
    fn a_game_read_since_for_the_first_time_has_nothing_to_compare_with() {
        let found = since(
            &Seen::of(5, 1_500, None),
            1_500,
            Some(&reading(MID_MAY_2024, 1_250, 1_000, 60)),
        );
        assert_eq!(found.standing, Standing::FirstRead);
    }

    #[test]
    fn a_reading_that_asked_another_question_is_not_taken_from() {
        let seen = seen_at_march();
        let changes: [fn(&mut ReadReport); 4] = [
            |report| report.read_with = "run-b".to_owned(),
            |report| report.read_by_rule = "lines-b".to_owned(),
            |report| report.language = None,
            |report| report.depth = Depth::Shallow,
        ];
        for change in changes {
            let mut now = reading(MID_MAY_2024, 1_650, 1_100, 260);
            change(&mut now);
            let found = since(&seen, 1_900, Some(&now));
            assert_eq!(found.standing, Standing::Reread);
            assert_eq!(found.read_new, 0);
            assert!(!found.moved());
        }
    }

    #[test]
    fn too_few_new_reviews_read_are_not_compared() {
        let found = since(
            &seen_at_march(),
            1_600,
            Some(&reading(MID_MAY_2024, 1_349, 1_000, 160)),
        );
        assert_eq!(found.standing, Standing::Few);
        assert_eq!(found.read_new, 99);
        let enough = since(
            &seen_at_march(),
            1_600,
            Some(&reading(MID_MAY_2024, 1_350, 1_080, 65)),
        );
        assert_eq!(enough.standing, Standing::Compared, "a hundred is enough");
    }

    #[test]
    fn complaints_that_quadrupled_in_the_new_reviews_moved_and_a_steady_share_did_not() {
        // 400 new reviews, 300 recommending (75% against the year's 80%: under four standard
        // errors), and 100 complaining about bugs (25% against 5%).
        let found = since(
            &seen_at_march(),
            1_900,
            Some(&reading(MID_MAY_2024, 1_650, 1_300, 160)),
        );
        assert_eq!(found.standing, Standing::Compared);
        assert_eq!(found.read_new, 400);
        assert_eq!(
            (found.from.as_deref(), found.to.as_deref()),
            (Some("2023-03"), Some("2024-02"))
        );
        assert_eq!(found.recommended, None);
        assert_eq!(found.moves.len(), 1);
        let moved = &found.moves[0];
        assert_eq!((moved.subject, moved.side), ("bugs", Side::Complaint));
        assert_eq!(moved.shift.before_reviews, 1_200);
        assert_eq!(moved.shift.recent_reviews, 400);
        assert!((moved.shift.before - 0.05).abs() < 1e-12);
        assert!((moved.shift.recent - 0.25).abs() < 1e-12);
        assert!(moved.shift.z > CLEAR);
        assert!(found.moved());
    }

    #[test]
    fn a_share_recommending_that_fell_in_the_new_reviews_moved() {
        // 200 of 400 new reviews recommend the game, against 960 of 1,200 before.
        let found = since(
            &seen_at_march(),
            1_900,
            Some(&reading(MID_MAY_2024, 1_650, 1_200, 80)),
        );
        let shift = found.recommended.expect("the share recommending moved");
        assert!((shift.before - 0.8).abs() < 1e-12);
        assert!((shift.recent - 0.5).abs() < 1e-12);
        assert!(shift.z < -CLEAR);
        assert!(found.moves.is_empty(), "bugs held at 5%");
        assert!(found.moved());
    }

    #[test]
    fn only_the_twelve_months_before_the_seen_capture_are_the_year_before() {
        let mut now = reading(MID_MAY_2024, 1_650, 1_300, 160);
        // A month before the year, the month the seen capture ends in and the months after are
        // none of them the year before, however loud.
        now.months.push(month("2023-02", 5_000, 0, 5_000));
        now.months.push(month("2024-03", 5_000, 0, 5_000));
        now.months.push(month("2024-04", 5_000, 0, 5_000));
        now.months.push(month("not-a-month", 5_000, 0, 5_000));
        let found = since(&seen_at_march(), 1_900, Some(&now));
        assert_eq!(found.moves[0].shift.before_reviews, 1_200);
        assert!((found.moves[0].shift.before - 0.05).abs() < 1e-12);
    }

    #[test]
    fn a_year_before_with_too_few_reviews_anchors_nothing() {
        let mut now = reading(MID_MAY_2024, 1_650, 1_300, 160);
        now.months.truncate(2);
        now.months.push(month("2024-02", 99, 80, 5));
        let found = since(&seen_at_march(), 1_900, Some(&now));
        assert_eq!(found.standing, Standing::Unanchored);
        assert_eq!((found.from.as_deref(), found.moves.len()), (None, 0));
        now.months.push(month("2024-01", 1, 1, 0));
        let enough = since(&seen_at_march(), 1_900, Some(&now));
        assert_eq!(
            enough.standing,
            Standing::Compared,
            "three hundred is enough"
        );
    }

    #[test]
    fn months_without_sides_compare_the_share_recommending_and_no_subject() {
        let mut now = reading(MID_MAY_2024, 1_650, 1_200, 160);
        now.months[0].praising.clear();
        let found = since(&seen_at_march(), 1_900, Some(&now));
        assert!(found.recommended.is_some());
        assert_eq!(found.moves.len(), 0);
        assert_eq!(found.recommended.unwrap().before_reviews, 1_200);
    }

    #[test]
    fn a_change_in_what_says_nothing_about_the_game_is_not_reported() {
        let mut seen = seen_at_march();
        let mut now = reading(MID_MAY_2024, 1_650, 1_300, 60);
        now.subjects = serde_json::from_value(serde_json::json!([
            subject(SAYS_NOTHING, 0, 300, 0),
            subject("bugs", 0, 80, 0)
        ]))
        .unwrap();
        seen.reading
            .as_mut()
            .unwrap()
            .complaining
            .insert(SAYS_NOTHING.to_owned(), 0);
        let found = since(&seen, 1_900, Some(&now));
        assert_eq!(found.standing, Standing::Compared);
        assert_eq!(found.moves.len(), 0);
    }

    #[test]
    fn praise_is_compared_as_well_as_complaint_and_clearest_comes_first() {
        let mut now = reading(MID_MAY_2024, 1_650, 1_300, 120);
        // Story praised by 200 of the 400 new reviews, against 10% of the year before; bugs
        // complained about by 60 of them, against 5%.
        now.subjects = serde_json::from_value(serde_json::json!([
            subject("bugs", 0, 120, 0),
            subject("story", 250, 0, 0)
        ]))
        .unwrap();
        for month in &mut now.months {
            month.praising[slot("story")] = 10;
        }
        let found = since(&seen_at_march(), 1_900, Some(&now));
        assert_eq!(found.moves.len(), 2);
        assert_eq!(
            (found.moves[0].subject, found.moves[0].side),
            ("story", Side::Praise)
        );
        assert!((found.moves[0].shift.before - 0.10).abs() < 1e-12);
        assert!((found.moves[0].shift.recent - 0.50).abs() < 1e-12);
        assert_eq!(found.moves[1].subject, "bugs");
        assert!(found.moves[0].shift.z.abs() > found.moves[1].shift.z.abs());
    }

    #[test]
    fn a_count_that_fell_or_ran_past_the_new_reviews_stays_a_share() {
        let mut now = reading(MID_MAY_2024, 1_650, 2_000, 0);
        now.subjects =
            serde_json::from_value(serde_json::json!([subject("bugs", 0, 0, 0)])).unwrap();
        let found = since(&seen_at_march(), 1_900, Some(&now));
        let shift = found
            .recommended
            .expect("all of the new reviews recommend it");
        assert!(
            (shift.recent - 1.0).abs() < 1e-12,
            "a gain past the new reviews is all of them"
        );
        // Bugs fell from 60 to none, which gained nothing: none of 400 new reviews against 5%.
        assert_eq!(found.moves.len(), 1);
        assert!(found.moves[0].shift.recent.abs() < 1e-12);
        let none = BTreeMap::new();
        let some = BTreeMap::from([("bugs".to_owned(), 4)]);
        assert_eq!(gained(&some, &none, "bugs"), 4);
        assert_eq!(gained(&none, &some, "bugs"), 0);
    }

    #[test]
    fn a_change_is_clear_at_four_standard_errors_and_two_points() {
        let shift = |before, recent, z| Shift {
            before,
            recent,
            before_reviews: 1_000,
            recent_reviews: 100,
            z,
        };
        assert!(clear(&shift(0.0, 0.02, 4.0)));
        assert!(clear(&shift(0.02, 0.0, -4.0)));
        assert!(!clear(&shift(0.0, 0.02, 3.99)));
        assert!(!clear(&shift(0.0, 0.019, 9.0)));
        assert!(!clear(&shift(0.019, 0.0, -9.0)));
        assert!(
            !clear(&shift(0.30, 0.31, 9.0)),
            "a point between two large shares is a point"
        );
    }

    #[test]
    fn the_looks_are_written_whole_and_read_back() {
        let dir = crate::tempdir::Dir::new();
        assert_eq!(LastSeen::load(dir.path()), LastSeen::default());
        let mut looks = LastSeen::default();
        looks.page_seen(7, seen_at_march());
        looks.cockpit_seen(Look {
            at: 200,
            games: BTreeMap::from([(7, Seen::of(200, 10, None))]),
        });
        looks.save(dir.path()).unwrap();
        assert_eq!(LastSeen::load(dir.path()), looks);
        assert!(!dir.path().join(format!("{FILE}.partial")).exists());
    }

    #[test]
    fn a_write_cut_short_leaves_the_record_before_it() {
        let dir = crate::tempdir::Dir::new();
        let mut looks = LastSeen::default();
        looks.page_seen(7, seen_at_march());
        looks.save(dir.path()).unwrap();
        // What a crash part way through the next write leaves beside the record.
        std::fs::write(
            dir.path().join(format!("{FILE}.partial")),
            b"{\"pages\": {\"7\"",
        )
        .unwrap();
        assert_eq!(LastSeen::load(dir.path()), looks);
        looks.page_seen(8, Seen::of(1, 2, None));
        looks.save(dir.path()).unwrap();
        assert_eq!(
            LastSeen::load(dir.path()),
            looks,
            "the next write goes over the remains"
        );
        std::fs::write(dir.path().join(FILE), b"not json").unwrap();
        assert_eq!(
            LastSeen::load(dir.path()),
            LastSeen::default(),
            "a record nobody can read is a first visit, not a failure"
        );
    }

    #[test]
    fn a_record_that_cannot_be_written_says_so() {
        let dir = crate::tempdir::Dir::new();
        assert!(
            LastSeen::default()
                .save(&dir.path().join("missing"))
                .is_err()
        );
    }

    #[test]
    fn a_page_never_opened_falls_back_to_the_cockpit_and_the_latest_look_wins() {
        let mut looks = LastSeen::default();
        assert_eq!(looks.page(7), None);
        assert_eq!(looks.latest(7), None);
        looks.cockpit_seen(Look {
            at: 300,
            games: BTreeMap::from([(7, Seen::of(300, 30, None))]),
        });
        assert_eq!(looks.page(7).map(|seen| seen.at), Some(300));
        assert_eq!(looks.latest(7).map(|seen| seen.at), Some(300));
        looks.page_seen(7, Seen::of(200, 20, None));
        assert_eq!(looks.page(7).map(|seen| seen.at), Some(200));
        assert_eq!(looks.latest(7).map(|seen| seen.at), Some(300));
        looks.page_seen(7, Seen::of(300, 31, None));
        assert_eq!(
            looks.latest(7).map(|seen| seen.held),
            Some(31),
            "a page seen at the same moment as the cockpit is the page"
        );
        looks.page_seen(7, Seen::of(400, 40, None));
        assert_eq!(looks.latest(7).map(|seen| seen.at), Some(400));
        looks.page_seen(8, Seen::of(300, 30, None));
        assert_eq!(
            looks.latest(8).map(|seen| seen.held),
            Some(30),
            "a page and no cockpit look"
        );
    }

    #[test]
    fn a_notification_names_a_move_once_until_the_game_is_looked_at() {
        let mut looks = LastSeen::default();
        assert!(looks.tell(7, "bugs:complaint"));
        assert!(!looks.tell(7, "bugs:complaint"));
        assert!(looks.tell(7, "story:praise"));
        assert!(looks.tell(8, "bugs:complaint"));
        looks.page_seen(7, Seen::of(1, 1, None));
        assert!(looks.tell(7, "bugs:complaint"));
        assert!(
            !looks.tell(8, "bugs:complaint"),
            "another game's page was not seen"
        );
        looks.cockpit_seen(Look::default());
        assert!(looks.tell(8, "bugs:complaint"));
    }

    #[test]
    fn a_game_removed_leaves_no_look_behind() {
        let mut looks = LastSeen::default();
        looks.cockpit_seen(Look {
            at: 1,
            games: BTreeMap::from([(7, Seen::of(1, 1, None)), (8, Seen::of(1, 1, None))]),
        });
        looks.page_seen(7, Seen::of(2, 2, None));
        looks.tell(7, "bugs:complaint");
        looks.forget(7);
        assert_eq!(looks.latest(7), None);
        assert!(looks.told.is_empty());
        assert!(looks.latest(8).is_some());
        LastSeen::default().forget(7);
    }
}
