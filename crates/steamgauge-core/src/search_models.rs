//! The two models search by meaning runs: an encoder that puts a game's claims and a search in
//! one space, and a reranker that reads the search and each claim near it together.
//!
//! Chosen on 1,029 results judged by whether they were what was asked for, over fourteen
//! searches on three games (DECISIONS.md): Qwen3-Embedding-0.6B with Qwen3-Reranker-0.6B found
//! it in 77% of their first ten, gte-multilingual-base alone in 64%, and gte put up 24 opposites
//! of what was asked where the pair put up 5. Both are this project's own exports
//! (`training/export_search.py`), graphs that take tokens and give back only what is used: a
//! vector, and a score between 0 and 1.
//!
//! Like the reader, both are pinned by hash and fetched from where they are published; until they
//! are, a copy in the model cache that matches its pin is used and nothing is fetched.

use std::path::{Path, PathBuf};

use ndarray::Array2;
use ort::{session::Session, value::Tensor};
use tokenizers::{PaddingParams, Tokenizer};

use crate::{
    Error, Result,
    model::{self, Asset, DownloadProgress},
};

/// The prompts both were trained to be asked in, shared with the export that checks them.
#[derive(serde::Deserialize)]
struct Prompts {
    instruction: String,
    query: String,
    reranker: String,
}

static PROMPTS: std::sync::LazyLock<Prompts> = std::sync::LazyLock::new(|| {
    serde_json::from_str(include_str!("search-prompts.json"))
        .expect("search-prompts.json is part of this build and parses")
});

/// One exported model: where it is published and cached, and its two files.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    /// What the vectors a game is prepared with are recorded as, and the cache directory.
    pub name: &'static str,
    /// Empty until published.
    pub repository: &'static str,
    /// The commit it is fetched from; empty until published.
    pub revision: &'static str,
    /// The release tag that commit carries, to name it to a person; empty until published.
    pub release: &'static str,
    /// The Hub's origin, [`model::HUB`] everywhere but a test. After the three above, which
    /// `training/publish.py` finds together to pin.
    pub hub: &'static str,
    graph: Asset,
    tokenizer: Asset,
}

// The pins are of the files `export_search.py` wrote and that are to be published; an export
// run again is not byte for byte the same file, so these are the ones, not any export.
pub const ENCODER: Model = Model {
    name: "search-encoder",
    repository: "Aureliolo/steamgauge-search-encoder",
    revision: "c09c77b972e610a01a628e91395b427193f455fa",
    release: "v1",
    hub: model::HUB,
    graph: Asset {
        remote: "model.onnx",
        local: "model.onnx",
        sha256: "f69a8a7017cfc32ae862bed62b8858a540d627ce691047f610a13821285aa859",
        bytes: 1_310_380_253,
    },
    tokenizer: Asset {
        remote: "tokenizer.json",
        local: "tokenizer.json",
        sha256: "c87c38db060bafb0122019c0c749ec1eb1ae510dae43c93f0042ec51099942e8",
        bytes: 11_423_971,
    },
};

pub const RERANKER: Model = Model {
    name: "search-reranker",
    repository: "Aureliolo/steamgauge-search-reranker",
    revision: "977b6ea57a5559c7931f55f1351ee8e795d4cb4e",
    release: "v1",
    hub: model::HUB,
    graph: Asset {
        remote: "model.onnx",
        local: "model.onnx",
        sha256: "0e73e548bd14f56b7f0c9b1f4ef00cb82b752f902150070622f822fa6aad8986",
        bytes: 1_310_384_274,
    },
    tokenizer: Asset {
        remote: "tokenizer.json",
        local: "tokenizer.json",
        sha256: "64916bb803f29cbbf1f60383b381612a965319299e249c6fff8a713e5b03be82",
        bytes: 11_422_920,
    },
};

/// The encoder's vectors are this wide.
pub const DIMENSIONS: usize = 1024;

