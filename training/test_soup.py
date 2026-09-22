"""What averaging a set of saved weights does, and what it refuses to do.

python -m pytest training/test_soup.py
"""

from __future__ import annotations

import json

import pytest
import torch

from soup import Soup, ingredients, must_agree

RECORD = {
    "backbone": "intfloat/multilingual-e5-large-instruct",
    "subjects": ["verdict", "gameplay"],
    "max_length": 128,
    "context": True,
    "prefix": True,
    "mark": True,
    "pooling": "mean",
    "split_seed": 1,
    "fold": None,
    "folds": None,
    "data_fingerprint": "abc123",
    "seed": 1,
}


def run_dir(tmp_path, name, **changed):
    out = tmp_path / name
    out.mkdir()
    (out / "run.json").write_text(json.dumps({**RECORD, **changed}), encoding="utf-8")
    (out / "model.bin").write_bytes(b"")
    return out


def test_the_mean_of_two_sets_of_weights_is_halfway_between_them():
    soup = Soup()
    soup.add({"subject.weight": torch.tensor([0.0, 2.0])})
    soup.add({"subject.weight": torch.tensor([1.0, 4.0])})
    assert soup.state()["subject.weight"].tolist() == [0.5, 3.0]


def test_a_trial_leaves_the_soup_where_it_was():
    soup = Soup()
    soup.add({"subject.weight": torch.tensor([0.0])})
    assert soup.added({"subject.weight": torch.tensor([2.0])})["subject.weight"].tolist() == [1.0]
    assert soup.state()["subject.weight"].tolist() == [0.0]
    assert soup.count == 1


def test_the_first_ingredient_is_itself():
    soup = Soup()
    weights = {"subject.weight": torch.tensor([3.0])}
    assert soup.added(weights)["subject.weight"].tolist() == [3.0]


def test_what_is_not_a_weight_is_carried_across_rather_than_averaged():
    soup = Soup()
    soup.add({"trunk.position_ids": torch.tensor([0, 1, 2])})
    soup.add({"trunk.position_ids": torch.tensor([0, 1, 2])})
    assert soup.state()["trunk.position_ids"].tolist() == [0, 1, 2]


def test_ingredients_that_disagree_on_something_that_is_not_a_weight_are_refused():
    soup = Soup()
    soup.add({"trunk.position_ids": torch.tensor([0, 1, 2])})
    with pytest.raises(SystemExit):
        soup.add({"trunk.position_ids": torch.tensor([0, 1, 3])})


def test_an_ingredient_holding_different_weights_is_refused():
    soup = Soup()
    soup.add({"subject.weight": torch.tensor([1.0])})
    with pytest.raises(SystemExit):
        soup.add({"polarity.weight": torch.tensor([1.0])})


def test_a_run_saved_without_its_weights_cannot_be_souped(tmp_path):
    out = run_dir(tmp_path, "unsaved")
    (out / "model.bin").unlink()
    with pytest.raises(SystemExit):
        ingredients([str(out), str(run_dir(tmp_path, "saved"))])


def test_one_run_is_not_a_soup(tmp_path):
    with pytest.raises(SystemExit):
        ingredients([str(run_dir(tmp_path, "alone"))])


def test_runs_of_one_configuration_that_differ_only_by_seed_agree(tmp_path):
    must_agree(
        [
            (path, json.loads((path / "run.json").read_text(encoding="utf-8")))
            for path in (run_dir(tmp_path, "s1", seed=1), run_dir(tmp_path, "s2", seed=2))
        ]
    )


@pytest.mark.parametrize(
    "differing",
    [
        {"backbone": "xlm-roberta-base"},
        {"subjects": ["gameplay", "verdict"]},
        {"max_length": 256},
        {"context": False},
        {"split_seed": 2},
        {"data_fingerprint": "def456"},
    ],
)
def test_runs_of_different_configurations_are_refused(tmp_path, differing):
    found = [
        (path, json.loads((path / "run.json").read_text(encoding="utf-8")))
        for path in (run_dir(tmp_path, "one"), run_dir(tmp_path, "two", **differing))
    ]
    with pytest.raises(SystemExit):
        must_agree(found)
