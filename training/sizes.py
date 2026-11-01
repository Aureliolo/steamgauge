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

REPO = Path(__file__).resolve().parent.parent
SIZES = REPO / "reference" / "reader-sizes.json"
PAGE = REPO / "docs" / "accuracy.md"
OPENS = "<!-- reader sizes: rendered from reference/reader-sizes.json by training/sizes.py -->"
CLOSES = "<!-- end of reader sizes -->"
# The page's prose width, which its Markdown lint holds every line but a table row to.
WIDTH = 100


def _parameters(count: int) -> str:
    return f"{count / 1e9:.1f}B" if count >= 1e9 else f"{count / 1e6:.0f}M"


def _download(mib: int) -> str:
    # In the decimal units a download is shown in everywhere else. A card's memory is sold in
    # binary ones, and `_memory` keeps those.
    megabytes = mib * 1.048576
    return f"{megabytes / 1000:.1f} GB" if megabytes >= 1000 else f"{megabytes:.0f} MB"


def _share(value: float | None) -> str:
    return "pending" if value is None else f"{value:.1%}"


def _seconds(value: float | None) -> str:
    if value is None:
        return "pending"
    return f"{value / 60:.0f} min" if value >= 600 else f"{value:.0f} s"


def _memory(mib: int | None) -> str:
    return "pending" if mib is None else f"{mib / 1024:.1f} GB"


def table(sizes: dict) -> str:
    """The sizes that ship as a Markdown table, with what each column was measured on beneath it.

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


def load(path: Path = SIZES) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def in_page(text: str, rendered: str) -> str:
    """`text` with the block between the markers replaced by `rendered`."""
    start, end = text.index(OPENS), text.index(CLOSES)
    return text[: start + len(OPENS)] + "\n" + rendered + "\n" + text[end:]


def main():
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
