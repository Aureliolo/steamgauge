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
from collections import Counter
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
# `claimset::TEACHING_SETS`. Named rather than "every subdirectory holding labels": the second
# readings answer claims the set already has, and go up as their own file.
TEACHING_SETS = ["declined", "mined", "retrieved", "multilingual"]
# `claimset::SECOND_READINGS`, in its order: the first to answer a claim is its second reading.
SECOND_READINGS = ["second", "opus"]

KEY = ("app_id", "review_id", "index")

# Every field a published file may carry. A field outside these stops the publish rather than
# going up: a text or author field arriving in a later version of a format would otherwise be
# published before anyone noticed.
LABEL_FIELDS = {
    *KEY,
    "start",
    "end",
    "language",
    "subset",
    "taxonomy",
    "produced_by",
    "subject",
    "polarity",
    "also",
    "ironic",
    "confidence",
    "ambiguous",
    "split_wrong",
}
JUDGEMENT_FIELDS = {*KEY, "subject", "acceptable", "by"}
REVIEW_FIELDS = {
    "app_id",
    "review_id",
    "language",
    "created",
    "updated",
    "voted_up",
    "votes_up",
    "votes_funny",
    "weighted_vote_score",
    "comment_count",
    "steam_purchase",
    "received_for_free",
    "written_during_early_access",
    "refunded",
    "primarily_steam_deck",
    "playtime_at_review_minutes",
    "text_sha256",
    "text_bytes",
}


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
        "ambiguous": sum(left == right for left, right in contested) / len(both),
        "contested_kappa": kappa(contested),
    }


def dataset_card(
    labels: int,
    games: int,
    agreed: dict,
    reviews: int = 0,
    gold: int = 0,
    labellers: dict[str, int] | None = None,
) -> str:
    twice = (
        [
            "",
            f"{agreed['claims']:,} claims have a second label from a different model, in",
            "`second-readings.jsonl`. On those claims the two labels agree:",
            "",
            "| Field | Agreement | Cohen's kappa |",
            "|---|---|---|",
            f"| `subject` | {agreed['subject']:.0%} | {agreed['subject_kappa']:.2f} |",
            f"| `polarity` | {agreed['polarity']:.0%} | {agreed['polarity_kappa']:.2f} |",
            f"| `ambiguous` | {agreed['ambiguous']:.0%} | {agreed['contested_kappa']:.2f} |",
        ]
        if agreed["claims"]
        else ["No claim has a second label yet."]
    )
    by = ", ".join(f"{name} ({count:,})" for name, count in (labellers or {}).items())
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
            f"{labels:,} claims from {reviews:,} Steam reviews of {games} games, each labelled",
            "with its subject and whether it is praise, a complaint or neutral. A claim is one",
            "point a reviewer makes. Part of",
            "[SteamGauge](https://github.com/Aureliolo/steamgauge).",
            "",
            "The dataset contains no review text and no author information. `fetch_text.py`",
            "fetches the labelled reviews from Steam's public review endpoint and adds each",
            "claim's text.",
            "",
            "## Files",
            "",
            "- `claims.jsonl`: one label per claim.",
            "- `second-readings.jsonl`: a second label, from a different model, for some claims.",
            f"- `gold.jsonl`: {gold:,} claims labelled by one person.",
            "- `acceptable.jsonl`: for some gold claims, whether a subject other than the gold",
            "  one is also acceptable, judged by the same person.",
            "- `reviews.jsonl`: one row per labelled review.",
            "- `fetch_text.py`: adds the text to the claims. Python standard library only.",
            "",
            "## Fields",
            "",
            "`claims.jsonl`, `second-readings.jsonl` and `gold.jsonl`:",
            "",
            "| Field | Meaning |",
            "|---|---|",
            "| `app_id`, `review_id` | The Steam game and review. |",
            "| `index` | The claim's position among its review's claims. |",
            "| `start`, `end` | The claim's UTF-8 byte offsets in the review text. |",
            "| `language` | The review's language, as Steam gives it. |",
            "| `subject` | What the claim is about. |",
            "| `polarity` | `praise`, `complaint` or `neutral`. |",
            "| `also` | Further subjects the claim is about, each with its own polarity. |",
            "| `ironic` | The claim means the opposite of what it says; `polarity` is what it "
            "means. |",
            "| `confidence` | The labeller's confidence in the subject: `high`, `medium` or "
            "`low`. |",
            "| `ambiguous` | The category rules do not settle which subject the claim is about. |",
            "| `split_wrong` | The claim is cut in the wrong place. |",
            "| `produced_by` | The model that wrote the label, or `a person`. |",
            "| `taxonomy` | The category sheet the label was written against: a hash of the "
            "sheet, or `core-5` or `core-6`. Null where it was not recorded. |",
            "| `subset` | How the claim was drawn (below). |",
            "",
            "`subset`:",
            "",
            "- `random`: a random draw of the game's reviews. Only these rows estimate how often",
            "  a subject comes up.",
            "- `declined`: claims a trained reader did not answer.",
            "- `mined`: claims found by keyword for rarely mentioned subjects.",
            "- `retrieved`: claims found by meaning for rarely mentioned subjects.",
            "- `multilingual`: claims from reviews in languages other than English.",
            "",
            "`acceptable.jsonl`: `app_id`, `review_id`, `index`, `subject`, `acceptable`, `by`.",
            "",
            "`reviews.jsonl`: `app_id`, `review_id`, `language`, `created` and `updated` (Unix",
            "time), `voted_up`, `votes_up`, `votes_funny`, `weighted_vote_score`,",
            "`comment_count`, `steam_purchase`, `received_for_free`,",
            "`written_during_early_access`, `refunded`, `primarily_steam_deck`,",
            "`playtime_at_review_minutes`, and the SHA-256 (`text_sha256`) and length in bytes",
            "(`text_bytes`) of the review text the labels were written against.",
            "",
            "## Text",
            "",
            "```",
            "python fetch_text.py --data . --to claims-with-text.jsonl",
            "```",
            "",
            "`--claims gold.jsonl` or `--claims second-readings.jsonl` does the same for another",
            "label file. Reviews deleted or edited on Steam since labelling are left out.",
            "",
            "## Labels",
            "",
            "Written by Claude models from a category sheet, one game at a time, without the",
            "game's name" + (f": {by}." if by else "."),
            *twice,
            "",
            "The category sheet is [`taxonomy.rs`](https://github.com/Aureliolo/steamgauge/blob/"
            "main/crates/steamgauge-core/src/taxonomy.rs) in the SteamGauge repository.",
            "",
            "## Licence",
            "",
            "CC BY 4.0. Review text is not included.",
        ]
    )