/// How many of the encoder's nearest claims the reranker reads, on the device it runs on.
///
/// The hundred the judged comparison gave it on a card, where they take a few seconds at most.
/// On a processor a hundred took 28 s a search, and reading the nearest 30 instead found what was
/// asked for in 76.6% of the first ten against 77.2%, inside what fourteen searches can tell
/// apart, in 8 s.
#[must_use]
pub fn candidates(device: &str) -> usize {
    if device == "cpu" { 30 } else { 100 }
}

/// A claim the reranker scores below this is not shown. On the 1,029 judged pairs, scored by the
/// exported graph: it keeps 93% of what was asked for and 81% of what was related, and
/// leaves out half of what was not and 62% of what said the opposite. What it keeps is ordered by
/// the same score, so the rest of those sit at the bottom.
pub const SHOWN_FROM: f32 = 0.1;

/// A claim, or a search in its instruction, is cut to this.
const ENCODE_TOKENS: usize = 512;
/// A search and a claim in the reranker's prompt, which is about sixty tokens on its own.
const RERANK_TOKENS: usize = 320;

impl Model {
    /// The folder this model's files are kept in.
    #[must_use]
    pub fn dir(self, cache_dir: &Path) -> PathBuf {
        cache_dir.join(self.name)
    }

    /// What a first search would fetch of this model, counting only the files not here.
    #[must_use]
    pub fn bytes_left(self, cache_dir: &Path) -> u64 {
        model::bytes_left(&[self.tokenizer, self.graph], &self.dir(cache_dir))
    }

    /// Whether both files are here, so a window can say what a first search would fetch.
    #[must_use]
    pub fn fetched(self, cache_dir: &Path) -> bool {
        self.bytes_left(cache_dir) == 0
    }

    /// Makes sure both files are here and match their pins, fetching what is missing.
    ///
    /// # Errors
    ///
    /// Fails if a file is missing or unpinned and the model has not been published, if a
    /// fetched file does not match its pin, or on transport and filesystem failures.
    pub async fn ensure(
        self,
        cache_dir: &Path,
        mut on_progress: impl FnMut(DownloadProgress),
    ) -> Result<()> {
        let dir = self.dir(cache_dir);
        std::fs::create_dir_all(&dir)?;
        let http = model::client()?;
        let source = model::Source {
            hub: self.hub,
            repository: self.repository,
            revision: self.revision,
        };
        for asset in [self.tokenizer, self.graph] {
            // A copy here that matches its pin is used without asking anyone; one that does not
            // is fetched again, which before publication fails rather than running a file
            // nothing vouches for.
            model::ensure_asset(&http, source, asset, &dir, &mut on_progress).await?;
        }
        Ok(())
    }

    fn session(self, cache_dir: &Path, limit: usize) -> Result<Loaded> {
        let dir = self.dir(cache_dir);
        let (session, device) = model::session_at(&dir.join(self.graph.local))?;
        let mut tokenizer = Tokenizer::from_file(dir.join(self.tokenizer.local))
            .map_err(|e| Error::Tokenizer(e.to_string()))?;
        let pad = tokenizer
            .token_to_id("<|endoftext|>")
            .ok_or_else(|| Error::Tokenizer("no <|endoftext|> to pad with".to_owned()))?;
        // Right padding to the longest of the batch, which is what the graph reads each row's
        // last real token under. Only the id is set: the graph is handed ids, and the text a
        // padded position would carry is read by nothing.
        tokenizer.with_padding(Some(PaddingParams {
            pad_id: pad,
            ..PaddingParams::default()
        }));
        tokenizer
            .with_truncation(Some(model::cut_at(limit)))
            .map_err(|e| Error::Tokenizer(e.to_string()))?;
        Ok(Loaded {
            session,
            tokenizer,
            device,
        })
    }
}

struct Loaded {
    session: Session,
    tokenizer: Tokenizer,
    device: &'static str,
}

