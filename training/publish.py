"""Publishes a release to Hugging Face, and pins the tool to it in the same motion.

Five repositories under one owner, each given one commit holding every file and tagged with the
release (`v1`, `v2`, ...):

- `game-review-reader` and `game-review-reader-small`: the sizes that ship, from
  `reference/reader-sizes.json`, each with a card written from its run.
- `steamgauge-search-encoder` and `steamgauge-search-reranker`: the two exports search by
  meaning runs, from the model cache, exactly the files `search_models.rs` pins.
- `game-review-claims`, a dataset: every label, the second labels, the person's gold answers
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
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import sizes

# One row of a labels, judgements or review-facts file: JSON whose fields the format fixes.
Row = dict[str, Any]

# How many bytes a file is read in while hashing it.
HASH_CHUNK = 1 << 20

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
WRITER_RS = CORE / "writer.rs"
# The writer's cache directory, repository, constant in writer.rs and what it was exported from.
WRITER = ("writer", "steamgauge-writer", "WRITER", "Qwen/Qwen3-4B-Instruct-2507")
WRITER_FILES = ["model.onnx", "model.onnx.data", "tokenizer.json"]
REPOSITORIES = [
    *(repo for repo, _ in READERS.values()),
    *(repo for repo, _, _ in SEARCH.values()),
    WRITER[1],
    DATASET,
]
TAG = re.compile(r"v[1-9][0-9]*")


def sha256(path: Path) -> str:
    """Return the hex SHA-256 of the file at `path`."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(HASH_CHUNK), b""):
            digest.update(chunk)
    return digest.hexdigest()


# The draws beside a game's random one that train the reader, as the Rust side names them in
# `claimset::TEACHING_SETS`. Named rather than "every subdirectory holding labels": the second
# labels answer claims the set already has, and go up as their own file.
TEACHING_SETS = ["declined", "keyword", "retrieved", "multilingual"]
# `claimset::SECOND_LABELS`, in its order: the first to answer a claim gives its second label.
SECOND_LABELS = ["second", "opus"]

KEY = ("app_id", "review_id", "index")
# What two labels of one claim share. Each draw numbers a review's claims by its own handout, so
# the index tells a published row apart and never joins two labels.
CLAIM = ("app_id", "review_id", "start", "end")


def claim_of(row: Row) -> tuple[Any, ...]:
    """Return what names the claim `row` is about, stopping the publish when a field is missing."""
    for name in CLAIM:
        if name not in row:
            msg = f"a row has no {name}; nothing can be joined back from it"
            raise SystemExit(msg)
    return tuple(row[name] for name in CLAIM)


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


def agreement(rows: list[Row], again: list[Row]) -> dict[str, float]:
    """Compare the second labeller's answers with the first's, computed at publish time.

    A card that carried the figures by hand was wrong within a week of being written, because
    every second reading moves them. The dataset says what its own rows say.
    """
    first: dict[tuple[Any, ...], Row] = {}
    for row in rows:
        first.setdefault(claim_of(row), row)
    both = [(first[key], row) for row in again if (key := claim_of(row)) in first]
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


@dataclass(frozen=True)
class Credits:
    """Who labelled the dataset: claims per gold handle, claims per labeller and handle names."""

    gold: dict[str, int] = field(default_factory=dict[str, int])
    labellers: dict[str, int] = field(default_factory=dict[str, int])
    names: dict[str, str] = field(default_factory=dict[str, str])


