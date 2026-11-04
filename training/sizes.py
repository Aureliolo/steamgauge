"""The reader in each size, measured one way, as one table for the docs and every model card.

The figures live in `reference/reader-sizes.json` and nowhere else. The table in
`docs/accuracy.md` and the table on each size's model card are both rendered from that file, so
the page somebody reads on the Hugging Face hub and the one in the repository cannot say
different things.

    python sizes.py            prints the table
    python sizes.py --write    writes it into docs/accuracy.md between its markers
"""

from __future__ import annotations

import argparse
import json
import textwrap
from pathlib import Path
from typing import TypedDict, cast

REPO = Path(__file__).resolve().parent.parent
SIZES = REPO / "reference" / "reader-sizes.json"
PAGE = REPO / "docs" / "accuracy.md"
OPENS = "<!-- reader sizes: rendered from reference/reader-sizes.json by training/sizes.py -->"
CLOSES = "<!-- end of reader sizes -->"
# The page's prose width, which its Markdown lint holds every line but a table row to.
WIDTH = 100
BILLION = 1e9
MILLION = 1e6
THOUSAND = 1000
MIB_IN_MB = 1.048576
KIB_IN_MIB = 1024
SECONDS_PER_MINUTE = 60
# From ten minutes up a duration is shown in minutes.
MINUTES_FROM = 600


class Size(TypedDict):
    """One reader size's measured figures, as `reader-sizes.json` holds them."""

    name: str
    ships: bool
    run: str
    backbone: str
    parameters: int
    download_mib: int
    right_at_80: float | None
    right_at_90: float | None
    card_mib: int | None
    card_seconds: float | None
    processor_seconds: float | None


class Measured(TypedDict):
    """What each column of the table was measured on."""

    frontier: str
    card: str
    processor: str


class Sizes(TypedDict):
    """The contents of `reader-sizes.json`."""

    measured: Measured
    sizes: list[Size]


def _parameters(count: int) -> str:
    return f"{count / BILLION:.1f}B" if count >= BILLION else f"{count / MILLION:.0f}M"


def _download(mib: int) -> str:
    # In the decimal units a download is shown in everywhere else. A card's memory is sold in
    # binary ones, and `_memory` keeps those.
    megabytes = mib * MIB_IN_MB
    return f"{megabytes / THOUSAND:.1f} GB" if megabytes >= THOUSAND else f"{megabytes:.0f} MB"


def _share(value: float | None) -> str:
    return "pending" if value is None else f"{value:.1%}"


def _seconds(value: float | None) -> str:
    if value is None:
        return "pending"
    if value >= MINUTES_FROM:
        return f"{value / SECONDS_PER_MINUTE:.0f} min"
    return f"{value:.0f} s"


def _memory(mib: int | None) -> str:
    return "pending" if mib is None else f"{mib / KIB_IN_MIB:.1f} GB"


def table(sizes: Sizes) -> str:
    """Return the shipped sizes as a Markdown table, with what each column was measured on.

    The notes on what was measured sit beneath the table.

    A size that does not ship stays in the data file for the record but is left off the table:
    a figure for a model nobody can download is a figure nobody can check.
    """
    measured = sizes["measured"]
    rows = [
        (
            "| Reader | Parameters | Download | Accuracy at 80% coverage | Accuracy at 90% "
            "coverage | GPU memory | One large game on the GPU | One small game on the CPU |"
        ),
        "|---|---|---|---|---|---|---|---|",
    ]
    for size in sizes["sizes"]:
        if not size["ships"]:
            continue
        rows.append(
            f"| {size['name']} | {_parameters(size['parameters'])} "
            f"| {_download(size['download_mib'])} "
            f"| {_share(size['right_at_80'])} | {_share(size['right_at_90'])} "
            f"| {_memory(size['card_mib'])} | {_seconds(size['card_seconds'])} "
            f"| {_seconds(size['processor_seconds'])} |"
        )
    notes = [
        f"- **Accuracy at a coverage:** {measured['frontier']}",
        f"- **GPU memory and one large game on the GPU:** {measured['card']}",
        f"- **One small game on the CPU:** {measured['processor']}",
    ]
    wrapped = [
        textwrap.fill(
            note,
            width=WIDTH,
            subsequent_indent="  ",
            break_long_words=False,
            break_on_hyphens=False,
        )
        for note in notes
    ]
    return "\n".join([*rows, "", *wrapped])


def load(path: Path = SIZES) -> Sizes:
    """Return the measured sizes from the data file."""
    return cast("Sizes", json.loads(path.read_text(encoding="utf-8")))


def in_page(text: str, rendered: str) -> str:
    """Return `text` with the block between the markers replaced by `rendered`."""
    start, end = text.index(OPENS), text.index(CLOSES)
    return text[: start + len(OPENS)] + "\n" + rendered + "\n" + text[end:]


def main() -> None:
    """Print the table, or write it into the accuracy page."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write", action="store_true", help="write the table into docs/accuracy.md"
    )
    args = parser.parse_args()
    rendered = table(load())
    if args.write:
        PAGE.write_text(
            in_page(PAGE.read_text(encoding="utf-8"), rendered), encoding="utf-8", newline="\n"
        )
    else:
        print(rendered)


if __name__ == "__main__":
    main()
