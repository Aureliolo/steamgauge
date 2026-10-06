//! How this copy of `SteamGauge` was installed, which decides the release file an update
//! downloads and how that file is put in place.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use semver::Version;

/// How a copy was installed, where that is a way an update can follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Install {
    /// The per-user setup program, which updates over itself without uninstalling first.
    WindowsSetup,
    /// An app bundle, replaced whole.
    MacApp { bundle: PathBuf },
    /// The `.deb`, which the package manager installs.
    Deb,
    /// The `.rpm`, which the package manager installs.
    Rpm,
    /// The portable archive, unpacked into a folder the person owns, whose files are replaced.
    LinuxArchive { dir: PathBuf },
}

/// The app bundle every macOS release holds, by the name the bundler gives it.
pub const APP: &str = "SteamGauge.app";

/// The files beside the program that only the portable archive puts there.
const ARCHIVE_FILES: [&str; 3] = ["README.md", "LICENSE", "THIRD-PARTY-NOTICES.txt"];

impl Install {
    /// The release file this copy is updated from, as `tools/release/names.sh` names it.
    #[must_use]
    pub fn asset(&self, version: &Version) -> String {
        match self {
            Self::WindowsSetup => format!("steamgauge-{version}-windows-x64-setup.exe"),
            Self::MacApp { .. } => format!("steamgauge-{version}-macos-arm64.app.tar.gz"),
            Self::Deb => format!("steamgauge_{version}-1_amd64.deb"),
            Self::Rpm => format!("steamgauge-{version}-1.x86_64.rpm"),
            Self::LinuxArchive { .. } => format!("{}.tar.gz", linux_folder(version)),
        }
    }
}

/// The folder the Linux archive holds its files in.
#[must_use]
pub fn linux_folder(version: &Version) -> String {
    format!("steamgauge-{version}-x86_64-linux-gnu")
}

/// How a copy on Windows was installed, from where its program is: the setup program leaves
/// its uninstaller beside it. Scoop and the portable archive leave none, and a setup program
/// run over either would install a second copy elsewhere.
///
/// # Errors
///
/// Says why a copy installed another way is not updated from the window.
pub fn on_windows(program: &Path) -> Result<Install, String> {
    let dir = program.parent().unwrap_or(program);
    if dir.join("uninstall.exe").is_file() {
        return Ok(Install::WindowsSetup);
    }
    if program
        .components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case("scoop"))
    {
        return Err(
            "Scoop installed this copy, so Scoop updates it: scoop update steamgauge".to_owned(),
        );
    }
    Err(
        "this copy runs from a folder the setup program did not install it in, so it is updated \
         the way it was put there"
            .to_owned(),
    )
}

/// How a copy on macOS was installed: the program inside an app bundle is updated by
/// replacing the bundle.
///
/// # Errors
///
/// Says why a program outside a bundle, the portable archive's, is not updated from the window,
/// nor one macOS runs from the read-only copy it makes of an app opened where it was downloaded.
pub fn on_macos(program: &Path) -> Result<Install, String> {
    if program
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Err(
            "macOS runs this copy from a read-only place of its own, which it does until \
             SteamGauge is moved into Applications"
                .to_owned(),
        );
    }
    let macos = program
        .parent()
        .filter(|dir| dir.file_name() == Some("MacOS".as_ref()));
    let contents = macos
        .and_then(Path::parent)
        .filter(|dir| dir.file_name() == Some("Contents".as_ref()));
    let bundle = contents
        .and_then(Path::parent)
        .filter(|dir| dir.extension() == Some("app".as_ref()));
    bundle
        .map(|bundle| Install::MacApp {
            bundle: bundle.to_path_buf(),
        })
        .ok_or_else(|| {
            "this copy is not inside an app, so it is updated the way it was put there".to_owned()
        })
}

/// Which package database, if either, lists a Linux program as one of its package's files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Dpkg,
    Rpm,
    Nobody,
}

/// The questions that find a Linux program's [`Owner`], each asked in turn: whichever succeeds
/// first owns it, and one that fails or is not installed owns nothing.
#[must_use]
pub fn owner_questions(program: &Path) -> [(Owner, Command); 2] {
    let mut dpkg = Command::new("dpkg-query");
    dpkg.arg("--search").arg(program);
    let mut rpm = Command::new("rpm");
    rpm.arg("--query").arg("--file").arg(program);
    [(Owner::Dpkg, dpkg), (Owner::Rpm, rpm)]
}