def search_card(name: str, base: str) -> str:
    """The card for one of the two search exports: Qwen's weights, unchanged, in the shape the
    tool runs."""
    encoder = name == "search-encoder"
    use = (
        [
            "- Returns `vector`: a normalised 1,024-dimension embedding per text, taken at the",
            "  last token.",
            "- Searches are written as `Instruct: {instruction}\\nQuery: {query}`; the passages",
            "  searched are embedded as they are.",
            "- Parity with the original model on DirectML: cosine similarity at least 0.99988.",
        ]
        if encoder
        else [
            "- Returns `score`: the probability that a passage answers a query, from the",
            '  model\'s "yes" and "no" at the last token. Only those two rows of the output',
            "  layer are kept.",
            "- Input is the reranker's own prompt: system and user turns holding `<Instruct>`,",
            "  `<Query>` and `<Document>`, as on the original model's card.",
            "- Parity with the original model on DirectML: scores within 0.0076.",
        ]
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
            f"# SteamGauge {'search encoder' if encoder else 'search reranker'}",
            "",
            f"[`{base}`](https://huggingface.co/{base}) as a half-precision ONNX graph, weights",
            "unchanged. [SteamGauge](https://github.com/Aureliolo/steamgauge) uses it to search",
            "Steam reviews by meaning: the encoder finds the claims closest to a search, and the",
            "reranker orders them.",
            "",
            "## Use",
            "",
            "- `model.onnx` takes `input_ids` and `attention_mask` from `tokenizer.json`, padded",
            "  on the right.",
            *use,
            "",
            "## Licence",
            "",
            f"Apache-2.0, as is `{base}`. The model is Qwen's work.",
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
    source: str, size: str, constant: str, release: tuple[str, str, str], files: dict
) -> str:
    """reader.rs with one size pinned to a repository, a commit and its tag, and each file
    (by remote name) to its hash and length."""
    repo, revision, tag = release
    source, count = re.subn(
        rf'(name: "{re.escape(size)}",.*?published: Published \{{\s*repository: )"[^"]*"'
        r'(,\s*revision: )"[^"]*"(,\s*release: )"[^"]*"',
        rf'\g<1>"{repo}"\g<2>"{revision}"\g<3>"{tag}"',
        source,
        count=1,
        flags=re.DOTALL,
    )
    if count != 1:
        raise SystemExit(f"could not find where to pin the {size} reader in {READER_RS}")
    for name, (digest, length) in files.items():
        source, count = re.subn(
            rf'(const {constant}\b.*?remote: "{re.escape(name)}",\s*local: "[^"]+",\s*sha256: )'
            r'"[^"]*"(,\s*bytes: )[0-9_]+',
            rf'\g<1>"{digest}"\g<2>{length:_}',
            source,
            count=1,
            flags=re.DOTALL,
        )
        if count != 1:
            raise SystemExit(f"could not find where to pin {name} in {constant}")
    return source


