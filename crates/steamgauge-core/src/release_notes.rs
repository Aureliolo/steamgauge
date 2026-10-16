//! What each release of `SteamGauge` changed in what people download, read from its notes.
//!
//! The release workflow writes each release's notes with a section listing the pull requests
//! whose files reach the download (`tools/release/notes.mjs`), each by a title that says what is
//! true after it. That section is what a person deciding whether to update needs; the rest
//! changes nothing they run. GitHub's releases API gives many releases' notes in one answer,
//! without a token, at sixty questions an hour per address, so it is asked only when a page
//! shows the list.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, newer_version::version};

/// The releases API for this repository, newest first.
pub const RELEASES_API: &str = "https://api.github.com/repos/Aureliolo/steamgauge/releases";

/// The heading `tools/release/notes.mjs` gives the changes that reach the download.
const SECTION: &str = "### In what you download";

/// The most releases one page holds; a shorter page is the last.
const PER_PAGE: usize = 100;

/// Pages asked at most. Far more releases than an update ever spans, and a bound on the sixty an
/// hour the address is allowed.
const PAGES: usize = 5;

/// The largest page read. A page of a hundred releases is a few hundred kilobytes; this only
/// stops a server that never stops sending.
const MOST: usize = 8 << 20;

const PATIENCE: Duration = Duration::from_secs(20);

/// One release and what it changed in what people download, oldest change first.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Changed {
    pub version: String,
    pub changes: Vec<String>,
}

