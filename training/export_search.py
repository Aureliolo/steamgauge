"""Exports the two models search by meaning runs, and checks each against the model it came from.

Measured on 1,029 judged results over three games and fourteen searches, the
Qwen3 embedding and reranker at 0.6B found what was asked for in 77% of their first ten, where
the encoder the search first shipped with found 64% and put up four times as many opposites.
Neither has a full-precision ONNX export of the shape the app runs: the embedding's carries a
text generator's cache as fifty-six inputs, and the reranker's is quantised only. So both are
exported here the way the reader is, through `LengthFree`, into graphs that take the tokens and
the mask and give back only what is used:

- `search-encoder`: each text's vector, taken at its last real token and normalised.
- `search-reranker`: how surely the claim says what was searched, the model's "yes" against its
  "no" at the last token. The two rows of the output layer are all that is kept of it.

Both in half precision, checked on the processor against the full-precision model and then
asked on DirectML, where they run.

    python export_search.py --to ../models --data data/claims.jsonl
"""

from __future__ import annotations

import argparse
import json
import os
import random
import shutil
from pathlib import Path
from typing import TYPE_CHECKING, Protocol, cast

import numpy as np
import torch
from transformers import AutoModel, AutoModelForCausalLM, AutoTokenizer

import backbones
from export import LengthFree, load_graph, save_graph
from typed_calls import save_pretrained

if TYPE_CHECKING:
    import numpy.typing as npt
    from transformers import PreTrainedModel, PreTrainedTokenizerBase

# How many texts go through a model or graph at once while checking, and how many it is traced on.
CHECK_BATCH = 16
TRACE_CLAIMS = 4
OPSET = 17

HERE = Path(__file__).resolve().parent
PROMPTS = json.loads(
    (HERE.parent / "crates" / "steamgauge-core" / "src" / "search-prompts.json").read_text(
        encoding="utf-8"
    )
)
ENCODER = "Qwen/Qwen3-Embedding-0.6B"
RERANKER = "Qwen/Qwen3-Reranker-0.6B"
# What a pair of prompt and claim is cut to. The prompt alone is about sixty tokens.
RERANK_TOKENS = 320
ENCODE_TOKENS = 512

# A vector drifts in every one of its thousand numbers at once, so it is held to its direction:
# the cosine to the full-precision vector. A score drifts as one number between 0 and 1.
MIN_COSINE = 0.999
MAX_SCORE_DRIFT = 0.02
# DirectML's half-precision kernels drift further and unevenly (export.py, HALF_TOLERANCE); what
# they may not do is change which claims come first.
MIN_COSINE_WHERE_RUN = 0.995
MAX_SCORE_DRIFT_WHERE_RUN = 0.06


def query_text(query: str) -> str:
    """Return `query` wrapped in the encoder's instruction prompt."""
    return PROMPTS["query"].format(instruction=PROMPTS["instruction"], query=query)


def rerank_text(query: str, claim: str) -> str:
    """Return the reranker's prompt for `claim` against `query`."""
    return PROMPTS["reranker"].format(instruction=PROMPTS["instruction"], query=query, claim=claim)


def last_real(hidden: torch.Tensor, attention_mask: torch.Tensor) -> torch.Tensor:
    """Return each row's vector at its last real token, right padding assumed as `LengthFree`."""
    at = attention_mask.sum(dim=1) - 1
    return hidden[torch.arange(hidden.shape[0]), at]


class CausalReranker(Protocol):
    """The two parts of a causal language model the reranker keeps."""

    model: PreTrainedModel
    lm_head: torch.nn.Linear


class SearchEncoder(torch.nn.Module):
    """The embedding model, giving each text's normalised vector at its last real token."""

    def __init__(self, trunk: PreTrainedModel) -> None:
        """Wrap `trunk` so its mask and positions are built from tensors."""
        super().__init__()
        # A cache of past keys is for writing text. Traced, it would be a graph output nobody
        # reads.
        trunk.config.use_cache = False
        self.trunk = LengthFree(trunk)

    def forward(self, input_ids: torch.Tensor, attention_mask: torch.Tensor) -> torch.Tensor:
        """Return the unit-length vector of each text."""
        hidden = self.trunk(input_ids, attention_mask).last_hidden_state
        pooled = last_real(hidden, attention_mask).float()
        return torch.nn.functional.normalize(pooled, dim=-1)


