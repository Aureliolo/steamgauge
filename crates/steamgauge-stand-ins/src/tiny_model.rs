//! Models small enough to write out in a test: an ONNX graph of a few tables and a tokenizer of a
//! few words.
//!
//! The code that loads and runs a model is tested on CPU with no model downloaded.
//!
//! The graph is written as the protobuf ONNX defines, field by field, which needs nothing beyond
//! the standard library. Each output is a table looked up by token id and, where asked, summed
//! over the tokens: what a test then asserts follows from the tables by arithmetic.

use std::path::Path;

/// `ONNX`'s numbers for the element types a graph here uses.
const FLOAT: i64 = 1;
const INT64: i64 = 7;

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push(u8::try_from(value & 0x7f).unwrap() | 0x80);
        value >>= 7;
    }
    out.push(u8::try_from(value).unwrap());
}

fn int(out: &mut Vec<u8>, field: u64, value: i64) {
    varint(out, field << 3);
    varint(out, value.cast_unsigned());
}

fn bytes(out: &mut Vec<u8>, field: u64, value: &[u8]) {
    varint(out, (field << 3) | 2);
    varint(out, value.len() as u64);
    out.extend_from_slice(value);
}

/// A tensor's type: its elements, and each dimension named where it varies or sized where not.
fn value_info(name: &str, element: i64, dims: &[Dim]) -> Vec<u8> {
    let mut shape = Vec::new();
    for dim in dims {
        let mut one = Vec::new();
        match dim {
            Dim::Named(name) => bytes(&mut one, 2, name.as_bytes()),
            Dim::Sized(size) => int(&mut one, 1, *size),
        }
        bytes(&mut shape, 1, &one);
    }
    let mut tensor = Vec::new();
    int(&mut tensor, 1, element);
    bytes(&mut tensor, 2, &shape);
    let mut kind = Vec::new();
    bytes(&mut kind, 1, &tensor);
    let mut info = Vec::new();
    bytes(&mut info, 1, name.as_bytes());
    bytes(&mut info, 2, &kind);
    info
}

fn width_of(table: &[Vec<f32>]) -> i64 {
    i64::try_from(table.first().map_or(0, Vec::len)).unwrap()
}

/// One dimension of an input or an output.
#[derive(Debug, Clone, Copy)]
pub enum Dim {
    /// A dimension that varies, under this name.
    Named(&'static str),
    /// A dimension of this size.
    Sized(i64),
}

/// A graph taking `input_ids` and `attention_mask`, as every model the core runs does.
#[derive(Debug, Default)]
pub struct Graph {
    body: Vec<u8>,
    nodes: usize,
}

impl Graph {
    /// A graph with its two inputs and nothing else.
    #[must_use]
    pub fn new() -> Self {
        let mut graph = Self::default();
        let tokens = [Dim::Named("batch"), Dim::Named("tokens")];
        graph.input("input_ids", &tokens);
        graph.input("attention_mask", &tokens);
        graph
    }

    /// Another input of token-shaped integers, taken and never read.
    pub fn input(&mut self, name: &str, dims: &[Dim]) -> &mut Self {
        bytes(&mut self.body, 11, &value_info(name, INT64, dims));
        self
    }

    /// A float input `past`, given back unchanged as `present`, as a decoder's cache of keys and
    /// values is handed from one step to the next.
    pub fn passed(&mut self, past: &str, present: &str, dims: &[Dim]) -> &mut Self {
        bytes(&mut self.body, 11, &value_info(past, FLOAT, dims));
        self.node("Identity", &[past], present, &[]);
        bytes(&mut self.body, 12, &value_info(present, FLOAT, dims));
        self
    }

    /// `output`, shaped `[1, width]`: one at the length of `attention_mask` and nought elsewhere,
    /// so the likeliest next token is how many tokens the caller says were read.
    pub fn counted(&mut self, output: &str, width: i64) -> &mut Self {
        let (shape, second, length, depth, values) = (
            format!("{output}_shape"),
            format!("{output}_second"),
            format!("{output}_length"),
            format!("{output}_depth"),
            format!("{output}_values"),
        );
        self.node("Shape", &["attention_mask"], &shape, &[]);
        self.initializer(&second, INT64, &[1], &1_i64.to_le_bytes());
        self.node("Gather", &[&shape, &second], &length, &[]);
        self.initializer(&depth, INT64, &[1], &width.to_le_bytes());
        let raw: Vec<u8> = [0.0_f32, 1.0]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        self.initializer(&values, FLOAT, &[2], &raw);
        self.node("OneHot", &[&length, &depth, &values], output, &[]);
        bytes(
            &mut self.body,
            12,
            &value_info(output, FLOAT, &[Dim::Sized(1), Dim::Sized(width)]),
        );
        self
    }

