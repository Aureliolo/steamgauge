"""A run given part of the card rests in proportion to its work, and learns from the same batches.

python -m pytest test_card_share.py
"""

from __future__ import annotations

import time

import pytest
import torch
from torch.utils.data import DataLoader

from train import Paced, rest_after


def test_half_the_card_rests_as_long_as_it_worked():
    assert rest_after(2.0, 0.5) == pytest.approx(2.0)


def test_a_quarter_of_the_card_rests_three_times_as_long():
    assert rest_after(1.0, 0.25) == pytest.approx(3.0)


def test_the_whole_card_never_rests():
    assert rest_after(5.0, 1.0) == 0.0


def test_pacing_changes_no_batch():
    rows = torch.arange(10)
    whole = [batch.tolist() for batch in Paced(DataLoader(rows, batch_size=3), 1.0, "cpu")]
    shared = [batch.tolist() for batch in Paced(DataLoader(rows, batch_size=3), 0.5, "cpu")]
    assert whole == shared == [[0, 1, 2], [3, 4, 5], [6, 7, 8], [9]]


def test_the_rest_follows_the_work_between_batches():
    paced = Paced(DataLoader(torch.arange(2), batch_size=1), 0.5, "cpu")
    started = time.perf_counter()
    for _ in paced:
        time.sleep(0.05)
    assert time.perf_counter() - started >= 0.2


def test_the_wrapper_answers_as_its_loader_does():
    rows = torch.arange(7)
    paced = Paced(DataLoader(rows, batch_size=2), 0.5, "cpu")
    assert len(paced) == 4
    assert paced.dataset is rows
