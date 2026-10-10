//! Update now, as the window shows it: [`crate::update`] downloads, verifies and installs; this
//! starts it, tells the window how far it has got, and closes or restarts the app into the new
//! version.

use std::{path::PathBuf, sync::Mutex, time::Instant};

use serde::Serialize;
use steamgauge_core::{newer_version::Asked, self_update::Version};
use tauri::{AppHandle, Emitter, Manager};

use super::work::{EVERY, Meter, left};
use crate::update::{self as updater, Folders, Step, Stop, Then};

/// Where the update stands, as the window draws it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Progress {
    /// Nothing under way. `why` says why this copy is not updated from the window, where it is
    /// not.
    Idle {
        why: Option<String>,
    },
    Downloading {
        done: f64,
        total: Option<f64>,
        rate: Option<f64>,
        left: Option<f64>,
    },
    Verifying,
    Installing,
    /// Nothing was installed. `file` is whether the verified file is there to be shown.
    Failed {
        why: String,
        file: bool,
    },
}

/// The update under way, if any, and the verified file a failed install leaves to be shown.
#[derive(Debug, Default)]
pub struct Updating {
    now: Mutex<Option<Progress>>,
    file: Mutex<Option<PathBuf>>,
}

impl Updating {
    fn set(&self, app: &AppHandle, progress: Progress) {
        *self
            .now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(progress.clone());
        let _ = app.emit("update", progress);
    }

    fn current(&self) -> Option<Progress> {
        self.now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Where the update stands now, or what keeps this copy from being updated from the window.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn update_state(updating: tauri::State<'_, Updating>) -> Progress {
    updating.current().unwrap_or_else(|| Progress::Idle {
        why: updater::installed().err(),
    })
}

/// Starts the update in the background, unless one is under way. Returns at once.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn update_now(app: AppHandle, updating: tauri::State<'_, Updating>) {
    {
        let mut now = updating
            .now
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if matches!(
            *now,
            Some(Progress::Downloading { .. } | Progress::Verifying | Progress::Installing)
        ) {
            return;
        }
        *now = Some(Progress::Downloading {
            done: 0.0,
            total: None,
            rate: None,
            left: None,
        });
    }
    tauri::async_runtime::spawn(async move {
        let updating = app.state::<Updating>();
        if let Err(stop) = update(&app, &updating).await {
            let file = stop.file.is_some();
            *updating
                .file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = stop.file;
            updating.set(
                &app,
                Progress::Failed {
                    why: stop.why,
                    file,
                },
            );
        }
    });
}

/// Shows the verified file in the system's file manager, for installing it by hand.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn show_update_file(updating: tauri::State<'_, Updating>) -> Result<(), String> {
    let file = updating
        .file
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .ok_or("no update has been downloaded")?;
    tauri_plugin_opener::reveal_item_in_dir(file).map_err(super::text)
}

fn folders(app: &AppHandle) -> Option<Folders> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|local| Folders::under(&local))
}

/// Removes what an earlier update downloaded, which has been installed or given up on by the
/// time the app opens again.
pub fn tidy(app: &AppHandle) {
    if let Some(folders) = folders(app) {
        let _ = std::fs::remove_dir_all(folders.download);
    }
}

async fn update(app: &AppHandle, updating: &Updating) -> Result<(), Stop> {
    let config = app.path().app_config_dir().map_err(Stop::new)?;
    let release = Asked::load(&config)
        .newer_than(env!("CARGO_PKG_VERSION"))
        .ok_or_else(|| Stop::new("no newer release is known"))?;
    let version = Version::parse(&release.version).map_err(Stop::new)?;
    let folders = folders(app).ok_or_else(|| Stop::new("there is no folder to download into"))?;

    let mut meter = Meter::new("download", 0.0, Instant::now());
    let mut sent = Instant::now()
        .checked_sub(EVERY)
        .unwrap_or_else(Instant::now);
    let mut tell = |step: Step| match step {
        Step::Downloading { done, total } => {
            let (done, total) = (bytes(done), total.map(bytes));
            let now = Instant::now();
            let rate = meter.note("download", done, now);
            if now.duration_since(sent) >= EVERY || total == Some(done) {
                sent = now;
                updating.set(
                    app,
                    Progress::Downloading {
                        done,
                        total,
                        rate,
                        left: left(done, total, rate),
                    },
                );
            }
        }
        Step::Verifying => updating.set(app, Progress::Verifying),
        Step::Installing => updating.set(app, Progress::Installing),
    };
    match updater::run(&folders, &version, true, &mut tell).await? {
        Then::Exit => app.exit(0),
        Then::Restart => app.restart(),
    }
    Ok(())
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a byte count is shown, and a file of 2^53 bytes is not downloaded"
)]
fn bytes(count: u64) -> f64 {
    count as f64
}