def dataset_card(
    labels: int,
    games: int,
    agreed: dict[str, float],
    reviews: int = 0,
    labelled_by: Credits | None = None,
) -> str:
    """Return the dataset's card, with the agreement figures its own rows give."""
    people_by = labelled_by or Credits()
    gold = people_by.gold
    labellers = people_by.labellers
    names = people_by.names
    twice = (
        [
            "",
            f"{agreed['claims']:,} claims have a second label from a different model, in",
            "`second-labels.jsonl`. On those claims the two labels agree:",
            "",
            "| Field | Agreement | Cohen's kappa |",
            "|---|---|---|",
            f"| `subject` | {agreed['subject']:.0%} | {agreed['subject_kappa']:.2f} |",
            f"| `polarity` | {agreed['polarity']:.0%} | {agreed['polarity_kappa']:.2f} |",
            f"| `ambiguous` | {agreed['ambiguous']:.0%} | {agreed['contested_kappa']:.2f} |",
        ]
        if agreed["claims"]
        else ["No claim has a second label."]
    )
    by = ", ".join(f"{name} ({count:,})" for name, count in labellers.items())
    people = gold
    credited = [(f"{names[handle]} (`{handle}`", count) for handle, count in people.items()]
    answered = ", ".join(f"{name}, {count:,})" for name, count in credited)
    gold_file = (
        f"- `gold.jsonl`: {sum(people.values()):,} answers from {len(people)} people, "
        f"each named in `produced_by`: {answered}. Where two people answered a claim, both "
        "answers are kept."
        if len(people) > 1
        else f"- `gold.jsonl`: {sum(people.values()):,} claims labelled by one person"
        + "".join(f", {name})" for name, _ in credited)
        + "."
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
            f"{labels:,} claims from {reviews:,} Steam reviews of {games} games, each labelled",
            "with its subject and whether it is praise, a complaint or neutral. A claim is one",
            "point a reviewer makes. Part of",
            "[SteamGauge](https://github.com/Aureliolo/steamgauge).",
            "",
            "The dataset contains no review text and no author information. `fetch_text.py`",
            "fetches the labelled reviews from Steam's public web API and adds each",
            "claim's text.",
            "",
            "## Files",
            "",
            "- `claims.jsonl`: one label per claim.",
            "- `second-labels.jsonl`: a second label, from a different model, for some claims.",
            gold_file,
            "- `acceptable.jsonl`: for some gold claims, whether a subject other than the gold",
            "  one is also acceptable, judged by the person who gave it.",
            "- `reviews.jsonl`: one row per labelled review.",
            "- `fetch_text.py`: adds the text to the claims. Python standard library only.",
            "",
            "## Fields",
            "",
            "`claims.jsonl`, `second-labels.jsonl` and `gold.jsonl`:",
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
            (
                "| `ironic` | The claim means the opposite of what it says; `polarity` is what it "
                "means. |"
            ),
            (
                "| `confidence` | The labeller's confidence in the subject: `high`, `medium` or "
                "`low`. |"
            ),
            "| `ambiguous` | The category rules do not settle which subject the claim is about. |",
            "| `split_wrong` | The claim is cut in the wrong place. |",
            "| `produced_by` | The model that wrote the label, or `a person`. |",
            (
                "| `taxonomy` | The category sheet the label was written against, as a hash of "
                "the sheet. Null where it was not recorded. |"
            ),
            "| `subset` | How the claim was drawn (below). |",
            "",
            "`subset`:",
            "",
            "- `random`: a random draw of the game's reviews. Only these rows estimate how often",
            "  a subject comes up.",
            "- `declined`: claims a trained reader did not answer.",
            "- `keyword`: claims found by keyword for rarely mentioned subjects.",
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
            "`--claims gold.jsonl` or `--claims second-labels.jsonl` does the same for another",
            "label file. Reviews deleted or edited on Steam since labelling are left out.",
            "",
            "## Labels",
            "",
            "Written by Claude models from a category sheet, one game at a time, without the",
            "game's name" + (f": {by}." if by else "."),
            *twice,
            "",
            (
                "The category sheet is [`taxonomy.rs`](https://github.com/Aureliolo/steamgauge/blob/"
                "main/crates/steamgauge-core/src/taxonomy.rs) in the SteamGauge repository."
            ),
            "",
            "## Licence",
            "",
            "CC BY 4.0. Review text is not included.",
        ]
    )


