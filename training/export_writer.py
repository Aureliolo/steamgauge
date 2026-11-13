"""Exports the model that writes a game's paragraph, and checks the export against the model.

The writer is Qwen3-4B-Instruct-2507: it answers without reasoning first, follows a written
brief closely, and is among the least inventive summarisers of its size. onnxruntime-genai's
model builder exports it with int4 weights for the processor's kernels, in one graph that takes
the tokens, the mask and the cache of past keys and values. The app decodes from it itself, on
the processor, so nothing of onnxruntime-genai ships.

The check asks the full-precision model to write the paragraph for a fixed set of points, then
feeds that paragraph to the export a token at a time and requires the export's likeliest next
token to be the model's own at most places along it. It then records the export's own
greedy answer to the same prompt, which `writer.rs` has to give token for token.

uv makes the environment from the lines below, because the builder needs the processor build of
onnxruntime, and that cannot sit beside the DirectML build the rest of training/ uses. Python 3.13,
because 3.14's argparse checks every help string as it is added and the builder's own has a
stray "%" in it, so on 3.14 its command line dies before it reads an argument:

    uv run training/export_writer.py --to ../models
"""

# /// script
# requires-python = ">=3.13,<3.14"
# dependencies = [
#     "onnxruntime-genai>=0.18",
#     "torch>=2.13",
#     "transformers>=4.57.2",
#     "huggingface_hub>=0.25",
#     "onnx>=1.22",
#     "onnx-ir>=0.1",
#     "numpy>=1.26",
# ]
# ///

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import TYPE_CHECKING, Protocol, cast

import numpy as np
import numpy.typing as npt

import backbones

if TYPE_CHECKING:
    from torch import Tensor

# One point of the brief: its label, the reviews behind it, its subject, the other subjects it
# also covers, and a quote.
Point = tuple[str, int, str, list[str], str]

# Past keys and values by input name, as the graph is fed them.
Cache = dict[str, npt.NDArray[np.generic]]


class Generates(Protocol):
    """The text generation of a transformers causal language model."""

    def generate(
        self,
        inputs: Tensor,
        *,
        attention_mask: Tensor,
        max_new_tokens: int,
        do_sample: bool,
        eos_token_id: list[int],
    ) -> Tensor:
        """Return `inputs` followed by the tokens the model writes."""
        ...


class Encoding(Protocol):
    """A tokenised text."""

    ids: list[int]


class WriterTokenizer(Protocol):
    """The part of a `tokenizers` tokenizer the check uses."""

    def encode(self, sequence: str, *, add_special_tokens: bool) -> Encoding:
        """Return `sequence` as token ids."""
        ...

    def decode(self, ids: list[int]) -> str:
        """Return the text of token ids."""
        ...


HERE = Path(__file__).resolve().parent
CORE = HERE.parent / "crates" / "steamgauge-core" / "src"
PROMPT = json.loads((CORE / "writer-prompt.json").read_text(encoding="utf-8"))
REFERENCE = CORE / "writer-reference.json"

WRITER = "Qwen/Qwen3-4B-Instruct-2507"
# The files the app fetches. The builder writes a configuration for its own runtime beside them.
SHIPPED = ["model.onnx", "model.onnx.data", "tokenizer.json"]

# `<|im_end|>` closes the answer. `<|endoftext|>` is what the model ends on without one.
STOPS = (151645, 151643)
MOST_TOKENS = 240
# The places along the full model's paragraph where the export's likeliest next token is the
# model's own. Int4 weights move close choices between words that mean the same ("praise" or
# "love", "often cited" or "frequently criticised"), and one moved word moves the wording after
# it: an export at 84% here writes the model's points, every one and no other, in its own words.
# The floor stops an export whose rounding went wrong, which loses the model's words altogether.
MIN_AGREEMENT = 0.75

# Invented, so the reference rests on no library: the shape the app writes, not anyone's game.
GAME = "Lantern Keep"
REVIEWS = 14_200
PRAISE: list[Point] = [
    (
        "Mixing two factions every run keeps it fresh.",
        1_840,
        "Gameplay and mechanics",
        [],
        "Mixing two factions every run keeps it fresh after a hundred hours.",
    ),
    (
        "The art is gorgeous.",
        920,
        "Graphics and art",
        [],
        "The hand-painted art is gorgeous in every biome.",
    ),
    ("Great value for the price.", 610, "Price and value", [], "Easily worth twice what I paid."),
    (
        "Runs well on the Steam Deck.",
        300,
        "Performance",
        ["Controls and interface"],
        "Runs well on the Steam Deck with no tweaking.",
    ),
]
COMPLAINT: list[Point] = [
    (
        "The late game drags.",
        700,
        "Pacing",
        ["Content and length"],
        "The late game drags once every relic is unlocked.",
    ),
    (
        "Black screen on startup.",
        210,
        "Bugs and crashes",
        ["Performance"],
        "Black screen on startup until I alt-tab.",
    ),
    (
        "The tutorial explains too little.",
        150,
        "Tutorial and onboarding",
        [],
        "The tutorial explains too little about synergies.",
    ),
]


