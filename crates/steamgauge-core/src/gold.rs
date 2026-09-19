//! The claims a person adjudicates, and the page they do it on.
//!
//! Everything measured so far is a model agreeing with a model. The README says so and no
//! citation can rest on it. What turns a silver set into a gold one is a person reading the
//! claims themselves, and the only reason that has not happened is that there was nothing to
//! read them on.
//!
//! Two draws, because they answer different questions:
//!
//! - **Blind.** A random sample of frozen claims with no answer shown, which is the only kind
//!   of reading that produces an accuracy figure rather than a ratification. Every frozen claim
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

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    Result,
    claimset::{ClaimLabel, DrawnReview},
    measure::{Role, SPLIT_SEED, role},
    taxonomy::SHEET,
};

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
    /// Not a separate draw: a sample over every frozen claim contains them in their true
    /// proportion, and scoring them apart afterwards is what tests the assumption the rest of
    /// the set rests on, that two labellers agreeing means both were right.
    pub settled: usize,
    /// Disagreements where neither labeller hedged. Two readers who were both sure and still
    /// answered differently have found either a real error or a boundary the sheet does not
    /// draw, and there is nothing else in the set with that much in it per claim read.
    pub contested_sure: usize,
    /// Which languages the person was asked about, empty for all of them. A sample restricted
    /// to what the adjudicator reads is a random sample of those languages and not of the
    /// corpus, and the figure it produces has to say so.
    pub languages: Vec<String>,
}

/// Which games' disagreements to put in front of a person.
///
/// The blind sample is always frozen, because that is what makes it a measurement. The splits
/// measure nothing: they settle a boundary, and a boundary settled on one game is settled for
/// every game, so they may be drawn from anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Splits {
    None,
    Frozen,
    Everywhere,
}

/// Where a claim is, so the blind draw and the disagreements cannot both take the same one.
type At = (u32, String, u16);
/// A frozen claim that could be asked blind: its draw order, where it is, whether both
/// labellers answered it the same way, and the question itself.
type Candidate = ([u8; 32], At, bool, Question);
/// A disagreement: where it is, whether either labeller hedged, and the question.
type Contested = (At, bool, Question);

/// Whether a labeller signalled doubt, by any of the three means the sheet gives them.
fn hedged(label: &ClaimLabel) -> bool {
    label.ambiguous || label.split_wrong || label.confidence == "low"
}

