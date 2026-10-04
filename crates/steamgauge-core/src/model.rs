//! Fetching, verifying and loading the local embedding model.
//!
//! The model is downloaded on first use rather than bundled, so the installer stays small,
//! and every file is checked against a pinned SHA-256 before it is loaded. A model that
//! silently differs from the pinned one would change every number the tool reports without
//! anything appearing to go wrong.

use std::{
    io::Read,
    path::{Path, PathBuf},
};

use ort::session::Session;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Tokens a review is cut to. Every encoder here allows at least this many, and several
/// allow far more; they are held to the same limit so a comparison between them is about
/// the encoder rather than about how much text each was allowed to read. Truncation affects
/// roughly the top 1% of reviews by length.
pub const MAX_TOKENS: usize = 512;

/// Which encoder turns a review into a vector.
///
/// Multilingual by necessity: 57% of a typical Steam corpus is not English, so an
/// English-only encoder would silently discard most of it. Every option is permissively
/// licensed and published as a single-file ONNX graph, because the tool fetches and runs the
/// graph itself rather than shipping a Python stack to do it.
///
/// A corpus records which encoder produced it, so vectors from two encoders can never be
/// silently compared. Changing this means re-embedding.
///
/// These encoders no longer decide any subject. What reads a claim is the trained model in
/// [`crate::reader`]; embeddings remain for the passes that need a vector rather than a
/// judgement, which is deduplication and neighbour search.
///
/// All four were compared on 2026-09-08 under the prototype classifier, which has since been
/// deleted for reasons that had nothing to do with which encoder it ran: gte-base won by 5.5
/// points on a game it had never seen, and it is the only candidate whose margin survived a
/// paired test. The comparison stands as a ranking of these four encoders, and the absolute
/// figures it was made from describe a method this tool no longer uses.
///
/// The e5 family remains for anyone who wants the smaller vectors or the first-party ONNX:
/// intfloat publish their own exports, while gte-base's graph is a third-party conversion of
/// Alibaba's weights. Every file is pinned by SHA-256 either way, so a conversion cannot
/// change under a corpus that was built from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoder {
    /// 118M parameters, 384 dimensions, MIT. The smallest and fastest.
    E5Small,
    /// 278M parameters, 768 dimensions, MIT.
    E5Base,
    /// Snowflake arctic-embed-m-v2.0. 305M parameters, 768 dimensions, Apache-2.0.
    ArcticMediumV2,
    /// Alibaba gte-multilingual-base, by way of its ONNX re-export. 305M parameters, 768
    /// dimensions, Apache-2.0, and the most accurate of the four.
    #[default]
    GteBase,
}

/// How a sequence of token vectors becomes the one vector that represents a review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pooling {
    /// The mean over real tokens, which is what the e5 family is trained to produce.
    Mean,
    /// The leading token's vector, which these encoders are trained to summarise into.
    Cls,
}