/// How a copy on Linux was installed: by a package, or from the portable archive into a folder
/// this account can write to.
///
/// # Errors
///
/// Says why any other copy is not updated from the window.
pub fn on_linux(program: &Path, owner: Owner) -> Result<Install, String> {
    match owner {
        Owner::Dpkg => return Ok(Install::Deb),
        Owner::Rpm => return Ok(Install::Rpm),
        Owner::Nobody => {}
    }
    let dir = program.parent().unwrap_or(program);
    if !ARCHIVE_FILES.iter().all(|file| dir.join(file).is_file()) {
        return Err(
            "this copy was not installed from a release's package or archive, so it is updated \
             the way it was put there"
                .to_owned(),
        );
    }
    if !writable(dir) {
        return Err("this copy's folder is not one this account can write to".to_owned());
    }
    Ok(Install::LinuxArchive {
        dir: dir.to_path_buf(),
    })
}

/// Whether a file can be made in `dir`, which is the only reliable answer: permissions, ACLs
/// and read-only mounts each say no in their own way.
#[must_use]
pub fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".steamgauge-probe-{}", std::process::id()));
    let made = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if made {
        let _ = std::fs::remove_file(&probe);
    }
    made
}

/// Runs the setup program as the in-app update runs it: passive, so it shows its progress and
/// asks nothing; as an update, so it installs over this copy without uninstalling it and leaves
/// its shortcuts where they are; and starting the new program when done.
#[must_use]
pub fn setup_command(setup: &Path) -> Command {
    let mut command = Command::new(setup);
    command.args(["/P", "/UPDATE", "/R"]);
    command
}

/// Installs a verified Linux package through the system's own package manager, which resolves
/// what it depends on, as root after the desktop's password prompt. `None` for a copy no
/// package manager installs.
#[must_use]
pub fn package_command(install: &Install, package: &Path) -> Option<Command> {
    let manager: &[&str] = match install {
        Install::Deb => &["/usr/bin/apt-get", "install", "--yes"],
        Install::Rpm => &["/usr/bin/dnf", "install", "--assumeyes"],
        _ => return None,
    };
    let mut command = Command::new("/usr/bin/pkexec");
    command.args(manager).arg(package);
    Some(command)
}

/// pkexec's status when the person dismissed the prompt or was not allowed, which is not a
/// failed install: nothing was tried.
#[must_use]
pub fn declined(status: Option<i32>) -> bool {
    matches!(status, Some(126 | 127))
}

/// Swaps `staged` and `bundle` as root, after macOS asks for an administrator's password, where
/// this account cannot write beside the bundle. The swap is the same exchange as
/// [`super::replace::swap`], made by `renamex_np` through JavaScript for Automation, which ships
/// with every macOS; where the file system cannot exchange, the bundle is moved to `backup` and
/// the new one into its place, and the old one is restored if that fails. Follows the privileged
/// install of Tauri's updater, `plugins/updater/src/updater.rs` in tauri-apps/plugins-workspace
/// (Apache-2.0 or MIT).
#[must_use]
pub fn privileged_swap(staged: &Path, bundle: &Path, backup: &Path) -> Command {
    const SWAP: &str = r#"function run(argv) {
  ObjC.import("stdio");
  return $.renamex_np(argv[0], argv[1], 2) === 0 ? "swapped" : "failed";
}"#;
    let swap = sh_quote(SWAP.as_ref());
    let (staged, bundle, backup) = (sh_quote(staged), sh_quote(bundle), sh_quote(backup));
    let shell = format!(
        "case \"$(/usr/bin/osascript -l JavaScript -e {swap} {staged} {bundle} 2>/dev/null)\" in \
         swapped) rm -rf {staged};; \
         failed) mv -f {bundle} {backup} && {{ mv -f {staged} {bundle} || {{ mv -f {backup} {bundle}; exit 1; }}; }} && rm -rf {backup};; \
         *) exit 1;; esac"
    );
    let mut command = Command::new("/usr/bin/osascript");
    command.arg("-e").arg(format!(
        "do shell script {} with administrator privileges",
        applescript_string(&shell)
    ));
    command
}

/// `path` as one `sh` word, whatever it holds.
fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

/// `text` as an `AppleScript` string literal.
fn applescript_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', r"\\").replace('"', "\\\""))
}

