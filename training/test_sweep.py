from __future__ import annotations

import json
from typing import TYPE_CHECKING

from sweep import SIZES, Run, differences, index, shipped_runs, shown

if TYPE_CHECKING:
    from pathlib import Path


def test_the_shipped_runs_are_the_sizes_that_ship(tmp_path: Path) -> None:
    sizes = tmp_path / "reader-sizes.json"
    sizes.write_text(
        json.dumps(
            {
                "sizes": [
                    {"name": "small", "ships": True, "run": "a"},
                    {"name": "4B", "ships": False, "run": "b"},
                ]
            }
        ),
        encoding="utf-8",
    )
    assert shipped_runs(sizes) == {"a": "small"}


def test_every_committed_shipping_run_is_on_the_index() -> None:
    shipped = shipped_runs(SIZES)
    assert shipped
    runs = SIZES.parent.parent / "training" / "runs"
    for run in shipped:
        assert (runs / run / "run.json").is_file(), run


def test_the_index_marks_each_shipping_run_with_its_size(tmp_path: Path) -> None:
    found: list[Run] = [
        {"id": "a", "config": {}, "validation": {}},
        {"id": "b", "config": {}, "validation": {}},
    ]
    page = tmp_path / "README.md"
    index(found, {"a": "small"}, page)
    rows = [line for line in page.read_text(encoding="utf-8").splitlines() if line.startswith("| ")]
    assert any(row.startswith("| **a** (ships as small) |") for row in rows)
    assert any(row.startswith("| b |") for row in rows)


def test_a_setting_printed_for_a_person_has_a_comma() -> None:
    assert shown("max_length", 1200, commas=True) == "max length 1,200"
    assert shown("learning_rate", 3e-4, commas=True) == "learning rate 3e-4"
    assert shown("mark", value=True, commas=True) == "mark True"
    assert shown("balance", 1500.0, commas=True) == "balance 1500.0"
    long: Run = {"id": "long", "max_length": 1200}
    short: Run = {"id": "short", "max_length": 256}
    assert differences(long, short) == "max length 1,200"


def test_the_index_keeps_a_setting_as_it_stands(tmp_path: Path) -> None:
    found: list[Run] = [
        {"id": "a", "max_length": 256},
        {"id": "b", "max_length": 256},
        {"id": "long", "max_length": 1024},
    ]
    page = tmp_path / "README.md"
    index(found, {}, page)
    rows = [line for line in page.read_text(encoding="utf-8").splitlines() if line.startswith("| ")]
    row = next(row for row in rows if row.startswith("| long |"))
    assert "max length 1024" in row
