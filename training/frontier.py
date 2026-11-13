"""Asks a frontier model the same question the reader is asked, and scores the answers.

The other baselines are things a frontier model obviously beats. This is the one that might
beat the trained reader, and until it is run the claim that a fine-tuned 278M encoder is worth
having is an assertion. Same claims, same category sheet the labellers worked from, same
review around each claim, and the same abstention: a model that may say "unsure" is measured
on coverage as well as accuracy, exactly as the reader is.

    python frontier.py handout --out ../scratchpad/frontier/    # what the model reads
    python frontier.py score --answers ../scratchpad/frontier/answers.json

The handout deliberately carries no labels and no game names. A model told which game it is
reading can lean on what it knows about that game rather than on what the claim says, and the
reader cannot.
"""

from __future__ import annotations

import argparse
import json
import random
from collections import defaultdict
from pathlib import Path
from typing import TYPE_CHECKING, Any, Protocol, TypedDict, cast

import claimdata

if TYPE_CHECKING:
    from collections.abc import Sequence

    from tokenizers import Tokenizer

HERE = Path(__file__).resolve().parent
BATCH = 50
# Claims the reader's graph is run on at once: enough to keep the provider busy while one
# padded batch stays cheap on a long claim.
READ_BATCH = 64
WILSON_Z = 1.96


class KeyRow(TypedDict):
    """One row of a key: where a claim came from and the labels it carries."""

    id: str
    app_id: int
    review_id: str
    claim_index: int
    subject: str
    polarity: str


Interval = tuple[float, float] | None
Answer = dict[str, Any]


def abstention_line(provenance: dict[str, Any], subject: int) -> float:
    """What the reader needs to be sure of before it answers, for the subject it picked.

    A reader that declines `vr` below 0.98 and answers `performance` at 0.23 is not the reader
    one line describes, so scoring it against a single threshold measures a model nobody ships.
    `null` is a subject declined outright, and an export without the array is every reader
    shipped before the lines existed. This mirrors `Provenance::line_for` in `reader.rs`.
    """
    lines = provenance.get("thresholds")
    if not lines:
        return float(provenance["threshold"])
    one = lines[subject]
    return float("inf") if one is None else float(one)


def drawn(claims: Sequence[claimdata.Claim], per_subject: int, seed: int) -> list[claimdata.Claim]:
    """A stratified draw, so the starved rows are measured rather than rounded away.

    A proportional sample of held-out claims is a quarter `verdict` and two claims of
    `licensing`, and a macro F1 over that says almost nothing about the rows that need it.
    """
    by_subject: defaultdict[str, list[claimdata.Claim]] = defaultdict(list)
    for claim in claims:
        by_subject[claim.subject].append(claim)
    rng = random.Random(seed)  # noqa: S311 a reproducible draw, not a secret
    picked: list[claimdata.Claim] = []
    for subject in sorted(by_subject):
        pool = by_subject[subject]
        picked.extend(rng.sample(pool, min(per_subject, len(pool))))
    rng.shuffle(picked)
    return picked


def write_handout(claims: Sequence[claimdata.Claim], out: Path, sheet: Path) -> tuple[int, int]:
    """Write the sheet, the claim batches, the key and the task into `out`.

    Returns the number of claims written and the number of batches.
    """
    out.mkdir(parents=True, exist_ok=True)
    (out / "sheet.txt").write_text(sheet.read_text(encoding="utf-8"), encoding="utf-8")

    keys: list[KeyRow] = []
    for at in range(0, len(claims), BATCH):
        chunk = claims[at : at + BATCH]
        rows: list[dict[str, str]] = []
        for index, claim in enumerate(chunk):
            claim_id = f"{at + index:04d}"
            keys.append(
                {
                    "id": claim_id,
                    "app_id": claim.app_id,
                    "review_id": claim.review_id,
                    "claim_index": claim.claim_index,
                    "subject": claim.subject,
                    "polarity": claim.polarity,
                }
            )
            rows.append(
                {
                    "id": claim_id,
                    "claim": claim.text,
                    "review": claim.review,
                    "language": claim.language,
                }
            )
        (out / f"batch-{at // BATCH:02d}.json").write_text(
            json.dumps(rows, ensure_ascii=False, indent=2), encoding="utf-8"
        )

    (out / "key.json").write_text(json.dumps(keys, indent=2), encoding="utf-8")
    (out / "TASK.md").write_text(TASK, encoding="utf-8")
    return len(keys), (len(claims) + BATCH - 1) // BATCH


