"""Averages the weights of several trained runs, and reads the average.

Three seeds of one configuration land a couple of points apart on the validation games, so
shipping the best of the three ships whichever seed the validation draw flattered: the held-out
figure does not follow it. Fine-tunes that started from the same pretrained encoder stay close
enough in weight space to be averaged, and the average keeps what the runs agree on rather
than what each of them invented (Wortsman et al., "Model soups", ICML 2022). It costs no
training, only one reading of each set.

The ingredients need not differ only by seed. Runs that learned the same claims through
different settings are the interesting soup, because what each setting overfits is different
and the average holds none of it. What they may not differ on is the encoder, the subjects,
the window or the split, and those are refused rather than averaged.

    python soup.py runs/a runs/b runs/c --run-id e5inst-pool-rdrop-soup --save

`--greedy` adds the ingredients one at a time in order of how well each reads the validation
games, and keeps one only while the soup improves. The held-out games are read once, at the end,
by whichever soup was chosen: nothing here may be picked by them.
"""

from __future__ import annotations

import argparse
import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any

import torch
from torch.utils.data import DataLoader
from transformers import AutoTokenizer

import backbones
import claimdata
from train import (
    ClaimReader,
    Claims,
    Reading,
    Split,
    confidence_of,
    evaluate,
    git_sha,
    selective,
)
from typed_calls import save_pretrained

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from transformers import PreTrainedTokenizerBase

    from train import Item, Metrics

HERE = Path(__file__).resolve().parent
MIN_INGREDIENTS = 2
DEFAULT_FOLDS = 5

# A run's `run.json`: the fields grow with the trainer and older runs lack newer ones.
Record = dict[str, Any]

# What the ingredients have to agree on before adding their weights together means anything:
# the same encoder, the same subjects in the same order, the same window around the claim, and
# the same split of the same labels. Differ on any of them and two different models are being
# added together, or one is being scored on games the other trained on.
SHARED = (
    "backbone",
    "subjects",
    "max_length",
    "context",
    "prefix",
    "mark",
    "pooling",
    "split_seed",
    "fold",
    "folds",
    "data_fingerprint",
)


class Soup:
    """A running mean of saved weights. It holds one ingredient at a time.

    A 560M encoder is 2.2 GB of weights and the card is shared, so the ingredients are summed
    as they are read rather than all held and averaged at the end. The sum stays in the dtype
    the weights were saved in: a handful of fp32 tensors added together loses nothing worth the
    second copy that a wider accumulator would cost.
    """

    def __init__(self) -> None:
        """Start with no ingredients."""
        self.total: dict[str, torch.Tensor] = {}
        # Anything that is not a weight, such as an integer position buffer: identical in every
        # run and meaningless averaged, so it is carried across from the first ingredient.
        self.fixed: dict[str, torch.Tensor] = {}
        self.count = 0

    def added(self, state: dict[str, torch.Tensor]) -> dict[str, torch.Tensor]:
        """What the soup would be with this ingredient in it, without putting it in."""
        if not self.count:
            return dict(state)
        divisor = self.count + 1
        mixed = dict(self.fixed)
        for key, summed in self.total.items():
            mixed[key] = ((summed + state[key]) / divisor).to(summed.dtype)
        return mixed

    def add(self, state: dict[str, torch.Tensor]) -> None:
        """Put one ingredient into the soup, refusing weights that do not match the first."""
        if not self.count:
            for key, value in state.items():
                if value.is_floating_point():
                    self.total[key] = value.clone()
                else:
                    self.fixed[key] = value.clone()
            self.count = 1
            return
        if set(state) != set(self.total) | set(self.fixed):
            msg = "an ingredient holds different weights from the first one"
            raise SystemExit(msg)
        for key, value in state.items():
            if key in self.total:
                self.total[key] += value
            elif not torch.equal(self.fixed[key], value):
                msg = f"the ingredients disagree on {key}, which is not a weight"
                raise SystemExit(msg)
        self.count += 1

    def state(self) -> dict[str, torch.Tensor]:
        """The mean of the weights added so far, with the non-weights carried across."""
        if not self.count:
            msg = "an empty soup has no weights"
            raise SystemExit(msg)
        return {**self.fixed, **{key: (value / self.count) for key, value in self.total.items()}}


