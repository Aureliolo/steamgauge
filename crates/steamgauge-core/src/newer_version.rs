//! Whether a newer `SteamGauge` has been released, and what was last heard about it.
//!
//! GitHub answers a repository's `/releases/latest` with a redirect to the newest release's tag.
//! The tag is read from where the redirect points, and the redirect is never followed: one small
//! request, with no API, no token and no account, so no key exists anywhere that could be taken
//! and used to push a build. Nothing is downloaded here; `self_update` installs a release.

use std::{path::Path, time::Duration};

use serde::{Deserialize, Serialize};

use crate::Result;

/// Where this repository's releases are listed. Every page the notice links to is under it.
pub const RELEASES: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

/// The page that redirects to the newest release, which is the question [`Asked::ask`] asks.
pub const LATEST: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/latest");

/// How long an answer stands, in an app left open, before it is asked for again, in seconds.
/// Opening the app always asks.
pub const ASK_EVERY: i64 = 6 * 60 * 60;

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
    /// When an answer last came back, in Unix seconds. Behind `at` when the last question went
    /// unanswered; none where no answer has, or where the file was kept by a build that did not
    /// record it, so an old file is never read as GitHub out of reach.
    pub answered: Option<i64>,
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
        keep(self, dir, FILE)
    }

    /// Whether [`ASK_EVERY`] has passed since the last question. A clock set back past it counts
    /// as due, or a clock that was once a year ahead would silence the notice for that year.
    #[must_use]
    pub fn due(&self, now: i64) -> bool {
        now < self.at || now - self.at >= ASK_EVERY
    }

    /// Asks `page`, which is [`LATEST`], and records the answer. A question that fails, offline
    /// or otherwise, still counts as a question and keeps the answer before it.
    pub async fn ask(self, page: &str, now: i64) -> Self {
        match latest(page).await {
            Ok(latest) => Self {
                at: now,
                answered: Some(now),
                latest,
            },
            Err(_) => Self { at: now, ..self },
        }
    }

    /// Whether the last question was answered.
    #[must_use]
    pub fn reached(&self) -> bool {
        self.at != 0 && self.answered == Some(self.at)
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

fn keep(value: &impl Serialize, dir: &Path, file: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file);
    let partial = path.with_extension("partial");
    std::fs::write(&partial, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(partial, path)
}

const OPENED: &str = "opened-version.json";

/// The version this copy last opened as, and the one it was updated from, so a copy that opens
/// as a newer version can say what changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Opened {
    pub version: String,
    /// The version opened before this one, where this one is newer.
    pub from: Option<String>,
    /// Whether the person has been shown what changed since `from`.
    pub seen: bool,
}

impl Opened {
    /// What was kept, or none where there is no file or it cannot be read.
    #[must_use]
    pub fn load(dir: &Path) -> Option<Self> {
        std::fs::read(dir.join(OPENED))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    }

