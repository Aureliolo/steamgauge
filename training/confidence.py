"""How the reader decides what not to answer, and whether a better rule exists.

Two questions, and they are separate. **What to score** an answer's confidence by: the reader
uses the largest softmax probability, which is the obvious choice and not usually the best
one. **Where to put the line**: the reader uses one threshold for all twenty-six subjects,
which over-abstains on the subjects it reads well and under-abstains on the ones it does not.
Neither costs anything to change. Both are decided here, on validation games, never on the
held-out ones.

Every figure is cross-fitted by game. A threshold fitted and reported on the same claims
flatters itself, and a threshold fitted on one game and reported on another is the only kind
whose number survives contact with a game nobody has seen. So each game is scored by a rule
fitted on the others, the held-out answers are pooled, and the interval around them resamples
whole games rather than claims, because claims from one game are not independent.

    python confidence.py --model ../models/game-review-reader
    python confidence.py --oof <dir of fold logits>

The second form is the one to trust. Four validation games give intervals eight points wide,
which is wider than every difference this study is asked to decide. Five cross-validation folds
(`train.py --fold`) give an out-of-fold answer for all twenty-eight games outside the held-out
set at the cost of five training runs and no test set spent.

The ranking is read with AUGRC, not AURC. AURC divides by how much was answered, so it mixes
the ordering quality of the score with the accuracy of the classifier underneath it, and
Traub et al. (arXiv:2407.01032) found that switching to AUGRC moved the ranking on five of
six datasets. AUGRC divides by the whole set instead and reads as the share of all claims
that are answered and wrong: the failures the abstention did not catch.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

import numpy as np
import numpy.typing as npt
import onnxruntime

import claimdata
from frontier import READ_BATCH, encode_pair, open_tokenizer
from train import Claims, Reading, ShippedTokenizer

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence

HERE = Path(__file__).resolve().parent

FloatArray = npt.NDArray[np.floating[Any]]
IntArray = npt.NDArray[np.integer[Any]]
BoolArray = npt.NDArray[np.bool_]
# One threshold per predicted class, or None where no threshold keeps the promise.
Thresholds = dict[int, float | None]
Interval = tuple[float, float]

# Below this many calibration predictions a class cannot support a threshold of its own: the
# quantile would be fitted on a handful of claims and would not survive the next game. Those
# classes share one, which is the clustered form of the same construction.
LONELY = 40

P_GRID = (0.3, 0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0)


def logits_of(
    model_dir: Path, claims: Sequence[claimdata.Claim], provenance: dict[str, Any]
) -> FloatArray:
    """The shipped reader's subject logits for every claim, run on the processor."""
    loaded, tokenizer = open_tokenizer(model_dir)

    budget = provenance["max_tokens"]
    cut = Claims(
        claims,
        ShippedTokenizer(loaded),
        provenance["subjects"],
        Reading.recorded(provenance, budget),
    )

    session = onnxruntime.InferenceSession(
        str(model_dir / "model.onnx"), providers=["CPUExecutionProvider"]
    )
    names = [out.name for out in session.get_outputs()]

    found: list[FloatArray] = []
    for at in range(0, len(claims), READ_BATCH):
        chunk = range(at, min(at + READ_BATCH, len(claims)))
        encoded = [encode_pair(tokenizer, cut.pair(i)) for i in chunk]
        longest = max(min(len(token_ids), budget) for token_ids, _ in encoded)
        ids = np.zeros((len(encoded), longest), dtype=np.int64)
        mask = np.zeros((len(encoded), longest), dtype=np.int64)
        for row, (token_ids, attention) in enumerate(encoded):
            keep = token_ids[:longest]
            ids[row, : len(keep)] = keep
            mask[row, : len(keep)] = attention[:longest]
        found.append(session.run(names, {"input_ids": ids, "attention_mask": mask})[0])
    return np.concatenate(found)


