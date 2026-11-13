//! A kept review put back to the text its labels were drawn on.
//!
//! A label names bytes, and the samples hold the words of every claim at its bytes. The text
//! kept for a review can read otherwise than the one drawn from: the store returned censor marks
//! over words the web API gives plain, and a later capture can hold an edit. No capture from the
//! day of a draw is left, so the draw's text is rebuilt from the samples and the kept text.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::Result;
use crate::claims::{holds_words, same_words};
use crate::claimset::{ClaimLabel, DrawnClaim, DrawnReview, REVIEWS, kept_reviews};

/// What [`restore`] did to a game.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Restored {
    /// Reviews whose kept text was replaced by the text they were drawn on.
    pub rebuilt: usize,
    /// Reviews whose kept text does not hold every claim and cannot be rebuilt whole.
    pub left_as_kept: usize,
    /// Reviews in a sample with no kept text.
    pub no_kept_text: usize,
    /// Labels that now name the bytes of their claim.
    pub labels_moved: usize,
}

impl std::ops::AddAssign for Restored {
    fn add_assign(&mut self, rhs: Self) {
        self.rebuilt += rhs.rebuilt;
        self.left_as_kept += rhs.left_as_kept;
        self.no_kept_text += rhs.no_kept_text;
        self.labels_moved += rhs.labels_moved;
    }
}

/// One `labels.json`, each label beside the claim of its own set's sample that it answers.
pub(crate) struct LabelFile {
    pub path: PathBuf,
    pub labels: Vec<(ClaimLabel, Option<DrawnClaim>)>,
}

/// The claims written as bytes: each claim whose words are as many bytes as its span, at its span,
/// joined where they touch or overlap. `None` where a span ends before it starts or the joined
/// bytes are not text.
fn runs_written(claims: &[DrawnClaim]) -> Option<Vec<(usize, String)>> {
    let mut written: Vec<(usize, &str)> = Vec::new();
    for claim in claims {
        let span = claim.end.checked_sub(claim.start)?;
        if !claim.text.is_empty() && usize::try_from(span).ok()? == claim.text.len() {
            written.push((usize::try_from(claim.start).ok()?, &claim.text));
        }
    }
    written.sort_by_key(|&(start, _)| start);
    let mut runs: Vec<(usize, Vec<u8>)> = Vec::new();
    for (start, text) in written {
        match runs.last_mut() {
            Some((from, bytes)) if start <= *from + bytes.len() => {
                // Where two claims hold other bytes at one place, the later claim is not held
                // at its span in the result, which `rebuilt` refuses.
                let covered = (*from + bytes.len() - start).min(text.len());
                bytes.extend_from_slice(&text.as_bytes()[covered..]);
            }
            _ => runs.push((start, text.as_bytes().to_vec())),
        }
    }
    runs.into_iter()
        .map(|(start, bytes)| Some((start, String::from_utf8(bytes).ok()?)))
        .collect()
}

/// The review as it read when `claims` were drawn from it: every claim's words at its own bytes,
/// and the kept text everywhere else.
///
/// Nothing is searched for. The kept text is read in step with the written bytes, so it gives as
/// many bytes as the samples leave before and between them, and each run must be the
/// [`same_words`] of the kept characters at its place. `None` where it is not, where the kept text
/// ends before a run, where a stretch does not end on a character, or where a claim of any sample
/// is not held at its span in the result.
#[must_use]
pub fn rebuilt(kept: &str, claims: &[DrawnClaim]) -> Option<String> {
    let runs = runs_written(claims)?;
    if runs.is_empty() {
        return None;
    }
    let mut text = String::with_capacity(kept.len());
    let mut taken = 0_usize;
    for (start, words) in &runs {
        let gap = start.checked_sub(text.len())?;
        let stretch = kept.get(taken..taken.checked_add(gap)?)?;
        taken += gap;
        let end = kept[taken..]
            .char_indices()
            .nth(words.chars().count())
            .map_or(kept.len(), |(at, _)| taken + at);
        if !same_words(words, &kept[taken..end]) {
            return None;
        }
        text.push_str(stretch);
        text.push_str(words);
        taken = end;
    }
    text.push_str(&kept[taken..]);
    claims
        .iter()
        .all(|claim| holds_words(&text, claim.start, claim.end, &claim.text))
        .then_some(text)
}

