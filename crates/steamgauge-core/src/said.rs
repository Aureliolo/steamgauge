//! What stands out in what was said about each subject.
//!
//! A count says a fifth of reviewers complain about performance. It does not say whether they
//! mean stutter, crashes or load times, and the reader who opened the row came for that. The
//! honest answer without a model paraphrasing anybody is the words themselves: the terms that
//! turn up in a subject's complaints far more than in its praise, and the other way round,
//! counted by how many reviewers used them.
//!
//! Compared side against side within one subject rather than against the corpus, because
//! against the corpus a subject's vocabulary is mostly the subject's name: "fps" and "runs"
//! stand out in every performance claim, praise or complaint alike, and tell a reader
//! nothing the row label did not. Complaint against praise is the comparison that separates
//! "stutter" from "smooth".
//!
//! Counted by reviews, once per review however often it repeated itself, for the same reason
//! the headline is a mention rate: nobody's verbosity moves it.
//!
//! And compared within each language, then pooled. A corpus is written in thirty languages
//! whose speakers do not praise and complain in the same proportions, so against the whole
//! other side a word is distinctive for being Spanish: "historia" stood out in the praise of
//! a story that Spanish speakers happened to like, and said nothing but "story". Praise against
//! complaint among the reviews that share a language is the comparison that cannot be won by
//! a language leaning one way.
//!
//! What it cannot do is read a negation that is not next to what it negates. "no
//! microtransactions" and "no micro transactions" are caught, because a word that turns what
//! follows reaches the word and the pair after it; "I never ran into any performance issues"
//! is not, and its "performance issues" is counted in praise like any other. Measured on one
//! game's performance row, three of the eight praising reviews that used the phrase wrote the
//! negation beside it and five wrote it further off. The page answers this by opening every
//! term onto the claims it was counted from, which is the only honest answer a counter that
//! does not parse can give.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::reader::Polarity;

/// Terms shown per side of a subject.
pub const TERMS_SHOWN: usize = 8;

/// A term used by fewer reviewers than this is a coincidence rather than a finding.
pub const FEWEST_REVIEWS: u64 = 5;

/// A side with fewer reviews than this shows nothing. Below it every word the few used clears
/// the bar against the many on the other side, and "but" was the finding about a subject
/// three people praised.
pub const FEWEST_ON_A_SIDE: u64 = 20;

/// Standard deviations of log-odds a term needs before it is shown.
const CLEARLY: f64 = 2.0;

/// How much likelier a term has to be on this side than on the other, as log-odds. Twice.
/// Significance alone is not enough: over a thousand reviews "and" is significantly commoner
/// in praise than in complaint, because praise runs longer, and it is not what anybody said.
const AT_LEAST_TWICE: f64 = std::f64::consts::LN_2;

/// The share of a term's every counted use that one side of one subject must hold. A word
/// spread across every subject is a word of the language rather than of the subject, however
/// unevenly the two sides of one subject happen to use it.
const OWNED: f64 = 0.25;

/// Distinct terms kept per side of a subject in one language while counting. Twice this is
/// the most a map ever holds, and only the largest languages on the largest subjects fill
/// one, so fifty sides over a million reviews in thirty languages stay within a hundred
/// megabytes whatever the vocabulary does.
const KEPT: usize = 4_096;

/// A term and how many reviews used it on one side of a subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    pub text: String,
    /// An undercount by at most a few where rare terms had to be forgotten to stay in memory,
    /// never an overcount.
    pub reviews: u64,
}

/// The terms that separate a subject's praise from its complaints.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SaidAbout {
    pub subject: String,
    /// Reviews with at least one praising claim about the subject.
    pub praising: u64,
    /// Reviews with at least one complaining claim about it.
    pub complaining: u64,
    pub praised: Vec<Term>,
    pub criticised: Vec<Term>,
}

/// Counts terms per side of every subject over a corpus, in bounded memory.
pub(crate) struct Said {
    /// Per language the reviews were written in: the two sides of every subject.
    languages: HashMap<String, Vec<[Counter; 2]>>,
    subjects: usize,
    /// What the review in hand has said so far, each term once per side of a subject.
    heard: HashSet<(usize, usize, String)>,
    scratch: Terms,
}

impl Said {
    pub(crate) fn new(subjects: usize) -> Self {
        Self {
            languages: HashMap::new(),
            subjects,
            heard: HashSet::new(),
            scratch: Terms::default(),
        }
    }

    /// Notes one claim of the review in hand. Neutral claims take no side and say nothing here.
    pub(crate) fn note(&mut self, subject: usize, polarity: Polarity, claim: &str) {
        let side = match polarity {
            Polarity::Praise => 0,
            Polarity::Complaint => 1,
            Polarity::Neutral => return,
        };
        if subject >= self.subjects {
            return;
        }
        let heard = &mut self.heard;
        self.scratch.each_in(claim, |term| {
            heard.insert((subject, side, term.to_owned()));
        });
    }

    /// Closes the review in hand and counts what it said, under the language it was written in.
    pub(crate) fn next_review(&mut self, language: &str) {
        if self.heard.is_empty() {
            return;
        }
        let subjects = self.subjects;
        let sides = self
            .languages
            .entry(language.to_owned())
            .or_insert_with(|| {
                (0..subjects)
                    .map(|_| [Counter::default(), Counter::default()])
                    .collect()
            });
        let mut sides_taken: HashSet<(usize, usize)> = HashSet::new();
        for (subject, side, term) in self.heard.drain() {
            let counter = &mut sides[subject][side];
            if sides_taken.insert((subject, side)) {
                counter.reviews += 1;
            }
            counter.add(term);
        }
    }

    /// What stands out on each side of every subject, in the order the subjects were given
    /// as `(id, label)`. A subject's own name is never one of its words: "graphics" under
    /// graphics and "tutorial" under tutorial say what the row label already said.
    pub(crate) fn finish(self, subjects: &[(&str, &str)]) -> Vec<SaidAbout> {
        // The languages pooled, for the candidates, the totals and the words of the language;
        // the languages apart, for the comparison.
        let mut pooled: Vec<[Counter; 2]> = (0..self.subjects)
            .map(|_| [Counter::default(), Counter::default()])
            .collect();
        for sides in self.languages.values() {
            for (subject, [praise, complaint]) in sides.iter().enumerate() {
                pooled[subject][0].absorb(praise);
                pooled[subject][1].absorb(complaint);
            }
        }
        let mut everywhere: HashMap<&str, u64> = HashMap::new();
        for [praise, complaint] in &pooled {
            for counter in [praise, complaint] {
                for (term, count) in &counter.terms {
                    *everywhere.entry(term.as_str()).or_default() += count;
                }
            }
        }
        pooled
            .iter()
            .zip(subjects)
            .enumerate()
            .map(|(subject, ([praise, complaint], (id, label)))| {
                let mut own: Vec<String> = label
                    .split(|ch: char| !ch.is_alphanumeric())
                    .filter(|word| !word.is_empty())
                    .map(str::to_lowercase)
                    .collect();
                own.push((*id).to_lowercase());
                if let Some((_, elsewhere)) = ALSO_CALLED.iter().find(|(named, _)| named == id) {
                    own.extend(elsewhere.iter().map(|name| (*name).to_owned()));
                }
                let apart: Vec<(&Counter, &Counter)> = self
                    .languages
                    .values()
                    .map(|sides| (&sides[subject][0], &sides[subject][1]))
                    .collect();
                let reversed: Vec<(&Counter, &Counter)> = apart
                    .iter()
                    .map(|(praise, complaint)| (*complaint, *praise))
                    .collect();
                SaidAbout {
                    subject: (*id).to_owned(),
                    praising: praise.reviews,
                    complaining: complaint.reviews,
                    praised: distinctive(praise, &apart, &everywhere, &own),
                    criticised: distinctive(complaint, &reversed, &everywhere, &own),
                }
            })
            .collect()
    }
}

/// Every term of a claim, by the cut that counts them, for diagnostics that read the corpus
/// the way the page does.
pub fn each_term(claim: &str, found: impl FnMut(&str)) {
    Terms::default().each_in(claim, found);
}

/// Whether a claim uses a term, by the same cut that counted it, so the claims a term opens
/// onto are exactly the ones it was counted from.
#[must_use]
pub fn mentions(claim: &str, term: &str) -> bool {
    let mut found = false;
    Terms::default().each_in(claim, |candidate| {
        if candidate == term {
            found = true;
        }
    });
    found
}

/// Term counts for one side of one subject, forgetting the rarest when it has to.
#[derive(Debug, Default)]
struct Counter {
    reviews: u64,
    terms: HashMap<String, u64>,
    /// The largest count ever dropped. A term absent from `terms` was used by at most this
    /// many reviews, which is what the other side is charged with when it lacks a term.
    forgotten: u64,
}