/// The arguments of `command`, as text, for a test or a log line.
#[must_use]
pub fn words(command: &Command) -> Vec<OsString> {
    std::iter::once(command.get_program().to_os_string())
        .chain(command.get_args().map(std::ffi::OsStr::to_os_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempdir::Dir;

    fn version() -> Version {
        Version::new(0, 2, 0)
    }

    fn texts(command: &Command) -> Vec<String> {
        words(command)
            .into_iter()
            .map(|word| word.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn each_install_takes_the_file_the_release_names_for_it() {
        let named = |install: Install| install.asset(&version());
        assert_eq!(
            named(Install::WindowsSetup),
            "steamgauge-0.2.0-windows-x64-setup.exe"
        );
        assert_eq!(
            named(Install::MacApp {
                bundle: PathBuf::new()
            }),
            "steamgauge-0.2.0-macos-arm64.app.tar.gz"
        );
        assert_eq!(named(Install::Deb), "steamgauge_0.2.0-1_amd64.deb");
        assert_eq!(named(Install::Rpm), "steamgauge-0.2.0-1.x86_64.rpm");
        assert_eq!(
            named(Install::LinuxArchive {
                dir: PathBuf::new()
            }),
            "steamgauge-0.2.0-x86_64-linux-gnu.tar.gz"
        );
    }

    /// The release scripts name every file; a name here they do not write is a file no
    /// release carries.
    #[test]
    fn the_release_scripts_write_every_name_an_update_asks_for() {
        let names = include_str!("../../../../tools/release/names.sh");
        for written in [
            "steamgauge-${VERSION}-windows-x64-setup.exe",
            "steamgauge-${VERSION}-macos-arm64.app.tar.gz",
            "steamgauge_${VERSION}-1_amd64.deb",
            "steamgauge-${VERSION}-1.x86_64.rpm",
            "x86_64-unknown-linux-gnu",
        ] {
            assert!(names.contains(written), "names.sh does not write {written}");
        }
        let package = include_str!("../../../../tools/release/package.sh");
        assert!(
            package.contains(APP),
            "package.sh does not pack the bundle {APP}"
        );
    }

    #[test]
    fn a_windows_copy_beside_its_uninstaller_was_installed_by_the_setup_program() {
        let dir = Dir::new();
        let program = dir.path().join("steamgauge.exe");
        assert!(on_windows(&program).is_err());
        std::fs::write(dir.path().join("uninstall.exe"), b"").unwrap();
        assert_eq!(on_windows(&program), Ok(Install::WindowsSetup));
    }

    #[test]
    fn a_windows_copy_scoop_installed_is_left_to_scoop() {
        let dir = Dir::new();
        let program = dir
            .path()
            .join("Scoop")
            .join("apps")
            .join("steamgauge")
            .join("current")
            .join("steamgauge.exe");
        let refused = on_windows(&program).unwrap_err();
        assert!(refused.contains("scoop update steamgauge"), "{refused}");
        let portable = on_windows(&dir.path().join("steamgauge.exe")).unwrap_err();
        assert!(portable.contains("did not install"), "{portable}");
    }

    #[test]
    fn a_mac_copy_inside_an_app_replaces_that_app() {
        let bundle = Path::new("/Applications/SteamGauge.app");
        let program = bundle.join("Contents").join("MacOS").join("steamgauge");
        assert_eq!(
            on_macos(&program),
            Ok(Install::MacApp {
                bundle: bundle.to_path_buf()
            })
        );
        for elsewhere in [
            "/usr/local/bin/steamgauge",
            "/Applications/SteamGauge/Contents/MacOS/steamgauge",
            "/Applications/SteamGauge.app/Contents/Resources/steamgauge",
            "/Applications/SteamGauge.app/Other/MacOS/steamgauge",
        ] {
            assert!(on_macos(Path::new(elsewhere)).is_err(), "{elsewhere}");
        }
        let moved = on_macos(Path::new(
            "/private/var/folders/xy/T/AppTranslocation/0A1B/d/SteamGauge.app/Contents/MacOS/steamgauge",
        ))
        .unwrap_err();
        assert!(moved.contains("Applications"), "{moved}");
    }

    #[test]
    fn a_linux_program_a_package_owns_is_updated_by_its_package() {
        let program = Path::new("/usr/bin/steamgauge");
        assert_eq!(on_linux(program, Owner::Dpkg), Ok(Install::Deb));
        assert_eq!(on_linux(program, Owner::Rpm), Ok(Install::Rpm));
    }

    #[test]
    fn a_linux_archive_unpacked_into_a_folder_of_one_s_own_has_its_files_replaced() {
        let dir = Dir::new();
        let program = dir.path().join("steamgauge");
        assert!(
            on_linux(&program, Owner::Nobody).is_err(),
            "no archive here"
        );
        for file in ARCHIVE_FILES {
            std::fs::write(dir.path().join(file), b"").unwrap();
        }
        assert_eq!(
            on_linux(&program, Owner::Nobody),
            Ok(Install::LinuxArchive {
                dir: dir.path().to_path_buf()
            })
        );
        std::fs::remove_file(dir.path().join("LICENSE")).unwrap();
        assert!(on_linux(&program, Owner::Nobody).is_err(), "one file short");
    }

    #[test]
    fn a_folder_that_takes_a_file_is_writable_and_the_probe_leaves_nothing() {
        let dir = Dir::new();
        assert!(writable(dir.path()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(!writable(&dir.path().join("missing")));
    }

    #[test]
    fn the_owner_is_asked_of_dpkg_then_rpm_by_the_program_s_path() {
        let [(first, dpkg), (second, rpm)] = owner_questions(Path::new("/usr/bin/steamgauge"));
        assert_eq!((first, second), (Owner::Dpkg, Owner::Rpm));
        assert_eq!(
            texts(&dpkg),
            ["dpkg-query", "--search", "/usr/bin/steamgauge"]
        );
        assert_eq!(
            texts(&rpm),
            ["rpm", "--query", "--file", "/usr/bin/steamgauge"]
        );
    }

    #[test]
    fn the_setup_program_runs_passive_as_an_update_and_starts_the_new_program() {
        assert_eq!(
            texts(&setup_command(Path::new("setup.exe"))),
            ["setup.exe", "/P", "/UPDATE", "/R"]
        );
    }

    #[test]
    fn a_package_is_installed_by_its_manager_as_root_and_nothing_else_is() {
        let package = Path::new("/home/someone/.local/share/steamgauge/update/the.deb");
        assert_eq!(
            texts(&package_command(&Install::Deb, package).unwrap()),
            [
                "/usr/bin/pkexec",
                "/usr/bin/apt-get",
                "install",
                "--yes",
                "/home/someone/.local/share/steamgauge/update/the.deb"
            ]
        );
        assert_eq!(
            texts(&package_command(&Install::Rpm, Path::new("/tmp/the.rpm")).unwrap()),
            [
                "/usr/bin/pkexec",
                "/usr/bin/dnf",
                "install",
                "--assumeyes",
                "/tmp/the.rpm"
            ]
        );
        assert!(package_command(&Install::WindowsSetup, package).is_none());
        assert!(
            package_command(
                &Install::LinuxArchive {
                    dir: PathBuf::new()
                },
                package
            )
            .is_none()
        );
    }

    #[test]
    fn a_prompt_dismissed_is_told_apart_from_an_install_that_failed() {
        assert!(declined(Some(126)));
        assert!(declined(Some(127)));
        assert!(!declined(Some(0)));
        assert!(!declined(Some(1)));
        assert!(!declined(Some(100)));
        assert!(!declined(None));
    }

    #[test]
    fn the_privileged_swap_quotes_every_path_for_the_shell_and_for_applescript() {
        let command = privileged_swap(
            Path::new("/tmp/stage/SteamGauge.app"),
            Path::new("/Applications/Steam\"Gauge's.app"),
            Path::new("/tmp/stage/previous"),
        );
        let words = texts(&command);
        assert_eq!(words[..2], ["/usr/bin/osascript", "-e"]);
        let script = &words[2];
        assert!(script.starts_with("do shell script \""), "{script}");
        assert!(
            script.ends_with("\" with administrator privileges"),
            "{script}"
        );
        assert!(
            script.contains(r#"'/Applications/Steam\"Gauge'\\''s.app'"#),
            "the bundle's quote and apostrophe are escaped: {script}"
        );
        assert!(
            script.contains("renamex_np(argv[0], argv[1], 2)"),
            "{script}"
        );
        assert!(
            script.contains("swapped) rm -rf '/tmp/stage/SteamGauge.app';;"),
            "{script}"
        );
    }

    #[test]
    fn a_shell_word_survives_an_apostrophe_and_applescript_a_backslash() {
        assert_eq!(sh_quote(Path::new("it's")), r"'it'\''s'");
        assert_eq!(applescript_string(r#"a\b"c"#), r#""a\\b\"c""#);
    }
}
