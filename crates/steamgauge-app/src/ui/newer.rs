//! The notice that a newer `SteamGauge` is out. The core asks and remembers; this decides when
//! to ask, where the answer is kept, and tells the window.

use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::Serialize;
use steamgauge_core::{
    newer_version::{Asked, LATEST, Opened, RELEASES, Release, version},
    release_notes::{self, Changed},
};
use tauri::{AppHandle, Emitter, Manager};

use super::{now_unix, settings::Settings};
use crate::update::{Folders, Verified};

/// Opening the app and saving the settings can both start a question; one is enough.
static ASKING: AtomicBool = AtomicBool::new(false);

fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok()
}

/// Asks in the background where the person wants it, however recently it was asked: a release
/// that came out since the last question is offered the next time the app opens.
pub fn check_on_opening(app: &AppHandle) {
    if let Some(dir) = dir(app) {
        ask_in_background(app, dir);
    }
}

/// Asks in the background where the person wants it and
/// [`ASK_EVERY`](steamgauge_core::newer_version::ASK_EVERY) has passed, for an app left open.
pub fn check_when_due(app: &AppHandle) {
    if let Some(dir) = dir(app)
        && Asked::load(&dir).due(now_unix())
    {
        ask_in_background(app, dir);
    }
}

/// Tells the window when the answer names a newer release. Returns at once: opening the app never
/// waits on it.
fn ask_in_background(app: &AppHandle, dir: PathBuf) {
    if !Settings::load(app).check_newer_version || ASKING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let asked = Asked::load(&dir).ask(LATEST, now_unix()).await;
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

/// Where a problem is reported: the issue forms, which ask for what [`About::details`] holds.
const ISSUES: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new/choose");

/// The repository's security policy, `SECURITY.md` as GitHub shows it.
const CHECKING: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/security/policy");

/// The file every release carries the third-party licences and notices in.
const NOTICES: &str = "THIRD-PARTY-NOTICES.txt";

/// This copy, and what GitHub last said about the newest release, as the About page shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct About {
    pub running: String,
    /// Who makes `SteamGauge`, by name.
    pub made_by: String,
    /// The newest release GitHub last named.
    pub latest: Option<String>,
    /// When GitHub was last asked, in Unix seconds; zero for never.
    pub asked: i64,
    /// When GitHub last answered, in Unix seconds; none where that is not known.
    pub answered: Option<i64>,
    /// A release newer than this copy, where the last answer named one.
    pub newer: Option<Release>,
    /// The version this copy was updated from, and whether what changed has been shown.
    pub updated: Option<Updated>,
    /// What a report of a problem needs to say about this copy, in the order it is shown.
    pub details: Vec<Detail>,
    /// This copy's own release page, which the window may open.
    pub notes: String,
    pub site: &'static str,
    pub repository: &'static str,
    pub issues: &'static str,
    /// The security policy, which says how a download is checked by hand.
    pub checking: &'static str,
    pub licence: &'static str,
    /// What `SteamGauge` is built on, from the notices' table of contents.
    pub built_on: Vec<String>,
    /// Whether this copy carries the third-party notices, which [`open_notices`] opens.
    pub notices: bool,
    /// Whether the app asks GitHub by itself, on opening and every six hours while open.
    pub by_itself: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Updated {
    pub from: String,
    pub seen: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Detail {
    pub name: &'static str,
    pub value: String,
}

/// The authors' names, without the addresses Cargo keeps beside them.
fn made_by() -> String {
    env!("CARGO_PKG_AUTHORS")
        .split(':')
        .map(|author| author.split(" <").next().unwrap_or(author).trim())
        .collect::<Vec<_>>()
        .join(", ")
}

fn system() -> String {
    let os = match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86-64",
        "aarch64" => "Arm64",
        other => other,
    };
    format!("{os}, {arch}")
}

fn installed() -> Detail {
    use steamgauge_core::self_update::Install;
    match crate::update::installed() {
        Ok(install) => Detail {
            name: "Installed with",
            value: match install {
                Install::WindowsSetup => "the setup program",
                Install::MacApp { .. } => "the app bundle",
                Install::Deb => "the .deb package",
                Install::Rpm => "the .rpm package",
                Install::LinuxArchive { .. } => "the portable archive",
            }
            .to_owned(),
        },
        Err(why) => {
            let mut letters = why.chars();
            Detail {
                name: "Updates",
                value: letters
                    .next()
                    .map(|first| first.to_uppercase().chain(letters).collect())
                    .unwrap_or_default(),
            }
        }
    }
}

/// Whether this copy's build was checked: by the update that installed it, which verified the
/// release file's provenance before installing it, or not at all.
fn build(verified: Option<&Verified>) -> Detail {
    Detail {
        name: "Build",
        value: verified.map_or_else(
            || "Not checked by SteamGauge, which checks the copies Update now installs".to_owned(),
            |verified| {
                format!(
                    "Checked before it was installed: SteamGauge's release workflow built {} for \
                     v{}",
                    verified.file, verified.version
                )
            },
        ),
    }
}

/// Where the release put the third-party notices: beside the program from the setup program and
/// the portable archives, in the bundle's Resources on macOS, and with the package's
/// documentation from a `.deb` or `.rpm`.
fn notices_file() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    let dir = program.parent()?;
    [
        dir.join(NOTICES),
        dir.join("..").join("Resources").join(NOTICES),
        Path::new("/usr/share/doc/steamgauge").join(NOTICES),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

/// What the notices' table of contents names, with the number of crates in place of the bare
/// heading, and without the part that only gathers the crates' own NOTICE files.
fn built_on(head: &str) -> Vec<String> {
    let crates = head.lines().find_map(|line| {
        line.split_once(" crates.")
            .and_then(|(count, _)| count.trim().parse::<usize>().ok())
    });
    head.lines()
        .take_while(|line| !line.starts_with("===="))
        .filter_map(|line| {
            let (number, part) = line.trim().split_once(". ")?;
            number.parse::<usize>().ok()?;
            Some(part)
        })
        .filter(|part| !part.starts_with("NOTICE files"))
        .map(|part| match (part, crates) {
            ("Rust crates", Some(count)) => format!("{count} Rust crates"),
            _ => part.to_owned(),
        })
        .collect()
}

/// The start of the notices, which holds the table of contents and the crate count; the file is
/// about a megabyte, and About needs only this.
fn notices_head(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(16 << 10)
        .read_to_string(&mut head)
        .ok()?;
    Some(head)
}

fn about_from(
    asked: &Asked,
    opened: Option<&Opened>,
    verified: Option<&Verified>,
    by_itself: bool,
) -> About {
    let running = env!("CARGO_PKG_VERSION");
    let notices = notices_file();
    let mut details = vec![
        Detail {
            name: "Version",
            value: running.to_owned(),
        },
        Detail {
            name: "System",
            value: system(),
        },
        installed(),
    ];
    if let Ok(webview) = tauri::webview_version() {
        details.push(Detail {
            name: "WebView",
            value: webview,
        });
    }
    details.push(build(
        verified.filter(|verified| verified.version == running),
    ));
    About {
        running: running.to_owned(),
        made_by: made_by(),
        latest: asked.latest.clone(),
        asked: asked.at,
        answered: asked.answered,
        newer: asked.newer_than(running),
        updated: opened
            .filter(|opened| opened.version == running)
            .and_then(|opened| {
                Some(Updated {
                    from: opened.from.clone()?,
                    seen: opened.seen,
                })
            }),
        details,
        notes: format!("{RELEASES}/tag/v{running}"),
        site: env!("CARGO_PKG_HOMEPAGE"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        issues: ISSUES,
        checking: CHECKING,
        licence: env!("CARGO_PKG_LICENSE"),
        built_on: notices
            .as_deref()
            .and_then(notices_head)
            .map(|head| built_on(&head))
            .unwrap_or_default(),
        notices: notices.is_some(),
        by_itself,
    }
}

fn verified(app: &AppHandle) -> Option<Verified> {
    let local = app.path().app_local_data_dir().ok()?;
    Verified::load(&Folders::under(&local).verified)
}

/// Opens the third-party notices this copy carries in the system's viewer for text.
///
/// # Errors
///
/// Says where the notices are not beside this copy, as in a build not made by the release.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn open_notices(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let path = notices_file().ok_or(
        "this copy carries no third-party notices; a release carries them beside the program",
    )?;
    app.opener()
        .open_path(path.display().to_string(), None::<&str>)
        .map_err(super::text)
}

/// This copy and the newest release last heard of; nothing is asked here.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn about(app: AppHandle) -> About {
    let dir = dir(&app);
    let asked = dir.as_deref().map(Asked::load).unwrap_or_default();
    let opened = dir.as_deref().and_then(Opened::load);
    about_from(
        &asked,
        opened.as_ref(),
        verified(&app).as_ref(),
        Settings::load(&app).check_newer_version,
    )
}

