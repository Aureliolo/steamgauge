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

import numpy as np

import backbones

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
PRAISE = [
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
COMPLAINT = [
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
    return f"{value:,}"


def points(said) -> str:
    if not said:
        return PROMPT["none"]
    lines = []
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


def build(source: Path, out: Path, cache: Path):
    out.mkdir(parents=True, exist_ok=True)
    # The builder's own rounding, to nearest in blocks of 32. k_quant and mixed precision round
    # better, but both hold the whole graph in single precision beside the model, over 25 GB.
    subprocess.run(
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
            raise SystemExit(f"the builder wrote no {file} in {out}")


class Graph:
    """The export, asked a token at a time the way `writer.rs` asks it."""

    def __init__(self, path: Path, provider: str):
        import onnxruntime

        self.session = onnxruntime.InferenceSession(str(path / "model.onnx"), providers=[provider])
        inputs = {given.name: given for given in self.session.get_inputs()}
        self.pasts = sorted(name for name in inputs if name.startswith("past_key_values."))
        first = inputs[self.pasts[0]]
        # The graph leaves the head's width unnamed. The builder's configuration says it, and
        # `writer.rs` holds it as a constant of the graph it pins.
        decoder = json.loads((path / "genai_config.json").read_text(encoding="utf-8"))
        self.heads = first.shape[1]
        self.width = decoder["model"]["decoder"]["head_size"]
        self.dtype = np.float16 if "float16" in first.type else np.float32
        self.outputs = [made.name for made in self.session.get_outputs()]

    def start(self):
        return {
            name: np.zeros((1, self.heads, 0, self.width), self.dtype) for name in self.pasts
        }, 0

    def step(self, ids: list[int], cache, seen: int):
        feed = {
            "input_ids": np.array([ids], dtype=np.int64),
            "attention_mask": np.ones((1, seen + len(ids)), dtype=np.int64),
            **cache,
        }
        made = dict(zip(self.outputs, self.session.run(None, feed), strict=True))
        cache = {name: made[name.replace("past_key_values", "present")] for name in self.pasts}
        return made["logits"][0, -1].astype(np.float32), cache, seen + len(ids)

    def greedy(self, prompt: list[int]) -> list[int]:
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
        cache, seen = self.start()
        logits, cache, seen = self.step(prompt, cache, seen)
        agreed = 0
        for token in answer:
            agreed += int(np.argmax(logits)) == token
            logits, cache, seen = self.step([token], cache, seen)
        return agreed / max(len(answer), 1)


def full_answer(source: Path, prompt: list[int]) -> list[int]:
    import torch
    from transformers import AutoModelForCausalLM

    model = backbones.load(AutoModelForCausalLM, source, dtype=torch.bfloat16).eval()
    with torch.no_grad():
        made = model.generate(
            torch.tensor([prompt]),
            attention_mask=torch.ones((1, len(prompt)), dtype=torch.long),
            max_new_tokens=MOST_TOKENS,
            do_sample=False,
            eos_token_id=list(STOPS),
        )[0, len(prompt) :].tolist()
    return [token for token in made if token not in STOPS]


def main():
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

    from tokenizers import Tokenizer

    tokenizer = Tokenizer.from_file(str(out / "tokenizer.json"))
    text = prompt_text()
    prompt = tokenizer.encode(text, add_special_tokens=False).ids
    print(f"prompt: {len(prompt)} tokens")

    wanted = full_answer(source, prompt)
    print(f"the model writes:\n{tokenizer.decode(wanted)}\n")
    graph = Graph(out, "CPUExecutionProvider")
    agreement = graph.agreement(prompt, wanted)
    print(f"the export picks the model's next token at {agreement:.1%} of {len(wanted)} places")
    started = time.perf_counter()
    answer = graph.greedy(prompt)
    seconds = time.perf_counter() - started
    print(f"the export writes, in {seconds:.1f} s on the processor:\n{tokenizer.decode(answer)}\n")
    if agreement < MIN_AGREEMENT:
        raise SystemExit("the export does not write what the model writes. Not shipping it.")
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
