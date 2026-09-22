"""Decides the backbone by measurement rather than by reputation.

Every candidate sees the same claims, the same split and the same schedule, and is judged on
two things together: how well it reads a claim, and how fast it reads thirteen million of
them. A model two points better and three times slower is not better here.

The encoder this project already ships was chosen the same way, as an embedder. That is a
different job from classifying, which is why it does not get to keep the position unmeasured.

    python bakeoff.py
    python bakeoff.py --epochs 2 --only xlm-roberta-base
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

import torch
from transformers import AutoModel, AutoTokenizer

import train as trainer

HERE = Path(__file__).resolve().parent

# Multilingual, small enough to run over a corpus, and permissively licensed. Anything added
# here has to be exportable to ONNX, because the tool runs the graph and not PyTorch.
#
# The pooling is part of the candidate: a decoder reads under a causal mask, so only its last
# token has seen the whole claim, and asking it for a mean would measure the wrong thing.
CANDIDATES: dict[str, str] = {
    "xlm-roberta-base": "mean",
    "microsoft/mdeberta-v3-base": "mean",
    "intfloat/multilingual-e5-base": "mean",
    "Alibaba-NLP/gte-multilingual-base": "mean",
}

# The second round, cast against the encoder that ships rather than against each other. Each
# is a different bet about what matters: a modern recipe at half the size (mmBERT, 3T tokens
# over 1,800 languages on ModernBERT's architecture), sparsity (nomic's mixture of experts,
# 475M parameters of which 305M are active), a decoder used as an embedder (Qwen3), and a
# dense encoder of the same size as the one in place (EuroBERT).
LATER: dict[str, str] = {
    "intfloat/multilingual-e5-large-instruct": "mean",
    "jhu-clsp/mmBERT-base": "mean",
    "nomic-ai/nomic-embed-text-v2-moe": "mean",
    "Qwen/Qwen3-Embedding-0.6B": "last",
    "EuroBERT/EuroBERT-610m": "mean",
}


def throughput(backbone: str, batch: int = 64, length: int = 128, rounds: int = 12) -> float:
    """Claims per second on this machine, which is half of what decides the winner."""
    device = "cuda" if torch.cuda.is_available() else "cpu"
    tokenizer = AutoTokenizer.from_pretrained(backbone, trust_remote_code=True)
    model = AutoModel.from_pretrained(backbone, trust_remote_code=True).to(device).eval()
    text = ["The driving physics feel floaty and the handling in the rain is broken."] * batch
    encoded = tokenizer(
        text, truncation=True, max_length=length, padding="max_length", return_tensors="pt"
    ).to(device)

    with torch.no_grad():
        for _ in range(3):
            model(**encoded)
        if device == "cuda":
            torch.cuda.synchronize()
        started = time.time()
        for _ in range(rounds):
            model(**encoded)
        if device == "cuda":
            torch.cuda.synchronize()
    return (batch * rounds) / (time.time() - started)


def settings(passed: list[str]) -> argparse.Namespace:
    """The trainer's own defaults, with these flags on top.

    Built by handing the flags to `train.py`'s parser rather than by writing a Namespace here,
    because every knob added to the trainer since is a field this file would otherwise not
    have and `run` would stop at the first one it asked for.
    """
    held = sys.argv
    sys.argv = ["train.py", *passed]
    try:
        return trainer.parse()
    finally:
        sys.argv = held


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    # Five rather than three: at three every candidate was still climbing and the gaps
    # between them measured how fast each learns as much as how well.
    parser.add_argument("--epochs", type=int, default=5)
    parser.add_argument("--batch-size", type=int, default=32)
    parser.add_argument("--accumulate", type=int, default=1)
    parser.add_argument(
        "--only",
        nargs="*",
        default=None,
        help="candidates to run, by name. A name in neither list is run at mean pooling",
    )
    parser.add_argument(
        "--later",
        action="store_true",
        help="run the second round of candidates instead of the first",
    )
    parser.add_argument("--min-accuracy", type=float, default=0.75)
    parser.add_argument(
        "--context",
        action="store_true",
        help="read each claim inside a window of its review, which is what ships. Off by "
        "default because the first bake-off was run without it and the table is comparable",
    )
    args = parser.parse_args()

    pooling = {**CANDIDATES, **LATER}
    candidates: list[str] = args.only or list(LATER if args.later else CANDIDATES)
    results = []
    for backbone in candidates:
        print(f"\n=== {backbone} ===", flush=True)
        try:
            speed = throughput(backbone)
            record = trainer.run(
                settings(
                    [
                        "--data",
                        args.data,
                        "--backbone",
                        backbone,
                        "--epochs",
                        str(args.epochs),
                        "--batch-size",
                        str(args.batch_size),
                        "--accumulate",
                        str(args.accumulate),
                        "--max-length",
                        "128",
                        "--polarity-weight",
                        "0.5",
                        "--min-accuracy",
                        str(args.min_accuracy),
                        "--pooling",
                        pooling.get(backbone, "mean"),
                        "--run-id",
                        f"bakeoff-{backbone.replace('/', '-')}",
                        # The frozen games stay unread. Four candidates measured on them and
                        # then a winner reported from them is a winner reported by the thing
                        # that chose it.
                        "--no-frozen",
                        *(["--context"] if args.context else []),
                    ]
                )
            )
        except Exception as failure:  # noqa: BLE001 - a candidate that cannot be loaded is a result too
            print(f"  {backbone} failed: {failure}")
            results.append({"backbone": backbone, "error": str(failure)})
            continue
        record["claims_per_second"] = round(speed, 1)
        results.append(record)

    # The first round's table is what the shipped encoder was chosen by, so a later round
    # writes beside it rather than over it.
    table = HERE / "runs" / ("bakeoff-later.json" if args.later else "bakeoff.json")
    table.parent.mkdir(parents=True, exist_ok=True)
    table.write_text(json.dumps(results, indent=2), encoding="utf-8")

    print("\n" + "=" * 96)
    print(
        f"{'backbone':<38} {'macro F1':>9} {'accuracy':>9} {'AURC':>7} "
        f"{'answers':>8} {'at':>6} {'claims/s':>9}"
    )
    for record in results:
        if "error" in record:
            print(f"{record['backbone']:<38} {'failed':>9}")
            continue
        metrics = record["validation"]
        print(
            f"{record['backbone']:<38} {metrics['macro_f1']:>9.3f} "
            f"{metrics['accuracy']:>9.3f} {metrics.get('aurc', 0):>7.3f} "
            f"{metrics.get('threshold_coverage', 0):>7.0%} "
            f"{metrics.get('threshold_accuracy') or 0:>6.3f} "
            f"{record['claims_per_second']:>9.1f}"
        )

    print(
        "\nAURC is the area under the risk-coverage curve, lower is better, and it is the "
        "column\nto read after macro F1: it says whether a backbone knows when it does not "
        "know, which\nis the whole of what lets it abstain. `answers` is the share of claims "
        f"it will commit to\nat {args.min_accuracy:.0%} accuracy, and `at` is what it actually "
        "scores on those.\n\nEvery figure is from the validation games, which is what they are "
        "for. The frozen games\nwere not read: a set that chooses between models cannot also "
        "say how the chosen one does.\nTrain the winner on its own to get that number."
    )
    print(f"\nwritten to {table}")


if __name__ == "__main__":
    main()