/// Asks GitHub now for the newest release, whatever the setting and however recently it was
/// asked: the person asked. The window is told either way, so a notice the answer no longer
/// bears out goes.
#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<About, String> {
    let dir = dir(&app).ok_or("there is no folder to keep the answer in")?;
    let asked = Asked::load(&dir).ask(LATEST, now_unix()).await;
    asked.save(&dir).map_err(super::text)?;
    let _ = app.emit("newer-version", ());
    Ok(about_from(
        &asked,
        Opened::load(&dir).as_ref(),
        verified(&app).as_ref(),
        Settings::load(&app).check_newer_version,
    ))
}

/// Keeps the version this copy opens as, and the one before it where that is older, once as the
/// app starts.
pub fn remember_opening(app: &AppHandle) {
    let Some(dir) = dir(app) else {
        return;
    };
    let kept = Opened::load(&dir);
    let opening = Opened::opening(kept.clone(), env!("CARGO_PKG_VERSION"));
    if kept.as_ref() != Some(&opening) {
        let _ = opening.save(&dir);
    }
}

/// The person has been shown what changed since the version this copy was updated from, so the
/// rail stops pointing at it.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri hands a command its arguments by value"
)]
pub fn saw_what_changed(app: AppHandle) -> Result<(), String> {
    let dir = dir(&app).ok_or("there is no folder the record is kept in")?;
    let Some(mut opened) = Opened::load(&dir) else {
        return Ok(());
    };
    if !opened.seen {
        opened.seen = true;
        opened.save(&dir).map_err(super::text)?;
        let _ = app.emit("newer-version", ());
    }
    Ok(())
}