impl Loaded {
    /// The graph's one output for each text, as rows of `width` numbers.
    fn run(&mut self, texts: Vec<String>, output: &str, width: usize) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        // With the tokenizer's own special tokens: the encoder's closes every text with the end
        // token its vector is read at, which is how the model was trained and exported.
        let encodings = self
            .tokenizer
            .encode_batch(texts, true)
            .map_err(|e| Error::Tokenizer(e.to_string()))?;
        let rows = encodings.len();
        let cols = encodings.first().map_or(0, |e| e.get_ids().len());
        let ids: Vec<i64> = encodings
            .iter()
            .flat_map(|e| e.get_ids().iter().map(|&id| i64::from(id)))
            .collect();
        let mask: Vec<i64> = encodings
            .iter()
            .flat_map(|e| e.get_attention_mask().iter().map(|&m| i64::from(m)))
            .collect();
        let outputs = self.session.run(ort::inputs![
            "input_ids" => Tensor::from_array(Array2::from_shape_vec((rows, cols), ids)?)?,
            "attention_mask" => Tensor::from_array(Array2::from_shape_vec((rows, cols), mask)?)?,
        ])?;
        let values = outputs[output].try_extract_array::<f32>()?;
        let flat: Vec<f32> = values.iter().copied().collect();
        Ok(flat.chunks(width).map(<[f32]>::to_vec).collect())
    }
}

/// Puts claims and searches in one space.
pub struct SearchEncoder(Loaded);

impl std::fmt::Debug for SearchEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchEncoder")
            .field("device", &self.0.device)
            .finish_non_exhaustive()
    }
}

impl SearchEncoder {
    /// # Errors
    ///
    /// Fails if the files are missing or corrupt, or the graph loads on no backend.
    pub fn load(cache_dir: &Path) -> Result<Self> {
        ENCODER.session(cache_dir, ENCODE_TOKENS).map(Self)
    }

    #[must_use]
    pub fn device(&self) -> &'static str {
        self.0.device
    }

    /// Each claim's unit vector, as it is.
    ///
    /// # Errors
    ///
    /// Fails if tokenisation or the forward pass fails.
    pub fn claims(&mut self, claims: &[String]) -> Result<Vec<Vec<f32>>> {
        self.0.run(claims.to_vec(), "vector", DIMENSIONS)
    }

    /// A search's unit vector, asked in the instruction the encoder was trained to be asked in:
    /// a search is a question and a claim an answer, and only the question is told what it is.
    ///
    /// # Errors
    ///
    /// Fails if tokenisation or the forward pass fails.
    pub fn search(&mut self, query: &str) -> Result<Vec<f32>> {
        let asked = PROMPTS
            .query
            .replace("{instruction}", &PROMPTS.instruction)
            .replace("{query}", query);
        self.0
            .run(vec![asked], "vector", DIMENSIONS)?
            .into_iter()
            .next()
            .ok_or(Error::MalformedPayload {
                field: "search vector",
            })
    }
}

/// Reads a search and a claim together, and says how surely the claim says it.
pub struct SearchReranker(Loaded);

impl std::fmt::Debug for SearchReranker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchReranker")
            .field("device", &self.0.device)
            .finish_non_exhaustive()
    }
}

/// Pairs read at once: a card takes more, and a processor gains nothing past a handful.
const RERANK_BATCH: usize = 16;

impl SearchReranker {
    /// # Errors
    ///
    /// Fails if the files are missing or corrupt, or the graph loads on no backend.
    pub fn load(cache_dir: &Path) -> Result<Self> {
        RERANKER.session(cache_dir, RERANK_TOKENS).map(Self)
    }

