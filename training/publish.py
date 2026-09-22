"""Publishes an exported run to Hugging Face, and pins it in the tool in the same motion.

Two things go up. The model: the graph, its tokenizer and reader.json, with the card the
export wrote. And the dataset it was trained from: review ids, claim offsets and labels, and
never the text, because this repository holds no review data and neither will that one.

Publishing without pinning would be the one thing worse than not publishing. The tool fetches
the reader by checksum, so a model uploaded and not pinned is a model nobody can use, and a
model pinned by hand is a hash somebody typed. This computes the hashes from the exact bytes
it uploaded and writes them into reader.rs.

    python publish.py --run runs/<id> --model-repo <user>/game-review-reader \
        --data-repo <user>/game-review-claims

Needs a Hugging Face token with write access, from `hf auth login` or HF_TOKEN.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
READER_RS = REPO / "crates" / "steamgauge-core" / "src" / "reader.rs"

MODEL_FILES = ["model.onnx", "tokenizer.json", "reader.json"]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


# The draws beside a game's random one that train the reader, as the Rust side names them in
# `claimset::TEACHING_SETS`. Named rather than "every subdirectory holding labels": `second/`
# holds another labeller's answers to claims the set already has, and goes up as its own file.
TEACHING_SETS = ["declined", "mined", "retrieved", "multilingual"]

KEY = ("app_id", "review_id", "index")


def kappa(pairs: list[tuple[str, str]]) -> float:
    """Cohen's kappa over two labellers' answers to the same claims."""
    if not pairs:
        return 0.0
    agreed = sum(left == right for left, right in pairs) / len(pairs)
    names = {name for pair in pairs for name in pair}
    chance = sum(
        (sum(left == name for left, _ in pairs) / len(pairs))
        * (sum(right == name for _, right in pairs) / len(pairs))
        for name in names
    )
    return 0.0 if chance == 1.0 else (agreed - chance) / (1.0 - chance)


def agreement(rows: list[dict], again: list[dict]) -> dict:
    """How the second labeller's answers compare with the first's, computed at publish time.

    A card that carried the figures by hand was wrong within a week of being written, because
    every second reading moves them; the dataset says what its own rows say.
    """
    first = {tuple(row[name] for name in KEY): row for row in rows}
    both = [
        (first[key], row) for row in again if (key := tuple(row[name] for name in KEY)) in first
    ]
    if not both:
        return {"claims": 0}
    subject = [(one["subject"], two["subject"]) for one, two in both]
    polarity = [(one["polarity"], two["polarity"]) for one, two in both]
    contested = [(str(one["ambiguous"]), str(two["ambiguous"])) for one, two in both]
    return {
        "claims": len(both),
        "subject": sum(left == right for left, right in subject) / len(both),
        "subject_kappa": kappa(subject),
        "polarity": sum(left == right for left, right in polarity) / len(both),
        "polarity_kappa": kappa(polarity),
        "contested_kappa": kappa(contested),
    }


def dataset_card(labels: int, games: int, fingerprint: str, agreed: dict, unstamped: int) -> str:
    twice = (
        [
            f"{agreed['claims']:,} of the claims are labelled a second time by a different",
            "model, in `second-readings.jsonl`, and on those the two agree on the subject",
            f"{agreed['subject']:.0%} of the time (Cohen's kappa {agreed['subject_kappa']:.2f})",
            f"and on polarity {agreed['polarity']:.0%} (kappa {agreed['polarity_kappa']:.2f}).",
            "They agree far less about whether a claim is contested (kappa",
            f"{agreed['contested_kappa']:.2f}), which is a fact about the labellers rather than",
            "the claims and is documented in the repository.",
            *(
                [
                    f"{unstamped:,} of the second readings predate the field that records which",
                    "sheet a label answered, and carry no `taxonomy`; the first readings all do.",
                ]
                if unstamped
                else []
            ),
        ]
        if agreed["claims"]
        else ["No claim here has been labelled a second time yet."]
    )
    return "\n".join(
        [
            "---",
            "license: apache-2.0",
            "task_categories:",
            "- text-classification",
            "language:",
            "- multilingual",
            "pretty_name: Game review claims",
            "---",
            "",
            "# Game review claims",
            "",
            f"{labels:,} claims from {games} games on Steam, each labelled with the subject it",
            "is about, whether it is praise or a complaint, whether it is ironic, how sure the",
            "labeller was, whether the call was genuinely contested, and whether the claim was",
            "cut in the wrong place.",
            "",
            "## What is not here",
            "",
            "The review text. Each row carries a Steam review id and the byte offsets of one",
            "claim within that review's text, and nothing else that a reviewer wrote. The text is",
            "Valve's and the reviewer's, and it stays where it is; anyone with the review id can",
            "fetch it from Steam and cut the claim out at the offsets given. `steamgauge`",
            "does exactly that, and so can you.",
            "",
            "## How the labels were made",
            "",
            "By a language model, working from a written category sheet, one game at a time and",
            "without being told which game. This is a silver standard: what it measures is",
            "agreement between models, not correctness.",
            *twice,
            "",
            f"Data fingerprint `{fingerprint}`. Every row names the `taxonomy` its subject comes",
            "from and the model that wrote it in `produced_by`, both versioned in the",
            "repository. Two models disagree with each other about as often as either disagrees",
            "with the truth, so a row that could not say which one wrote it would be a row you",
            "could not split back apart. A row's offsets may name a span the current splitter",
            "does not cut as one claim; `steamgauge` counts those rather than scoring them",
            "against whatever now sits there. `subset` says how the claim's review was drawn:",
            "`random` and `stratified` are random draws of a game and the only rows any",
            "prevalence figure may count; the rest were drawn for being hard or rare.",
            "",
            "## Licence",
            "",
            "Apache-2.0. The labels are ours to give; the reviews are not, and are not here.",
        ]
    )


def pin(repo: str, hashes: dict[str, str]) -> None:
    """Writes the repository and hashes into reader.rs, where the tool reads them from."""
    source = READER_RS.read_text(encoding="utf-8")

    def replace_once(pattern: str, replacement: str, text: str) -> str:
        found = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
        if found[1] != 1:
            raise SystemExit(f"could not find where to pin {pattern!r} in {READER_RS}")
        return found[0]

    source = replace_once(
        r'(pub const PUBLISHED: Published = Published \{\s*repository: )"[^"]*"',
        rf'\1"{repo}"',
        source,
    )
    for name, digest in hashes.items():
        source = replace_once(
            rf'(remote: "{re.escape(name)}",\s*local: "{re.escape(name)}",\s*sha256: )"[^"]*"',
            rf'\1"{digest}"',
            source,
        )
    READER_RS.write_text(source, encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", required=True)
    parser.add_argument("--model-repo", required=True)
    parser.add_argument("--data-repo", required=True)
    parser.add_argument("--reference", default=str(REPO / "reference" / "claims"))
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="hash and check without uploading or pinning, to see what would change",
    )
    args = parser.parse_args()

    run = Path(args.run)
    for name in MODEL_FILES:
        if not (run / name).is_file():
            raise SystemExit(f"{run / name} is missing; export the run first")
    if not (run / "MODEL_CARD.md").is_file():
        raise SystemExit(f"{run} has no model card; export writes one")

    hashes = {name: sha256(run / name) for name in MODEL_FILES}
    for name, digest in hashes.items():
        print(f"{name:<16} {digest}")

    # The dataset is built from the reference sets, not from the training export. The export
    # carries each claim's text because training needs it; the reference sets carry the
    # claim's offsets within its review because that is all a label needs to be joined back.
    # Only the second shape leaves this machine.
    rows = []
    again = []
    for game in sorted(Path(args.reference).iterdir()):
        for draw in [game, *(game / name for name in TEACHING_SETS)]:
            if (draw / "labels.json").is_file():
                rows.extend(json.loads((draw / "labels.json").read_text(encoding="utf-8")))
        if (game / "second" / "labels.json").is_file():
            again.extend(json.loads((game / "second" / "labels.json").read_text(encoding="utf-8")))
    if not rows:
        raise SystemExit(f"no labelled sets under {args.reference}")

    # Checked rather than trusted, because a text field arriving in a later version of the
    # reference format would be published before anyone noticed.
    forbidden = {"text", "review", "claim", "body"}
    leaking = sorted(forbidden & set().union(*(row.keys() for row in rows + again)))
    if leaking:
        raise SystemExit(f"the reference sets carry review text in {leaking}; not publishing that")
    for name in (*KEY, "start", "end", "subject", "polarity", "produced_by"):
        if any(name not in row for row in rows + again):
            raise SystemExit(f"a row has no {name}; nothing can be joined back from it")
    # The second readings made before a label recorded its sheet carry none, and a fingerprint
    # guessed from a commit date would be the stamping the ingest refuses to do.
    if any("taxonomy" not in row for row in rows):
        raise SystemExit("a row has no taxonomy; the sheet it answered is not known")
    unstamped = sum("taxonomy" not in row for row in again)

    def jsonl(found: list[dict]) -> str:
        return "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in found)

    data = run / "claims.jsonl"
    data.write_text(jsonl(rows), encoding="utf-8")
    twice = run / "second-readings.jsonl"
    twice.write_text(jsonl(again), encoding="utf-8")
    games = len({row["app_id"] for row in rows})
    record = json.loads((run / "run.json").read_text(encoding="utf-8"))
    agreed = agreement(rows, again)
    card = dataset_card(len(rows), games, record["data_fingerprint"], agreed, unstamped)

    # The pin is a promise that these hashes can be fetched from that repository, so a dry
    # run, which uploads nothing, must not write it: a tool built from a pin nobody published
    # fetches from a repository that does not hold the files.
    if args.dry_run:
        print(
            f"\nwould upload {len(rows):,} rows from {games} games and {len(again):,} second "
            f"readings to {args.data_repo}"
        )
        print(f"would upload {', '.join(MODEL_FILES)} to {args.model_repo}")
        print(f"would pin {args.model_repo} in {READER_RS.relative_to(REPO)}")
        return

    from huggingface_hub import HfApi

    api = HfApi()
    api.create_repo(args.model_repo, repo_type="model", exist_ok=True)
    api.create_repo(args.data_repo, repo_type="dataset", exist_ok=True)

    for name in MODEL_FILES:
        api.upload_file(
            path_or_fileobj=str(run / name),
            path_in_repo=name,
            repo_id=args.model_repo,
            repo_type="model",
        )
    api.upload_file(
        path_or_fileobj=str(run / "MODEL_CARD.md"),
        path_in_repo="README.md",
        repo_id=args.model_repo,
        repo_type="model",
    )
    for path in (data, twice):
        api.upload_file(
            path_or_fileobj=str(path),
            path_in_repo=path.name,
            repo_id=args.data_repo,
            repo_type="dataset",
        )
    api.upload_file(
        path_or_fileobj=card.encode("utf-8"),
        path_in_repo="README.md",
        repo_id=args.data_repo,
        repo_type="dataset",
    )

    # Hashed before the upload, from the same files the upload read. If anything between here
    # and the hub changes a byte, the pin will not match what comes down, and the tool refuses
    # it at fetch time, which is the failure this whole arrangement exists to make loud.
    pin(args.model_repo, hashes)
    print(f"\npublished to {args.model_repo} and {args.data_repo}")
    print(f"pinned in {READER_RS.relative_to(REPO)}; rebuild and commit")


if __name__ == "__main__":
    main()
