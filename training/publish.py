"""Publishes a release to Hugging Face, and pins the tool to it in the same motion.

Five repositories under one owner, each given one commit holding every file and tagged with the
release (`v1`, `v2`, ...):

- `game-review-reader` and `game-review-reader-small`: the sizes that ship, from
  `reference/reader-sizes.json`, each with a card written from its run.
- `steamgauge-search-encoder` and `steamgauge-search-reranker`: the two exports search by
  meaning runs, from the model cache, exactly the files `search_models.rs` pins.
- `game-review-claims`, a dataset: every label, the second readings, the person's gold answers
  and judgements, what Steam shows about each labelled review, and `fetch_text.py`. Never the
  review text and never the author: the words are the reviewers' and the author is a person.

Publishing without pinning would be the one thing worse than not publishing. The tool fetches
every file from the commit it is pinned to and checks it against its hash, so a model uploaded
and not pinned is a model nobody can use, and a pin typed by hand is a hash somebody typed. This
pins the commits the uploads made and the hashes of the exact bytes uploaded, into `reader.rs`
and `search_models.rs`.

    steamgauge export-review-facts --to reviews.jsonl
    python publish.py --owner <user> --tag v1 --reviews reviews.jsonl [--dry-run]

Needs a Hugging Face token with write access, from `hf auth login` or HF_TOKEN.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from pathlib import Path

import sizes

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
CORE = REPO / "crates" / "steamgauge-core" / "src"
READER_RS = CORE / "reader.rs"
SEARCH_RS = CORE / "search_models.rs"

MODEL_FILES = ["model.onnx", "tokenizer.json", "reader.json"]
SEARCH_FILES = ["model.onnx", "tokenizer.json"]
DATASET = "game-review-claims"
# Each size's repository, and the constant in reader.rs its files are pinned in.
READERS = {
    "standard": ("game-review-reader", "STANDARD_FILES"),
    "small": ("game-review-reader-small", "SMALL_FILES"),
}
# Each search model's cache directory, repository, constant in search_models.rs and what it was
# exported from.
SEARCH = {
    "search-encoder": ("steamgauge-search-encoder", "ENCODER", "Qwen/Qwen3-Embedding-0.6B"),
    "search-reranker": ("steamgauge-search-reranker", "RERANKER", "Qwen/Qwen3-Reranker-0.6B"),
}
TAG = re.compile(r"v[1-9][0-9]*")


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

# Checked rather than trusted, because a text or author field arriving in a later version of a
# format would be published before anyone noticed.
FORBIDDEN = {"text", "review", "claim", "body", "author", "author_steamid", "steamid"}


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


def dataset_card(
    labels: int,
    games: int,
    fingerprint: str,
    agreed: dict,
    unstamped: int,
    reviews: int = 0,
    gold: int = 0,
) -> str:
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
            "license: cc-by-4.0",
            "task_categories:",
            "- text-classification",
            "language:",
            "- multilingual",
            "tags:",
            "- steam",
            "- game-reviews",
            "- aspect-based-sentiment-analysis",
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
            "## Files",
            "",
            "- `claims.jsonl`: one label per claim. A claim is the bytes `start` to `end` of its",
            "  review's text, as UTF-8.",
            "- `second-readings.jsonl`: the claims a second labeller answered too.",
            f"- `gold.jsonl`: {gold:,} claims one person adjudicated, and `acceptable.jsonl`, that",
            "  person's judgement of which other subjects would also do for them.",
            f"- `reviews.jsonl`: what Steam shows about each of the {reviews:,} labelled reviews:",
            "  language, dates, whether it recommends the game, its votes, how the game was",
            "  bought, the playtime shown on the review, and `text_sha256`, the SHA-256 of the",
            "  text the labels were written against.",
            "- `fetch_text.py`: fetches the text back from Steam on your own machine.",
            "",
            "## What is not here",
            "",
            "The review text and the author. The words are the reviewers', and the author is a",
            "person; neither is ours to publish. Every review is public on Steam, and",
            "`python fetch_text.py` fetches each labelled one by its id, checks it against",
            "`text_sha256` and writes every claim with its text cut out at its offsets. A review",
            "deleted since it was labelled cannot come back, and one edited since no longer",
            "matches its fingerprint; both are counted and left out rather than cut at offsets",
            "that no longer name the same words. The script needs nothing beyond Python.",
            "",
            "## How the labels were made",
            "",
            "By a language model, working from a written category sheet, one game at a time and",
            "without being told which game. This is a silver standard: what it measures is",
            "agreement between models, not correctness. The gold answers are the measure",
            "against a person.",
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
            "CC BY 4.0. The labels are ours to give; the reviews are not, and are not here.",
        ]
    )


def search_card(name: str, base: str, owner: str) -> str:
    """The card for one of the two search exports: Qwen's weights, unchanged, in the shape the
    tool runs."""
    encoder = name == "search-encoder"
    gives = (
        "each text's unit vector at its last token, 1,024 numbers, for nearest-neighbour search"
        if encoder
        else "the model's 'yes' against its 'no' at the last token, for a query and a passage "
        "laid out in the reranker's own prompt, two rows of its output layer being all that "
        "is kept of it"
    )
    return "\n".join(
        [
            "---",
            "license: apache-2.0",
            f"base_model: {base}",
            "tags:",
            "- onnx",
            "- steam",
            "- game-reviews",
            f"pipeline_tag: {'feature-extraction' if encoder else 'text-ranking'}",
            "---",
            "",
            f"# {'Search encoder' if encoder else 'Search reranker'} for SteamGauge",
            "",
            f"[`{base}`](https://huggingface.co/{base}), its weights unchanged, exported to one",
            "half-precision ONNX graph that gives " + gives + ".",
            "",
            "SteamGauge searches what a game's reviewers said by meaning: the encoder gathers",
            "the claims nearest a search, and the reranker reads each beside it and orders",
            "them. Neither had a full-precision ONNX export of the shape the tool runs: the",
            "embedding's only one carries a text generator's cache as fifty-six inputs, and the",
            "reranker has only quantised ones. Checked against the full-precision model on",
            "DirectML: cosine at least 0.99988 for the encoder, score drift at most 0.0076 for",
            "the reranker.",
            "",
            "The tool fetches these files from a pinned commit and checks each against its",
            f"SHA-256. The labelled data they were chosen on is [`{owner}/{DATASET}`]"
            f"(https://huggingface.co/datasets/{owner}/{DATASET}).",
            "",
            "## Licence",
            "",
            f"Apache-2.0, as is `{base}`, whose authors are Qwen. All credit for the model is",
            "theirs; this is a conversion.",
            "",
        ]
    )


def pins_in(source: str, constant: str) -> dict[str, str]:
    """The file hashes a Rust constant pins, by remote name."""
    found = re.search(rf"const {constant}\b.*?(?=\n(?:pub )?const |\Z)", source, re.DOTALL)
    if not found:
        raise SystemExit(f"no constant {constant} to read pins from")
    return dict(
        re.findall(r'remote: "([^"]+)",\s*local: "[^"]+",\s*sha256: "([^"]*)"', found.group(0))
    )


def pin_reader(
    source: str, size: str, constant: str, repo: str, revision: str, hashes: dict
) -> str:
    """reader.rs with one size pinned to a repository, a commit and its files' hashes."""
    source, count = re.subn(
        rf'(name: "{re.escape(size)}",.*?published: Published \{{\s*repository: )"[^"]*"'
        r'(,\s*revision: )"[^"]*"',
        rf'\g<1>"{repo}"\g<2>"{revision}"',
        source,
        count=1,
        flags=re.DOTALL,
    )
    if count != 1:
        raise SystemExit(f"could not find where to pin the {size} reader in {READER_RS}")
    for name, digest in hashes.items():
        source, count = re.subn(
            rf'(const {constant}\b.*?remote: "{re.escape(name)}",\s*local: "[^"]+",\s*sha256: )"[^"]*"',
            rf'\g<1>"{digest}"',
            source,
            count=1,
            flags=re.DOTALL,
        )
        if count != 1:
            raise SystemExit(f"could not find where to pin {name} in {constant}")
    return source