impl Counter {
    fn add(&mut self, term: String) {
        if let Some(count) = self.terms.get_mut(&term) {
            *count += 1;
            return;
        }
        self.terms.insert(term, 1);
        if self.terms.len() >= KEPT * 2 {
            self.forget();
        }
    }

    /// Adds another language's counts of the same side to this one.
    fn absorb(&mut self, other: &Self) {
        self.reviews += other.reviews;
        self.forgotten = self.forgotten.max(other.forgotten);
        for (term, count) in &other.terms {
            *self.terms.entry(term.clone()).or_default() += count;
        }
    }

    /// Keeps the commonest half. A term that comes back after being dropped starts again
    /// from one, so what survives is an undercount rather than an estimate, and a term used
    /// by many reviewers is never in the half that goes.
    fn forget(&mut self) {
        let mut kept: Vec<(String, u64)> = self.terms.drain().collect();
        kept.sort_unstable_by(|(left_term, left), (right_term, right)| {
            right.cmp(left).then_with(|| left_term.cmp(right_term))
        });
        if let Some((_, dropped)) = kept.get(KEPT) {
            self.forgotten = self.forgotten.max(*dropped);
        }
        kept.truncate(KEPT);
        self.terms = kept.into_iter().collect();
    }
}

/// The terms far commoner on `this` side than on `other`, and belonging to this subject
/// rather than to the language, most clearly so first.
#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn distinctive(
    this: &Counter,
    by_language: &[(&Counter, &Counter)],
    everywhere: &HashMap<&str, u64>,
    own_words: &[String],
) -> Vec<Term> {
    if this.reviews < FEWEST_ON_A_SIDE {
        return Vec::new();
    }
    let mut scored: Vec<(f64, &str, u64)> = this
        .terms
        .iter()
        .filter(|(_, here)| **here >= FEWEST_REVIEWS)
        // The row's own name says nothing new alone; in a phrase ("no bugs") it does.
        .filter(|(term, _)| !own_words.iter().any(|word| word == *term))
        .filter(|(term, here)| {
            let counted = everywhere.get(term.as_str()).copied().unwrap_or(**here);
            **here as f64 >= OWNED * counted as f64
        })
        .filter_map(|(term, &here)| {
            let (delta, z) = pooled_log_odds(term, by_language);
            (delta >= AT_LEAST_TWICE && z >= CLEARLY).then_some((z, term.as_str(), here))
        })
        .collect();
    // On a dead heat the phrase beats the word: "frame drops" and "drops" seen by exactly the
    // same reviewers are one finding, and the phrase is the one that names it.
    scored.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| right.1.len().cmp(&left.1.len()))
            .then_with(|| left.1.cmp(right.1))
    });

    let mut shown: Vec<Term> = Vec::with_capacity(TERMS_SHOWN);
    for (_, term, reviews) in scored {
        if shown.len() == TERMS_SHOWN {
            break;
        }
        // "full price" and "full" say one thing. The word outranks the phrase whenever a few
        // reviewers used it in some other phrase, and the phrase is still what was said, so
        // it takes the word's place unless it was noticeably rarer. A phrase a modifier turns
        // takes it at half: "no microtransactions" said by half the reviewers who said
        // "microtransactions" in praise is what the word meant there, and the word alone
        // reads as the opposite; and "no micro transactions" takes "no micro" the same way,
        // because the shorter is the phrase cut off.
        if let Some(at) = shown
            .iter()
            .position(|kept| shares_a_word(&kept.text, term))
        {
            let kept = &mut shown[at];
            let turned = turned_by_a_modifier(term)
                && (!turned_by_a_modifier(&kept.text) || term.starts_with(&kept.text));
            if term.len() > kept.text.len()
                && (nearly(reviews, kept.reviews) || (turned && reviews * 2 >= kept.reviews))
            {
                term.clone_into(&mut kept.text);
                kept.reviews = reviews;
                // The phrase that took the place of "micro transactions" also holds "no
                // micro", shown separately until now; a term that is a part of another shown
                // term is one finding, not two.
                let whole = term.to_owned();
                shown.retain(|other| other.text == whole || !shares_a_word(&other.text, &whole));
            }
            continue;
        }
        // "listened", "listening" and "listens" are one thing said three ways, and on a real
        // game they took three of the seven places a row has. The form more reviewers used
        // takes the place, because the point of the list is what was said.
        if let Some(kept) = shown
            .iter_mut()
            .find(|kept| one_word_inflected(&kept.text, term))
        {
            if reviews > kept.reviews {
                term.clone_into(&mut kept.text);
                kept.reviews = reviews;
            }
            continue;
        }
        // にほ, ほん and んご used by the same reviewers are one word, にほんご, cut into the
        // pairs the counter works in for a script it has no dictionary for. Joined back up
        // where the pairs overlap.
        if let Some(kept) = shown
            .iter_mut()
            .find(|kept| nearly(reviews, kept.reviews) && overlaps(&kept.text, term))
        {
            kept.text = joined(&kept.text, term);
            kept.reviews = kept.reviews.min(reviews);
            continue;
        }
        shown.push(Term {
            text: term.to_owned(),
            reviews,
        });
    }
    coalesce(&mut shown);
    shown
}

/// Joins runs of pairs that came out in two pieces.
///
/// The pairs arrive in an order that has nothing to do with the word, and a pair joins the
/// first run it overlaps, so にほんごがない can come out as にほん and んごがない with the
/// bridge taken by the second. Two runs that overlap by a character and were said by the
/// same reviewers are one word.
fn coalesce(shown: &mut Vec<Term>) {
    let mut at = 0;
    while at < shown.len() {
        let joined_one = (0..shown.len()).find(|&other| {
            other != at
                && nearly(shown[at].reviews, shown[other].reviews)
                && shown[other].text.chars().next().is_some_and(|first| {
                    crate::claims::writes_without_spaces(first)
                        && !is_han(first)
                        && shown[at].text.chars().count() > 1
                        && shown[at].text.ends_with(first)
                })
        });
        match joined_one {
            Some(other) => {
                let tail: String = shown[other].text.chars().skip(1).collect();
                let reviews = shown[other].reviews;
                shown[at].text.push_str(&tail);
                shown[at].reviews = shown[at].reviews.min(reviews);
                shown.remove(other);
                if other < at {
                    at -= 1;
                }
            }
            None => at += 1,
        }
    }
}

/// Whether two counts are within a twentieth of each other, which is what "the same
/// reviewers" means once a few of them have phrased a thing two ways.
fn nearly(left: u64, right: u64) -> bool {
    left * 20 >= right * 19 && right * 20 >= left * 19
}

/// Whether a term is one of the words that say nothing alone and turn what follows. A term
/// list holding one is a defect, which is what the diagnostic that asks this checks for.
#[must_use]
pub fn turns_what_follows(term: &str) -> bool {
    MODIFIERS.contains(&term)
}

/// Whether a term opens on a word that turns what follows: "no bugs", "not worth".
fn turned_by_a_modifier(term: &str) -> bool {
    term.split(' ')
        .next()
        .is_some_and(|first| MODIFIERS.contains(&first))
}

/// Whether one term is a whole word of the other, or a phrase the other continues. "price
/// tag" and "tag" are one finding, and so are "no micro" and "no micro transactions"; "full
/// price" and "price tag" are two, and a reader wants both. A Chinese pair is written solid,
/// so its words are its substrings: 操作手感 holds 操作 the way "price tag" holds "tag".
fn shares_a_word(left: &str, right: &str) -> bool {
    if left.chars().all(is_han) && right.chars().all(is_han) {
        return left.contains(right) || right.contains(left);
    }
    let (short, long) = if left.len() <= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    long.split(' ').any(|word| word == short)
        || (short.contains(' ')
            && (long.starts_with(&format!("{short} ")) || long.ends_with(&format!(" {short}"))))
}

/// The endings two inflections of one word may differ by. Deliberately short: "d" alone would
/// make "car" and "card" one word, and "y" alone would make "part" and "party" one, so a
/// plural in -ies is the one pair allowed to differ on both sides.
const ENDINGS: &[&str] = &["", "s", "es", "ed", "ing"];

/// Whether two terms are the same word in two forms: "server" and "servers", "listened" and
/// "listening", "story" and "stories". The shared beginning has to be a word's worth of
/// characters, because a three-letter agreement is a coincidence: "mode" and "mods" share
/// "mod" and are two findings, and the endings they differ by settle it.
///
/// Public so that a diagnostic can ask the same question of every page that has been counted,
/// which is how the rule is checked at the size it has to hold at.
#[must_use]
pub fn one_word_inflected(left: &str, right: &str) -> bool {
    if left == right {
        return false;
    }
    let shared = left
        .char_indices()
        .zip(right.chars())
        .take_while(|((_, here), there)| here == there)
        .map(|((at, here), _)| at + here.len_utf8())
        .last()
        .unwrap_or(0);
    if shared < 3 {
        return false;
    }
    let (here, there) = (&left[shared..], &right[shared..]);
    (ENDINGS.contains(&here) && ENDINGS.contains(&there))
        || matches!((here, there), ("y", "ies") | ("ies", "y"))
}