impl Encoder {
    /// The name this encoder is chosen by and cached under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::E5Small => "e5-small",
            Self::E5Base => "e5-base",
            Self::ArcticMediumV2 => "arctic-m-v2",
            Self::GteBase => "gte-base",
        }
    }

    /// The repository the graph and tokenizer come from, recorded with every corpus.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::E5Small => "intfloat/multilingual-e5-small",
            Self::E5Base => "intfloat/multilingual-e5-base",
            Self::ArcticMediumV2 => "Snowflake/snowflake-arctic-embed-m-v2.0",
            Self::GteBase => "onnx-community/gte-multilingual-base",
        }
    }

    /// The commit the pinned files are fetched from: the newest at which every one of them has
    /// the hash pinned below. A branch name would let whoever controls the repository decide
    /// what is downloaded; the hashes would refuse it, and the commit means nothing else is
    /// ever asked for.
    fn revision(self) -> &'static str {
        match self {
            Self::E5Small => "614241f622f53c4eeff9890bdc4f31cfecc418b3",
            Self::E5Base => "d128750597153bb5987e10b1c3493a34e5a4502a",
            Self::ArcticMediumV2 => "95c2741480856aa9666782eb4afe11959938017f",
            Self::GteBase => "2edbf5e672aab465f9ed4c154a8b61791c082c69",
        }
    }

    #[must_use]
    pub const fn dimensions(self) -> usize {
        match self {
            Self::E5Small => 384,
            Self::E5Base | Self::ArcticMediumV2 | Self::GteBase => 768,
        }
    }

    #[must_use]
    pub fn pooling(self) -> Pooling {
        match self {
            Self::E5Small | Self::E5Base => Pooling::Mean,
            Self::ArcticMediumV2 | Self::GteBase => Pooling::Cls,
        }
    }

    /// Text put in front of every review before tokenising.
    ///
    /// e5 models are trained with an instruction prefix and produce measurably worse vectors
    /// without one; their cards specify `query:` for symmetric similarity, which is what
    /// classification needs. The others prefix queries only, so comparing reviews to
    /// reviews means no prefix at all.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::E5Small | Self::E5Base => "query: ",
            Self::ArcticMediumV2 | Self::GteBase => "",
        }
    }

    fn graph(self, precision: Precision) -> Asset {
        match (self, precision) {
            (Self::E5Small, Precision::Float16) => Asset {
                remote: "onnx/model_O4.onnx",
                local: "model_fp16.onnx",
                sha256: "4654c156f3e4171abc9c716cdb771bf9116455d15ac1aab364aeeede0e3205b0",
                bytes: 235_052_531,
            },
            (Self::E5Small, Precision::Float32) => Asset {
                remote: "onnx/model.onnx",
                local: "model_fp32.onnx",
                sha256: "ca456c06b3a9505ddfd9131408916dd79290368331e7d76bb621f1cba6bc8665",
                bytes: 470_268_510,
            },
            (Self::E5Base, Precision::Float16) => Asset {
                remote: "onnx/model_O4.onnx",
                local: "model_fp16.onnx",
                sha256: "f60256a833caee5c75a3903e589116752ee016ca7bc16f9b96e4db09984c5703",
                bytes: 554_948_118,
            },
            (Self::E5Base, Precision::Float32) => Asset {
                remote: "onnx/model.onnx",
                local: "model_fp32.onnx",
                sha256: "84a4d426f7e87a6bf5bf195f0bae2c4a7d15f675b23ca96f42fab8326d7a77aa",
                bytes: 1_110_059_084,
            },
            (Self::ArcticMediumV2, Precision::Float16) => Asset {
                remote: "onnx/model_fp16.onnx",
                local: "model_fp16.onnx",
                sha256: "f27ab40ab6e230265ba49a202a37f1ad031556256cbbc105d0ca9c0bdc7ec42e",
                bytes: 613_266_244,
            },
            (Self::ArcticMediumV2, Precision::Float32) => Asset {
                remote: "onnx/model.onnx",
                local: "model_fp32.onnx",
                sha256: "c0c53d7f49a2db60761b92b7bbf5be87a7b3cf5d92dbbd7f1b5028bd5a40aa39",
                bytes: 1_226_099_913,
            },
            (Self::GteBase, Precision::Float16) => Asset {
                remote: "onnx/model_fp16.onnx",
                local: "model_fp16.onnx",
                sha256: "f1d0f4ec988a6c17387d3b256e631deea506a891aed3a6ded4f9bf09386cc38e",
                bytes: 627_988_827,
            },
            (Self::GteBase, Precision::Float32) => Asset {
                remote: "onnx/model.onnx",
                local: "model_fp32.onnx",
                sha256: "5b9f03fdc40350a78fa064b4cfb6bf9a229a7c40aa87736f537e3ebd00aa2b86",
                bytes: 1_255_502_649,
            },
        }
    }

    fn tokenizer(self) -> Asset {
        let (sha256, bytes) = match self {
            Self::E5Small => (
                "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
                17_082_730,
            ),
            Self::E5Base => (
                "62c24cdc13d4c9952d63718d6c9fa4c287974249e16b7ade6d5a85e7bbb75626",
                17_082_660,
            ),
            Self::ArcticMediumV2 => (
                "f1cc44ad7faaeec47241864835473fd5403f2da94673f3f764a77ebcb0a803ec",
                17_083_009,
            ),
            Self::GteBase => (
                "3a56def25aa40facc030ea8b0b87f3688e4b3c39eb8b45d5702b3a1300fe2a20",
                17_082_734,
            ),
        };
        Asset {
            remote: "tokenizer.json",
            local: "tokenizer.json",
            sha256,
            bytes,
        }
    }
}

