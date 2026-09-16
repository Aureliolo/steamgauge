"""What each language is actually promised, and what it is actually given.

The reader promises that a claim it answers is right three times in four. That promise is kept
on average and it is kept subject by subject, because the line is drawn per subject. Nobody has
asked whether it is kept language by language, and a promise kept on average is not a promise:
a Korean report and an English report print the same sentence about what the rates are worth.

Every policy here is fitted leave-one-game-out and applied to the game left out, so a language
that does well because one of its games is easy cannot fit its own line and then be marked on
it. That is the same discipline `confidence.py` uses and it is the reason these numbers are
lower than the ones the export prints, which fits on everything because it has to ship.

    python lines.py --oof <dir of cv-*.npz>
    python lines.py --oof <dir> --min-accuracy 0.75 --min-claims 200
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np

import claimdata
from confidence import answered_by, mondrian, most_coverage, softmax, pooled_folds
from language import wilson

HERE = Path(__file__).resolve().parent


def languages_of(paths, data):
    """Each out-of-fold claim's language, joined on the claim the labeller was shown.

    The fold files carry the review and claim index for exactly this: the logits know nothing
    about language and the label set knows nothing about the model, and the only honest way to
    put them side by side is the key they both hold.
    """
    parts = [np.load(path, allow_pickle=False) for path in sorted(paths)]
    review_ids = np.concatenate([part["review_id"] for part in parts])
    claim_index = np.concatenate([part["claim_index"] for part in parts])
    labelled = {
        (claim.review_id, claim.claim_index): claim for claim in claimdata.load(Path(data))
    }
    beside = [labelled.get((str(rid), int(at))) for rid, at in zip(review_ids, claim_index)]
    return np.array([one.language if one else "" for one in beside])


def per_game(score, correct, group, app_ids, floor, classes, joint=None):
    """Fit a rule on every game but one, apply it to that one, and pool the answers.

    `joint` is a second grouping whose line must also be cleared. Two promises made separately
    are not one promise made jointly, and a claim that clears its subject's bar while falling
    under its language's bar is exactly the claim this is here to catch.
    """
    answered = np.zeros(len(correct), dtype=bool)
    for game in sorted(set(app_ids.tolist())):
        held = app_ids == game
        fit = ~held
        if not fit.any():
            continue
        lines = mondrian(score[fit], group[fit], correct[fit], floor, classes)
        said = answered_by(lines, score[held], group[held])
        if joint is not None:
            other, names = joint
            second = mondrian(score[fit], other[fit], correct[fit], floor, names)
            said &= answered_by(second, score[held], other[held])
        answered[held] = said
    return answered


def one_line(score, correct, app_ids, floor):
    """The single shared threshold, fitted the same leave-one-game-out way."""
    answered = np.zeros(len(correct), dtype=bool)
    for game in sorted(set(app_ids.tolist())):
        held = app_ids == game
        line = most_coverage(score[~held], correct[~held], floor)
        if line is not None:
            answered[held] = score[held] >= line
    return answered


def report(name, answered, correct, languages, order, floor, min_claims):
    """One policy's coverage and delivered accuracy, language by language."""
    print(f"\n{name}")
    if answered.any():
        print(
            f"  overall: answers {100 * answered.mean():.1f}% at "
            f"{100 * correct[answered].mean():.1f}%"
        )
    else:
        print("  overall: silent")
    print(
        f"  {'language':12}{'claims':>8}{'answers':>10}{'delivered':>11}"
        f"{'95% interval':>20}{'against the promise':>22}"
    )
    worst = None
    for language in order:
        mine = languages == language
        total = int(mine.sum())
        said = mine & answered
        if not said.any():
            print(f"  {language:12}{total:>8,}{0.0:>9.1f}%{'silent':>11}")
            continue
        hits = int(correct[said].sum())
        rate = hits / said.sum()
        low, high = wilson(hits, int(said.sum()))
        gap = 100 * (rate - floor)
        thin = "   thin" if total < min_claims else ""
        print(
            f"  {language:12}{total:>8,}{100 * said.sum() / total:>9.1f}%{100 * rate:>10.1f}%"
            f"   [{100 * low:>5.1f}, {100 * high:>5.1f}]{gap:>+20.1f}{thin}"
        )
        if total >= min_claims and (worst is None or rate < worst[1]):
            worst = (language, rate)
    return worst


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oof", required=True, help="directory of cv-*.npz fold logits")
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument("--min-accuracy", type=float, default=0.75)
    parser.add_argument(
        "--min-claims",
        type=int,
        default=200,
        help="below this a language is shown but its delivered rate is not worth quoting",
    )
    args = parser.parse_args()

    paths = sorted(Path(args.oof).glob("*.npz"))
    logits, truth, app_ids, subjects = pooled_folds(paths)
    languages = languages_of(paths, args.data)

    probabilities = softmax(logits)
    predicted = probabilities.argmax(axis=1)
    score = probabilities.max(axis=1)
    correct = (predicted == truth).astype(float)

    known = languages != ""
    if not known.all():
        print(f"{int((~known).sum())} claims carry no language in the label set and are left out")
    logits, truth, app_ids = logits[known], truth[known], app_ids[known]
    predicted, score, correct = predicted[known], score[known], correct[known]
    languages = languages[known]

    names = sorted(set(languages.tolist()))
    index = np.array([names.index(one) for one in languages])
    order = sorted(names, key=lambda one: -int((languages == one).sum()))

    print(
        f"{len(truth):,} out-of-fold claims over {len(set(app_ids.tolist()))} games, "
        f"{len(names)} languages, promise {100 * args.min_accuracy:.0f}%"
    )

    policies = {
        "one line for everything": one_line(score, correct, app_ids, args.min_accuracy),
        "a line per subject (what ships)": per_game(
            score, correct, predicted, app_ids, args.min_accuracy, subjects
        ),
        "a line per language": per_game(
            score, correct, index, app_ids, args.min_accuracy, names
        ),
        "a line per subject and per language": per_game(
            score,
            correct,
            predicted,
            app_ids,
            args.min_accuracy,
            subjects,
            joint=(index, names),
        ),
    }

    worsts = {}
    for name, answered in policies.items():
        worsts[name] = report(
            name, answered, correct, languages, order, args.min_accuracy, args.min_claims
        )

    print("\nthe language the promise fails hardest for, under each policy:")
    for name, worst in worsts.items():
        if worst is None:
            print(f"  {name:36} no language has enough claims to say")
            continue
        language, rate = worst
        print(
            f"  {name:36} {language} at {100 * rate:.1f}%, "
            f"{100 * (rate - args.min_accuracy):+.1f} against the promise"
        )


if __name__ == "__main__":
    main()