/// Whether a character is a Chinese ideograph, the script the dictionary cuts.
fn is_han(ch: char) -> bool {
    matches!(ch as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

/// Whether a character is a Korean syllable.
fn is_hangul(ch: char) -> bool {
    matches!(ch as u32, 0xAC00..=0xD7AF)
}

/// The particles Korean writes on the end of a word: subject, topic, object, "also", "of",
/// "at", "from", "to", "with", "than", "like", "only". Longest first, so 에서 comes off before
/// 서 would be looked for, and a word that is nothing but a particle keeps itself.
const HANGUL_PARTICLES: [&str; 26] = [
    "에서는",
    "에서도",
    "으로는",
    "으로도",
    "에서",
    "으로",
    "부터",
    "까지",
    "에게",
    "한테",
    "처럼",
    "만큼",
    "보다",
    "이나",
    "라도",
    "이",
    "가",
    "은",
    "는",
    "을",
    "를",
    "도",
    "의",
    "에",
    "로",
    "만",
];

/// The endings Korean conjugates a verb or an adjective with: 환불했습니다, 환불하고 and
/// 환불받았어요 are all "refunded" to a reader and three words to a counter. Longest first.
const HANGUL_ENDINGS: [&str; 30] = [
    "했습니다",
    "됐습니다",
    "받았습니다",
    "습니다",
    "했어요",
    "됐어요",
    "합니다",
    "됩니다",
    "하네요",
    "했다가",
    "하는데",
    "했는데",
    "하면서",
    "하고",
    "하는",
    "하다",
    "했다",
    "해서",
    "해요",
    "하게",
    "하지",
    "되는",
    "되다",
    "됐다",
    "이다",
    "네요",
    "어요",
    "아요",
    "었다",
    "았다",
];

/// A Korean word with its particle and its ending taken off, where they are on it and
/// something is left.
fn without_particle(word: &str) -> &str {
    without_suffix(without_suffix(word, &HANGUL_PARTICLES), &HANGUL_ENDINGS)
}

fn without_suffix<'a>(word: &'a str, suffixes: &[&str]) -> &'a str {
    suffixes
        .iter()
        .find_map(|suffix| {
            let stem = word.strip_suffix(suffix)?;
            (stem.chars().count() >= 2).then_some(stem)
        })
        .unwrap_or(word)
}

/// Whether a pair of characters from a script cut into pairs continues or precedes a run.
///
/// Chinese is cut into words now, and two of its words that happen to share an end
/// character (游戏性 and 性能) are two words, not a run to join.
fn overlaps(run: &str, pair: &str) -> bool {
    let mut chars = pair.chars();
    let (Some(first), Some(second), None) = (chars.next(), chars.next(), chars.next()) else {
        return false;
    };
    if !crate::claims::writes_without_spaces(first) || !crate::claims::writes_without_spaces(second)
    {
        return false;
    }
    if is_han(first) && is_han(second) {
        return false;
    }
    run.ends_with(first) || run.starts_with(second)
}

/// The run with the pair joined on at whichever end it overlaps.
fn joined(run: &str, pair: &str) -> String {
    let mut chars = pair.chars();
    let (first, second) = (chars.next().unwrap_or('\0'), chars.next().unwrap_or('\0'));
    if run.ends_with(first) {
        let mut out = run.to_owned();
        out.push(second);
        out
    } else {
        let mut out = String::from(first);
        out.push_str(run);
        out
    }
}

/// The log-odds of a term between two sides, and their standard deviation.
///
/// Half a count added to every cell so a term absent from one side is very unlikely rather
/// than infinitely so, which is the usual prior and keeps a term seen five times against
/// none from outranking one seen three hundred times against ten.
#[expect(
    clippy::cast_precision_loss,
    reason = "review counts are far below 2^53"
)]
fn log_odds(here: u64, of: u64, there: u64, of_other: u64) -> (f64, f64) {
    let a = here as f64 + 0.5;
    let b = of.saturating_sub(here) as f64 + 0.5;
    let c = there as f64 + 0.5;
    let d = of_other.saturating_sub(there) as f64 + 0.5;
    let delta = (a / b).ln() - (c / d).ln();
    let sigma = (1.0 / a + 1.0 / b + 1.0 / c + 1.0 / d).sqrt();
    (delta, sigma)
}

/// The log-odds of a term between two sides compared within each language and then pooled,
/// weighted by how much each language can say, and how many standard deviations that is.
///
/// A language whose reviewers lean one way cannot make its words distinctive of that side:
/// each language's odds are its own praise against its own complaints. A term absent from a
/// language's counter on this side is taken as unused there, and on the other side as used
/// by as many reviews as the counter may have forgotten, which errs against showing it.
fn pooled_log_odds(term: &str, by_language: &[(&Counter, &Counter)]) -> (f64, f64) {
    let (mut weight, mut weighted) = (0.0, 0.0);
    for (this, other) in by_language {
        if this.reviews == 0 && other.reviews == 0 {
            continue;
        }
        let (here, there) = match (this.terms.get(term), other.terms.get(term)) {
            (None, None) => continue,
            (Some(&here), None) => (here, other.forgotten),
            (here, Some(&there)) => (here.copied().unwrap_or(0), there),
        };
        let (delta, sigma) = log_odds(here, this.reviews, there, other.reviews);
        let w = 1.0 / (sigma * sigma);
        weight += w;
        weighted += w * delta;
    }
    if weight == 0.0 {
        return (0.0, 0.0);
    }
    let delta = weighted / weight;
    (delta, delta * weight.sqrt())
}

/// Cuts a claim into the terms worth counting: words and pairs of adjacent words, with a
/// run of Chinese cut into words by a dictionary first, and a run of Japanese or Korean
/// into pairs of adjacent characters, which is the best that can be done without one.
///
/// Reused across claims so a corpus of millions does not allocate a buffer per word.
#[derive(Debug, Default)]
struct Terms {
    word: String,
    previous: String,
    /// The modifier before `previous`, when there was one: "no" in "no micro transactions",
    /// so that the negation reaches the pair it was said about and not only the word after it.
    turning: String,
    pair: String,
    /// The run of a spaceless script in hand, cut when it ends.
    run: String,
}

/// The Chinese dictionary, loaded on the first Chinese claim and kept, with the words of
/// the trade added: a general dictionary cuts 掉帧 into "drop" and "frame".
fn segmenter() -> &'static jieba_rs::Jieba {
    static JIEBA: std::sync::LazyLock<jieba_rs::Jieba> = std::sync::LazyLock::new(|| {
        let mut jieba = jieba_rs::Jieba::new();
        for word in HAN_LEXICON {
            jieba.add_word(word, None, None);
        }
        jieba
    });
    &JIEBA
}

/// What players write about games that a general Chinese dictionary does not know as words:
/// frame drops, save files, achievements, controls, localisation, the store. Without them the
/// segmenter hands back single characters, and a single character is heard only in a pair.
const HAN_LEXICON: [&str; 72] = [
    "掉帧",
    "帧数",
    "帧率",
    "锁帧",
    "卡顿",
    "卡死",
    "卡关",
    "闪退",
    "黑屏",
    "崩溃",
    "优化",
    "上手",
    "手感",
    "打击感",
    "键位",
    "键鼠",
    "手柄",
    "适配",
    "分辨率",
    "画质",
    "画风",
    "建模",
    "贴图",
    "光污染",
    "存档",
    "读档",
    "全成就",
    "成就",
    "白金",
    "流程",
    "剧情",
    "结局",
    "跑图",
    "刷刷刷",
    "肉鸽",
    "魂系",
    "类魂",
    "平台跳跃",
    "银河城",
    "新手引导",
    "引导",
    "判定",
    "碰撞",
    "连招",
    "数值",
    "氪金",
    "抽卡",
    "内购",
    "皮肤",
    "季票",
    "通行证",
    "开箱",
    "首发",
    "史低",
    "折扣",
    "退款",
    "汉化",
    "简中",
    "繁中",
    "中配",
    "配音",
    "字幕",
    "乱码",
    "联机",
    "单机",
    "掉线",
    "延迟",
    "服务器",
    "外挂",
    "作弊",
    "热修",
    "跳票",
];

