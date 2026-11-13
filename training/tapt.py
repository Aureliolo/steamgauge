"""Continues the encoder's pretraining on reviews before it is fine-tuned to read them.

The backbone learned its language on web text. Reviews have their own (frame drops, heists,
gacha, "skill issue") and their own shape. Masked-language pretraining on the corpus for a
pass moves the encoder towards that before a single label is spent, which is the one thing a
labelled set cannot buy. The reviews come from `steamgauge export-pool`, so they are from the
games the model trains on and never from a game it is scored on.

    python tapt.py --pool data/tapt.jsonl --run-id tapt-e5 --epochs 1
    python train.py --backbone runs/tapt-e5/backbone --context ...
"""

from __future__ import annotations

import argparse
import json
import math
import time
from pathlib import Path
from typing import TYPE_CHECKING

import torch
from torch.utils.data import DataLoader, Dataset
from transformers import (
    AutoModelForMaskedLM,
    AutoTokenizer,
    BatchEncoding,
    DataCollatorForLanguageModeling,
    get_linear_schedule_with_warmup,  # pyright: ignore[reportUnknownVariableType]  # transformers leaves num_training_steps unannotated
)

if TYPE_CHECKING:
    from transformers import PreTrainedTokenizerBase

import backbones
from typed_calls import manual_seed, save_pretrained

HERE = Path(__file__).resolve().parent


def reviews_in(path: Path) -> list[str]:
    """Each review once, however many claims of it the pool holds."""
    seen: set[tuple[int, str]] = set()
    texts: list[str] = []
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            row = json.loads(line)
            key = (int(row["app_id"]), str(row["review_id"]))
            if key in seen:
                continue
            seen.add(key)
            texts.append(row["review"])
    return texts


class Reviews(Dataset[BatchEncoding]):
    """Tokenised reviews for masked-language pretraining."""

    def __init__(
        self, texts: list[str], tokenizer: PreTrainedTokenizerBase, max_length: int
    ) -> None:
        """Hold the review texts and the tokenizer that cuts them to `max_length`."""
        self.texts = texts
        self.tokenizer = tokenizer
        self.max_length = max_length

    def __len__(self) -> int:
        """Return the number of reviews."""
        return len(self.texts)

    def __getitem__(self, at: int) -> BatchEncoding:
        """Return review `at` tokenised, with its special-token mask."""
        # Truncated from the end: a review's opening is where most of its language sits, and
        # the fine-tune windows every claim anyway.
        return self.tokenizer(
            self.texts[at],
            truncation=True,
            max_length=self.max_length,
            return_special_tokens_mask=True,
        )


def parse() -> argparse.Namespace:
    """Read the command line."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pool", default=str(HERE / "data" / "pool.jsonl"))
    parser.add_argument("--backbone", default="intfloat/multilingual-e5-large")
    parser.add_argument("--epochs", type=int, default=1)
    parser.add_argument("--batch-size", type=int, default=32)
    parser.add_argument("--learning-rate", type=float, default=2e-5)
    parser.add_argument("--max-length", type=int, default=256)
    parser.add_argument("--mask", type=float, default=0.15)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--run-id", required=True)
    return parser.parse_args()


def main() -> None:
    """Continue the backbone's pretraining on the pool's reviews and save the result."""
    args = parse()

    manual_seed(args.seed)
    device = "cuda" if torch.cuda.is_available() else "cpu"
    texts = reviews_in(Path(args.pool))
    print(f"{len(texts):,} reviews from {args.pool}")

    tokenizer = backbones.load(AutoTokenizer, args.backbone)
    # The encoder's checkpoint carries no language-model head, so the head starts from noise
    # and learns in the first few hundred steps. The encoder underneath is what is kept.
    model = backbones.load(AutoModelForMaskedLM, args.backbone)
    # Called on the base class because pyright misreads transformers' wrapper around `.to`.
    torch.nn.Module.to(model, device)
    collate = DataCollatorForLanguageModeling(tokenizer, mlm_probability=args.mask)
    loader = DataLoader(
        Reviews(texts, tokenizer, args.max_length),
        batch_size=args.batch_size,
        shuffle=True,
        collate_fn=collate,
        num_workers=0,
    )
    steps = len(loader) * args.epochs
    optimiser = torch.optim.AdamW(model.parameters(), lr=args.learning_rate, weight_decay=0.01)
    schedule = get_linear_schedule_with_warmup(optimiser, int(steps * 0.06), steps)

    started = time.time()
    taken = 0
    running = 0.0
    recent = 0.0
    for epoch in range(args.epochs):
        model.train()
        for batch in loader:
            moved = {name: tensor.to(device) for name, tensor in batch.items()}
            with torch.amp.autocast(device, enabled=device == "cuda", dtype=torch.bfloat16):
                loss = model(**moved).loss
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            optimiser.step()  # pyright: ignore[reportUnknownMemberType]  # torch leaves closure's type unknown
            schedule.step()
            optimiser.zero_grad(set_to_none=True)
            taken += 1
            running += float(loss.detach())
            if taken % 100 == 0:
                rate = taken / (time.time() - started)
                left = (steps - taken) / rate if rate else 0
                recent = running / 100
                print(
                    f"  epoch {epoch + 1:,} step {taken:,}/{steps:,} loss {recent:.3f} "
                    f"({left / 60:,.0f} min left)",
                    flush=True,
                )
                running = 0.0

    out = HERE / "runs" / args.run_id
    backbone = out / "backbone"
    backbone.mkdir(parents=True, exist_ok=True)
    # The whole masked-language model, so the next pass of this script can continue from it;
    # `AutoModel` takes the encoder out of it for the fine-tune.
    save_pretrained(model, backbone)
    save_pretrained(tokenizer, backbone)
    # Not `run.json`: the index reads every one of those as a fine-tune with scores, and this
    # has none. The fine-tune trained from this backbone is the run that scores.
    (out / "pretraining.json").write_text(
        json.dumps(
            {
                "pretraining": "masked language modelling",
                "from": args.backbone,
                "pool": args.pool,
                "reviews": len(texts),
                "epochs": args.epochs,
                "batch_size": args.batch_size,
                "learning_rate": args.learning_rate,
                "max_length": args.max_length,
                "mask": args.mask,
                "seed": args.seed,
                "steps": steps,
                "loss_last_100": recent,
                "perplexity_last_100": math.exp(recent),
                "seconds": round(time.time() - started),
                "device": device,
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    print(f"\nwritten to {out}")


if __name__ == "__main__":
    main()