def pin_search(source: str, constant: str, repo: str, revision: str) -> str:
    """search_models.rs with one model pinned to a repository and a commit."""
    source, count = re.subn(
        rf'(pub const {constant}: Model = Model \{{\s*name: "[^"]*",\s*repository: )"[^"]*"'
        r'(,\s*revision: )"[^"]*"',
        rf'\g<1>"{repo}"\g<2>"{revision}"',
        source,
        count=1,
    )
    if count != 1:
        raise SystemExit(f"could not find where to pin {constant} in {SEARCH_RS}")
    return source


def read_labels(reference: Path) -> tuple[list[dict], list[dict], list[dict], list[dict]]:
    """Every label, every second reading, the gold answers and the judgements beside them."""
    rows, again, gold, acceptable = [], [], [], []

    def load(path: Path) -> list[dict]:
        return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else []

    for game in sorted(path for path in reference.iterdir() if path.is_dir()):
        for draw in [game, *(game / name for name in TEACHING_SETS)]:
            rows.extend(load(draw / "labels.json"))
        again.extend(load(game / "second" / "labels.json"))
        gold.extend(load(game / "gold" / "labels.json"))
        app = int(game.name)
        acceptable.extend({"app_id": app, **one} for one in load(game / "gold" / "acceptable.json"))
    return rows, again, gold, acceptable


