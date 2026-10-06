//! Fetching a release's file, its build provenance, and the root Sigstore's signatures are
//! checked against. Nothing fetched here is trusted for where it came from: the file is trusted
//! once [`super::verify`] says so, and the provenance and the root carry their own signatures.

use std::{io::Write, path::Path, time::Duration};

use sha2::{Digest, Sha256};
use sigstore_verify::trust_root::{TrustedRoot, TufBootstrap, TufConfig};

use super::verify::{Refusal, embedded_root};

/// Where a release's files and its attestations are asked for: GitHub everywhere but a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origins {
    /// Release files, as `<downloads>/v<version>/<name>`.
    pub downloads: String,
    /// The attestations API, as `<attestations>/sha256:<digest>`.
    pub attestations: String,
    /// Sigstore's TUF repository, which serves the current trusted root.
    pub tuf: String,
}

impl Origins {
    #[must_use]
    pub fn github() -> Self {
        Self {
            downloads: concat!(env!("CARGO_PKG_REPOSITORY"), "/releases/download").to_owned(),
            attestations: "https://api.github.com/repos/Aureliolo/steamgauge/attestations"
                .to_owned(),
            tuf: "https://tuf-repo-cdn.sigstore.dev".to_owned(),
        }
    }

    #[must_use]
    pub fn file(&self, version: &semver::Version, name: &str) -> String {
        format!("{}/v{version}/{name}", self.downloads)
    }
}

/// The release asset holding the provenance alone, beside the files it covers.
#[must_use]
pub fn provenance_name(version: &semver::Version) -> String {
    format!("steamgauge-{version}.provenance.sigstore.json")
}

/// The largest file an update downloads. The biggest installer is tens of megabytes; this only
/// stops a server that never stops sending from filling the disk.
pub const MOST: u64 = 1 << 30;

/// The largest provenance read: a bundle is about thirteen kilobytes, and the attestations API's
/// answer wraps a few of them.
const MOST_PROVENANCE: u64 = 4 << 20;

/// Long enough for a slow line to make progress, short enough that a dead one is given up.
const PATIENCE: Duration = Duration::from_secs(60);

/// A client for everything this module asks, following redirects only to HTTPS: GitHub serves
/// release files from another host it redirects to.
///
/// # Errors
///
/// Fails where no TLS client can be built.
pub fn client() -> reqwest::Result<reqwest::Client> {
    crate::http::builder()
        .user_agent(concat!("steamgauge/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            match hop(attempt.previous().len(), attempt.url().scheme()) {
                Hop::Follow => attempt.follow(),
                Hop::Stop => attempt.stop(),
                Hop::TooMany => attempt.error("too many redirects"),
            }
        }))
        .connect_timeout(PATIENCE)
        .read_timeout(PATIENCE)
        .build()
}

/// What is done with a redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hop {
    Follow,
    /// The redirect itself is the answer, which a download takes for a failure.
    Stop,
    TooMany,
}

/// The redirect after `previous` others, to an address of `scheme`: followed to HTTPS, ten at
/// most, as a browser would.
fn hop(previous: usize, scheme: &str) -> Hop {
    if previous >= 10 {
        Hop::TooMany
    } else if scheme == "https" {
        Hop::Follow
    } else {
        Hop::Stop
    }
}

/// What a download wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub bytes: u64,
    /// Lower-case hex, taken over the bytes as they were written.
    pub sha256: String,
}

