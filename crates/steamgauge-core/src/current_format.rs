//! Bringing a library written by an earlier release to the format this build reads.
//!
//! Every pass that reads a library reads the current format and nothing else, so what an
//! earlier release wrote differently is rewritten here, once, before anything reads it. Each
//! step recognises its file by its shape, not by a version stamp, and leaves a file already in
//! the current format as it is, so bringing a library twice changes nothing the second time.
//! A file a step rewrites keeps the time it was written, since a game's vectors
//! and its themes and its themes are recorded by that time.
//!
//! What an earlier release wrote that the current format cannot express is removed rather than
//! guessed at: a game whose reading goes is a game downloaded and not yet read, which the
//! window already offers to read.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{Error, Result};

/// What bringing a library changed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Brought {
    /// Readings given the fingerprint of the text each answer was read from.
    pub keyed: Vec<PathBuf>,
    /// Readings given the day their game was read, which earlier releases told by the time of
    /// the file.
    pub dated: Vec<PathBuf>,
    /// Readings whose held-out figure was under the name releases before 0.2.0 gave it.
    pub renamed: Vec<PathBuf>,
    /// Readings removed, each with why. Their game is left to be read again.
    pub removed: Vec<(PathBuf, String)>,
    /// Snapshots where a reading or a record could not be read or written, each with the error,
    /// left as it was. A pass that reads a reading says it was written by another build.
    pub failed: Vec<(PathBuf, String)>,
    /// Snapshots whose vectors, themes or paragraph named their model by less than its files.
    pub named: Vec<PathBuf>,
}

/// What earlier releases recorded a game's vectors and themes by, and the one encoder published
/// under it. Literals, since the pins of the current encoder move with its next release.
const ENCODER_BY_NAME: (&str, &str) = (
    "search-encoder",
    "c87c38db060bafb0122019c0c749ec1eb1ae510dae43c93f0042ec51099942e8+f69a8a7017cfc32ae862bed62b8858a540d627ce691047f610a13821285aa859",
);

/// What those releases recorded a paragraph by, the graph alone, and the one writer published
/// with it.
const WRITER_BY_GRAPH: (&str, &str) = (
    "04ca14036926efcb3f97a6f9fdd981000e87a556f8d7a902b305c907cbd27080",
    "be75606093db2094d7cd20f3c2f385c212750648bd6ea4fb2bf507a6a4c55506+04ca14036926efcb3f97a6f9fdd981000e87a556f8d7a902b305c907cbd27080+c39f184b43404066dde244d07360908b2dbb1f5a061dda43e9bf100f09d67a4f",
);

/// Brings every reading in the library at `library`, and the records of what was prepared from
/// it, to the current format.
///
/// # Errors
///
/// Fails if the library cannot be listed. A reading that cannot be brought does not stop the
/// rest: one the current format cannot express is removed and listed in [`Brought::removed`],
/// and one that cannot be read or written is listed in [`Brought::failed`].
/// A game whose files are being removed is listed in [`Brought::failed`].
pub fn bring_library(library: &Path) -> Result<Brought> {
    let mut brought = Brought::default();
    let Ok(games) = std::fs::read_dir(library) else {
        return Ok(brought);
    };
    for game in games {
        let game = game?.path();
        if !game
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("appid="))
        {
            continue;
        }
        let _writing = crate::lock_file::writing_in(&game);
        #[expect(
            clippy::used_underscore_binding,
            reason = "the binding holds the lock until the game is brought, and is read once"
        )]
        if let Err(refused) = &_writing {
            brought.failed.push((game, refused.to_string()));
            continue;
        }
        for snapshot in std::fs::read_dir(&game)? {
            let snapshot = snapshot?.path();
            // Before the reading, which a record by name outlives however the reading goes.
            // Before the reading, which a record by name outlives however the reading goes.
            match name_files(&snapshot) {
                Ok(true) => brought.named.push(snapshot.clone()),
                Ok(false) => {}
                Err(failed) => brought.failed.push((snapshot.clone(), failed.to_string())),
            }
            if snapshot.join("reading.json").is_file()
                && let Err(failed) = bring_reading(&snapshot, &mut brought)
            {
                brought.failed.push((snapshot, failed.to_string()));
            }
        }
    }
    Ok(brought)
}