/// One file pinned by hash and length: where it lives in a repository, what it is called here,
/// and what it must hash to before it is used.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Asset {
    pub(crate) remote: &'static str,
    pub(crate) local: &'static str,
    pub(crate) sha256: &'static str,
    /// The file's length. Known before a byte is fetched, so a download can say how far it has
    /// to go across every file it needs, and a server sending more than this is cut off.
    pub(crate) bytes: u64,
}

impl Asset {
    /// Whether the copy in `dir` is at least the right length. The hash is what decides; this
    /// is what a window can ask about without reading a gigabyte.
    pub(crate) fn present_in(self, dir: &Path) -> bool {
        std::fs::metadata(dir.join(self.local)).is_ok_and(|found| found.len() == self.bytes)
    }
}

/// What fetching these files into `dir` would download, counting only the ones not there.
pub(crate) fn bytes_left(assets: &[Asset], dir: &Path) -> u64 {
    assets
        .iter()
        .filter(|asset| !asset.present_in(dir))
        .map(|asset| asset.bytes)
        .sum()
}

/// Which build of the graph to run.
///
/// Measured on a 2,875-text corpus under e5-small rather than assumed, because the obvious
/// assumption was wrong. Quantising was expected to trade a little accuracy for speed and
/// size; it cost accuracy and bought no speed at all. Downloads scale with the encoder, but
/// fp32 is twice fp16 for all of them.
///
/// | build | `DirectML` | CPU | download | batch-invariant | mean cosine to fp32 |
/// |-------|-----------|-----|----------|-----------------|---------------------|
/// | int8  | 11.8s     | 92.4s  | 112 MB | **no**, 0.9969 | 0.9960 |
/// | fp16  | **4.9s**  | 115.0s | 224 MB | yes, 0.999999  | **0.999999** |
/// | fp32  | 4.9s      | **93.3s** | 448 MB | yes         | reference |
///
/// int8 was the slowest of the three on a GPU, where dynamic quantisation pays for its
/// scales on every layer while fp16 runs on hardware built for it, and no faster on this CPU
/// either. It was also the only build whose vectors depended on what a review was embedded
/// alongside: activation scales are taken per tensor, and a tensor spans the batch. Worse on
/// every axis but a 112 MB download, so it is not offered.
///
/// fp16 is the default: indistinguishable from the full graph, reproducible, the fastest
/// option where a GPU exists, and half the download of fp32. fp32 is for CPU-only runs,
/// where it is the faster of the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    /// Indistinguishable from the full graph, and the fastest where a GPU exists.
    #[default]
    Float16,
    /// The graph as exported. The reference fp16 is judged against, and faster on CPU.
    Float32,
}

impl Precision {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Float16 => "fp16",
            Self::Float32 => "fp32",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress<'a> {
    pub file: &'a str,
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// Default location for the model cache, shared across corpora.
#[must_use]
pub fn default_cache_dir() -> PathBuf {
    dirs_cache().join("steamgauge").join("models")
}

fn dirs_cache() -> PathBuf {
    // Deliberately dependency-free: the platform conventions are three env vars, and a
    // crate for that is not worth the supply-chain surface.
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(dir);
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if cfg!(target_os = "macos") {
            return home.join("Library").join("Caches");
        }
        return home.join(".cache");
    }
    PathBuf::from(".cache")
}

