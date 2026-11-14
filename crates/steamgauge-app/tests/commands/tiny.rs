//! The tiny library: two games of a few reviews each, a capture of them and a reader that
//! counts them. Each stage is built afresh in the folder it is given, so no test shares a file
//! with another.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// A game the split trains on.
pub(crate) const TRAINED: u32 = 5;
/// A game the split holds out.
pub(crate) const HELD_OUT: u32 = 27;
/// The name both games carry in their crawl record.
pub(crate) const NAME: &str = "Tiny Game";
/// The second the capture was taken, and the snapshot folder's name.
pub(crate) const CAPTURED: i64 = 1_700_000_000;

/// Each review as (game, id, text, recommends), all in English. The repeated text is one claim
/// twice, and `officially licensed cars` names no subject.
pub(crate) const REVIEWS: [(u32, &str, &str, bool); 7] = [
    (TRAINED, "501", "hate crashes", false),
    (TRAINED, "502", "love pretty", true),
    (TRAINED, "503", "hate crashes", false),
    (TRAINED, "504", "fine", true),
    (TRAINED, "505", "officially licensed cars", true),
    (HELD_OUT, "2701", "daily crashes hate", false),
    (HELD_OUT, "2702", "art pretty love", true),
];

/// A game's `REVIEWS` in the form Steam's API gives them, written one day before the capture.
pub(crate) fn as_steam_serves(app_id: u32) -> Vec<Value> {
    let written = CAPTURED - 86_400;
    REVIEWS
        .iter()
        .filter(|(game, ..)| *game == app_id)
        .map(|(_, id, text, recommends)| {
            json!({
                "recommendationid": id,
                "review": text,
                "language": "english",
                "voted_up": recommends,
                "timestamp_created": written,
                "timestamp_updated": written,
            })
        })
        .collect()
}

/// `<folder>/library`, with one shard, a crawl record and the store's answer for each game.
pub(crate) fn captured(folder: &Path) -> PathBuf {
    let library = folder.join("library");
    for app_id in [TRAINED, HELD_OUT] {
        let game = library.join(format!("appid={app_id}"));
        let snapshot = game.join(format!("snapshot={CAPTURED}"));
        std::fs::create_dir_all(&snapshot).unwrap();
        let rows = as_steam_serves(app_id);
        let mut writer = steamgauge_core::capture::CaptureWriter::create(
            &snapshot.join("shard-0000.parquet"),
            app_id,
        )
        .unwrap();
        writer.write(&rows.iter().collect::<Vec<_>>()).unwrap();
        writer.close().unwrap();
        // The nine fields a crawl record must hold, and its coverage. A sweep adds what it
        // fetched to `rows_unique`.
        std::fs::write(
            snapshot.join("crawl.json"),
            json!({
                "app_id": app_id,
                "name": NAME,
                "review_score_desc": "Mixed",
                "rows_unique": rows.len(),
                "valve_total_reviews": rows.len(),
                "valve_total_positive": rows.len(),
                "valve_total_negative": 0,
                "coverage": 1.0,
                "snapshot_unix": CAPTURED,
                "shards": 1,
            })
            .to_string(),
        )
        .unwrap();
        // With the store's answer kept, a read asks nobody.
        steamgauge_core::facts::Facts {
            headset_only: false,
        }
        .save(&game)
        .unwrap();
    }
    library
}

/// A tiny reader for the sheet's subjects, under `<folder>/reader`.
pub(crate) fn reader(folder: &Path) -> PathBuf {
    let reader = folder.join("reader");
    std::fs::create_dir_all(&reader).unwrap();
    let subjects: Vec<&str> = steamgauge_core::taxonomy::SHEET
        .iter()
        .map(|subject| subject.id)
        .collect();
    steamgauge_stand_ins::tiny_model::reader(&reader, &subjects, &json!({}));
    reader
}

/// `captured`, then each game read in this process with the tiny reader and saved as
/// `reading.json`.
pub(crate) fn read_library(folder: &Path) -> PathBuf {
    let library = captured(folder);
    let reader = reader(folder);
    let options = steamgauge_core::read::ReadOptions {
        out_dir: library.clone(),
        ..steamgauge_core::read::ReadOptions::default()
    };
    let mut model = steamgauge_core::reader::ClaimReader::load(&reader).unwrap();
    for app_id in [TRAINED, HELD_OUT] {
        let report =
            steamgauge_core::read::read_corpus(&mut model, app_id, &options, |_| {}).unwrap();
        let snapshot = library
            .join(format!("appid={app_id}"))
            .join(format!("snapshot={CAPTURED}"));
        report.save(&snapshot.join("reading.json")).unwrap();
    }
    library
}

