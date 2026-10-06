//! Update now: downloads the newer release's file for this system, installs it once its build
//! provenance verifies, and closes or restarts the app into the new version. The core decides
//! which file, and whether it may be installed; this runs it and tells the window how far it
//! has got.

use std::{
    fs::File,
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Instant,
};

use serde::Serialize;
use steamgauge_core::{
    newer_version::Asked,
    self_update::{self, Arrived, Install, Origins, RELEASE_BUILD, Version},
};
use tauri::{AppHandle, Emitter, Manager};

use super::{
    text,
    work::{EVERY, Meter, left},
};

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

/// Why an update stopped before installing anything.
struct Stop {
    why: String,
    /// The verified file is on disk and the person can install it themselves.
    file: bool,
}

impl Stop {
    fn new(why: impl std::fmt::Display) -> Self {
        Self {
            why: why.to_string(),
            file: false,
        }
    }
}

/// How this copy was installed, asked once: it does not change while the copy runs.
fn installed() -> Result<Install, String> {
    static FOUND: OnceLock<Result<Install, String>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let program = std::env::current_exe().map_err(text)?;
            on_this_system(&program)
        })
        .clone()
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn on_this_system(program: &Path) -> Result<Install, String> {
    self_update::install::on_windows(program)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn on_this_system(program: &Path) -> Result<Install, String> {
    self_update::install::on_macos(program)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn on_this_system(program: &Path) -> Result<Install, String> {
    use std::process::Stdio;
    let owner = self_update::install::owner_questions(program)
        .into_iter()
        .find_map(|(owner, mut question)| {
            question
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
                .then_some(owner)
        })
        .unwrap_or(self_update::Owner::Nobody);
    self_update::install::on_linux(program, owner)
}

#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
)))]
fn on_this_system(_program: &Path) -> Result<Install, String> {
    Err("no release is built for this kind of computer".to_owned())
}

/// Where the update is now, or what keeps this copy from being updated from the window.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn update_state(updating: tauri::State<'_, Updating>) -> Progress {
    updating.current().unwrap_or_else(|| Progress::Idle {
        why: installed().err(),
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
            updating.set(
                &app,
                Progress::Failed {
                    why: stop.why,
                    file: stop.file,
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
    tauri_plugin_opener::reveal_item_in_dir(file).map_err(text)
}

/// The folder an update is downloaded into, under the app's local data.
fn folder(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|dir| dir.join("update"))
}

/// Removes what an earlier update downloaded, which has been installed or given up on by the
/// time the app opens again.
pub fn tidy(app: &AppHandle) {
    if let Some(folder) = folder(app) {
        let _ = std::fs::remove_dir_all(folder);
    }
}

/// Opens the file a download is written to. On Windows nothing else may open it while it is
/// written, so no other program can change what is being hashed.
fn for_download(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(path)
}

/// Opens the downloaded file to read, and on Windows holds it so that others may read it but
/// nothing may write, rename or delete it until it is closed: from its hash to the setup
/// program's start, the bytes are the bytes that were verified.
fn held(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        options.share_mode(FILE_SHARE_READ);
    }
    options.open(path)
}

async fn update(app: &AppHandle, updating: &Updating) -> Result<(), Stop> {
    let running = Version::parse(env!("CARGO_PKG_VERSION")).map_err(Stop::new)?;
    let config = app.path().app_config_dir().map_err(Stop::new)?;
    let release = Asked::load(&config)
        .newer_than(&running.to_string())
        .ok_or_else(|| Stop::new("no newer release is known"))?;
    let version = Version::parse(&release.version).map_err(Stop::new)?;
    let install = installed().map_err(Stop::new)?;
    let name = install.asset(&version);
    let local = app.path().app_local_data_dir().map_err(Stop::new)?;
    let folder = folder(app).ok_or_else(|| Stop::new("there is no folder to download into"))?;
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).map_err(Stop::new)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))
            .map_err(Stop::new)?;
    }
    let path = folder.join(&name);

    let origins = Origins::github();
    let http = self_update::client().map_err(Stop::new)?;
    let fetched = {
        let mut file = for_download(&path).map_err(Stop::new)?;
        let mut meter = Meter::new("download", 0.0, Instant::now());
        let mut sent = Instant::now()
            .checked_sub(EVERY)
            .unwrap_or_else(Instant::now);
        let mut tell = |done: u64, total: Option<u64>| {
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
        };
        self_update::download(
            &http,
            &origins.file(&version, &name),
            &mut file,
            self_update::MOST,
            &mut tell,
        )
        .await
        .map_err(|failed| Stop::new(format!("the download failed: {failed}")))?
    };

    updating.set(app, Progress::Verifying);
    let mut file = held(&path).map_err(Stop::new)?;
    let sha256 = self_update::sha256_of(&mut file).map_err(Stop::new)?;
    if sha256 != fetched.sha256 {
        return Err(Stop::new(
            "the file changed on disk after it was downloaded",
        ));
    }
    let root = self_update::trusted_root(&http, &origins, &local.join("sigstore-tuf"))
        .await
        .map_err(Stop::new)?;
    let bundles = self_update::provenance(&http, &origins, &version, &sha256)
        .await
        .map_err(Stop::new)?;
    let arrived = Arrived {
        name: &name,
        sha256: &sha256,
        version: &version,
    };
    self_update::verify_any(&bundles, &arrived, &running, &RELEASE_BUILD, &root)
        .map_err(Stop::new)?;
    *updating
        .file
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(path.clone());

    updating.set(app, Progress::Installing);
    file.seek(SeekFrom::Start(0)).map_err(Stop::new)?;
    put_in_place(app, &install, &path, file, &version)
}

