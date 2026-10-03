"""The sizes table is one table: the README's is the one the data file renders."""

from __future__ import annotations

import sizes


def test_the_readme_carries_the_table_the_data_renders():
    # Edited by hand, the README's copy drifts from every model card, which render the data.
    readme = sizes.README.read_text(encoding="utf-8")
    start = readme.index(sizes.OPENS) + len(sizes.OPENS)
    block = readme[start : readme.index(sizes.CLOSES)].strip()

    assert block == sizes.table(sizes.load()), "run `python sizes.py --readme`"


def test_every_size_says_every_figure_the_table_has_a_column_for():
    wanted = {
        "name",
        "ships",
        "run",
        "backbone",
        "parameters",
        "download_mib",
        "right_at_80",
        "right_at_90",
        "card_mib",
        "card_seconds",
        "processor_seconds",
    }
    for size in sizes.load()["sizes"]:
        assert wanted <= size.keys(), size.get("name")


def test_figures_read_the_way_a_person_reads_them():
    data = {
        "measured": {"frontier": "f", "card": "c", "processor": "p"},
        "sizes": [
            {
                "name": "large",
                "ships": False,
                "parameters": 4_022_000_000,
                "download_mib": 8403,
                "right_at_80": 0.8392,
                "right_at_90": None,
                "card_mib": 12851,
                "card_seconds": 1399,
                "processor_seconds": 36.0,
            }
        ],
    }

    row = sizes.table(data).splitlines()[2]

    assert row == (
        "| *large* (not published) | 4.0B | 8.8 GB | 83.9% | pending | 12.5 GB | 23 min | 36 s |"
    )


def test_the_readme_block_is_replaced_and_nothing_around_it():
    text = f"before\n{sizes.OPENS}\nold\n{sizes.CLOSES}\nafter\n"

    assert sizes.in_readme(text, "new") == f"before\n{sizes.OPENS}\nnew\n{sizes.CLOSES}\nafter\n"
