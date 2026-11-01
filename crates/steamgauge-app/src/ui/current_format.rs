//! Bringing what an earlier release of the app kept to the format this one reads: where the
//! library lives, the settings, and every reading in the library.
//!
//! It runs once, the first time anything asks for the library or the settings, and whatever
//! asks while it runs waits for it, so nothing in the app ever reads either in an earlier
//! format.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use tauri::{AppHandle, Manager};

use super::settings::{self, Settings};

static LIBRARY: OnceLock<PathBuf> = OnceLock::new();

/// Where corpora live, once the library and the settings are in the current format.
pub(super) fn library_dir(app: &AppHandle) -> PathBuf {
    LIBRARY.get_or_init(|| bring(app)).clone()
}

fn bring(app: &AppHandle) -> PathBuf {
    if let Some(file) = settings::file(app) {
        bring_settings(&file);
    }
    let library = place_library(app);
    // A reading that could not be brought keeps its old format, and every pass that reads it
    // then says it was written by another build and to read the game again, which is the
    // one thing that fixes it.
    let _ = steamgauge_core::current_format::bring_library(&library);
    library
}

/// The library belongs in the local app data folder unless `STEAMGAUGE_DATA` names another:
/// on Windows the roaming one travels with a roaming profile, which is no place for gigabytes
/// of reviews. A pipeline run from a terminal defaults to `data` beside it instead, which is
/// wrong for an icon: an application opened from a menu has no working directory worth
/// writing gigabytes into.
fn place_library(app: &AppHandle) -> PathBuf {
    if let Some(chosen) = std::env::var_os("STEAMGAUGE_DATA") {
        return PathBuf::from(chosen);
    }
    let paths = app.path();
    match (paths.app_data_dir(), paths.app_local_data_dir()) {
        (Ok(roaming), Ok(local)) => settle_library(&roaming.join("data"), &local.join("data")),
        (_, Ok(local)) => local.join("data"),
        _ => PathBuf::from("data"),
    }
}

/// Releases before 0.1.4 kept the library in the roaming folder. It moves once, by rename,
/// which on one drive is instant and all or nothing. When it cannot, the library stays where
/// it is and is used there, rather than the app opening on an empty library beside the full
/// one. On macOS and Linux the two folders are one, and nothing moves.
fn settle_library(roaming: &Path, local: &Path) -> PathBuf {
    if roaming == local || local.exists() || !roaming.is_dir() {
        return local.to_path_buf();
    }
    let moved = local
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::rename(roaming, local));
    if moved.is_ok() {
        local.to_path_buf()
    } else {
        roaming.to_path_buf()
    }
}

/// Settings kept by a release that offered fewer of them gain the ones added since, at their
/// defaults, with every choice already made kept. A file that is not a set of settings at all
/// is left for [`Settings::load`], which reads it as nothing chosen.
fn bring_settings(file: &Path) {
    let Ok(bytes) = std::fs::read(file) else {
        return;
    };
    let Ok(serde_json::Value::Object(kept)) = serde_json::from_slice(&bytes) else {
        return;
    };
    let Ok(serde_json::Value::Object(mut whole)) = serde_json::to_value(Settings::default()) else {
        return;
    };
    if whole.keys().all(|key| kept.contains_key(key)) {
        return;
    }
    for (key, value) in kept {
        if whole.contains_key(&key) {
            whole.insert(key, value);
        }
    }
    if let Ok(settings) = serde_json::from_value::<Settings>(serde_json::Value::Object(whole)) {
        let _ = settings.write_to(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("steamgauge-library-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_first_start_keeps_the_library_in_the_local_folder() {
        let scratch = Scratch::new("first");
        let (roaming, local) = (scratch.0.join("Roaming/data"), scratch.0.join("Local/data"));
        assert_eq!(settle_library(&roaming, &local), local);
        assert!(!roaming.exists());
    }

    #[test]
    fn a_library_in_the_roaming_folder_moves_whole_to_the_local_one() {
        let scratch = Scratch::new("moves");
        let (roaming, local) = (scratch.0.join("Roaming/data"), scratch.0.join("Local/data"));
        std::fs::create_dir_all(roaming.join("appid=1")).unwrap();
        std::fs::write(roaming.join("appid=1/reviews.parquet"), b"held").unwrap();
        assert_eq!(settle_library(&roaming, &local), local);
        assert_eq!(
            std::fs::read(local.join("appid=1/reviews.parquet")).unwrap(),
            b"held"
        );
        assert!(!roaming.exists());
    }

    #[test]
    fn a_library_already_in_the_local_folder_is_left_beside_an_old_one() {
        let scratch = Scratch::new("both");
        let (roaming, local) = (scratch.0.join("Roaming/data"), scratch.0.join("Local/data"));
        std::fs::create_dir_all(&roaming).unwrap();
        std::fs::create_dir_all(&local).unwrap();
        assert_eq!(settle_library(&roaming, &local), local);
        assert!(roaming.exists());
    }

    #[test]
    fn a_library_that_cannot_move_is_used_where_it_is() {
        let scratch = Scratch::new("stuck");
        let roaming = scratch.0.join("Roaming/data");
        std::fs::create_dir_all(&roaming).unwrap();
        // A file where the local folder's parent has to be, so the move cannot happen.
        std::fs::write(scratch.0.join("Local"), b"").unwrap();
        assert_eq!(
            settle_library(&roaming, &scratch.0.join("Local/data")),
            roaming
        );
        assert!(roaming.exists());
    }

    #[test]
    fn one_folder_for_both_moves_nothing() {
        let scratch = Scratch::new("same");
        let both = scratch.0.join("data");
        std::fs::create_dir_all(&both).unwrap();
        assert_eq!(settle_library(&both, &both), both);
        assert!(both.exists());
    }

    #[test]
    fn settings_from_a_release_with_fewer_of_them_keep_every_choice_and_gain_the_rest() {
        let scratch = Scratch::new("fewer-settings");
        let file = scratch.0.join("settings.json");
        std::fs::write(
            &file,
            r#"{"gpu_share": 0.5, "reader": "small", "language": null, "check_steam": false}"#,
        )
        .unwrap();
        bring_settings(&file);
        let brought: Settings = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(
            brought,
            Settings {
                gpu_share: 0.5,
                reader: Some("small".to_owned()),
                language: None,
                check_steam: false,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn settings_in_the_current_format_or_not_settings_at_all_are_left_as_they_are() {
        let scratch = Scratch::new("current-settings");
        let file = scratch.0.join("settings.json");
        let current = serde_json::to_vec(&Settings {
            gpu_share: 0.25,
            ..Settings::default()
        })
        .unwrap();
        std::fs::write(&file, &current).unwrap();
        bring_settings(&file);
        assert_eq!(std::fs::read(&file).unwrap(), current);

        std::fs::write(&file, b"not settings").unwrap();
        bring_settings(&file);
        assert_eq!(std::fs::read(&file).unwrap(), b"not settings");
    }
}
