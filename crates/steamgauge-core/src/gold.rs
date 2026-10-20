//! The claims a person adjudicates, and the page they do it on.
//!
//! Everything measured so far is a model agreeing with a model. The docs say so and no
//! citation can rest on it. What turns a silver set into a gold one is a person reading the
//! claims themselves, and the only reason that has not happened is that there was nothing to
//! read them on.
//!
//! Two draws, because they answer different questions:
//!
//! - **Blind.** A random sample of held-out claims with no answer shown, which is the only kind
//!   of reading that produces an accuracy figure rather than a ratification. Every held-out claim
//!   is a candidate: whether a second labeller happened to reach it is a fact about scheduling,
//!   and a sample that depends on it is a sample of the schedule. Some of what it draws will be
//!   claims both labellers already answered the same way, in their true proportion, and scoring
//!   those apart afterwards is what checks the assumption the whole silver standard rests on:
//!   that two labellers agreeing means both were right.
//! - **Split.** The claims two labellers answered differently, with both answers shown, which
//!   is what settles a boundary rather than measuring one.
//!
//! The page is one self-contained file that fetches nothing and sends nothing. It holds review
//! text, so it is written outside the repository and never committed, exactly like a report.
//!
//! Each person's answers are kept in a directory of their own, `gold/<name>` inside a set, as
//! each labelling model's are: two people answering the same claim is the one measure of how
//! far a person's answer is itself the truth, and it needs both answers kept.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    claimset::{ClaimLabel, DrawnReview},
    measure::{Role, SPLIT_SEED, role},
    taxonomy::SHEET,
};

/// The directory inside each set that holds the people's answers, one directory per person.
pub const GOLD: &str = "gold";

/// A person's name as it names their directory: lowercase letters, digits and hyphens, as a
/// GitHub handle is written.
///
/// # Errors
///
/// Refuses anything else. The name becomes a path, and one that climbs out of the set or
/// differs only in case on one system and not another files answers where nobody looks.
pub fn person(name: &str) -> Result<&str> {
    let fits = !name.is_empty()
        && name.len() <= 39
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if fits {
        Ok(name)
    } else {
        Err(Error::Refused(format!(
            "{name:?} cannot name a person's answers: use lowercase letters, digits and hyphens, \
             as a GitHub handle is written"
        )))
    }
}

/// Where `who` keeps their answers for the set in `set`.
#[must_use]
pub fn answers_of(set: &Path, who: &str) -> PathBuf {
    set.join(GOLD).join(who)
}

/// One claim put in front of a person, with everything needed to judge it and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub app_id: u32,
    pub review_id: String,
    pub index: u16,
    /// The claim itself.
    pub claim: String,
    /// The review it came from, split around the claim rather than sent whole with an offset
    /// into it. An offset would be in bytes here and in UTF-16 units in the page that reads
    /// it, and on the third of this corpus that is not English those are different numbers:
    /// the highlight would cover the whole review instead of one sentence of it.
    pub before: String,
    pub after: String,
    pub language: String,
    /// What the labellers said, shown only for a split: `None` on a blind question, because an
    /// answer on the page is an answer in the reader's head.
    pub shown: Option<Vec<Answered>>,
    /// What the person themselves said the first time, on a re-judgement. Shown beside the
    /// labellers' answers and the sheet's rule for each, so what is being asked is whether
    /// they accept the rule, not what they think of the claim cold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub was: Option<Answered>,
    /// A subject a reader gave where the person gave another, on a question asking whether it
    /// would also do. Shown beside the person's own answer and both rules; the answer is yes
    /// or no, and it is kept apart from the gold label, which it does not change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offered: Option<String>,
}

/// One labeller's answer to a question, as it is shown on a split.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Answered {
    pub subject: String,
    pub polarity: String,
    pub confidence: String,
    pub ambiguous: bool,
}

/// What a draw turned out to be.
#[derive(Debug, Clone, Default)]
pub struct GoldDraw {
    pub blind: usize,
    pub split: usize,
    pub games: usize,
    /// Claims the two labellers answered the same way, which are not worth a person's time:
    /// counted so the page can say what share of the set was never in question.
    pub agreed: usize,
    /// How many of the blind sample are claims both labellers already answered the same way.
    /// Not a separate draw: a sample over every held-out claim contains them in their true
    /// proportion, and scoring them apart afterwards is what tests the assumption the rest of
    /// the set rests on, that two labellers agreeing means both were right.
    pub settled: usize,
    /// Disagreements where neither labeller hedged. Two readers who were both sure and still
    /// answered differently have found either a real error or a boundary the sheet does not
    /// draw, and there is nothing else in the set with that much in it per claim read.
    pub contested_sure: usize,
    /// Claims held back because they are a template option the reviewer left blank. Counted
    /// rather than dropped quietly: the number is how much of the reference set was cut before
    /// the splitter learnt to collapse a ballot, and it is the same fragments the reader is
    /// being trained on.
    pub declined: usize,
    /// Claims held back because this build's splitter no longer cuts the span they name. The
    /// sets were cut by four splitters over the run, and a label from an earlier one can name
    /// a template heading since joined to its option, or a comma list since taken apart. The
    /// words are still in the review and the label is not wrong about them, but there is no
    /// claim there any more to adjudicate and no reading that could be joined to the answer.
    pub recut: usize,
    /// Claims held back because they are written in a script the language they are tagged
    /// with does not use. The language filter reads Steam's tag, which is the reviewer's
    /// account setting and not the text: a review tagged English and written in Hangul was
    /// put in front of somebody who reads English, and a question they cannot read costs the
    /// same minute as a real one and collects noise.
    pub mistagged: usize,
    /// Which languages the person was asked about, empty for all of them. A sample restricted
    /// to what the adjudicator reads is a random sample of those languages and not of the
    /// corpus, and the figure it produces has to say so.
    pub languages: Vec<String>,
    /// Which boundaries the disagreements were narrowed to, empty for all of them. Only the
    /// splits are aimed; the blind sample stays a random sample of the held-out games, because
    /// aiming it would make the accuracy figure a fact about the boundaries that were chosen.
    pub boundaries: Vec<(String, String)>,
    /// Disagreements left out because they fall on a boundary that was not asked for.
    pub elsewhere: usize,
}

impl GoldDraw {
    /// Whether a label is one nobody should be asked about, counted under why.
    ///
    /// Nothing anybody can adjudicate is not a hard question, it is a broken one. It costs
    /// the person the same time as a real claim and the answer it collects is worth nothing
    /// either way. Three shapes: a template option its author left blank, a claim written in
    /// a script its tag does not use where a language was asked for, and a span this build
    /// cuts no claim at.
    fn holds_back(
        &mut self,
        label: &ClaimLabel,
        text: &str,
        by_language: bool,
        cut: &crate::claimset::CutSpans,
    ) -> bool {
        if crate::claims::is_not_a_claim(text) {
            self.declined += 1;
        } else if by_language && written_in_another_script(&label.language, text) {
            self.mistagged += 1;
        } else if !still_cut(cut, label) {
            self.recut += 1;
        } else {
            return false;
        }
        true
    }
}

/// Which games' disagreements to put in front of a person.
///
/// The blind sample is always held out, because that is what makes it a measurement. The splits
/// measure nothing: they settle a boundary, and a boundary settled on one game is settled for
/// every game, so they may be drawn from anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Splits {
    None,
    HeldOut,
    Everywhere,
}

/// What a person is asked, past where the sets and captures are.
#[derive(Debug, Clone, Copy)]
pub struct Asked<'a> {
    /// How many claims to draw blind.
    pub blind: usize,
    pub splits: Splits,
    pub seed: u64,
    /// Only claims in these languages, empty for every language.
    pub languages: &'a [String],
    /// Which second reading the disagreements come from.
    pub reading: &'a str,
    /// Only disagreements on these boundaries, empty for every boundary.
    pub boundaries: &'a [(String, String)],
}

/// Where a claim is, so the blind draw and the disagreements cannot both take the same one.
type At = (u32, String, u16);
/// A held-out claim that could be asked blind: its draw order, where it is, whether both
/// labellers answered it the same way, and the question itself.
type Candidate = ([u8; 32], At, bool, Question);
/// A disagreement: where it is, whether either labeller hedged, and the question.
type Contested = (At, bool, Question);

/// One game's draw, its labels and the second reading's, or nothing where the game has no
/// drawn sample or has never been labelled.
type Set = (Vec<DrawnReview>, Vec<ClaimLabel>, Vec<ClaimLabel>);