def search_card(name: str, base: str) -> str:
    """Return the card for a search export: Qwen's weights in half precision."""
    encoder = name == "search-encoder"
    use = (
        [
            "- Returns `vector`: a normalised 1,024-dimension embedding per text, taken at the",
            "  last token.",
            "- Searches are written as `Instruct: {instruction}\\nQuery: {query}`; the passages",
            "  searched are embedded as they are.",
            "- `training/export_search.py` measures its cosine similarity to the original model",
            "  on DirectML, over any claims it is given.",
        ]
        if encoder
        else [
            "- Returns `score`: the probability that a passage answers a query, from the",
            '  model\'s "yes" and "no" at the last token. Only those two rows of the output',
            "  layer are kept.",
            "- Input is the reranker's own prompt: system and user turns holding `<Instruct>`,",
            "  `<Query>` and `<Document>`, as on the original model's card.",
            "- `training/export_search.py` measures how far its scores are from the original",
            "  model's on DirectML, over any claims it is given.",
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
            f"[`{base}`](https://huggingface.co/{base}) as a half-precision ONNX graph.",
            "[SteamGauge](https://github.com/Aureliolo/steamgauge) uses it to search",
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


def writer_card(base: str) -> str:
    """Return the card for the writer: Qwen's weights rounded to four bits, in one graph."""
    return "\n".join(
        [
            "---",
            "license: apache-2.0",
            f"base_model: {base}",
            "tags:",
            "- onnx",
            "- steam",
            "- game-reviews",
            "pipeline_tag: text-generation",
            "---",
            "",
            "# SteamGauge writer",
            "",
            f"[`{base}`](https://huggingface.co/{base}) as an ONNX graph with int4 weights,",
            "rounded to nearest in blocks of 32 by onnxruntime-genai's model builder.",
            "[SteamGauge](https://github.com/Aureliolo/steamgauge) uses it to write the",
            "paragraph at the top of a game's page from the points its Steam reviewers make most,",
            "and leaves out any sentence giving a number or a quotation those points do not give.",
            "",
            "## Use",
            "",
            "- `model.onnx` takes `input_ids` from `tokenizer.json`, `attention_mask`, and",
            "  `past_key_values.{layer}.key` and `.value` for each of its 36 layers, in single",
            "  precision, 8 heads of 128; it returns `logits` and `present.{layer}.key` and",
            "  `.value`, which are the next step's past. The weights are in `model.onnx.data`.",
            "- Input is the model's chat format. SteamGauge takes the likeliest token at every",
            "  step and stops at `<|im_end|>` or `<|endoftext|>`.",
            "- It is built for the processor's int4 kernels and runs on the processor. On",
            "  DirectML it runs slower and its answers are not the model's.",
            "- `training/export_writer.py` builds it and checks it against the original model:",
            "  fed the original's own paragraph for a fixed brief, it has to pick the original's",
            "  next token at three places in four or more.",
            "",
            "## Licence",
            "",
            f"Apache-2.0, as is `{base}`. The model is Qwen's work.",
            "",
        ]
    )


def pins_in(source: str, constant: str) -> dict[str, str]:
    """Return the file hashes a Rust constant pins, by remote name."""
    found = re.search(rf"const {constant}\b.*?(?=\n(?:pub )?const |\Z)", source, re.DOTALL)
    if not found:
        msg = f"no constant {constant} to read pins from"
        raise SystemExit(msg)
    return dict(
        re.findall(r'remote: "([^"]+)",\s*local: "[^"]+",\s*sha256: "([^"]*)"', found.group(0))
    )


def pin_reader(
    source: str,
    size: str,
    constant: str,
    release: tuple[str, str, str],
    files: dict[str, tuple[str, int]],
) -> str:
    """Return reader.rs with one size pinned to a repository, a commit, its tag and file hashes.

    Each file is named by its remote name and carries its hash and length.
    """
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
        msg = f"could not find where to pin the {size} reader in {READER_RS}"
        raise SystemExit(msg)
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
            msg = f"could not find where to pin {name} in {constant}"
            raise SystemExit(msg)
    return source


def pin_search(source: str, constant: str, release: tuple[str, str, str]) -> str:
    """Return search_models.rs or writer.rs with one model pinned to a commit and tag."""
    repo, revision, tag = release
    source, count = re.subn(
        rf'(pub const {constant}: \w+ = \w+ \{{\s*name: "[^"]*",\s*repository: )"[^"]*"'
        r'(,\s*revision: )"[^"]*"(,\s*release: )"[^"]*"',
        rf'\g<1>"{repo}"\g<2>"{revision}"\g<3>"{tag}"',
        source,
        count=1,
    )
    if count != 1:
        msg = f"could not find where to pin {constant}"
        raise SystemExit(msg)
    return source


def read_labels(reference: Path) -> tuple[list[Row], list[Row], list[Row], list[Row]]:
    """Return every label, every second label, the gold answers and the judgements beside them."""
    rows: list[Row] = []
    again: list[Row] = []
    gold: list[Row] = []
    acceptable: list[Row] = []

    def load(path: Path) -> list[Row]:
        return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else []

    for game in sorted(path for path in reference.iterdir() if path.is_dir()):
        for draw in [game, *(game / name for name in TEACHING_SETS)]:
            rows.extend(load(draw / "labels.json"))
        answered: set[tuple[Any, ...]] = set()
        for second in SECOND_LABELS:
            for row in load(game / second / "labels.json"):
                if (key := claim_of(row)) not in answered:
                    answered.add(key)
                    again.append(row)
        app = int(game.name)
        people = sorted(path for path in (game / "gold").glob("*") if path.is_dir())
        for person in people:
            gold.extend(load(person / "labels.json"))
            acceptable.extend({"app_id": app, **one} for one in load(person / "acceptable.json"))
    return rows, again, gold, acceptable


def checked(rows: list[Row], fields: set[str], what: str) -> None:
    """Stop the publish when any row carries a field outside `fields`."""
    unknown = sorted(set[str]().union(*(row.keys() for row in rows)) - fields)
    if unknown:
        msg = f"{what} carry {unknown}, which nothing says may be published"
        raise SystemExit(msg)


def jsonl(found: list[Row]) -> str:
    """Return `found` as JSON lines with sorted keys."""
    return "".join(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n" for row in found)


def _parse_args() -> argparse.Namespace:
    """Read the command line."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--owner", required=True, help="the Hugging Face user or organisation")
    parser.add_argument("--tag", required=True, help="the release, v1, v2, ...")
    parser.add_argument(
        "--reviews", help="what `steamgauge export-review-facts` wrote; the dataset needs it"
    )
    parser.add_argument(
        "--only",
        action="append",
        choices=REPOSITORIES,
        help="publish this repository alone, as a new one is published at v1 while the rest "
        "stay where they are; may be given more than once, and every one is published without it",
    )
    parser.add_argument("--reference", default=str(REPO / "reference" / "claims"))
    parser.add_argument(
        "--people",
        default=str(REPO / "reference" / "people.json"),
        help="each gold labeller's handle and the name the dataset card credits",
    )
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
    return parser.parse_args()


# What goes where: each repository's kind and its files as (path in the repository, path here).
Uploads = dict[str, tuple[str, list[tuple[str, Path]]]]


def _stage_readers(
    args: argparse.Namespace, wanted: set[str], staging: Path, uploads: Uploads
) -> dict[str, dict[str, tuple[str, int]]]:
    """Stage the reader repositories and return each size's file hashes and lengths."""
    reader_files: dict[str, dict[str, tuple[str, int]]] = {}

    # export imports torch and transformers, which a dry run of the search and dataset
    # repositories does not otherwise need.
    import export  # noqa: PLC0415 heavy optional dependency, loaded only when a reader is staged

    for size in sizes.load()["sizes"]:
        if not size["ships"]:
            continue
        repo, _ = READERS[size["name"]]
        if repo not in wanted:
            continue
        run = Path(args.runs) / size["run"]
        for name in MODEL_FILES:
            if not (run / name).is_file():
                msg = f"{run / name} is missing; export the run first"
                raise SystemExit(msg)
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
    return reader_files


def _check_pinned_files(
    directory: Path, files: list[str], pinned: dict[str, str], constant: str, writer: str
) -> None:
    """Refuse a file that is missing or is not the one `constant` pins."""
    for file in files:
        if not (directory / file).is_file():
            msg = f"{directory / file} is missing; {writer} writes it"
            raise SystemExit(msg)
        # Only the bytes the tool already pins go up: an export run again is not the same
        # file, and publishing one would publish something nothing was measured on.
        if sha256(directory / file) != pinned[file]:
            msg = f"{directory / file} is not the file {constant} pins"
            raise SystemExit(msg)


def _stage_search(
    args: argparse.Namespace, wanted: set[str], search_rs: str, staging: Path, uploads: Uploads
) -> None:
    """Stage the search models' repositories."""
    for name, (repo, constant, base) in SEARCH.items():
        if repo not in wanted:
            continue
        directory = Path(args.cache) / name
        pinned = pins_in(search_rs, constant)
        _check_pinned_files(directory, SEARCH_FILES, pinned, constant, "export_search.py")
        card = staging / f"{repo}.md"
        card.write_text(search_card(name, base), encoding="utf-8")
        uploads[repo] = (
            "model",
            [*((file, directory / file) for file in SEARCH_FILES), ("README.md", card)],
        )


def _stage_writer(
    args: argparse.Namespace, wanted: set[str], writer_rs: str, staging: Path, uploads: Uploads
) -> None:
    """Stage the writer's repository."""
    name, repo, constant, base = WRITER
    if repo not in wanted:
        return
    directory = Path(args.cache) / name
    pinned = pins_in(writer_rs, constant)
    _check_pinned_files(directory, WRITER_FILES, pinned, constant, "export_writer.py")
    card = staging / f"{repo}.md"
    card.write_text(writer_card(base), encoding="utf-8")
    uploads[repo] = (
        "model",
        [*((file, directory / file) for file in WRITER_FILES), ("README.md", card)],
    )


def _upload(owner: str, tag: str, uploads: Uploads) -> dict[str, str]:
    """Upload every repository as one tagged commit and return the commit of each."""
    # A dry run needs no Hugging Face client, or the token it reads.
    from huggingface_hub import CommitOperationAdd, HfApi  # noqa: PLC0415 loaded only to upload

    api = HfApi()
    commits: dict[str, str] = {}
    for repo, (kind, contents) in uploads.items():
        repo_id = f"{owner}/{repo}"
        api.create_repo(repo_id, repo_type=kind, exist_ok=True, private=False)
        made = api.create_commit(
            repo_id,
            repo_type=kind,
            operations=[
                CommitOperationAdd(path_in_repo=path, path_or_fileobj=str(local))
                for path, local in contents
            ],
            commit_message=f"Release {tag}",
        )
        api.create_tag(repo_id, repo_type=kind, tag=tag, revision=made.oid)
        commits[repo] = made.oid
        print(f"published {repo_id} at {made.oid} as {tag}")
    return commits


def _pin_sources(
    args: argparse.Namespace,
    reader_files: dict[str, dict[str, tuple[str, int]]],
    commits: dict[str, str],
    sources: tuple[str, str, str],
) -> None:
    """Pin the tool's three sources to the commits and write them."""
    reader_rs, search_rs, writer_rs = sources
    # Pinned to the commits the uploads made and the hashes taken before them from the same
    # files. If anything between here and the hub changes a byte, the pin will not match what
    # comes down and the tool refuses it, which is the failure this arrangement exists to make
    # loud.
    for size, files in reader_files.items():
        repo, constant = READERS[size]
        release = (f"{args.owner}/{repo}", commits[repo], args.tag)
        reader_rs = pin_reader(reader_rs, size, constant, release, files)
    for repo, constant, _ in SEARCH.values():
        if repo in commits:
            release = (f"{args.owner}/{repo}", commits[repo], args.tag)
            search_rs = pin_search(search_rs, constant, release)
    _, repo, constant, _ = WRITER
    if repo in commits:
        release = (f"{args.owner}/{repo}", commits[repo], args.tag)
        writer_rs = pin_search(writer_rs, constant, release)
    # The sources are kept with Unix line ends, which Windows would otherwise turn every one of.
    READER_RS.write_text(reader_rs, encoding="utf-8", newline="\n")
    SEARCH_RS.write_text(search_rs, encoding="utf-8", newline="\n")
    WRITER_RS.write_text(writer_rs, encoding="utf-8", newline="\n")
    pinned_in = ", ".join(str(path.relative_to(REPO)) for path in (READER_RS, SEARCH_RS, WRITER_RS))
    print(f"\npinned in {pinned_in}")


def main() -> None:
    """Stage every repository's files, upload them and pin the tool to the commits."""
    args = _parse_args()
    if not TAG.fullmatch(args.tag):
        msg = f"{args.tag!r} is not a release tag; they are v1, v2, ..."
        raise SystemExit(msg)
    wanted = set(args.only or REPOSITORIES)
    staging = Path(args.staging)
    staging.mkdir(parents=True, exist_ok=True)
    sources = (
        READER_RS.read_text(encoding="utf-8"),
        SEARCH_RS.read_text(encoding="utf-8"),
        WRITER_RS.read_text(encoding="utf-8"),
    )

    uploads: Uploads = {}
    reader_files = _stage_readers(args, wanted, staging, uploads)
    _stage_search(args, wanted, sources[1], staging, uploads)
    _stage_writer(args, wanted, sources[2], staging, uploads)

    if DATASET in wanted:
        if not args.reviews:
            msg = "the dataset needs --reviews"
            raise SystemExit(msg)
        uploads[DATASET] = (
            "dataset",
            stage_dataset(args.reference, args.reviews, staging, args.people),
        )

    for repo, (kind, contents) in uploads.items():
        print(
            f"{kind:<8} {args.owner}/{repo} {args.tag}: {', '.join(path for path, _ in contents)}"
        )
    if args.dry_run:
        print(f"\nstaged in {staging}; nothing uploaded and nothing pinned")
        return

    commits = _upload(args.owner, args.tag, uploads)
    _pin_sources(args, reader_files, commits, sources)


def stage_dataset(
    reference: str, reviews_file: str, staging: Path, people_file: str
) -> list[tuple[str, Path]]:
    """Write the dataset's files into `staging`, as (path in the repository, path here)."""
    # The dataset is built from the reference sets, not from the training export. The export
    # carries each claim's text because training needs it. The reference sets carry the claim's
    # offsets within its review because that is all a label needs to be joined back. Only the
    # second shape leaves this machine.
    rows, again, gold, acceptable = read_labels(Path(reference))
    if not rows:
        msg = f"no labelled sets under {reference}"
        raise SystemExit(msg)
    reviews = [
        json.loads(line)
        for line in Path(reviews_file).read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    for found, fields, what in (
        (rows, LABEL_FIELDS, "labels"),
        (again, LABEL_FIELDS, "second labels"),
        (gold, LABEL_FIELDS, "gold answers"),
        (acceptable, JUDGEMENT_FIELDS, "judgements"),
        (reviews, REVIEW_FIELDS, "review facts"),
    ):
        checked(found, fields, f"the {what}")
    # A label written before labels recorded their sheet holds either nothing or an empty
    # string. Both go up as one null so a reader of the file has a single case to handle. A
    # fingerprint guessed from a commit date would be the stamping the ingest refuses to do.
    for row in rows + again:
        row["taxonomy"] = row.get("taxonomy") or None
    for name in (*KEY, "start", "end", "subject", "polarity", "produced_by"):
        if any(name not in row for row in rows + again):
            msg = f"a row has no {name}; nothing can be joined back from it"
            raise SystemExit(msg)
    # The gold page stamps every answer with the sheet it showed, so one without is broken.
    if any(not row.get("taxonomy") for row in gold):
        msg = "a gold answer has no taxonomy; the sheet it answered is not known"
        raise SystemExit(msg)
    described = {row["review_id"] for row in reviews}
    bare = {row["review_id"] for row in rows + again + gold} - described
    if bare:
        msg = (
            f"{len(bare):,} labelled reviews have no facts in {reviews_file}; a label nobody can "
            "check against its text is not one to publish. Run export-review-facts again."
        )
        raise SystemExit(msg)
    names = json.loads(Path(people_file).read_text(encoding="utf-8"))
    # A person's answers go up under the name they asked to be credited by, so one with no
    # name written down is not published under a guess.
    unnamed = sorted({row["produced_by"] for row in gold} - names.keys())
    if unnamed:
        msg = f"no name to credit in {people_file} for {', '.join(unnamed)}"
        raise SystemExit(msg)
    files = {
        "claims.jsonl": rows,
        "second-labels.jsonl": again,
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
            labelled_by=Credits(
                gold=dict(Counter(row["produced_by"] for row in gold).most_common()),
                labellers=dict(Counter(row["produced_by"] for row in rows).most_common()),
                names=names,
            ),
        ),
        encoding="utf-8",
    )
    return [
        *((name, staging / name) for name in files),
        ("fetch_text.py", HERE / "fetch_text.py"),
        ("README.md", card),
    ]


if __name__ == "__main__":
    main()
