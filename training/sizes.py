"""The reader in each size, measured one way, as one table for the README and every model card.

The figures live in `reference/reader-sizes.json` and nowhere else. The README's table and the
table on each size's model card are both rendered from that file, so the page somebody reads on
the Hugging Face hub and the one in the repository cannot say different things.

    python sizes.py            prints the table
    python sizes.py --readme   writes it into the README between its markers
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SIZES = REPO / "reference" / "reader-sizes.json"
README = REPO / "README.md"
OPENS = "<!-- reader sizes: rendered from reference/reader-sizes.json by training/sizes.py -->"
CLOSES = "<!-- end of reader sizes -->"


def _parameters(count: int) -> str:
    return f"{count / 1e9:.1f}B" if count >= 1e9 else f"{count / 1e6:.0f}M"


def _download(mib: int) -> str:
    # In the decimal units a download is shown in everywhere else; a card's memory is sold in
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
    """The sizes as a Markdown table, with what each column was measured on beneath it."""
    measured = sizes["measured"]
    rows = [
        "| Reader | Parameters | Download | Right, answering its surest 80% | Right, answering "
        "its surest 90% | Card memory | One big game on a card | One small game on the "
        "processor |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for size in sizes["sizes"]:
        name = size["name"] if size["ships"] else f"*{size['name']}* (not shipped)"
        rows.append(
            f"| {name} | {_parameters(size['parameters'])} | {_download(size['download_mib'])} "
            f"| {_share(size['right_at_80'])} | {_share(size['right_at_90'])} "
            f"| {_memory(size['card_mib'])} | {_seconds(size['card_seconds'])} "
            f"| {_seconds(size['processor_seconds'])} |"
        )
    notes = [
        "",
        f"- **Right, answering its surest share:** {measured['frontier']}",
        f"- **Card memory and one big game on a card:** {measured['card']}",
        f"- **One small game on the processor:** {measured['processor']}",
    ]
    return "\n".join(rows + notes)


def load(path: Path = SIZES) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def in_readme(text: str, rendered: str) -> str:
    """`text` with the block between the markers replaced by `rendered`."""
    start, end = text.index(OPENS), text.index(CLOSES)
    return text[: start + len(OPENS)] + "\n" + rendered + "\n" + text[end:]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--readme", action="store_true", help="write the table into README.md")
    args = parser.parse_args()
    rendered = table(load())
    if args.readme:
        README.write_text(in_readme(README.read_text(encoding="utf-8"), rendered), encoding="utf-8")
    else:
        print(rendered)


if __name__ == "__main__":
    main()
