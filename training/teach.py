"""Read an unlabelled pool with an ensemble of saved runs and write their averaged answers.

The averaged answers are the soft targets a student learns from beside the labels.

Three seeds of one configuration disagree on a tenth of what they read, and the average of
their distributions is a better reader than any one of them. What a student trained on that
average over a quarter of a million claims inherits is the ensemble's reading at one model's
cost. The pool comes from `steamgauge export-pool`, and every row is named, so the answers can
be checked against the pool they answer.

    python teach.py --runs e5-29006-ema-s1 e5-29006-ema-s2 e5-29006-ema-s3
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import TYPE_CHECKING, Any

import numpy as np
import torch
from torch.utils.data import DataLoader
from transformers import AutoTokenizer

import backbones
import claimdata
from train import HERE, ClaimReader, Pool, Reading, TrunkLoad

if TYPE_CHECKING:
    import numpy.typing as npt

# How many batches pass between progress lines.
PROGRESS_EVERY = 200


@torch.no_grad()
def answers_of(
    run: Path, pool: list[claimdata.Claim], device: str, batch_size: int
) -> tuple[dict[str, Any], npt.NDArray[np.float32], npt.NDArray[np.float32]]:
    """Return one saved run's record and its distributions over the pool, in the pool's order."""
    record: dict[str, Any] = json.loads((run / "run.json").read_text(encoding="utf-8"))
    tokenizer = backbones.load(AutoTokenizer, run / "tokenizer")
    tokenizer.padding_side = "right"
    model = ClaimReader(
        record["backbone"],
        len(record["subjects"]),
        pooling=record.get("pooling", "mean"),
        # A teacher trained in bf16 is read in bf16: the same weights in fp32 are twice the
        # card, and a 4B one would not fit at all.
        load=TrunkLoad(dtype=torch.bfloat16 if record.get("dtype") == "bfloat16" else None),
    )
    model.load_state_dict(torch.load(run / "model.bin", map_location="cpu", weights_only=True))
    model.to(device).eval()
    loader = DataLoader(
        Pool(
            pool,
            tokenizer,
            record["subjects"],
            Reading.recorded(record, record["max_length"]),
        ),
        batch_size=batch_size,
        shuffle=False,
        num_workers=0,
    )
    subjects: list[torch.Tensor] = []
    polarities: list[torch.Tensor] = []
    started = time.time()
    for done, batch in enumerate(loader, 1):
        with torch.amp.autocast(device, enabled=device == "cuda", dtype=torch.bfloat16):
            subject, polarity, _ = model(
                batch["input_ids"].to(device), batch["attention_mask"].to(device)
            )
        subjects.append(torch.softmax(subject.float(), dim=-1).cpu())
        polarities.append(torch.softmax(polarity.float(), dim=-1).cpu())
        if done % PROGRESS_EVERY == 0:
            rate = done * batch_size / (time.time() - started)
            print(f"  {run.name}: {done * batch_size:,} claims, {rate:,.0f}/s", flush=True)
    return record, torch.cat(subjects).numpy(), torch.cat(polarities).numpy()


def main() -> None:
    """Read the pool with each teacher run and save the averaged distributions."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--runs", nargs="+", required=True, help="run ids under runs/, each saved with --save"
    )
    parser.add_argument("--pool", default=str(HERE / "data" / "pool.jsonl"))
    parser.add_argument("--to", default=str(HERE / "data" / "pool-targets.npz"))
    parser.add_argument("--batch-size", type=int, default=128)
    args = parser.parse_args()

    device = "cuda" if torch.cuda.is_available() else "cpu"
    pool = claimdata.load_pool(args.pool)
    print(f"{len(pool):,} claims in the pool, read by {len(args.runs):,} teachers on {device}")

    subjects: list[str] = []
    answered: list[npt.NDArray[np.float32]] = []
    judged: list[npt.NDArray[np.float32]] = []
    for name in args.runs:
        record, subject, polarity = answers_of(HERE / "runs" / name, pool, device, args.batch_size)
        if not subjects:
            subjects = record["subjects"]
        elif record["subjects"] != subjects:
            msg = f"{name} answers over different subjects from {args.runs[0]}"
            raise SystemExit(msg)
        answered.append(subject)
        judged.append(polarity)
        print(f"{name}: read; sure of its answer at {subject.max(axis=1).mean():.3f} on average")

    subject_mean = np.mean(answered, axis=0)
    polarity_mean = np.mean(judged, axis=0)
    favourites = np.array([answer.argmax(axis=1) for answer in answered])
    unanimous = float(np.mean(np.all(favourites == favourites[0], axis=0)))
    print(
        f"teachers agree on {unanimous:.1%} of claims; the average is sure of its answer at "
        f"{subject_mean.max(axis=1).mean():.3f} on average"
    )

    out = Path(args.to)
    out.parent.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(
        out,
        subject=subject_mean.astype(np.float16),
        polarity=polarity_mean.astype(np.float16),
        app_id=np.array([claim.app_id for claim in pool]),
        review_id=np.array([claim.review_id for claim in pool]),
        claim_index=np.array([claim.claim_index for claim in pool]),
        subjects=np.array(subjects),
        teachers=np.array(args.runs),
        unanimous=np.array(unanimous),
    )
    print(f"written to {out}")


if __name__ == "__main__":
    main()
