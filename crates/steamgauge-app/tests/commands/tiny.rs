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
