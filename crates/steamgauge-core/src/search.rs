//! What a game's reviewers say about anything somebody types, by the words they used.
//!
//! The subjects on the sheet are the questions the reader was trained to answer, and the terms
//! that stand out are the ones the counts surfaced on their own. Neither answers "what do they
//! say about the Steam Deck", which is the first thing a developer asks and is on no sheet.
//! This does, over the claims a reading already cut, so a match is one point a reviewer made
//! and is counted the way every other figure on the page is: once per review, split by what
//! the reader said about it.
//!
//! It matches words, not meaning. "Steam Deck" does not find "the Deck" or a review in
//! Russian, and every figure it gives is a count of the words typed and of nothing else; the
//! forms that matched are returned beside it so nobody has to guess which those were.

use std::{collections::HashMap, path::Path, sync::Arc};

use crate::{Result, claims::Span, reader::Polarity, taxonomy::SHEET};

/// Words at least this long also match the words they begin: "stutter" finds "stuttering"
/// and "stutters". A shorter one would find too much it does not mean, as "art" would
/// "artist" and "run" would "runes", so it matches only itself.
const PREFIX_FROM: usize = 4;

/// How many of the matched word forms are returned, commonest first.
const FORMS_KEPT: usize = 12;

/// What somebody typed, ready to be looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrase {
    /// Lowercased, in order. Empty for a phrase in a script written without spaces.
    words: Vec<String>,
    /// The phrase as one lowercased run, for scripts with no spaces to find words by.
    run: Option<String>,
}

impl Phrase {
    /// `None` for a query with nothing in it to look for.
    #[must_use]
    pub fn new(query: &str) -> Option<Self> {
        let lowered = query.trim().to_lowercase();
        if lowered.chars().any(spaceless) {
            let run: String = lowered.chars().filter(|c| !c.is_whitespace()).collect();
            return (!run.is_empty()).then_some(Self {
                words: Vec::new(),
                run: Some(run),
            });
        }
        let words: Vec<String> = words_of(&lowered).map(str::to_owned).collect();
        (!words.is_empty()).then_some(Self { words, run: None })
    }

    /// Whether `text` says the phrase, handing each form it was said in to `form`.
    pub fn found_in(&self, text: &str, mut form: impl FnMut(&str)) -> bool {
        let lowered = text.to_lowercase();
        if let Some(run) = &self.run {
            let found = lowered.contains(run.as_str());
            if found {
                form(run);
            }
            return found;
        }
        if !lowered.contains(self.words[0].as_str()) {
            return false;
        }
        let words: Vec<&str> = words_of(&lowered).collect();
        let mut found = false;
        for start in 0..words.len().saturating_sub(self.words.len() - 1) {
            let window = &words[start..start + self.words.len()];
            if window
                .iter()
                .zip(&self.words)
                .all(|(word, wanted)| matches(word, wanted))
            {
                found = true;
                form(&window.join(" "));
            }
        }
        found
    }
}

fn words_of(text: &str) -> impl Iterator<Item = &str> {
    // A spaceless script ends a word too: "steam deck不能玩" is two words and a sentence in
    // Chinese, and to a test of letters alone it is "steam" and "deck不能玩".
    text.split(|c: char| !c.is_alphanumeric() || spaceless(c))
        .filter(|word| !word.is_empty())
}

fn matches(word: &str, wanted: &str) -> bool {
    word == wanted || (wanted.chars().count() >= PREFIX_FROM && word.starts_with(wanted))
}

/// Scripts written without spaces between words, where a phrase can only be looked for as a
/// run of characters.
fn spaceless(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30FF}'   // hiragana and katakana
        | '\u{3400}'..='\u{4DBF}' // CJK extension A
        | '\u{4E00}'..='\u{9FFF}' // CJK unified ideographs
        | '\u{F900}'..='\u{FAFF}' // CJK compatibility ideographs
        | '\u{0E00}'..='\u{0E7F}' // Thai
    )
}

/// One claim that says the phrase.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Shared by every hit in one review, since a common word is said many times in a long one.
    pub review_id: Arc<str>,
    pub at: Span,
    pub polarity: &'static str,
    /// The subject the reader put first, or [`DECLINED`].
    pub subject: &'static str,
    pub confidence: f32,
}

/// What a game's reviewers said in the words searched for.
///
/// Every hit is kept, most helpful review first, because a search is one walk of the whole
/// capture and a page turned or a side chosen should not be another.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Said {
    /// Reviews with at least one claim that says it.
    pub reviews: u64,
    pub claims: u64,
    pub praise: u64,
    pub complaint: u64,
    pub neutral: u64,
    /// Claims per subject, commonest first, by the subject the reader put first, and
    /// [`DECLINED`] for the claims it would not put a subject on.
    pub subjects: Vec<(&'static str, u64)>,
    /// The forms the phrase was found in, commonest first, and how many claims used each.
    pub forms: Vec<(String, u64)>,
    /// Most helpful review first: the order Steam's own page reads in.
    pub hits: Vec<Hit>,
}