def softmax(logits: FloatArray) -> FloatArray:
    """Row-wise softmax of `logits`."""
    shifted = logits - logits.max(axis=1, keepdims=True)
    exp = np.exp(shifted)
    return exp / exp.sum(axis=1, keepdims=True)


def p_norm_max_logit(logits: FloatArray, p: float) -> FloatArray:
    """The logit vector divided by its p-norm, then the largest entry.

    Cattelan and Silva (arXiv:2305.15508) ran this across 84 pretrained classifiers and found
    a good many of them have confidence estimators that are simply broken: the ordering is far
    worse than the accuracy implies, and normalising the logits fixes it outright. Softmax is
    the p to infinity case of the same family, so the grid contains the incumbent and can only
    find it if it is genuinely the best of them.
    """
    norm = np.power(np.power(np.abs(logits), p).sum(axis=1, keepdims=True), 1.0 / p)
    return (logits / np.maximum(norm, 1e-12)).max(axis=1)


def scores_of(logits: FloatArray) -> dict[str, FloatArray]:
    """Every confidence score that costs nothing beyond the forward pass already done."""
    probabilities = softmax(logits)
    ordered = np.sort(probabilities, axis=1)
    found: dict[str, FloatArray] = {
        "max probability": probabilities.max(axis=1),
        "margin over the runner-up": ordered[:, -1] - ordered[:, -2],
        "negative entropy": -(-probabilities * np.log(probabilities + 1e-12)).sum(axis=1),
        "max logit": logits.max(axis=1),
    }
    for p in P_GRID:
        found[f"max logit over its {p:g}-norm"] = p_norm_max_logit(logits, p)
    return found


def augrc(confidence: FloatArray, correct: FloatArray) -> float:
    """The share of all claims that are answered and wrong, averaged over every coverage.

    Lower is better, and unlike AURC it does not divide by how much was answered, so the
    low-coverage end of the sweep, where a handful of claims decide the number, cannot
    dominate it.
    """
    ranked = correct[np.argsort(-confidence)]
    undetected = np.cumsum(1.0 - ranked) / len(ranked)
    return float(undetected.mean()) if len(ranked) else 0.0


def aurc(confidence: FloatArray, correct: FloatArray) -> float:
    """Selective risk averaged over every coverage. Kept for comparison with AUGRC."""
    ranked = correct[np.argsort(-confidence)]
    if not len(ranked):
        return 0.0
    sizes = np.arange(1, len(ranked) + 1)
    return float((1.0 - np.cumsum(ranked) / sizes).mean())


def cut_points(score: FloatArray) -> IntArray:
    """The sizes at which a threshold can actually stop, given ties in the score.

    A threshold is a number, not a rank, so two claims scoring the same are answered or
    declined together. Choosing a rank that falls inside a tie would report a coverage no
    threshold can produce.
    """
    ordered = np.sort(score)[::-1]
    boundary = np.flatnonzero(ordered[:-1] > ordered[1:]) + 1
    return np.append(boundary, len(ordered))


def most_coverage(score: FloatArray, correct: FloatArray, floor: float) -> float | None:
    """The lowest threshold whose answers are right at least `floor` of the time.

    Returns the threshold and nothing else: what it is worth is measured on other games. When
    no threshold reaches the floor there is no honest answer to give, and `None` says so
    rather than a number that quietly does not keep the promise.
    """
    if not len(score):
        return None
    order = np.argsort(-score)
    ranked_score, ranked_correct = score[order], correct[order]
    sizes = cut_points(score)
    accuracy = np.cumsum(ranked_correct)[sizes - 1] / sizes
    meeting = sizes[accuracy >= floor]
    if not len(meeting):
        return None
    return float(ranked_score[meeting.max() - 1])