class SearchReranker(torch.nn.Module):
    """The reranker, keeping only the output layer's "no" and "yes" rows."""

    def __init__(self, causal: CausalReranker, no: int, yes: int) -> None:
        """Wrap `causal`'s trunk and keep the output rows for token ids `no` and `yes`."""
        super().__init__()
        causal.model.config.use_cache = False
        self.trunk = LengthFree(causal.model)
        self.head = torch.nn.Linear(causal.lm_head.weight.shape[1], 2, bias=False)
        with torch.no_grad():
            self.head.weight.copy_(causal.lm_head.weight[[no, yes]])

    def forward(self, input_ids: torch.Tensor, attention_mask: torch.Tensor) -> torch.Tensor:
        """Return how surely each text says "yes" rather than "no" at its last token."""
        hidden = self.trunk(input_ids, attention_mask).last_hidden_state
        pair = self.head(last_real(hidden, attention_mask)).float()
        return torch.softmax(pair, dim=-1)[:, 1]


def encoded(
    tokenizer: PreTrainedTokenizerBase, texts: list[str], limit: int
) -> tuple[torch.Tensor, torch.Tensor]:
    """Return `texts` as right-padded token ids and attention mask, cut to `limit` tokens."""
    tokenizer.padding_side = "right"
    batch = tokenizer(texts, truncation=True, max_length=limit, padding=True, return_tensors="pt")
    return cast("torch.Tensor", batch["input_ids"]), cast("torch.Tensor", batch["attention_mask"])


@torch.no_grad()
def answers(
    model: torch.nn.Module, ids: torch.Tensor, mask: torch.Tensor
) -> npt.NDArray[np.float32]:
    """Return what `model` gives for every row, a batch at a time."""
    return torch.cat(
        [
            model(ids[at : at + CHECK_BATCH], mask[at : at + CHECK_BATCH])
            for at in range(0, len(ids), CHECK_BATCH)
        ]
    ).numpy()


def export(
    model: torch.nn.Module, ids: torch.Tensor, mask: torch.Tensor, out: Path, output: str
) -> None:
    """Trace `model` and write it as one `model.onnx` in `out`, its single output named `output`."""
    traced = out / "traced"
    shutil.rmtree(traced, ignore_errors=True)
    traced.mkdir(parents=True)
    torch.onnx.export(  # pyright: ignore[reportUnknownMemberType]  # torch leaves ScriptFunction generics unknown
        model,
        (ids[:TRACE_CLAIMS], mask[:TRACE_CLAIMS]),
        os.fspath(traced / "model.onnx"),
        input_names=["input_ids", "attention_mask"],
        output_names=[output],
        dynamic_axes={
            "input_ids": {0: "batch", 1: "tokens"},
            "attention_mask": {0: "batch", 1: "tokens"},
            output: {0: "batch"},
        },
        opset_version=OPSET,
        dynamo=False,
    )
    # One file, so one pin covers it: both are well under the 2 GB a protobuf holds.
    save_graph(load_graph(traced / "model.onnx", weights=True), out / "model.onnx")
    shutil.rmtree(traced)


def asked_on(
    path: Path,
    provider: str,
    ids: npt.NDArray[np.int64],
    mask: npt.NDArray[np.int64],
    output: str,
) -> npt.NDArray[np.float32]:
    """Return what the graph at `path` gives for every row when run on `provider`."""
    import onnxruntime  # noqa: PLC0415 heavy optional dependency, loaded only for the check

    session = onnxruntime.InferenceSession(str(path), providers=[provider])
    print(f"  asked on {session.get_providers()[0]}")
    got: list[npt.NDArray[np.float32]] = []
    for at in range(0, len(ids), CHECK_BATCH):
        width = int(mask[at : at + CHECK_BATCH].sum(axis=1).max())
        got.append(
            session.run(
                [output],
                {
                    "input_ids": ids[at : at + CHECK_BATCH, :width],
                    "attention_mask": mask[at : at + CHECK_BATCH, :width],
                },
            )[0]
        )
    return np.concatenate(got).astype(np.float32)


def sample(data: Path, count: int) -> list[str]:
    """Return `count` claim texts from `data`, in an order fixed by a seed."""
    claims = [json.loads(line)["text"] for line in data.open(encoding="utf-8")]
    random.Random(1).shuffle(claims)  # noqa: S311 a fixed seed makes the sample repeatable, no security use
    return claims[:count]


QUERIES = [
    "boring",
    "fun",
    "controller support",
    "steam deck",
    "desync",
    "too expensive",
    "crashes",
    "grindy",
    "servers",
    "waste of money",
    "masterpiece",
    "tutorial",
    "campaign",
    "matchmaking",
    "story",
    "performance",
]