def pin_search(source: str, constant: str, release: tuple[str, str, str]) -> str:
    """search_models.rs with one model pinned to a repository, a commit and its tag."""
    repo, revision, tag = release
    source, count = re.subn(
        rf'(pub const {constant}: Model = Model \{{\s*name: "[^"]*",\s*repository: )"[^"]*"'
        r'(,\s*revision: )"[^"]*"(,\s*release: )"[^"]*"',
        rf'\g<1>"{repo}"\g<2>"{revision}"\g<3>"{tag}"',
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
        answered: set[tuple] = set()
        for reading in SECOND_READINGS:
            for row in load(game / reading / "labels.json"):
                if (key := tuple(row[name] for name in KEY)) not in answered:
                    answered.add(key)
                    again.append(row)
        gold.extend(load(game / "gold" / "labels.json"))
        app = int(game.name)
        acceptable.extend({"app_id": app, **one} for one in load(game / "gold" / "acceptable.json"))
    return rows, again, gold, acceptable


def checked(rows: list[dict], fields: set[str], what: str) -> None:
    unknown = sorted(set().union(*(row.keys() for row in rows)) - fields)
    if unknown:
        raise SystemExit(f"{what} carry {unknown}, which nothing says may be published")


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
    parser.add_argument("--runs", default=str(HERE / "runs"), help="where the exported runs are")
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
    reader_files: dict[str, dict[str, tuple[str, int]]] = {}

    import export

    for size in sizes.load()["sizes"]:
        if not size["ships"]:
            continue
        repo, _ = READERS[size["name"]]
        run = Path(args.runs) / size["run"]
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
                title=export.READER_NAME
                + ("" if size["name"] == "standard" else f" ({size['name']})"),
            ),
            encoding="utf-8",
        )
        reader_files[size["name"]] = {
            name: (sha256(run / name), (run / name).stat().st_size) for name in MODEL_FILES
        }
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
        card.write_text(search_card(name, base), encoding="utf-8")
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
    for found, fields, what in (
        (rows, LABEL_FIELDS, "labels"),
        (again, LABEL_FIELDS, "second readings"),
        (gold, LABEL_FIELDS, "gold answers"),
        (acceptable, JUDGEMENT_FIELDS, "judgements"),
        (reviews, REVIEW_FIELDS, "review facts"),
    ):
        checked(found, fields, f"the {what}")
    # A second reading whose sheet was not recorded holds either nothing or an empty string;
    # both go up as one null so a reader of the file has a single case to handle.
    for row in again:
        row["taxonomy"] = row.get("taxonomy") or None
    for name in (*KEY, "start", "end", "subject", "polarity", "produced_by"):
        if any(name not in row for row in rows + again):
            raise SystemExit(f"a row has no {name}; nothing can be joined back from it")
    # Only second readings made before a label recorded its sheet may lack one, and a fingerprint
    # guessed from a commit date would be the stamping the ingest refuses to do.
    if any(not row.get("taxonomy") for row in rows + gold):
        raise SystemExit("a row has no taxonomy; the sheet it answered is not known")
    described = {row["review_id"] for row in reviews}
    bare = {row["review_id"] for row in rows + again + gold} - described
    if bare:
        raise SystemExit(
            f"{len(bare):,} labelled reviews have no facts in {args.reviews}; a label nobody can "
            "check against its text is not one to publish. Run export-review-facts again."
        )
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
            agreement(rows, again),
            reviews=len(reviews),
            gold=len(gold),
            labellers=dict(Counter(row["produced_by"] for row in rows).most_common()),
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
    for size, files in reader_files.items():
        repo, constant = READERS[size]
        release = (f"{args.owner}/{repo}", commits[repo], args.tag)
        reader_rs = pin_reader(reader_rs, size, constant, release, files)
    for name, (repo, constant, _) in SEARCH.items():
        release = (f"{args.owner}/{repo}", commits[repo], args.tag)
        search_rs = pin_search(search_rs, constant, release)
    # The sources are kept with Unix line ends, which Windows would otherwise turn every one of.
    READER_RS.write_text(reader_rs, encoding="utf-8", newline="\n")
    SEARCH_RS.write_text(search_rs, encoding="utf-8", newline="\n")
    print(f"\npinned in {READER_RS.relative_to(REPO)} and {SEARCH_RS.relative_to(REPO)}")


if __name__ == "__main__":
    main()
