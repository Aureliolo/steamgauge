"""The held-out score, from the model's logits to the share it answers and how often it is right.

python -m pytest test_held_out.py
"""

from __future__ import annotations

import math
from typing import TYPE_CHECKING, cast

import numpy as np
import pytest
import torch

from train import confidence_of, expected_calibration_error, risk_coverage, selective

if TYPE_CHECKING:
    from collections.abc import Iterator

    from train import ClaimReader, Item

# Between the steps of the sweep, so no step lands on a claim's confidence.
CONFIDENCE = np.array([0.955, 0.905, 0.605, 0.305])
CORRECT = np.array([1.0, 1.0, 0.0, 1.0])


def test_the_threshold_is_the_one_answering_most_at_the_promised_accuracy() -> None:
    curve, chosen = risk_coverage(CONFIDENCE, CORRECT, 0.75)
    assert chosen == {"threshold": 0.05, "coverage": 1.0, "accuracy": 0.75, "met": True}
    at = {point["threshold"]: point for point in curve}
    assert at[0.31] == {"threshold": 0.31, "coverage": 0.75, "accuracy": pytest.approx(2 / 3)}
    assert at[0.95] == {"threshold": 0.95, "coverage": 0.25, "accuracy": 1.0}
    assert 0.96 not in at, "a step nothing clears is not on the curve"


def test_a_promise_no_threshold_keeps_is_recorded_as_not_met() -> None:
    _, chosen = risk_coverage(CONFIDENCE, CORRECT, 1.01)
    assert chosen == {"threshold": 0.61, "coverage": 0.5, "accuracy": 1.0, "met": False}
    _, nothing = risk_coverage(np.array([0.01, 0.02]), np.array([1.0, 1.0]), 0.75)
    assert nothing == {"threshold": 1.0, "coverage": 0.0, "accuracy": None, "met": False}


def test_a_threshold_answers_what_clears_it_and_is_scored_on_those() -> None:
    answers = (np.array([0.9, 0.8, 0.4]), np.array([0, 1, 2]))
    assert selective(answers, [0, 2, 2], 0.5) == {
        "coverage": pytest.approx(2 / 3),
        "accuracy": 0.5,
        "answered": 2,
    }
    assert selective(answers, [0, 2, 2], 0.8) == {
        "coverage": pytest.approx(2 / 3),
        "accuracy": 0.5,
        "answered": 2,
    }, "a claim exactly on the threshold is answered"
    assert selective(answers, [0, 2, 2], 0.95) == {
        "coverage": 0.0,
        "accuracy": None,
        "answered": 0,
    }


def test_calibration_error_weighs_each_bin_by_its_claims() -> None:
    # Bin (0, 0.5]: one claim at 0.1, never right, off by 0.1. Bin (0.5, 1]: two at 0.9, right
    # half the time, off by 0.4. Weighted: 1/3 * 0.1 + 2/3 * 0.4 = 0.3.
    error = expected_calibration_error(np.array([0.9, 0.9, 0.1]), np.array([1.0, 0.0, 0.0]), 2)
    assert error == pytest.approx(0.3)


class GivenLogits:
    """A model that answers each batch with the next of the logits it holds."""

    def __init__(self, batches: list[list[list[float]]]) -> None:
        """Hold the logits, one list per batch."""
        self.batches: Iterator[list[list[float]]] = iter(batches)
        self.evaluating = False

    def eval(self) -> None:
        """Note that the model was put in evaluation mode."""
        self.evaluating = True

    def __call__(
        self, input_ids: torch.Tensor, attention_mask: torch.Tensor
    ) -> tuple[torch.Tensor, ...]:
        """Return the next batch's subject logits."""
        del attention_mask
        logits = torch.tensor(next(self.batches))
        assert len(logits) == len(input_ids)
        return (logits,)


def test_confidence_is_the_top_probability_in_the_loaders_order() -> None:
    three = math.log(3)
    model = GivenLogits([[[0.0, three], [three, 0.0]], [[0.0, 0.0]]])
    batch = {"input_ids": torch.zeros(2, 4), "attention_mask": torch.ones(2, 4)}
    loader: list[Item] = [batch, {key: value[:1] for key, value in batch.items()}]
    confidence, predicted = confidence_of(cast("ClaimReader", model), loader, "cpu")
    assert model.evaluating
    assert confidence.tolist() == pytest.approx([0.75, 0.75, 0.5])
    assert predicted.tolist() == [1, 0, 0]