/// What the first labeller and the second said of each labelled review, as (game, id, subject,
/// polarity, the second's subject). Game 5's `505` is left unlabelled on purpose, so a draw that
/// leaves out what is labelled has one review to keep. The second labeller disagrees on `504`
/// and `2702`, so each game has one claim that splits.
const LABELS: [(u32, &str, &str, &str, &str); 6] = [
    (TRAINED, "501", "bugs", "complaint", "bugs"),
    (TRAINED, "502", "graphics", "praise", "graphics"),
    (TRAINED, "503", "bugs", "complaint", "bugs"),
    (TRAINED, "504", "verdict", "neutral", "offtopic"),
    (HELD_OUT, "2701", "bugs", "complaint", "bugs"),
    (HELD_OUT, "2702", "graphics", "praise", "audio"),
];

/// The text a review of `REVIEWS` holds.
fn text_of(id: &str) -> &'static str {
    REVIEWS
        .iter()
        .find(|(_, own, ..)| *own == id)
        .map(|(_, _, text, _)| *text)
        .unwrap()
}

/// One label on the only claim of a review, naming the bytes that claim covers.
fn label(
    app_id: u32,
    review: &steamgauge_core::claimset::DrawnReview,
    by: &str,
    subject: &str,
    polarity: &str,
) -> steamgauge_core::claimset::ClaimLabel {
    let claim = &review.claims[0];
    steamgauge_core::claimset::ClaimLabel {
        review_id: review.id.clone(),
        index: claim.index,
        app_id,
        language: review.language.clone(),
        subset: review.subset.clone(),
        start: claim.start,
        end: claim.end,
        taxonomy: steamgauge_core::taxonomy::sheet(),
        produced_by: by.to_owned(),
        subject: subject.to_owned(),
        polarity: polarity.to_owned(),
        ironic: false,
        confidence: "high".to_owned(),
        ambiguous: false,
        split_wrong: false,
        also: None,
    }
}

/// `read_library` and, in `<folder>/reference`, each game's labelled set: the drawn sample, the
/// kept review texts, the store's answer, the first labeller's `labels.json` and the second's
/// under `second`.
/// Returns the library and the reference folder.
pub(crate) fn labelled(folder: &Path) -> (PathBuf, PathBuf) {
    let library = read_library(folder);
    let reference = folder.join("reference");
    for app_id in [TRAINED, HELD_OUT] {
        let game = reference.join(app_id.to_string());
        let rows: Vec<_> = LABELS.iter().filter(|row| row.0 == app_id).collect();
        let drawn: Vec<steamgauge_core::claimset::DrawnReview> = rows
            .iter()
            .map(|(_, id, ..)| {
                let text = text_of(id);
                steamgauge_core::claimset::DrawnReview {
                    id: (*id).to_owned(),
                    app_id,
                    language: "english".to_owned(),
                    subset: "random".to_owned(),
                    claims: steamgauge_core::claims::claims_of(text)
                        .into_iter()
                        .enumerate()
                        .map(
                            |(index, (at, claim))| steamgauge_core::claimset::DrawnClaim {
                                index: u16::try_from(index).unwrap(),
                                start: u32::try_from(at.start).unwrap(),
                                end: u32::try_from(at.end).unwrap(),
                                text: claim.into_owned(),
                            },
                        )
                        .collect(),
                    asked: None,
                    text: text.to_owned(),
                }
            })
            .collect();
        steamgauge_core::claimset::keep_reviews(
            &game,
            drawn
                .iter()
                .map(|review| steamgauge_core::claimset::KeptReview {
                    review_id: review.id.clone(),
                    language: review.language.clone(),
                    text: review.text.clone(),
                }),
        )
        .unwrap();
        std::fs::write(
            game.join("sample.json"),
            serde_json::to_vec(&drawn).unwrap(),
        )
        .unwrap();
        let first: Vec<_> = rows
            .iter()
            .zip(&drawn)
            .map(|((_, _, subject, polarity, _), review)| {
                label(app_id, review, "a-model", subject, polarity)
            })
            .collect();
        let second: Vec<_> = rows
            .iter()
            .zip(&drawn)
            .map(|((_, _, _, polarity, subject), review)| {
                label(app_id, review, "another-model", subject, polarity)
            })
            .collect();
        std::fs::write(
            game.join("labels.json"),
            serde_json::to_vec(&first).unwrap(),
        )
        .unwrap();
        // A set without the store's answer is not complete enough to export.
        steamgauge_core::facts::Facts {
            headset_only: false,
        }
        .save(&game)
        .unwrap();
        std::fs::create_dir_all(game.join("second")).unwrap();
        std::fs::write(
            game.join("second").join("labels.json"),
            serde_json::to_vec(&second).unwrap(),
        )
        .unwrap();
    }
    (library, reference)
}
