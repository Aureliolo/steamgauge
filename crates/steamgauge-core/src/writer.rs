//! The model that writes a game's paragraph from what its players praise and complain about.
//!
//! Qwen3-4B-Instruct-2507, exported by `training/export_writer.py` with int4 weights into one
//! graph that takes tokens, a mask and the cache of past keys and values, and gives the next
//! token's logits. It is decoded here a token at a time, always taking the likeliest token, so
//! the same themes give the same paragraph. What it writes is then held to what it was told:
//! a sentence with a number or a quotation the themes do not hold is left out ([`checked`]).
//!
//! It runs on the processor everywhere. The graph is built for the processor's int4 kernels;
//! `DirectML` runs it at under a third of the speed and writes nonsense from it.
//!
//! Like the search models it is pinned by hash and fetched from where it is published. A copy
//! already in the model cache that matches its pins is used and nothing is fetched.

use std::{
    path::{Path, PathBuf},
    sync::{
        LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

use ort::{
    session::{Session, SessionInputValue},
    value::{DynValue, Tensor},
};
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;

use crate::{
    Error, Result,
    model::{self, Asset, DownloadProgress},
    taxonomy::SHEET,
    themes::Themes,
};

/// The brief the writer is given, shared with the export that checks it.
#[derive(Deserialize)]
struct Prompt {
    system: String,
    user: String,
    point: String,
    also: String,
    none: String,
}

static PROMPT: LazyLock<Prompt> = LazyLock::new(|| {
    serde_json::from_str(include_str!("writer-prompt.json"))
        .expect("writer-prompt.json is part of this build and parses")
});

/// Where the writer is published and cached, and its three files.
#[derive(Debug, Clone, Copy)]
pub struct WriterModel {
    pub name: &'static str,
    /// Empty until published.
    pub repository: &'static str,
    /// The commit it is fetched from. Empty until published.
    pub revision: &'static str,
    /// The release tag that commit carries, to name it to a person. Empty until published.
    pub release: &'static str,
    /// The Hub's origin, [`model::HUB`] everywhere but a test.
    pub hub: &'static str,
    assets: [Asset; 3],
}

// The pins are of the files `export_writer.py` wrote and that are to be published.
pub const WRITER: WriterModel = WriterModel {
    name: "writer",
    repository: "Aureliolo/steamgauge-writer",
    revision: "646f9254da44de3425ed8c0dd5edf14edb91a66f",
    release: "v1",
    hub: model::HUB,
    assets: [
        Asset {
            remote: "tokenizer.json",
            local: "tokenizer.json",
            sha256: "be75606093db2094d7cd20f3c2f385c212750648bd6ea4fb2bf507a6a4c55506",
            bytes: 11_422_650,
        },
        Asset {
            remote: "model.onnx",
            local: "model.onnx",
            sha256: "04ca14036926efcb3f97a6f9fdd981000e87a556f8d7a902b305c907cbd27080",
            bytes: 385_800,
        },
        // Named as the graph names it: the runtime finds the weights beside the graph by it.
        Asset {
            remote: "model.onnx.data",
            local: "model.onnx.data",
            sha256: "c39f184b43404066dde244d07360908b2dbb1f5a061dda43e9bf100f09d67a4f",
            bytes: 2_652_471_296,
        },
    ],
};

impl WriterModel {
    #[must_use]
    pub fn dir(self, cache_dir: &Path) -> PathBuf {
        cache_dir.join(self.name)
    }

    /// What writing a first paragraph would fetch. Only the files not here count.
    #[must_use]
    pub fn bytes_left(self, cache_dir: &Path) -> u64 {
        model::bytes_left(&self.assets, &self.dir(cache_dir))
    }

    #[must_use]
    pub fn fetched(self, cache_dir: &Path) -> bool {
        self.bytes_left(cache_dir) == 0
    }

    /// Makes sure every file is here and matches its pin, and fetches what is missing.
    ///
    /// # Errors
    ///
    /// Fails if a file is missing and the writer has not been published, if a fetched file does
    /// not match its pin, or on transport and filesystem failures.
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
        for asset in self.assets {
            model::ensure_asset(&http, source, asset, &dir, &mut on_progress).await?;
        }
        Ok(())
    }
}

/// `<|im_end|>` closes an answer. `<|endoftext|>` is what the model ends on without one.
const STOPS: [u32; 2] = [151_645, 151_643];

/// Past this a paragraph has run on: four short sentences take about a hundred and twenty.
pub const MOST_TOKENS: usize = 240;

/// How wide each attention head's keys and values are. The graph leaves it unnamed, so it is
/// held here beside the graph's pin: Qwen3-4B's is 128.
const HEAD_WIDTH: usize = 128;

/// Points of each side the writer is told about. Past these a point is made in too few reviews
/// to lead a paragraph with, and every point lengthens the prompt the processor reads first.
const POINTS_TOLD: usize = 8;

/// Other subjects named beside a point, as the page names them.
const ALSO_TOLD: usize = 3;

/// About how long one paragraph takes: 35 seconds for the eight points of each side on a
/// desktop processor of sixteen cores, and longer on a laptop's.
pub const SECONDS: f64 = 60.0;

/// The writer, loaded.
pub struct Writer {
    session: Session,
    tokenizer: Tokenizer,
    /// The cache inputs, each with the output that fills it on the next step.
    pasts: Vec<(String, String)>,
    heads: usize,
    width: usize,
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer").finish_non_exhaustive()
    }
}