def mondrian(
    score: FloatArray,
    predicted: IntArray,
    correct: FloatArray,
    floor: float,
    classes: Sequence[str],
) -> Thresholds:
    """One threshold per subject, and one shared by the subjects too rare to fit their own.

    The reader is not equally reliable across subjects: a `verdict` prediction at 0.6 is worth
    more than a `vr` prediction at 0.6, and a single line drawn through both answers too
    little of the first and too much of the second. Each subject's line is therefore drawn
    against its own claims.

    The floor is held per subject rather than overall on purpose. The rule that maximises
    overall coverage under one overall floor is to answer the head and abstain on the entire
    tail, which would be a metric win and a product failure: the rare complaints are the ones
    worth finding. A per-subject floor cannot buy coverage that way.
    """
    thresholds: Thresholds = {}
    lonely: list[int] = []
    for index in range(len(classes)):
        mine = predicted == index
        if mine.sum() < LONELY:
            lonely.append(index)
            continue
        thresholds[index] = most_coverage(score[mine], correct[mine], floor)

    pooled = np.isin(predicted, lonely)
    shared = most_coverage(score[pooled], correct[pooled], floor) if pooled.any() else None
    for index in lonely:
        thresholds[index] = shared
    return thresholds


def answered_by(thresholds: Thresholds, score: FloatArray, predicted: IntArray) -> BoolArray:
    """Which claims a per-subject rule answers. A subject with no threshold answers nothing."""
    line = np.array(
        [
            thresholds.get(int(index)) if thresholds.get(int(index)) is not None else np.inf
            for index in predicted
        ]
    )
    return score >= line


@dataclass(frozen=True)
class Answers:
    """What the reader got right or wrong, claim by claim, and which game each claim is from."""

    correct: FloatArray
    predicted: IntArray
    app_ids: IntArray


def held_out_by_game(
    logits: FloatArray,
    answers: Answers,
    floor: float,
    classes: Sequence[str],
) -> tuple[dict[str, FloatArray], dict[str, dict[str, BoolArray]]]:
    """Fit every rule on the other games, apply it to this one, and pool what comes back.

    Four validation games means four fits. It is not many, and that is the honest size of the
    thing: a fifth game would be worth more here than any amount of arithmetic.
    """
    correct, predicted, app_ids = answers.correct, answers.predicted, answers.app_ids
    games = sorted(set(app_ids))
    scored = scores_of(logits)
    pooled: dict[str, dict[str, BoolArray]] = {}

    for name, score in scored.items():
        answered_global = np.zeros(len(correct), dtype=bool)
        answered_class = np.zeros(len(correct), dtype=bool)
        for game in games:
            held = app_ids == game
            fit = ~held
            one = most_coverage(score[fit], correct[fit], floor)
            if one is not None:
                answered_global[held] = score[held] >= one
            per_class = mondrian(score[fit], predicted[fit], correct[fit], floor, classes)
            answered_class[held] = answered_by(per_class, score[held], predicted[held])
        pooled[name] = {"one line": answered_global, "a line per subject": answered_class}
    return scored, pooled


def share(row: Mapping[str, Any]) -> str:
    """One subject's share of answers and how right they were, or that it went silent."""
    if not row["answered"]:
        return "silent"
    return f"{row['coverage']:.0%} at {row['accuracy']:.2f}"


def by_subject(
    answered: BoolArray, predicted: IntArray, correct: FloatArray, classes: Sequence[str]
) -> dict[str, dict[str, Any]]:
    """What each policy does subject by subject, which is where the trade becomes visible.

    A per-subject floor cannot buy coverage by abandoning the tail, but it can still silence a
    subject outright: if no threshold makes that subject's predictions right three times in
    four, the honest answer is that it has none, and it stops answering. Whether that has
    happened, and to which subjects, is the thing a coverage average hides.
    """
    rows: dict[str, dict[str, Any]] = {}
    for index, name in enumerate(classes):
        mine = predicted == index
        if not mine.any():
            continue
        said = answered & mine
        rows[name] = {
            "predictions": int(mine.sum()),
            "answered": int(said.sum()),
            "coverage": float(said.sum() / mine.sum()),
            "accuracy": float(correct[said].mean()) if said.any() else None,
        }
    return rows