fn read_set(dir: &Path, reading: &str) -> Result<Option<Set>> {
    let (Ok(sample), Ok(first)) = (
        std::fs::read(dir.join("sample.json")),
        std::fs::read(dir.join("labels.json")),
    ) else {
        return Ok(None);
    };
    // A set with no second reading is still drawn from: its claims can be asked blind, and
    // only its disagreements are missing, because there is no second answer to differ from.
    let second = std::fs::read(dir.join(reading).join("labels.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default();
    Ok(Some((
        serde_json::from_slice(&sample)?,
        serde_json::from_slice(&first)?,
        second,
    )))
}

/// What one game's labels are looked up against: where this build cuts claims, each claim as
/// the labeller was handed it, and the second reading, both keyed by claim.
type Indexed<'a> = (
    crate::claimset::CutSpans,
    std::collections::HashMap<(&'a str, u16), &'a str>,
    std::collections::HashMap<(&'a str, u16), &'a ClaimLabel>,
);

fn indexed<'a>(
    captures: &Path,
    app_id: u32,
    drawn: &'a [DrawnReview],
    first: &[ClaimLabel],
    second: &'a [ClaimLabel],
) -> Result<Indexed<'a>> {
    let labelled: std::collections::HashSet<String> =
        first.iter().map(|label| label.review_id.clone()).collect();
    Ok((
        crate::claimset::spans_cut_now(captures, app_id, &labelled)?,
        drawn
            .iter()
            .flat_map(|review| {
                review
                    .claims
                    .iter()
                    .map(move |claim| ((review.id.as_str(), claim.index), claim.text.as_str()))
            })
            .collect(),
        second
            .iter()
            .map(|label| ((label.review_id.as_str(), label.index), label))
            .collect(),
    ))
}

/// Whether a disagreement falls outside the boundaries asked for, counted as it goes past.
///
/// A person settles a boundary, and the boundaries are not equally expensive: four of them
/// carry a third of every disagreement the two readings produce. Asking about those four in
/// one sitting settles more of the sheet per question than a draw that samples all of them,
/// and the ones left out are still there to ask about next time.
fn elsewhere(wanted: &[(String, String)], first: &str, second: &str, passed: &mut usize) -> bool {
    if wanted.is_empty() {
        return false;
    }
    let asked = wanted.iter().any(|(left, right)| {
        (left == first && right == second) || (left == second && right == first)
    });
    *passed += usize::from(!asked);
    !asked
}

/// Whether a labeller signalled doubt, by any of the three means the sheet gives them.
fn hedged(label: &ClaimLabel) -> bool {
    label.ambiguous || label.split_wrong || label.confidence == "low"
}

/// The scripts a Steam language is written in.
///
/// Steam's tag is the reviewer's account setting, not the text, and the two disagree often
/// enough that six of eight labellers reported it independently. Reading the tag is right
/// for the reader, which sees the tag at inference too; it is wrong for a person, who is
/// asked to read the words. Japanese takes Han as well as kana, and Chinese only Han, so a
/// Japanese review tagged Chinese passes and the reverse does not.
fn scripts_of(language: &str) -> &'static [Script] {
    match language {
        "russian" | "ukrainian" | "bulgarian" => &[Script::Cyrillic],
        "greek" => &[Script::Greek],
        "schinese" | "tchinese" => &[Script::Han],
        "japanese" => &[Script::Han, Script::Kana],
        "koreana" => &[Script::Hangul],
        "thai" => &[Script::Thai],
        "arabic" => &[Script::Arabic],
        _ => &[Script::Latin],
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Cyrillic,
    Greek,
    Han,
    Kana,
    Hangul,
    Thai,
    Arabic,
}

/// Which script a letter belongs to, for the scripts Steam reviews arrive in; anything else
/// is not evidence either way.
fn script_of(letter: char) -> Option<Script> {
    Some(match letter as u32 {
        0x0041..=0x024F => Script::Latin,
        0x0370..=0x03FF => Script::Greek,
        0x0400..=0x04FF => Script::Cyrillic,
        0x0600..=0x06FF => Script::Arabic,
        0x0E00..=0x0E7F => Script::Thai,
        0x3040..=0x30FF => Script::Kana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => Script::Han,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Script::Hangul,
        _ => return None,
    })
}

/// Whether a claim's letters are mostly of a script the language it is tagged with does not
/// use. Mostly rather than all, because an English review names a Japanese boss and a Korean
/// one quotes an English error message; and only where there are enough letters to say,
/// since "10/10" is written in every language on Steam.
#[must_use]
pub fn written_in_another_script(language: &str, text: &str) -> bool {
    const ENOUGH_LETTERS: usize = 4;

    let expected = scripts_of(language);
    let (mut theirs, mut others) = (0_usize, 0_usize);
    for letter in text.chars().filter(|ch| ch.is_alphabetic()) {
        match script_of(letter) {
            Some(script) if expected.contains(&script) => theirs += 1,
            Some(_) => others += 1,
            None => {}
        }
    }
    theirs + others >= ENOUGH_LETTERS && others > theirs
}

/// Whether this build still cuts a claim exactly where the label says one is.
///
/// A gold answer is keyed to a span, so a label naming one the splitter has since joined to
/// its neighbour or taken apart describes a claim no reader will ever be handed. Asking about
/// it spends the same minute as a real question and the answer can be joined to nothing.
#[must_use]
pub fn still_cut(cut: &crate::claimset::CutSpans, label: &ClaimLabel) -> bool {
    crate::claimset::cut_at(cut, label).is_some()
}

/// The claim a label names and the review around it, as this build cuts and shows it, or
/// nothing where this build cuts no claim there.
fn as_written<'a>(
    cut: &'a crate::claimset::CutSpans,
    shown: &mut std::collections::HashMap<&'a str, Rejoined>,
    label: &ClaimLabel,
) -> Option<(String, String, String)> {
    let span = crate::claimset::cut_at(cut, label)?;
    let (id, review) = cut.get_key_value(label.review_id.as_str())?;
    let rejoined = shown
        .entry(id.as_str())
        .or_insert_with(|| Rejoined::of(&review.text));
    let (at, claim) = rejoined.find(span)?;
    Some((
        claim.to_owned(),
        rejoined.text[..at].to_owned(),
        rejoined.text[at + claim.len()..].to_owned(),
    ))
}

/// Draws the claims a person should read.
///
/// Held-out games because a gold figure has to be about games that chose nothing: a person
/// adjudicating the games the model trained on would produce a number that flatters every part
/// of the chain at once.
///
/// Every game drawn from needs its capture. The sets were cut by several splitters, so a
/// stored span may be one this build joins to its neighbour or takes apart, and only the
/// review itself can tell which; it is also what shows the review with its own line breaks
/// rather than as the labeller's pieces.
///
/// # Errors
///
/// Fails if a reference set cannot be read, or a game drawn from has no capture.
pub fn draw(
    reference: &Path,
    captures: &Path,
    asked: &Asked<'_>,
) -> Result<(Vec<Question>, GoldDraw)> {
    let Asked {
        blind: blind_wanted,
        splits,
        seed,
        languages,
        reading,
        boundaries,
    } = *asked;
    // Ranked, whether the two labellers had both answered it, and the question itself.
    let mut blind: Vec<Candidate> = Vec::new();
    // Ranked as they are collected: a claim both readers answered without hedging, and still
    // answered differently, is the sharpest question in the set, and a person's time is worth
    // most there. Sorting on it puts those first rather than leaving them in file order.
    let mut split: Vec<Contested> = Vec::new();
    let mut found = GoldDraw::default();

    for entry in std::fs::read_dir(reference)? {
        let dir = entry?.path();
        let Some(app_id) = dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let held_out = role(app_id, SPLIT_SEED) == Role::HeldOut;
        if !held_out && splits != Splits::Everywhere {
            continue;
        }
        let Some((drawn, first, second)) = read_set(&dir, reading)? else {
            continue;
        };
        found.games += usize::from(held_out);
        let (cut, handed, twice) = indexed(captures, app_id, &drawn, &first, &second)?;
        let mut shown = std::collections::HashMap::new();

        for label in &first {
            // A person cannot adjudicate a language they do not read, and a question they cannot
            // answer is worse than one that was never asked: it sits in the count, it cannot be
            // skipped honestly, and whatever they put is noise wearing the one label in this
            // project that is allowed to be called truth.
            if !languages.is_empty() && !languages.iter().any(|one| one == &label.language) {
                continue;
            }
            let Some(text) = handed.get(&(label.review_id.as_str(), label.index)) else {
                continue;
            };
            if found.holds_back(label, text, !languages.is_empty(), &cut) {
                continue;
            }
            let Some((claim, before, after)) = as_written(&cut, &mut shown, label) else {
                continue;
            };
            let question = Question {
                app_id,
                review_id: label.review_id.clone(),
                index: label.index,
                claim,
                before,
                after,
                language: label.language.clone(),
                shown: None,
                was: None,
                offered: None,
            };

            // The same key for both pools, so a control claim lands wherever its review lands
            // rather than in a block of its own. A person who can tell which questions are the
            // control is answering a different question on them.
            let rank =
                crate::bounded::rank(seed, "gold-blind", &format!("{app_id}#{}", label.review_id));

            let read_twice = twice.get(&(label.review_id.as_str(), label.index));
            let both_said_the_same = read_twice.is_some_and(|other| other.subject == label.subject);
            found.agreed += usize::from(both_said_the_same);

            if let Some(other) = read_twice
                && !both_said_the_same
                && splits != Splits::None
                && !elsewhere(
                    boundaries,
                    &label.subject,
                    &other.subject,
                    &mut found.elsewhere,
                )
            {
                split.push((
                    (app_id, label.review_id.clone(), label.index),
                    hedged(label) || hedged(other),
                    Question {
                        shown: Some(vec![answered(label), answered(other)]),
                        ..question.clone()
                    },
                ));
            }

            // Every held-out claim is a candidate, whether or not a second labeller reached it and
            // whether or not the two agreed. Drawing only from claims nobody read twice made the
            // sample a fact about where the second pass had got to. Drawing only from the ones
            // they agreed on is worse in a quieter way: those are the easy claims, and an
            // accuracy figure over them alone is the flattering half of the corpus.
            if !held_out {
                continue;
            }
            blind.push((
                rank,
                (app_id, label.review_id.clone(), label.index),
                both_said_the_same,
                question,
            ));
        }
    }

    found.languages = languages.to_vec();
    found.boundaries = boundaries.to_vec();
    let questions = assemble(blind, split, blind_wanted, &mut found);

    Ok((questions, found))
}