/// Downloads any missing or corrupt model file into `cache_dir`.
///
/// # Errors
///
/// Returns [`Error::ModelChecksum`] if a downloaded file does not match its pinned hash,
/// and propagates transport and filesystem failures.
pub async fn ensure(
    cache_dir: &Path,
    encoder: Encoder,
    precision: Precision,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<()> {
    let dir = encoder_dir(cache_dir, encoder);
    std::fs::create_dir_all(&dir)?;
    let http = client()?;

    for asset in [encoder.tokenizer(), encoder.graph(precision)] {
        let source = Source {
            hub: HUB,
            repository: encoder.id(),
            revision: encoder.revision(),
        };
        ensure_asset(&http, source, asset, &dir, &mut on_progress).await?;
    }
    Ok(())
}

/// The Hugging Face Hub, where every model this crate runs is published.
pub const HUB: &str = "https://huggingface.co";

/// Where pinned files are fetched from: a Hugging Face repository at one commit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Source {
    /// The Hub's origin, [`HUB`] everywhere but a test.
    pub(crate) hub: &'static str,
    pub(crate) repository: &'static str,
    pub(crate) revision: &'static str,
}

impl Source {
    /// Whether this names a repository and a full commit, the only revision that cannot move.
    pub(crate) fn is_pinned(self) -> bool {
        !self.repository.is_empty()
            && self.revision.len() == 40
            && self.revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    fn url(self, remote: &str) -> String {
        format!(
            "{}/{}/resolve/{}/{remote}",
            self.hub, self.repository, self.revision
        )
    }
}

/// Fetches one pinned file from a Hugging Face repository unless the copy on disk already
/// matches its hash.
///
/// # Errors
///
/// Returns [`Error::NoAnchors`] if the file has to be fetched and the source is not pinned to a
/// commit, [`Error::ModelChecksum`] if what was downloaded does not match, and propagates
/// transport and filesystem failures.
pub(crate) async fn ensure_asset(
    http: &reqwest::Client,
    source: Source,
    asset: Asset,
    dir: &Path,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    let path = dir.join(asset.local);
    if asset.present_in(dir) && sha256_file(&path)? == asset.sha256 {
        return Ok(());
    }
    if !source.is_pinned() {
        return Err(Error::NoAnchors {
            path: dir.to_path_buf(),
        });
    }
    download(http, &source.url(asset.remote), asset, &path, on_progress).await?;

    let actual = sha256_file(&path)?;
    if actual != asset.sha256 {
        // A file that fails verification is removed rather than left to be picked up by the
        // next run, which would otherwise skip the download and load it.
        std::fs::remove_file(&path)?;
        return Err(Error::ModelChecksum {
            file: asset.local,
            expected: asset.sha256,
            actual,
        });
    }
    Ok(())
}

/// The release a published repository has gone furthest to, `v2` past `v1`, from the list of
/// its tags on `hub`, which is [`HUB`]. Nothing is fetched from it: a newer release comes with
/// the build that pins it, and this only says one exists.
///
/// # Errors
///
/// Fails on transport failures and on a listing that is not the Hub's.
pub async fn newest_release(hub: &str, repository: &str) -> Result<Option<String>> {
    #[derive(serde::Deserialize)]
    struct Refs {
        tags: Vec<Ref>,
    }
    #[derive(serde::Deserialize)]
    struct Ref {
        name: String,
    }

    let refs: Refs = client()?
        .get(format!("{hub}/api/models/{repository}/refs"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(refs
        .tags
        .into_iter()
        .filter_map(|tag| release_number(&tag.name).map(|number| (number, tag.name)))
        .max_by_key(|(number, _)| *number)
        .map(|(_, name)| name))
}

/// The number of a release tag, `v1` and on; none for any other tag.
#[must_use]
pub fn release_number(tag: &str) -> Option<u32> {
    let digits = tag.strip_prefix('v')?;
    (!digits.starts_with('0') && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())
        .flatten()
}

pub(crate) fn client() -> Result<reqwest::Client> {
    Ok(crate::http::builder()
        .user_agent(concat!("steamgauge/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

async fn download(
    http: &reqwest::Client,
    url: &str,
    asset: Asset,
    path: &Path,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<()> {
    let mut response = http.get(url).send().await?.error_for_status()?;

    // Written beside the target and renamed, so an interrupted download is never mistaken
    // for a complete one on the next run.
    let partial = path.with_extension("partial");
    let mut file = std::fs::File::create(&partial)?;
    let mut downloaded = 0;

    while let Some(chunk) = response.chunk().await? {
        downloaded += chunk.len() as u64;
        if downloaded > asset.bytes {
            drop(file);
            std::fs::remove_file(&partial)?;
            return Err(Error::ModelLength {
                file: asset.local,
                expected: asset.bytes,
            });
        }
        std::io::Write::write_all(&mut file, &chunk)?;
        on_progress(DownloadProgress {
            file: asset.local,
            downloaded,
            total: Some(asset.bytes),
        });
    }
    drop(file);
    if downloaded != asset.bytes {
        std::fs::remove_file(&partial)?;
        return Err(Error::ModelLength {
            file: asset.local,
            expected: asset.bytes,
        });
    }
    std::fs::rename(&partial, path)?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// One directory per encoder, so two encoders' graphs never share a filename.
fn encoder_dir(cache_dir: &Path, encoder: Encoder) -> PathBuf {
    cache_dir.join(encoder.as_str())
}

#[must_use]
pub fn model_path(cache_dir: &Path, encoder: Encoder, precision: Precision) -> PathBuf {
    encoder_dir(cache_dir, encoder).join(encoder.graph(precision).local)
}

#[must_use]
pub fn tokenizer_path(cache_dir: &Path, encoder: Encoder) -> PathBuf {
    encoder_dir(cache_dir, encoder).join("tokenizer.json")
}

/// Whether this build carries a backend that can put a model on a graphics card at all. Where
/// it does not, a card's memory says nothing about which reader will run well.
pub const REACHES_A_CARD: bool = cfg!(any(
    feature = "cuda",
    feature = "directml",
    feature = "coreml"
));

/// Builds a session on the fastest backend this build supports and the machine provides.
///
/// GPU backends are opt-in at build time so the default binary needs no vendor runtime to
/// start. Each is *attempted* rather than assumed, because a build that supports `DirectML`
/// still has to run on machines with no suitable adapter. Which backend runs changes only
/// where the arithmetic happens, never which model runs, so results stay comparable.
///
/// # Errors
///
/// Fails if no backend, including the CPU fallback, can load the model.
pub fn session(
    cache_dir: &Path,
    encoder: Encoder,
    precision: Precision,
) -> Result<(Session, &'static str)> {
    session_at(&model_path(cache_dir, encoder, precision))
}

/// The same, for a graph this project trained rather than downloaded.
///
/// CUDA is tried first where the build has it. `DirectML` reaches every vendor and is the
/// right default breadth, but on a machine that has both it is the slower way to use the same
/// card, and the difference is hours over a corpus of millions of claims.
///
/// # Errors
///
/// Fails if no backend, including the CPU fallback, can load the graph.
pub fn session_at(path: &Path) -> Result<(Session, &'static str)> {
    #[cfg(feature = "cuda")]
    if let Ok(session) = try_session(path, ort::ep::CUDA::default().build()) {
        return Ok((session, "cuda"));
    }
    // High performance, not the default order: on a laptop with two graphics adapters the
    // default is the one driving the display, which is the integrated one, and a reading meant
    // for the card would run on a fraction of it while still reporting "directml".
    #[cfg(feature = "directml")]
    if let Ok(session) = try_session(
        path,
        ort::ep::DirectML::default()
            .with_performance_preference(ort::ep::directml::PerformancePreference::HighPerformance)
            .build(),
    ) {
        return Ok((session, "directml"));
    }
    #[cfg(feature = "coreml")]
    if let Ok(session) = try_session(path, ort::ep::CoreML::default().build()) {
        return Ok((session, "coreml"));
    }

    Ok((try_session(path, ort::ep::CPU::default().build())?, "cpu"))
}

/// Loads the graph on exactly one backend, or admits it could not.
///
/// `error_on_failure` is the whole point. Left at its default, a provider that cannot
/// initialise is logged and skipped, the session builds on the CPU anyway, and the caller is
/// handed a working session it will describe as running on a card. That is how this tool spent
/// a fortnight reporting "cuda" on a machine with no CUDA runtime installed, at fifty claims a
/// second, while nvidia-smi showed it was never on the GPU at all. A backend that did not load
/// has to say so here, so that the fallback below it is a decision rather than an accident.
fn try_session(path: &Path, provider: ort::ep::ExecutionProviderDispatch) -> Result<Session> {
    let mut builder = Session::builder()?
        .with_execution_providers([provider.error_on_failure()])
        .map_err(ort::Error::from)?;
    Ok(builder.commit_from_file(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in;

    const EVERY_ENCODER: [Encoder; 4] = [
        Encoder::E5Small,
        Encoder::E5Base,
        Encoder::ArcticMediumV2,
        Encoder::GteBase,
    ];

    #[test]
    fn every_pinned_hash_is_a_sha256() {
        for encoder in EVERY_ENCODER {
            for asset in [
                encoder.tokenizer(),
                encoder.graph(Precision::Float16),
                encoder.graph(Precision::Float32),
            ] {
                assert_eq!(asset.sha256.len(), 64, "{} {}", encoder.id(), asset.local);
                assert!(
                    asset.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                    "{} {}",
                    encoder.id(),
                    asset.local
                );
            }
        }
    }

    #[test]
    fn every_encoder_is_fetched_from_a_commit_and_not_a_branch() {
        for encoder in EVERY_ENCODER {
            let source = Source {
                hub: HUB,
                repository: encoder.id(),
                revision: encoder.revision(),
            };
            assert!(source.is_pinned(), "{}", encoder.id());
            assert!(
                source
                    .url("tokenizer.json")
                    .ends_with(&format!("/resolve/{}/tokenizer.json", encoder.revision())),
                "{}",
                encoder.id()
            );
        }
    }

    #[test]
    fn a_source_without_a_full_commit_is_not_pinned() {
        for revision in ["", "main", "v1", "614241f", &"g".repeat(40)] {
            let source = Source {
                hub: HUB,
                repository: "intfloat/multilingual-e5-small",
                revision: revision.to_owned().leak(),
            };
            assert!(!source.is_pinned(), "{revision:?}");
        }
        assert!(
            !Source {
                hub: HUB,
                repository: "",
                revision: "614241f622f53c4eeff9890bdc4f31cfecc418b3",
            }
            .is_pinned()
        );
    }

    #[test]
    fn only_release_tags_are_numbered_and_they_number_as_they_read() {
        assert_eq!(release_number("v1"), Some(1));
        assert_eq!(release_number("v12"), Some(12));
        for other in ["main", "v", "v0", "v01", "1", "v1.2", "release"] {
            assert_eq!(release_number(other), None, "{other}");
        }
    }

    #[test]
    fn only_files_missing_or_at_another_length_count_towards_a_download() {
        let dir = crate::tempdir::Dir::new();
        let pinned = |local: &'static str, bytes: u64| Asset {
            remote: local,
            local,
            sha256: "",
            bytes,
        };
        std::fs::write(dir.path().join("here.json"), [0_u8; 4]).unwrap();
        std::fs::write(dir.path().join("short.onnx"), [0_u8; 3]).unwrap();
        let assets = [
            pinned("here.json", 4),
            pinned("short.onnx", 10),
            pinned("missing.onnx", 100),
        ];
        assert_eq!(bytes_left(&assets, dir.path()), 110);
    }

    #[test]
    fn a_file_is_never_fetched_from_a_source_that_is_not_pinned() {
        let dir = crate::tempdir::Dir::new();
        let asset = Asset {
            remote: "tokenizer.json",
            local: "tokenizer.json",
            sha256: "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
            bytes: 17_082_730,
        };
        let source = Source {
            hub: HUB,
            repository: "intfloat/multilingual-e5-small",
            revision: "main",
        };
        let fetched = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(ensure_asset(
                &client().unwrap(),
                source,
                asset,
                dir.path(),
                &mut |_| {},
            ));
        assert!(
            matches!(fetched, Err(Error::NoAnchors { .. })),
            "{fetched:?}"
        );
        assert!(!dir.path().join("tokenizer.json").exists());
    }

    #[test]
    fn no_two_encoders_share_a_name_a_repository_or_a_tokenizer() {
        for (slot, one) in EVERY_ENCODER.iter().enumerate() {
            for other in &EVERY_ENCODER[slot + 1..] {
                assert_ne!(one.as_str(), other.as_str());
                assert_ne!(one.id(), other.id());
                assert_ne!(
                    one.tokenizer().sha256,
                    other.tokenizer().sha256,
                    "{} and {} would share a cached tokenizer",
                    one.id(),
                    other.id()
                );
            }
        }
    }

    #[test]
    fn each_encoder_caches_under_its_own_directory() {
        let root = Path::new("cache");
        for encoder in EVERY_ENCODER {
            let path = model_path(root, encoder, Precision::Float16);
            assert!(path.starts_with(root.join(encoder.as_str())), "{path:?}");
            assert!(
                tokenizer_path(root, encoder).starts_with(root.join(encoder.as_str())),
                "{encoder:?}"
            );
        }
    }

    const COMMIT: &str = "614241f622f53c4eeff9890bdc4f31cfecc418b3";
    const GRAPH: &[u8] = b"graph bytes";

    /// A file pinned to what `content` hashes to and how long it is.
    fn pinned_to(content: &[u8]) -> Asset {
        Asset {
            remote: "onnx/model.onnx",
            local: "model.onnx",
            sha256: hex(&Sha256::digest(content)).leak(),
            bytes: content.len() as u64,
        }
    }

    /// A stand-in Hub serving `content` for every file asked of it.
    fn hub(content: &'static [u8]) -> (stand_in::Server, Source) {
        let server = stand_in::Server::new(move |_| stand_in::Answer::body(content));
        let source = Source {
            hub: server.origin().leak(),
            repository: "someone/encoder",
            revision: COMMIT,
        };
        (server, source)
    }

    /// Fetches `asset` into `dir`, with every progress report as (downloaded, total).
    async fn fetch(
        source: Source,
        asset: Asset,
        dir: &Path,
    ) -> (Result<()>, Vec<(u64, Option<u64>)>) {
        let mut reports = Vec::new();
        let fetched = ensure_asset(&client().unwrap(), source, asset, dir, &mut |progress| {
            reports.push((progress.downloaded, progress.total));
        })
        .await;
        (fetched, reports)
    }

    #[tokio::test]
    async fn a_file_is_fetched_from_its_commit_and_kept_once_it_matches_its_pin() {
        let dir = crate::tempdir::Dir::new();
        let (server, source) = hub(GRAPH);
        let (fetched, reports) = fetch(source, pinned_to(GRAPH), dir.path()).await;
        fetched.unwrap();
        assert_eq!(std::fs::read(dir.path().join("model.onnx")).unwrap(), GRAPH);
        assert!(!dir.path().join("model.partial").exists());
        assert_eq!(reports.last(), Some(&(11, Some(11))));
        let asked = &server.asked()[0];
        assert_eq!(
            asked.path(),
            format!("/someone/encoder/resolve/{COMMIT}/onnx/model.onnx")
        );
        assert!(
            asked
                .header("user-agent")
                .is_some_and(|agent| agent.starts_with("steamgauge/")),
            "{asked:?}"
        );
    }

    #[tokio::test]
    async fn a_copy_that_matches_its_pin_is_used_without_asking_and_one_that_does_not_is_replaced()
    {
        let dir = crate::tempdir::Dir::new();
        let (server, source) = hub(GRAPH);
        std::fs::write(dir.path().join("model.onnx"), GRAPH).unwrap();
        fetch(source, pinned_to(GRAPH), dir.path()).await.0.unwrap();
        assert!(server.asked().is_empty());

        std::fs::write(dir.path().join("model.onnx"), b"other bytes").unwrap();
        fetch(source, pinned_to(GRAPH), dir.path()).await.0.unwrap();
        assert_eq!(server.asked().len(), 1);
        assert_eq!(std::fs::read(dir.path().join("model.onnx")).unwrap(), GRAPH);
    }

    #[tokio::test]
    async fn a_download_that_does_not_match_its_pin_is_refused_and_removed() {
        let dir = crate::tempdir::Dir::new();
        let (_server, source) = hub(b"other bytes");
        let (fetched, _) = fetch(source, pinned_to(GRAPH), dir.path()).await;
        assert!(
            matches!(fetched, Err(Error::ModelChecksum { .. })),
            "{fetched:?}"
        );
        assert!(!dir.path().join("model.onnx").exists());
    }

    #[tokio::test]
    async fn a_server_sending_more_than_the_pin_is_cut_off_before_the_excess_is_kept() {
        let dir = crate::tempdir::Dir::new();
        let (_server, source) = hub(b"graph bytes and then a great deal more");
        let (fetched, reports) = fetch(source, pinned_to(GRAPH), dir.path()).await;
        assert!(
            matches!(fetched, Err(Error::ModelLength { .. })),
            "{fetched:?}"
        );
        assert!(
            reports.iter().all(|(downloaded, _)| *downloaded <= 11),
            "{reports:?}"
        );
        assert!(!dir.path().join("model.onnx").exists());
        assert!(!dir.path().join("model.partial").exists());
    }

    #[tokio::test]
    async fn a_download_that_ends_short_is_refused_and_leaves_nothing_behind() {
        let dir = crate::tempdir::Dir::new();
        let (_server, source) = hub(b"graph");
        let (fetched, _) = fetch(source, pinned_to(GRAPH), dir.path()).await;
        assert!(
            matches!(fetched, Err(Error::ModelLength { .. })),
            "{fetched:?}"
        );
        assert!(!dir.path().join("model.onnx").exists());
        assert!(!dir.path().join("model.partial").exists());
    }

    #[tokio::test]
    async fn the_newest_release_is_the_highest_numbered_tag_on_the_hub() {
        let server = stand_in::Server::new(|_| {
            stand_in::Answer::json(&serde_json::json!({
                "branches": [{"name": "main"}],
                "tags": [{"name": "v2"}, {"name": "v10"}, {"name": "latest"}, {"name": "v9"}],
            }))
        });
        let newest = newest_release(&server.origin(), "someone/reader")
            .await
            .unwrap();
        assert_eq!(newest.as_deref(), Some("v10"));
        assert_eq!(server.asked()[0].path(), "/api/models/someone/reader/refs");
    }

    #[tokio::test]
    async fn a_cache_that_cannot_be_made_fails_before_anything_is_fetched() {
        let dir = crate::tempdir::Dir::new();
        let occupied = dir.path().join("a file");
        std::fs::write(&occupied, b"").unwrap();
        let made = ensure(&occupied, Encoder::E5Small, Precision::Float16, |_| {}).await;
        assert!(matches!(made, Err(Error::Io(_))), "{made:?}");
    }

    #[test]
    fn the_cache_is_where_the_platform_keeps_caches_and_not_wherever_the_app_started() {
        let platform_says = ["LOCALAPPDATA", "XDG_CACHE_HOME", "HOME"]
            .iter()
            .any(|name| std::env::var_os(name).is_some());
        assert!(
            platform_says,
            "every machine the tests run on names a cache or a home"
        );
        assert!(
            default_cache_dir().is_absolute(),
            "{:?}",
            default_cache_dir()
        );
    }

    #[test]
    fn hex_encodes_lowercase_and_pads() {
        assert_eq!(hex(&[0x00, 0x0f, 0xff]), "000fff");
    }

    #[test]
    fn the_cache_directory_is_namespaced_to_this_tool() {
        let dir = default_cache_dir();
        assert!(dir.to_string_lossy().contains("steamgauge"));
        assert!(dir.ends_with("models"));
    }
}