/// Names the records beside one snapshot by their model's files, where an earlier release named
/// them by less. A record that is absent, cannot be read or is no JSON object is left, since no
/// release wrote one.
///
/// # Errors
///
/// Fails if a record that names its model by less cannot be written.
fn name_files(snapshot: &Path) -> Result<bool> {
    let mut changed = false;
    for (file, field, (by, files)) in [
        (
            snapshot.join("meaning").join("meaning.json"),
            "encoder",
            ENCODER_BY_NAME,
        ),
        (snapshot.join("themes.json"), "encoder", ENCODER_BY_NAME),
        (snapshot.join("paragraph.json"), "writer", WRITER_BY_GRAPH),
    ] {
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let Ok(mut record) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        let Some(fields) = record.as_object_mut() else {
            continue;
        };
        let Some(named) = fields
            .get_mut(field)
            .filter(|named| named.as_str() == Some(by))
        else {
            continue;
        };
        *named = Value::from(files);
        let partial = crate::whole_file::beside(&file);
        std::fs::write(&partial, serde_json::to_vec(&record)?)?;
        replace_keeping_time(&partial, &file)?;
        changed = true;
    }
    Ok(changed)
}

/// Brings the reading beside one snapshot.
fn bring_reading(snapshot: &Path, brought: &mut Brought) -> Result<()> {
    let path = snapshot.join("reading.json");
    let mut reading: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    let Some(fields) = reading.as_object_mut() else {
        remove_reading(snapshot, "it is not a reading", brought)?;
        return Ok(());
    };
    // Releases before 0.1.6 counted no kind of reviewer apart, and counting them takes the
    // reader that answered, which no release carries any more.
    if !fields.contains_key("who") {
        remove_reading(
            snapshot,
            "it was counted before reviewers were told apart, by a reader no longer published",
            brought,
        )?;
        return Ok(());
    }
    // Earlier releases told the day a game was read by when this file was written, which a
    // recount moved.
    let dated = if fields.contains_key("read_unix") {
        false
    } else {
        let written = std::fs::metadata(&path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
        fields.insert("read_unix".to_owned(), Value::from(written));
        true
    };
    // Releases before 0.2.0 kept the reader's held-out figure as `frozen`, in the same shape.
    let renamed = if let Some(figure) = fields.remove("frozen") {
        fields.entry("held_out").or_insert(figure);
        true
    } else {
        false
    };
    if dated || renamed {
        let partial = crate::whole_file::beside(&path);
        std::fs::write(&partial, serde_json::to_vec_pretty(&reading)?)?;
        replace_keeping_time(&partial, &path)?;
        if dated {
            brought.dated.push(snapshot.to_path_buf());
        }
        if renamed {
            brought.renamed.push(snapshot.to_path_buf());
        }
    }
    match crate::read::key_readings(snapshot) {
        Ok(true) => brought.keyed.push(snapshot.to_path_buf()),
        Ok(false) => {}
        Err(Error::Refused(why)) => remove_reading(snapshot, &why, brought)?,
        Err(Error::StaleClassifications { field, .. }) => remove_reading(
            snapshot,
            &format!("its readings have no usable `{field}`"),
            brought,
        )?,
        Err(failed) => return Err(failed),
    }
    Ok(())
}

/// Puts `partial` in place of `file` with the time `file` was written, which what was made from
/// the file is recorded by. Where it cannot, `partial` is removed and `file` is left as it was.
///
/// # Errors
///
/// Fails if the time of `file` cannot be read, the time cannot be set on `partial`, or the
/// rename fails.
pub(crate) fn replace_keeping_time(partial: &Path, file: &Path) -> std::io::Result<()> {
    // The time goes on before the rename, so a stop between the two leaves the old file whole
    // and never a new one that carries the time of the bring.
    let put = || -> std::io::Result<()> {
        let written = std::fs::metadata(file)?.modified()?;
        std::fs::File::options()
            .write(true)
            .open(partial)?
            .set_modified(written)?;
        std::fs::rename(partial, file)
    };
    let put = put();
    if put.is_err() {
        let _ = std::fs::remove_file(partial);
    }
    put
}

fn remove_reading(snapshot: &Path, why: &str, brought: &mut Brought) -> Result<()> {
    // The readings first: the reading is what makes a game read, so a failure part way
    // leaves it whole rather than a game counted as unread beside answers nobody can open.
    for file in ["readings.parquet", "reading.json"] {
        match std::fs::remove_file(snapshot.join(file)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error.into());
            }
            _ => {}
        }
    }
    brought
        .removed
        .push((snapshot.to_path_buf(), why.to_owned()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;
    use crate::read::tests::{read_these, unkey_readings};

    fn review(id: &str, text: &str) -> Value {
        serde_json::json!({
            "recommendationid": id, "review": text, "language": "english", "voted_up": true,
            "weighted_vote_score": "0.5", "votes_up": 3, "timestamp_created": 1_700_000_000,
            "author": {"steamid": format!("7656{id}"), "playtime_at_review": 3_000},
        })
    }

    fn edit_reading(snapshot: &Path, change: impl FnOnce(&mut serde_json::Map<String, Value>)) {
        let path = snapshot.join("reading.json");
        let mut reading: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        change(reading.as_object_mut().unwrap());
        std::fs::write(&path, reading.to_string()).unwrap();
    }

    #[test]
    fn a_library_in_the_current_format_is_left_as_it_is() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        let before = std::fs::read(snapshot.join("readings.parquet")).unwrap();
        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought, Brought::default());
        assert_eq!(
            std::fs::read(snapshot.join("readings.parquet")).unwrap(),
            before
        );
        crate::read::ReadReport::load(&snapshot).unwrap();
    }

    #[test]
    fn readings_from_before_0_3_0_are_keyed_and_then_left_alone() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        unkey_readings(&snapshot);
        assert!(
            crate::read::for_each_reading(&snapshot.join("readings.parquet"), |_, _, _, _, _| {})
                .is_err(),
            "the current format does not read them"
        );

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.keyed, std::slice::from_ref(&snapshot));
        assert_eq!(brought.removed, []);
        crate::read::for_each_reading(&snapshot.join("readings.parquet"), |_, _, _, _, _| {})
            .unwrap();
        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
    }

    #[test]
    fn a_game_being_removed_is_left_unbrought() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        unkey_readings(&snapshot);
        let before = std::fs::read(snapshot.join("readings.parquet")).unwrap();
        let removal = crate::crawl::lock_for_removal(out.path(), 1).unwrap();

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.failed.len(), 1, "{brought:?}");
        assert_eq!(brought.failed[0].0, snapshot.parent().unwrap());
        assert!(brought.failed[0].1.contains("being removed"), "{brought:?}");
        assert_eq!((brought.keyed.len(), brought.removed.len()), (0, 0));
        assert_eq!(
            std::fs::read(snapshot.join("readings.parquet")).unwrap(),
            before
        );
        assert!(snapshot.join("reading.json").is_file());

        drop(removal);
        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.keyed, std::slice::from_ref(&snapshot));
    }

    #[test]
    fn a_held_out_figure_under_its_old_name_is_renamed() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        let figure = serde_json::json!(
            {"games": 10, "claims": 7, "coverage": 0.9, "accuracy": 0.8, "macro_f1": 0.7}
        );
        edit_reading(&snapshot, |fields| {
            fields.remove("held_out");
            fields.insert("frozen".to_owned(), figure.clone());
        });

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.renamed, std::slice::from_ref(&snapshot));
        let reading = crate::read::ReadReport::load(&snapshot).unwrap();
        assert_eq!(reading.held_out.map(|held_out| held_out.claims), Some(7));
        let stored: Value =
            serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).unwrap()).unwrap();
        assert!(stored.get("frozen").is_none());
    }

    #[test]
    fn a_reading_from_before_reviewers_were_told_apart_is_removed() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        edit_reading(&snapshot, |fields| {
            fields.remove("who");
        });

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.removed.len(), 1);
        assert!(brought.removed[0].1.contains("reviewers"), "{brought:?}");
        assert!(!snapshot.join("reading.json").exists());
        assert!(!snapshot.join("readings.parquet").exists());
        assert!(
            snapshot.join("shard-0000.parquet").exists(),
            "the reviews themselves stay"
        );
    }

    #[test]
    fn a_reading_whose_readings_are_already_gone_is_removed_all_the_same() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        edit_reading(&snapshot, |fields| {
            fields.remove("who");
        });
        std::fs::remove_file(snapshot.join("readings.parquet")).unwrap();

        let brought = bring_library(out.path()).unwrap();
        assert_eq!((brought.removed.len(), brought.failed.len()), (1, 0));
        assert!(!snapshot.join("reading.json").exists());
    }

    #[test]
    fn a_reading_that_cannot_be_removed_is_said_to_have_failed() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        edit_reading(&snapshot, |fields| {
            fields.remove("who");
        });
        // A folder where the readings file is: removing it as a file fails, and not because
        // it is missing.
        std::fs::remove_file(snapshot.join("readings.parquet")).unwrap();
        std::fs::create_dir(snapshot.join("readings.parquet")).unwrap();

        let brought = bring_library(out.path()).unwrap();
        assert_eq!((brought.removed.len(), brought.failed.len()), (0, 1));
        assert!(snapshot.join("readings.parquet").is_dir());
        assert!(
            snapshot.join("reading.json").exists(),
            "the reading is left whole"
        );
    }

    #[test]
    fn a_reading_that_names_reviews_its_capture_never_held_is_removed() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        unkey_readings(&snapshot);
        crate::read::tests::capture_these(out.path(), 1, &[review("9", "Bugs everywhere.")]);

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.removed.len(), 1);
        assert!(brought.removed[0].1.contains("review 1"), "{brought:?}");
        assert!(!snapshot.join("reading.json").exists());
    }

    fn written_at(path: &Path, seconds: u64) {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(seconds))
            .unwrap();
    }

    fn prepared(snapshot: &Path) -> u64 {
        crate::meaning::prepare(
            snapshot,
            |texts| {
                Ok(texts
                    .iter()
                    .map(|_| {
                        let mut vector = vec![0.0_f32; crate::search_models::DIMENSIONS];
                        vector[0] = 1.0;
                        vector
                    })
                    .collect())
            },
            (&AtomicBool::new(false), 1.0),
            |_| {},
        )
        .unwrap()
        .embedded
    }

    #[test]
    fn readings_keyed_keep_the_time_their_vectors_and_themes_name() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(
            out.path(),
            1,
            &[
                review("1", "Bugs everywhere. The music is lovely."),
                review("2", "The story is good."),
            ],
        );
        let readings = snapshot.join("readings.parquet");
        written_at(&readings, 1_700_000_000);
        let embedded = prepared(&snapshot);
        crate::themes::find(&snapshot, crate::themes::Closeness::USED)
            .unwrap()
            .save(&snapshot)
            .unwrap();
        unkey_readings(&snapshot);
        written_at(&readings, 1_700_000_000);
        assert_eq!(
            crate::meaning::status(&snapshot),
            crate::meaning::Status::Ready
        );

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.keyed, std::slice::from_ref(&snapshot));
        assert_eq!(
            crate::meaning::readings_written(&snapshot),
            Some(1_700_000_000)
        );
        assert_eq!(
            crate::meaning::status(&snapshot),
            crate::meaning::Status::Ready
        );
        assert_eq!(crate::meaning::held(&snapshot).0, embedded);
        assert!(crate::themes::Themes::load(&snapshot).is_some());

        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
        assert_eq!(
            crate::meaning::readings_written(&snapshot),
            Some(1_700_000_000)
        );
        assert_eq!(
            crate::meaning::status(&snapshot),
            crate::meaning::Status::Ready
        );
    }

    #[test]
    fn a_figure_renamed_keeps_the_time_its_reading_was_written() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        let figure = serde_json::json!(
            {"games": 10, "claims": 7, "coverage": 0.9, "accuracy": 0.8, "macro_f1": 0.7}
        );
        edit_reading(&snapshot, |fields| {
            fields.remove("held_out");
            fields.insert("frozen".to_owned(), figure.clone());
        });
        let reading = snapshot.join("reading.json");
        written_at(&reading, 1_700_000_000);

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.renamed, std::slice::from_ref(&snapshot));
        assert_eq!(brought.failed, []);
        assert_eq!(
            std::fs::metadata(&reading).unwrap().modified().unwrap(),
            UNIX_EPOCH + Duration::from_secs(1_700_000_000)
        );
        assert!(!crate::whole_file::beside(&reading).exists());
        let stored: Value = serde_json::from_slice(&std::fs::read(&reading).unwrap()).unwrap();
        assert!(stored.get("held_out").is_some() && stored.get("frozen").is_none());
    }

    #[test]
    fn a_file_that_cannot_be_replaced_is_left_with_no_partial_beside_it() {
        let out = crate::tempdir::Dir::new();
        let file = out.path().join("a.json");
        std::fs::create_dir(&file).unwrap();
        let partial = out.path().join("a.json.partial");
        std::fs::write(&partial, b"{}").unwrap();

        assert!(replace_keeping_time(&partial, &file).is_err());
        assert!(file.is_dir());
        assert!(!partial.exists());
    }

    #[test]
    fn a_file_replaced_keeps_its_whole_time_and_takes_the_new_content() {
        let out = crate::tempdir::Dir::new();
        let file = out.path().join("a.json");
        std::fs::write(&file, b"{}").unwrap();
        let at = UNIX_EPOCH + Duration::new(1_700_000_000, 123_456_700);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(at)
            .unwrap();
        let partial = out.path().join("a.json.partial");
        std::fs::write(&partial, b"{\"a\": 1}").unwrap();

        replace_keeping_time(&partial, &file).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), at);
        assert_eq!(std::fs::read(&file).unwrap(), b"{\"a\": 1}");
        assert!(!partial.exists());
    }

    #[test]
    fn a_file_that_is_not_there_gives_an_error_and_leaves_no_partial() {
        let out = crate::tempdir::Dir::new();
        let partial = out.path().join("a.json.partial");
        std::fs::write(&partial, b"{}").unwrap();

        let failed = replace_keeping_time(&partial, &out.path().join("a.json")).unwrap_err();
        assert_eq!(failed.kind(), std::io::ErrorKind::NotFound);
        assert!(!partial.exists());
    }

    #[test]
    fn a_library_with_a_figure_to_rename_and_readings_to_key_keeps_both_times() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        unkey_readings(&snapshot);
        edit_reading(&snapshot, |fields| {
            let figure = fields.remove("held_out").unwrap_or(Value::Null);
            fields.insert("frozen".to_owned(), figure);
        });
        written_at(&snapshot.join("reading.json"), 1_700_000_000);
        written_at(&snapshot.join("readings.parquet"), 1_700_000_500);

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.failed, []);
        assert_eq!(brought.renamed, std::slice::from_ref(&snapshot));
        assert_eq!(brought.keyed, std::slice::from_ref(&snapshot));
        assert_eq!(
            crate::meaning::readings_written(&snapshot),
            Some(1_700_000_500)
        );
        let reading = std::fs::metadata(snapshot.join("reading.json")).unwrap();
        assert_eq!(
            reading.modified().unwrap(),
            UNIX_EPOCH + Duration::from_secs(1_700_000_000)
        );
    }

    #[test]
    fn a_reading_with_no_day_is_given_the_time_its_file_was_written() {
        let out = crate::tempdir::Dir::new();
        let first = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        let second = read_these(out.path(), 2, &[review("1", "Bugs everywhere.")]);
        let third = read_these(out.path(), 3, &[review("1", "Bugs everywhere.")]);
        for snapshot in [&first, &second] {
            edit_reading(snapshot, |fields| {
                fields.remove("read_unix");
            });
        }
        edit_reading(&second, |fields| {
            let figure = fields.remove("held_out").unwrap_or(Value::Null);
            fields.insert("frozen".to_owned(), figure);
        });
        edit_reading(&third, |fields| {
            fields.insert("read_unix".to_owned(), Value::from(0));
        });
        for snapshot in [&first, &second] {
            written_at(&snapshot.join("reading.json"), 1_700_000_000);
        }
        let untouched = std::fs::read(third.join("reading.json")).unwrap();

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.failed, []);
        let mut dated = brought.dated.clone();
        dated.sort();
        assert_eq!(dated, [first.clone(), second.clone()]);
        assert_eq!(brought.renamed, std::slice::from_ref(&second));
        for snapshot in [&first, &second] {
            assert_eq!(
                crate::read::ReadReport::load(snapshot).unwrap().read_unix,
                1_700_000_000
            );
            assert!(!crate::whole_file::beside(&snapshot.join("reading.json")).exists());
        }

        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
        assert_eq!(
            std::fs::read(third.join("reading.json")).unwrap(),
            untouched
        );
    }

    #[test]
    fn a_reading_with_no_who_is_removed_and_never_dated() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        edit_reading(&snapshot, |fields| {
            fields.remove("who");
            fields.remove("read_unix");
        });

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.removed.len(), 1);
        assert_eq!(brought.dated, Vec::<PathBuf>::new());
        assert!(!snapshot.join("reading.json").exists());
    }

    #[test]
    fn a_reading_is_dated_only_once_its_file_is_in_place() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        edit_reading(&snapshot, |fields| {
            fields.remove("read_unix");
        });
        std::fs::create_dir(crate::whole_file::beside(&snapshot.join("reading.json"))).unwrap();
        let before = std::fs::read(snapshot.join("reading.json")).unwrap();

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.dated, Vec::<PathBuf>::new());
        assert_eq!(brought.failed.len(), 1);
        assert_eq!(
            std::fs::read(snapshot.join("reading.json")).unwrap(),
            before
        );
    }

    #[test]
    fn a_library_that_is_not_there_has_nothing_to_bring() {
        let out = crate::tempdir::Dir::new();
        let brought = bring_library(&out.path().join("never-made")).unwrap();
        assert_eq!(brought, Brought::default());
    }

    #[test]
    fn the_models_named_by_less_are_held_to_their_files_in_order() {
        assert_eq!(
            ENCODER_BY_NAME.1,
            "c87c38db060bafb0122019c0c749ec1eb1ae510dae43c93f0042ec51099942e8+f69a8a7017cfc32ae862bed62b8858a540d627ce691047f610a13821285aa859"
        );
        let writer_graph = "04ca14036926efcb3f97a6f9fdd981000e87a556f8d7a902b305c907cbd27080";
        assert_eq!(WRITER_BY_GRAPH.0, writer_graph);
        assert_eq!(
            WRITER_BY_GRAPH.1,
            format!(
                "be75606093db2094d7cd20f3c2f385c212750648bd6ea4fb2bf507a6a4c55506+{writer_graph}+c39f184b43404066dde244d07360908b2dbb1f5a061dda43e9bf100f09d67a4f"
            )
        );
    }

    fn edit_record(path: &Path, field: &str, value: &str) {
        let mut record: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        record[field] = Value::from(value);
        std::fs::write(path, record.to_string()).unwrap();
    }

    fn field_of(path: &Path, field: &str) -> String {
        let record: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        record[field].as_str().unwrap().to_owned()
    }

    fn name_meaning(snapshot: &Path) {
        edit_record(
            &snapshot.join("meaning").join("meaning.json"),
            "encoder",
            ENCODER_BY_NAME.0,
        );
    }

    /// The snapshot's vectors as releases before the encoder was recorded by its files left them,
    /// and how many claims were embedded.
    fn by_name(snapshot: &Path) -> u64 {
        let embedded = prepared(snapshot);
        name_meaning(snapshot);
        embedded
    }

    fn encoder_is_current() -> bool {
        ENCODER_BY_NAME.1 == crate::search_models::ENCODER.pins()
    }

    fn writer_is_current() -> bool {
        WRITER_BY_GRAPH.1 == crate::writer::WRITER.pins()
    }

    #[test]
    fn records_by_name_are_brought_to_the_files_that_made_them() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(
            out.path(),
            1,
            &[
                review("1", "Bugs everywhere. The music is lovely."),
                review("2", "The story is good."),
            ],
        );
        let readings = snapshot.join("readings.parquet");
        written_at(&readings, 1_700_000_000);
        let embedded = prepared(&snapshot);
        crate::themes::find(&snapshot, crate::themes::Closeness::USED)
            .unwrap()
            .save(&snapshot)
            .unwrap();
        unkey_readings(&snapshot);
        written_at(&readings, 1_700_000_000);
        name_meaning(&snapshot);
        let themes = snapshot.join("themes.json");
        edit_record(&themes, "encoder", ENCODER_BY_NAME.0);
        written_at(&themes, 1_700_000_000);
        crate::writer::Paragraph::new("The music is lovely.".to_owned(), &snapshot)
            .save(&snapshot)
            .unwrap();
        let paragraph = snapshot.join("paragraph.json");
        edit_record(&paragraph, "writer", WRITER_BY_GRAPH.0);

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.named, std::slice::from_ref(&snapshot));
        assert_eq!(
            field_of(&snapshot.join("meaning").join("meaning.json"), "encoder"),
            ENCODER_BY_NAME.1
        );
        assert_eq!(field_of(&themes, "encoder"), ENCODER_BY_NAME.1);
        assert_eq!(field_of(&paragraph, "writer"), WRITER_BY_GRAPH.1);
        assert_eq!(
            std::fs::metadata(&themes)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            1_700_000_000
        );
        assert_eq!(
            crate::meaning::status(&snapshot),
            if encoder_is_current() {
                crate::meaning::Status::Ready
            } else {
                crate::meaning::Status::None
            }
        );
        assert_eq!(
            crate::meaning::held(&snapshot).0,
            if encoder_is_current() { embedded } else { 0 }
        );
        assert_eq!(
            crate::themes::Themes::load(&snapshot).is_some(),
            encoder_is_current()
        );
        assert_eq!(
            crate::writer::Paragraph::load(&snapshot).is_some(),
            writer_is_current()
        );

        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
        let other = snapshot.join("meaning").join("meaning.json");
        edit_record(&other, "encoder", "another-encoder");
        let before = std::fs::read(&other).unwrap();
        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
        assert_eq!(std::fs::read(&other).unwrap(), before);
    }

    #[test]
    fn a_record_by_name_that_cannot_be_written_whole_is_left_as_it_was() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        by_name(&snapshot);
        let record = snapshot.join("meaning").join("meaning.json");
        let before = std::fs::read(&record).unwrap();
        std::fs::create_dir(crate::whole_file::beside(&record)).unwrap();

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.named, Vec::<PathBuf>::new());
        assert_eq!(brought.failed.len(), 1, "{brought:?}");
        assert_eq!(brought.failed[0].0, snapshot);
        assert_eq!(std::fs::read(&record).unwrap(), before);
    }

    #[test]
    fn records_by_name_are_brought_whatever_became_of_their_reading() {
        let out = crate::tempdir::Dir::new();
        let reviews = [review("1", "Bugs everywhere.")];
        let games: Vec<(PathBuf, u64)> = (1..=5)
            .map(|app_id| {
                let snapshot = read_these(out.path(), app_id, &reviews);
                let embedded = by_name(&snapshot);
                (snapshot, embedded)
            })
            .collect();
        let snapshot = |at: usize| games[at].0.clone();
        let figure = serde_json::json!(
            {"games": 10, "claims": 7, "coverage": 0.9, "accuracy": 0.8, "macro_f1": 0.7}
        );
        // As releases 0.1.0 to 0.1.5 wrote it.
        edit_reading(&snapshot(0), |fields| {
            fields.remove("who");
        });
        // As 0.4.0 left that one.
        std::fs::remove_file(snapshot(1).join("reading.json")).unwrap();
        std::fs::remove_file(snapshot(1).join("readings.parquet")).unwrap();
        // As releases 0.1.6 to 0.1.10 wrote it.
        unkey_readings(&snapshot(2));
        edit_reading(&snapshot(2), |fields| {
            fields.remove("held_out");
            fields.insert("frozen".to_owned(), figure.clone());
        });
        // As releases 0.2.0 to 0.2.17 wrote it.
        unkey_readings(&snapshot(3));
        // A reading that cannot be removed.
        edit_reading(&snapshot(4), |fields| {
            fields.remove("who");
        });
        std::fs::remove_file(snapshot(4).join("readings.parquet")).unwrap();
        std::fs::create_dir(snapshot(4).join("readings.parquet")).unwrap();

        let brought = bring_library(out.path()).unwrap();
        for (at, (game, embedded)) in games.iter().enumerate() {
            assert!(brought.named.contains(game), "game {at}: {brought:?}");
            assert_eq!(
                crate::meaning::held(game).0,
                if encoder_is_current() { *embedded } else { 0 },
                "game {at}"
            );
        }
        assert_eq!(brought.named.len(), 5);
        assert_eq!(
            brought
                .removed
                .iter()
                .map(|(game, _)| game)
                .collect::<Vec<_>>(),
            [&snapshot(0)]
        );
        assert!(brought.keyed.contains(&snapshot(2)), "{brought:?}");
        assert!(brought.keyed.contains(&snapshot(3)), "{brought:?}");
        assert_eq!(brought.renamed, [snapshot(2)]);
        assert_eq!(
            brought
                .failed
                .iter()
                .map(|(game, _)| game)
                .collect::<Vec<_>>(),
            [&snapshot(4)]
        );

        let again = read_these(out.path(), 1, &reviews);
        assert_eq!(again, snapshot(0));
        assert_eq!(
            prepared(&snapshot(0)),
            if encoder_is_current() { 0 } else { games[0].1 }
        );
    }

    #[test]
    fn what_is_no_record_is_left_as_it_is() {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        let game = snapshot.parent().unwrap().join("game.json");
        std::fs::write(&game, "{}").unwrap();
        std::fs::create_dir(snapshot.join("meaning")).unwrap();
        let record = snapshot.join("meaning").join("meaning.json");
        std::fs::write(&record, "not a record").unwrap();

        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
        assert_eq!(std::fs::read(&record).unwrap(), b"not a record");
        assert_eq!(std::fs::read(&game).unwrap(), b"{}");
    }

    /// A record of this kind holding `content` is left as it is and nothing is said of it.
    fn record_is_left(file: &str, content: Option<&str>) {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        let path = snapshot.join(file);
        if let Some(content) = content {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }

        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
        match content {
            Some(content) => assert_eq!(std::fs::read(&path).unwrap(), content.as_bytes()),
            None => assert!(!path.exists()),
        }
    }

    /// A record by name that cannot be written is said to have failed and left whole.
    fn record_cannot_be_written(file: &str, field: &str, by: &str) {
        let out = crate::tempdir::Dir::new();
        let snapshot = read_these(out.path(), 1, &[review("1", "Bugs everywhere.")]);
        let path = snapshot.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let content = serde_json::json!({ field: by }).to_string();
        std::fs::write(&path, &content).unwrap();
        std::fs::create_dir(crate::whole_file::beside(&path)).unwrap();

        let brought = bring_library(out.path()).unwrap();
        assert_eq!(brought.named, Vec::<PathBuf>::new());
        assert_eq!(brought.failed.len(), 1, "{brought:?}");
        assert_eq!(brought.failed[0].0, snapshot);
        assert_eq!(std::fs::read(&path).unwrap(), content.as_bytes());
    }

    #[test]
    fn a_missing_meaning_record_is_left() {
        record_is_left("meaning/meaning.json", None);
    }

    #[test]
    fn an_unparsable_meaning_record_is_left() {
        record_is_left("meaning/meaning.json", Some("not a record"));
    }

    #[test]
    fn a_meaning_record_that_is_no_object_is_left() {
        record_is_left("meaning/meaning.json", Some("\"search-encoder\""));
    }

    #[test]
    fn a_meaning_record_that_cannot_be_written_is_said_to_have_failed() {
        record_cannot_be_written("meaning/meaning.json", "encoder", ENCODER_BY_NAME.0);
    }

    #[test]
    fn a_missing_themes_record_is_left() {
        record_is_left("themes.json", None);
    }

    #[test]
    fn an_unparsable_themes_record_is_left() {
        record_is_left("themes.json", Some("not a record"));
    }

    #[test]
    fn a_themes_record_that_is_no_object_is_left() {
        record_is_left("themes.json", Some("\"search-encoder\""));
    }

    #[test]
    fn a_themes_record_that_cannot_be_written_is_said_to_have_failed() {
        record_cannot_be_written("themes.json", "encoder", ENCODER_BY_NAME.0);
    }

    #[test]
    fn a_missing_paragraph_record_is_left() {
        record_is_left("paragraph.json", None);
    }

    #[test]
    fn an_unparsable_paragraph_record_is_left() {
        record_is_left("paragraph.json", Some("not a record"));
    }

    #[test]
    fn a_paragraph_record_that_is_no_object_is_left() {
        record_is_left("paragraph.json", Some("\"search-encoder\""));
    }

    #[test]
    fn a_paragraph_record_that_cannot_be_written_is_said_to_have_failed() {
        record_cannot_be_written("paragraph.json", "writer", WRITER_BY_GRAPH.0);
    }
}