TASK = """# Read each claim and say what it is about

`sheet.txt` is the category sheet. Every batch file holds claims drawn from Steam reviews of
games you are not told the names of. For each claim, using the review around it as context:

1. Which category of the sheet it is about, by its `id`.
2. Whether it is `praise`, a `complaint`, or `neutral` about that subject.
3. Whether you are sure. Answer `unsure` as the subject when you genuinely cannot tell;
   that is measured as coverage, not counted against you as an error.

Answer every claim of every batch. Write one file, `answers.json`, holding a list of
`{"id": "0000", "subject": "<sheet id or unsure>", "polarity": "praise|complaint|neutral"}`.

Nothing else: no reasoning in the file, no extra fields, no claims left out.
"""


def wilson(hits: int, total: int, z: float = WILSON_Z) -> Interval:
    """A proportion's interval, the same way every other figure in this project reports one."""
    if total == 0:
        return None
    rate = hits / total
    middle = rate + z * z / (2 * total)
    spread = z * ((rate * (1 - rate) + z * z / (4 * total)) / total) ** 0.5
    divisor = 1 + z * z / total
    return ((middle - spread) / divisor, (middle + spread) / divisor)


def score(
    answers: Path,
    key: Path,
    subjects: set[str] | None = None,
    over: set[str] | None = None,
) -> dict[str, Any]:
    """The answers against the key, over every claim in it or only the ones in `over`.

    `over` is how a reader is scored on the claims it can be handed: a key drawn before the
    splitter last changed names spans this build no longer cuts, and counting those against a
    reader that was never asked them would charge it for the splitter.
    """
    given: dict[str, Answer] = {
        row["id"]: row for row in json.loads(answers.read_text(encoding="utf-8"))
    }
    key_rows = cast("list[KeyRow]", json.loads(key.read_text(encoding="utf-8")))
    wanted = [row for row in key_rows if over is None or row["id"] in over]

    answered = subject_right = polarity_right = 0
    missing = 0
    off_sheet: defaultdict[str, int] = defaultdict(int)
    per_subject: defaultdict[str, dict[str, int]] = defaultdict(
        lambda: {"found": 0, "wanted": 0, "hit": 0}
    )
    for row in wanted:
        said = given.get(row["id"])
        if said is None:
            missing += 1
            continue
        per_subject[row["subject"]]["wanted"] += 1
        if said.get("subject") in (None, "", "unsure"):
            continue
        answered += 1
        # A subject the sheet does not have is a wrong answer, but it is a different kind of
        # wrong from picking the neighbouring category, and it is worth reporting apart.
        if subjects is not None and said["subject"] not in subjects:
            off_sheet[said["subject"]] += 1
        per_subject[said["subject"]]["found"] += 1
        if said["subject"] == row["subject"]:
            subject_right += 1
            per_subject[row["subject"]]["hit"] += 1
            if said.get("polarity") == row["polarity"]:
                polarity_right += 1

    scores: list[float] = []
    for counts in per_subject.values():
        if not counts["wanted"] and not counts["found"]:
            continue
        precision = counts["hit"] / max(counts["found"], 1)
        recall = counts["hit"] / max(counts["wanted"], 1)
        scores.append(0.0 if not counts["hit"] else 2 * precision * recall / (precision + recall))

    found: dict[str, Any] = {
        "claims": len(wanted),
        "unanswered_rows": missing,
        "coverage": answered / max(len(wanted), 1),
        "accuracy_where_answered": subject_right / max(answered, 1),
        "accuracy_interval": wilson(subject_right, answered),
        "polarity_where_subject_right": polarity_right / max(subject_right, 1),
        "macro_f1": sum(scores) / max(len(scores), 1),
        "off_sheet_subjects": dict(off_sheet),
    }
    at_share = accuracy_at_shares(wanted, given)
    if at_share:
        found["accuracy_at_coverage"] = at_share
    return found


# The shares every reader is compared at. Each reader's own lines put it at its own coverage, so
# two readers' accuracies where answered are figures for different amounts of work. Answering
# the same share of the same claims, most confident first, they are figures for the same work.
SHARES = (0.8, 0.9)


