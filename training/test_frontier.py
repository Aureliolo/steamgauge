"""The benchmark row has to score the reader the way the reader actually answers."""

from __future__ import annotations

import frontier

SUBJECTS = ["performance", "vr", "gameplay"]


def test_one_line_when_the_export_carries_none():
    reader = {"subjects": SUBJECTS, "threshold": 0.69}
    assert frontier.abstention_line(reader, 0) == 0.69
    assert frontier.abstention_line(reader, 1) == 0.69


def test_a_line_per_subject():
    reader = {"subjects": SUBJECTS, "threshold": 0.69, "thresholds": [0.23, 0.98, 0.77]}
    assert frontier.abstention_line(reader, 0) == 0.23
    assert frontier.abstention_line(reader, 1) == 0.98
    assert frontier.abstention_line(reader, 2) == 0.77


def test_a_null_line_is_declined_outright():
    reader = {"subjects": SUBJECTS, "threshold": 0.69, "thresholds": [0.23, None, 0.77]}
    assert frontier.abstention_line(reader, 1) == float("inf")