    fn node(&mut self, op: &str, inputs: &[&str], output: &str, attributes: &[u8]) {
        let mut node = Vec::new();
        for input in inputs {
            bytes(&mut node, 1, input.as_bytes());
        }
        bytes(&mut node, 2, output.as_bytes());
        bytes(&mut node, 3, format!("node{}", self.nodes).as_bytes());
        bytes(&mut node, 4, op.as_bytes());
        if !attributes.is_empty() {
            bytes(&mut node, 5, attributes);
        }
        self.nodes += 1;
        bytes(&mut self.body, 1, &node);
    }

    fn initializer(&mut self, name: &str, element: i64, dims: &[i64], raw: &[u8]) {
        let mut tensor = Vec::new();
        for dim in dims {
            int(&mut tensor, 1, *dim);
        }
        int(&mut tensor, 2, element);
        bytes(&mut tensor, 8, name.as_bytes());
        bytes(&mut tensor, 9, raw);
        bytes(&mut self.body, 5, &tensor);
    }

    /// Writes `table` as a float initializer named `name`, a row to each token id.
    fn table(&mut self, name: &str, table: &[Vec<f32>]) {
        let raw: Vec<u8> = table
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let rows = i64::try_from(table.len()).unwrap();
        self.initializer(name, FLOAT, &[rows, width_of(table)], &raw);
    }

    /// `output`, shaped `[batch, tokens, width]`: row `id` of `table` for each token.
    pub fn per_token(&mut self, output: &str, table: &[Vec<f32>]) -> &mut Self {
        let name = format!("{output}_table");
        self.table(&name, table);
        self.node("Gather", &[&name, "input_ids"], output, &[]);
        let dims = [
            Dim::Named("batch"),
            Dim::Named("tokens"),
            Dim::Sized(width_of(table)),
        ];
        bytes(&mut self.body, 12, &value_info(output, FLOAT, &dims));
        self
    }

    /// `output`, shaped `[batch, width]`: the rows of `table` for each token, summed.
    pub fn summed(&mut self, output: &str, table: &[Vec<f32>]) -> &mut Self {
        let dims = [Dim::Named("batch"), Dim::Sized(width_of(table))];
        self.summed_into(output, table, &dims, None)
    }

    /// `output`, shaped `[batch, subjects, answers]`: the rows of `table`, each `subjects` times
    /// `answers` wide, summed over the tokens and folded.
    pub fn summed_folded(
        &mut self,
        output: &str,
        table: &[Vec<f32>],
        subjects: i64,
        answers: i64,
    ) -> &mut Self {
        let dims = [
            Dim::Named("batch"),
            Dim::Sized(subjects),
            Dim::Sized(answers),
        ];
        self.summed_into(output, table, &dims, Some([-1, subjects, answers]))
    }

    fn summed_into(
        &mut self,
        output: &str,
        table: &[Vec<f32>],
        dims: &[Dim],
        fold: Option<[i64; 3]>,
    ) -> &mut Self {
        let (table_name, looked_up, axes, summed) = (
            format!("{output}_table"),
            format!("{output}_per_token"),
            format!("{output}_axes"),
            format!("{output}_summed"),
        );
        self.table(&table_name, table);
        self.node("Gather", &[&table_name, "input_ids"], &looked_up, &[]);
        self.initializer(&axes, INT64, &[1], &1_i64.to_le_bytes());
        let mut keep_dims = Vec::new();
        bytes(&mut keep_dims, 1, b"keepdims");
        int(&mut keep_dims, 3, 0);
        int(&mut keep_dims, 20, 2);
        let reduced = if fold.is_some() {
            summed.as_str()
        } else {
            output
        };
        self.node("ReduceSum", &[&looked_up, &axes], reduced, &keep_dims);
        if let Some(shape) = fold {
            let shape_name = format!("{output}_shape");
            let raw: Vec<u8> = shape.iter().flat_map(|value| value.to_le_bytes()).collect();
            self.initializer(&shape_name, INT64, &[3], &raw);
            self.node("Reshape", &[&summed, &shape_name], output, &[]);
        }
        bytes(&mut self.body, 12, &value_info(output, FLOAT, dims));
        self
    }

    /// Written as a model of opset 17, which every runtime this crate links has.
    ///
    /// # Panics
    ///
    /// Panics when the file cannot be written.
    pub fn write(&self, path: &Path) {
        let mut graph = self.body.clone();
        bytes(&mut graph, 2, b"tiny");
        let mut opset = Vec::new();
        bytes(&mut opset, 1, b"");
        int(&mut opset, 2, 17);
        let mut model = Vec::new();
        int(&mut model, 1, 8);
        bytes(&mut model, 2, b"steamgauge tests");
        bytes(&mut model, 7, &graph);
        bytes(&mut model, 8, &opset);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, model).unwrap();
    }
}