#[derive(Deserialize)]
struct Listed {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Every published release newer than `from` and no newer than `to`, newest first, asked of
/// `api`, which is [`RELEASES_API`].
///
/// # Errors
///
/// Fails where GitHub cannot be reached, does not answer with a list of releases, or answers
/// with more than [`MOST`] bytes.
pub async fn between(
    api: &str,
    from: &semver::Version,
    to: &semver::Version,
) -> Result<Vec<Changed>> {
    let client = crate::http::builder()
        .user_agent(concat!("steamgauge/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(PATIENCE)
        .timeout(PATIENCE)
        .build()?;
    let mut found: Vec<(semver::Version, Changed)> = Vec::new();
    for page in 1..=PAGES {
        let url = format!("{api}?per_page={PER_PAGE}&page={page}");
        let mut response = crate::http::send(|| {
            client
                .get(&url)
                .header(reqwest::header::ACCEPT, "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
        })
        .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Error::Refused(
                if status == reqwest::StatusCode::FORBIDDEN
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                {
                    "GitHub answers this address sixty such questions an hour, and they are used \
                     up; it answers again within the hour"
                        .to_owned()
                } else {
                    format!("GitHub answered {status}")
                },
            ));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            body.extend_from_slice(&chunk);
            if body.len() > MOST {
                return Err(Error::Refused(
                    "GitHub's list of releases ran past its limit".to_owned(),
                ));
            }
        }
        let listed: Vec<Listed> = serde_json::from_slice(&body)?;
        let mut reached_from = false;
        for release in &listed {
            let Some(at) = version(&release.tag_name) else {
                continue;
            };
            if at <= *from {
                reached_from = true;
            }
            if release.draft || release.prerelease || at <= *from || at > *to {
                continue;
            }
            let changes = changes_in(release.body.as_deref().unwrap_or_default());
            found.push((
                at.clone(),
                Changed {
                    version: at.to_string(),
                    changes,
                },
            ));
        }
        // Releases come newest first, so one at or below `from` means the rest are older still.
        if reached_from || listed.len() < PER_PAGE {
            break;
        }
    }
    found.sort_by(|(left, _), (right, _)| right.cmp(left));
    Ok(found.into_iter().map(|(_, changed)| changed).collect())
}

/// The titles listed under [`SECTION`], without the author and the link each line ends with.
fn changes_in(notes: &str) -> Vec<String> {
    let mut lines = notes.lines().map(str::trim);
    if !lines.any(|line| line == SECTION) {
        return Vec::new();
    }
    lines
        .take_while(|line| !line.starts_with('#') && !line.starts_with("**"))
        .filter_map(|line| line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")))
        .map(title_of)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
        .collect()
}

fn title_of(entry: &str) -> &str {
    let entry = entry.rfind(" in https://").map_or(entry, |at| &entry[..at]);
    entry.rfind(" by @").map_or(entry, |at| &entry[..at]).trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in;

    const NOTES: &str = "## Download\r\n\r\nTake the installer.\r\n\r\n## What's Changed\r\n\r\n\
        ### In what you download\r\n\r\n\
        * About lists what each newer version changes by @Aureliolo in https://github.com/Aureliolo/steamgauge/pull/311\r\n\
        * A change merged without a pull request in https://github.com/Aureliolo/steamgauge/commit/abc\r\n\r\n\
        ### Repository only: training, reference data, tools, CI and docs\r\n\r\n\
        * CI runs on the newest runner by @Aureliolo in https://github.com/Aureliolo/steamgauge/pull/312\r\n\r\n\
        **Full Changelog**: https://github.com/Aureliolo/steamgauge/compare/v0.2.11...v0.2.12\r\n";

    fn v(text: &str) -> semver::Version {
        version(text).unwrap()
    }

    #[test]
    fn the_api_asked_is_this_repository_s() {
        assert_eq!(
            RELEASES_API,
            format!(
                "https://api.github.com/repos/{}/releases",
                env!("CARGO_PKG_REPOSITORY")
                    .strip_prefix("https://github.com/")
                    .unwrap()
            )
        );
    }

    #[test]
    fn only_the_changes_that_reach_the_download_are_taken_without_author_or_link() {
        assert_eq!(
            changes_in(NOTES),
            [
                "About lists what each newer version changes",
                "A change merged without a pull request",
            ]
        );
    }

    #[test]
    fn notes_without_the_section_changed_nothing_downloaded() {
        assert_eq!(
            changes_in("## What's Changed\n\nThe first release.\n"),
            Vec::<String>::new()
        );
        assert_eq!(changes_in(""), Vec::<String>::new());
    }

    fn listed(tag: &str, body: &str) -> serde_json::Value {
        serde_json::json!({ "tag_name": tag, "body": body, "draft": false, "prerelease": false })
    }

    #[tokio::test]
    async fn releases_after_one_version_up_to_another_are_given_newest_first() {
        let page = serde_json::json!([
            listed("v0.2.14", NOTES),
            listed("v0.2.13", "nothing"),
            { "tag_name": "v0.2.13-rc.1", "body": NOTES, "draft": false, "prerelease": true },
            { "tag_name": "v0.2.15", "body": NOTES, "draft": true, "prerelease": false },
            listed("v0.2.12", NOTES),
            listed("v0.2.11", NOTES),
            listed("v0.2.10", NOTES),
        ]);
        let server = stand_in::Server::new(move |_| stand_in::Answer::json(&page));
        let got = between(&server.origin(), &v("0.2.11"), &v("0.2.13"))
            .await
            .unwrap();
        assert_eq!(
            got.iter()
                .map(|changed| (changed.version.as_str(), changed.changes.len()))
                .collect::<Vec<_>>(),
            [("0.2.13", 0), ("0.2.12", 2)]
        );
        let asked = server.asked();
        assert_eq!(
            asked.len(),
            1,
            "the page reached 0.2.11, so no other was asked"
        );
        assert_eq!(asked[0].param("per_page"), Some("100"));
        assert_eq!(asked[0].header("x-github-api-version"), Some("2022-11-28"));
    }

    #[tokio::test]
    async fn the_next_page_is_asked_only_while_a_full_page_stays_above_the_older_version() {
        let full: Vec<serde_json::Value> = (0..PER_PAGE)
            .map(|patch| listed(&format!("v1.0.{patch}"), NOTES))
            .collect();
        let second = serde_json::json!([listed("v0.9.0", NOTES), listed("v0.1.0", NOTES)]);
        let server = stand_in::Server::new(move |asked| match asked.param("page") {
            Some("1") => stand_in::Answer::json(&serde_json::Value::from(full.clone())),
            _ => stand_in::Answer::json(&second),
        });
        let got = between(&server.origin(), &v("0.1.0"), &v("2.0.0"))
            .await
            .unwrap();
        assert_eq!(got.len(), PER_PAGE + 1);
        assert_eq!(
            got.last().map(|changed| changed.version.as_str()),
            Some("0.9.0")
        );
        assert_eq!(server.asked().len(), 2);
    }

    #[tokio::test]
    async fn a_short_page_is_the_last_even_above_the_older_version() {
        let page = serde_json::json!([listed("v0.2.13", NOTES), listed("v0.2.12", NOTES)]);
        let server = stand_in::Server::new(move |_| stand_in::Answer::json(&page));
        let got = between(&server.origin(), &v("0.2.0"), &v("0.2.13"))
            .await
            .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(server.asked().len(), 1);
    }

    async fn refusal_for(status: u16) -> String {
        let server = stand_in::Server::new(move |_| stand_in::Answer::status(status));
        between(&server.origin(), &v("0.2.10"), &v("0.2.11"))
            .await
            .unwrap_err()
            .to_string()
    }

    #[tokio::test]
    async fn a_used_up_limit_is_said_as_such_and_any_other_refusal_by_its_status() {
        for limited in [403, 429] {
            let refusal = refusal_for(limited).await;
            assert!(
                refusal.contains("sixty such questions an hour"),
                "{limited}: {refusal}"
            );
        }
        let refusal = refusal_for(500).await;
        assert!(refusal.contains("GitHub answered 500"), "{refusal}");
    }

    /// A list of no releases, `[` and `]` around enough spaces to come to `length` bytes.
    fn padded(length: usize) -> Vec<u8> {
        let mut body = vec![b' '; length];
        body[0] = b'[';
        body[length - 1] = b']';
        body
    }

    #[tokio::test]
    async fn a_list_of_exactly_its_limit_is_read() {
        let server = stand_in::Server::new(|_| stand_in::Answer::body(padded(MOST)));
        assert_eq!(
            between(&server.origin(), &v("0.2.10"), &v("0.2.11"))
                .await
                .unwrap(),
            []
        );
    }

    #[tokio::test]
    async fn a_list_past_its_limit_is_not_read() {
        let server = stand_in::Server::new(|_| stand_in::Answer::body(padded(MOST + 2)));
        let refusal = between(&server.origin(), &v("0.2.10"), &v("0.2.11"))
            .await
            .unwrap_err()
            .to_string();
        assert!(refusal.contains("ran past its limit"), "{refusal}");
    }
}