def thousands(value: int) -> str:
    """Return `value` with thousands separators."""
    return f"{value:,}"


def points(said: list[Point]) -> str:
    """Return the brief's lines for `said`, or its wording for no points."""
    if not said:
        return PROMPT["none"]
    lines: list[str] = []
    for label, reviews, subject, also, quote in said:
        lines.append(
            PROMPT["point"].format(
                label=label,
                reviews=thousands(reviews),
                subject=subject,
                also=PROMPT["also"].format(subjects=", ".join(also)) if also else "",
                quote=quote,
            )
        )
    return "\n".join(lines)


def prompt_text() -> str:
    """Return the chat prompt for the fixed set of points."""
    user = PROMPT["user"].format(
        game=GAME,
        reviews=thousands(REVIEWS),
        praise=points(PRAISE),
        complaint=points(COMPLAINT),
    )
    return (
        f"<|im_start|>system\n{PROMPT['system']}<|im_end|>\n"
        f"<|im_start|>user\n{user}<|im_end|>\n"
        "<|im_start|>assistant\n"
    )


def build(source: Path, out: Path, cache: Path) -> None:
    """Export the model at `source` to int4 graph files in `out`, using `cache` as scratch."""
    out.mkdir(parents=True, exist_ok=True)
    # The builder's own rounding, to nearest in blocks of 32. k_quant and mixed precision round
    # better, but both hold the whole graph in single precision beside the model, over 25 GB.
    subprocess.run(  # noqa: S603 the interpreter and builder module are fixed, no shell is involved
        [
            sys.executable,
            "-m",
            "onnxruntime_genai.models.builder",
            "-i",
            str(source),
            "-o",
            str(out),
            "-p",
            "int4",
            "-e",
            "cpu",
            "-c",
            str(cache),
        ],
        check=True,
    )
    for file in SHIPPED:
        if not (out / file).is_file():
            msg = f"the builder wrote no {file} in {out}"
            raise SystemExit(msg)


class Graph:
    """The export, asked a token at a time the way `writer.rs` asks it."""

    def __init__(self, path: Path, provider: str) -> None:
        """Open the graph in `path` on `provider`."""
        # The processor build of onnxruntime exists only in the environment uv makes for this
        # script, so it loads when the check opens a graph.
        import onnxruntime  # noqa: PLC0415 heavy optional dependency, loaded only for the check

        self.session = onnxruntime.InferenceSession(str(path / "model.onnx"), providers=[provider])
        inputs = {given.name: given for given in self.session.get_inputs()}
        self.pasts = sorted(name for name in inputs if name.startswith("past_key_values."))
        first = inputs[self.pasts[0]]
        # The graph leaves the head's width unnamed. The builder's configuration says it, and
        # `writer.rs` holds it as a constant of the graph it pins.
        decoder = json.loads((path / "genai_config.json").read_text(encoding="utf-8"))
        heads = first.shape[1]
        if not isinstance(heads, int):
            msg = f"the graph names its key-value heads {heads!r}, not a count"
            raise TypeError(msg)
        self.heads = heads
        self.width: int = decoder["model"]["decoder"]["head_size"]
        self.dtype: type[np.float16 | np.float32] = (
            np.float16 if "float16" in first.type else np.float32
        )
        self.outputs = [made.name for made in self.session.get_outputs()]

    def start(self) -> tuple[Cache, int]:
        """Return an empty cache and the count of tokens it holds."""
        return {
            name: np.zeros((1, self.heads, 0, self.width), self.dtype) for name in self.pasts
        }, 0

    def step(
        self, ids: list[int], cache: Cache, seen: int
    ) -> tuple[npt.NDArray[np.float32], Cache, int]:
        """Feed `ids` after `seen` tokens, giving the next-token logits, the cache and the count."""
        feed: Cache = {
            "input_ids": np.array([ids], dtype=np.int64),
            "attention_mask": np.ones((1, seen + len(ids)), dtype=np.int64),
            **cache,
        }
        made = dict(zip(self.outputs, self.session.run(None, feed), strict=True))
        cache = {name: made[name.replace("past_key_values", "present")] for name in self.pasts}
        return made["logits"][0, -1].astype(np.float32), cache, seen + len(ids)

    def greedy(self, prompt: list[int]) -> list[int]:
        """Return the export's own greedy answer to `prompt`, as token ids."""
        cache, seen = self.start()
        logits, cache, seen = self.step(prompt, cache, seen)
        answer: list[int] = []
        while len(answer) < MOST_TOKENS:
            token = int(np.argmax(logits))
            if token in STOPS:
                break
            answer.append(token)
            logits, cache, seen = self.step([token], cache, seen)
        return answer

    def agreement(self, prompt: list[int], answer: list[int]) -> float:
        """Return the share of places along `answer` where the export's likeliest token matches."""
        cache, seen = self.start()
        logits, cache, seen = self.step(prompt, cache, seen)
        agreed = 0
        for token in answer:
            agreed += int(np.argmax(logits)) == token
            logits, cache, seen = self.step([token], cache, seen)
        return agreed / max(len(answer), 1)