def accuracy_at_shares(
    wanted: Sequence[KeyRow], given: dict[str, Answer]
) -> dict[str, dict[str, Any]] | None:
    """Accuracy answering only the most confident share of `wanted`, for each of `SHARES`.

    None unless every claim was answered with a confidence and a best guess, which a frontier
    model's answers do not have and the reader's do.
    """
    said = [given.get(row["id"]) for row in wanted]
    present = [one for one in said if one is not None]
    if not wanted or len(present) != len(said) or any("confidence" not in one for one in present):
        return None
    ranked = sorted(
        zip(wanted, present, strict=True), key=lambda pair: pair[1]["confidence"], reverse=True
    )
    shares: dict[str, dict[str, Any]] = {}
    for share in SHARES:
        taken = ranked[: round(share * len(ranked))]
        right = sum(1 for row, one in taken if one["guess"] == row["subject"])
        shares[f"{share:.0%}"] = {
            "claims": len(taken),
            "accuracy": right / max(len(taken), 1),
            "interval": wilson(right, len(taken)),
        }
    return shares


def readable(key: Path, data: str) -> tuple[list[KeyRow], list[claimdata.Claim]]:
    """The key rows a reader can be handed, with their claims, out of the export in `data`.

    A key row the export no longer holds is a span this build cuts no claim at, 30 of the 487
    when it was last counted. Every row of the benchmark is scored over the same survivors, the
    frontier model's as much as the reader's: a frontier model answered all 487 when it was
    asked, and scored over all of them it sits on a different denominator from every reader.
    """
    held = {(c.app_id, c.review_id, c.claim_index): c for c in claimdata.load(data)}
    key_rows = cast("list[KeyRow]", json.loads(key.read_text(encoding="utf-8")))
    rows = [
        row for row in key_rows if (row["app_id"], row["review_id"], row["claim_index"]) in held
    ]
    return rows, [held[(row["app_id"], row["review_id"], row["claim_index"])] for row in rows]


class Encoding(Protocol):
    """The part of a tokenizers encoding the reader run reads."""

    ids: list[int]
    attention_mask: list[int]


class Encoder(Protocol):
    """The part of a tokenizers tokenizer the reader run calls.

    The package's own stub leaves these signatures untyped, so they are restated here.
    """

    def no_padding(self) -> None:
        """Turn padding off."""

    def no_truncation(self) -> None:
        """Turn truncation off."""

    def encode(self, sequence: str, pair: str | None = None) -> Encoding:
        """Tokenise one sequence or a pair."""
        ...


def open_tokenizer(model_dir: Path) -> tuple[Tokenizer, Encoder]:
    """Load the model directory's `tokenizer.json` with padding and truncation off.

    Returns the tokenizer for `ShippedTokenizer` and the same object typed for `encode_pair`.
    """
    # Local so that the commands that never read a model start without loading the library.
    from tokenizers import Tokenizer  # noqa: PLC0415 heavy import, only reader runs need it

    loaded = cast(
        "Tokenizer",
        Tokenizer.from_file(  # pyright: ignore[reportUnknownMemberType]  # stub leaves from_file untyped
            str(model_dir / "tokenizer.json")
        ),
    )
    encoder = cast("Encoder", loaded)
    encoder.no_padding()
    encoder.no_truncation()
    return loaded, encoder


def encode_pair(tokenizer: Encoder, pair: tuple[str, ...]) -> tuple[list[int], list[int]]:
    """Token ids and attention mask of a claim's one or two sequences."""
    first, *rest = pair
    encoded = tokenizer.encode(first, rest[0] if rest else None)
    return encoded.ids, encoded.attention_mask