impl Said {
    /// How many hits `narrow` leaves, and `count` of them from `from`.
    #[must_use]
    pub fn page(&self, narrow: Narrow<'_>, from: usize, count: usize) -> (u64, Vec<&Hit>) {
        let left: Vec<&Hit> = self
            .hits
            .iter()
            .filter(|hit| {
                narrow.side.is_none_or(|side| side == hit.polarity)
                    && narrow.subject.is_none_or(|subject| subject == hit.subject)
            })
            .collect();
        let total = left.len() as u64;
        (total, left.into_iter().skip(from).take(count).collect())
    }
}

/// What the subjects of [`Said`] call the claims the reader would not put a subject on. No
/// subject on the sheet is called this, so it cannot be mistaken for one.
pub const DECLINED: &str = "declined";

/// Which of the claims that say it to page through: one side, one subject, or both.
#[derive(Debug, Clone, Copy, Default)]
pub struct Narrow<'a> {
    /// `praise`, `complaint` or `neutral`.
    pub side: Option<&'a str>,
    /// A subject's id, or [`DECLINED`].
    pub subject: Option<&'a str>,
}

/// A claim as the readings file holds it, small enough to keep a whole game's in memory.
struct Filed {
    at: Span,
    subject: Option<u16>,
    polarity: Polarity,
    confidence: f32,
}

/// Every claim of a read game, ordered by the review it is in: what a search looks for words in.
///
/// Loading it is most of a search of a large game, a second for the largest's three million
/// claims against a third of one to walk its capture, so a window keeps the game it is
/// searching, at about 32 bytes a claim. One flat list rather than a map of reviews, whose
/// million keys and lists took longer to build and to free than the capture takes to walk.
/// Steam's review ids are numbers, so a claim is found by its number without a string held for
/// it; an id that is not one is no review Steam served and finds nothing.
pub struct Readings(Vec<(u64, Filed)>);

impl std::fmt::Debug for Readings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Readings({} claims)", self.0.len())
    }
}

impl Readings {
    /// The readings of the game read at `snapshot`.
    ///
    /// # Errors
    ///
    /// Fails if the readings cannot be read.
    pub fn load(snapshot: &Path) -> Result<Self> {
        let mut claims = Vec::new();
        let path = snapshot.join("readings.parquet");
        crate::read::for_each_full_reading(&path, |id, at, subject, confidence, polarity, _| {
            let Ok(id) = id.parse::<u64>() else {
                return;
            };
            let subject = subject
                .and_then(|name| SHEET.iter().position(|row| row.id == name))
                .and_then(|index| u16::try_from(index).ok());
            claims.push((
                id,
                Filed {
                    at,
                    subject,
                    polarity: Polarity::from_name(polarity),
                    confidence,
                },
            ));
        })?;
        // Stable, so a review's claims stay in the order they were read.
        claims.sort_by_key(|(id, _)| *id);
        Ok(Self(claims))
    }

    fn of(&self, review: &str) -> impl Iterator<Item = &Filed> {
        let id = review.parse::<u64>().ok();
        let from = id.map_or(self.0.len(), |id| {
            self.0.partition_point(|(held, _)| *held < id)
        });
        self.0[from..]
            .iter()
            .take_while(move |(held, _)| Some(*held) == id)
            .map(|(_, claim)| claim)
    }
}