impl Writer {
    /// # Errors
    ///
    /// Fails if the files are not in the cache, or the graph is not one this loop can drive.
    pub fn load(cache_dir: &Path) -> Result<Self> {
        let dir = WRITER.dir(cache_dir);
        let session = model::session_on_processor(&dir.join("model.onnx"))?;
        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| Error::Tokenizer(e.to_string()))?;
        let mut pasts = Vec::new();
        let mut shape = None;
        for input in session.inputs() {
            let Some(rest) = input.name().strip_prefix("past_key_values.") else {
                continue;
            };
            if input.dtype().tensor_type() != Some(ort::value::TensorElementType::Float32) {
                return Err(Error::Refused(format!(
                    "the writer's {} is not in single precision, which is what this build runs",
                    input.name()
                )));
            }
            shape = input.dtype().tensor_shape().map(|dims| dims.to_vec());
            pasts.push((input.name().to_owned(), format!("present.{rest}")));
        }
        let heads = shape
            .and_then(|dims| dims.get(1).copied())
            .and_then(|heads| usize::try_from(heads).ok())
            .ok_or_else(|| {
                Error::Refused(
                    "the writer's graph takes no cache of past keys and values".to_owned(),
                )
            })?;
        Ok(Self {
            session,
            tokenizer,
            pasts,
            heads,
            width: HEAD_WIDTH,
        })
    }

    /// The model's answer to a prompt in its chat format, taking the likeliest token at every
    /// step, as far as [`MOST_TOKENS`] or until it stops; `None` where `stop` was set first.
    ///
    /// # Errors
    ///
    /// Fails if the prompt cannot be tokenised or the graph does not run.
    pub fn answer(&mut self, prompt: &str, stop: &AtomicBool) -> Result<Option<String>> {
        let ids = self
            .tokenizer
            .encode(prompt, false)
            .map_err(|e| Error::Tokenizer(e.to_string()))?
            .get_ids()
            .to_vec();
        let Some(answer) = self.tokens(&ids, stop)? else {
            return Ok(None);
        };
        self.tokenizer
            .decode(&answer, true)
            .map(Some)
            .map_err(|e| Error::Tokenizer(e.to_string()))
    }

    fn tokens(&mut self, prompt: &[u32], stop: &AtomicBool) -> Result<Option<Vec<u32>>> {
        let mut cache: Vec<DynValue> = Vec::with_capacity(self.pasts.len());
        for _ in &self.pasts {
            cache.push(
                Tensor::from_array((vec![1, self.heads, 0, self.width], Vec::<f32>::new()))?
                    .into_dyn(),
            );
        }
        let mut seen = 0;
        let mut feeding: Vec<u32> = prompt.to_vec();
        let mut answer = Vec::new();
        loop {
            if stop.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let (next, filled) = self.step(&feeding, cache, seen)?;
            seen += feeding.len();
            cache = filled;
            if STOPS.contains(&next) || answer.len() >= MOST_TOKENS {
                return Ok(Some(answer));
            }
            answer.push(next);
            feeding = vec![next];
        }
    }

    /// Reads `ids` after the `seen` tokens the cache holds, and gives the likeliest next token
    /// with the cache that now holds them all.
    fn step(
        &mut self,
        ids: &[u32],
        cache: Vec<DynValue>,
        seen: usize,
    ) -> Result<(u32, Vec<DynValue>)> {
        let count = ids.len();
        let mut inputs: Vec<(String, SessionInputValue<'_>)> = vec![
            (
                "input_ids".to_owned(),
                Tensor::from_array((
                    vec![1, count],
                    ids.iter().map(|&id| i64::from(id)).collect::<Vec<i64>>(),
                ))?
                .into(),
            ),
            (
                "attention_mask".to_owned(),
                Tensor::from_array((vec![1, seen + count], vec![1_i64; seen + count]))?.into(),
            ),
        ];
        for ((name, _), past) in self.pasts.iter().zip(cache) {
            inputs.push((name.clone(), past.into()));
        }
        let mut outputs = self.session.run(inputs)?;
        let (shape, logits) = outputs["logits"].try_extract_tensor::<f32>()?;
        let vocabulary = usize::try_from(shape.last().copied().unwrap_or(0)).unwrap_or(0);
        let last = &logits[logits.len().saturating_sub(vocabulary)..];
        let next = last
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map_or(0, |(at, _)| u32::try_from(at).unwrap_or(0));
        let mut filled = Vec::with_capacity(self.pasts.len());
        for (_, present) in &self.pasts {
            filled.push(
                outputs.remove(present).ok_or_else(|| {
                    Error::Refused(format!("the writer's graph gave no {present}"))
                })?,
            );
        }
        Ok((next, filled))
    }
}

