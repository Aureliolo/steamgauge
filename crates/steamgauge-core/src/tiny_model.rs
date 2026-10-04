//! Models small enough to write out in a test: an ONNX graph of a few tables looked up by token,
//! and a tokenizer of a few words, so the code that loads and runs a model is tested on CPU
//! with no model downloaded.
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
    Named(&'static str),
    Sized(i64),
}

/// A graph taking `input_ids` and `attention_mask`, as every model this crate runs does.
#[derive(Debug, Default)]
pub struct Graph {
    body: Vec<u8>,
    nodes: usize,
}

impl Graph {
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
/// punctuation; any word not given is `[UNK]`. `[PAD]` is id 0 and `[UNK]` id 1.
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

pub fn write_json(path: &Path, value: &serde_json::Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_graph_runs_and_gives_back_its_tables() {
        let dir = crate::tempdir::Dir::new();
        let path = dir.path().join("model.onnx");
        Graph::new()
            .per_token("hidden", &[vec![0.0, 0.0], vec![1.0, 2.0], vec![3.0, 4.0]])
            .summed("total", &[vec![0.0], vec![1.0], vec![10.0]])
            .summed_folded("folded", &[vec![0.0; 4], vec![1.0; 4], vec![2.0; 4]], 2, 2)
            .write(&path);
        let (mut session, device) = crate::model::session_at(&path).unwrap();
        assert_eq!(device, "cpu");
        let ids = ndarray::Array2::from_shape_vec((1, 3), vec![1_i64, 2, 2]).unwrap();
        let mask = ndarray::Array2::from_shape_vec((1, 3), vec![1_i64, 1, 1]).unwrap();
        let outputs = session
            .run(ort::inputs![
                "input_ids" => ort::value::Tensor::from_array(ids).unwrap(),
                "attention_mask" => ort::value::Tensor::from_array(mask).unwrap(),
            ])
            .unwrap();
        let hidden = outputs["hidden"].try_extract_array::<f32>().unwrap();
        assert_eq!(hidden.shape(), [1, 3, 2]);
        assert_eq!(
            hidden.iter().copied().collect::<Vec<_>>(),
            [1.0, 2.0, 3.0, 4.0, 3.0, 4.0]
        );
        let total = outputs["total"].try_extract_array::<f32>().unwrap();
        assert_eq!(total.iter().copied().collect::<Vec<_>>(), [21.0]);
        let folded = outputs["folded"].try_extract_array::<f32>().unwrap();
        assert_eq!(folded.shape(), [1, 2, 2]);
        assert_eq!(folded.iter().copied().collect::<Vec<_>>(), [5.0; 4]);
    }
}