def token_id(tokenizer: PreTrainedTokenizerBase, token: str) -> int:
    """Return the vocabulary id of `token`."""
    # transformers types this method as a union that includes non-callables.
    return int(tokenizer.convert_tokens_to_ids(token))  # pyright: ignore[reportCallIssue, reportOptionalCall, reportUnknownMemberType, reportUnknownArgumentType]


def save_tokenizer(tokenizer: PreTrainedTokenizerBase, out: Path) -> None:
    """Leave `tokenizer.json` in `out`, the one file of the saved tokenizer the app reads."""
    save_pretrained(tokenizer, out / "tokenizer")
    shutil.copy(out / "tokenizer" / "tokenizer.json", out / "tokenizer.json")
    shutil.rmtree(out / "tokenizer")


def main() -> None:
    """Export the encoder, the reranker or both, as the command line asks."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--to", required=True, help="where search-encoder and search-reranker go")
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument("--check", type=int, default=256)
    parser.add_argument(
        "--only",
        choices=["encoder", "reranker"],
        default=None,
        help="export one of the two. Each in a process of its own holds only its own model: in "
        "one process the reranker's export ran out of room beside what the encoder's had left.",
    )
    args = parser.parse_args()
    to = Path(args.to)
    claims = sample(Path(args.data), args.check)
    if args.only != "reranker":
        export_encoder(to, claims, args.check)
    if args.only != "encoder":
        export_reranker(to, claims)
    print(f"written to {to}")


def export_encoder(to: Path, claims: list[str], check: int) -> None:
    """Export the encoder, asked about claims as they are and about searches by instruction."""
    out = to / "search-encoder"
    out.mkdir(parents=True, exist_ok=True)
    tokenizer = backbones.load(AutoTokenizer, ENCODER)
    texts = claims[: check - len(QUERIES)] + [query_text(q) for q in QUERIES]
    ids, mask = encoded(tokenizer, texts, ENCODE_TOKENS)
    model = SearchEncoder(backbones.load(AutoModel, ENCODER, dtype=torch.float32)).eval()
    wanted = answers(model, ids, mask)
    export(model.half(), ids, mask, out, "vector")
    del model
    save_tokenizer(tokenizer, out)
    for provider, floor in (
        ("CPUExecutionProvider", MIN_COSINE),
        ("DmlExecutionProvider", MIN_COSINE_WHERE_RUN),
    ):
        got = asked_on(out / "model.onnx", provider, ids.numpy(), mask.numpy(), "vector")
        cosine = np.sum(got * wanted, axis=1)
        print(
            f"encoder on {provider}: cosine to the model {cosine.mean():.5f} mean, "
            f"{cosine.min():.5f} lowest"
        )
        if cosine.min() < floor:
            msg = f"the encoder's graph is not the model on {provider}. Not shipping it."
            raise SystemExit(msg)


def export_reranker(to: Path, claims: list[str]) -> None:
    """Export the reranker, asked about every search against claims, the pairs spread over both."""
    out = to / "search-reranker"
    out.mkdir(parents=True, exist_ok=True)
    tokenizer = backbones.load(AutoTokenizer, RERANKER)
    no, yes = token_id(tokenizer, "no"), token_id(tokenizer, "yes")
    pairs = [rerank_text(QUERIES[n % len(QUERIES)], claim) for n, claim in enumerate(claims)]
    ids, mask = encoded(tokenizer, pairs, RERANK_TOKENS)
    causal = cast(
        "CausalReranker", backbones.load(AutoModelForCausalLM, RERANKER, dtype=torch.float32)
    )
    model = SearchReranker(causal, no, yes).eval()
    del causal
    wanted = answers(model, ids, mask)
    export(model.half(), ids, mask, out, "score")
    del model
    save_tokenizer(tokenizer, out)
    for provider, limit in (
        ("CPUExecutionProvider", MAX_SCORE_DRIFT),
        ("DmlExecutionProvider", MAX_SCORE_DRIFT_WHERE_RUN),
    ):
        got = asked_on(out / "model.onnx", provider, ids.numpy(), mask.numpy(), "score")
        drift = np.abs(got - wanted)
        print(
            f"reranker on {provider}: score drift {drift.mean():.4f} mean, {drift.max():.4f} most"
        )
        if drift.max() > limit:
            msg = f"the reranker's graph is not the model on {provider}. Not shipping it."
            raise SystemExit(msg)


if __name__ == "__main__":
    main()