def checked(rows: list[dict], what: str) -> None:
    leaking = sorted(FORBIDDEN & set().union(*(row.keys() for row in rows)))
    if leaking:
        raise SystemExit(f"{what} carry {leaking}; not publishing that")


def jsonl(found: list[dict]) -> str:
    return "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in found)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--owner", required=True, help="the Hugging Face user or organisation")
    parser.add_argument("--tag", required=True, help="the release, v1, v2, ...")
    parser.add_argument(
        "--reviews", required=True, help="what `steamgauge export-review-facts` wrote"
    )
    parser.add_argument("--reference", default=str(REPO / "reference" / "claims"))
    parser.add_argument(
        "--cache",
        default=str(Path(os.environ.get("LOCALAPPDATA", "")) / "steamgauge" / "models"),
        help="the model cache the search exports sit in",
    )
    parser.add_argument("--staging", default=str(HERE / "data" / "release"))
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="build and check everything without uploading or pinning",
    )
    args = parser.parse_args()
    if not TAG.fullmatch(args.tag):
        raise SystemExit(f"{args.tag!r} is not a release tag; they are v1, v2, ...")
    staging = Path(args.staging)
    staging.mkdir(parents=True, exist_ok=True)
    reader_rs = READER_RS.read_text(encoding="utf-8")
    search_rs = SEARCH_RS.read_text(encoding="utf-8")

    # What goes where: each repository's files as (path in the repository, path here).
    uploads: dict[str, tuple[str, list[tuple[str, Path]]]] = {}
    reader_hashes: dict[str, dict[str, str]] = {}

    import export

    for size in sizes.load()["sizes"]:
        if not size["ships"]:
            continue
        repo, _ = READERS[size["name"]]
        run = HERE / "runs" / size["run"]
        for name in MODEL_FILES:
            if not (run / name).is_file():
                raise SystemExit(f"{run / name} is missing; export the run first")
        record = json.loads((run / "run.json").read_text(encoding="utf-8"))
        card = staging / f"{repo}.md"
        card.write_text(
            export.model_card(
                run,
                {
                    "license": "apache-2.0",
                    "base_model": record["backbone"],
                    "pipeline_tag": "text-classification",
                    "language": ["multilingual"],
                    "datasets": [f"{args.owner}/{DATASET}"],
                    "tags": ["onnx", "steam", "game-reviews", "aspect-based-sentiment-analysis"],
                },
            ),
            encoding="utf-8",
        )
        reader_hashes[size["name"]] = {name: sha256(run / name) for name in MODEL_FILES}
        uploads[repo] = (
            "model",
            [*((name, run / name) for name in MODEL_FILES), ("README.md", card)],
        )

    for name, (repo, constant, base) in SEARCH.items():
        directory = Path(args.cache) / name
        pinned = pins_in(search_rs, constant)
        for file in SEARCH_FILES:
            if not (directory / file).is_file():
                raise SystemExit(f"{directory / file} is missing; export_search.py writes it")
            # Only the bytes the tool already pins go up: an export run again is not the same
            # file, and publishing one would publish something nothing was measured on.
            if sha256(directory / file) != pinned[file]:
                raise SystemExit(f"{directory / file} is not the file {constant} pins")
        card = staging / f"{repo}.md"
        card.write_text(search_card(name, base, args.owner), encoding="utf-8")
        uploads[repo] = (
            "model",
            [*((file, directory / file) for file in SEARCH_FILES), ("README.md", card)],
        )

    # The dataset is built from the reference sets, not from the training export. The export
    # carries each claim's text because training needs it; the reference sets carry the claim's
    # offsets within its review because that is all a label needs to be joined back. Only the
    # second shape leaves this machine.
    rows, again, gold, acceptable = read_labels(Path(args.reference))
    if not rows:
        raise SystemExit(f"no labelled sets under {args.reference}")
    reviews = [
        json.loads(line)
        for line in Path(args.reviews).read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    for found, what in (
        (rows, "labels"),
        (again, "second readings"),
        (gold, "gold answers"),
        (acceptable, "judgements"),
        (reviews, "review facts"),
    ):
        checked(found, f"the {what}")
    for name in (*KEY, "start", "end", "subject", "polarity", "produced_by"):
        if any(name not in row for row in rows + again):
            raise SystemExit(f"a row has no {name}; nothing can be joined back from it")
    # The second readings made before a label recorded its sheet carry none, and a fingerprint
    # guessed from a commit date would be the stamping the ingest refuses to do.
    if any("taxonomy" not in row for row in rows):
        raise SystemExit("a row has no taxonomy; the sheet it answered is not known")
    described = {row["review_id"] for row in reviews}
    bare = {row["review_id"] for row in rows + again + gold} - described
    if bare:
        raise SystemExit(
            f"{len(bare):,} labelled reviews have no facts in {args.reviews}; a label nobody can "
            "check against its text is not one to publish. Run export-review-facts again."
        )
    unstamped = sum("taxonomy" not in row for row in again)
    standard = (
        HERE / "runs" / next(s["run"] for s in sizes.load()["sizes"] if s["name"] == "standard")
    )
    fingerprint = json.loads((standard / "run.json").read_text(encoding="utf-8"))[
        "data_fingerprint"
    ]
    files = {
        "claims.jsonl": rows,
        "second-readings.jsonl": again,
        "gold.jsonl": gold,
        "acceptable.jsonl": acceptable,
        "reviews.jsonl": reviews,
    }
    for name, found in files.items():
        (staging / name).write_text(jsonl(found), encoding="utf-8")
    card = staging / f"{DATASET}.md"
    card.write_text(
        dataset_card(
            len(rows),
            len({row["app_id"] for row in rows}),
            fingerprint,
            agreement(rows, again),
            unstamped,
            reviews=len(reviews),
            gold=len(gold),
        ),
        encoding="utf-8",
    )
    uploads[DATASET] = (
        "dataset",
        [
            *((name, staging / name) for name in files),
            ("fetch_text.py", HERE / "fetch_text.py"),
            ("README.md", card),
        ],
    )

    for repo, (kind, contents) in uploads.items():
        print(
            f"{kind:<8} {args.owner}/{repo} {args.tag}: {', '.join(path for path, _ in contents)}"
        )
    if args.dry_run:
        print(f"\nstaged in {staging}; nothing uploaded and nothing pinned")
        return

    from huggingface_hub import CommitOperationAdd, HfApi

    api = HfApi()
    commits = {}
    for repo, (kind, contents) in uploads.items():
        repo_id = f"{args.owner}/{repo}"
        api.create_repo(repo_id, repo_type=kind, exist_ok=True, private=False)
        made = api.create_commit(
            repo_id,
            repo_type=kind,
            operations=[
                CommitOperationAdd(path_in_repo=path, path_or_fileobj=str(local))
                for path, local in contents
            ],
            commit_message=f"Release {args.tag}",
        )
        api.create_tag(repo_id, repo_type=kind, tag=args.tag, revision=made.oid)
        commits[repo] = made.oid
        print(f"published {repo_id} at {made.oid} as {args.tag}")

    # Pinned to the commits the uploads made and the hashes taken before them from the same
    # files. If anything between here and the hub changes a byte, the pin will not match what
    # comes down and the tool refuses it, which is the failure this arrangement exists to make
    # loud.
    for size, hashes in reader_hashes.items():
        repo, constant = READERS[size]
        reader_rs = pin_reader(
            reader_rs, size, constant, f"{args.owner}/{repo}", commits[repo], hashes
        )
    for name, (repo, constant, _) in SEARCH.items():
        search_rs = pin_search(search_rs, constant, f"{args.owner}/{repo}", commits[repo])
    READER_RS.write_text(reader_rs, encoding="utf-8")
    SEARCH_RS.write_text(search_rs, encoding="utf-8")
    print(f"\npinned in {READER_RS.relative_to(REPO)} and {SEARCH_RS.relative_to(REPO)}")


if __name__ == "__main__":
    main()
