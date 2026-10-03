//! Whether a newer `SteamGauge` has been released, and what was last heard about it.
//!
//! GitHub answers a repository's `/releases/latest` with a redirect to the newest release's tag.
//! The tag is read from where the redirect points, and the redirect is never followed: one small
//! request, with no API, no token and no account, so no key exists anywhere that could be taken
//! and used to push a build. Nothing is downloaded; the person installs the new release
//! themselves.

use std::{path::Path, time::Duration};

use serde::{Deserialize, Serialize};

use crate::Result;

/// Where this repository's releases are listed. Every page the notice links to is under it.
pub const RELEASES: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

/// How long an answer stands before it is asked for again, in seconds.
pub const ASK_EVERY: i64 = 24 * 60 * 60;

/// The question runs behind the window and nothing waits on it, so this only bounds how long a
/// dead connection holds a socket open.
const PATIENCE: Duration = Duration::from_secs(10);

const FILE: &str = "newer-version.json";

/// The last time the release page was asked, and what it said.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Asked {
    /// When it was last asked, in Unix seconds; zero for never.
    pub at: i64,
    /// The version of the newest release, from the last answer that came back.
    pub latest: Option<String>,
}

/// A release newer than the running build, as the window shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub running: String,
    /// The release's own page, built here from the version rather than taken from the answer,
    /// so nothing a server says decides what the window opens.
    pub url: String,
}

impl Asked {
    /// What was last heard, or never asked where there is no file or it cannot be read.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Written beside its place and moved over it, so a file is never half one answer.
    ///
    /// # Errors
    ///
    /// Fails where the directory cannot be made or written to.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(FILE);
        let partial = path.with_extension("partial");
        std::fs::write(&partial, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(partial, path)
    }

    /// Whether a day has passed since the last question. A clock set back past it counts as
    /// due, or a clock that was once a year ahead would silence the notice for that year.
    #[must_use]
    pub fn due(&self, now: i64) -> bool {
        now < self.at || now - self.at >= ASK_EVERY
    }

    /// Asks the release page and records the answer. A question that fails, offline or
    /// otherwise, still counts as the day's question and keeps the answer before it.
    pub async fn ask(self, now: i64) -> Self {
        Self {
            at: now,
            latest: latest().await.unwrap_or(self.latest),
        }
    }

    /// The release newer than `running`, where the last answer named one.
    #[must_use]
    pub fn newer_than(&self, running: &str) -> Option<Release> {
        let latest = version(self.latest.as_deref()?)?;
        let ours = version(running)?;
        latest.cmp_precedence(&ours).is_gt().then(|| Release {
            version: latest.to_string(),
            running: ours.to_string(),
            url: format!("{RELEASES}/tag/v{latest}"),
        })
    }
}

/// A tag or a version as a semantic version, with or without the `v` the tags carry.
#[must_use]
pub fn version(text: &str) -> Option<semver::Version> {
    semver::Version::parse(text.strip_prefix('v').unwrap_or(text)).ok()
}

/// The version the release page's redirect names: none where it points anywhere but a release
/// tag of this repository, which is where it points while nothing has been released.
fn version_in(location: &str) -> Option<semver::Version> {
    let path = location
        .strip_prefix("https://github.com")
        .unwrap_or(location);
    let repository = RELEASES.strip_prefix("https://github.com")?;
    let tag = path.strip_prefix(repository)?.strip_prefix("/tag/")?;
    version(tag)
}

/// The version of the newest release, asked of the release page without following its redirect.
///
/// # Errors
///
/// Fails on transport failures, including the timeout.
pub async fn latest() -> Result<Option<String>> {
    let response = reqwest::Client::builder()
        .user_agent(concat!("steamgauge/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(PATIENCE)
        .timeout(PATIENCE)
        .build()?
        .head(format!("{RELEASES}/latest"))
        .send()
        .await?;
    Ok(response
        .status()
        .is_redirection()
        .then(|| response.headers().get(reqwest::header::LOCATION))
        .flatten()
        .and_then(|location| location.to_str().ok())
        .and_then(version_in)
        .map(|found| found.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(latest: &str) -> Asked {
        Asked {
            at: 0,
            latest: Some(latest.to_owned()),
        }
    }

    #[test]
    fn versions_are_compared_as_numbers_and_not_as_text() {
        assert!(heard("0.10.0").newer_than("0.9.3").is_some());
        assert!(heard("1.0.0").newer_than("0.99.99").is_some());
        assert!(heard("0.1.1").newer_than("0.1.0").is_some());
        assert_eq!(heard("0.1.0").newer_than("0.1.0"), None);
        assert_eq!(heard("0.9.0").newer_than("0.10.0"), None);
    }

    #[test]
    fn a_release_is_newer_than_its_own_pre_releases_and_build_metadata_is_ignored() {
        assert!(heard("1.0.0").newer_than("1.0.0-rc.1").is_some());
        assert_eq!(heard("1.0.0-rc.2").newer_than("1.0.0"), None);
        assert_eq!(heard("1.0.0+other").newer_than("1.0.0"), None);
    }

    #[test]
    fn the_notice_names_both_versions_and_links_the_release_page() {
        assert_eq!(
            heard("v0.2.0").newer_than("0.1.0"),
            Some(Release {
                version: "0.2.0".to_owned(),
                running: "0.1.0".to_owned(),
                url: "https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0".to_owned(),
            })
        );
    }

    #[test]
    fn nothing_heard_or_nothing_readable_is_never_newer() {
        assert_eq!(Asked::default().newer_than("0.1.0"), None);
        assert_eq!(heard("latest").newer_than("0.1.0"), None);
        assert_eq!(heard("0.2.0").newer_than("a dev build"), None);
    }

    #[test]
    fn it_is_asked_once_a_day() {
        let asked = Asked {
            at: 1_000_000,
            latest: None,
        };
        assert!(Asked::default().due(1_000_000), "never asked is due");
        assert!(!asked.due(1_000_000));
        assert!(!asked.due(1_000_000 + ASK_EVERY - 1));
        assert!(asked.due(1_000_000 + ASK_EVERY));
        assert!(
            asked.due(1_000_000 - 1),
            "a clock set back before the last question asks again"
        );
    }

    #[test]
    fn a_restart_remembers_the_day_s_answer() {
        let dir = crate::tempdir::Dir::new();
        assert_eq!(Asked::load(dir.path()), Asked::default());
        let asked = Asked {
            at: 1_000_000,
            latest: Some("0.2.0".to_owned()),
        };
        asked.save(dir.path()).unwrap();
        let loaded = Asked::load(dir.path());
        assert_eq!(loaded, asked);
        assert!(!loaded.due(1_000_000 + 60));
    }

    #[test]
    fn the_version_is_read_only_from_a_release_tag_of_this_repository() {
        let release = |tag: &str| version_in(tag).map(|found| found.to_string());
        assert_eq!(
            release("https://github.com/Aureliolo/steamgauge/releases/tag/v0.1.0").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(
            release("/Aureliolo/steamgauge/releases/tag/v1.2.3").as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            release("https://github.com/Aureliolo/steamgauge/releases"),
            None
        );
        assert_eq!(
            release("https://github.com/someone/else/releases/tag/v9.0.0"),
            None
        );
        assert_eq!(
            release("https://example.com/Aureliolo/steamgauge/releases/tag/v9.0.0"),
            None
        );
        assert_eq!(
            release("https://github.com/Aureliolo/steamgauge/releases/tag/nightly"),
            None
        );
    }
}