def score_reader(model_dir: Path, key: Path, data: str) -> tuple[list[Answer], str]:
    """The shipped reader, over exactly the claims the frontier model was given.

    Without this the comparison is a cheat. The reader's held-out figure is over every held-out
    claim, which is a quarter `verdict`; the frontier handout is stratified, twenty a subject,
    and the starved rows are the hard ones. Two numbers from two distributions are not a
    comparison, however carefully each was measured.
    """
    # Local so that `handout` and `score` start without loading the reader's runtime.
    import numpy as np  # noqa: PLC0415 heavy import, only the reader run needs it
    import onnxruntime  # noqa: PLC0415 heavy optional import, only the reader run needs it

    from train import Claims, Reading, ShippedTokenizer  # noqa: PLC0415 train imports torch

    provenance = json.loads((model_dir / "reader.json").read_text(encoding="utf-8"))
    subjects = provenance["subjects"]
    wanted, claims = readable(key, data)

    loaded, tokenizer = open_tokenizer(model_dir)
    cut = Claims(
        claims,
        ShippedTokenizer(loaded),
        subjects,
        # A model trained to look for the marks and scored without them is scored on input it
        # has never seen, which understates it and looks like a worse model rather than a
        # worse measurement. The same goes for the pair's prefixes.
        Reading.recorded(provenance, provenance["max_tokens"]),
    )

    # Scored where the tool reads: DirectML is what the reader opens on this machine and what the
    # Windows release ships with, and a half-precision graph has no processor kernels worth the
    # name, so on the processor the 4B took seventeen minutes of every core. The processor is
    # the fallback only where DirectML is not installed, and the figure says which ran.
    providers = [
        provider
        for provider in ("DmlExecutionProvider", "CPUExecutionProvider")
        if provider in onnxruntime.get_available_providers()
    ]
    session = onnxruntime.InferenceSession(str(model_dir / "model.onnx"), providers=providers)
    outputs = [out.name for out in session.get_outputs()]

    confidence: list[float] = []
    predicted: list[str] = []
    picked: list[int] = []
    polarity: list[str] = []
    for at in range(0, len(claims), READ_BATCH):
        chunk = list(range(at, min(at + READ_BATCH, len(claims))))
        encoded = [encode_pair(tokenizer, cut.pair(i)) for i in chunk]
        longest = max(min(len(token_ids), provenance["max_tokens"]) for token_ids, _ in encoded)
        ids = np.zeros((len(chunk), longest), dtype=np.int64)
        mask = np.zeros((len(chunk), longest), dtype=np.int64)
        for row, (token_ids, attention) in enumerate(encoded):
            keep = token_ids[:longest]
            ids[row, : len(keep)] = keep
            mask[row, : len(keep)] = attention[:longest]
        found = session.run(outputs, {"input_ids": ids, "attention_mask": mask})
        subject_logits, polarity_logits = found[0], found[1]
        for row in range(len(chunk)):
            exp = np.exp(subject_logits[row] - subject_logits[row].max())
            probabilities = exp / exp.sum()
            confidence.append(float(probabilities.max()))
            chosen = int(probabilities.argmax())
            picked.append(chosen)
            predicted.append(subjects[chosen])
            polarity.append(
                claimdata.POLARITIES[int(np.argmax(polarity_logits[row]))]
                if int(np.argmax(polarity_logits[row])) < len(claimdata.POLARITIES)
                else "neutral"
            )

    answers: list[Answer] = [
        {
            "id": row["id"],
            "subject": (
                predicted[at]
                if confidence[at] >= abstention_line(provenance, picked[at])
                else "unsure"
            ),
            "polarity": polarity[at],
            # What it would have said and how sure it was, whether or not its lines let it say
            # it: readers whose lines differ are only comparable answering the same share.
            "guess": predicted[at],
            "confidence": confidence[at],
        }
        for at, row in enumerate(wanted)
    ]
    return answers, session.get_providers()[0]


def _run_handout(args: argparse.Namespace) -> None:
    claims = claimdata.load(args.data)
    _, _, held_out = claimdata.split_by_game(claims, seed=args.split_seed)
    picked = drawn(held_out, args.per_subject, args.seed)
    count, batches = write_handout(picked, Path(args.out), Path(args.sheet))
    print(f"{count:,} claims over {batches:,} batches -> {args.out}")
    print(f"drawn from {len({c.app_id for c in picked}):,} held-out games, labels held back")


def _write_held_out_key(args: argparse.Namespace) -> Path:
    """Write the key over every held-out claim and return where it went."""
    # Every held-out claim rather than a stratified draw, because this is the run that
    # has to reproduce what training reported, and training reported over all of them.
    claims = claimdata.load(args.data)
    _, _, held_out = claimdata.split_by_game(claims, seed=args.split_seed)
    if args.app_id:
        held_out = [claim for claim in held_out if claim.app_id == args.app_id]
    # Beside the export it is built from, which is where this project keeps the files
    # that hold review ids rather than review text and are not committed either way.
    key = Path(args.key) if args.key else HERE / "data" / "held-out-key.json"
    key.write_text(
        json.dumps(
            [
                {
                    "id": f"{at:05d}",
                    "app_id": claim.app_id,
                    "review_id": claim.review_id,
                    "claim_index": claim.claim_index,
                    "subject": claim.subject,
                    "polarity": claim.polarity,
                }
                for at, claim in enumerate(held_out)
            ]
        ),
        encoding="utf-8",
    )
    print(f"{len(held_out):,} held-out claims -> {key}")
    return key