/// What the writer is told about one game, and what its paragraph is held to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brief {
    /// The whole prompt, in the model's chat format.
    pub prompt: String,
    /// What it is told, without the instructions: every number it may give is in here.
    told: String,
    /// Every label and quote it is given, which is everything it may quote.
    quotable: Vec<String>,
}

/// The brief for a game from its themes; `None` where no side has a point to write about.
#[must_use]
pub fn brief(game: &str, reviews: u64, themes: &Themes) -> Option<Brief> {
    let label = |id: &str| SHEET.iter().find(|row| row.id == id).map(|row| row.label);
    let mut quotable = Vec::new();
    let mut side = |side: &str| -> (String, usize) {
        let ranked = themes.ranked(side);
        if ranked.is_empty() {
            return (PROMPT.none.clone(), 0);
        }
        let lines: Vec<String> = ranked
            .iter()
            .take(POINTS_TOLD)
            .map(|one| {
                let said = one.said;
                quotable.push(said.label.clone());
                quotable.push(said.quote.text.clone());
                let also: Vec<&str> = said
                    .also_in
                    .iter()
                    .filter_map(|place| label(&place.subject))
                    .take(ALSO_TOLD)
                    .collect();
                PROMPT
                    .point
                    .replace("{label}", &said.label)
                    .replace("{reviews}", &crate::html::thousands(said.reviews))
                    .replace("{subject}", label(one.subject).unwrap_or(one.subject))
                    .replace(
                        "{also}",
                        &if also.is_empty() {
                            String::new()
                        } else {
                            PROMPT.also.replace("{subjects}", &also.join(", "))
                        },
                    )
                    .replace("{quote}", &said.quote.text)
            })
            .collect();
        (lines.join("\n"), ranked.len())
    };
    let (praise, praised) = side("praise");
    let (complaint, complained) = side("complaint");
    if praised + complained == 0 {
        return None;
    }
    let told = PROMPT
        .user
        .replace("{game}", game)
        .replace("{reviews}", &crate::html::thousands(reviews))
        .replace("{praise}", &praise)
        .replace("{complaint}", &complaint);
    Some(Brief {
        prompt: format!(
            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{told}<|im_end|>\n\
             <|im_start|>assistant\n",
            PROMPT.system
        ),
        told,
        quotable,
    })
}