/// Single characters that turn the Chinese word after them, as "not" and "too" do: 不好 is
/// "not good" and 太贵 is "too expensive", and neither half says it alone.
const HAN_MODIFIERS: [&str; 11] = [
    "不", "没", "无", "太", "很", "更", "最", "超", "不太", "不够", "超级",
];

impl Terms {
    fn each_in(&mut self, claim: &str, mut visit: impl FnMut(&str)) {
        self.word.clear();
        self.previous.clear();
        self.turning.clear();
        self.run.clear();

        for ch in claim.chars() {
            // Korean is written with spaces between words, so it goes the way of a spaced
            // script and only its particles need taking off; the other scripts here do not.
            if crate::claims::writes_without_spaces(ch) && !is_hangul(ch) {
                self.close_word(&mut visit);
                self.previous.clear();
                self.turning.clear();
                self.run.push(ch);
                continue;
            }
            self.close_run(&mut visit);
            if ch.is_alphanumeric() {
                for lower in ch.to_lowercase() {
                    self.word.push(lower);
                }
            } else if (ch == '\'' || ch == '\u{2019}') && !self.word.is_empty() {
                // "don't" is one word, and the curly apostrophe phones type is the same one.
                self.word.push('\'');
            } else {
                self.close_word(&mut visit);
                // A pair of words straddling a comma or a bracket is not a phrase anybody
                // used; only a space carries the chain on, and a hyphen, because "full-price"
                // and "full price" are the same two words.
                if !ch.is_whitespace() && ch != '-' {
                    self.previous.clear();
                    self.turning.clear();
                }
            }
        }
        self.close_run(&mut visit);
        self.close_word(&mut visit);
    }

    /// Cuts the run of a spaceless script in hand and emits its terms.
    ///
    /// A run of Chinese alone goes through the dictionary: a pair of characters that
    /// straddles two words (作手 out of 操作手感) is not a word anybody used, and it was a
    /// third of what the page showed for a Chinese-speaking game. Words come out the way words
    /// in a spaced script do, alone and paired with the word before them, the pair written
    /// solid as Chinese is. A run holding kana or hangul keeps the pairs of characters: the
    /// dictionary is Chinese, and a Japanese sentence through it is cut wrongly with
    /// confidence.
    fn close_run(&mut self, visit: &mut impl FnMut(&str)) {
        if self.run.is_empty() {
            return;
        }
        if self.run.chars().all(is_han) {
            // The word a pair starts from, and whether it is a modifier, because modifiers
            // stack: 不太友好 is "not too friendly", and a pair starting from the last of them
            // alone would say the opposite.
            let mut previous = String::new();
            let mut modifying = false;
            let mut previous_lone = false;
            for token in segmenter().cut(&self.run, true) {
                let word = token.word;
                if HAN_MODIFIERS.contains(&word) {
                    if !modifying {
                        previous.clear();
                    }
                    previous.push_str(word);
                    modifying = true;
                    previous_lone = false;
                    continue;
                }
                modifying = false;
                if filler().contains(word) {
                    previous.clear();
                    continue;
                }
                // A lone character is a syllable more often than a word, so it is heard in
                // the pair it makes with what came before it and not on its own. Two lone
                // characters in a row are a word the dictionary lacks (掉帧), so they pair;
                // a lone character before a whole word is the tail of something (手 out of
                // 上手 before 难度), and does not lead a pair.
                let whole = word.chars().count() >= 2;
                if whole {
                    visit(word);
                }
                if !previous.is_empty() && (!whole || !previous_lone) {
                    self.pair.clear();
                    self.pair.push_str(&previous);
                    self.pair.push_str(word);
                    visit(&self.pair);
                }
                previous.clear();
                previous.push_str(word);
                previous_lone = !whole;
            }
        } else {
            let mut last: Option<char> = None;
            for ch in self.run.chars() {
                if let Some(before) = last {
                    self.pair.clear();
                    self.pair.push(before);
                    self.pair.push(ch);
                    // A filler pair breaks the chain as a filler word does, so 没有中文 yields
                    // 中文 and not the bridge 有中 as well.
                    if filler().contains(self.pair.as_str()) {
                        last = None;
                        continue;
                    }
                    visit(&self.pair);
                }
                last = Some(ch);
            }
        }
        self.run.clear();
    }

    /// Emits the word in hand, and its pair with the word before it, then remembers it.
    fn close_word(&mut self, visit: &mut impl FnMut(&str)) {
        let mut word = self.word.trim_matches('\'');
        if word.is_empty() {
            return;
        }
        // 그래픽이, 그래픽은 and 그래픽도 are "graphics" with a particle on the end, one
        // word to a reader and three to a counter until the particle comes off.
        if word.chars().all(is_hangul) {
            word = without_particle(word);
        }
        // "runs at 60 fps" must not yield "at fps": a word not worth counting still breaks
        // the chain of pairs.
        if word.chars().count() < 2 || word.chars().all(|ch| ch.is_ascii_digit()) {
            self.word.clear();
            self.previous.clear();
            self.turning.clear();
            return;
        }
        let kind = kind_of(word);
        if kind == Word::Content {
            visit(word);
            if !self.previous.is_empty() {
                self.pair.clear();
                self.pair.push_str(&self.previous);
                self.pair.push(' ');
                self.pair.push_str(word);
                visit(&self.pair);
                // "no micro transactions": the modifier reaches over a pair as well as a word,
                // because a pair the modifier turns is said about the pair, and "micro
                // transactions" shown in praise beside "no micro" would read as the opposite
                // of what was said.
                if !self.turning.is_empty() {
                    self.pair.insert(0, ' ');
                    self.pair.insert_str(0, &self.turning);
                    visit(&self.pair);
                }
            }
        }
        // A pair ends on a content word and starts on one, or on the few function words that
        // change what follows: "not worth" and "on sale" are findings, "worth it" and "is
        // great" are not, and "it on" was the best a thousand price claims had to offer.
        self.turning.clear();
        if kind == Word::Content && kind_of(&self.previous) == Word::Modifier {
            self.turning.push_str(&self.previous);
        }
        self.previous.clear();
        if kind != Word::Filler {
            self.previous.push_str(word);
        }
        self.word.clear();
    }
}

/// What a word is worth on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Word {
    /// Says something: counted alone and in pairs.
    Content,
    /// Says nothing alone but turns what follows: "not", "no", "too", "on".
    Modifier,
    /// Says nothing and is counted nowhere: "the", "is", "game".
    Filler,
}

fn kind_of(word: &str) -> Word {
    if MODIFIERS.contains(&word) {
        Word::Modifier
    } else if filler().contains(word) {
        Word::Filler
    } else {
        Word::Content
    }
}

