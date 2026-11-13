"""Whether shifting the logits by the class priors is worth what it costs.

Twenty-six subjects, and `verdict` is a fifth of the labels where `licensing` is under two in a
thousand. A model trained on that learns the prior along with the task and answers `verdict`
when it is unsure, which is what the per-subject figures show. Weighting the rare subjects up
in the loss was tried three times here, at exponents 0.3, 0.5 and 0.7, and made macro F1 worse
each time.

Logit adjustment is the same correction applied in the right place. Train with plain
cross-entropy, then at inference subtract `tau * log(prior)` from each logit: the model's
estimate of `P(subject | claim)` is divided by the prior it absorbed, which is what Bayes says
to do and what reweighting only approximates (Menon et al., ICLR 2021, arXiv:2007.07314). It
costs one subtraction and no retraining.

**The reason to expect it to hurt as well as help.** It deliberately makes rare-subject
predictions more confident, and this project's objective is coverage at a fixed accuracy, which
rewards a confidence score that ranks correctness well rather than one that is brave. Promoting
rare-subject logits promotes the wrong ones too. So both numbers are reported and neither is
allowed to stand for the other: macro F1 is what this is supposed to move, coverage at the
promise is what the product is.

Read from the cross-validation folds, cross-fitted by game: the prior and the tau come from the
other games, never from the game being scored.

    python adjust.py --oof <dir of fold logits>
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import TYPE_CHECKING, TypedDict

import numpy as np

from confidence import augrc, most_coverage, pooled_folds, softmax
from train import macro_f1

if TYPE_CHECKING:
    from collections.abc import Mapping

    from confidence import FloatArray, IntArray
    from train import ClassScore

HERE = Path(__file__).resolve().parent

TAUS = (0.0, 0.1, 0.25, 0.5, 0.75, 1.0)

# The prior of a subject absent from a fit fold, in counts.
PRIOR_FLOOR = 0.5
SUBJECTS_SHOWN = 8


class TauResult(TypedDict):
    """The figures reported for one value of tau."""

    macro_f1: float
    accuracy: float
    augrc: float
    coverage: float
    coverage_accuracy: float
    per_subject: Mapping[str, ClassScore]


def priors_of(truth: IntArray, classes: int) -> FloatArray:
    """Return each subject's share of the labels.

    A floor keeps a subject absent from a fit fold from sending its logit to negative infinity.
    """
    counts = np.bincount(truth, minlength=classes).astype(float)
    return np.maximum(counts, PRIOR_FLOOR) / max(counts.sum(), 1.0)


def adjusted(logits: FloatArray, priors: FloatArray, tau: float) -> FloatArray:
    """Return the logits with the prior divided out of them, to the degree `tau` asks for."""
    return logits - tau * np.log(priors)[None, :]


def scored_at(logits: FloatArray, priors: FloatArray, tau: float) -> tuple[IntArray, FloatArray]:
    """Return the predicted subject and its probability once the prior is adjusted out."""
    probabilities = softmax(adjusted(logits, priors, tau))
    return probabilities.argmax(axis=1), probabilities.max(axis=1)


def main() -> None:
    """Report macro F1 and coverage at the promise for each value of tau."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oof", required=True, help="directory of .npz files from --save-logits")
    parser.add_argument("--min-accuracy", type=float, default=0.75)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    logits, truth, app_ids, subjects = pooled_folds(Path(args.oof).glob("*.npz"))
    games = sorted(set(app_ids.tolist()))
    print(f"{len(truth):,} out-of-fold claims over {len(games):,} games\n")

    found: dict[str, TauResult] = {}
    print(f"{'tau':>5} {'macro F1':>9} {'accuracy':>9} {'AUGRC':>7}   answers at the promise")
    for tau in TAUS:
        # Every figure is leave-one-game-out: the prior and the threshold are fitted on the
        # other games, so a subject that is common in this game cannot set its own correction.
        predicted = np.zeros(len(truth), dtype=int)
        confidence = np.zeros(len(truth))
        answered = np.zeros(len(truth), dtype=bool)
        for game in games:
            held = app_ids == game
            fit = ~held
            priors = priors_of(truth[fit], len(subjects))
            predicted[held], confidence[held] = scored_at(logits[held], priors, tau)
            fit_predicted, fit_confidence = scored_at(logits[fit], priors, tau)
            line = most_coverage(
                fit_confidence, (fit_predicted == truth[fit]).astype(float), args.min_accuracy
            )
            if line is not None:
                answered[held] = confidence[held] >= line

        correct = (predicted == truth).astype(float)
        macro, per_class = macro_f1(truth, predicted, subjects)
        coverage = float(answered.mean())
        accuracy = float(correct[answered].mean()) if answered.any() else float("nan")
        found[str(tau)] = TauResult(
            macro_f1=macro,
            accuracy=float(correct.mean()),
            augrc=augrc(confidence, correct),
            coverage=coverage,
            coverage_accuracy=accuracy,
            per_subject=per_class,
        )
        print(
            f"{tau:>5.2f} {macro:>9.3f} {correct.mean():>9.3f} "
            f"{found[str(tau)]['augrc']:>7.4f}   {coverage:>6.1%} at {accuracy:.3f}"
        )

    best_macro = max(found, key=lambda tau: found[tau]["macro_f1"])
    best_coverage = max(found, key=lambda tau: found[tau]["coverage"])
    print(f"\nmacro F1 wants tau {best_macro}, coverage at the promise wants tau {best_coverage}")

    # The starved subjects are the ones this is for, so they are shown rather than summarised:
    # a macro F1 that moved because `verdict` moved would be the wrong win.
    starved = sorted(subjects, key=lambda name: found["0.0"]["per_subject"][name]["support"])[
        :SUBJECTS_SHOWN
    ]
    print(f"\n{'subject':<16} {'labels':>7}   " + "   ".join(f"tau {tau:g}" for tau in TAUS))
    for name in starved:
        row = [f"{found[str(tau)]['per_subject'][name]['f1']:>7.3f}" for tau in TAUS]
        support = found["0.0"]["per_subject"][name]["support"]
        print(f"{name:<16} {support:>7,}   " + "  ".join(row))

    if args.out:
        Path(args.out).write_text(json.dumps(found, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
