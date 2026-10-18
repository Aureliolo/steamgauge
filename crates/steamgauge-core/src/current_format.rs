//! Bringing a library written by an earlier release to the format this build reads.
//!
//! Every pass that reads a library reads the current format and nothing else, so what an
//! earlier release wrote differently is rewritten here, once, before anything reads it. Each
//! step recognises its file by its shape, not by a version stamp, and leaves a file already in
//! the current format as it is, so bringing a library twice changes nothing the second time.
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
    /// Readings whose held-out figure was under the name releases before 0.2.0 gave it.
    pub renamed: Vec<PathBuf>,
    /// Readings removed, each with why, leaving their game to be read again.
    pub removed: Vec<(PathBuf, String)>,
    /// Readings that could not be read or written, each with the error, left as they were. A
    /// pass that reads one says it was written by another build.
    pub failed: Vec<(PathBuf, String)>,
}

/// Brings every reading in the library at `library` to the current format.
///
/// # Errors
///
/// Fails if the library cannot be listed. A reading that cannot be brought does not stop the
/// rest: one the current format cannot express is removed and listed in [`Brought::removed`],
/// and one that cannot be read or written is listed in [`Brought::failed`].
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
        for snapshot in std::fs::read_dir(&game)? {
            let snapshot = snapshot?.path();
            if snapshot.join("reading.json").is_file()
                && let Err(failed) = bring_reading(&snapshot, &mut brought)
            {
                brought.failed.push((snapshot, failed.to_string()));
            }
        }
    }
    Ok(brought)
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
    // Releases before 0.2.0 kept the reader's held-out figure as `frozen`, in the same shape.
    if let Some(figure) = fields.remove("frozen") {
        fields.entry("held_out").or_insert(figure);
        std::fs::write(&path, serde_json::to_vec_pretty(&reading)?)?;
        brought.renamed.push(snapshot.to_path_buf());
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
        assert!(crate::read::ReadReport::load(&snapshot).is_ok());
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
        assert!(
            crate::read::for_each_reading(&snapshot.join("readings.parquet"), |_, _, _, _, _| {})
                .is_ok()
        );
        assert_eq!(bring_library(out.path()).unwrap(), Brought::default());
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

    #[test]
    fn a_library_that_is_not_there_has_nothing_to_bring() {
        let out = crate::tempdir::Dir::new();
        let brought = bring_library(&out.path().join("never-made")).unwrap();
        assert_eq!(brought, Brought::default());
    }
}
