//! Updating this copy to a newer release: the release's file for the way this copy was installed
//! is downloaded, hashed as it is written, installed once its build provenance verifies, and
//! nothing is installed otherwise. The core decides which file and whether it may be installed;
//! this runs it. The window's Update now and `steamgauge update` both come here, so the path a
//! release is checked through is the path a person's update takes.

use std::{
    fs::File,
    io::{Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use steamgauge_core::self_update::{self, Arrived, Install, Origins, RELEASE_BUILD, Version};

/// The identifier `tauri.conf.json` gives the app, which names its folders on every system.
pub const IDENTIFIER: &str = "com.aureliolo.steamgauge";

/// How far an update has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Downloading { done: u64, total: Option<u64> },
    Verifying,
    Installing,
}

/// Why an update stopped before installing anything.
#[derive(Debug)]
pub struct Stop {
    pub why: String,
    /// The verified file, where it is on disk for the person to install themselves.
    pub file: Option<PathBuf>,
}

impl Stop {
    pub fn new(why: impl std::fmt::Display) -> Self {
        Self {
            why: why.to_string(),
            file: None,
        }
    }
}

/// What follows once the new version is in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    /// The setup program is installing it and closes this copy, so this copy exits now.
    Exit,
    /// It is installed; what is running is the old version until it starts again.
    Restart,
}

/// Where an update keeps what it fetches, under the app's local data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folders {
    /// The release's file, removed before each update and when the app next opens.
    pub download: PathBuf,
    /// Sigstore's trusted root, as its TUF repository last served it.
    pub tuf: PathBuf,
}

impl Folders {
    #[must_use]
    pub fn under(local: &Path) -> Self {
        Self {
            download: local.join("update"),
            tuf: local.join("sigstore-tuf"),
        }
    }
}

/// How this copy was installed, asked once: it does not change while the copy runs.
///
/// # Errors
///
/// Says why this copy is not updated from inside itself.
pub fn installed() -> Result<Install, String> {
    static FOUND: OnceLock<Result<Install, String>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let program = std::env::current_exe().map_err(|error| error.to_string())?;
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

/// Updates this copy to `version`, telling `tell` each step and the download's progress.
/// `reopen` is whether the new version opens its window once a setup program has installed it,
/// which is what a person updating from the window expects and a command line does not.
///
/// # Errors
///
/// Stops, having installed nothing, where the file cannot be fetched, does not verify, or cannot
/// be put in place.
pub async fn run(
    folders: &Folders,
    version: &Version,
    reopen: bool,
    tell: &mut (dyn FnMut(Step) + Send),
) -> Result<Then, Stop> {
    let running = Version::parse(env!("CARGO_PKG_VERSION")).map_err(Stop::new)?;
    let install = installed().map_err(Stop::new)?;
    let name = install.asset(version);
    let _ = std::fs::remove_dir_all(&folders.download);
    std::fs::create_dir_all(&folders.download).map_err(Stop::new)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&folders.download, std::fs::Permissions::from_mode(0o700))
            .map_err(Stop::new)?;
    }
    let path = folders.download.join(&name);

    let origins = Origins::github();
    let http = self_update::client().map_err(Stop::new)?;
    let fetched = {
        let mut file = for_download(&path).map_err(Stop::new)?;
        self_update::download(
            &http,
            &origins.file(version, &name),
            &mut file,
            self_update::MOST,
            &mut |done, total| tell(Step::Downloading { done, total }),
        )
        .await
        .map_err(|failed| Stop::new(format!("the download failed: {failed}")))?
    };

    tell(Step::Verifying);
    let mut file = held(&path).map_err(Stop::new)?;
    let sha256 = self_update::sha256_of(&mut file).map_err(Stop::new)?;
    if sha256 != fetched.sha256 {
        return Err(Stop::new(
            "the file changed on disk after it was downloaded",
        ));
    }
    let root = self_update::trusted_root(&http, &origins, &folders.tuf)
        .await
        .map_err(Stop::new)?;
    let bundles = self_update::provenance(&http, &origins, version, &sha256)
        .await
        .map_err(Stop::new)?;
    let arrived = Arrived {
        name: &name,
        sha256: &sha256,
        version,
    };
    self_update::verify_any(&bundles, &arrived, &running, &RELEASE_BUILD, &root)
        .map_err(Stop::new)?;

    tell(Step::Installing);
    file.seek(SeekFrom::Start(0)).map_err(Stop::new)?;
    put_in_place(&install, &path, file, version, reopen)
}

/// Installs the verified file `held` from `path` the way `install` says. Where a setup program
/// or a package fails to install, the file is left to be installed by hand; an app or an archive
/// is not something a person installs that way.
fn put_in_place(
    install: &Install,
    path: &Path,
    held: File,
    version: &Version,
    reopen: bool,
) -> Result<Then, Stop> {
    let shown = |why: String| Stop {
        why,
        file: Some(path.to_owned()),
    };
    match install {
        Install::WindowsSetup => {
            self_update::install::setup_command(path, reopen)
                .spawn()
                .map_err(|error| shown(format!("the setup program did not start: {error}")))?;
            // Held until the setup program has started, which then holds the file itself.
            drop(held);
            Ok(Then::Exit)
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
            Ok(Then::Restart)
        }
        Install::Deb | Install::Rpm => {
            let Some(mut command) = self_update::install::package_command(install, path) else {
                return Err(Stop::new("this package has no manager to install it"));
            };
            drop(held);
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
            Ok(Then::Restart)
        }
        #[cfg(unix)]
        Install::LinuxArchive { dir } => {
            self_update::replace::replace_files(
                held,
                &self_update::install::linux_folder(version),
                dir,
            )
            .map_err(Stop::new)?;
            Ok(Then::Restart)
        }
        #[cfg(not(unix))]
        Install::MacApp { .. } | Install::LinuxArchive { .. } => {
            let _ = (held, version);
            Err(Stop::new("this copy is not updated from inside itself"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identifier_is_the_one_the_app_is_configured_with() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config["identifier"], IDENTIFIER);
    }

    #[test]
    fn an_update_keeps_its_files_under_the_local_data() {
        let folders = Folders::under(Path::new("local"));
        assert_eq!(folders.download, Path::new("local").join("update"));
        assert_eq!(folders.tuf, Path::new("local").join("sigstore-tuf"));
    }
}