/// What each subject is called in the languages the library is written in, which the page
/// keeps off the subject's rows as it keeps the English label off them: "сюжет" under story
/// says what the row said. Read from the corpus rather than translated: the term most of a
/// language's reviews about a subject use is its name there (`subject-names`, an example),
/// and it sits at a quarter to a half of them where the next term sits at a tenth. Nouns and
/// their common inflections only; "optimised", "hard" and "worth" are findings and stay.
const ALSO_CALLED: &[(&str, &[&str])] = &[
    (
        "performance",
        &[
            "производительность",
            "оптимизация",
            "оптимизации",
            "optimierung",
            "optimisation",
            "rendimiento",
            "optimización",
            "desempenho",
            "otimização",
            "prestazioni",
            "ottimizzazione",
            "wydajność",
            "optymalizacja",
            "performans",
            "optimizasyon",
            "优化",
            "性能",
            "優化",
            "最適化",
            "최적화",
            "성능",
        ],
    ),
    (
        "bugs",
        &[
            "bug",
            "crash",
            "баги",
            "баг",
            "багов",
            "ошибки",
            "вылеты",
            "вылет",
            "fehler",
            "abstürze",
            "bogues",
            "plantages",
            "errores",
            "erros",
            "travamentos",
            "bugi",
            "błędy",
            "hatalar",
            "hata",
            "çökme",
            "버그",
            "闪退",
            "崩溃",
            "错误",
            "閃退",
            "崩潰",
        ],
    ),
    (
        "gameplay",
        &[
            "геймплей",
            "геймплея",
            "игровой процесс",
            "механики",
            "механика",
            "spielmechanik",
            "mechaniken",
            "jugabilidad",
            "mecánicas",
            "mecânicas",
            "giocabilità",
            "meccaniche",
            "rozgrywka",
            "mechanika",
            "oynanış",
            "mekanikler",
            "玩法",
            "游戏性",
            "机制",
            "遊戲性",
            "게임플레",
            "게임성",
        ],
    ),
    (
        "genre",
        &[
            "жанр",
            "жанра",
            "género",
            "gênero",
            "genere",
            "gatunek",
            "tür",
            "类型",
            "類型",
            "장르",
        ],
    ),
    (
        "story",
        &[
            "сюжет",
            "сюжета",
            "сюжетом",
            "сюжету",
            "история",
            "истории",
            "geschichte",
            "handlung",
            "histoire",
            "l'histoire",
            "scénario",
            "historia",
            "trama",
            "história",
            "enredo",
            "storia",
            "fabuła",
            "fabuły",
            "hikaye",
            "hikayesi",
            "senaryo",
            "剧情",
            "故事",
            "劇情",
            "物語",
            "스토리",
            "시나리오",
        ],
    ),
    (
        "atmosphere",
        &[
            "атмосфера",
            "атмосферу",
            "атмосферы",
            "атмосферой",
            "atmosphäre",
            "ambiance",
            "l'ambiance",
            "atmosphère",
            "atmósfera",
            "ambiente",
            "atmosfera",
            "clima",
            "klimat",
            "klimatu",
            "atmosfer",
            "氛围",
            "气氛",
            "氛圍",
            "분위기",
        ],
    ),
    (
        "graphics",
        &[
            "графика",
            "графику",
            "графики",
            "графикой",
            "grafik",
            "die grafik",
            "graphismes",
            "graphisme",
            "graphiques",
            "gráficos",
            "graficos",
            "grafica",
            "grafika",
            "grafiki",
            "grafikler",
            "grafikleri",
            "画面",
            "画质",
            "畫面",
            "畫質",
            "그래픽",
        ],
    ),
    (
        "audio",
        &[
            "sound",
            "sounds",
            "soundtrack",
            "музыка",
            "музыку",
            "звук",
            "звуки",
            "звуковое",
            "саундтрек",
            "musik",
            "sonore",
            "bande",
            "musique",
            "sonido",
            "sonora",
            "banda sonora",
            "música",
            "som",
            "trilha",
            "trilha sonora",
            "suono",
            "sonoro",
            "colonna sonora",
            "comparto",
            "muzyka",
            "dźwięk",
            "udźwiękowienie",
            "ses",
            "sesler",
            "müzik",
            "müzikler",
            "音效",
            "音乐",
            "音樂",
            "配乐",
            "bgm",
            "音楽",
            "사운드",
            "소리",
            "음악",
        ],
    ),
    (
        "controls",
        &[
            "управление",
            "управления",
            "интерфейс",
            "steuerung",
            "contrôles",
            "commandes",
            "controles",
            "mandos",
            "controlli",
            "comandi",
            "sterowanie",
            "interfejs",
            "kontroller",
            "kontrol",
            "arayüz",
            "操作",
            "控制",
            "界面",
            "操作性",
            "컨트롤",
            "조작",
            "인터페이스",
        ],
    ),
    (
        "difficulty",
        &[
            "сложность",
            "сложности",
            "баланс",
            "schwierigkeit",
            "schwierigkeitsgrad",
            "difficulté",
            "dificultad",
            "dificuldade",
            "difficoltà",
            "trudność",
            "trudności",
            "poziom trudności",
            "zorluk",
            "denge",
            "难度",
            "難度",
            "난이",
            "밸런스",
        ],
    ),
    (
        "content",
        &[
            "контент",
            "контента",
            "inhalt",
            "contenu",
            "contenido",
            "conteúdo",
            "contenuto",
            "contenuti",
            "zawartość",
            "içerik",
            "内容",
            "內容",
            "컨텐츠",
            "콘텐츠",
        ],
    ),
    (
        "mods",
        &[
            "mod",
            "моды",
            "модов",
            "modding",
            "mody",
            "modlar",
            "模组",
            "模組",
            "모드",
        ],
    ),
    (
        "price",
        &[
            "цена", "цены", "цену", "preis", "prix", "precio", "preço", "prezzo", "cena", "ceny",
            "fiyat", "价格", "價格", "値段", "価格", "가격",
        ],
    ),
    (
        "monetisation",
        &[
            "монетизация",
            "донат",
            "микротранзакции",
            "monetarisierung",
            "monétisation",
            "monetización",
            "monetização",
            "monetizzazione",
            "monetyzacja",
            "mikrotransakcje",
            "内购",
            "氪金",
            "交易",
            "과금",
            "課金",
        ],
    ),
    (
        "multiplayer",
        &[
            "мультиплеер",
            "онлайн",
            "mehrspieler",
            "multijoueur",
            "multijugador",
            "tryb wieloosobowy",
            "oyunculu",
            "多人",
            "联机",
            "聯機",
            "線上",
            "온라인",
            "멀티",
            "멀티플레",
        ],
    ),
    (
        "community",
        &[
            "сообщество",
            "комьюнити",
            "игроки",
            "spieler",
            "communauté",
            "joueurs",
            "comunidad",
            "jugadores",
            "comunidade",
            "jogadores",
            "comunità",
            "giocatori",
            "społeczność",
            "gracze",
            "topluluk",
            "oyuncular",
            "社区",
            "社群",
            "커뮤니티",
            "유저",
            "플레이어",
        ],
    ),
    (
        "updates",
        &[
            "обновления",
            "обновление",
            "разработчики",
            "разработчик",
            "патчи",
            "entwickler",
            "développeurs",
            "actualizaciones",
            "desarrolladores",
            "atualizações",
            "desenvolvedores",
            "aggiornamenti",
            "sviluppatori",
            "aktualizacje",
            "deweloperzy",
            "güncelleme",
            "güncellemeler",
            "geliştiriciler",
            "更新",
            "开发者",
            "开发商",
            "開發",
            "업데이트",
            "개발자",
        ],
    ),
    (
        "policy",
        &[
            "издатель",
            "издателя",
            "verlag",
            "éditeur",
            "editora",
            "editore",
            "wydawca",
            "yayıncı",
            "发行商",
            "퍼블리셔",
        ],
    ),
    (
        "compatibility",
        &[
            "совместимость",
            "железо",
            "kompatibilität",
            "compatibilité",
            "compatibilidad",
            "compatibilidade",
            "compatibilità",
            "kompatybilność",
            "uyumluluk",
            "兼容",
            "配置",
            "兼容性",
            "호환",
            "사양",
        ],
    ),
    (
        "accessibility",
        &[
            "доступность",
            "настройки",
            "barrierefreiheit",
            "einstellungen",
            "accessibilité",
            "accesibilidad",
            "opciones",
            "acessibilidade",
            "opções",
            "accessibilità",
            "opzioni",
            "dostępność",
            "opcje",
            "erişilebilirlik",
            "seçenekler",
            "ayarlar",
            "无障碍",
            "选项",
            "設定",
            "옵션",
            "접근성",
        ],
    ),
    (
        "language",
        &[
            "язык",
            "перевод",
            "локализация",
            "sprache",
            "übersetzung",
            "langue",
            "traduction",
            "idioma",
            "traducción",
            "doblaje",
            "tradução",
            "dublagem",
            "lingua",
            "traduzione",
            "język",
            "tłumaczenie",
            "dil",
            "çeviri",
            "中文",
            "汉化",
            "语言",
            "翻译",
            "繁中",
            "簡中",
            "한국어",
            "한글",
            "번역",
            "日本語",
            "翻訳",
        ],
    ),
    (
        "tutorial",
        &[
            "обучение",
            "туториал",
            "anleitung",
            "tutoriel",
            "didacticiel",
            "samouczek",
            "öğretici",
            "教程",
            "教學",
            "新手",
            "튜토리얼",
        ],
    ),
    (
        "licensing",
        &[
            "лицензия",
            "лицензии",
            "lizenz",
            "licence",
            "licences",
            "licencia",
            "licença",
            "licenza",
            "licencja",
            "lisans",
            "授权",
            "版权",
            "授權",
            "라이선스",
            "라이센스",
        ],
    ),
    (
        "vr",
        &[
            "виар",
            "шлем",
            "гарнитура",
            "headset",
            "casque",
            "visor",
            "óculos",
            "visore",
            "gogle",
            "sanal gerçeklik",
            "虚拟现实",
            "头显",
            "頭顯",
            "헤드셋",
            "가상현실",
        ],
    ),
];