/// Every claim of the game read at `snapshot`, whose readings are `filed`, that says `phrase`,
/// counted and kept.
///
/// # Errors
///
/// Fails if the capture cannot be read.
pub fn search(snapshot: &Path, filed: &Readings, phrase: &Phrase) -> Result<Said> {
    let name_of = |subject: Option<u16>| {
        subject
            .and_then(|index| SHEET.get(usize::from(index)))
            .map_or(DECLINED, |row| row.id)
    };
    // Each review that says it, with its helpfulness so the hits can be the most helpful first
    // rather than the first the capture happened to hold, and each claim's first form.
    let reviews = crate::capture::rows_kept(snapshot, |row, text| {
        let mut claims = filed.of(&row.recommendationid).peekable();
        claims.peek()?;
        if !phrase.found_in(text, |_| {}) {
            return None;
        }
        let review_id: Arc<str> = Arc::from(row.recommendationid.as_str());
        let mut hits = Vec::new();
        for claim in claims {
            let Some(words) = text.get(claim.at.0 as usize..claim.at.1 as usize) else {
                continue;
            };
            let mut first_form: Option<String> = None;
            if !phrase.found_in(words, |form| {
                first_form.get_or_insert_with(|| form.to_owned());
            }) {
                continue;
            }
            let hit = Hit {
                review_id: Arc::clone(&review_id),
                at: claim.at,
                polarity: claim.polarity.as_str(),
                subject: name_of(claim.subject),
                confidence: claim.confidence,
            };
            hits.push((hit, claim.polarity, first_form));
        }
        (!hits.is_empty()).then_some((row.helpfulness, hits))
    })?;

    let mut said = Said::default();
    let mut subjects: HashMap<&'static str, u64> = HashMap::new();
    let mut forms: HashMap<String, u64> = HashMap::new();
    let mut hits: Vec<(f64, Hit)> = Vec::new();
    for (helpfulness, found) in reviews {
        said.reviews += 1;
        for (hit, polarity, form) in found {
            said.claims += 1;
            match polarity {
                Polarity::Praise => said.praise += 1,
                Polarity::Complaint => said.complaint += 1,
                Polarity::Neutral => said.neutral += 1,
            }
            *subjects.entry(hit.subject).or_default() += 1;
            if let Some(form) = form {
                *forms.entry(form).or_default() += 1;
            }
            hits.push((helpfulness, hit));
        }
    }

    // Stable, so reviews Steam scores alike keep the capture's order and a page never
    // reshuffles between two asks.
    hits.sort_by(|a, b| b.0.total_cmp(&a.0));
    said.hits = hits.into_iter().map(|(_, hit)| hit).collect();
    said.subjects = commonest(subjects);
    said.forms = commonest(forms);
    said.forms.truncate(FORMS_KEPT);
    Ok(said)
}