/// Text compared as a reader compares it: case and runs of spaces aside, and curly quotation
/// marks the same as straight ones.
fn plain(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
}

/// The sentences of a paragraph, each with its closing mark. An unfinished last one is not one.
fn sentences(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, mark)) = chars.next() {
        let closes = matches!(mark, '.' | '!' | '?');
        // A closing quotation mark after the full stop belongs to the sentence.
        let mut end = at + mark.len_utf8();
        if closes {
            while let Some(&(next_at, next)) = chars.peek() {
                if matches!(next, '"' | '\u{201d}' | '\'' | '\u{2019}' | ')') {
                    end = next_at + next.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            if chars.peek().is_none_or(|(_, next)| next.is_whitespace()) {
                let sentence = text[start..end].trim();
                if !sentence.is_empty() {
                    found.push(sentence);
                }
                start = end;
            }
        }
    }
    found
}

/// The numbers a sentence gives, as written: "1,840", "12%", "4.5".
///
/// One pass over the bytes, each looked at once, so no change to the arithmetic can make it
/// run for ever or find a number twice.
fn numbers(sentence: &str) -> Vec<&str> {
    let bytes = sentence.as_bytes();
    let mut found = Vec::new();
    let mut start: Option<usize> = None;
    for (at, byte) in bytes.iter().enumerate() {
        let digit = byte.is_ascii_digit();
        let joins =
            matches!(byte, b',' | b'.') && bytes.get(at + 1).is_some_and(u8::is_ascii_digit);
        match start {
            None if digit => start = Some(at),
            Some(from) if !digit && !joins => {
                let end = if *byte == b'%' { at + 1 } else { at };
                found.push(&sentence[from..end]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push(&sentence[from..]);
    }
    found
}

/// What a sentence puts in quotation marks, straight or curly.
fn quotations(sentence: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut open: Option<usize> = None;
    for (at, mark) in sentence.char_indices() {
        match (mark, open) {
            ('"' | '\u{201c}', None) => open = Some(at + mark.len_utf8()),
            ('"' | '\u{201d}', Some(from)) => {
                found.push(&sentence[from..at]);
                open = None;
            }
            _ => {}
        }
    }
    found
}

/// American spellings the writer falls into whatever its brief says, with the British ones the
/// rest of the page is written in.
const SPELLINGS: [(&str, &str); 22] = [
    ("analyze", "analyse"),
    ("behavior", "behaviour"),
    ("center", "centre"),
    ("color", "colour"),
    ("colors", "colours"),
    ("criticize", "criticise"),
    ("criticized", "criticised"),
    ("criticizes", "criticises"),
    ("criticizing", "criticising"),
    ("favor", "favour"),
    ("favorite", "favourite"),
    ("favorites", "favourites"),
    ("gray", "grey"),
    ("humor", "humour"),
    ("labor", "labour"),
    ("optimization", "optimisation"),
    ("optimized", "optimised"),
    ("organized", "organised"),
    ("realize", "realise"),
    ("realized", "realised"),
    ("recognized", "recognised"),
    ("summarized", "summarised"),
];

/// `text` in British spelling, with what it puts in quotation marks left as the reviewer wrote it.
fn british(text: &str) -> String {
    fn spelled(word: &str) -> String {
        let lower = word.to_lowercase();
        let Some(&(_, british)) = SPELLINGS.iter().find(|(american, _)| *american == lower) else {
            return word.to_owned();
        };
        if word.starts_with(char::is_uppercase) {
            let mut letters = british.chars();
            letters
                .next()
                .map(|first| first.to_uppercase().chain(letters).collect())
                .unwrap_or_default()
        } else {
            british.to_owned()
        }
    }
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let mut quoted = false;
    for mark in text.chars() {
        if mark.is_alphabetic() {
            word.push(mark);
            continue;
        }
        out.push_str(&if quoted { word.clone() } else { spelled(&word) });
        word.clear();
        if matches!(mark, '"' | '\u{201c}' | '\u{201d}') {
            quoted = !quoted;
        }
        out.push(mark);
    }
    out.push_str(&if quoted { word } else { spelled(&word) });
    out
}

/// The paragraph without any sentence the brief does not bear out: one giving a number it was
/// not told, or quoting words no label or quote it was given holds. `None` where nothing is
/// left, because a paragraph of one surviving clause reads as the whole of what players say.
/// What is kept is in British spelling outside its quotations.
#[must_use]
pub fn checked(paragraph: &str, brief: &Brief) -> Option<String> {
    let told = plain(&brief.told);
    let quotable: Vec<String> = brief.quotable.iter().map(|text| plain(text)).collect();
    let paragraph = british(paragraph);
    let kept: Vec<&str> = sentences(&paragraph)
        .into_iter()
        .filter(|sentence| {
            numbers(sentence).iter().all(|number| told.contains(number))
                && quotations(sentence).iter().all(|quoted| {
                    let quoted = plain(quoted.trim_matches(|c: char| c.is_ascii_punctuation()));
                    !quoted.is_empty() && quotable.iter().any(|given| given.contains(&quoted))
                })
        })
        .collect();
    (kept.len() >= FEWEST_SENTENCES).then(|| kept.join(" "))
}

/// Sentences a paragraph needs to be shown at all.
const FEWEST_SENTENCES: usize = 2;

/// Whether somebody asked for paragraphs to be written, kept in the library: the writer is a
/// download of its own, so preparing a game fetches and runs it only once asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub write: bool,
}

const CHOICE_FILE: &str = "writer-choice.json";

impl Choice {
    /// The choice kept in `dir`, or none made where there is no file or it cannot be read.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(CHOICE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(self, dir: &Path) -> Result<()> {
        std::fs::write(dir.join(CHOICE_FILE), serde_json::to_vec_pretty(&self)?)?;
        Ok(())
    }
}

const FILE: &str = "paragraph.json";

/// A game's paragraph as written, for the themes and the writer it was written from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Paragraph {
    pub text: String,
    /// When the themes it was written from were last written, in seconds since the epoch.
    themes: Option<u64>,
    /// The graph's pin, which names the writer whatever it is published as.
    writer: String,
}

fn themes_written(snapshot: &Path) -> Option<u64> {
    std::fs::metadata(snapshot.join(crate::themes::FILE))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|since| since.as_secs())
}

