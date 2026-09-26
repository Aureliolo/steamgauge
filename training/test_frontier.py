"""The benchmark row has to score the reader the way the reader actually answers."""

from __future__ import annotations

import json

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


def test_a_claim_the_reader_was_never_handed_is_not_charged_against_it(tmp_path):
    # A key drawn before the splitter last changed names spans this build cuts no claim at, and
    # the reader is never asked those. Counted as unanswered, they read as the reader declining.
    key = tmp_path / "key.json"
    key.write_text(
        json.dumps(
            [
                {"id": "a", "subject": "vr", "polarity": "praise"},
                {"id": "b", "subject": "gameplay", "polarity": "complaint"},
                {"id": "gone", "subject": "performance", "polarity": "praise"},
            ]
        ),
        encoding="utf-8",
    )
    answers = tmp_path / "answers.json"
    answers.write_text(
        json.dumps(
            [
                {"id": "a", "subject": "vr", "polarity": "praise"},
                {"id": "b", "subject": "unsure", "polarity": "complaint"},
            ]
        ),
        encoding="utf-8",
    )

    over_all = frontier.score(answers, key)
    assert over_all["claims"] == 3
    assert over_all["unanswered_rows"] == 1

    readable = frontier.score(answers, key, over={"a", "b"})
    assert readable["claims"] == 2
    assert readable["unanswered_rows"] == 0
    assert readable["coverage"] == 0.5, "declined one of the two it was handed"
    assert readable["accuracy_where_answered"] == 1.0
