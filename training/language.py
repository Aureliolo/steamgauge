"""How well does the reader read each language, and on how much evidence?

The frozen games answer this for English and almost nobody else: they hold 4,017 English claims
and 55 Japanese ones, so the Japanese interval is wide enough to be compatible with the reader
being fine and with it being ten points worse. Meanwhile the library those games stand for is
barely half English, and three of its games are led by a language that is not.

The cross-validation folds answer it far better and cost nothing, because they already exist.
Every non-frozen game is held out by exactly one fold, so pooling them gives roughly thirty
thousand claims answered by a model that never trained on the game they came from, and the
language of each one is in the label set beside it.

This is not the frozen-game figure and does not replace it. It is the same out-of-fold evidence
the shipped abstention lines are drawn from, asked a question nobody had asked it.

    python language.py --oof <dir of cv-*.npz>
    python language.py --oof <dir> --min-claims 50
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path

import numpy as np

import claimdata
from confidence import pooled_folds, softmax

HERE = Path(__file__).resolve().parent


def wilson(hits: int, total: int, z: float = 1.959963985) -> tuple[float, float]:
    """The interval a rate from this many claims is entitled to claim.

    The same one the Rust side prints beside every rate, so a figure here and a figure on a
    report page cannot disagree about how sure they are.
    """
    if not total:
        return (0.0, 1.0)
    p = hits / total
    n = float(total)
    denominator = 1.0 + z * z / n
    centre = p + z * z / (2.0 * n)
    spread = z * math.sqrt(p * (1.0 - p) / n + z * z / (4.0 * n * n))
    return max(0.0, (centre - spread) / denominator), min(1.0, (centre + spread) / denominator)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oof", required=True, help="directory of cv-*.npz fold logits")
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument(
        "--min-claims",
        type=int,
        default=100,
        help="below this a language is listed but its rate is not worth reading",
    )
    args = parser.parse_args()

    logits, truth, _, subjects = pooled_folds(Path(args.oof).glob("*.npz"))
    parts = [np.load(path, allow_pickle=False) for path in sorted(Path(args.oof).glob("*.npz"))]
    review_ids = np.concatenate([part["review_id"] for part in parts])
    claim_index = np.concatenate([part["claim_index"] for part in parts])

    # The language lives with the label, not with the logits, so the two are joined on the claim
    # the labeller was actually shown. The labeller's own doubt comes across with it, because a
    # language the model reads badly and a language the labeller was unsure in look identical in
    # an agreement figure and are not the same problem at all.
    labelled = {
        (claim.review_id, claim.claim_index): claim for claim in claimdata.load(Path(args.data))
    }
    beside = [labelled.get((str(rid), int(at))) for rid, at in zip(review_ids, claim_index)]
    languages = np.array([one.language if one else "" for one in beside])
    # What the labeller was sure of: not contested, and not hedged. Scoring the reader against a
    # label its author doubted measures the doubt as much as the reading.
    settled = np.array(
        [bool(one and not one.ambiguous and one.confidence == "high") for one in beside]
    )
    missing = int((languages == "").sum())

    probabilities = softmax(logits)
    predicted = probabilities.argmax(axis=1)
    correct = predicted == truth

    print(f"{len(truth):,} out-of-fold claims over {len(subjects)} subjects")
    if missing:
        print(f"  {missing} carry no language in the label set and are left out")
    print(
        f"  {int(settled.sum()):,} were labelled without hedging "
        f"({100 * settled.mean():.0f}%), which is the second column"
    )
    print(
        f"\n{'language':12}{'claims':>8}{'share':>8}{'agreement':>11}{'95% interval':>20}"
        f"{'settled only':>15}{'hedged':>9}"
    )

    whole = int((languages != "").sum())
    rows = []
    for name in sorted(set(languages.tolist())):
        if not name:
            continue
        mine = languages == name
        total = int(mine.sum())
        hits = int(correct[mine].sum())
        low, high = wilson(hits, total)
        sure = mine & settled
        clean = correct[sure].mean() if sure.any() else float("nan")
        rows.append((total, name, hits / total, low, high, clean, 1.0 - sure.sum() / total))

    for total, name, rate, low, high, clean, hedged in sorted(rows, reverse=True):
        thin = "" if total >= args.min_claims else "   thin"
        print(
            f"{name:12}{total:>8,}{100 * total / whole:>7.1f}%{100 * rate:>10.1f}%"
            f"   [{100 * low:>5.1f}, {100 * high:>5.1f}]{100 * clean:>14.1f}%"
            f"{100 * hedged:>8.0f}%{thin}"
        )

    enough = [row for row in rows if row[0] >= args.min_claims]
    print(
        f"\n{len(enough)} of {len(rows)} languages have {args.min_claims} claims or more; "
        f"the rest are listed for completeness and should not be quoted"
    )
    english = next((row for row in enough if row[1] == "english"), None)
    if not english:
        return
    worst = min(enough, key=lambda row: row[2])
    best = max(enough, key=lambda row: row[2])
    print(
        f"against english at {100 * english[2]:.1f}%, the measured spread runs from "
        f"{worst[1]} at {100 * worst[2]:.1f}% to {best[1]} at {100 * best[2]:.1f}%"
    )
    # A gap that shrinks once the hedged claims are dropped was partly the labelling; one that
    # survives is the reading. They want different work and the difference is not visible in an
    # agreement figure, which is the whole reason this column exists.
    print("\nhow much of each gap survives on the claims the labeller was sure of:")
    for _, name, rate, _, _, clean, _ in sorted(enough, key=lambda row: row[2]):
        if name == "english":
            continue
        whole_gap = 100 * (rate - english[2])
        settled_gap = 100 * (clean - english[5])
        if whole_gap >= 0:
            continue
        kept = settled_gap / whole_gap if whole_gap else 0.0
        print(
            f"  {name:11}{whole_gap:>6.1f} points against english, {settled_gap:>5.1f} once "
            f"hedged claims are dropped ({100 * kept:.0f}% of it survives)"
        )


if __name__ == "__main__":
    main()