def ingredients(runs: list[str]) -> list[tuple[Path, Record]]:
    """Each run directory with its saved record, refusing a run without weights or a lone run."""
    found: list[tuple[Path, Record]] = []
    for run in runs:
        path = Path(run)
        if not (path / "model.bin").exists():
            msg = f"{path} has no model.bin; a run is only souped if it was --save'd"
            raise SystemExit(msg)
        found.append((path, json.loads((path / "run.json").read_text(encoding="utf-8"))))
    if len(found) < MIN_INGREDIENTS:
        msg = "averaging needs at least two runs"
        raise SystemExit(msg)
    return found


def must_agree(found: list[tuple[Path, Record]]) -> None:
    """Refuse ingredients that differ on anything in `SHARED`."""
    first_path, first = found[0]
    for path, record in found[1:]:
        for key in SHARED:
            if record.get(key) != first.get(key):
                msg = (
                    f"{path} and {first_path} differ on {key}: {record.get(key)!r} against "
                    f"{first.get(key)!r}. Weights are only worth averaging across runs of one "
                    f"configuration on one export."
                )
                raise SystemExit(msg)


@dataclass(frozen=True)
class Scorer:
    """The model and everything needed to read the validation games with a set of weights."""

    model: ClaimReader
    loaders: Mapping[str, DataLoader[Item]]
    device: str
    subjects: Sequence[str]
    parts: Mapping[str, Sequence[claimdata.Claim]]
    min_accuracy: float

    def read_with(self, state: dict[str, torch.Tensor]) -> Metrics:
        """Load a set of weights into the model and read the validation games with them.

        The weights arrive on the CPU, where they were averaged; `load_state_dict` copies them
        into parameters that are already on the card, so nothing here moves a second copy across.
        """
        self.model.load_state_dict(state)
        return evaluate(
            self.model,
            Split(self.loaders["validation"], self.parts["validation"]),
            self.device,
            self.subjects,
            self.min_accuracy,
        )


def how_it_reads(metrics: Metrics) -> str:
    """One line on what a set of weights answers and how well it ranks."""
    answers = (
        f"answers {metrics['threshold_coverage']:.0%} at {metrics['threshold_accuracy']:.3f}"
        if metrics["threshold_coverage"]
        else "answers nothing at the accuracy asked for"
    )
    return f"validation {answers}  macro F1 {metrics['macro_f1']:.3f}  AURC {metrics['aurc']:.3f}"


def coverage_of(metrics: Metrics) -> float:
    """What a soup is judged by while it is being built.

    That is how much of the validation set it answers at the accuracy the threshold promises.
    A soup that answers no more than its ingredients is not worth the averaging, whatever its
    raw accuracy says.
    """
    return metrics["threshold_coverage"] if metrics["threshold_met"] else 0.0


def _checked_claims(args: argparse.Namespace, first: Record) -> tuple[list[claimdata.Claim], str]:
    """The labelled claims, refusing an export the runs did not learn."""
    claims = claimdata.load(args.data)
    fingerprint = claimdata.fingerprint(claims)
    if fingerprint != first["data_fingerprint"]:
        msg = (
            f"{args.data} fingerprints {fingerprint}, "
            f"the runs learned {first['data_fingerprint']}. "
            f"The split moves with the export, so this would score a soup on games it trained on."
        )
        raise SystemExit(msg)
    if claimdata.subjects_in(claims) != first["subjects"]:
        msg = "the export's subjects are not the ones the runs' heads were built for"
        raise SystemExit(msg)
    return claims, fingerprint