/// Commonest first, and ties in a fixed order so the same search always reads the same.
fn commonest<K: Ord>(counts: HashMap<K, u64>) -> Vec<(K, u64)> {
    let mut ranked: Vec<(K, u64)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

#[cfg(test)]
pub(crate) mod tests {
    use arrow::{
        array::{ArrayRef, Float32Builder, StringBuilder, UInt32Builder},
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    };
    use parquet::arrow::ArrowWriter;
    use serde_json::json;

    use super::*;

    fn forms_of(query: &str, text: &str) -> Option<Vec<String>> {
        let mut forms = Vec::new();
        Phrase::new(query)
            .unwrap()
            .found_in(text, |form| forms.push(form.to_owned()))
            .then_some(forms)
    }

    #[test]
    fn a_phrase_is_found_as_the_words_in_order_whatever_their_case() {
        assert_eq!(
            forms_of("Steam Deck", "Runs great on the STEAM deck, honestly."),
            Some(vec!["steam deck".to_owned()])
        );
        assert_eq!(forms_of("steam deck", "the deck of cards on steam"), None);
        assert_eq!(
            forms_of("steam deck", "steam, deck"),
            Some(vec!["steam deck".to_owned()])
        );
    }

    #[test]
    fn a_long_word_finds_the_words_it_begins_and_a_short_one_only_itself() {
        assert_eq!(
            forms_of("stutter", "it stutters, and the stuttering is constant"),
            Some(vec!["stutters".to_owned(), "stuttering".to_owned()])
        );
        assert_eq!(forms_of("art", "the artist clearly cared"), None);
        assert_eq!(
            forms_of("art", "the art is lovely"),
            Some(vec!["art".to_owned()])
        );
    }

    #[test]
    fn a_word_ends_where_a_script_without_spaces_begins() {
        assert_eq!(
            forms_of("steam deck", "steam deck不能玩"),
            Some(vec!["steam deck".to_owned()])
        );
    }

    #[test]
    fn a_word_is_not_found_inside_another() {
        assert_eq!(forms_of("lag", "the flag was captured"), None);
    }

    #[test]
    fn a_script_without_spaces_is_looked_for_as_a_run() {
        assert_eq!(
            forms_of("卡顿", "这游戏卡顿太严重了"),
            Some(vec!["卡顿".to_owned()])
        );
        assert_eq!(forms_of("卡顿", "这游戏很好玩"), None);
    }

    #[test]
    fn a_query_with_nothing_to_look_for_is_no_phrase() {
        assert_eq!(Phrase::new("  "), None);
        assert_eq!(Phrase::new("?!"), None);
    }

    /// Three reviews read into five claims, three of which mention the Steam Deck.
    pub(crate) fn snapshot(dir: &Path) {
        let reviews = [
            json!({"recommendationid": "1", "review": "Runs badly on Steam Deck. Great story.",
                   "language": "english", "timestamp_created": 1, "timestamp_updated": 1,
                   "voted_up": false, "votes_up": 3, "weighted_vote_score": "0.5"}),
            json!({"recommendationid": "2", "review": "Perfect on my Steam Deck!",
                   "language": "english", "timestamp_created": 2, "timestamp_updated": 2,
                   "voted_up": true, "votes_up": 1, "weighted_vote_score": "0.1"}),
            json!({"recommendationid": "3", "review": "Steam deck? No idea. Fun though.",
                   "language": "english", "timestamp_created": 3, "timestamp_updated": 3,
                   "voted_up": true, "votes_up": 0, "weighted_vote_score": "0.9"}),
        ];
        let mut writer =
            crate::capture::CaptureWriter::create(&dir.join("shard-0000.parquet"), 1).unwrap();
        writer.write(&reviews.iter().collect::<Vec<_>>()).unwrap();
        writer.close().unwrap();

        // (review, start, end, subject, polarity)
        let rows: [(&str, u32, u32, Option<&str>, &str); 5] = [
            ("1", 0, 25, Some("performance"), "complaint"),
            ("1", 26, 38, Some("story"), "praise"),
            ("2", 0, 25, Some("performance"), "praise"),
            ("3", 0, 20, None, "neutral"),
            ("3", 21, 32, Some("verdict"), "praise"),
        ];
        let schema = Arc::new(Schema::new(vec![
            Field::new("recommendationid", DataType::Utf8, false),
            Field::new("start", DataType::UInt32, false),
            Field::new("end", DataType::UInt32, false),
            Field::new("subject", DataType::Utf8, true),
            Field::new("confidence", DataType::Float32, false),
            Field::new("polarity", DataType::Utf8, false),
        ]));
        let (mut ids, mut starts, mut ends) = (
            StringBuilder::new(),
            UInt32Builder::new(),
            UInt32Builder::new(),
        );
        let (mut subjects, mut confidences, mut polarities) = (
            StringBuilder::new(),
            Float32Builder::new(),
            StringBuilder::new(),
        );
        for (id, start, end, subject, polarity) in rows {
            ids.append_value(id);
            starts.append_value(start);
            ends.append_value(end);
            subjects.append_option(subject);
            confidences.append_value(0.9);
            polarities.append_value(polarity);
        }
        let columns: Vec<ArrayRef> = vec![
            Arc::new(ids.finish()),
            Arc::new(starts.finish()),
            Arc::new(ends.finish()),
            Arc::new(subjects.finish()),
            Arc::new(confidences.finish()),
            Arc::new(polarities.finish()),
        ];
        let batch = RecordBatch::try_new(Arc::clone(&schema), columns).unwrap();
        let file = std::fs::File::create(dir.join("readings.parquet")).unwrap();
        let mut writer = ArrowWriter::try_new(file, schema, None).unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
    }

    #[test]
    fn a_search_counts_the_claims_that_say_it_by_review_side_and_subject() {
        let dir = crate::tempdir::Dir::new();
        snapshot(dir.path());
        let phrase = Phrase::new("steam deck").unwrap();

        let said = search(dir.path(), &Readings::load(dir.path()).unwrap(), &phrase).unwrap();

        assert_eq!((said.reviews, said.claims), (3, 3));
        assert_eq!((said.praise, said.complaint, said.neutral), (1, 1, 1));
        assert_eq!(
            said.subjects,
            vec![("performance", 2), (DECLINED, 1)],
            "the claims that do not say it, the story and the verdict, are not counted"
        );
        assert_eq!(said.forms, vec![("steam deck".to_owned(), 3)]);
        let order: Vec<&str> = said.hits.iter().map(|hit| &*hit.review_id).collect();
        assert_eq!(order, ["3", "1", "2"], "most helpful review first");
    }

    #[test]
    fn a_narrowing_narrows_the_page_and_leaves_the_counts_whole() {
        let dir = crate::tempdir::Dir::new();
        snapshot(dir.path());
        let readings = Readings::load(dir.path()).unwrap();
        let said = search(dir.path(), &readings, &Phrase::new("steam deck").unwrap()).unwrap();
        let narrow = |side, subject| Narrow { side, subject };

        let (total, page) = said.page(narrow(Some("complaint"), None), 0, 10);
        assert_eq!(total, 1);
        assert_eq!((&*page[0].review_id, page[0].at), ("1", (0, 25)));
        assert_eq!(
            said.claims, 3,
            "the counts are of every claim, whatever the page"
        );

        assert_eq!(said.page(narrow(None, Some("performance")), 0, 10).0, 2);
        let (_, declined) = said.page(narrow(None, Some(DECLINED)), 0, 10);
        assert_eq!(&*declined[0].review_id, "3");

        let (total, later) = said.page(Narrow::default(), 2, 10);
        assert_eq!((total, later.len()), (3, 1), "the page starts at `from`");
    }
}