/// A tokenizer of whole words, one id each in the order given, splitting on whitespace and
/// punctuation. Any word not given is `[UNK]`. `[PAD]` is id 0 and `[UNK]` id 1.
#[must_use]
pub fn word_tokenizer(words: &[&str]) -> serde_json::Value {
    let mut vocab = serde_json::Map::new();
    for (id, word) in ["[PAD]", "[UNK]"].iter().chain(words).enumerate() {
        vocab.insert((*word).to_owned(), id.into());
    }
    serde_json::json!({
        "version": "1.0",
        "truncation": null,
        "padding": null,
        "added_tokens": [],
        "pre_tokenizer": {"type": "Whitespace"},
        "post_processor": null,
        "decoder": null,
        "model": {"type": "WordLevel", "vocab": vocab, "unk_token": "[UNK]"},
    })
}

/// A tokenizer of single letters, each letter given its own id, that drops any letter it was
/// not given, as a tokenizer without an unknown token does.
#[must_use]
pub fn letter_tokenizer(letters: &str) -> serde_json::Value {
    let mut vocab = serde_json::Map::new();
    for (id, letter) in letters.chars().enumerate() {
        vocab.insert(letter.to_string(), id.into());
    }
    serde_json::json!({
        "version": "1.0",
        "truncation": null,
        "padding": null,
        "added_tokens": [],
        "pre_tokenizer": {"type": "Whitespace"},
        "post_processor": null,
        "decoder": null,
        "model": {"type": "BPE", "vocab": vocab, "merges": []},
    })
}

/// Writes `value` as JSON at `path`, making the folder first.
///
/// # Panics
///
/// Panics when the file cannot be written.
pub fn write_json(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

/// The words the tiny reader knows, after `[PAD]` and `[UNK]`.
pub const READER_WORDS: [&str; 7] = ["query", ":", "passage", "crashes", "pretty", "love", "hate"];

/// A reader's record as the export writes it, every field present, with `given` laid over
/// it: a reader of no subjects without lines, an aspect head or a held-out figure.
///
/// # Panics
///
/// Panics when `given` is not a JSON object.
#[must_use]
pub fn reader_record(given: &serde_json::Value) -> serde_json::Value {
    let mut record = serde_json::json!({
        "subjects": [], "threshold": 0.5, "thresholds": null, "language_thresholds": null,
        "max_tokens": 128, "max_batch": null, "context": false, "mark": false,
        "prefix": false, "headset_marker": false, "trained_from": "a backbone",
        "data_fingerprint": "labels-1", "lines_fingerprint": "lines-1", "run_id": "run-1",
        "name": "Test Reader", "usual_declined": null, "held_out": null,
    });
    for (key, value) in given.as_object().expect("fields") {
        record[key] = value.clone();
    }
    record
}

/// A tiny reader in `dir`, trained, as far as `reader.json` says, on `subjects` in the order given.
///
/// "crashes" argues for bugs and "pretty" for graphics, five each a time it appears, "love" for
/// praise and "hate" for complaint. `settings` is laid over its record.
///
/// # Panics
///
/// Panics when `subjects` lacks `bugs` or `graphics`, when `settings` is not a JSON object or
/// when a file cannot be written.
pub fn reader(dir: &Path, subjects: &[&str], settings: &serde_json::Value) {
    let class = |id: &str| subjects.iter().position(|subject| *subject == id).unwrap();
    let vocabulary = READER_WORDS.len() + 2;
    let mut subject = vec![vec![0.0; subjects.len()]; vocabulary];
    subject[2 + 3][class("bugs")] = 5.0;
    subject[2 + 4][class("graphics")] = 5.0;
    let mut polarity = vec![vec![0.0; 3]; vocabulary];
    polarity[2 + 5][0] = 5.0;
    polarity[2 + 6][1] = 5.0;
    Graph::new()
        .summed("subject_logits", &subject)
        .summed("polarity_logits", &polarity)
        .write(&dir.join("model.onnx"));
    write_json(&dir.join("tokenizer.json"), &word_tokenizer(&READER_WORDS));
    let mut record = reader_record(&serde_json::json!({
        "subjects": subjects,
        "max_tokens": 512,
    }));
    for (key, value) in settings.as_object().unwrap() {
        record[key] = value.clone();
    }
    write_json(&dir.join("reader.json"), &record);
}
