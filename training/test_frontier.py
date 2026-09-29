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


def test_readers_are_compared_answering_the_same_share_most_confident_first(tmp_path):
    # Ten claims, the guesses right on the eight most confident and wrong on the last two. The
    # reader's own line declines everything below 0.5, which is not what the shares look at.
    key = tmp_path / "key.json"
    key.write_text(
        json.dumps([{"id": str(n), "subject": "vr", "polarity": "praise"} for n in range(10)]),
        encoding="utf-8",
    )
    answers = tmp_path / "answers.json"
    answers.write_text(
        json.dumps(
            [
                {
                    "id": str(n),
                    "subject": "vr" if n < 5 else "unsure",
                    "polarity": "praise",
                    "guess": "vr" if n < 8 else "gameplay",
                    "confidence": 1.0 - n / 10,
                }
                for n in range(10)
            ]
        ),
        encoding="utf-8",
    )

    found = frontier.score(answers, key)

    assert found["coverage"] == 0.5
    assert found["accuracy_at_coverage"]["80%"]["claims"] == 8
    assert found["accuracy_at_coverage"]["80%"]["accuracy"] == 1.0
    assert found["accuracy_at_coverage"]["90%"]["claims"] == 9
    assert found["accuracy_at_coverage"]["90%"]["accuracy"] == 8 / 9


def test_answers_without_confidences_have_no_shares(tmp_path):
    # A frontier model's answers carry no confidence, and a share ranked by nothing is invented.
    key = tmp_path / "key.json"
    key.write_text(json.dumps([{"id": "a", "subject": "vr", "polarity": "praise"}]), "utf-8")
    answers = tmp_path / "answers.json"
    answers.write_text(json.dumps([{"id": "a", "subject": "vr", "polarity": "praise"}]), "utf-8")

    assert "accuracy_at_coverage" not in frontier.score(answers, key)


def test_every_row_of_the_benchmark_counts_the_claims_a_reader_can_be_handed(tmp_path):
    # A frontier model answered every key row when it was asked; a reader is handed only the
    # rows the export still holds. Scored over different rows, the two figures are not a
    # comparison, so the key is cut to the survivors for both.
    key = tmp_path / "key.json"
    key.write_text(
        json.dumps(
            [
                {"id": "a", "app_id": 1, "review_id": "r", "claim_index": 0, "subject": "vr"},
                {"id": "gone", "app_id": 1, "review_id": "r", "claim_index": 7, "subject": "vr"},
            ]
        ),
        encoding="utf-8",
    )
    data = tmp_path / "claims.jsonl"
    data.write_text(
        json.dumps(
            {
                "text": "It runs well in the headset.",
                "subject": "vr",
                "app_id": 1,
                "review_id": "r",
                "claim_index": 0,
            }
        )
        + "\n",
        encoding="utf-8",
    )

    rows, claims = frontier.readable(key, str(data))
    assert [row["id"] for row in rows] == ["a"]
    assert [claim.text for claim in claims] == ["It runs well in the headset."]
