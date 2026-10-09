import json

from sweep import SIZES, index, shipped_runs


def test_the_shipped_runs_are_the_sizes_that_ship(tmp_path):
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


def test_every_committed_shipping_run_is_on_the_index():
    shipped = shipped_runs(SIZES)
    assert shipped
    runs = SIZES.parent.parent / "training" / "runs"
    for run in shipped:
        assert (runs / run / "run.json").is_file(), run


def test_the_index_marks_each_shipping_run_with_its_size(tmp_path):
    found = [
        {"id": "a", "config": {}, "validation": {}},
        {"id": "b", "config": {}, "validation": {}},
    ]
    page = tmp_path / "README.md"
    index(found, {"a": "small"}, page)
    rows = [line for line in page.read_text(encoding="utf-8").splitlines() if line.startswith("| ")]
    assert any(row.startswith("| **a** (ships as small) |") for row in rows)
    assert any(row.startswith("| b |") for row in rows)