def _open_tokenizer(first: Record) -> PreTrainedTokenizerBase:
    """The ingredients' tokenizer, padded on the right as the readers were trained."""
    tokenizer = backbones.load(AutoTokenizer, first["backbone"])
    if first["pooling"] == "last" and tokenizer.pad_token_id is None:  # pyright: ignore[reportUnknownMemberType]  # transformers types pad_token_id partly Unknown
        tokenizer.pad_token = tokenizer.eos_token  # pyright: ignore[reportUnknownMemberType]  # same untyped property family
    tokenizer.padding_side = "right"
    return tokenizer


def _loaders_for(
    first: Record,
    tokenizer: PreTrainedTokenizerBase,
    subjects: Sequence[str],
    parts: Mapping[str, Sequence[claimdata.Claim]],
    batch_size: int,
) -> dict[str, DataLoader[Item]]:
    """One unshuffled loader for the validation games and one for the held-out games."""
    return {
        name: DataLoader(
            Claims(
                parts[name],
                tokenizer,
                subjects,
                Reading(
                    max_length=first["max_length"],
                    context=first["context"],
                    mark=first["mark"],
                    prefix=first["prefix"],
                    headset_marker=first.get("headset_marker", False),
                ),
            ),
            batch_size=batch_size,
            shuffle=False,
            num_workers=0,
        )
        for name in ("validation", "test")
    }


def _blend(
    found: list[tuple[Path, Record]],
    alone: Mapping[str, Metrics],
    scorer: Scorer,
    *,
    greedy: bool,
) -> tuple[Soup, list[str], list[str]]:
    """Build the soup, returning it with the ingredients it kept and refused."""
    order = sorted(found, key=lambda pair: -coverage_of(alone[str(pair[0])]))
    soup = Soup()
    kept: list[str] = []
    refused: list[str] = []
    best = 0.0
    for path, _ in order if greedy else found:
        state: dict[str, torch.Tensor] = torch.load(path / "model.bin", map_location="cpu")
        if not greedy:
            soup.add(state)
            kept.append(str(path))
            continue
        metrics = scorer.read_with(soup.added(state))
        score = coverage_of(metrics)
        if soup.count and score <= best:
            refused.append(str(path))
            print(f"{path.name:<32} refused: {score:.0%} against the soup's {best:.0%}", flush=True)
            continue
        soup.add(state)
        kept.append(str(path))
        best = score
        print(f"{path.name:<32} kept: the soup answers {score:.0%}", flush=True)
    return soup, kept, refused


def _read_held_out(scorer: Scorer, test: Sequence[claimdata.Claim], metrics: Metrics) -> Metrics:
    """Read the held-out games once, with the threshold the validation games chose."""
    held = evaluate(
        scorer.model,
        Split(scorer.loaders["test"], test),
        scorer.device,
        scorer.subjects,
        scorer.min_accuracy,
    )
    at_threshold = selective(
        confidence_of(scorer.model, scorer.loaders["test"], scorer.device),
        [scorer.subjects.index(claim.subject) for claim in test],
        metrics["threshold"],
    )
    held["at_validation_threshold"] = at_threshold
    print(
        f"held-out games: {at_threshold['coverage']:.0%} of claims answered at "
        f"{at_threshold['accuracy']:.3f}"
        if at_threshold["coverage"] > 0
        else "held-out games: nothing cleared the threshold"
    )
    return held