/// Every file called `name` under `game`, in path order.
fn files_named(game: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dirs = vec![game.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
        {
            if path.is_dir() {
                dirs.push(path);
            } else if path.file_name().is_some_and(|own| own == name) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn read_sample(path: &Path) -> Result<Vec<DrawnReview>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

/// Every `labels.json` under `game`, each label beside the claim at its index in its own set's
/// sample, or `None` where that sample holds none. A set with no `sample.json`, such as a
/// person's gold answers, takes the game's own sample.
pub(crate) fn label_files(game: &Path) -> Result<Vec<LabelFile>> {
    let own = game.join("sample.json");
    let mut files = Vec::new();
    for path in files_named(game, "labels.json") {
        let beside = path.with_file_name("sample.json");
        let sample = if beside.is_file() {
            Some(beside)
        } else {
            own.is_file().then(|| own.clone())
        };
        let mut claims: BTreeMap<(String, u16), DrawnClaim> = BTreeMap::new();
        if let Some(sample) = sample {
            for review in read_sample(&sample)? {
                for claim in review.claims {
                    claims.insert((review.id.clone(), claim.index), claim);
                }
            }
        }
        let labels: Vec<ClaimLabel> = serde_json::from_slice(&std::fs::read(&path)?)?;
        files.push(LabelFile {
            path,
            labels: labels
                .into_iter()
                .map(|label| {
                    let claim = claims.get(&(label.review_id.clone(), label.index)).cloned();
                    (label, claim)
                })
                .collect(),
        });
    }
    Ok(files)
}

/// Puts every review of `game` whose kept text does not hold its claims back to the text it was
/// drawn on, and every label of such a review on the bytes of its claim.
///
/// A review that cannot be rebuilt whole, or has no kept text, keeps its line. The labels are
/// written before `reviews.jsonl`, so a stop between the two leaves a game that the next run
/// finishes. A second run changes nothing.
///
/// # Errors
///
/// Fails if a sample, a label file or the kept text cannot be read, or a file cannot be written.
pub fn restore(game: &Path) -> Result<Restored> {
    let mut claims: BTreeMap<String, Vec<DrawnClaim>> = BTreeMap::new();
    for path in files_named(game, "sample.json") {
        for review in read_sample(&path)? {
            claims.entry(review.id).or_default().extend(review.claims);
        }
    }
    let mut kept = match kept_reviews(game) {
        Err(crate::Error::NoKeptText { .. }) => std::collections::HashMap::new(),
        other => other?,
    };

    let mut done = Restored::default();
    let mut rebuilt_ids = std::collections::HashSet::new();
    for (id, drawn) in &claims {
        let Some(review) = kept.get_mut(id) else {
            done.no_kept_text += 1;
            continue;
        };
        if drawn
            .iter()
            .all(|claim| holds_words(&review.text, claim.start, claim.end, &claim.text))
        {
            continue;
        }
        if let Some(text) = rebuilt(&review.text, drawn) {
            review.text = text;
            rebuilt_ids.insert(id.clone());
            done.rebuilt += 1;
        } else {
            done.left_as_kept += 1;
        }
    }
    if rebuilt_ids.is_empty() {
        return Ok(done);
    }

    for mut file in label_files(game)? {
        let mut moved = 0;
        for (label, claim) in &mut file.labels {
            if let Some(claim) = claim
                && rebuilt_ids.contains(&label.review_id)
                && (label.start, label.end) != (claim.start, claim.end)
            {
                label.start = claim.start;
                label.end = claim.end;
                moved += 1;
            }
        }
        if moved > 0 {
            let labels: Vec<&ClaimLabel> = file.labels.iter().map(|(label, _)| label).collect();
            crate::whole_file::write(&file.path, &serde_json::to_vec_pretty(&labels)?)?;
            done.labels_moved += moved;
        }
    }

    let mut ordered: Vec<_> = kept.values().collect();
    ordered.sort_by(|left, right| left.review_id.cmp(&right.review_id));
    let mut body = Vec::new();
    for review in ordered {
        serde_json::to_writer(&mut body, review)?;
        body.push(b'\n');
    }
    crate::whole_file::write(&game.join(REVIEWS), &body)?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks(count: usize) -> String {
        "\u{2665}".repeat(count)
    }

    fn claim(index: u16, start: u32, end: u32, text: &str) -> DrawnClaim {
        DrawnClaim {
            index,
            start,
            end,
            text: text.to_owned(),
        }
    }

    fn a_censored_pair() -> Vec<DrawnClaim> {
        vec![
            claim(0, 0, 33, &format!("{} incredible.", marks(7))),
            claim(1, 34, 53, "However, some bugs."),
        ]
    }

    #[test]
    fn a_censored_word_is_put_back_over_the_plain_one() {
        let text = rebuilt(
            "Fucking incredible. However, some bugs.",
            &a_censored_pair(),
        );
        assert_eq!(
            text,
            Some(format!("{} incredible. However, some bugs.", marks(7)))
        );
    }

    #[test]
    fn a_review_edited_since_it_was_drawn_is_left_as_kept() {
        let claims = [
            claim(0, 0, 11, "Great game."),
            claim(1, 12, 26, "Runs well now."),
        ];
        assert_eq!(rebuilt("Great game. Runs badly now.", &claims), None);
    }

    #[test]
    fn bytes_of_another_length_are_not_carried_over() {
        let claims = [claim(0, 0, 5, "Good."), claim(1, 6, 10, "Bad.")];
        assert_eq!(rebuilt("Good.\n\n\nBad.", &claims), None);
        assert_eq!(
            rebuilt("Wow!!\nGreat game.", &[claim(0, 4, 15, "Great game.")]),
            None
        );
    }

    #[test]
    fn a_first_claim_at_byte_0_is_read_at_the_kept_texts_first_byte() {
        let claims = [
            claim(0, 0, 11, "Great game."),
            claim(1, 12, 22, "Runs well."),
        ];
        assert_eq!(
            rebuilt("Edit: fixed.\nGreat game. Runs well.", &claims),
            None
        );
    }

    #[test]
    fn two_claims_holding_other_words_at_one_place_give_nothing() {
        let claims = [claim(0, 0, 5, "Good."), claim(1, 0, 5, "Fine.")];
        assert_eq!(rebuilt("Good.", &claims), None);
    }

    #[test]
    fn a_censored_claim_handed_out_without_its_markup_is_held_by_the_plain_words() {
        let claims = [
            claim(0, 0, 17, &format!("{} good.", marks(4))),
            claim(1, 18, 23, "Fine."),
        ];
        let kept = "[b]Damn[/b] good. Fine.";
        assert_eq!(rebuilt(kept, &claims).as_deref(), Some(kept));
    }

    #[test]
    fn the_kept_text_follows_the_last_claim_written() {
        let claims = [
            claim(0, 0, 33, &format!("{} incredible.", marks(7))),
            claim(1, 34, 50, "Fine end."),
        ];
        assert_eq!(
            rebuilt("Fucking incredible. [b]Fine[/b] end.", &claims),
            Some(format!("{} incredible. [b]Fine[/b] end.", marks(7)))
        );
    }

    fn a_label(review_id: &str, index: u16, start: u32, end: u32) -> ClaimLabel {
        ClaimLabel {
            review_id: review_id.to_owned(),
            index,
            app_id: 1,
            language: "english".to_owned(),
            subset: "random".to_owned(),
            start,
            end,
            taxonomy: String::new(),
            produced_by: "a-labeller".to_owned(),
            subject: "gameplay".to_owned(),
            polarity: "praise".to_owned(),
            ironic: false,
            confidence: "high".to_owned(),
            ambiguous: false,
            split_wrong: false,
            also: None,
        }
    }

    fn a_sample(reviews: &[(&str, Vec<DrawnClaim>)]) -> Vec<u8> {
        let drawn: Vec<DrawnReview> = reviews
            .iter()
            .map(|(id, claims)| DrawnReview {
                id: (*id).to_owned(),
                app_id: 1,
                language: "english".to_owned(),
                subset: "random".to_owned(),
                claims: claims.clone(),
                asked: None,
                text: String::new(),
            })
            .collect();
        serde_json::to_vec_pretty(&drawn).unwrap()
    }

    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn kept_lines(game: &Path) -> Vec<String> {
        std::fs::read_to_string(game.join(REVIEWS))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn labels_in(path: &Path) -> Vec<ClaimLabel> {
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    fn files_of(game: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut found: Vec<(PathBuf, Vec<u8>)> = ["labels.json", "sample.json", REVIEWS]
            .iter()
            .flat_map(|name| files_named(game, name))
            .map(|path| {
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        found.sort();
        found
    }

    /// A game with five reviews: `1` and `3` censored in the samples, `2` as drawn, `4` with no
    /// kept text and `5` edited since it was drawn. Review `1` is labelled in four sets.
    fn a_game_to_restore() -> PathBuf {
        let game = std::env::temp_dir()
            .join(format!("steamgauge-restore-{}", std::process::id()))
            .join("1");
        let _ = std::fs::remove_dir_all(&game);

        let first = a_censored_pair();
        let third = vec![
            claim(0, 0, 5, "Fine."),
            claim(1, 6, 34, &format!("{} awful.", marks(7))),
        ];
        let second = vec![claim(0, 0, 5, "Fine.")];
        let fifth = vec![
            claim(0, 0, 11, "Great game."),
            claim(1, 12, 26, "Runs well now."),
        ];
        let samples = a_sample(&[
            ("1", first.clone()),
            ("2", second.clone()),
            ("3", third),
            ("4", vec![claim(0, 0, 5, "Hello")]),
            ("5", fifth),
        ]);
        put(&game.join("sample.json"), &samples);
        put(
            &game.join("second").join("sample.json"),
            &a_sample(&[("1", first), ("2", second.clone())]),
        );
        put(
            &game.join("keyword").join("sample.json"),
            &a_sample(&[("2", second)]),
        );

        let kept = [
            ("5", "Great game. Runs badly now."),
            ("3", "Fine. Fucking awful."),
            ("2", "Fine."),
            ("1", "Fucking incredible. However, some bugs."),
        ];
        let mut body = String::new();
        for (id, text) in kept {
            body.push_str(
                &serde_json::to_string(&crate::claimset::KeptReview {
                    review_id: id.to_owned(),
                    language: "english".to_owned(),
                    text: text.to_owned(),
                })
                .unwrap(),
            );
            body.push('\n');
        }
        put(&game.join(REVIEWS), body.as_bytes());

        let moved = a_label("1", 1, 20, 39);
        let mut sheet = moved.clone();
        sheet.taxonomy = "x".to_owned();
        let with_empty_key = serde_json::to_string_pretty(&[sheet, a_label("3", 0, 0, 5)])
            .unwrap()
            .replace("\"taxonomy\": \"x\"", "\"taxonomy\": \"\"");
        put(&game.join("labels.json"), with_empty_key.as_bytes());
        for set in ["second", "gold/ann", "keyword"] {
            put(
                &game.join(set).join("labels.json"),
                &serde_json::to_vec_pretty(std::slice::from_ref(&moved)).unwrap(),
            );
        }
        game
    }

    #[test]
    fn restoring_a_game_rebuilds_its_censored_reviews_and_puts_their_labels_back() {
        let game = a_game_to_restore();
        let moved = a_label("1", 1, 20, 39);
        let staying = a_label("3", 0, 0, 5);
        let before = files_of(&game);
        let review_two = kept_lines(&game)[2].clone();
        let done = restore(&game).unwrap();
        assert_eq!(
            done,
            Restored {
                rebuilt: 2,
                left_as_kept: 1,
                no_kept_text: 1,
                labels_moved: 3,
            }
        );

        let lines = kept_lines(&game);
        let ids: Vec<&str> = lines
            .iter()
            .map(|line| line.split('"').nth(3).unwrap())
            .collect();
        assert_eq!(ids, ["1", "2", "3", "5"], "written in review id order");
        assert!(lines[0].contains(&format!("{} incredible. However, some bugs.", marks(7))));
        assert_eq!(lines[1], review_two, "a review not rebuilt keeps its line");
        assert!(lines[2].contains(&marks(7)));
        assert!(lines[3].contains("Great game. Runs badly now."));

        let after_moved = a_label("1", 1, 34, 53);
        assert_eq!(
            labels_in(&game.join("labels.json")),
            [after_moved.clone(), staying]
        );
        for set in ["second", "gold/ann"] {
            assert_eq!(
                labels_in(&game.join(set).join("labels.json")),
                std::slice::from_ref(&after_moved)
            );
        }
        assert_eq!(
            labels_in(&game.join("keyword").join("labels.json")),
            [moved],
            "a set whose sample does not hold the review keeps its label"
        );
        for set in ["", "second", "gold/ann"] {
            let text = std::fs::read_to_string(game.join(set).join("labels.json")).unwrap();
            assert!(!text.contains("taxonomy"), "{set}: {text}");
        }

        let rewritten = files_of(&game);
        let changed: Vec<&PathBuf> = rewritten
            .iter()
            .zip(&before)
            .filter(|(now, was)| now != was)
            .map(|(now, _)| &now.0)
            .collect();
        assert_eq!(changed.len(), 4, "{changed:?}");

        let again = restore(&game).unwrap();
        assert_eq!(
            again,
            Restored {
                rebuilt: 0,
                left_as_kept: 1,
                no_kept_text: 1,
                labels_moved: 0,
            }
        );
        assert_eq!(
            files_of(&game),
            rewritten,
            "a second restore changes nothing"
        );
        std::fs::remove_dir_all(game.parent().unwrap()).unwrap();
    }
}