def full_answer(source: Path, prompt: list[int]) -> list[int]:
    """Return the full-precision model's greedy answer to `prompt`, as token ids."""
    # Heavy libraries the export step never needs, so they load only for the reference answer.
    import torch  # noqa: PLC0415 heavy optional dependency, loaded only for the reference answer
    from transformers import AutoModelForCausalLM  # noqa: PLC0415 same reason

    model = cast(
        "Generates", backbones.load(AutoModelForCausalLM, source, dtype=torch.bfloat16).eval()
    )
    with torch.no_grad():
        written = model.generate(
            torch.tensor([prompt]),
            attention_mask=torch.ones((1, len(prompt)), dtype=torch.long),
            max_new_tokens=MOST_TOKENS,
            do_sample=False,
            eos_token_id=list(STOPS),
        )[0, len(prompt) :]
    made = cast("list[int]", written.tolist())  # pyright: ignore[reportUnknownMemberType]  # torch types tolist as a bare list
    return [token for token in made if token not in STOPS]


def main() -> None:
    """Export the writer, check it against the model and record its reference answer."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--to", required=True, help="where `writer` goes")
    parser.add_argument(
        "--source",
        help="the model's files already on disk at the pinned commit; fetched when not given",
    )
    parser.add_argument(
        "--checked-only",
        action="store_true",
        help="check the export already in --to rather than building it again, as after the "
        "brief in writer-prompt.json changes",
    )
    args = parser.parse_args()
    out = Path(args.to) / "writer"
    cache = Path(args.to) / "writer-build"

    source = backbones.snapshot(args.source or WRITER)
    if not args.checked_only:
        build(source, out, cache)
        shutil.rmtree(cache, ignore_errors=True)

    from tokenizers import Tokenizer  # noqa: PLC0415 only the checking half needs it

    tokenizer = cast(
        "WriterTokenizer",
        Tokenizer.from_file(str(out / "tokenizer.json")),  # pyright: ignore[reportUnknownMemberType]  # tokenizers' stubs leave from_file untyped
    )
    text = prompt_text()
    prompt = tokenizer.encode(text, add_special_tokens=False).ids
    print(f"prompt: {len(prompt):,} tokens")

    wanted = full_answer(source, prompt)
    print(f"the model writes:\n{tokenizer.decode(wanted)}\n")
    graph = Graph(out, "CPUExecutionProvider")
    agreement = graph.agreement(prompt, wanted)
    print(f"the export picks the model's next token at {agreement:.1%} of {len(wanted):,} places")
    started = time.perf_counter()
    answer = graph.greedy(prompt)
    seconds = time.perf_counter() - started
    print(f"the export writes, in {seconds:.1f} s on the processor:\n{tokenizer.decode(answer)}\n")
    if agreement < MIN_AGREEMENT:
        msg = "the export does not write what the model writes. Not shipping it."
        raise SystemExit(msg)
    REFERENCE.write_text(
        json.dumps(
            {"prompt": text, "prompt_ids": prompt, "answer_ids": answer},
            ensure_ascii=False,
            indent=1,
        )
        + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"written to {out}, and the reference to {REFERENCE.relative_to(HERE.parent)}")


if __name__ == "__main__":
    main()