def game_interval(
    answered: BoolArray,
    correct: FloatArray,
    app_ids: IntArray,
    draws: int,
    rng: np.random.Generator,
) -> tuple[Interval, Interval]:
    """A percentile interval that resamples whole games, because claims are not independent.

    Resampling claims would treat 935 claims from one game as 935 independent observations
    and report an interval several times tighter than the four games behind it can support.
    """
    games = sorted(set(app_ids))
    where = {game: np.flatnonzero(app_ids == game) for game in games}
    coverages: list[float] = []
    accuracies: list[float] = []
    for _ in range(draws):
        drawn = np.concatenate([where[game] for game in rng.choice(games, len(games))])
        said = answered[drawn]
        coverages.append(said.mean())
        accuracies.append(float(correct[drawn][said].mean()) if said.any() else float("nan"))

    def span(values: list[float]) -> Interval:
        return float(np.nanpercentile(values, 2.5)), float(np.nanpercentile(values, 97.5))

    return span(coverages), span(accuracies)


def pooled_folds(paths: Iterable[Path]) -> tuple[FloatArray, IntArray, IntArray, list[str]]:
    """Every fold's held-out logits, checked for a game answered by a model that trained on it.

    The check is the point of pooling them here rather than concatenating them by hand. A game
    in two folds means one of those folds trained on it, and its answers would be a model
    marking its own homework in a table that says otherwise.
    """
    parts = [np.load(path, allow_pickle=False) for path in sorted(paths)]
    if not parts:
        msg = "no fold logits found"
        raise SystemExit(msg)
    subjects = [str(name) for name in parts[0]["subjects"]]
    for part in parts[1:]:
        if [str(name) for name in part["subjects"]] != subjects:
            msg = "the folds do not agree on the subjects, so they cannot be pooled"
            raise SystemExit(msg)

    seen: dict[int, int] = {}
    for at, part in enumerate(parts):
        for game in set(part["app_id"].tolist()):
            if game in seen:
                msg = f"game {game} is held out by folds {seen[game]} and {at}"
                raise SystemExit(msg)
            seen[game] = at

    return (
        np.concatenate([part["logits"] for part in parts]),
        np.concatenate([part["truth"] for part in parts]),
        np.concatenate([part["app_id"] for part in parts]),
        subjects,
    )


def _load_study(
    args: argparse.Namespace,
) -> tuple[FloatArray, IntArray, IntArray, list[str]]:
    """The logits, truth, games and subject names the study scores, from folds or the reader."""
    if args.oof:
        logits, truth, app_ids, subjects = pooled_folds(Path(args.oof).glob("*.npz"))
        print(f"{len(truth):,} out-of-fold claims over {len(set(app_ids))} games, from {args.oof}")
    else:
        model_dir = Path(args.model)
        provenance = json.loads((model_dir / "reader.json").read_text(encoding="utf-8"))
        subjects = provenance["subjects"]
        claims = claimdata.load(args.data)
        _, validation, _ = claimdata.split_by_game(claims, seed=args.split_seed)
        validation = [claim for claim in validation if claim.subject in subjects]
        app_ids = np.array([claim.app_id for claim in validation])
        logits = logits_of(model_dir, validation, provenance)
        truth = np.array([subjects.index(claim.subject) for claim in validation])
        print(
            f"{len(validation):,} validation claims over {len(set(app_ids))} games, "
            f"model {model_dir}"
        )
    return logits, truth, app_ids, subjects