/// Draws the claims a person should read.
///
/// Frozen because a gold figure has to be about games that chose nothing: a person adjudicating
/// the games the model trained on would produce a number that flatters every part of the chain
/// at once.
///
/// # Errors
///
/// Fails if a reference set cannot be read.
pub fn draw(
    reference: &Path,
    blind_wanted: usize,
    splits: Splits,
    seed: u64,
    languages: &[String],
    reading: &str,
) -> Result<(Vec<Question>, GoldDraw)> {
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
        let frozen = role(app_id, SPLIT_SEED) == Role::Frozen;
        if !frozen && splits != Splits::Everywhere {
            continue;
        }
        let Ok(sample) = std::fs::read(dir.join("sample.json")) else {
            continue;
        };
        let Ok(first) = std::fs::read(dir.join("labels.json")) else {
            continue;
        };
        let drawn: Vec<DrawnReview> = serde_json::from_slice(&sample)?;
        let first: Vec<ClaimLabel> = serde_json::from_slice(&first)?;
        let second: Vec<ClaimLabel> = std::fs::read(dir.join(reading).join("labels.json"))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        found.games += usize::from(frozen);

        let around: std::collections::HashMap<&str, Rejoined<'_>> = drawn
            .iter()
            .map(|review| (review.id.as_str(), Rejoined::of(review)))
            .collect();

        let twice: std::collections::HashMap<(&str, u16), &ClaimLabel> = second
            .iter()
            .map(|label| ((label.review_id.as_str(), label.index), label))
            .collect();

        for label in &first {
            let Some(rejoined) = around.get(label.review_id.as_str()) else {
                continue;
            };
            // A person cannot adjudicate a language they do not read, and a question they cannot
            // answer is worse than one that was never asked: it sits in the count, it cannot be
            // skipped honestly, and whatever they put is noise wearing the one label in this
            // project that is allowed to be called truth.
            if !languages.is_empty() && !languages.iter().any(|one| one == &label.language) {
                continue;
            }
            let Some((at, text)) = rejoined.find(label.index) else {
                continue;
            };
            let question = Question {
                app_id,
                review_id: label.review_id.clone(),
                index: label.index,
                claim: text.to_owned(),
                before: rejoined.text[..at].to_owned(),
                after: rejoined.text[at + text.len()..].to_owned(),
                language: label.language.clone(),
                shown: None,
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

            // Every frozen claim is a candidate, whether or not a second labeller reached it and
            // whether or not the two agreed. Drawing only from claims nobody read twice made the
            // sample a fact about where the second pass had got to. Drawing only from the ones
            // they agreed on is worse in a quieter way: those are the easy claims, and an
            // accuracy figure over them alone is the flattering half of the corpus.
            if !frozen {
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
    let questions = assemble(blind, split, blind_wanted, &mut found);

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

    split.sort_by_key(|(_, hedged, _)| *hedged);
    found.contested_sure = split.iter().filter(|(_, hedged, _)| !hedged).count();

    let mut questions: Vec<Question> = blind.into_iter().map(|(_, _, _, q)| q).collect();
    questions.extend(split.into_iter().map(|(_, _, question)| question));
    questions
}

/// A review's claims joined back together, and where each one lands in the result.
///
/// The same view the labeller was shown and the same one the reader is trained against, so a
/// person adjudicating sees neither more nor less than either of them did.
struct Rejoined<'a> {
    text: String,
    /// Claim index, byte offset into `text`, and the claim itself.
    claims: Vec<(u16, usize, &'a str)>,
}

impl<'a> Rejoined<'a> {
    fn of(review: &'a DrawnReview) -> Self {
        let mut text = String::new();
        let mut claims = Vec::with_capacity(review.claims.len());
        for claim in &review.claims {
            if !text.is_empty() {
                text.push(' ');
            }
            claims.push((claim.index, text.len(), claim.text.as_str()));
            text.push_str(&claim.text);
        }
        Self { text, claims }
    }

    fn find(&self, index: u16) -> Option<(usize, &'a str)> {
        self.claims
            .iter()
            .find(|(at, _, _)| *at == index)
            .map(|&(_, at, text)| (at, text))
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

/// Renders the page a person adjudicates on: one file, fetching nothing, sending nothing.
#[must_use]
pub fn render(questions: &[Question], found: &GoldDraw) -> String {
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
        "splitter": crate::claims::SPLITTER_VERSION,
        "blind": found.blind,
        "settled": found.settled,
        "split": found.split,
        "games": found.games,
        "agreed": found.agreed,
        "languages": found.languages,
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

    /// A reference set on disk: one frozen game and one training game, each with three claims.
    /// The second labeller reads two of them, agreeing on one and differing on the other, and
    /// never sees the third, which is therefore the only one left to read blind.
    fn a_reference_set(root: &Path) {
        for app_id in [214_490_u32, 296_970_u32] {
            let dir = root.join(app_id.to_string());
            std::fs::create_dir_all(dir.join("second")).unwrap();

            let drawn = serde_json::json!([{
                "id": "r1", "app_id": app_id, "language": "english", "subset": "random",
                "claims": [
                    {"index": 0, "start": 0, "end": 9, "text": "Runs badly"},
                    {"index": 1, "start": 10, "end": 20, "text": "Looks great"},
                    {"index": 2, "start": 21, "end": 30, "text": "Worth it"}
                ]
            }]);
            std::fs::write(dir.join("sample.json"), drawn.to_string()).unwrap();

            let label = |index: u16, subject: &str| {
                serde_json::json!({
                    "review_id": "r1", "index": index, "app_id": app_id,
                    "language": "english", "subset": "random", "start": 0, "end": 9,
                    "splitter": "claims-5", "taxonomy": "core-6", "produced_by": "one",
                    "subject": subject, "polarity": "praise", "ironic": false,
                    "confidence": "high", "ambiguous": false, "split_wrong": false
                })
            };
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
    fn a_blind_draw_is_frozen_games_only_and_a_split_can_come_from_anywhere() {
        // 214490 is frozen and 296970 trains, both pinned in DECISIONS.md.
        assert_eq!(role(214_490, SPLIT_SEED), Role::Frozen);
        assert_ne!(role(296_970, SPLIT_SEED), Role::Frozen);

        let root = std::env::temp_dir().join(format!("steamgauge-gold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);

        let (frozen_only, counts) = draw(&root, 100, Splits::Frozen, 1, &[], "second").unwrap();
        assert!(
            frozen_only
                .iter()
                .all(|question| question.app_id == 214_490),
            "a training game reached a draw that measures the model"
        );
        assert_eq!(
            counts.blind, 3,
            "the blind sample is every frozen claim, whether or not a second labeller reached \
             it and whether or not the two agreed"
        );
        assert_eq!(
            counts.settled, 1,
            "one of the three is a claim both labellers answered the same way, which is the \
             control, counted rather than drawn separately"
        );
        assert_eq!(
            counts.split, 0,
            "a blind sample large enough to take every frozen claim leaves the frozen game no \
             disagreement to show, because the same claim cannot be asked both ways"
        );
        assert_eq!(counts.agreed, 1);
        assert_eq!(
            counts.games, 1,
            "only the frozen game counts as a game read"
        );

        let (everywhere, wider) = draw(&root, 100, Splits::Everywhere, 1, &[], "second").unwrap();
        assert_eq!(
            wider.split, 1,
            "the training game's disagreement was left out; the frozen game's own went into the \
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

        let (none, quiet) = draw(&root, 100, Splits::None, 1, &[], "second").unwrap();
        assert_eq!(quiet.split, 0);
        assert!(none.iter().all(|question| question.shown.is_none()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_blind_sample_does_not_depend_on_where_the_second_labelling_got_to() {
        // Drawing blind only from claims nobody had read twice made the accuracy figure a
        // figure about whichever games the second pass had not reached. In the real set that
        // was four of the ten frozen games, 486 of 1,000 claims from one of them, and
        // finishing those four would have emptied the sample entirely. Coverage of the second
        // reading is a fact about scheduling; it must not decide what a person is asked.
        let root = std::env::temp_dir().join(format!("steamgauge-blind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);

        // 214490 is frozen and its second labelling covers two of three claims. 1274570 is
        // also frozen and has no second labelling at all.
        let bare = root.join("1274570");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::copy(
            root.join("214490").join("sample.json"),
            bare.join("sample.json"),
        )
        .unwrap();
        let relabelled = std::fs::read_to_string(root.join("214490").join("labels.json"))
            .unwrap()
            .replace("214490", "1274570");
        std::fs::write(bare.join("labels.json"), relabelled).unwrap();

        let (questions, counts) = draw(&root, 100, Splits::None, 1, &[], "second").unwrap();
        let asked: std::collections::HashSet<u32> =
            questions.iter().map(|question| question.app_id).collect();
        assert!(
            asked.contains(&214_490) && asked.contains(&1_274_570),
            "a frozen game was left out of the blind sample because a second labeller had              reached it, which makes the sample a fact about scheduling"
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
    fn the_blind_sample_is_not_only_the_claims_the_labellers_agreed_on() {
        // Claims two labellers agreed on are the easy ones. If the disagreements are taken for
        // the split pool before the blind sample is drawn, the sample is left with nothing but
        // agreed claims and the accuracy figure is the flattering half of the corpus. Once
        // every frozen game had been read twice that is exactly what happened: 1,000 of 1,000
        // blind claims were agreed ones.
        let root = std::env::temp_dir().join(format!("steamgauge-bias-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        a_reference_set(&root);

        // One claim short of the three the frozen game has, so the draw has to choose.
        let (questions, counts) = draw(&root, 3, Splits::Everywhere, 1, &[], "second").unwrap();
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

        let drawn = serde_json::json!([{
            "id": "r1", "app_id": 214_490, "language": "english", "subset": "random",
            "claims": [
                {"index": 0, "start": 0, "end": 9, "text": "Runs badly"},
                {"index": 1, "start": 10, "end": 20, "text": "Looks great"}
            ]
        }]);
        std::fs::write(root.join("214490").join("sample.json"), drawn.to_string()).unwrap();

        let label = |index: u16, subject: &str, doubted: bool| {
            serde_json::json!({
                "review_id": "r1", "index": index, "app_id": 214_490,
                "language": "english", "subset": "random", "start": 0, "end": 9,
                "splitter": "claims-5", "taxonomy": "core-6", "produced_by": "one",
                "subject": subject, "polarity": "praise", "ironic": false,
                "confidence": if doubted { "low" } else { "high" },
                "ambiguous": false, "split_wrong": false
            })
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

        let (questions, counts) = draw(&root, 0, Splits::Frozen, 1, &[], "second").unwrap();
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

    #[test]
    fn the_three_pieces_of_a_question_are_the_review_it_came_from() {
        // The page cannot check this: it is handed the pieces and has nothing to compare them
        // with. If the split is wrong the reader is shown a review that was never written,
        // with the wrong sentence highlighted in it, and nothing anywhere looks broken.
        let review = DrawnReview {
            id: "r1".to_owned(),
            app_id: 1,
            language: "schinese".to_owned(),
            subset: "random".to_owned(),
            claims: vec![
                crate::claimset::DrawnClaim {
                    index: 0,
                    start: 0,
                    end: 0,
                    text: "\u{6559}\u{5b66}\u{7eaf}\u{9760}\u{81ea}\u{5df1}\u{9886}\u{609f}"
                        .to_owned(),
                },
                crate::claimset::DrawnClaim {
                    index: 1,
                    start: 0,
                    end: 0,
                    text: "\u{6218}\u{6597}\u{624b}\u{611f}\u{6781}\u{597d}".to_owned(),
                },
                crate::claimset::DrawnClaim {
                    index: 2,
                    start: 0,
                    end: 0,
                    text: "Worth it.".to_owned(),
                },
            ],
            asked: None,
        };
        let rejoined = Rejoined::of(&review);
        for claim in &review.claims {
            let (at, text) = rejoined
                .find(claim.index)
                .expect("the claim is in the review");
            let before = &rejoined.text[..at];
            let after = &rejoined.text[at + text.len()..];
            assert_eq!(
                format!("{before}{text}{after}"),
                rejoined.text,
                "claim {} does not put the review back together",
                claim.index
            );
            assert_eq!(text, claim.text, "claim {} is not itself", claim.index);
        }
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
        }];
        let page = render(&questions, &GoldDraw::default());
        assert!(page.contains("Runs badly"));
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
        }];
        let page = render(&questions, &GoldDraw::default());
        assert!(
            page.contains("\"shown\":null"),
            "an answer on the page is an answer in the reader's head"
        );
    }
}