    /// Written beside its place and moved over it, so a file is never half a record.
    ///
    /// # Errors
    ///
    /// Fails where the directory cannot be made or written to.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        keep(self, dir, OPENED)
    }

    /// What opening as `running` makes of what was `kept`. A first opening has nothing to tell,
    /// and nor has one after a newer version, a copy put back. One after an older version tells
    /// what changed since it until that is shown, and since the oldest version not yet shown
    /// where several updates came in between.
    #[must_use]
    pub fn opening(kept: Option<Self>, running: &str) -> Self {
        let quiet = Self {
            version: running.to_owned(),
            from: None,
            seen: true,
        };
        let Some(kept) = kept else {
            return quiet;
        };
        if kept.version == running {
            return kept;
        }
        match (version(&kept.version), version(running)) {
            (Some(before), Some(now)) if before < now => Self {
                version: running.to_owned(),
                from: Some(kept.from.filter(|_| !kept.seen).unwrap_or(kept.version)),
                seen: false,
            },
            _ => quiet,
        }
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

/// The version of the newest release, asked of `page`, which is [`LATEST`], without following
/// its redirect.
///
/// # Errors
///
/// Fails on transport failures, including the timeout.
pub async fn latest(page: &str) -> Result<Option<String>> {
    let response = crate::http::builder()
        .user_agent(concat!("steamgauge/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(PATIENCE)
        .timeout(PATIENCE)
        .build()?
        .head(page)
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
    use crate::stand_in;

    fn heard(latest: &str) -> Asked {
        Asked {
            latest: Some(latest.to_owned()),
            ..Asked::default()
        }
    }

    fn opened(version: &str, from: Option<&str>, seen: bool) -> Opened {
        Opened {
            version: version.to_owned(),
            from: from.map(str::to_owned),
            seen,
        }
    }

    #[test]
    fn opening_as_a_newer_version_tells_what_changed_since_the_one_before_until_it_is_shown() {
        assert_eq!(
            Opened::opening(None, "0.2.12"),
            opened("0.2.12", None, true),
            "a first opening"
        );
        let updated = Opened::opening(Some(opened("0.2.10", None, true)), "0.2.12");
        assert_eq!(updated, opened("0.2.12", Some("0.2.10"), false));
        assert_eq!(
            Opened::opening(Some(updated.clone()), "0.2.12"),
            updated,
            "opening again as the same version keeps the record"
        );
        assert_eq!(
            Opened::opening(Some(updated), "0.2.13"),
            opened("0.2.13", Some("0.2.10"), false),
            "an update not yet shown is told with the next"
        );
        assert_eq!(
            Opened::opening(Some(opened("0.2.12", Some("0.2.10"), true)), "0.2.13"),
            opened("0.2.13", Some("0.2.12"), false)
        );
        assert_eq!(
            Opened::opening(Some(opened("0.2.13", None, true)), "0.2.12"),
            opened("0.2.12", None, true),
            "a copy put back tells nothing"
        );
    }

    #[test]
    fn what_was_opened_is_kept_across_openings() {
        let dir = crate::tempdir::Dir::new();
        assert_eq!(Opened::load(dir.path()), None);
        let kept = opened("0.2.12", Some("0.2.10"), false);
        kept.save(dir.path()).unwrap();
        assert_eq!(Opened::load(dir.path()), Some(kept));
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
    fn an_answer_stands_for_six_hours() {
        let asked = Asked {
            at: 1_000_000,
            ..Asked::default()
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
    fn six_hours_are_six_hours_and_not_one_sooner() {
        let asked = Asked {
            at: 1_000_000,
            ..Asked::default()
        };
        assert!(!asked.due(1_000_000 + 5 * 60 * 60));
        assert!(asked.due(1_000_000 + 6 * 60 * 60));
    }

    /// A stand-in release page answering as GitHub does: a redirect to the newest tag.
    fn release_page(answer: stand_in::Answer) -> (stand_in::Server, String) {
        let server = stand_in::Server::new(move |_| answer.clone());
        let page = format!("{}/latest", server.origin());
        (server, page)
    }

    fn tag_page(tag: &str) -> String {
        let repository = RELEASES.strip_prefix("https://github.com").unwrap();
        format!("{repository}/tag/{tag}")
    }

    fn redirect_to(tag: &str) -> stand_in::Answer {
        stand_in::Answer::status(302).header("Location", &tag_page(tag))
    }

    #[tokio::test]
    async fn asking_records_when_and_the_release_the_redirect_names() {
        let (server, page) = release_page(redirect_to("v9.9.9"));
        let asked = Asked::default().ask(&page, 1_000_000).await;
        assert_eq!(
            asked,
            Asked {
                at: 1_000_000,
                answered: Some(1_000_000),
                latest: Some("9.9.9".to_owned()),
            }
        );
        assert!(asked.reached());
        let request = &server.asked()[0];
        assert_eq!(request.method, "HEAD", "nothing is downloaded");
        assert!(
            request
                .header("user-agent")
                .is_some_and(|agent| agent.starts_with("steamgauge/")),
            "{request:?}"
        );
    }

    #[tokio::test]
    async fn a_question_that_fails_keeps_the_answer_before_it_and_still_counts() {
        let (_server, page) = release_page(stand_in::Answer::hang_up());
        let before = Asked {
            at: 900_000,
            answered: Some(900_000),
            ..heard("0.2.0")
        };
        let asked = before.clone().ask(&page, 1_000_000).await;
        assert_eq!(
            asked,
            Asked {
                at: 1_000_000,
                ..before
            }
        );
        assert!(!asked.due(1_000_000 + 60));
        assert!(!asked.reached(), "the answer kept is the one from before");
    }

    #[test]
    fn never_asked_is_not_reached() {
        assert!(!Asked::default().reached());
    }

    #[tokio::test]
    async fn an_answer_that_is_not_a_redirect_names_no_release() {
        let (_server, page) =
            release_page(stand_in::Answer::status(200).header("Location", &tag_page("v1.0.0")));
        assert_eq!(latest(&page).await.unwrap(), None);
    }

    #[test]
    fn a_restart_remembers_the_day_s_answer() {
        let dir = crate::tempdir::Dir::new();
        assert_eq!(Asked::load(dir.path()), Asked::default());
        let asked = Asked {
            at: 1_000_000,
            answered: Some(1_000_000),
            latest: Some("0.2.0".to_owned()),
        };
        asked.save(dir.path()).unwrap();
        let loaded = Asked::load(dir.path());
        assert_eq!(loaded, asked);
        assert!(!loaded.due(1_000_000 + 60));
    }

    #[test]
    fn a_file_kept_before_answers_were_timed_does_not_say_github_was_out_of_reach() {
        let dir = crate::tempdir::Dir::new();
        std::fs::write(
            dir.path().join(FILE),
            r#"{ "at": 1000000, "latest": "0.2.0" }"#,
        )
        .unwrap();
        let loaded = Asked::load(dir.path());
        assert_eq!(
            loaded,
            Asked {
                at: 1_000_000,
                answered: None,
                latest: Some("0.2.0".to_owned()),
            }
        );
        assert!(
            !loaded.due(1_000_000 + 60),
            "the last question still counts"
        );
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