def _print_by_subject(
    pooled: dict[str, dict[str, BoolArray]],
    answers: Answers,
    subjects: Sequence[str],
) -> tuple[dict[str, dict[str, Any]], dict[str, dict[str, Any]]]:
    """Print the two policies subject by subject for the incumbent score and return both."""
    incumbent = "max probability"
    one = by_subject(pooled[incumbent]["one line"], answers.predicted, answers.correct, subjects)
    per = by_subject(
        pooled[incumbent]["a line per subject"], answers.predicted, answers.correct, subjects
    )
    print(f"\nsubject by subject on {incumbent!r}, the two policies side by side")
    print(f"{'subject':<18} {'predicted':>9}   {'one line':>16}   {'a line per subject':>18}")
    for name in sorted(one, key=lambda name: -one[name]["predictions"]):
        print(
            f"{name:<18} {one[name]['predictions']:>9}   "
            f"{share(one[name]):>16}   {share(per[name]):>18}"
        )
    return one, per


def main() -> None:
    """Fit and score the abstention rules and print the study's tables."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", default=str(HERE.parent / "models" / "game-review-reader"))
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument(
        "--oof",
        default=None,
        help="directory of .npz files written by `train.py --save-logits`, one per fold. Reads "
        "those instead of running the shipped reader, which is how the study gets twenty-eight "
        "games of held-out answers instead of four.",
    )
    parser.add_argument("--split-seed", type=int, default=1)
    parser.add_argument("--min-accuracy", type=float, default=0.75)
    parser.add_argument("--draws", type=int, default=2000)
    parser.add_argument("--out", default=None)
    args = parser.parse_args()

    logits, truth, app_ids, subjects = _load_study(args)

    predicted = softmax(logits).argmax(axis=1)
    correct = (predicted == truth).astype(float)
    print(f"accuracy over everything: {correct.mean():.3f}\n")

    answers = Answers(correct, predicted, app_ids)
    scored, pooled = held_out_by_game(logits, answers, args.min_accuracy, subjects)
    rng = np.random.default_rng(args.split_seed)

    print(f"{'score':<28} {'AUGRC':>7} {'AURC':>7}   held-out coverage at the promise")
    found: dict[str, dict[str, Any]] = {}
    for name, score in scored.items():
        row: dict[str, Any] = {
            "augrc": augrc(score, correct),
            "aurc": aurc(score, correct),
            "policies": {},
        }
        said: list[str] = []
        for policy, answered in pooled[name].items():
            coverage = float(answered.mean())
            accuracy = float(correct[answered].mean()) if answered.any() else float("nan")
            low_coverage, low_accuracy = game_interval(answered, correct, app_ids, args.draws, rng)
            row["policies"][policy] = {
                "coverage": coverage,
                "coverage_interval": low_coverage,
                "accuracy": accuracy,
                "accuracy_interval": low_accuracy,
                "met": accuracy >= args.min_accuracy,
            }
            said.append(f"{policy}: {coverage:.1%} at {accuracy:.3f}")
        found[name] = row
        print(f"{name:<28} {row['augrc']:>7.4f} {row['aurc']:>7.4f}   {'   '.join(said)}")

    best_augrc = min(found, key=lambda name: found[name]["augrc"])
    best_aurc = min(found, key=lambda name: found[name]["aurc"])
    print(f"\nAUGRC prefers {best_augrc!r}; AURC prefers {best_aurc!r}")
    for policy in ("one line", "a line per subject"):
        winner = max(found, key=lambda name: found[name]["policies"][policy]["coverage"])
        entry = found[winner]["policies"][policy]
        low, high = entry["coverage_interval"]
        print(
            f"most coverage with {policy}: {winner!r} at {entry['coverage']:.1%} "
            f"[{low:.1%}, {high:.1%}], accuracy {entry['accuracy']:.3f}"
        )

    one, per = _print_by_subject(pooled, answers, subjects)

    if args.out:
        Path(args.out).write_text(
            json.dumps(
                {"scores": found, "by_subject": {"one line": one, "a line per subject": per}},
                indent=2,
            ),
            encoding="utf-8",
        )


if __name__ == "__main__":
    main()
