//! The commands that take a labeller's answers back, score them or export them: `ingest-claims`,
//! `ingest-revisit`, `ingest-gold`, `ingest-induced`, `measure-claims`, `compare-labels`,
//! `ceiling`, `export-training` and `export-review-facts`.

use std::path::Path;

use serde_json::{Value, json};
use steamgauge_stand_ins::tempdir::Dir;

use crate::ran;
use crate::tiny::{HELD_OUT, TRAINED, labelled};

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

/// The JSON array a command wrote.
fn array(path: &Path) -> Vec<Value> {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The lines of a JSONL file, each parsed.
fn rows(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// One returned label for the only claim of a review, in the form a labeller writes it.
fn returned(review: &str, subject: &str, polarity: &str) -> Value {
    json!({
        "review_id": review,
        "index": 0,
        "subject": subject,
        "polarity": polarity,
        "ironic": false,
        "confidence": "high",
        "ambiguous": false,
    })
}

/// Writes `answers` as the one file of a folder of returned labels.
fn returned_folder(parent: &Path, name: &str, answers: &[Value]) -> std::path::PathBuf {
    let folder = parent.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("answers.json"),
        serde_json::to_vec(answers).unwrap(),
    )
    .unwrap();
    folder
}

/// The subject a label file holds for a review.
fn subject_of(labels: &[Value], review: &str) -> String {
    labels
        .iter()
        .find(|label| label["review_id"] == review)
        .unwrap()["subject"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn ingest_claims_files_what_came_back_under_its_labeller() {
    let dir = Dir::new();
    let (_, reference) = labelled(dir.path());
    let set = reference.join(TRAINED.to_string());
    std::fs::remove_file(set.join("labels.json")).unwrap();
    let from = returned_folder(
        dir.path(),
        "returned",
        &[
            returned("501", "bugs", "complaint"),
            returned("502", "graphics", "praise"),
            returned("503", "bugs", "complaint"),
            returned("504", "verdict", "neutral"),
        ],
    );

    let done = ran(
        dir.path(),
        &[
            "ingest-claims",
            "5",
            "--from",
            text(&from),
            "--by",
            " ",
            "--to",
            text(&set),
        ],
    );
    assert_ne!(done.code, 0, "{}", done.out);
    assert!(done.err.contains("cannot be empty"), "{}", done.err);
    assert!(!set.join("labels.json").exists());

    let done = ran(
        dir.path(),
        &[
            "ingest-claims",
            "5",
            "--from",
            text(&from),
            "--by",
            "a-model",
            "--to",
            text(&set),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    for line in [
        "accepted     4 claims",
        "contested    0",
        "polarity     praise 1, complaint 2, neutral 1",
        "commonest    bugs 2, graphics 1, verdict 1",
    ] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }

    let filed = array(&set.join("labels.json"));
    assert_eq!(filed.len(), 4);
    for label in &filed {
        assert_eq!(label["produced_by"], "a-model");
        assert_eq!(label["taxonomy"], steamgauge_core::taxonomy::sheet());
    }
}

#[test]
fn ingest_revisit_replaces_only_what_was_asked() {
    let dir = Dir::new();
    let (_, reference) = labelled(dir.path());
    let set = reference.join(TRAINED.to_string());
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
    let from = returned_folder(
        dir.path(),
        "answers",
        &[
            returned("501", "performance", "complaint"),
            returned("503", "bugs", "complaint"),
        ],
    );

    let done = ran(
        dir.path(),
        &[
            "ingest-revisit",
            "5",
            "--from",
            text(&from),
            "--by",
            "a-model",
            "--to",
            text(&set),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    for line in [
        "revisited  2 claims",
        "moved      1 of them to another subject",
    ] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }

    let filed = array(&set.join("labels.json"));
    assert_eq!(filed.len(), 4);
    for (review, subject) in [
        ("501", "performance"),
        ("503", "bugs"),
        ("502", "graphics"),
        ("504", "verdict"),
    ] {
        assert_eq!(subject_of(&filed, review), subject, "{review}");
    }
}

#[test]
fn ingest_gold_files_a_persons_answer_through_the_program() {
    let dir = Dir::new();
    let (_, reference) = labelled(dir.path());
    let answers = dir.path().join("answers.json");
    std::fs::write(
        &answers,
        serde_json::to_vec(&[json!({
            "app_id": HELD_OUT,
            "review_id": "2701",
            "index": 0,
            "subject": "bugs",
            "polarity": "complaint",
            "sheet": steamgauge_core::taxonomy::sheet(),
            "by": "one",
        })])
        .unwrap(),
    )
    .unwrap();

    let done = ran(
        dir.path(),
        &[
            "ingest-gold",
            "--from",
            text(&answers),
            "--reference",
            text(&reference),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    assert!(
        has_fields(
            &done.out,
            &["27", "1", "adjudicated,", "1", "held", "for", "one"]
        ),
        "{}",
        done.out
    );
    for line in [
        "gold       1 claims by one",
        "agreement  1 of them (100.0%) match the labeller already on record",
    ] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }

    let set = reference.join(HELD_OUT.to_string());
    let filed = array(&steamgauge_core::gold::answers_of(&set, "one").join("labels.json"));
    assert_eq!(filed.len(), 1);
    assert_eq!(filed[0]["review_id"], "2701");
    assert_eq!(filed[0]["produced_by"], "one");
    assert_eq!(filed[0]["subject"], "bugs");
}

#[test]
fn ingest_induced_keeps_a_subject_three_reviews_support() {
    let dir = Dir::new();
    let handout: Vec<steamgauge_core::diverse::Handout> = ["501", "502", "503", "504"]
        .iter()
        .map(|id| steamgauge_core::diverse::Handout {
            review_id: (*id).to_owned(),
            language: "english".to_owned(),
            novelty: 0.5,
            review: format!("review {id}"),
        })
        .collect();
    let handout_file = dir.path().join("handout.json");
    std::fs::write(&handout_file, serde_json::to_vec(&handout).unwrap()).unwrap();
    let subject = |id: &str, evidence: &[&str]| {
        json!({
            "id": id,
            "label": id,
            "description": "a subject the sheet has no row for",
            "evidence": evidence,
        })
    };
    let returned_file = dir.path().join("returned.json");
    std::fs::write(
        &returned_file,
        serde_json::to_vec(&json!({
            "induced_by": "a-model",
            "subjects": [
                subject("modding", &["501", "502", "503"]),
                subject("two-only", &["501", "502"]),
                subject("stranger", &["501", "502", "503", "9999"]),
            ],
        }))
        .unwrap(),
    )
    .unwrap();
    let kept = dir.path().join("induced").join("5.json");

    let done = ran(
        dir.path(),
        &[
            "ingest-induced",
            "5",
            "--from",
            text(&returned_file),
            "--handout",
            text(&handout_file),
            "--to",
            text(&kept),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    for line in ["handout      4 reviews", "kept         1 of 3 subjects"] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }
    let (_, refused) = done.out.split_once("\nrefused\n").unwrap();
    assert!(refused.contains("two-only"), "{}", done.out);
    assert!(
        refused.lines().any(|line| line.contains("stranger")
            && line.contains("1 of its evidence names reviews not in the handout")),
        "{}",
        done.out
    );
    assert!(!refused.contains("modding"), "{}", done.out);

    let filed: Value = serde_json::from_slice(&std::fs::read(&kept).unwrap()).unwrap();
    assert_eq!(filed["handout_size"], 4);
    assert_eq!(filed["seed"], 1);
    let subjects = filed["subjects"].as_array().unwrap();
    assert_eq!(subjects.len(), 1);
    assert_eq!(subjects[0]["id"], "modding");
}

#[test]
fn compare_labels_counts_where_two_readings_differ() {
    let dir = Dir::new();
    let (_, reference) = labelled(dir.path());
    let done = ran(
        dir.path(),
        &["compare-labels", "5", "--reference", text(&reference)],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    for line in ["app 5", "  4 claims read twice, 0 only once"] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }
    assert!(
        done.out.lines().any(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            fields.first() == Some(&"subject")
                && fields.contains(&"75.0%")
                && fields.contains(&"verdict")
                && fields.contains(&"offtopic")
        }),
        "{}",
        done.out
    );
}

#[test]
fn measure_claims_scores_what_the_reader_answered() {
    let dir = Dir::new();
    let (library, reference) = labelled(dir.path());
    let done = ran(
        dir.path(),
        &[
            "measure-claims",
            "5",
            "--out",
            text(&library),
            "--reference",
            text(&reference),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    for line in [
        "  4 labelled claims found in the readings, 3 answered, 1 declined (25.0%)",
        "  100.0% of polarity",
    ] {
        assert!(has_line(&done.out, line), "{line}\n{}", done.out);
    }
    assert!(
        done.out
            .lines()
            .any(|line| line.starts_with("  100.0% agreement on what it answered")),
        "{}",
        done.out
    );
    assert!(
        done.out.contains("These are AGREEMENT figures"),
        "{}",
        done.out
    );
}

#[test]
fn ceiling_scores_a_trained_game_apart() {
    let dir = Dir::new();
    let (library, reference) = labelled(dir.path());
    let done = ran(
        dir.path(),
        &[
            "ceiling",
            "5",
            "--out",
            text(&library),
            "--reference",
            text(&reference),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    let role = steamgauge_core::measure::Role::Train.as_str();
    assert!(
        has_fields(
            &done.out,
            &["5", role, "3", "100.0%", "100.0%", "-", "of", "0"]
        ),
        "{}",
        done.out
    );
    let block = "1 games the model trained on, 3 claims read twice and answered";
    assert!(has_line(&done.out, block), "{block}\n{}", done.out);
    assert!(!done.out.contains("never saw"), "{}", done.out);
}

#[test]
fn export_training_writes_every_labelled_claim() {
    let dir = Dir::new();
    let (_, reference) = labelled(dir.path());
    let to = dir.path().join("training").join("claims.jsonl");
    let done = ran(
        dir.path(),
        &[
            "export-training",
            "--from",
            text(&reference),
            "--to",
            text(&to),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    let line = format!("6 labelled claims -> {}", to.display());
    assert!(has_line(&done.out, &line), "{line}\n{}", done.out);
    let written = rows(&to);
    assert_eq!(written.len(), 6);
    let of_held_out = written
        .iter()
        .filter(|row| row["app_id"] == HELD_OUT)
        .count();
    assert_eq!(of_held_out, 2);
    assert!(
        written.iter().all(|row| row["headset_only"] == false),
        "each row carries its set's store facts"
    );
}

#[test]
fn export_review_facts_writes_the_labelled_reviews_alone() {
    let dir = Dir::new();
    let (library, reference) = labelled(dir.path());
    let to = dir.path().join("facts.jsonl");
    let done = ran(
        dir.path(),
        &[
            "export-review-facts",
            "--from",
            text(&reference),
            "--out",
            text(&library),
            "--to",
            text(&to),
        ],
    );
    assert_eq!(done.code, 0, "{}", done.err);
    let line = format!("6 labelled reviews -> {}", to.display());
    assert!(has_line(&done.out, &line), "{line}\n{}", done.out);
    let written = rows(&to);
    assert_eq!(written.len(), 6);
    assert_eq!(written[0]["app_id"], TRAINED);
    assert_eq!(written[0]["review_id"], "501");
    assert!(
        !written
            .iter()
            .any(|row| row["app_id"] == TRAINED && row["review_id"] == "505"),
        "505 names no label"
    );
}
