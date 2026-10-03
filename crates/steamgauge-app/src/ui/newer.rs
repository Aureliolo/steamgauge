//! The notice that a newer `SteamGauge` is out. The core asks and remembers; this decides when
//! to ask, where the answer is kept, and tells the window.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use steamgauge_core::newer_version::{Asked, Release};
use tauri::{AppHandle, Emitter, Manager};

use super::{now_unix, settings::Settings};

/// Opening the app and saving the settings can both start a question; one is enough.
static ASKING: AtomicBool = AtomicBool::new(false);

fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok()
}

/// Asks in the background where the person wants it and a day has passed, and tells the window
/// when the answer names a newer release. Returns at once: opening the app never waits on it.
pub fn check_in_background(app: &AppHandle) {
    let Some(dir) = dir(app) else {
        return;
    };
    if !Settings::load(app).check_newer_version
        || !Asked::load(&dir).due(now_unix())
        || ASKING.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let asked = Asked::load(&dir).ask(now_unix()).await;
        // Kept before the window is told, so a window that asks after the event still hears it.
        let _ = asked.save(&dir);
        ASKING.store(false, Ordering::Release);
        if let Some(release) = asked.newer_than(env!("CARGO_PKG_VERSION")) {
            let _ = app.emit("newer-version", release);
        }
    });
}

/// The newer release last heard of, from what is kept on disk; nothing is asked here.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn newer_version(app: AppHandle) -> Option<Release> {
    if !Settings::load(&app).check_newer_version {
        return None;
    }
    Asked::load(&dir(&app)?).newer_than(env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    /// The opener's own rule: a URL may be opened when no denied pattern matches it and an
    /// allowed one does.
    fn may_open(url: &str) -> bool {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
        let opener = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|permission| permission["identifier"] == "opener:allow-open-url")
            .unwrap();
        let any = |list: &str| {
            opener[list].as_array().is_some_and(|entries| {
                entries.iter().any(|entry| {
                    glob::Pattern::new(entry["url"].as_str().unwrap())
                        .unwrap()
                        .matches(url)
                })
            })
        };
        !any("deny") && any("allow")
    }

    #[test]
    fn the_window_may_open_a_release_page_of_this_repository_and_nothing_else_on_github() {
        let release = steamgauge_core::newer_version::Asked {
            at: 0,
            latest: Some("0.2.0".to_owned()),
        }
        .newer_than("0.1.0")
        .unwrap();
        assert!(may_open(&release.url));
        for refused in [
            "https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0/../../../../someone/else",
            "https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0/%2e%2e/%2E%2E/settings",
            "https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0?redirect=elsewhere",
            "https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0#elsewhere",
            "https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0\\..\\..",
            "https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0/elsewhere",
            "https://github.com/Aureliolo/steamgauge/releases/tag/nightly",
            "https://github.com/Aureliolo/steamgauge/releases",
            "https://github.com/someone/else/releases/tag/v0.2.0",
            "https://github.com.example.com/Aureliolo/steamgauge/releases/tag/v0.2.0",
            "http://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0",
        ] {
            assert!(!may_open(refused), "{refused} may be opened");
        }
    }

    #[test]
    fn steam_pages_may_still_be_opened() {
        assert!(may_open(
            "https://steamcommunity.com/profiles/76561197960287930/recommended/1/"
        ));
        assert!(may_open("https://store.steampowered.com/app/1/"));
    }
}
