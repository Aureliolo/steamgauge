//! The commands that put claims in front of a labeller: `sample-claims`, `declined`,
//! `rare-claims`, `second-opinion`, `revisit`, `gold` and `export-pool`.

use std::path::Path;

use serde_json::Value;
use steamgauge_stand_ins::tempdir::Dir;

use crate::ran;
use crate::tiny::{captured, labelled, read_library};

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn has_line(out: &str, line: &str) -> bool {
    out.lines().any(|found| found == line)
}

/// Whether a line holds exactly these fields, whatever the padding between them.
fn has_fields(out: &str, fields: &[&str]) -> bool {
    out.lines()
        .any(|line| line.split_whitespace().eq(fields.iter().copied()))
}

/// The review ids of a written `sample.json`, each with the subset it was filed under, in id
/// order because a second opinion writes its reviews in the order of a hash.
fn drawn(sample: &Path) -> Vec<(String, String)> {
    let rows: Vec<Value> = serde_json::from_slice(&std::fs::read(sample).unwrap()).unwrap();
    let mut found: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            (
                row["id"].as_str().unwrap().to_owned(),
                row["subset"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    found.sort();
    found
}

fn all_as(ids: &[&str], subset: &str) -> Vec<(String, String)> {
    ids.iter()
        .map(|id| ((*id).to_owned(), subset.to_owned()))
        .collect()
}

#[test]
fn sample_claims_draws_every_review_of_a_small_game() {
    let dir = Dir::new();
    let library = captured(dir.path());
    let reference = dir.path().join("reference");
    let done = ran(
        dir.path(),
        &[
            "sample-claims",
            "5",
            "--out",
            text(&library),
            "--to",
            text(&reference),
            "--reviews",
            "10",
            "--english",
            "1",
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    let first = done.out.lines().next().unwrap();
    assert_eq!(
        first.split_whitespace().collect::<Vec<_>>(),
        [
            "5", "5", "reviews", "5", "claims", "1", "batches", "1.00", "per", "review"
        ],
        "{}",
        done.out
    );
    for line in ["drawn        5 reviews", "languages    english 5"] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }
    assert_eq!(
        drawn(&reference.join("5").join("sample.json")),
        all_as(&["501", "502", "503", "504", "505"], "random")
    );

    let refused = dir.path().join("refused");
    let done = ran(
        dir.path(),
        &[
            "sample-claims",
            "5",
            "--out",
            text(&library),
            "--to",
            text(&refused),
            "--subset",
            "nosuch",
        ],
    );
    assert_ne!(done.code, 0, "{}", done.out);
    assert!(
        done.err.contains("is not a draw this build knows"),
        "{}",
        done.err
    );
    assert!(!refused.exists());
}

#[test]
fn declined_draws_what_the_reader_would_not_answer() {
    let dir = Dir::new();
    let library = read_library(dir.path());
    let reference = dir.path().join("reference");
    let done = ran(
        dir.path(),
        &[
            "declined",
            "5",
            "--out",
            text(&library),
            "--reference",
            text(&reference),
            "--claims",
            "10",
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_fields(
            &done.out,
            &["5", "2", "reviews", "2", "claims", "1", "batches"]
        ),
        "{}",
        done.out
    );
    assert!(
        has_fields(
            &done.out,
            &["drawn", "2", "reviews", "2", "claims", "1", "batches"]
        ),
        "{}",
        done.out
    );
    assert_eq!(
        drawn(&reference.join("5").join("declined").join("sample.json")),
        all_as(&["504", "505"], "declined")
    );

    let refused = dir.path().join("refused");
    let done = ran(
        dir.path(),
        &[
            "declined",
            "27",
            "--out",
            text(&library),
            "--reference",
            text(&refused),
        ],
    );
    assert_ne!(done.code, 0, "{}", done.out);
    assert!(
        done.err.contains("27 is held back from training"),
        "{}",
        done.err
    );
    assert!(!refused.join("27").exists());
}

#[test]
fn rare_claims_draws_by_the_words_of_the_subject_named() {
    let dir = Dir::new();
    let library = read_library(dir.path());
    let reference = dir.path().join("reference");
    let done = ran(
        dir.path(),
        &[
            "rare-claims",
            "5",
            "--out",
            text(&library),
            "--reference",
            text(&reference),
            "--only",
            "licensing",
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(has_fields(&done.out, &["licensing", "1"]), "{}", done.out);
    assert_eq!(
        drawn(&reference.join("5").join("keyword").join("sample.json")),
        all_as(&["505"], "keyword")
    );

    let refused = dir.path().join("refused");
    let done = ran(
        dir.path(),
        &[
            "rare-claims",
            "5",
            "--out",
            text(&library),
            "--reference",
            text(&refused),
            "--only",
            "nosuch",
        ],
    );
    assert_ne!(done.code, 0, "{}", done.out);
    assert!(
        done.err.contains("nosuch has no keyword list"),
        "{}",
        done.err
    );
}

#[test]
fn second_opinion_writes_the_set_again_without_its_labels() {
    let dir = Dir::new();
    let (_, reference) = labelled(dir.path());
    let second_labels = reference.join("5").join("second").join("labels.json");
    let before = std::fs::read(&second_labels).unwrap();
    let done = ran(
        dir.path(),
        &[
            "second-opinion",
            "5",
            "--reference",
            text(&reference),
            "--share",
            "1",
            "--into",
            "third",
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_line(&done.out, "drawn        4 reviews, 4 claims, 1 batches"),
        "{}",
        done.out
    );
    let third = reference.join("5").join("third");
    assert_eq!(
        drawn(&third.join("sample.json")),
        all_as(&["501", "502", "503", "504"], "random")
    );
    assert!(!third.join("labels.json").exists());
    assert_eq!(std::fs::read(&second_labels).unwrap(), before);
}

#[test]
fn revisit_draws_the_claims_that_use_the_word() {
    let dir = Dir::new();
    let (_, reference) = labelled(dir.path());
    let done = ran(
        dir.path(),
        &[
            "revisit",
            "5",
            "--words",
            "nosuchword",
            "--reference",
            text(&reference),
        ],
    );
    assert_ne!(done.code, 0, "{}", done.out);
    assert!(done.err.contains("nothing to revisit"), "{}", done.err);
    assert!(!reference.join("5").join("revisit").exists());

    let done = ran(
        dir.path(),
        &[
            "revisit",
            "5",
            "--words",
            "crashes",
            "--reference",
            text(&reference),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_fields(
            &done.out,
            &["5", "2", "reviews", "2", "claims", "1", "batches"]
        ),
        "{}",
        done.out
    );
    assert!(
        has_fields(
            &done.out,
            &["drawn", "2", "reviews", "2", "claims", "over", "1", "games"]
        ),
        "{}",
        done.out
    );
    assert_eq!(
        drawn(&reference.join("5").join("revisit").join("sample.json")),
        all_as(&["501", "503"], "random")
    );
}

#[test]
fn gold_asks_about_the_held_out_game_alone() {
    let dir = Dir::new();
    let (library, reference) = labelled(dir.path());
    let asked = |name: &str, splits: &str| {
        let page = dir.path().join(name);
        let done = ran(
            dir.path(),
            &[
                "gold",
                "--for",
                "one",
                "--reference",
                text(&reference),
                "--out",
                text(&library),
                "--to",
                text(&page),
                "--blind",
                "10",
                "--splits",
                splits,
            ],
        );
        assert_eq!(done.code, 0, "{}", done.err);
        for line in [
            "games      1 held out".to_owned(),
            format!("page       {}", page.display()),
        ] {
            assert!(has_line(&done.out, &line), "{line}\n{}", done.out);
        }
        std::fs::read_to_string(&page).unwrap()
    };
    let asks = |html: &str, id: &str| html.contains(&format!("\"review_id\":\"{id}\""));

    // The held-out game's agreed claim is drawn blind and its split is asked. The trained
    // game's split and its agreed claims are left out.
    let held_out = asked("held-out.html", "held-out");
    assert!(held_out.contains("daily crashes hate"));
    assert!(!held_out.contains("love pretty"));
    assert!(asks(&held_out, "2701") && asks(&held_out, "2702"));
    for id in ["501", "502", "503", "504"] {
        assert!(!asks(&held_out, id), "{id}");
    }

    // With every game's splits in, the trained game's split is asked and its agreed claims
    // still never go to the blind sample.
    let everywhere = asked("everywhere.html", "everywhere");
    assert!(asks(&everywhere, "504"));
    for id in ["501", "502", "503"] {
        assert!(!asks(&everywhere, id), "{id}");
    }
}

#[test]
fn export_pool_leaves_out_what_is_labelled() {
    let dir = Dir::new();
    let (library, reference) = labelled(dir.path());
    let pool = dir.path().join("pool.jsonl");
    let done = ran(
        dir.path(),
        &[
            "export-pool",
            "5",
            "--out",
            text(&library),
            "--reference",
            text(&reference),
            "--to",
            text(&pool),
            "--reviews",
            "10",
            "--english",
            "1",
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_fields(
            &done.out,
            &[
                "5", "1", "reviews", "1", "claims", "4", "reviews", "already", "labelled"
            ]
        ),
        "{}",
        done.out
    );
    let line = format!("1 reviews 1 claims -> {}", pool.display());
    assert!(has_line(&done.out, &line), "{line}\n{}", done.out);
    let rows = std::fs::read_to_string(&pool).unwrap();
    assert_eq!(rows.lines().count(), 1, "{rows}");
    assert!(rows.contains("officially licensed cars"), "{rows}");
}
