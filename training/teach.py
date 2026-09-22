"""Reads an unlabelled pool with an ensemble of saved runs and writes their averaged answers,
as the soft targets a student learns from beside the labels.

Three seeds of one configuration disagree on a tenth of what they read, and the average of
their distributions is a better reader than any one of them; what a student trained on that
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

import numpy as np
import torch
from torch.utils.data import DataLoader
from transformers import AutoTokenizer

import claimdata
from train import HERE, ClaimReader, Pool


@torch.no_grad()
def answers_of(run: Path, pool, device, batch_size):
    """One saved run's distributions over the pool, in the pool's order."""
    record = json.loads((run / "run.json").read_text(encoding="utf-8"))
    tokenizer = AutoTokenizer.from_pretrained(run / "tokenizer")
    tokenizer.padding_side = "right"
    model = ClaimReader(record["backbone"], len(record["subjects"]), pooling=record.get("pooling", "mean"))
    model.load_state_dict(torch.load(run / "model.bin", map_location="cpu", weights_only=True))
    model.to(device).eval()
    loader = DataLoader(
        Pool(
            pool,
            tokenizer,
            record["subjects"],
            record["max_length"],
            record["context"],
            record.get("mark", False),
            record.get("prefix", False),
        ),
        batch_size=batch_size,
        shuffle=False,
        num_workers=0,
    )
    subjects, polarities = [], []
    started = time.time()
    for done, batch in enumerate(loader, 1):
        with torch.amp.autocast(device, enabled=device == "cuda", dtype=torch.bfloat16):
            subject, polarity, _ = model(
                batch["input_ids"].to(device), batch["attention_mask"].to(device)
            )
        subjects.append(torch.softmax(subject.float(), dim=-1).cpu())
        polarities.append(torch.softmax(polarity.float(), dim=-1).cpu())
        if done % 200 == 0:
            rate = done * batch_size / (time.time() - started)
            print(f"  {run.name}: {done * batch_size} claims, {rate:.0f}/s", flush=True)
    return record, torch.cat(subjects).numpy(), torch.cat(polarities).numpy()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", nargs="+", required=True, help="run ids under runs/, each saved with --save")
    parser.add_argument("--pool", default=str(HERE / "data" / "pool.jsonl"))
    parser.add_argument("--to", default=str(HERE / "data" / "pool-targets.npz"))
    parser.add_argument("--batch-size", type=int, default=128)
    args = parser.parse_args()

    device = "cuda" if torch.cuda.is_available() else "cpu"
    pool = claimdata.load_pool(args.pool)
    print(f"{len(pool)} claims in the pool, read by {len(args.runs)} teachers on {device}")

    subjects: list[str] = []
    answered, judged = [], []
    for name in args.runs:
        record, subject, polarity = answers_of(HERE / "runs" / name, pool, device, args.batch_size)
        if not subjects:
            subjects = record["subjects"]
        elif record["subjects"] != subjects:
            raise SystemExit(f"{name} answers over different subjects from {args.runs[0]}")
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