    #[must_use]
    pub fn device(&self) -> &'static str {
        self.0.device
    }

    /// How surely each claim says what was searched, from 0 to 1, in the claims' order.
    ///
    /// # Errors
    ///
    /// Fails if tokenisation or the forward pass fails.
    pub fn score(&mut self, query: &str, claims: &[String]) -> Result<Vec<f32>> {
        let asked = PROMPTS
            .reranker
            .replace("{instruction}", &PROMPTS.instruction)
            .replace("{query}", query);
        let mut scores = Vec::with_capacity(claims.len());
        for chunk in claims.chunks(RERANK_BATCH) {
            let prompts = chunk
                .iter()
                .map(|claim| asked.replace("{claim}", claim))
                .collect();
            scores.extend(self.0.run(prompts, "score", 1)?.into_iter().flatten());
        }
        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompts_carry_every_place_they_are_filled_in() {
        assert!(PROMPTS.query.contains("{instruction}") && PROMPTS.query.contains("{query}"));
        for hole in ["{instruction}", "{query}", "{claim}"] {
            assert!(PROMPTS.reranker.contains(hole), "{hole}");
        }
    }

    #[test]
    fn an_unpublished_model_with_nothing_here_cannot_be_ensured() {
        let empty = crate::tempdir::Dir::new();
        let unpublished = Model {
            repository: "",
            revision: "",
            ..ENCODER
        };
        let refused = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(unpublished.ensure(empty.path(), |_| {}));
        assert!(refused.is_err());
    }

    #[test]
    fn a_processor_reranks_fewer_candidates_than_a_card() {
        assert!(candidates("cpu") < candidates("directml"));
        assert!(candidates("cpu") > 0);
        assert_eq!(candidates("directml"), candidates("cuda"));
    }

    /// The words the tiny models know, after `[PAD]` and `[UNK]`.
    const WORDS: [&str; 3] = ["<|endoftext|>", "good", "bad"];
    const GOOD: usize = 3;

    /// Tiny copies of both models in `cache`: the encoder's vector counts each token at the
    /// dimension of its id, and the reranker's score counts "good"s. `[PAD]` counts in both and
    /// the end token in neither, so a batch padded with anything but the end token shows.
    fn tiny_models(cache: &Path) {
        let vocabulary = WORDS.len() + 2;
        let vectors: Vec<Vec<f32>> = (0..vocabulary)
            .map(|id| {
                (0..DIMENSIONS)
                    .map(|dim| if dim == id && id != 2 { 1.0 } else { 0.0 })
                    .collect()
            })
            .collect();
        let mut scores = vec![vec![0.0]; vocabulary];
        scores[0][0] = 7.0;
        scores[GOOD][0] = 1.0;
        let tokenizer = crate::tiny_model::word_tokenizer(&WORDS);
        for (model, graph) in [
            (
                ENCODER,
                crate::tiny_model::Graph::new().summed("vector", &vectors),
            ),
            (
                RERANKER,
                crate::tiny_model::Graph::new().summed("score", &scores),
            ),
        ] {
            let dir = model.dir(cache);
            graph.write(&dir.join(model.graph.local));
            crate::tiny_model::write_json(&dir.join(model.tokenizer.local), &tokenizer);
        }
    }

    fn texts(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|&text| text.to_owned()).collect()
    }

    #[test]
    fn claims_and_a_search_are_put_in_one_space_and_only_the_search_is_told_what_it_is() {
        let cache = crate::tempdir::Dir::new();
        tiny_models(cache.path());
        let mut encoder = SearchEncoder::load(cache.path()).unwrap();
        assert_eq!(encoder.device(), "cpu");

        let claims = encoder.claims(&texts(&["good", "bad good bad"])).unwrap();
        assert_eq!(claims.len(), 2);
        assert!(claims.iter().all(|vector| vector.len() == DIMENSIONS));
        assert_eq!(claims[0][GOOD..=GOOD + 1], [1.0, 0.0]);
        assert_eq!(claims[1][GOOD..=GOOD + 1], [1.0, 2.0]);
        assert_eq!(
            claims[0],
            encoder.claims(&texts(&["good"])).unwrap()[0],
            "padding is the end token, which says nothing"
        );
        assert_eq!(encoder.claims(&[]).unwrap(), [] as [Vec<f32>; 0]);

        let asked = PROMPTS
            .query
            .replace("{instruction}", &PROMPTS.instruction)
            .replace("{query}", "good");
        let search = encoder.search("good").unwrap();
        assert_eq!(search, encoder.claims(&[asked]).unwrap()[0]);
        assert_ne!(search, claims[0]);
    }

    #[test]
    fn every_claim_is_scored_in_its_order_however_many_batches_it_takes() {
        let cache = crate::tempdir::Dir::new();
        tiny_models(cache.path());
        let mut reranker = SearchReranker::load(cache.path()).unwrap();
        assert_eq!(reranker.device(), "cpu");

        let claims: Vec<String> = (0..=RERANK_BATCH)
            .map(|goods| "good ".repeat(goods))
            .collect();
        let scores = reranker.score("bad", &claims).unwrap();
        assert_eq!(scores.len(), RERANK_BATCH + 1);
        for (goods, score) in scores.iter().enumerate() {
            let alone = reranker.score("bad", &claims[goods..=goods]).unwrap();
            assert_eq!(alone, [*score], "{goods}");
        }
        let counted: Vec<f32> = (0..=RERANK_BATCH)
            .map(|goods| f32::from(u8::try_from(goods).unwrap()))
            .collect();
        let base = scores[0];
        assert_eq!(
            scores.iter().map(|score| score - base).collect::<Vec<_>>(),
            counted,
            "each claim scored for its own goods, in order"
        );
        assert_eq!(reranker.score("bad", &[]).unwrap(), [] as [f32; 0]);
    }

    #[test]
    fn the_reranker_reads_a_search_and_a_claim_only_as_far_as_its_budget() {
        let cache = crate::tempdir::Dir::new();
        tiny_models(cache.path());
        let mut reranker = SearchReranker::load(cache.path()).unwrap();
        let long = reranker
            .score("bad", &texts(&[&"good ".repeat(400), &"good ".repeat(500)]))
            .unwrap();
        assert_eq!(long[..1], long[1..]);
        assert!(long[0] < 320.0, "{long:?}");
    }

    #[tokio::test]
    async fn a_model_is_fetched_into_a_directory_of_its_own_and_then_counts_as_here() {
        let hub = crate::stand_in::Server::new(|asked| {
            crate::stand_in::Answer::body(if asked.path().ends_with("model.onnx") {
                "the graph"
            } else {
                "the tokenizer"
            })
        });
        let pinned = |name: &'static str, content: &str| Asset {
            remote: name,
            local: name,
            sha256: crate::embed::sha256_hex(content).leak(),
            bytes: content.len() as u64,
        };
        let model = Model {
            hub: hub.origin().leak(),
            graph: pinned("model.onnx", "the graph"),
            tokenizer: pinned("tokenizer.json", "the tokenizer"),
            ..ENCODER
        };
        let cache = crate::tempdir::Dir::new();
        assert_eq!(model.bytes_left(cache.path()), 9 + 13);
        assert!(!model.fetched(cache.path()));

        model.ensure(cache.path(), |_| {}).await.unwrap();
        assert_eq!(
            std::fs::read(cache.path().join("search-encoder").join("model.onnx")).unwrap(),
            b"the graph"
        );
        assert_eq!(model.bytes_left(cache.path()), 0);
        assert!(model.fetched(cache.path()));
        assert_eq!(
            hub.asked()[0].path(),
            format!(
                "/{}/resolve/{}/tokenizer.json",
                ENCODER.repository, ENCODER.revision
            )
        );
    }

    #[test]
    fn both_search_models_are_fetched_from_a_pinned_commit() {
        for model in [ENCODER, RERANKER] {
            let source = model::Source {
                hub: model.hub,
                repository: model.repository,
                revision: model.revision,
            };
            assert!(source.is_pinned(), "{}", model.name);
        }
    }
}