/// Function words that carry meaning into the word after them.
///
/// The contractions are here because a review negates with them far more often than with
/// "not": "can't" was a complaint term of its own on a million-review page, where it said
/// nothing at all, and "can't recommend" said the thing the reviewer meant.
///
/// The rest are the same word in the languages the library is written in. Read from what the
/// pages already show: over 53 counted games, "keine", "без", "слишком" and "нельзя" had each
/// taken a place on a row by themselves. A negation is the one word whose absence changes a
/// finding into its opposite, so the languages that write it as a word of its own get the
/// same treatment English does. Japanese and Chinese negate inside the word and are handled
/// where those scripts are cut.
const MODIFIERS: &[&str] = &[
    // English, including the contractions a review actually uses.
    "no",
    "not",
    "never",
    "without",
    "too",
    "on",
    "off",
    "less",
    "more",
    "only",
    "still",
    "always",
    "can't",
    "cannot",
    "don't",
    "doesn't",
    "didn't",
    "won't",
    "wouldn't",
    "couldn't",
    "shouldn't",
    "isn't",
    "wasn't", //
    // German.
    "nicht",
    "kein",
    "keine",
    "keinen",
    "ohne",
    "nie",
    "zu",
    "sehr", //
    // Russian and Ukrainian.
    "не",
    "нет",
    "без",
    "нельзя",
    "слишком",
    "очень", //
    // Spanish and Portuguese.
    "sin",
    "sem",
    "não",
    "nunca",
    "demasiado",
    "muy",
    "muito", //
    // French.
    "pas",
    "sans",
    "jamais",
    "trop", //
    // Italian.
    "non",
    "senza",
    "mai",
    "troppo", //
    // Polish and Czech.
    "nie",
    "bez",
    "zbyt", //
    // Turkish.
    "değil",
    "yok",
    "çok", //
    // Korean, which is written with spaces and negates with a word of its own.
    "안",
    "못",
];

/// Function words, and the handful of words that are function words in a Steam review:
/// every claim is about a game somebody played. English, Chinese and a short list each for
/// the next six languages Steam is written in; the rest mostly fail the ownership bar on
/// their own, and a list for each of forty languages would be a maintenance burden with no
/// measured return. Sentiment words are not here: "great" fails the ownership bar by itself,
/// and "worth" is what the price subject is made of. The Chinese entries are the words the
/// dictionary cuts out, and the single characters among them are the particles it leaves
/// standing alone: 了 and 的 would otherwise pair with every word before them.
const FILLER: &str = "\
the a an and or but if so as of to in at by for from with into onto about over under than \
then that this these those there here it its it's is are was were be been being am i i'm i've \
i'd i'll you your you're he she they them their we our us me my mine him his her who whom \
whose which what when where why how have has had having do does did doing done will would can \
could should may might must shall get got gets getting make makes made go goes going went \
come comes came also just very really quite pretty even much many most some any all each \
every both few such own other another same one ones thing things something anything nothing \
everything someone anyone everyone yes yeah yep nope ok okay well like lot lots bit way \
because cause while though although since until after before again out up down back through \
around now ever yet far game games play played playing plays player players steam review \
reviews \
游戏 玩家 这个 那个 一个 自己 不是 就是 可以 什么 但是 因为 所以 如果 还是 已经 觉得 知道 一下 \
一些 非常 比较 这样 那么 然后 而且 或者 虽然 不过 真的 感觉 我们 你们 他们 没有 有点 这种 那种 \
时候 东西 的话 是的 不能 不会 不要 应该 可能 现在 之后 之前 以及 对于 关于 需要 只是 只有 而已 \
的时 我的 你的 它的 他的 这些 那些 一样 一直 一点 一定 一起 还有 也是 都是 就会 就能 不了 \
太多 很多 玩了 玩的 玩过 玩到 小时 多小 个小 \
的 了 是 在 和 也 都 就 吗 呢 吧 啊 与 及 或 而 被 把 让 给 对 从 到 去 来 有 会 能 要 \
我 你 他 她 它 们 这 那 个 些 又 还 才 却 并 之 其 所 为 以 于 \
и в не на что это как но а то же он она они мы вы я ты у из за для по от о до при или \
если бы был была было были есть нет очень так только уже еще ещё все всё игра игры игру \
игре этот эта это эти его её их мне меня тебе вас нам них там тут здесь \
der das und ist nicht ein eine einer einen dem den des ich du er sie es wir ihr \
mit von zu auf für aus bei nach über auch nur noch schon sehr aber oder wenn dass wie \
ab um \
wo da hier dort wird sind habe haben kann spiel spiele spielen \
el la los las un una unos unas y o pero de del en con por para que es son está están \
muy más menos también ya no sí este esta esto ese esa eso lo le les se me te su sus mi \
juego juegos jugar jugando \
o a os as um uma e é são está estão de do da dos das em no na nos nas com por para que \
não sim muito mais menos também já este esta isto esse essa isso ele ela eles elas eu tu \
jogo jogos jogar jogando \
le la les un une des et ou mais de du au aux en dans sur pour par avec sans que qui ne \
pas plus moins très aussi est sont c'est il elle ils elles je tu on nous vous ce cette \
ces ça jeu jeux jouer \
i w na z do nie się jest są to tak jak ale co za od po dla przez ten ta te tego tej tym \
gra gry grę grze grać \
ve bir bu şu o de da için ile çok daha ama ya en gibi kadar var yok mi mı mu mü değil \
oyun oyunu oyunda oyna \
게임 그리고 하지만 그냥 정말 진짜 너무 이거 저거 그거 이런 그런 저런 있다 없다 하다 같다 되다 \
있는 없는 하는 되는 입니다 합니다 있습니다 없습니다 것 수 등 더 좀 잘 안 못 왜 다 또 아직 이미 \
계속 근데 그래서 그런데 만약 뭐 걍 ㅋㅋ ㅎㅎ";