/// Draws every gold answer that differs from the first labeller's, to be judged again with
/// the sheet's rule in view.
///
/// A blind answer measures how often the labels agree with a person reading cold, and that is
/// the only thing it should measure. What it cannot say is why they differ: a rule the sheet
/// lacks, a rule it has that nobody applied, or a claim two readings fit. Shown their own
/// answer, the labellers' and the boundary each category draws, a person either accepts the
/// rule, in which case the first answer was a miss and the label stands, or rejects it, in
/// which case the rule is what has to change. Only the second kind is worth a relabel.
///
/// Answered on the same page and keyed by the same claim, so a re-judgement replaces the
/// first answer at the next ingest and the gold set carries one answer per claim. The page
/// marks an answer given with the rule in view as `rejudged`, which is how it tells one from
/// the first answer sitting in the same file under the same claim, and how the ingest counts
/// them.
///
/// Shown as [`draw`] shows a claim, from the game's capture, and only where this build still
/// cuts it: an answer about a span the splitter has since moved is about no claim at all.
///
/// # Errors
///
/// Fails if a reference set cannot be read, or a game with answers to judge has no capture.
pub fn rejudge(
    reference: &Path,
    captures: &Path,
    reading: &str,
    who: &str,
) -> Result<(Vec<Question>, GoldDraw)> {
    let mut questions = Vec::new();
    let mut found = GoldDraw::default();

    for entry in std::fs::read_dir(reference)? {
        let dir = entry?.path();
        let Some(app_id) = dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let (Ok(first), Ok(gold)) = (
            std::fs::read(dir.join("labels.json")),
            std::fs::read(answers_of(&dir, who).join("labels.json")),
        ) else {
            continue;
        };
        let first: Vec<ClaimLabel> = serde_json::from_slice(&first)?;
        let gold: Vec<ClaimLabel> = serde_json::from_slice(&gold)?;
        let second: Vec<ClaimLabel> = std::fs::read(dir.join(reading).join("labels.json"))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        found.games += 1;

        let reviews: std::collections::HashSet<String> =
            gold.iter().map(|label| label.review_id.clone()).collect();
        let cut = crate::claimset::spans_cut_now(captures, app_id, &reviews)?;
        let mut shown = std::collections::HashMap::new();
        let by_claim =
            |labels: &[ClaimLabel]| -> std::collections::HashMap<(String, u16), Answered> {
                labels
                    .iter()
                    .map(|label| ((label.review_id.clone(), label.index), answered(label)))
                    .collect()
            };
        let (first, second) = (by_claim(&first), by_claim(&second));

        for label in &gold {
            let key = (label.review_id.clone(), label.index);
            let Some(labelled) = first.get(&key) else {
                continue;
            };
            if labelled.subject == label.subject {
                found.agreed += 1;
                continue;
            }
            let Some((claim, before, after)) = as_written(&cut, &mut shown, label) else {
                found.recut += 1;
                continue;
            };
            let mut labellers = vec![labelled.clone()];
            labellers.extend(second.get(&key).cloned());
            questions.push(Question {
                app_id,
                review_id: label.review_id.clone(),
                index: label.index,
                claim,
                before,
                after,
                language: label.language.clone(),
                shown: Some(labellers),
                was: Some(answered(label)),
                offered: None,
            });
        }
    }

    questions
        .sort_by(|a, b| (a.app_id, &a.review_id, a.index).cmp(&(b.app_id, &b.review_id, b.index)));
    found.split = questions.len();
    Ok((questions, found))
}

/// A person's judgement of whether a subject other than their own would also do for a claim.
///
/// Human writing is not one subject a sentence: "then you meet Jar Jar and follow him to the
/// Gungan base" is the story as much as the level. A reader scored only against the person's
/// single answer is counted wrong every time it names the other fair one, which understates it
/// against the one judge whose view is the truth. So the person is asked, once for each subject
/// a reader has actually proposed, and the answer is kept beside the gold label rather than in
/// it: the label is what they would say, this is what they would accept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Acceptable {
    pub review_id: String,
    pub index: u16,
    pub subject: String,
    pub acceptable: bool,
    pub by: String,
}

/// The file the judgements are kept in, beside the person's answers.
pub const ACCEPTABLE: &str = "acceptable.json";

/// The judgements `who` has made for the set in `set`, or none.
#[must_use]
pub fn acceptable_in(set: &Path, who: &str) -> Vec<Acceptable> {
    std::fs::read(answers_of(set, who).join(ACCEPTABLE))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

/// Keeps `judged` with the set's judgements, a later one replacing an earlier one about the
/// same claim and subject. Returns how many are kept in all.
///
/// # Errors
///
/// Fails if the file cannot be written.
pub fn keep_acceptable(set: &Path, who: &str, judged: &[Acceptable]) -> Result<usize> {
    let mut kept = acceptable_in(set, who);
    for one in judged {
        kept.retain(|old| {
            (&old.review_id, old.index, &old.subject) != (&one.review_id, one.index, &one.subject)
        });
        kept.push(one.clone());
    }
    kept.sort_by(|a, b| {
        (&a.review_id, a.index, &a.subject).cmp(&(&b.review_id, b.index, &b.subject))
    });
    let dir = answers_of(set, who);
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(ACCEPTABLE), serde_json::to_vec_pretty(&kept)?)?;
    Ok(kept.len())
}

/// Every gold claim the library's reader answered with a subject other than the person's, and
/// that the person has not yet judged, as a question asking whether that subject would also do.
///
/// The reader's answer is the one the library was read with, so what is asked about is what the
/// tool actually says. A claim the reader declined, or answered as the person did, is not asked.
///
/// # Errors
///
/// Fails if a reference set or a reading cannot be read.
pub fn offered(reference: &Path, library: &Path, who: &str) -> Result<(Vec<Question>, GoldDraw)> {
    let mut questions = Vec::new();
    let mut found = GoldDraw::default();

    for entry in std::fs::read_dir(reference)? {
        let dir = entry?.path();
        let Some(app_id) = dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(gold) = std::fs::read(answers_of(&dir, who).join("labels.json")) else {
            continue;
        };
        let Ok(snapshot) = crate::embed::latest_snapshot(library, app_id) else {
            continue;
        };
        let gold: Vec<ClaimLabel> = serde_json::from_slice(&gold)?;
        let judged: std::collections::HashSet<(String, u16, String)> = acceptable_in(&dir, who)
            .into_iter()
            .map(|one| (one.review_id, one.index, one.subject))
            .collect();

        let wanted: std::collections::HashSet<&str> =
            gold.iter().map(|label| label.review_id.as_str()).collect();
        let mut read: std::collections::HashMap<(String, u32, u32), String> =
            std::collections::HashMap::new();
        crate::read::for_each_full_reading(
            &snapshot.join("readings.parquet"),
            |id, (start, end), subject, _, _, _| {
                if let Some(subject) = subject.filter(|_| wanted.contains(id)) {
                    read.insert((id.to_owned(), start, end), subject.to_owned());
                }
            },
        )?;
        found.games += 1;

        let wanted: std::collections::HashSet<String> =
            wanted.into_iter().map(str::to_owned).collect();
        let cut = crate::claimset::spans_cut_now(library, app_id, &wanted)?;
        let mut shown = std::collections::HashMap::new();
        for label in &gold {
            let Some(said) = read.get(&(label.review_id.clone(), label.start, label.end)) else {
                continue;
            };
            if *said == label.subject {
                found.agreed += 1;
                continue;
            }
            if judged.contains(&(label.review_id.clone(), label.index, said.clone())) {
                continue;
            }
            let Some((claim, before, after)) = as_written(&cut, &mut shown, label) else {
                found.recut += 1;
                continue;
            };
            questions.push(Question {
                app_id,
                review_id: label.review_id.clone(),
                index: label.index,
                claim,
                before,
                after,
                language: label.language.clone(),
                shown: None,
                was: Some(answered(label)),
                offered: Some(said.clone()),
            });
        }
    }

    questions
        .sort_by(|a, b| (a.app_id, &a.review_id, a.index).cmp(&(b.app_id, &b.review_id, b.index)));
    found.split = questions.len();
    Ok((questions, found))
}

/// Takes the blind sample first, then leaves the disagreements whatever it did not claim.
///
/// Order matters and it is the whole point. Letting the disagreements go first leaves the blind
/// sample drawn only from claims the two labellers agreed on, which are the easy ones, and an
/// accuracy figure over those alone is the flattering half of the corpus.
fn assemble(
    mut blind: Vec<Candidate>,
    mut split: Vec<Contested>,
    blind_wanted: usize,
    found: &mut GoldDraw,
) -> Vec<Question> {
    blind.sort_by_key(|(key, _, _, _)| *key);
    blind.truncate(blind_wanted);
    found.blind = blind.len();
    found.settled = blind.iter().filter(|(_, _, same, _)| *same).count();

    let asked_blind: std::collections::HashSet<&At> =
        blind.iter().map(|(_, at, _, _)| at).collect();
    split.retain(|(at, _, _)| !asked_blind.contains(at));
    found.split = split.len();

    let (sharp, doubted): (Vec<Contested>, Vec<Contested>) =
        split.into_iter().partition(|(_, hedged, _)| !hedged);
    found.contested_sure = sharp.len();

    // Sharpest first, then the blind sample, then the rest. Two readers who were both sure and
    // still disagreed are the shortest list worth anyone's time, and there are only ever a few
    // dozen of them; asking them after a thousand blind claims is asking them of somebody who
    // has stopped. Putting them first costs the blind sample nothing, because a prefix of a
    // randomly ordered sample is still a random sample, so whatever of it gets answered stands
    // on its own.
    let mut questions: Vec<Question> = sharp.into_iter().map(|(_, _, q)| q).collect();
    questions.extend(blind.into_iter().map(|(_, _, _, q)| q));
    questions.extend(doubted.into_iter().map(|(_, _, q)| q));
    questions
}