def _run_reader(args: argparse.Namespace) -> None:
    model_dir = Path(args.model)
    key = _write_held_out_key(args) if args.held_out else Path(args.key)
    said, scored_on = score_reader(model_dir, key, args.data)
    written = Path(args.answers) if args.answers else key.parent / "reader-answers.json"
    written.write_text(json.dumps(said, indent=2), encoding="utf-8")
    keyed = json.loads(key.read_text("utf-8"))
    found = score(
        written,
        key,
        {row["subject"] for row in keyed},
        over={row["id"] for row in said},
    )
    found["unreadable"] = len(keyed) - len(said)
    # What model, not where it sat on one machine. A path names a directory on the laptop
    # that ran this, which means nothing to anyone reading the published figure and puts a
    # home directory in a file that ships.
    reader = json.loads((model_dir / "reader.json").read_text("utf-8"))
    found["model"] = reader["trained_from"]
    found["scored_on"] = scored_on
    found["trained_on"] = reader["data_fingerprint"]
    found["threshold"] = reader["threshold"]
    # Naming one threshold for a reader that abstains per subject describes a run nobody can
    # reproduce from the row, so the lines it actually used travel with the figure.
    if reader.get("thresholds"):
        found["thresholds"] = dict(zip(reader["subjects"], reader["thresholds"], strict=True))
    print(json.dumps(found, indent=2))
    if args.out:
        Path(args.out).write_text(json.dumps(found, indent=2), encoding="utf-8")


def _run_score(args: argparse.Namespace) -> None:
    answers = Path(args.answers)
    key = Path(args.key) if args.key else answers.parent / "key.json"
    rows, _ = readable(key, args.data)
    found = score(
        answers,
        key,
        {claim.subject for claim in claimdata.load(args.data)},
        over={row["id"] for row in rows},
    )
    found["unreadable"] = len(json.loads(key.read_text("utf-8"))) - len(rows)
    found["answered_by"] = args.by
    print(json.dumps(found, indent=2))
    if args.out:
        Path(args.out).write_text(json.dumps(found, indent=2), encoding="utf-8")


def main() -> None:
    """Run the `handout`, `score` or `reader` command from the command line."""
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)

    make = sub.add_parser("handout")
    make.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    make.add_argument("--sheet", default=str(HERE.parent / "reference" / "claim-brief.txt"))
    make.add_argument("--out", required=True)
    make.add_argument("--per-subject", type=int, default=20)
    make.add_argument("--seed", type=int, default=1)
    make.add_argument("--split-seed", type=int, default=1)

    read = sub.add_parser("score")
    read.add_argument("--answers", required=True)
    read.add_argument("--key", default=None)
    read.add_argument("--out", default=None)
    read.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    read.add_argument(
        "--by",
        required=True,
        help="which model answered, as its own name. A benchmark row nobody can attribute is "
        "a row nobody can reproduce or argue with.",
    )

    mine = sub.add_parser("reader")
    mine.add_argument("--model", default=str(HERE.parent / "models" / "game-review-reader"))
    mine.add_argument("--key", default=None)
    mine.add_argument(
        "--held-out",
        action="store_true",
        help="score every held-out claim, writing the key first. This is the run that has to "
        "reproduce what training reported, and `steamgauge measure-claims` over the same "
        "games is the third opinion: any two of them more than a point apart is a bug.",
    )
    mine.add_argument("--app-id", type=int, default=None, help="one game of the held-out set")
    mine.add_argument("--split-seed", type=int, default=1)
    mine.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    mine.add_argument("--answers", default=None, help="where to write the reader's answers")
    mine.add_argument("--out", default=None)

    args = parser.parse_args()

    if args.mode == "handout":
        _run_handout(args)
    elif args.mode == "reader":
        _run_reader(args)
    else:
        _run_score(args)


if __name__ == "__main__":
    main()
