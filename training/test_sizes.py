"""The sizes table is one table: the docs' is the one the data file renders."""

from __future__ import annotations

import sizes


def test_the_page_carries_the_table_the_data_renders():
    # Edited by hand, the page's copy drifts from every model card, which render the data.
    page = sizes.PAGE.read_text(encoding="utf-8")
    start = page.index(sizes.OPENS) + len(sizes.OPENS)
    block = page[start : page.index(sizes.CLOSES)].strip()

    assert block == sizes.table(sizes.load()), "run `python sizes.py --write`"


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


LARGE = {
    "name": "large",
    "ships": True,
    "parameters": 4_022_000_000,
    "download_mib": 8403,
    "right_at_80": 0.8392,
    "right_at_90": None,
    "card_mib": 12851,
    "card_seconds": 1399,
    "processor_seconds": 36.0,
}


def test_figures_read_the_way_a_person_reads_them():
    data = {"measured": {"frontier": "f", "card": "c", "processor": "p"}, "sizes": [LARGE]}

    row = sizes.table(data).splitlines()[2]

    assert row == "| large | 4.0B | 8.8 GB | 83.9% | pending | 12.5 GB | 23 min | 36 s |"


def test_a_size_nobody_can_download_is_left_off_the_table():
    data = {
        "measured": {"frontier": "f", "card": "c", "processor": "p"},
        "sizes": [{**LARGE, "name": "unpublished", "ships": False}, LARGE],
    }

    rendered = sizes.table(data)

    assert "unpublished" not in rendered
    assert rendered.splitlines()[2].startswith("| large |")


def test_what_each_column_was_measured_on_keeps_to_the_prose_width():
    said = "word " * 60
    data = {
        "measured": {"frontier": said, "card": said, "processor": said},
        "sizes": [],
    }

    notes = sizes.table(data).splitlines()[3:]

    assert len(notes) > 3
    assert all(len(line) <= sizes.WIDTH for line in notes)
    assert all(line.startswith(("- **", "  ")) for line in notes)


def test_the_page_block_is_replaced_and_nothing_around_it():
    text = f"before\n{sizes.OPENS}\nold\n{sizes.CLOSES}\nafter\n"

    assert sizes.in_page(text, "new") == f"before\n{sizes.OPENS}\nnew\n{sizes.CLOSES}\nafter\n"