/// A review as this build cuts it, put back together, and where each claim lands in the result.
///
/// Each claim reads as the reader is handed it, with markup taken out, and the claims stand
/// apart as the reviewer set them: on a line of their own where a line break came between
/// them, a paragraph apart where a blank line did, a space apart otherwise. Run together, a
/// heading, a list or a ballot reads as one long sentence, and the person cannot see why the
/// claim in front of them ends where it does.
struct Rejoined {
    text: String,
    /// Where the claim sits in the review, its byte offset into `text`, and its length there.
    claims: Vec<(crate::claims::Span, usize, usize)>,
}

impl Rejoined {
    fn of(review: &str) -> Self {
        let mut text = String::new();
        let mut claims = Vec::new();
        let mut last = None;
        for (at, claim) in crate::claims::claims_of(review) {
            let (Ok(start), Ok(end)) = (u32::try_from(at.start), u32::try_from(at.end)) else {
                continue;
            };
            // Each box ticked under one heading is a claim that starts at the heading, so two
            // claims can share their first bytes: those stand a line apart, as the options do.
            if let Some(last) = last {
                match review
                    .get(last..at.start)
                    .map(|gap| gap.matches('\n').count())
                {
                    Some(0) => text.push(' '),
                    Some(1) | None => text.push('\n'),
                    Some(_) => text.push_str("\n\n"),
                }
            }
            claims.push(((start, end), text.len(), claim.len()));
            text.push_str(&claim);
            last = Some(at.end);
        }
        Self { text, claims }
    }

    fn find(&self, span: crate::claims::Span) -> Option<(usize, &str)> {
        self.claims
            .iter()
            .find(|(at, _, _)| *at == span)
            .map(|&(_, at, len)| (at, &self.text[at..at + len]))
    }
}

fn answered(label: &ClaimLabel) -> Answered {
    Answered {
        subject: label.subject.clone(),
        polarity: label.polarity.clone(),
        confidence: label.confidence.clone(),
        ambiguous: label.ambiguous,
    }
}

const STYLE: &str = include_str!("gold.css");
const SCRIPT: &str = include_str!("gold.js");

