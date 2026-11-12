"""Validation holds no gradients: with them on, the first validation pass allocates over 30 GB
and the run dies.

python -m pytest test_evaluate.py
"""

from __future__ import annotations

from pathlib import Path
from typing import TYPE_CHECKING

import numpy as np
import pytest

from confidence import aurc
from train import area_under_risk_coverage, macro_f1

if TYPE_CHECKING:
    from collections.abc import Callable

    from confidence import FloatArray


@pytest.mark.parametrize("area", [area_under_risk_coverage, aurc])
def test_claims_the_model_is_equally_sure_of_score_the_same_in_any_order(
    area: Callable[[FloatArray, FloatArray], float],
) -> None:
    confidence = np.full(4, 0.5)
    right_first = area(confidence, np.array([1.0, 1.0, 0.0, 0.0]))
    wrong_first = area(confidence, np.array([0.0, 0.0, 1.0, 1.0]))
    assert right_first == pytest.approx(wrong_first)


def test_without_ties_the_area_is_the_one_the_running_mean_gives() -> None:
    generator = np.random.default_rng(7)
    confidence = generator.permutation(60).astype(float) / 60 + 0.001
    correct = (generator.random(60) < 0.6).astype(float)
    assert area_under_risk_coverage(confidence, correct) == 0.40429215726996426
    assert aurc(confidence, correct) == 0.40429215726996426


@pytest.mark.parametrize("area", [area_under_risk_coverage, aurc])
@pytest.mark.parametrize("tied", [[1.0, 0.0, 1.0], [0.0, 1.0, 1.0], [1.0, 1.0, 0.0]])
def test_a_block_of_tied_claims_weighs_the_coverage_it_adds(
    area: Callable[[FloatArray, FloatArray], float], tied: list[float]
) -> None:
    # Blocks of 1 and 3 claims. Risk after the first block is 0, after all four 1/4.
    # Area: 1/4 * 0 + 3/4 * 1/4 = 0.1875, whatever order the three tied claims come in.
    confidence = np.array([0.9, 0.5, 0.5, 0.5])
    assert area(confidence, np.array([1.0, *tied])) == pytest.approx(0.1875)


def test_macro_f1_weighs_each_subject_present_alike() -> None:
    # a: 2 of 3 said right, 2 of 3 found. b: 1 of 2 and 1 of 2. c: never found. d: never true,
    # so it is left out rather than counted as a zero.
    truth = np.array([0, 0, 0, 1, 1, 2])
    predicted = np.array([0, 0, 1, 1, 2, 0])
    macro, per_class = macro_f1(truth, predicted, ["a", "b", "c", "d"])
    assert macro == pytest.approx((2 / 3 + 1 / 2 + 0) / 3)
    assert list(per_class) == ["a", "b", "c"]
    assert per_class["a"] == pytest.approx(
        {"precision": 2 / 3, "recall": 2 / 3, "f1": 2 / 3, "support": 3}
    )
    assert per_class["c"]["f1"] == 0.0
    assert macro_f1(np.array([], dtype=int), np.array([], dtype=int), ["a"]) == (0.0, {})


def test_the_two_area_functions_agree_with_ties() -> None:
    generator = np.random.default_rng(3)
    confidence = generator.integers(0, 5, 80).astype(float)
    correct = (generator.random(80) < 0.5).astype(float)
    assert aurc(confidence, correct) == area_under_risk_coverage(confidence, correct)


def test_evaluate_runs_under_no_grad() -> None:
    source = (Path(__file__).parent / "train.py").read_text(encoding="utf-8")
    assert "@torch.no_grad()\ndef evaluate(" in source
