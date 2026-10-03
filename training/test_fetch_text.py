"""The published labels get their text back only where it is the text they were written against.

python -m pytest test_fetch_text.py
"""

from __future__ import annotations

import hashlib

from fetch_text import BACKOFF_CAP, DAY, PATIENCE, fetch, joined, next_wait, windows


def fingerprint(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def test_windows_cover_each_review_a_day_either_side_and_merge_overlaps():
    assert windows([10 * DAY, 10 * DAY + 5, 50 * DAY]) == [
        (9 * DAY, 11 * DAY + 5),
        (49 * DAY, 51 * DAY),
    ]


def test_fetch_walks_each_window_with_the_census_parameters_and_keeps_only_what_was_asked():
    asked = []

    def page(app, query):
        asked.append((app, dict(query)))
        if query["cursor"] == "*":
            return {
                "reviews": [
                    {"recommendationid": "1", "review": "one"},
                    {"recommendationid": "9", "review": "not labelled"},
                ],
                "cursor": "next",
            }
        return {"reviews": [{"recommendationid": "2", "review": "two"}], "cursor": "next"}

    texts = fetch(7, {"1": 10 * DAY, "2": 10 * DAY}, page)

    assert texts == {"1": "one", "2": "two"}
    first = asked[0][1]
    assert (first["purchase_type"], first["filter_offtopic_activity"]) == ("all", "0")
    assert (first["language"], first["filter"]) == ("all", "recent")
    assert (first["start_date"], first["end_date"]) == (9 * DAY, 11 * DAY)


def test_a_refusal_is_waited_out_longer_each_time_and_for_as_long_as_steam_says():
    assert [next_wait(n, 0, None) for n in (1, 2, 3)] == [2, 4, 8]
    assert next_wait(20, 0, None) == BACKOFF_CAP
    assert next_wait(1, 0, 90) == 90
    assert next_wait(1, PATIENCE - 1, None) is None


def test_a_cursor_that_stops_moving_ends_the_window():
    calls = []

    def page(app, query):
        calls.append(query["cursor"])
        return {"reviews": [{"recommendationid": "8", "review": "x"}], "cursor": "stuck"}

    assert fetch(1, {"1": DAY}, page) == {}
    assert calls == ["*", "stuck"]


def test_a_claim_is_cut_from_its_text_by_bytes_and_only_from_the_text_it_was_labelled_on():
    text = "Ünïcode first. Then the claim."
    start = len("Ünïcode first. ".encode("utf-8"))
    claims = [
        {"review_id": "1", "start": start, "end": start + len("Then the claim."), "subject": "x"},
        {"review_id": "2", "start": 0, "end": 3, "subject": "y"},
        {"review_id": "3", "start": 0, "end": 3, "subject": "z"},
        {"review_id": "4", "start": 0, "end": 3, "subject": "w"},
    ]
    facts = [
        {"review_id": "1", "text_sha256": fingerprint(text)},
        {"review_id": "2", "text_sha256": fingerprint("before an edit")},
        {"review_id": "3", "text_sha256": fingerprint("deleted")},
    ]
    texts = {"1": text, "2": "after an edit"}

    out, left = joined(claims, facts, texts)

    assert [row["text"] for row in out] == ["Then the claim."]
    assert out[0]["subject"] == "x"
    assert left == {"deleted": 1, "edited": 1, "no facts": 1}