impl Paragraph {
    #[must_use]
    pub fn new(text: String, snapshot: &Path) -> Self {
        Self {
            text,
            themes: themes_written(snapshot),
            writer: WRITER.assets[1].sha256.to_owned(),
        }
    }

    /// The paragraph written for the themes at `snapshot` as they now are, by this writer.
    #[must_use]
    pub fn load(snapshot: &Path) -> Option<Self> {
        let paragraph: Self =
            serde_json::from_slice(&std::fs::read(snapshot.join(FILE)).ok()?).ok()?;
        (paragraph.writer == WRITER.assets[1].sha256
            && paragraph.themes == themes_written(snapshot))
        .then_some(paragraph)
    }

    /// The text to show for the themes at `snapshot`: none where nothing was written for them,
    /// or where the check left nothing of what was.
    #[must_use]
    pub fn shown(snapshot: &Path) -> Option<String> {
        Self::load(snapshot)
            .map(|written| written.text)
            .filter(|text| !text.is_empty())
    }

    /// Written beside its name and moved over it, so the file is never half a paragraph.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn save(&self, snapshot: &Path) -> Result<()> {
        let partial = snapshot.join(format!("{FILE}.partial"));
        std::fs::write(&partial, serde_json::to_vec(self)?)?;
        std::fs::rename(partial, snapshot.join(FILE))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        themes::{Elsewhere, Quote, SubjectThemes, Theme},
        tiny_model::{Dim, Graph, word_tokenizer, write_json},
    };

    /// What a stand-in writer's cache is.
    enum Cache {
        Float,
        Integer,
        Missing,
    }

    /// A writer whose likeliest next word after "one" is "two", after "two" "three", and after
    /// "three" "one" again, with two heads of keys and values handed back unchanged.
    fn stand_in(cache_dir: &Path, cache: &Cache) {
        // The tokenizer's ids: [PAD], [UNK], one, two, three.
        let towards =
            |id: usize| -> Vec<f32> { (0..5).map(|at| if at == id { 1.0 } else { 0.0 }).collect() };
        let mut graph = Graph::new();
        graph.per_token(
            "logits",
            &[towards(2), towards(2), towards(3), towards(4), towards(2)],
        );
        write_stand_in(cache_dir, cache, graph);
    }

    /// `graph`, which gives the logits, with the cache and the tokenizer of one, two, three.
    fn write_stand_in(cache_dir: &Path, cache: &Cache, mut graph: Graph) {
        let dir = WRITER.dir(cache_dir);
        let dims = [
            Dim::Named("batch"),
            Dim::Sized(2),
            Dim::Named("past"),
            Dim::Named("width"),
        ];
        match cache {
            Cache::Float => {
                graph
                    .passed("past_key_values.0.key", "present.0.key", &dims)
                    .passed("past_key_values.0.value", "present.0.value", &dims);
            }
            Cache::Integer => {
                graph.input("past_key_values.0.key", &dims);
            }
            Cache::Missing => {}
        }
        graph.write(&dir.join("model.onnx"));
        write_json(
            &dir.join("tokenizer.json"),
            &word_tokenizer(&["one", "two", "three"]),
        );
    }

    #[test]
    fn the_writer_answers_with_its_likeliest_word_each_step_up_to_its_longest() {
        let dir = crate::tempdir::Dir::new();
        stand_in(dir.path(), &Cache::Float);
        let mut writer = Writer::load(dir.path()).unwrap();
        let answer = writer
            .answer("one two", &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        let words: Vec<&str> = answer.split_whitespace().collect();
        assert_eq!(words[..4], ["three", "one", "two", "three"]);
        assert_eq!(
            words.len(),
            MOST_TOKENS,
            "a writer that never stops is cut off"
        );
        assert!(format!("{writer:?}").starts_with("Writer"));
    }

    #[test]
    fn each_step_masks_every_token_read_before_it() {
        let dir = crate::tempdir::Dir::new();
        let mut graph = Graph::new();
        graph.counted("logits", 5);
        write_stand_in(dir.path(), &Cache::Float, graph);
        let answer = Writer::load(dir.path())
            .unwrap()
            .answer("one two", &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        let words: Vec<&str> = answer.split_whitespace().collect();
        assert_eq!(words[..3], ["one", "two", "three"]);
    }

    #[test]
    fn the_paragraph_is_spelt_as_the_page_is_and_a_quotation_as_its_reviewer_wrote_it() {
        assert_eq!(
            british("Humor is praised; the combat is criticized. \"Great humor\" says one."),
            "Humour is praised; the combat is criticised. \"Great humor\" says one."
        );
        assert_eq!(
            british("Its color"),
            "Its colour",
            "a last word with no mark after it"
        );
    }

    #[tokio::test]
    async fn the_writer_is_fetched_into_a_directory_of_its_own_and_then_counts_as_here() {
        let hub = crate::stand_in::Server::new(|asked| {
            crate::stand_in::Answer::body(asked.path().rsplit('/').next().unwrap_or_default())
        });
        let pinned = |name: &'static str| Asset {
            remote: name,
            local: name,
            sha256: crate::embed::sha256_hex(name).leak(),
            bytes: name.len() as u64,
        };
        let model = WriterModel {
            repository: "someone/steamgauge-writer",
            revision: "614241f622f53c4eeff9890bdc4f31cfecc418b3",
            hub: hub.origin().leak(),
            assets: [
                pinned("tokenizer.json"),
                pinned("model.onnx"),
                pinned("model.onnx.data"),
            ],
            ..WRITER
        };
        let cache = crate::tempdir::Dir::new();
        assert_eq!(model.bytes_left(cache.path()), 14 + 10 + 15);
        assert!(!model.fetched(cache.path()));

        model.ensure(cache.path(), |_| {}).await.unwrap();
        assert_eq!(
            std::fs::read(cache.path().join("writer").join("model.onnx.data")).unwrap(),
            b"model.onnx.data"
        );
        assert!(model.fetched(cache.path()));
        assert_eq!(
            hub.asked()[0].path(),
            "/someone/steamgauge-writer/resolve/614241f622f53c4eeff9890bdc4f31cfecc418b3/tokenizer.json"
        );
    }

    #[test]
    fn the_writer_is_fetched_from_a_pinned_commit_of_every_file_it_needs() {
        let source = model::Source {
            hub: WRITER.hub,
            repository: WRITER.repository,
            revision: WRITER.revision,
        };
        assert!(source.is_pinned());
        assert_ne!(WRITER.release, "");
        assert!(
            WRITER
                .assets
                .iter()
                .all(|asset| asset.sha256.len() == 64 && asset.bytes > 0)
        );
    }

    #[tokio::test]
    async fn an_unpublished_writer_that_is_not_here_cannot_be_fetched() {
        let empty = crate::tempdir::Dir::new();
        let unpublished = WriterModel {
            repository: "",
            revision: "",
            ..WRITER
        };
        assert!(unpublished.ensure(empty.path(), |_| {}).await.is_err());
    }

    #[test]
    fn nobody_has_asked_for_paragraphs_until_somebody_does() {
        let dir = crate::tempdir::Dir::new();
        assert!(!Choice::load(dir.path()).write);
        Choice { write: true }.save(dir.path()).unwrap();
        assert!(Choice::load(dir.path()).write);
    }

    #[test]
    fn a_paragraph_stands_only_for_the_themes_it_was_written_from() {
        let dir = crate::tempdir::Dir::new();
        let themes = dir.path().join(crate::themes::FILE);
        let written_at = |seconds: u64| {
            std::fs::write(&themes, "{}").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&themes)
                .unwrap()
                .set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
                .unwrap();
        };
        written_at(1_700_000_000);
        assert_eq!(Paragraph::load(dir.path()), None, "nothing written yet");

        Paragraph::new("Players like it.".to_owned(), dir.path())
            .save(dir.path())
            .unwrap();
        assert_eq!(
            Paragraph::shown(dir.path()).as_deref(),
            Some("Players like it.")
        );

        Paragraph::new(String::new(), dir.path())
            .save(dir.path())
            .unwrap();
        assert!(
            Paragraph::load(dir.path()).is_some(),
            "a paragraph the check left nothing of is kept, so it is not offered again"
        );
        assert_eq!(Paragraph::shown(dir.path()), None, "but nothing is shown");

        written_at(1_700_000_600);
        assert_eq!(
            Paragraph::load(dir.path()),
            None,
            "the themes were found again since"
        );
    }

    #[test]
    fn a_stopped_writer_gives_no_answer() {
        let dir = crate::tempdir::Dir::new();
        stand_in(dir.path(), &Cache::Float);
        let mut writer = Writer::load(dir.path()).unwrap();
        assert_eq!(writer.answer("one", &AtomicBool::new(true)).unwrap(), None);
    }

    #[test]
    fn a_graph_without_a_cache_in_single_precision_is_refused() {
        for cache in [Cache::Integer, Cache::Missing] {
            let dir = crate::tempdir::Dir::new();
            stand_in(dir.path(), &cache);
            assert!(matches!(Writer::load(dir.path()), Err(Error::Refused(_))));
        }
    }

    fn theme(label: &str, quote: &str, reviews: u64, also: &[(&str, usize)]) -> Theme {
        Theme {
            label: label.to_owned(),
            quote: Quote {
                review_id: "1".to_owned(),
                at: (0, 0),
                text: quote.to_owned(),
            },
            reviews,
            claims: reviews,
            also_in: also
                .iter()
                .map(|&(subject, theme)| Elsewhere {
                    subject: subject.to_owned(),
                    theme,
                })
                .collect(),
            members: Vec::new(),
        }
    }

    fn themes() -> Themes {
        Themes {
            encoder: "test".to_owned(),
            readings: None,
            subjects: vec![
                SubjectThemes {
                    subject: "bugs".to_owned(),
                    side: "complaint".to_owned(),
                    reviews: 300,
                    themes: vec![theme(
                        "Black screen on startup.",
                        "Black screen on startup until I alt-tab.",
                        210,
                        &[("performance", 0)],
                    )],
                },
                SubjectThemes {
                    subject: "gameplay".to_owned(),
                    side: "praise".to_owned(),
                    reviews: 2_000,
                    themes: vec![theme(
                        "Mixing factions keeps it fresh.",
                        "Mixing two factions every run keeps it fresh.",
                        1_840,
                        &[],
                    )],
                },
            ],
        }
    }

    #[test]
    fn the_brief_names_each_point_with_its_count_subject_and_quote() {
        let brief = brief("Lantern Keep", 14_200, &themes()).unwrap();
        assert!(brief.prompt.starts_with("<|im_start|>system\n"));
        assert!(brief.prompt.ends_with("<|im_start|>assistant\n"));
        assert!(brief.told.starts_with("Lantern Keep, from 14,200 reviews."));
        assert!(brief.told.contains(
            "- Mixing factions keeps it fresh. (1,840 reviews, about Gameplay and mechanics) \
             Quote: \"Mixing two factions every run keeps it fresh.\""
        ));
        assert!(brief.told.contains(
            "- Black screen on startup. (210 reviews, about Bugs and crashes; also about \
             Performance) Quote: \"Black screen on startup until I alt-tab.\""
        ));
    }

    #[test]
    fn a_game_with_no_point_on_either_side_has_no_brief() {
        let empty = Themes {
            encoder: "test".to_owned(),
            readings: None,
            subjects: Vec::new(),
        };
        assert_eq!(brief("Nothing", 10, &empty), None);
    }

    #[test]
    fn a_game_with_points_on_one_side_only_has_a_brief() {
        let mut praised_only = themes();
        praised_only
            .subjects
            .retain(|subject| subject.side == "praise");
        let brief = brief("Lantern Keep", 14_200, &praised_only).unwrap();
        assert!(brief.told.contains("Mixing factions keeps it fresh."));
        assert!(!brief.told.contains("Black screen"));
    }

    #[test]
    fn a_sentence_with_a_number_or_a_quotation_it_was_not_given_is_left_out() {
        let brief = brief("Lantern Keep", 14_200, &themes()).unwrap();
        let written = "Players love mixing factions, which 1,840 reviews praise. \
                       About 2,000 reviewers call it the best roguelike ever. \
                       Some report a “black screen on startup until I alt-tab.” \
                       Others say it “crashes every hour”. \
                       The paragraph then runs out mid";
        assert_eq!(
            checked(written, &brief).as_deref(),
            Some(
                "Players love mixing factions, which 1,840 reviews praise. Some report a \
                 “black screen on startup until I alt-tab.”"
            )
        );
    }

    #[test]
    fn one_sentence_left_is_no_paragraph() {
        let brief = brief("Lantern Keep", 14_200, &themes()).unwrap();
        assert_eq!(
            checked("It has 1,840 fans. It has 99 problems.", &brief),
            None
        );
    }

    #[test]
    fn numbers_and_quotations_are_read_as_written() {
        assert_eq!(
            numbers("From 14,200 reviews, 4.5% and 12 say so."),
            ["14,200", "4.5%", "12"]
        );
        assert_eq!(
            numbers("Rated 1, then 1,840. Now 12"),
            ["1", "1,840", "12"],
            "a mark not followed by a digit ends the number, as does the sentence"
        );
        assert_eq!(
            quotations("They say \"fine\" and “not fine”."),
            ["fine", "not fine"]
        );
        assert_eq!(
            sentences("One. Two “quoted.” Three is 4.5 long! Unfinished"),
            ["One.", "Two “quoted.”", "Three is 4.5 long!"]
        );
    }

    /// What the export check recorded the graph answering, which this loop has to give token
    /// for token: the same graph decoded the same way.
    #[test]
    #[ignore = "needs the writer in the model cache"]
    fn the_loop_answers_as_the_export_check_recorded() {
        #[derive(Deserialize)]
        struct Reference {
            prompt: String,
            prompt_ids: Vec<u32>,
            answer_ids: Vec<u32>,
        }
        let reference: Reference =
            serde_json::from_str(include_str!("writer-reference.json")).unwrap();
        let mut writer = Writer::load(&model::default_cache_dir()).unwrap();
        let ids = writer
            .tokenizer
            .encode(reference.prompt.as_str(), false)
            .unwrap()
            .get_ids()
            .to_vec();
        assert_eq!(ids, reference.prompt_ids);
        let answer = writer
            .tokens(&ids, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        assert_eq!(answer, reference.answer_ids);
    }
}