/// What each release between two versions changed in what people download, newest first.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WhatsNew {
    pub from: String,
    pub to: String,
    pub releases: Vec<Changed>,
}

/// The last list GitHub gave, kept while the app runs: the API answers each address sixty times
/// an hour, and the About page is opened more often than releases come out.
static KEPT: Mutex<Option<WhatsNew>> = Mutex::new(None);

/// What changed between this copy and the newer release last heard of, or, with none, between
/// the version this copy was updated from and this one; nothing where there is neither.
///
/// # Errors
///
/// Says why GitHub gave no list.
#[tauri::command]
pub async fn whats_new(app: AppHandle) -> Result<Option<WhatsNew>, String> {
    let running = env!("CARGO_PKG_VERSION");
    let dir = dir(&app).ok_or("there is no folder the record is kept in")?;
    let (from, to) = if let Some(newer) = Asked::load(&dir).newer_than(running) {
        (newer.running, newer.version)
    } else if let Some(Opened {
        from: Some(from), ..
    }) = Opened::load(&dir).filter(|opened| opened.version == running)
    {
        (from, running.to_owned())
    } else {
        return Ok(None);
    };
    if let Some(kept) = KEPT
        .lock()
        .ok()
        .and_then(|kept| kept.clone())
        .filter(|kept| kept.from == from && kept.to == to)
    {
        return Ok(Some(kept));
    }
    let (Some(after), Some(upto)) = (version(&from), version(&to)) else {
        return Ok(None);
    };
    let releases = release_notes::between(release_notes::RELEASES_API, &after, &upto)
        .await
        .map_err(super::text)?;
    let listed = WhatsNew { from, to, releases };
    if let Ok(mut kept) = KEPT.lock() {
        *kept = Some(listed.clone());
    }
    Ok(Some(listed))
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
            latest: Some("0.2.0".to_owned()),
            ..steamgauge_core::newer_version::Asked::default()
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
    fn the_about_page_may_open_its_own_addresses_and_nothing_beside_them() {
        let about = super::about_from(
            &steamgauge_core::newer_version::Asked::default(),
            None,
            None,
            true,
        );
        for address in [
            &about.notes,
            about.site,
            about.repository,
            about.issues,
            about.checking,
        ] {
            assert!(may_open(address), "{address}");
        }
        for refused in [
            "https://aureliolo.github.io/steamgauge/elsewhere",
            "https://aureliolo.github.io/other/",
            "https://github.com/Aureliolo/steamgauge/settings",
            "https://github.com/Aureliolo/steamgauge/issues/new?template=elsewhere",
            "https://github.com/Aureliolo/steamgauge-elsewhere",
            "https://github.com/Aureliolo/steamgauge/security/advisories",
        ] {
            assert!(!may_open(refused), "{refused} may be opened");
        }
        assert_eq!(about.running, env!("CARGO_PKG_VERSION"));
        assert_eq!((about.asked, about.answered, about.newer), (0, None, None));
        assert_eq!(about.updated, None);
        assert_eq!(about.made_by, "Aurelio Amoroso");
        assert_eq!(about.details[0].value, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_build_is_said_checked_only_when_the_update_that_installed_this_version_checked_it() {
        let asked = steamgauge_core::newer_version::Asked::default();
        let running = env!("CARGO_PKG_VERSION");
        let verified = |version: &str| crate::update::Verified {
            version: version.to_owned(),
            file: format!("steamgauge-{version}-windows-x64-setup.exe"),
            sha256: "ab".repeat(32),
        };
        let build = |about: super::About| {
            about
                .details
                .into_iter()
                .find(|detail| detail.name == "Build")
                .unwrap()
                .value
        };
        assert_eq!(
            build(super::about_from(
                &asked,
                None,
                Some(&verified(running)),
                true
            )),
            format!(
                "Checked before it was installed: SteamGauge's release workflow built \
                 steamgauge-{running}-windows-x64-setup.exe for v{running}"
            )
        );
        for unchecked in [None, Some(verified("0.0.1"))] {
            assert!(
                build(super::about_from(&asked, None, unchecked.as_ref(), true))
                    .starts_with("Not checked by SteamGauge")
            );
        }
    }

    #[test]
    fn what_steamgauge_is_built_on_is_read_from_the_notices_contents() {
        let head = "THIRD-PARTY SOFTWARE NOTICES\nsteamgauge for x86_64-pc-windows-msvc\n\n  \
                    1. Rust crates\n  2. NOTICE files the crates carry\n  3. ONNX Runtime 1.31.0\n  \
                    4. DirectML 1.15.4\n\n=====\n1. Rust crates\n=====\n\n498 crates. Each line \
                    gives the crate\n";
        assert_eq!(
            super::built_on(head),
            ["498 Rust crates", "ONNX Runtime 1.31.0", "DirectML 1.15.4"]
        );
        assert_eq!(super::built_on(""), Vec::<String>::new());
    }

    #[test]
    fn a_copy_tells_what_changed_only_since_a_version_it_was_updated_from() {
        let opened = |version: &str, from: Option<&str>| steamgauge_core::newer_version::Opened {
            version: version.to_owned(),
            from: from.map(str::to_owned),
            seen: false,
        };
        let asked = steamgauge_core::newer_version::Asked::default();
        let running = env!("CARGO_PKG_VERSION");
        assert_eq!(
            super::about_from(&asked, Some(&opened(running, Some("0.0.1"))), None, true).updated,
            Some(super::Updated {
                from: "0.0.1".to_owned(),
                seen: false
            })
        );
        assert_eq!(
            super::about_from(&asked, Some(&opened(running, None)), None, true).updated,
            None
        );
        assert_eq!(
            super::about_from(&asked, Some(&opened("0.0.2", Some("0.0.1"))), None, true).updated,
            None,
            "a record another version kept"
        );
    }

    #[test]
    fn an_update_opens_on_steams_store_and_the_news_apis_own_address_is_not_allowed() {
        assert!(may_open(&steamgauge_core::updates::link(
            "1844751498220713"
        )));
        // The address Steam's news API names for a post is on a content server; the window
        // never opens it, and the allow-list does not reach it.
        assert!(!may_open(
            "https://steamstore-a.akamaihd.net/news/externalpost/steam_community_announcements/\
             1844751498220713"
        ));
    }

    #[test]
    fn steam_pages_may_still_be_opened() {
        assert!(may_open(
            "https://steamcommunity.com/profiles/76561197960287930/recommended/1/"
        ));
        assert!(may_open("https://store.steampowered.com/app/1/"));
    }
}