/// Why a download did not finish.
#[derive(Debug, thiserror::Error)]
pub enum Failed {
    #[error("{0}")]
    Http(#[from] reqwest::Error),
    #[error("the file could not be written: {0}")]
    Disk(#[from] std::io::Error),
    #[error("the server sent more than {0} bytes")]
    TooLong(u64),
    #[error("the server said {0}")]
    Status(reqwest::StatusCode),
}

/// Downloads `url` into `into`, hashing every byte as it is written, and tells `progress` how
/// many bytes have arrived of how many the server announced.
///
/// # Errors
///
/// Fails on a status other than success, on transport and write failures, and once more than
/// `most` bytes have arrived.
pub async fn download(
    http: &reqwest::Client,
    url: &str,
    into: &mut impl Write,
    most: u64,
    progress: &mut impl FnMut(u64, Option<u64>),
) -> Result<Fetched, Failed> {
    let mut response = http.get(url).send().await?;
    if !response.status().is_success() {
        return Err(Failed::Status(response.status()));
    }
    let total = response.content_length();
    if total.is_some_and(|total| total > most) {
        return Err(Failed::TooLong(most));
    }
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    progress(0, total);
    while let Some(chunk) = response.chunk().await? {
        bytes += chunk.len() as u64;
        if bytes > most {
            return Err(Failed::TooLong(most));
        }
        hasher.update(&chunk);
        into.write_all(&chunk)?;
        progress(bytes, total);
    }
    into.flush()?;
    Ok(Fetched {
        bytes,
        sha256: hex(&hasher.finalize()),
    })
}

/// The SHA-256 of everything `from` holds, in lower-case hex.
///
/// # Errors
///
/// Fails where `from` cannot be read.
pub fn sha256_of(from: &mut impl std::io::Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 16];
    loop {
        let read = from.read(&mut buffer)?;
        if read == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            use std::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Every bundle the attestations API's answer carries in full. One it gives only as a link to
/// a compressed blob is passed over: the release asset is where a bundle is read from, and the
/// API is only asked where that is missing.
///
/// # Errors
///
/// Fails on an answer that is not the API's JSON.
pub fn bundles_in_answer(answer: &[u8]) -> serde_json::Result<Vec<String>> {
    #[derive(serde::Deserialize)]
    struct Answer {
        #[serde(default)]
        attestations: Vec<Attestation>,
    }
    #[derive(serde::Deserialize)]
    struct Attestation {
        bundle: Option<serde_json::Value>,
    }
    let answer: Answer = serde_json::from_slice(answer)?;
    answer
        .attestations
        .into_iter()
        .filter_map(|attestation| attestation.bundle)
        .map(|bundle| serde_json::to_string(&bundle))
        .collect()
}

/// A small answer whole, or nothing where the request fails, is refused, or runs past
/// [`MOST_PROVENANCE`].
async fn small(request: reqwest::RequestBuilder) -> Option<Vec<u8>> {
    let mut response = request.send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        body.extend_from_slice(&chunk);
        if body.len() as u64 > MOST_PROVENANCE {
            return None;
        }
    }
    Some(body)
}

/// The provenance bundles that may cover the file with digest `sha256` from release `version`:
/// the one the release publishes beside its files, and only where that cannot be had, the
/// ones GitHub's attestations API keeps for the digest, which allows sixty unauthenticated
/// questions an hour per address.
///
/// # Errors
///
/// Returns [`Refusal::NoProvenance`] when neither gives a bundle.
pub async fn provenance(
    http: &reqwest::Client,
    origins: &Origins,
    version: &semver::Version,
    sha256: &str,
) -> Result<Vec<String>, Refusal> {
    let asset = origins.file(version, &provenance_name(version));
    if let Some(bundle) = small(http.get(asset)).await
        && let Ok(bundle) = String::from_utf8(bundle)
    {
        return Ok(vec![bundle]);
    }
    let api = format!(
        "{}/sha256:{sha256}?predicate_type=provenance",
        origins.attestations
    );
    let request = http
        .get(api)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    small(request)
        .await
        .and_then(|answer| bundles_in_answer(&answer).ok())
        .filter(|bundles| !bundles.is_empty())
        .ok_or(Refusal::NoProvenance)
}

/// The trusted root for checking a signature: Sigstore's current one through TUF, which starts
/// from the TUF root embedded in this build and keeps what it verified in `cache`; else the
/// copy in `cache` verified again offline; else the trusted root embedded in this build.
///
/// # Errors
///
/// Fails only if the embedded root does not parse.
pub async fn trusted_root(
    http: &reqwest::Client,
    origins: &Origins,
    cache: &Path,
) -> Result<TrustedRoot, Refusal> {
    let bootstrap = || {
        TufConfig::custom(
            origins.tuf.clone(),
            TufBootstrap::trusted(
                sigstore_verify::trust_root::SigstoreInstance::PublicGood.tuf_root(),
            ),
        )
        .with_cache_dir(cache.to_path_buf())
    };
    if let Ok(root) = TrustedRoot::from_tuf(bootstrap().with_http_client(http.clone())).await {
        return Ok(root);
    }
    if let Ok(root) = TrustedRoot::from_tuf(bootstrap().offline()).await {
        return Ok(root);
    }
    embedded_root()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in;

    fn version() -> semver::Version {
        semver::Version::new(0, 2, 0)
    }

    fn origins(server: &stand_in::Server) -> Origins {
        Origins {
            downloads: format!("{}/releases/download", server.origin()),
            attestations: format!("{}/attestations", server.origin()),
            tuf: format!("{}/tuf", server.origin()),
        }
    }

    #[test]
    fn github_is_asked_for_this_repository_s_files_and_attestations() {
        let github = Origins::github();
        assert_eq!(
            github.file(&version(), "steamgauge-0.2.0-windows-x64-setup.exe"),
            "https://github.com/Aureliolo/steamgauge/releases/download/v0.2.0/steamgauge-0.2.0-windows-x64-setup.exe"
        );
        assert_eq!(
            github.attestations,
            format!(
                "https://api.github.com/repos/{}/attestations",
                env!("CARGO_PKG_REPOSITORY")
                    .strip_prefix("https://github.com/")
                    .unwrap()
            )
        );
        assert_eq!(
            provenance_name(&version()),
            "steamgauge-0.2.0.provenance.sigstore.json"
        );
    }

    #[tokio::test]
    async fn a_download_is_hashed_as_it_is_written_and_reports_its_progress() {
        let body = b"the setup program".repeat(1000);
        let sent = body.clone();
        let server = stand_in::Server::new(move |_| stand_in::Answer::body(sent.clone()));
        let mut written = Vec::new();
        let mut seen = Vec::new();
        let fetched = download(
            &client().unwrap(),
            &format!("{}/file", server.origin()),
            &mut written,
            MOST,
            &mut |done, total| seen.push((done, total)),
        )
        .await
        .unwrap();
        assert_eq!(written, body);
        assert_eq!(fetched.bytes, body.len() as u64);
        assert_eq!(fetched.sha256, sha256_of(&mut body.as_slice()).unwrap());
        assert_eq!(seen.first(), Some(&(0, Some(body.len() as u64))));
        assert_eq!(
            seen.last(),
            Some(&(body.len() as u64, Some(body.len() as u64)))
        );
    }

    #[test]
    fn redirects_are_followed_to_https_alone_and_ten_at_most() {
        assert_eq!(hop(0, "https"), Hop::Follow);
        assert_eq!(hop(9, "https"), Hop::Follow);
        assert_eq!(hop(10, "https"), Hop::TooMany);
        assert_eq!(hop(0, "http"), Hop::Stop);
        assert_eq!(hop(10, "http"), Hop::TooMany);
    }

    #[tokio::test]
    async fn a_provenance_file_of_exactly_its_limit_is_read() {
        let limit = usize::try_from(MOST_PROVENANCE).unwrap();
        let server = stand_in::Server::new(move |_| stand_in::Answer::body(vec![b' '; limit]));
        let bundles = provenance(&client().unwrap(), &origins(&server), &version(), "ab12")
            .await
            .unwrap();
        assert_eq!(bundles.iter().map(String::len).collect::<Vec<_>>(), [limit]);
    }

    #[test]
    fn a_digest_is_lower_case_hex() {
        assert_eq!(
            sha256_of(&mut &b"abc"[..]).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn a_download_longer_than_the_limit_is_cut_off() {
        let server = stand_in::Server::new(|_| stand_in::Answer::body(vec![7_u8; 5000]));
        let mut written = Vec::new();
        let failed = download(
            &client().unwrap(),
            &format!("{}/file", server.origin()),
            &mut written,
            4999,
            &mut |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(failed, Failed::TooLong(4999)), "{failed}");
        assert!(written.len() <= 4999);
    }

    #[tokio::test]
    async fn exactly_the_limit_is_still_a_download() {
        let server = stand_in::Server::new(|_| stand_in::Answer::body(vec![7_u8; 5000]));
        let mut written = Vec::new();
        let fetched = download(
            &client().unwrap(),
            &format!("{}/file", server.origin()),
            &mut written,
            5000,
            &mut |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(fetched.bytes, 5000);
    }

    #[tokio::test]
    async fn a_redirect_away_from_https_is_not_followed() {
        let server = stand_in::Server::new(|asked| {
            if asked.path() == "/file" {
                stand_in::Answer::status(302).header("Location", "http://127.0.0.1:9/elsewhere")
            } else {
                stand_in::Answer::body("followed")
            }
        });
        let mut written = Vec::new();
        let failed = download(
            &client().unwrap(),
            &format!("{}/file", server.origin()),
            &mut written,
            MOST,
            &mut |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(
            matches!(failed, Failed::Status(status) if status.as_u16() == 302),
            "{failed}"
        );
        assert_eq!(written, b"");
    }

    #[tokio::test]
    async fn a_missing_file_is_a_failed_download_and_writes_nothing() {
        let server = stand_in::Server::new(|_| stand_in::Answer::status(404));
        let mut written = Vec::new();
        let failed = download(
            &client().unwrap(),
            &format!("{}/file", server.origin()),
            &mut written,
            MOST,
            &mut |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(matches!(failed, Failed::Status(status) if status.as_u16() == 404));
        assert_eq!(written, b"");
    }

    #[tokio::test]
    async fn the_release_s_own_provenance_file_is_taken_and_the_api_is_not_asked() {
        let server = stand_in::Server::new(|asked| {
            if asked.path() == "/releases/download/v0.2.0/steamgauge-0.2.0.provenance.sigstore.json"
            {
                stand_in::Answer::body("{\"the\":\"bundle\"}")
            } else {
                stand_in::Answer::status(404)
            }
        });
        let bundles = provenance(&client().unwrap(), &origins(&server), &version(), "ab12")
            .await
            .unwrap();
        assert_eq!(bundles, vec!["{\"the\":\"bundle\"}".to_owned()]);
        assert_eq!(server.asked().len(), 1);
    }

    #[tokio::test]
    async fn without_the_release_s_file_the_api_is_asked_for_the_digest_s_provenance() {
        let answer = include_str!("provenance-0.1.3.json");
        let server = stand_in::Server::new(move |asked| {
            if asked.path() == "/attestations/sha256:ab12" {
                stand_in::Answer::body(answer)
            } else {
                stand_in::Answer::status(404)
            }
        });
        let bundles = provenance(&client().unwrap(), &origins(&server), &version(), "ab12")
            .await
            .unwrap();
        assert_eq!(bundles, bundles_in_answer(answer.as_bytes()).unwrap());
        let asked = server.asked();
        let api = asked
            .iter()
            .find(|request| request.path() == "/attestations/sha256:ab12")
            .unwrap();
        assert_eq!(api.param("predicate_type"), Some("provenance"));
        assert_eq!(api.header("x-github-api-version"), Some("2022-11-28"));
    }

    #[tokio::test]
    async fn no_bundle_anywhere_is_no_provenance() {
        let server = stand_in::Server::new(|asked| {
            if asked.path().starts_with("/attestations/") {
                stand_in::Answer::json(&serde_json::json!({
                    "attestations": [{ "bundle": null, "bundle_url": "https://elsewhere" }]
                }))
            } else {
                stand_in::Answer::status(404)
            }
        });
        assert_eq!(
            provenance(&client().unwrap(), &origins(&server), &version(), "ab12").await,
            Err(Refusal::NoProvenance)
        );
    }

    #[tokio::test]
    async fn a_provenance_file_past_its_limit_is_not_read() {
        let server = stand_in::Server::new(|_| {
            stand_in::Answer::body(vec![b' '; usize::try_from(MOST_PROVENANCE).unwrap() + 1])
        });
        assert_eq!(
            provenance(&client().unwrap(), &origins(&server), &version(), "ab12").await,
            Err(Refusal::NoProvenance)
        );
    }

    #[test]
    fn the_api_s_answer_gives_each_bundle_it_carries_in_full() {
        let answer = br#"{"attestations": [
            {"bundle": {"mediaType": "one"}, "bundle_url": null},
            {"bundle": null, "bundle_url": "https://elsewhere"},
            {"bundle": {"mediaType": "two"}}
        ]}"#;
        assert_eq!(
            bundles_in_answer(answer).unwrap(),
            vec![
                r#"{"mediaType":"one"}"#.to_owned(),
                r#"{"mediaType":"two"}"#.to_owned()
            ]
        );
        assert!(bundles_in_answer(b"not json").is_err());
    }

    #[tokio::test]
    async fn with_no_tuf_repository_and_nothing_cached_the_embedded_root_is_used() {
        let server = stand_in::Server::new(|_| stand_in::Answer::status(404));
        let cache = crate::tempdir::Dir::new();
        let root = trusted_root(&client().unwrap(), &origins(&server), cache.path())
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&root).unwrap(),
            serde_json::to_value(embedded_root().unwrap()).unwrap()
        );
        assert!(
            server
                .asked()
                .iter()
                .any(|asked| asked.path().starts_with("/tuf/")),
            "the TUF repository is asked first"
        );
    }
}