/// Renders the page `who` adjudicates on: one file, fetching nothing, sending nothing. Every
/// answer it exports carries their name, so a file handed back is filed under the person it
/// was made for whoever runs the ingest.
#[must_use]
pub fn render(questions: &[Question], found: &GoldDraw, who: &str) -> String {
    let categories: Vec<serde_json::Value> = SHEET
        .iter()
        .map(|category| {
            serde_json::json!({
                "id": category.id,
                "label": category.label,
                "description": category.description,
                "boundary": category.boundary,
            })
        })
        .collect();

    let data = serde_json::json!({
        "questions": questions,
        "categories": categories,
        "taxonomy": crate::taxonomy::sheet(),
        "blind": found.blind,
        "settled": found.settled,
        "split": found.split,
        "games": found.games,
        "agreed": found.agreed,
        "languages": found.languages,
        "for": who,
    });

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Adjudicate | SteamGauge</title>\n<style>{STYLE}</style>\n</head>\n<body>\n\
         <div id=\"app\"></div>\n\
         <script id=\"data\" type=\"application/json\">{}</script>\n\
         <script>{SCRIPT}</script>\n</body>\n</html>\n",
        serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_owned())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The usual draw: every language, the `second` reading, every boundary.
    fn asking(blind: usize, splits: Splits) -> Asked<'static> {
        Asked {
            blind,
            splits,
            seed: 1,
            languages: &[],
            reading: "second",
            boundaries: &[],
        }
    }

    /// A review this build cuts into three claims.
    const THREE: &str = "The game runs badly on my laptop. The art style looks great to me. It is worth the full price.";

    /// A capture of `app_id` under `out` holding these reviews, as a crawl leaves one.
    fn captured(out: &Path, app_id: u32, reviews: &[(&str, &str)]) {
        let snapshot = out.join(format!("appid={app_id}")).join("snapshot=1");
        std::fs::create_dir_all(&snapshot).unwrap();
        let rows: Vec<serde_json::Value> = reviews
            .iter()
            .map(|(id, text)| {
                serde_json::json!({"recommendationid": id, "review": text,
                    "language": "english", "timestamp_created": 1, "timestamp_updated": 1,
                    "voted_up": true, "votes_up": 0})
            })
            .collect();
        let mut writer =
            crate::capture::CaptureWriter::create(&snapshot.join("shard-0000.parquet"), 1).unwrap();
        writer.write(&rows.iter().collect::<Vec<_>>()).unwrap();
        writer.close().unwrap();
    }

    /// Where this build cuts claim `index` of `text`.
    fn span_of(text: &str, index: usize) -> (usize, usize) {
        let (at, _) = &crate::claims::claims_of(text)[index];
        (at.start, at.end)
    }

    /// A review as a draw records it, cut by this build.
    fn drawn_from(id: &str, app_id: u32, subset: &str, text: &str) -> serde_json::Value {
        let claims: Vec<serde_json::Value> = crate::claims::claims_of(text)
            .into_iter()
            .enumerate()
            .map(|(index, (at, claim))| {
                serde_json::json!({"index": index, "start": at.start, "end": at.end, "text": claim})
            })
            .collect();
        serde_json::json!({"id": id, "app_id": app_id, "language": "english",
            "subset": subset, "claims": claims})
    }

    /// The first labeller's label of claim `index` of review `r1`, which reads `text`.
    fn label_of(app_id: u32, text: &str, index: u16, subject: &str) -> serde_json::Value {
        let (start, end) = span_of(text, usize::from(index));
        serde_json::json!({
            "review_id": "r1", "index": index, "app_id": app_id,
            "language": "english", "subset": "random", "start": start, "end": end,
            "taxonomy": "a-sheet", "produced_by": "one",
            "subject": subject, "polarity": "praise", "ironic": false,
            "confidence": "high", "ambiguous": false, "split_wrong": false
        })
    }

    /// A reference set on disk, with the captures it was drawn from under `data`: one held-out
    /// game and one training game, each with three claims. The second labeller reads two of
    /// them, agreeing on one and differing on the other, and never sees the third, which is
    /// therefore the only one left to read blind.
    fn a_reference_set(root: &Path) {
        for app_id in [214_490_u32, 296_970_u32] {
            let dir = root.join(app_id.to_string());
            std::fs::create_dir_all(dir.join("second")).unwrap();
            captured(&root.join("data"), app_id, &[("r1", THREE)]);

            let drawn = serde_json::json!([drawn_from("r1", app_id, "random", THREE)]);
            std::fs::write(dir.join("sample.json"), drawn.to_string()).unwrap();

            let label = |index: u16, subject: &str| label_of(app_id, THREE, index, subject);
            std::fs::write(
                dir.join("labels.json"),
                serde_json::json!([
                    label(0, "performance"),
                    label(1, "graphics"),
                    label(2, "verdict")
                ])
                .to_string(),
            )
            .unwrap();
            std::fs::write(
                dir.join("second").join("labels.json"),
                serde_json::json!([label(0, "performance"), label(2, "price")]).to_string(),
            )
            .unwrap();
        }
    }

    #[test]
    fn a_blind_draw_is_held_out_games_only_and_a_split_can_come_from_anywhere() {
        // 214490 is held out and 296970 trains, as the trainer places them.
        assert_eq!(role(214_490, SPLIT_SEED), Role::HeldOut);
        assert_ne!(role(296_970, SPLIT_SEED), Role::HeldOut);

        let root = std::env::temp_dir().join(format!("steamgauge-gold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);

        let (held_out_only, counts) =
            draw(&root, &root.join("data"), &asking(100, Splits::HeldOut)).unwrap();
        assert!(
            held_out_only
                .iter()
                .all(|question| question.app_id == 214_490),
            "a training game reached a draw that measures the model"
        );
        assert_eq!(
            counts.blind, 3,
            "the blind sample is every held-out claim, whether or not a second labeller reached \
             it and whether or not the two agreed"
        );
        assert_eq!(
            counts.settled, 1,
            "one of the three is a claim both labellers answered the same way, which is the \
             control, counted rather than drawn separately"
        );
        assert_eq!(
            counts.split, 0,
            "a blind sample large enough to take every held-out claim leaves the held-out game no \
             disagreement to show, because the same claim cannot be asked both ways"
        );
        assert_eq!(counts.agreed, 1);
        assert_eq!(
            counts.games, 1,
            "only the held-out game counts as a game read"
        );

        let (everywhere, wider) =
            draw(&root, &root.join("data"), &asking(100, Splits::Everywhere)).unwrap();
        assert_eq!(
            wider.split, 1,
            "the training game's disagreement was left out; the held-out game's own went into the \
             blind sample instead, which takes what it needs before the disagreements are cut"
        );
        assert_eq!(
            wider.blind, counts.blind,
            "widening which games' disagreements are shown widened the blind sample too, which \
             would put a game the model trained on into the measurement"
        );
        assert!(
            everywhere
                .iter()
                .filter(|question| question.app_id == 296_970)
                .all(|question| question.shown.is_some()),
            "a claim from a training game may only appear as a disagreement"
        );

        let (none, quiet) = draw(&root, &root.join("data"), &asking(100, Splits::None)).unwrap();
        assert_eq!(quiet.split, 0);
        assert!(none.iter().all(|question| question.shown.is_none()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_draw_aimed_at_one_boundary_asks_about_that_one_and_leaves_the_blind_sample_alone() {
        let root = std::env::temp_dir().join(format!("steamgauge-aimed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);
        // The fixture's one disagreement, in both games: the first reading said `verdict` and
        // the second said `price`.
        let asked = |boundaries: &[(String, String)]| {
            draw(
                &root,
                &root.join("data"),
                &Asked {
                    boundaries,
                    ..asking(0, Splits::Everywhere)
                },
            )
            .unwrap()
            .1
        };

        let every = asked(&[]);
        let named = asked(&[("price".to_owned(), "verdict".to_owned())]);
        let other = asked(&[("difficulty".to_owned(), "gameplay".to_owned())]);

        assert_eq!(every.split, 2);
        assert_eq!(every.elsewhere, 0, "an unaimed draw leaves nothing out");
        // Named either way round, because which reading said which is not the boundary.
        assert_eq!(named.split, 2);
        assert_eq!(named.elsewhere, 0);
        assert_eq!(other.split, 0, "a boundary nothing falls on asks nothing");
        assert_eq!(other.elsewhere, 2);
        assert_eq!(
            (every.blind, named.blind, other.blind),
            (0, 0, 0),
            "aiming the disagreements must not touch the sample the measurement comes from"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_blind_sample_does_not_depend_on_where_the_second_labelling_got_to() {
        // Drawing blind only from claims nobody had read twice made the accuracy figure a
        // figure about whichever games the second pass had not reached. In the real set that
        // was four of the ten held-out games, 486 of 1,000 claims from one of them, and
        // finishing those four would have emptied the sample entirely. Coverage of the second
        // reading is a fact about scheduling; it must not decide what a person is asked.
        let root = std::env::temp_dir().join(format!("steamgauge-blind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);

        // 214490 is held out and its second labelling covers two of three claims. 1274570 is
        // also held out and has no second labelling at all.
        let bare = root.join("1274570");
        std::fs::create_dir_all(&bare).unwrap();
        captured(&root.join("data"), 1_274_570, &[("r1", THREE)]);
        std::fs::copy(
            root.join("214490").join("sample.json"),
            bare.join("sample.json"),
        )
        .unwrap();
        let relabelled = std::fs::read_to_string(root.join("214490").join("labels.json"))
            .unwrap()
            .replace("214490", "1274570");
        std::fs::write(bare.join("labels.json"), relabelled).unwrap();

        let (questions, counts) =
            draw(&root, &root.join("data"), &asking(100, Splits::None)).unwrap();
        let asked: std::collections::HashSet<u32> =
            questions.iter().map(|question| question.app_id).collect();
        assert!(
            asked.contains(&214_490) && asked.contains(&1_274_570),
            "a held-out game was left out of the blind sample because a second labeller had              reached it, which makes the sample a fact about scheduling"
        );
        assert_eq!(
            counts.settled, 1,
            "the one claim both labellers answered the same way is counted inside the sample"
        );
        assert!(
            questions.iter().all(|question| question.shown.is_none()),
            "a blind question that shows an answer is a ratification, not a measurement"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_sharpest_disagreements_come_before_the_blind_sample() {
        // A person stops when they stop. The few dozen claims two confident readers answered
        // differently are the shortest high-value list in the set, and putting them behind a
        // thousand blind claims is putting them in front of somebody who has already left.
        let root = std::env::temp_dir().join(format!("steamgauge-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);

        // blind_wanted of 0 keeps the held-out game's disagreement out of the blind sample, so it
        // stays a split and the ordering has something to order.
        let (questions, counts) =
            draw(&root, &root.join("data"), &asking(0, Splits::Everywhere)).unwrap();
        assert_eq!(counts.blind, 0);
        assert!(counts.contested_sure > 0, "nothing sharp to put first");
        assert!(
            questions
                .iter()
                .take(counts.contested_sure)
                .all(|question| question.shown.is_some()),
            "the questions asked first are not the disagreements"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_blind_sample_is_not_only_the_claims_the_labellers_agreed_on() {
        // Claims two labellers agreed on are the easy ones. If the disagreements are taken for
        // the split pool before the blind sample is drawn, the sample is left with nothing but
        // agreed claims and the accuracy figure is the flattering half of the corpus. Once
        // every held-out game had been read twice that is exactly what happened: 1,000 of 1,000
        // blind claims were agreed ones.
        let root = std::env::temp_dir().join(format!("steamgauge-bias-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);

        // One claim short of the three the held-out game has, so the draw has to choose.
        let (questions, counts) =
            draw(&root, &root.join("data"), &asking(3, Splits::Everywhere)).unwrap();
        let blind: Vec<&Question> = questions
            .iter()
            .filter(|question| question.shown.is_none())
            .collect();
        assert_eq!(counts.blind, 3);
        assert!(
            counts.settled < counts.blind,
            "every claim drawn blind is one the two labellers agreed on, so the figure it \
             produces is about the easy half of the corpus"
        );

        // Nothing is asked twice, once without answers and once with them.
        let asked: std::collections::HashSet<(u32, &str, u16)> = questions
            .iter()
            .map(|question| (question.app_id, question.review_id.as_str(), question.index))
            .collect();
        assert_eq!(asked.len(), questions.len(), "a claim was asked both ways");
        assert!(blind.iter().all(|question| question.app_id == 214_490));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_disagreement_neither_labeller_hedged_is_asked_before_one_they_doubted() {
        // A person adjudicates until they stop, not until the list ends, so the order is the
        // whole of what their hour buys. Two readers who were both sure and still disagreed
        // have found a real error or an undrawn boundary; one where either hedged is usually
        // just a hard claim, and burying the first behind the second wastes the draw.
        let root = std::env::temp_dir().join(format!("steamgauge-sharp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("214490").join("second")).unwrap();
        captured(&root.join("data"), 214_490, &[("r1", THREE)]);
        let drawn = serde_json::json!([drawn_from("r1", 214_490, "random", THREE)]);
        std::fs::write(root.join("214490").join("sample.json"), drawn.to_string()).unwrap();

        let label = |index: u16, subject: &str, doubted: bool| {
            let mut label = label_of(214_490, THREE, index, subject);
            label["confidence"] = (if doubted { "low" } else { "high" }).into();
            label
        };
        // Claim 0 is doubted and comes first in the file; claim 1 is the sure disagreement.
        std::fs::write(
            root.join("214490").join("labels.json"),
            serde_json::json!([label(0, "performance", true), label(1, "graphics", false)])
                .to_string(),
        )
        .unwrap();
        std::fs::write(
            root.join("214490").join("second").join("labels.json"),
            serde_json::json!([label(0, "bugs", false), label(1, "atmosphere", false)]).to_string(),
        )
        .unwrap();

        let (questions, counts) =
            draw(&root, &root.join("data"), &asking(0, Splits::HeldOut)).unwrap();
        assert_eq!(counts.split, 2);
        assert_eq!(
            counts.contested_sure, 1,
            "only the claim neither labeller doubted counts as a sure disagreement"
        );
        let split: Vec<&Question> = questions
            .iter()
            .filter(|question| question.shown.is_some())
            .collect();
        assert_eq!(
            split[0].index, 1,
            "the disagreement neither labeller hedged was asked second, behind a doubted one"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// Steam's tag is the reviewer's account setting, and a review tagged English and written
    /// in Hangul was put in front of somebody who reads English. The question they cannot
    /// read is held back; the one that names a foreign word is not.
    #[test]
    fn a_claim_written_in_a_script_its_tag_does_not_use_is_not_asked() {
        assert!(written_in_another_script(
            "english",
            "\u{D080}\u{C2A4}\u{D2B8} \u{C790}\u{CCB4}\u{C5D0}\u{C11C} \u{C0AC}\u{B294}\u{AC8C} \u{B354} .."
        ));
        assert!(
            !written_in_another_script("english", "The boss \u{7121}\u{540D} took me an hour."),
            "one foreign name in an English sentence is still English"
        );
        assert!(
            !written_in_another_script("english", "10/10"),
            "a claim with no letters is written in every language"
        );
        assert!(
            !written_in_another_script(
                "japanese",
                "\u{6226}\u{95D8}\u{624B}\u{611F}\u{6975}\u{597D}"
            ),
            "Japanese is written with Han as well as kana"
        );
        assert!(
            written_in_another_script("koreana", "Great game, would recommend to anyone."),
            "an English review under a Korean tag is not Korean either"
        );
        assert!(
            written_in_another_script(
                "english",
                "\u{8FD9}\u{4E2A}\u{6E38}\u{620F}\u{5F88}\u{597D}\u{73A9}"
            ),
            "Chinese under an English tag is Han, which English is not written in"
        );
        assert!(
            !written_in_another_script("english", "Boss \u{D080}\u{C2A4}\u{D2B8}\u{C790}"),
            "as many letters of the tag's script as of another is not mostly another"
        );
    }

    /// A label from an earlier splitter can name a span this build cuts elsewhere: a template
    /// heading since joined to the option under it, a comma list since taken apart. The words
    /// are still in the review, so nothing about the label is wrong, but there is no claim
    /// there to adjudicate and no reading that could ever be joined to the answer. Those rise
    /// to the front of the queue on their own, because two labellers reading a fragment land
    /// differently as often as they land together.
    ///
    /// The label's span is brought to its words before it is looked for, the way the measure
    /// joins it: a label cut under rules that left the bullet on still names the claim under
    /// the bullet, and holding it back here while the measure scores it would make the two
    /// disagree about which labels this build still cuts.
    #[test]
    fn a_span_this_build_no_longer_cuts_is_not_put_in_front_of_anybody() {
        let text = "- Too hard for me.\n- Great music.";
        let label = |start: u32, end: u32| ClaimLabel {
            review_id: "r1".to_owned(),
            index: 0,
            app_id: 1,
            language: "english".to_owned(),
            subset: "random".to_owned(),
            start,
            end,
            taxonomy: crate::taxonomy::sheet(),
            produced_by: "one".to_owned(),
            subject: "difficulty".to_owned(),
            polarity: "neutral".to_owned(),
            ironic: false,
            confidence: "high".to_owned(),
            ambiguous: false,
            split_wrong: false,
            also: None,
        };
        let cut = |had: &[(u32, u32)]| {
            std::collections::HashMap::from([(
                "r1".to_owned(),
                crate::claimset::Cut {
                    text: text.to_owned(),
                    spans: had.iter().copied().collect(),
                },
            )])
        };

        assert!(
            still_cut(&cut(&[(2, 18), (21, 33)]), &label(2, 18)),
            "the span is cut exactly as the label names it"
        );
        assert!(
            still_cut(&cut(&[(2, 18), (21, 33)]), &label(0, 18)),
            "a label cut under rules that kept the bullet still names the claim under it"
        );
        assert!(
            !still_cut(&cut(&[(2, 33)]), &label(2, 18)),
            "the heading was joined to the option it labels, so the fragment is not a claim"
        );
        assert!(
            !still_cut(&cut(&[(2, 9), (10, 18)]), &label(2, 18)),
            "the comma list was taken apart, so one label now names two claims"
        );
        assert!(
            !still_cut(&cut(&[]), &label(2, 18)),
            "the review produces no claims at all now, which is what happens to drawings"
        );
        assert!(
            !still_cut(&crate::claimset::CutSpans::new(), &label(2, 18)),
            "a review the capture no longer holds, deleted since it was labelled, has no claims"
        );

        let mut draw = GoldDraw::default();
        assert!(draw.holds_back(&label(2, 18), "Too hard for me.", false, &cut(&[(2, 33)])));
        assert!(!draw.holds_back(&label(2, 18), "Too hard for me.", false, &cut(&[(2, 18)])));
        assert_eq!(
            (draw.recut, draw.declined, draw.mistagged),
            (1, 0, 0),
            "the draw says how many it held back, and why"
        );
    }

    #[test]
    fn a_draw_of_some_boundaries_passes_over_the_others_whichever_way_round_they_came() {
        let wanted = vec![("story".to_owned(), "gameplay".to_owned())];
        let mut passed = 0;
        assert!(!elsewhere(&wanted, "story", "gameplay", &mut passed));
        assert!(!elsewhere(&wanted, "gameplay", "story", &mut passed));
        assert!(elsewhere(&wanted, "story", "price", &mut passed));
        assert!(elsewhere(&wanted, "price", "gameplay", &mut passed));
        assert!(elsewhere(&wanted, "price", "story", &mut passed));
        assert_eq!(passed, 3);
        assert!(!elsewhere(&[], "story", "price", &mut passed));
    }

    /// Since a heading introduces every box ticked under it, two claims of one review can
    /// begin with the same words, and a template with the same option ticked twice makes two
    /// claims that are the same words throughout. Finding a claim by searching the rejoined
    /// text for it would highlight the first one both times, and the person would answer the
    /// same sentence twice without either question looking wrong.
    #[test]
    fn a_claim_is_found_by_where_it_sits_and_not_by_what_it_says() {
        let text = "Audience\n\u{2611} Adults\n\nAudience\n\u{2611} Adults";
        let rejoined = Rejoined::of(text);
        let span = |index: usize| {
            let (start, end) = span_of(text, index);
            (u32::try_from(start).unwrap(), u32::try_from(end).unwrap())
        };
        let (first, said) = rejoined.find(span(0)).expect("the first claim is there");
        let (second, again) = rejoined.find(span(1)).expect("the second claim is there");
        assert_eq!(said, again, "the two claims are the same words");
        assert_ne!(
            first, second,
            "two claims with the same words were given the same place in the review"
        );
    }

    #[test]
    fn options_ticked_under_one_heading_stand_a_line_apart() {
        let ballot = "---{ Audience }---\n\u{2611} Kids\n\u{2610} Everyone\n\u{2611} Adults";
        let claims = crate::claims::claims_of(ballot);
        assert!(
            claims[1].0.start < claims[0].0.end,
            "the two options no longer share the heading, so this tests nothing: {claims:?}"
        );
        let rejoined = Rejoined::of(ballot);
        assert_eq!(
            rejoined.text,
            format!("{}\n{}", claims[0].1, claims[1].1),
            "each ticked option is shown on a line of its own"
        );
    }

    #[test]
    fn an_option_the_reviewer_left_blank_is_never_put_in_front_of_a_person() {
        // Most of the reference set was cut before the splitter collapsed a ballot template,
        // so these fragments are still in it, and they cluster in the disagreement pool
        // because two labellers reading an unchosen option rarely land the same way. They are
        // not hard questions. They are unanswerable ones wearing the costume of the hardest.
        let root = std::env::temp_dir().join(format!("steamgauge-ballot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("214490").join("second")).unwrap();
        let ballot = "\u{2610} Worth the price\n\u{2611} The game runs badly on my laptop";
        captured(&root.join("data"), 214_490, &[("r1", ballot)]);
        let (ticked, ticked_end) = span_of(ballot, 0);
        let spans = [(0, ticked - 1), (ticked, ticked_end)];

        let drawn = serde_json::json!([{
            "id": "r1", "app_id": 214_490, "language": "english", "subset": "held-out",
            "claims": [
                {"index": 0, "start": spans[0].0, "end": spans[0].1,
                 "text": "\u{2610} Worth the price"},
                {"index": 1, "start": spans[1].0, "end": spans[1].1,
                 "text": "\u{2611} The game runs badly on my laptop"}
            ]
        }]);
        std::fs::write(root.join("214490").join("sample.json"), drawn.to_string()).unwrap();

        let label = |index: u16, subject: &str| {
            let (start, end) = spans[usize::from(index)];
            serde_json::json!({
                "review_id": "r1", "index": index, "app_id": 214_490,
                "language": "english", "subset": "held-out", "start": start, "end": end,
                "taxonomy": "a-sheet", "produced_by": "one",
                "subject": subject, "polarity": "praise", "ironic": false,
                "confidence": "high", "ambiguous": false, "split_wrong": false
            })
        };
        std::fs::write(
            root.join("214490").join("labels.json"),
            serde_json::json!([label(0, "value"), label(1, "performance")]).to_string(),
        )
        .unwrap();
        std::fs::write(
            root.join("214490").join("second").join("labels.json"),
            serde_json::json!([label(0, "offtopic"), label(1, "bugs")]).to_string(),
        )
        .unwrap();

        let (questions, counts) =
            draw(&root, &root.join("data"), &asking(10, Splits::HeldOut)).unwrap();
        assert_eq!(counts.declined, 1, "the blank option was not held back");
        assert!(
            questions
                .iter()
                .all(|question| !question.claim.starts_with('\u{2610}')),
            "an option the reviewer declined was asked anyway"
        );
        assert_eq!(
            questions.len(),
            1,
            "the ticked option is a real claim and still has to be asked"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_three_pieces_of_a_question_are_the_review_it_came_from() {
        // The page cannot check this: it is handed the pieces and has nothing to compare them
        // with. If the split is wrong the reader is shown a review that was never written,
        // with the wrong sentence highlighted in it, and nothing anywhere looks broken.
        let review = "\u{6559}\u{5b66}\u{7eaf}\u{9760}\u{81ea}\u{5df1}\u{9886}\u{609f}\u{ff0c}\
                      \u{5b8c}\u{5168}\u{6ca1}\u{6709}\u{5f15}\u{5bfc}\u{3002}\n\
                      \u{6218}\u{6597}\u{624b}\u{611f}\u{6781}\u{597d}\u{ff0c}\
                      \u{6253}\u{51fb}\u{611f}\u{4e00}\u{6d41}\u{3002}\n\n\
                      [b]It is worth the full price.[/b]";
        let rejoined = Rejoined::of(review);
        let claims = crate::claims::claims_of(review);
        assert_eq!(claims.len(), 3, "{claims:?}");
        for (index, (at, claim)) in claims.iter().enumerate() {
            let span = (
                u32::try_from(at.start).unwrap(),
                u32::try_from(at.end).unwrap(),
            );
            let (at, text) = rejoined.find(span).expect("the claim is in the review");
            let before = &rejoined.text[..at];
            let after = &rejoined.text[at + text.len()..];
            assert_eq!(
                format!("{before}{text}{after}"),
                rejoined.text,
                "claim {index} does not put the review back together"
            );
            assert_eq!(text, claim, "claim {index} is not itself");
        }
        assert_eq!(
            rejoined.text,
            "\u{6559}\u{5b66}\u{7eaf}\u{9760}\u{81ea}\u{5df1}\u{9886}\u{609f}\u{ff0c}\
             \u{5b8c}\u{5168}\u{6ca1}\u{6709}\u{5f15}\u{5bfc}\u{3002}\n\
             \u{6218}\u{6597}\u{624b}\u{611f}\u{6781}\u{597d}\u{ff0c}\
             \u{6253}\u{51fb}\u{611f}\u{4e00}\u{6d41}\u{3002}\n\n\
             It is worth the full price.",
            "each claim reads as the reader is handed it, markup out, and the claims stand apart \
             as the reviewer set them: a line break, then a blank line"
        );
    }

    #[test]
    fn a_page_is_drawn_only_from_games_whose_capture_says_what_this_build_cuts() {
        let root = crate::tempdir::Dir::new();
        a_reference_set(root.path());
        assert!(
            matches!(
                draw(
                    root.path(),
                    &root.path().join("elsewhere"),
                    &asking(100, Splits::HeldOut)
                ),
                Err(Error::NoCapture { .. })
            ),
            "without the capture nothing says which stored spans this build still cuts, and \
             the page would ask about all of them"
        );

        // The held-out set was cut by an older splitter that kept the whole review as one
        // claim; this build cuts it in three, so the one stored label names no claim at all.
        let set = root.path().join("214490");
        let whole = u32::try_from(THREE.len()).unwrap();
        let drawn = serde_json::json!([{
            "id": "r1", "app_id": 214_490, "language": "english", "subset": "random",
            "claims": [{"index": 0, "start": 0, "end": whole, "text": THREE}]
        }]);
        std::fs::write(set.join("sample.json"), drawn.to_string()).unwrap();
        let mut stale = label_of(214_490, THREE, 0, "performance");
        stale["end"] = whole.into();
        std::fs::write(
            set.join("labels.json"),
            serde_json::json!([stale]).to_string(),
        )
        .unwrap();
        std::fs::remove_dir_all(set.join("second")).unwrap();

        let (questions, found) = draw(
            root.path(),
            &root.path().join("data"),
            &asking(100, Splits::HeldOut),
        )
        .unwrap();
        assert!(questions.is_empty(), "{questions:?}");
        assert_eq!(found.recut, 1);
    }

    #[test]
    fn a_question_shows_the_review_with_its_own_line_breaks() {
        let root = crate::tempdir::Dir::new();
        let set = root.path().join("214490");
        std::fs::create_dir_all(&set).unwrap();
        let review = "The game runs badly on my laptop.\nThe art style looks great to me.\n\n\
                      It is worth the full price.";
        captured(&root.path().join("data"), 214_490, &[("r1", review)]);
        let drawn = serde_json::json!([drawn_from("r1", 214_490, "random", review)]);
        std::fs::write(set.join("sample.json"), drawn.to_string()).unwrap();
        std::fs::write(
            set.join("labels.json"),
            serde_json::json!([label_of(214_490, review, 1, "graphics")]).to_string(),
        )
        .unwrap();

        let (questions, _) = draw(
            root.path(),
            &root.path().join("data"),
            &asking(100, Splits::HeldOut),
        )
        .unwrap();
        let asked = &questions[0];
        assert_eq!(asked.claim, "The art style looks great to me.");
        assert_eq!(asked.before, "The game runs badly on my laptop.\n");
        assert_eq!(asked.after, "\n\nIt is worth the full price.");
    }

    #[test]
    fn a_claim_in_another_script_is_held_back_only_where_a_language_was_asked_for() {
        let hangul = "\u{D080}\u{C2A4}\u{D2B8} \u{C790}\u{CCB4}\u{C5D0}\u{C11C}";
        let label: ClaimLabel = serde_json::from_value(serde_json::json!({
            "review_id": "r1", "index": 0, "app_id": 1, "language": "english",
            "subset": "random", "start": 0, "end": hangul.len(), "taxonomy": "a-sheet",
            "produced_by": "one", "subject": "verdict", "polarity": "praise", "ironic": false,
            "confidence": "high", "ambiguous": false, "split_wrong": false
        }))
        .unwrap();
        let end = u32::try_from(hangul.len()).unwrap();
        let cut = std::collections::HashMap::from([(
            "r1".to_owned(),
            crate::claimset::Cut {
                text: hangul.to_owned(),
                spans: std::collections::HashSet::from([(0, end)]),
            },
        )]);
        let mut found = GoldDraw::default();
        assert!(!found.holds_back(&label, hangul, false, &cut));
        assert!(found.holds_back(&label, hangul, true, &cut));
        assert!(found.holds_back(&label, hangul, true, &cut));
        assert_eq!(found.mistagged, 2);
    }

    #[test]
    fn a_labeller_hedges_by_any_one_of_the_three_signs() {
        let label = |ambiguous: bool, split_wrong: bool, confidence: &str| -> ClaimLabel {
            serde_json::from_value(serde_json::json!({
                "review_id": "r1", "index": 0, "app_id": 1, "language": "english",
                "subset": "random", "start": 0, "end": 4, "taxonomy": "a-sheet",
                "produced_by": "one", "subject": "verdict", "polarity": "praise",
                "ironic": false, "confidence": confidence, "ambiguous": ambiguous,
                "split_wrong": split_wrong
            }))
            .unwrap()
        };
        assert!(hedged(&label(true, false, "high")));
        assert!(hedged(&label(false, true, "high")));
        assert!(hedged(&label(false, false, "low")));
        assert!(!hedged(&label(false, false, "high")));
    }

    #[test]
    fn each_language_is_read_in_the_scripts_it_is_written_in() {
        let written = [
            (
                "russian",
                "\u{0418}\u{0433}\u{0440}\u{0430} \u{0445}\u{043e}\u{0440}\u{043e}\u{0448}\u{0430}\u{044f}",
            ),
            (
                "ukrainian",
                "\u{0414}\u{0443}\u{0436}\u{0435} \u{0434}\u{043e}\u{0431}\u{0440}\u{0435}",
            ),
            (
                "bulgarian",
                "\u{041c}\u{043d}\u{043e}\u{0433}\u{043e} \u{0434}\u{043e}\u{0431}\u{0440}\u{0430}",
            ),
            (
                "greek",
                "\u{03a0}\u{03bf}\u{03bb}\u{03cd} \u{03ba}\u{03b1}\u{03bb}\u{03cc}",
            ),
            (
                "thai",
                "\u{0e40}\u{0e01}\u{0e21}\u{0e14}\u{0e35}\u{0e21}\u{0e32}\u{0e01}",
            ),
            (
                "arabic",
                "\u{0644}\u{0639}\u{0628}\u{0629} \u{0631}\u{0627}\u{0626}\u{0639}\u{0629}",
            ),
            (
                "japanese",
                "\u{3068}\u{3066}\u{3082}\u{3044}\u{3044}\u{30b2}\u{30fc}\u{30e0}",
            ),
            (
                "schinese",
                "\u{8fd9}\u{4e2a}\u{6e38}\u{620f}\u{5f88}\u{597d}",
            ),
            (
                "tchinese",
                "\u{9019}\u{500b}\u{904a}\u{6232}\u{5f88}\u{597d}",
            ),
        ];
        for (language, text) in written {
            assert!(
                !written_in_another_script(language, text),
                "{language} is written in its own script"
            );
            assert!(
                written_in_another_script("english", text),
                "{language} under an English tag is not English"
            );
        }
    }

    #[test]
    fn only_claims_in_the_languages_asked_for_are_drawn() {
        let root = crate::tempdir::Dir::new();
        a_reference_set(root.path());
        let languages = |asked: &[String]| {
            draw(
                root.path(),
                &root.path().join("data"),
                &Asked {
                    languages: asked,
                    ..asking(100, Splits::HeldOut)
                },
            )
            .unwrap()
        };
        let (english, found) = languages(&["english".to_owned()]);
        assert_eq!(english.len(), 3);
        assert_eq!(found.languages, ["english"]);
        let (russian, _) = languages(&["russian".to_owned()]);
        assert!(russian.is_empty(), "{russian:?}");
    }

    #[test]
    fn a_draw_by_language_holds_back_a_claim_its_tag_misnames() {
        let root = crate::tempdir::Dir::new();
        let set = root.path().join("214490");
        std::fs::create_dir_all(&set).unwrap();
        let review = "The game runs badly on my laptop.\n\
                      \u{D080}\u{C2A4}\u{D2B8} \u{C790}\u{CCB4}\u{C5D0}\u{C11C}";
        captured(&root.path().join("data"), 214_490, &[("r1", review)]);
        let drawn = serde_json::json!([drawn_from("r1", 214_490, "held-out", review)]);
        std::fs::write(set.join("sample.json"), drawn.to_string()).unwrap();
        let label = |index: u16| label_of(214_490, review, index, "performance");
        std::fs::write(
            set.join("labels.json"),
            serde_json::json!([label(0), label(1)]).to_string(),
        )
        .unwrap();
        let (questions, found) = draw(
            root.path(),
            &root.path().join("data"),
            &Asked {
                languages: &["english".to_owned()],
                ..asking(10, Splits::HeldOut)
            },
        )
        .unwrap();
        assert_eq!(found.mistagged, 1);
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].claim, "The game runs badly on my laptop.");
    }

    #[test]
    fn a_gold_answer_that_differs_from_the_label_is_asked_again_with_both_labellers_shown() {
        let root = crate::tempdir::Dir::new();
        a_reference_set(root.path());
        let set = root.path().join("214490");
        std::fs::create_dir_all(answers_of(&set, "one")).unwrap();
        let gold = |index: u16, subject: &str| {
            let mut answer = label_of(214_490, THREE, index, subject);
            answer["produced_by"] = "a person".into();
            answer
        };
        // Answered under an older splitter that kept the whole review as one claim.
        let mut stale = gold(1, "content");
        stale["start"] = 0.into();
        stale["end"] = THREE.len().into();
        std::fs::write(
            answers_of(&set, "one").join("labels.json"),
            serde_json::json!([gold(0, "performance"), stale, gold(2, "content")]).to_string(),
        )
        .unwrap();

        let captures = root.path().join("data");
        assert!(
            rejudge(root.path(), &captures, "second", "two")
                .unwrap()
                .0
                .is_empty(),
            "nobody is asked again about another person's answers"
        );
        assert!(
            matches!(
                rejudge(root.path(), &root.path().join("elsewhere"), "second", "one"),
                Err(Error::NoCapture { .. })
            ),
            "an answer is judged again only where the capture says its span is still a claim"
        );
        let (questions, found) = rejudge(root.path(), &captures, "second", "one").unwrap();
        assert_eq!(
            (found.games, found.agreed, found.split, found.recut),
            (1, 1, 1, 1),
            "the answer on a span this build no longer cuts is counted, not asked again"
        );
        let asked = &questions[0];
        assert_eq!((asked.app_id, asked.index), (214_490, 2));
        assert_eq!(asked.claim, "It is worth the full price.");
        assert_eq!(
            asked.before,
            "The game runs badly on my laptop. The art style looks great to me. "
        );
        assert_eq!(asked.after, "");
        assert_eq!(
            asked.was.as_ref().map(|was| was.subject.as_str()),
            Some("content")
        );
        let shown: Vec<&str> = asked
            .shown
            .iter()
            .flatten()
            .map(|one| one.subject.as_str())
            .collect();
        assert_eq!(
            shown,
            ["verdict", "price"],
            "the first labeller, then the second"
        );
    }

    #[test]
    fn a_page_carries_every_question_and_the_whole_sheet() {
        let questions = vec![Question {
            app_id: 1,
            review_id: "42".to_owned(),
            index: 0,
            claim: "Runs badly".to_owned(),
            before: String::new(),
            after: " and I love it".to_owned(),
            language: "english".to_owned(),
            shown: None,
            was: None,
            offered: None,
        }];
        let page = render(&questions, &GoldDraw::default(), "one");
        assert!(page.contains("Runs badly"));
        assert!(
            page.contains("\"for\":\"one\""),
            "the page says whose answers it collects"
        );
        for category in SHEET {
            assert!(page.contains(category.id), "{} is missing", category.id);
        }
        assert!(
            page.contains("<script id=\"data\""),
            "the questions travel as data rather than as markup"
        );
    }

    #[test]
    fn a_blind_question_carries_no_answer() {
        let questions = vec![Question {
            app_id: 1,
            review_id: "42".to_owned(),
            index: 0,
            claim: "Runs badly".to_owned(),
            before: String::new(),
            after: String::new(),
            language: "english".to_owned(),
            shown: None,
            was: None,
            offered: None,
        }];
        let page = render(&questions, &GoldDraw::default(), "one");
        assert!(
            page.contains("\"shown\":null"),
            "an answer on the page is an answer in the reader's head"
        );
    }

    /// A game read by the reader: two reviews, the first cut into a claim about the Deck and one
    /// about the story, the second into one about the Deck, all three put under a subject.
    fn read_library(snapshot: &Path) {
        let reviews = [
            serde_json::json!({"recommendationid": "1", "review": "Runs badly on Steam Deck. Great story.",
                "language": "english", "timestamp_created": 1, "timestamp_updated": 1,
                "voted_up": false, "votes_up": 0}),
            serde_json::json!({"recommendationid": "2", "review": "Perfect on my Steam Deck!",
                "language": "english", "timestamp_created": 2, "timestamp_updated": 2,
                "voted_up": true, "votes_up": 0}),
        ];
        let mut writer =
            crate::capture::CaptureWriter::create(&snapshot.join("shard-0000.parquet"), 1).unwrap();
        writer.write(&reviews.iter().collect::<Vec<_>>()).unwrap();
        writer.close().unwrap();

        let text = |at: usize| reviews[at]["review"].as_str().unwrap();
        crate::read::tests::write_readings(
            &snapshot.join("readings.parquet"),
            &[
                (
                    "1",
                    text(0),
                    (0, 25),
                    Some("performance"),
                    "complaint",
                    None,
                ),
                ("1", text(0), (26, 38), Some("story"), "complaint", None),
                // An older splitter's cut of the whole review, which this build cuts in two.
                (
                    "1",
                    text(0),
                    (0, 38),
                    Some("performance"),
                    "complaint",
                    None,
                ),
                (
                    "2",
                    text(1),
                    (0, 25),
                    Some("performance"),
                    "complaint",
                    None,
                ),
            ],
        );
    }

    fn judged(review: &str, subject: &str, acceptable: bool) -> Acceptable {
        Acceptable {
            review_id: review.to_owned(),
            index: 0,
            subject: subject.to_owned(),
            acceptable,
            by: "a person".to_owned(),
        }
    }

    #[test]
    fn a_later_judgement_of_a_subject_replaces_the_earlier_and_leaves_the_others() {
        let set = crate::tempdir::Dir::new();

        keep_acceptable(
            set.path(),
            "one",
            &[judged("1", "story", false), judged("1", "gameplay", true)],
        )
        .unwrap();
        let kept = keep_acceptable(set.path(), "one", &[judged("1", "story", true)]).unwrap();
        keep_acceptable(set.path(), "two", &[judged("1", "story", false)]).unwrap();

        assert_eq!(kept, 2);
        let story = acceptable_in(set.path(), "one")
            .into_iter()
            .find(|one| one.subject == "story")
            .unwrap();
        assert!(story.acceptable, "the later judgement is the one kept");
        assert_eq!(
            acceptable_in(set.path(), "two").len(),
            1,
            "a second person's judgement sits beside the first's"
        );
    }

    #[test]
    fn a_person_is_named_as_a_github_handle_is_written_and_nothing_else() {
        for fits in ["aureliolo", "pexp13", "a-b", "x"] {
            assert_eq!(person(fits).unwrap(), fits);
        }
        let long = "a".repeat(40);
        for refused in [
            "",
            "../x",
            "Aureliolo",
            "a b",
            "-a",
            "a/b",
            "a\\b",
            ".",
            long.as_str(),
        ] {
            assert!(person(refused).is_err(), "{refused:?} was taken");
        }
        assert_eq!(person(&"a".repeat(39)).unwrap().len(), 39);
    }

    #[test]
    fn a_person_is_asked_only_where_the_reader_named_another_subject_not_yet_judged() {
        let root = crate::tempdir::Dir::new();
        let library = root.path().join("data");
        let snapshot = library.join("appid=7").join("snapshot=1");
        std::fs::create_dir_all(&snapshot).unwrap();
        read_library(&snapshot);

        let set = root.path().join("reference").join("7");
        std::fs::create_dir_all(answers_of(&set, "one")).unwrap();
        let claim = |index: u16, start: u32, end: u32, text: &str| crate::claimset::DrawnClaim {
            index,
            start,
            end,
            text: text.to_owned(),
        };
        let drawn = vec![
            DrawnReview {
                id: "1".to_owned(),
                app_id: 7,
                language: "english".to_owned(),
                subset: "random".to_owned(),
                claims: vec![
                    claim(0, 0, 25, "Runs badly on Steam Deck."),
                    claim(1, 26, 38, "Great story."),
                ],
                asked: None,
            },
            DrawnReview {
                id: "2".to_owned(),
                app_id: 7,
                language: "english".to_owned(),
                subset: "random".to_owned(),
                claims: vec![claim(0, 0, 25, "Perfect on my Steam Deck!")],
                asked: None,
            },
        ];
        std::fs::write(set.join("sample.json"), serde_json::to_vec(&drawn).unwrap()).unwrap();
        let gold = |review: &str, index: u16, start: u32, end: u32, subject: &str| ClaimLabel {
            review_id: review.to_owned(),
            index,
            app_id: 7,
            language: "english".to_owned(),
            subset: "random".to_owned(),
            start,
            end,
            taxonomy: crate::taxonomy::sheet(),
            produced_by: "a person".to_owned(),
            subject: subject.to_owned(),
            polarity: "complaint".to_owned(),
            ironic: false,
            confidence: "high".to_owned(),
            ambiguous: false,
            split_wrong: false,
            also: None,
        };
        let labels = vec![
            gold("1", 0, 0, 25, "compatibility"),
            gold("1", 1, 26, 38, "story"),
            gold("2", 0, 0, 25, "compatibility"),
            gold("1", 2, 0, 38, "story"),
        ];
        std::fs::write(
            answers_of(&set, "one").join("labels.json"),
            serde_json::to_vec(&labels).unwrap(),
        )
        .unwrap();
        keep_acceptable(&set, "one", &[judged("2", "performance", true)]).unwrap();

        let reference = root.path().join("reference");
        assert!(
            offered(&reference, &library, "two").unwrap().0.is_empty(),
            "nobody is asked about another person's answers"
        );
        let (questions, found) = offered(&reference, &library, "one").unwrap();

        assert_eq!(questions.len(), 1, "{questions:?}");
        let asked = &questions[0];
        assert_eq!((asked.review_id.as_str(), asked.index), ("1", 0));
        assert_eq!(asked.offered.as_deref(), Some("performance"));
        assert_eq!(
            asked.was.as_ref().map(|was| was.subject.as_str()),
            Some("compatibility")
        );
        assert_eq!(asked.claim, "Runs badly on Steam Deck.");
        assert_eq!(
            (asked.before.as_str(), asked.after.as_str()),
            ("", " Great story.")
        );
        assert_eq!(
            found.agreed, 1,
            "the story claim the reader answered as the person did"
        );
        assert_eq!(found.games, 1);
        assert_eq!(
            found.recut, 1,
            "the answer on the whole review, which this build cuts in two, is counted, not asked"
        );
    }
}