def main(args: argparse.Namespace) -> Record:
    """Average the runs in `args.runs`, score the soup and write its run record."""
    started = time.time()
    found = ingredients(args.runs)
    must_agree(found)
    first = found[0][1]

    claims, fingerprint = _checked_claims(args, first)
    subjects = claimdata.subjects_in(claims)

    train, validation, test = claimdata.split_by_game(
        claims,
        seed=first["split_seed"],
        fold=first["fold"],
        folds=first["folds"] or DEFAULT_FOLDS,
    )
    parts = {"train": train, "validation": validation, "test": test}
    print(f"train {len(train)}  validation {len(validation)}  test {len(test)} (held out)")

    device = "cuda" if torch.cuda.is_available() else "cpu"
    tokenizer = _open_tokenizer(first)
    model = ClaimReader(first["backbone"], len(subjects), pooling=first["pooling"]).to(device)

    loaders = _loaders_for(first, tokenizer, subjects, parts, args.batch_size)
    scorer = Scorer(model, loaders, device, subjects, parts, args.min_accuracy)

    # Each ingredient is read on its own first, under this export and this loader rather than
    # whatever its own run.json remembers, because that is the only way the order the greedy
    # soup adds them in, and the comparison the soup is judged against, mean anything.
    alone: dict[str, Metrics] = {}
    for path, _ in found:
        ingredient: dict[str, torch.Tensor] = torch.load(path / "model.bin", map_location="cpu")
        metrics = scorer.read_with(ingredient)
        alone[str(path)] = metrics
        print(f"{path.name:<32} {how_it_reads(metrics)}", flush=True)

    soup, kept, refused = _blend(found, alone, scorer, greedy=args.greedy)

    state = soup.state()
    metrics = scorer.read_with(state)
    print(f"\nsoup of {soup.count}: {how_it_reads(metrics)}")

    # The held-out games, read once, by the soup that was already chosen. Reading them per
    # ingredient would turn the one set nothing is picked on into a set something was picked on.
    held: Metrics | None = _read_held_out(scorer, test, metrics) if args.held_out else None

    record: Record = {
        "soup": {
            "kind": "greedy" if args.greedy else "uniform",
            "runs": [str(path) for path, _ in found],
            "kept": kept,
            "refused": refused,
            "each_alone": {
                name: {
                    "threshold_coverage": each["threshold_coverage"],
                    "threshold_accuracy": each["threshold_accuracy"],
                    "macro_f1": each["macro_f1"],
                    "aurc": each["aurc"],
                }
                for name, each in alone.items()
            },
        },
        "backbone": first["backbone"],
        "max_length": first["max_length"],
        "context": first["context"],
        "mark": first["mark"],
        "prefix": first["prefix"],
        "pooling": first["pooling"],
        "split_seed": first["split_seed"],
        "fold": first["fold"],
        "folds": first["folds"],
        "device": device,
        "git_sha": git_sha(),
        "data_fingerprint": fingerprint,
        "claims": {"train": len(train), "validation": len(validation), "test": len(test)},
        "games": {
            "train": sorted({claim.app_id for claim in train}),
            "validation": sorted({claim.app_id for claim in validation}),
            "test": sorted({claim.app_id for claim in test}),
        },
        "subjects": subjects,
        "min_accuracy": args.min_accuracy,
        "seconds": round(time.time() - started),
        "validation": metrics,
    }
    if held is not None:
        record["test"] = held

    out = HERE / "runs" / (args.run_id or f"soup-{int(time.time())}")
    out.mkdir(parents=True, exist_ok=True)
    (out / "run.json").write_text(json.dumps(record, indent=2), encoding="utf-8")
    if args.save:
        torch.save(state, out / "model.bin")
        save_pretrained(tokenizer, out / "tokenizer")
    print(f"\nwritten to {out}")
    return record


def parse() -> argparse.Namespace:
    """Read the command line."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runs", nargs="+", help="run directories to average, each with a model.bin")
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument("--batch-size", type=int, default=32)
    parser.add_argument("--min-accuracy", type=float, default=0.75)
    parser.add_argument("--run-id", default=None)
    parser.add_argument(
        "--greedy",
        action="store_true",
        help="add the ingredients best first and keep one only while the soup answers more of "
        "the validation set, rather than averaging all of them",
    )
    parser.add_argument("--save", action="store_true", help="write the averaged weights")
    parser.add_argument(
        "--no-held-out",
        dest="held_out",
        action="store_false",
        help="leave the held-out games unread",
    )
    return parser.parse_args()


if __name__ == "__main__":
    main(parse())