/// The filler words as a set, built once: the tokeniser asks about every word of every claim.
fn filler() -> &'static HashSet<&'static str> {
    static SET: std::sync::OnceLock<HashSet<&'static str>> = std::sync::OnceLock::new();
    SET.get_or_init(|| FILLER.split_whitespace().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(claim: &str) -> Vec<String> {
        let mut found = Vec::new();
        Terms::default().each_in(claim, |term| found.push(term.to_owned()));
        found
    }

    #[test]
    fn words_and_their_pairs_stop_at_punctuation_and_skip_bare_numbers() {
        assert_eq!(
            terms("Frame drops in the city, 60fps elsewhere. 10/10"),
            [
                "frame",
                "drops",
                "frame drops",
                "city",
                "60fps",
                "elsewhere",
                "60fps elsewhere",
            ]
        );
    }

    #[test]
    fn apostrophes_stay_inside_a_word_and_case_is_folded() {
        // "don't" turns what follows and says nothing alone, so the pair is where it shows.
        assert_eq!(terms("Don’t BUY it"), ["buy", "don't buy"]);
    }

    #[test]
    fn a_contraction_negates_the_word_after_it_rather_than_standing_alone() {
        let found = terms("can't recommend the combat");
        assert!(!found.contains(&"can't".to_owned()), "{found:?}");
        assert!(found.contains(&"can't recommend".to_owned()), "{found:?}");
    }

    #[test]
    fn a_function_word_leads_a_pair_only_when_it_turns_the_word_after_it() {
        assert_eq!(
            terms("not worth it on sale, and the story is short"),
            ["worth", "not worth", "sale", "on sale", "story", "short"]
        );
    }

    #[test]
    fn a_modifier_reaches_over_the_pair_it_turns() {
        assert_eq!(
            terms("no micro transactions at all"),
            [
                "micro",
                "no micro",
                "transactions",
                "micro transactions",
                "no micro transactions",
            ]
        );
        // Only over the pair straight after it: the turn does not carry a word further.
        assert_eq!(
            terms("no frame rate drops"),
            [
                "frame",
                "no frame",
                "rate",
                "frame rate",
                "no frame rate",
                "drops",
                "rate drops",
            ]
        );
        // A modifier before a modifier does not stack into a triple of function words.
        assert_eq!(terms("not too hard"), ["hard", "too hard"]);
    }

    #[test]
    fn a_phrase_and_the_phrase_that_continues_it_are_one_finding() {
        assert!(shares_a_word("no micro", "no micro transactions"));
        assert!(shares_a_word("micro transactions", "no micro transactions"));
        assert!(shares_a_word("tag", "price tag"));
        assert!(!shares_a_word("full price", "price tag"));
        assert!(!shares_a_word("no micro", "micro transactions"));
    }

    #[test]
    fn a_negated_phrase_takes_the_place_of_a_word_said_half_the_time_negated() {
        let mut said = Said::new(1);
        for review in 0..100 {
            said.note(
                0,
                Polarity::Praise,
                if review < 55 {
                    "no microtransactions at all"
                } else {
                    "microtransactions are optional"
                },
            );
            said.note(0, Polarity::Complaint, "too expensive");
            said.next_review("english");
        }
        let found = said.finish(&[("monetisation", "Monetisation and DLC")]);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        assert!(praised.contains(&"no microtransactions"), "{praised:?}");
        assert!(!praised.contains(&"microtransactions"), "{praised:?}");

        // Written as two words, the negation has to reach the pair, and the phrase cut off at
        // "no micro" must not be what is shown when most of those reviewers went on to say
        // "transactions".
        let mut said = Said::new(1);
        for review in 0..100 {
            said.note(
                0,
                Polarity::Praise,
                match review % 10 {
                    0..=5 => "no micro transactions at all",
                    6 => "no micro payments",
                    _ => "micro transactions are optional",
                },
            );
            said.note(0, Polarity::Complaint, "too expensive");
            said.next_review("english");
        }
        let found = said.finish(&[("monetisation", "Monetisation and DLC")]);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        assert!(praised.contains(&"no micro transactions"), "{praised:?}");
        assert!(!praised.contains(&"no micro"), "{praised:?}");
        assert!(!praised.contains(&"transactions"), "{praised:?}");
        assert!(!praised.contains(&"micro"), "{praised:?}");
    }

    #[test]
    fn chinese_is_cut_into_words_and_a_lone_character_is_heard_only_in_its_pair() {
        // 很 turns 好 the way "very" turns "good"; 卡 alone is a syllable.
        assert_eq!(terms("画面很好 lags 卡"), ["画面", "很好", "lags"]);
        // Two words and the pair they make, written solid; the pair is what a reader wants
        // where the two are one thing (操作手感, "the feel of the controls").
        assert_eq!(
            terms("操作手感不错"),
            ["操作", "手感", "操作手感", "不错", "手感不错"]
        );
        // A particle ends the chain, so nothing pairs across 了.
        assert_eq!(terms("画面太差了"), ["画面", "太差", "画面太差"]);
        // Modifiers stack, and the pair keeps all of them: "not too friendly" is not "too
        // friendly".
        assert_eq!(terms("不太友好"), ["友好", "不太友好"]);
        // Two lone characters are a word the dictionary lacks; a lone character before a
        // whole word does not lead into it.
        assert_eq!(terms("帧数很低"), ["帧数", "很低"]);
        assert_eq!(terms("画面渣"), ["画面", "画面渣"]);
        assert_eq!(terms("超好玩"), ["好玩", "超好玩"]);
        // The words of the trade are cut as words: 上手 is "to get the hang of", not "up hand".
        assert_eq!(terms("上手难度"), ["上手", "难度", "上手难度"]);
        assert_eq!(terms("掉帧严重"), ["掉帧", "严重", "掉帧严重"]);
    }

    #[test]
    fn a_script_without_a_dictionary_counts_pairs_of_characters() {
        assert_eq!(terms("にほんご"), ["にほ", "ほん", "んご"]);
    }

    #[test]
    fn korean_is_spaced_words_with_the_particle_taken_off() {
        assert_eq!(terms("그래픽이 좋다"), ["그래픽", "좋다", "그래픽 좋다"]);
        assert_eq!(terms("그래픽은"), ["그래픽"]);
        assert_eq!(terms("스토리에서는"), ["스토리"]);
        assert_eq!(terms("환불했습니다"), ["환불"]);
        assert_eq!(terms("환불하고"), ["환불"]);
        // A word that is nothing but a particle keeps itself.
        assert_eq!(terms("이가"), ["이가"]);
    }

    #[test]
    fn a_claim_mentions_a_term_by_the_cut_that_counted_it() {
        assert!(mentions("Constant frame DROPS in town", "frame drops"));
        assert!(mentions("Constant frame DROPS in town", "town"));
        assert!(!mentions("frame rate drops", "frame drops"));
        assert!(!mentions("framedrops", "drops"));
        assert!(mentions("画面很好", "画面"));
        assert!(!mentions("画面很好", "面很"));
    }

    #[test]
    fn a_review_counts_a_term_once_per_side_however_often_it_repeats_it() {
        let mut said = Said::new(1);
        said.note(0, Polarity::Complaint, "stutter everywhere");
        said.note(0, Polarity::Complaint, "constant stutter");
        said.note(0, Polarity::Praise, "stutter aside, it looks great");
        said.next_review("english");
        let sides = &said.languages["english"];
        assert_eq!(sides[0][1].terms["stutter"], 1);
        assert_eq!(sides[0][1].reviews, 1);
        assert_eq!(sides[0][0].terms["stutter"], 1);
        assert_eq!(sides[0][0].reviews, 1);
    }

    #[test]
    fn the_names_elsewhere_belong_to_subjects_the_sheet_has_and_are_cut_as_terms() {
        let mut wrong = Vec::new();
        for (subject, names) in ALSO_CALLED {
            assert!(
                crate::taxonomy::SHEET
                    .iter()
                    .any(|category| category.id == *subject),
                "{subject} is not on the sheet"
            );
            for name in *names {
                // A name the counter would never produce as a term can never be kept off.
                let mut cut = Vec::new();
                each_term(name, |term| cut.push(term.to_owned()));
                if !cut.iter().any(|term| term == name) {
                    wrong.push(format!("{name} under {subject} is cut as {cut:?}"));
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn a_subjects_name_in_another_language_is_not_a_finding_either() {
        let mut said = Said::new(1);
        for turn in 0..120 {
            if turn % 4 == 0 {
                said.note(0, Polarity::Complaint, "концовка слабая");
            } else {
                said.note(0, Polarity::Praise, "сюжет отличный");
            }
            said.next_review("russian");
        }
        let found = said.finish(&[("story", "Story and writing")]);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        assert!(!praised.contains(&"сюжет"), "{praised:?}");
        // The phrase keeps the name, as "no bugs" keeps "bugs": it says more than the row.
        assert!(
            praised.iter().any(|term| term.contains("отличный")),
            "{praised:?}"
        );
    }

    #[test]
    fn a_language_that_leans_one_way_does_not_make_its_words_distinctive() {
        // Spanish speakers praise the story three times as often as they complain about it,
        // and every one of them calls it "historia" either way. Against the whole other side
        // the word is twelve times likelier in praise; within Spanish it is even, and even is
        // what it is.
        let mut said = Said::new(1);
        for turn in 0..120 {
            if turn % 4 == 0 {
                said.note(0, Polarity::Complaint, "la historia es floja");
            } else {
                said.note(0, Polarity::Praise, "la historia es buena");
            }
            said.next_review("spanish");
        }
        for turn in 0..200 {
            if turn % 2 == 0 {
                said.note(0, Polarity::Complaint, "the ending is weak");
            } else {
                said.note(0, Polarity::Praise, "gripping from start to finish");
            }
            said.next_review("english");
        }
        let found = said.finish(&[("story", "Story and writing")]);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        assert!(
            !praised.contains(&"historia"),
            "a Spanish word stood out for being Spanish: {praised:?}"
        );
        // What English complaints say and English praise does not is still found.
        let criticised: Vec<&str> = found[0]
            .criticised
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert!(criticised.contains(&"ending"), "{criticised:?}");
        assert!(criticised.contains(&"weak"), "{criticised:?}");
    }

    #[test]
    fn what_stands_out_is_what_one_side_says_and_the_other_does_not() {
        let mut said = Said::new(1);
        for review in 0..100 {
            said.note(0, Polarity::Praise, "the game runs smooth for me");
            if review < 40 {
                said.note(0, Polarity::Complaint, "the game stutters in the city");
            } else {
                said.note(0, Polarity::Complaint, "the game crashed once");
            }
            said.next_review("english");
        }
        let found = said.finish(&[("performance", "Performance")]);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        let criticised: Vec<&str> = found[0]
            .criticised
            .iter()
            .map(|t| t.text.as_str())
            .collect();

        assert!(praised.iter().any(|t| t.contains("smooth")), "{praised:?}");
        assert!(
            criticised.iter().any(|t| t.contains("stutters")),
            "{criticised:?}"
        );
        assert!(
            criticised.iter().any(|t| t.contains("crashed")),
            "{criticised:?}"
        );
        // Said equally often on both sides, so it says nothing about either.
        for common in ["the", "game", "the game"] {
            assert!(!praised.contains(&common), "{praised:?}");
            assert!(!criticised.contains(&common), "{criticised:?}");
        }
        assert_eq!(found[0].praising, 100);
        assert_eq!(found[0].complaining, 100);
        let stutters = found[0]
            .criticised
            .iter()
            .find(|t| t.text.contains("stutters"))
            .unwrap();
        assert_eq!(stutters.reviews, 40);
        // The commoner complaint stands out more clearly than the rarer one.
        let crashed = found[0]
            .criticised
            .iter()
            .position(|t| t.text.contains("crashed"))
            .unwrap();
        let stuttered = found[0]
            .criticised
            .iter()
            .position(|t| t.text.contains("stutters"))
            .unwrap();
        assert!(crashed < stuttered);
    }

    #[test]
    fn a_side_with_too_few_reviews_shows_nothing_rather_than_its_accidents() {
        let mut said = Said::new(1);
        said.note(0, Polarity::Complaint, "laggy menus");
        said.next_review("english");
        said.note(0, Polarity::Complaint, "laggy menus");
        said.next_review("english");
        for _ in 0..50 {
            said.note(0, Polarity::Praise, "buttery smooth");
            said.next_review("english");
        }
        let found = said.finish(&[("performance", "Performance")]);
        assert!(found[0].criticised.is_empty(), "{:?}", found[0].criticised);
    }

    #[test]
    fn a_phrase_and_a_word_inside_it_are_shown_once() {
        let mut said = Said::new(1);
        for _ in 0..60 {
            said.note(0, Polarity::Complaint, "frame drops");
            said.note(0, Polarity::Praise, "looks lovely");
            said.next_review("english");
        }
        let found = said.finish(&[("performance", "Performance")]);
        let criticised: Vec<&str> = found[0]
            .criticised
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(criticised.len(), 1, "{criticised:?}");
    }

    #[test]
    fn two_forms_of_one_word_are_shown_once_in_the_form_more_reviewers_used() {
        let mut said = Said::new(1);
        // The filler changes with the review so that no phrase survives the cut and the two
        // forms of the word stand alone, which is the shape a real row has.
        for review in 0..200 {
            said.note(0, Polarity::Praise, &format!("{review} they are listening"));
            if review < 120 {
                said.note(0, Polarity::Praise, &format!("{review} they listened"));
            }
            said.note(
                0,
                Polarity::Complaint,
                &format!("{review} roadmap is empty"),
            );
            said.next_review("english");
        }
        let found = said.finish(&[("updates", "Updates and developer support")]);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            praised.iter().filter(|t| t.contains("listen")).count(),
            1,
            "{praised:?}"
        );
        assert!(praised.contains(&"listening"), "{praised:?}");
    }

    #[test]
    fn words_that_only_start_the_same_are_two_findings() {
        for (left, right) in [
            ("mode", "mods"),
            ("car", "card"),
            ("part", "party"),
            ("play", "player"),
            ("服务", "服务器"),
        ] {
            assert!(!one_word_inflected(left, right), "{left} and {right}");
            assert!(!one_word_inflected(right, left), "{right} and {left}");
        }
    }

    #[test]
    fn one_word_in_two_forms_is_recognised_whichever_way_round_it_comes() {
        for (left, right) in [
            ("server", "servers"),
            ("listened", "listening"),
            ("story", "stories"),
            ("fix", "fixes"),
            ("no bug", "no bugs"),
        ] {
            assert!(one_word_inflected(left, right), "{left} and {right}");
            assert!(one_word_inflected(right, left), "{right} and {left}");
        }
    }

    #[test]
    fn forgetting_keeps_the_commonest_and_records_the_most_it_dropped() {
        let mut counter = Counter::default();
        for _ in 0..5 {
            counter.add("common".to_owned());
        }
        for n in 0..(KEPT * 2) {
            counter.add(format!("rare{n}"));
        }
        assert!(counter.terms.len() < KEPT * 2);
        assert_eq!(counter.terms["common"], 5);
        assert_eq!(counter.forgotten, 1);
    }

    fn deviations(here: u64, of: u64, there: u64, of_other: u64) -> f64 {
        let (delta, sigma) = log_odds(here, of, there, of_other);
        delta / sigma
    }

    #[test]
    fn log_odds_prefer_the_well_attested_over_the_merely_absent_elsewhere() {
        let few = deviations(3, 100, 0, 100);
        let many = deviations(300, 1000, 10, 1000);
        assert!(many > few, "{many} vs {few}");
        assert!(deviations(90, 100, 85, 100) < CLEARLY);
    }

    #[test]
    fn pooling_across_languages_is_the_one_language_alone_where_the_others_are_silent() {
        let mut english = [Counter::default(), Counter::default()];
        english[0].reviews = 1000;
        english[0].terms.insert("stutter".to_owned(), 300);
        english[1].reviews = 1000;
        english[1].terms.insert("stutter".to_owned(), 10);
        let mut spanish = [Counter::default(), Counter::default()];
        spanish[0].reviews = 200;
        spanish[1].reviews = 50;
        let alone = pooled_log_odds("stutter", &[(&english[0], &english[1])]);
        let with_a_silent_language = pooled_log_odds(
            "stutter",
            &[(&english[0], &english[1]), (&spanish[0], &spanish[1])],
        );
        assert_eq!(alone, with_a_silent_language);
        let (delta, sigma) = log_odds(300, 1000, 10, 1000);
        assert!((alone.0 - delta).abs() < 1e-9);
        assert!((alone.1 - delta / sigma).abs() < 1e-9);
    }

    #[test]
    fn a_word_of_the_language_is_not_a_finding_however_significant_its_lean() {
        // "and" in 42% of a thousand praising reviews against 30% of five hundred complaining
        // ones is a real difference and says nothing: praise runs longer. It fails the twice
        // bar. A word used across every subject fails the ownership bar even where it leans.
        let (delta, _) = log_odds(420, 1000, 150, 500);
        let z = deviations(420, 1000, 150, 500);
        assert!(z >= CLEARLY, "{z}");
        assert!(delta < AT_LEAST_TWICE, "{delta}");

        let subjects = [
            ("gameplay", "Gameplay"),
            ("audio", "Audio"),
            ("story", "Story"),
            ("graphics", "Graphics"),
            ("controls", "Controls"),
        ];
        let praised_for = ["combat", "music", "plot", "colours", "keybinds"];
        let mut said = Said::new(subjects.len());
        for review in 0..200 {
            for (slot, aspect) in praised_for.iter().enumerate() {
                said.note(
                    slot,
                    Polarity::Praise,
                    &format!("you will love the {aspect}"),
                );
                if review % 4 == 0 {
                    said.note(slot, Polarity::Complaint, "you might hate it though");
                }
            }
            said.next_review("english");
        }
        let found = said.finish(&subjects);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        assert!(praised.iter().any(|t| t.contains("combat")), "{praised:?}");
        // "love" leans entirely to praise and clears every bar but one: it is a fifth of
        // every subject's praise, so it belongs to none of them.
        assert!(!praised.iter().any(|t| t.contains("love")), "{praised:?}");
        assert!(!praised.iter().any(|t| t.contains("you")), "{praised:?}");
    }

    #[test]
    fn a_subject_never_stands_out_for_its_own_name_unless_the_name_is_in_a_phrase() {
        let mut said = Said::new(1);
        for review in 0..100 {
            said.note(0, Polarity::Praise, "no bugs at all, the graphics shine");
            if review < 30 {
                said.note(0, Polarity::Complaint, "bugs, bugs, bugs");
            }
            said.next_review("english");
        }
        let found = said.finish(&[("bugs", "Bugs and crashes")]);
        let praised: Vec<&str> = found[0].praised.iter().map(|t| t.text.as_str()).collect();
        assert!(praised.contains(&"no bugs"), "{praised:?}");
        assert!(!praised.contains(&"bugs"), "{praised:?}");
        assert!(found[0].criticised.is_empty(), "{:?}", found[0].criticised);
    }

    #[test]
    fn the_phrase_takes_the_place_of_a_word_nearly_always_said_inside_it() {
        let mut said = Said::new(1);
        for review in 0..100 {
            said.note(0, Polarity::Praise, "cheap enough");
            said.note(
                0,
                Polarity::Complaint,
                if review < 97 {
                    "full price is a joke"
                } else {
                    "the full game is a joke"
                },
            );
            said.next_review("english");
        }
        let found = said.finish(&[("price", "Price and value")]);
        let criticised: Vec<&str> = found[0]
            .criticised
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert!(criticised.contains(&"full price"), "{criticised:?}");
        assert!(!criticised.contains(&"full"), "{criticised:?}");
    }

    #[test]
    fn chinese_words_said_together_by_the_same_reviewers_are_shown_as_the_phrase() {
        let mut said = Said::new(1);
        for _ in 0..30 {
            said.note(0, Polarity::Complaint, "没有中文配音");
            said.note(0, Polarity::Praise, "very good indeed");
            said.next_review("english");
        }
        let found = said.finish(&[("language", "Language and localisation")]);
        let criticised: Vec<&str> = found[0]
            .criticised
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        // 没有 is "there is no", a function word, and no pair bridges it to the noun. The
        // two words after it were said by exactly the same reviewers, so the phrase stands
        // for both of them, as "full price" stands for "full".
        assert_eq!(criticised, ["中文配音"]);
    }

    #[test]
    fn character_pairs_used_by_the_same_reviewers_are_joined_back_into_the_word() {
        let mut said = Said::new(1);
        for _ in 0..30 {
            said.note(0, Polarity::Complaint, "にほんごがない");
            said.note(0, Polarity::Praise, "very good indeed");
            said.next_review("english");
        }
        let found = said.finish(&[("language", "Language and localisation")]);
        let criticised: Vec<&str> = found[0]
            .criticised
            .iter()
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(criticised, ["にほんごがない"]);
    }
}
