//! A game's own updates on its timeline, and each subject either side of the one chosen. The
//! core decides what an update is and what changed across it; this hands the window what is
//! kept on disk, and nothing here reaches the network.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

use serde::Serialize;
use steamgauge_core::{
    before_after::{self, Around, Dated},
    embed, updates,
    who::SEGMENTS,
};
use tauri::AppHandle;

use super::{library_dir, read_report, text};

/// One update, where the window draws it.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateOut {
    gid: String,
    title: String,
    posted: i64,
    link: String,
    /// The month it was posted in, as `2024-02`, and how far through it, so it can be placed on
    /// whichever months the timeline draws: everyone's, or one kind of reviewer's.
    month: String,
    through: f64,
}

/// A game's updates, oldest first.
#[derive(Debug, Clone, Serialize)]
pub struct GameUpdates {
    /// When Steam was last asked; none where it never was.
    asked: Option<i64>,
    updates: Vec<UpdateOut>,
    window_days: i64,
    enough: u64,
}

/// The updates kept for a game.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn game_updates(app: AppHandle, app_id: u32) -> GameUpdates {
    let kept = updates::kept(&library_dir(&app), app_id);
    GameUpdates {
        asked: kept.as_ref().map(|kept| kept.asked),
        updates: kept
            .map(|kept| kept.updates())
            .unwrap_or_default()
            .into_iter()
            .map(|update| UpdateOut {
                month: steamgauge_core::time::year_month(update.posted),
                through: before_after::through_month(update.posted),
                gid: update.gid,
                title: update.title,
                posted: update.posted,
                link: update.link,
            })
            .collect(),
        window_days: before_after::WINDOW_DAYS,
        enough: before_after::ENOUGH,
    }
}

/// A game's counted reviews as last walked, and what they were walked from: the reading's own
/// file and when it was written, so a read since walks again.
type Walked = (PathBuf, Option<SystemTime>, Arc<Vec<Dated>>);

/// The last game walked. Choosing one update after another is the common case, and a large
/// game's walk takes seconds; one game is kept, since only one page is open.
static WALKED: Mutex<Option<Walked>> = Mutex::new(None);

fn dated(snapshot: &Path, language: Option<&str>) -> Result<Arc<Vec<Dated>>, String> {
    let reading = snapshot.join("reading.json");
    let written = std::fs::metadata(&reading)
        .and_then(|found| found.modified())
        .ok();
    let mut walked = WALKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((path, when, reviews)) = walked.as_ref()
        && *path == reading
        && *when == written
    {
        return Ok(Arc::clone(reviews));
    }
    let reviews = Arc::new(before_after::dated_reviews(snapshot, language).map_err(text)?);
    *walked = Some((reading, written, Arc::clone(&reviews)));
    Ok(reviews)
}

/// Each subject in the four weeks before one of a game's updates against the four weeks after,
/// over every reviewer or, where `kind` names one, over that kind's reviews alone. Off the
/// window's thread, because the first choice on a game walks its capture.
#[tauri::command]
pub async fn before_after(
    app: AppHandle,
    app_id: u32,
    gid: String,
    kind: Option<String>,
) -> Result<Around, String> {
    let dir = library_dir(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let snapshot = embed::latest_snapshot(&dir, app_id).map_err(text)?;
        let reading = read_report(&snapshot)?;
        let all = updates::kept(&dir, app_id)
            .map(|kept| kept.updates())
            .unwrap_or_default();
        let update = all
            .iter()
            .find(|update| update.gid == gid)
            .ok_or_else(|| format!("no update {gid} is kept for app {app_id}"))?;
        let reviews = dated(&snapshot, reading.language.as_deref())?;
        let held = before_after::held_until(&reading, &reviews);
        Ok(match kind.filter(|kind| !kind.is_empty()) {
            None => before_after::around(update, &reviews, held, &all),
            Some(kind) => {
                let at = SEGMENTS
                    .iter()
                    .position(|segment| segment.id == kind)
                    .ok_or_else(|| format!("no kind of reviewer is called {kind}"))?;
                let theirs = before_after::written_by(&reviews, at);
                before_after::around(update, &theirs, held, &all)
            }
        })
    })
    .await
    .map_err(text)?
}