/// Installs the verified file `held` from `path` the way `install` says, and ends with the app
/// closed, or opened again as the new version.
fn put_in_place(
    app: &AppHandle,
    install: &Install,
    path: &Path,
    held: File,
    version: &Version,
) -> Result<(), Stop> {
    match install {
        Install::WindowsSetup => {
            self_update::install::setup_command(path)
                .spawn()
                .map_err(|error| Stop::new(format!("the setup program did not start: {error}")))?;
            // Held until the setup program has started, which then holds the file itself.
            drop(held);
            app.exit(0);
            Ok(())
        }
        #[cfg(unix)]
        Install::MacApp { bundle } => {
            let staged = self_update::replace::stage_app(held, bundle).map_err(Stop::new)?;
            match self_update::replace::swap(&staged, bundle) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                    let swapped = self_update::install::privileged_swap(
                        staged.app(),
                        bundle,
                        &staged.backup(),
                    )
                    .status()
                    .is_ok_and(|status| status.success());
                    if !swapped {
                        return Err(Stop::new(
                            "macOS did not allow the app to be replaced: the password prompt was \
                             closed or refused",
                        ));
                    }
                }
                Err(error) => return Err(Stop::new(error)),
            }
            drop(staged);
            app.restart()
        }
        Install::Deb | Install::Rpm => {
            let Some(mut command) = self_update::install::package_command(install, path) else {
                return Err(Stop::new("this package has no manager to install it"));
            };
            drop(held);
            let shown = |why: String| Stop { why, file: true };
            let status = command.status().map_err(|error| {
                shown(format!(
                    "no password prompt is available to install it as root ({error})"
                ))
            })?;
            if self_update::install::declined(status.code()) {
                return Err(shown("the password prompt was closed".to_owned()));
            }
            if !status.success() {
                return Err(shown(format!("the package manager stopped with {status}")));
            }
            app.restart()
        }
        #[cfg(unix)]
        Install::LinuxArchive { dir } => {
            self_update::replace::replace_files(
                held,
                &self_update::install::linux_folder(version),
                dir,
            )
            .map_err(Stop::new)?;
            app.restart()
        }
        #[cfg(not(unix))]
        Install::MacApp { .. } | Install::LinuxArchive { .. } => {
            let _ = (held, version);
            Err(Stop::new("this copy is not updated from the window"))
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "a byte count is shown, and a file of 2^53 bytes is not downloaded"
)]
fn bytes(count: u64) -> f64 {
    count as f64
}
