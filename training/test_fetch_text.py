"""The published labels get their text back only where it is the text they were written against.

python -m pytest test_fetch_text.py
"""

from __future__ import annotations

import hashlib
import json
import urllib.parse
import urllib.request
from typing import TYPE_CHECKING, Literal, Self

from fetch_text import (
    BACKOFF_CAP,
    DAY,
    ENDPOINT,
    PATIENCE,
    fetch,
    joined,
    main,
    next_wait,
    steam_page,
    windows,
)

if TYPE_CHECKING:
    from pathlib import Path
    from types import TracebackType

    import pytest

    from fetch_text import Row


def fingerprint(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def test_windows_cover_each_review_a_day_either_side_and_merge_overlaps() -> None:
    assert windows([10 * DAY, 10 * DAY + 5, 50 * DAY]) == [
        (9 * DAY, 11 * DAY + 5),
        (49 * DAY, 51 * DAY),
    ]


def test_fetch_walks_each_window_with_the_census_parameters_and_keeps_only_what_was_asked() -> None:
    asked: list[tuple[int, Row]] = []

    def page(app: int, query: Row) -> Row:
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
    assert (first["purchase_type"], first["filter_offtopic_activity"]) == (1, False)
    assert (first["languages"], first["filter"]) == (["all"], 1)
    assert (first["date_range_start"], first["date_range_end"]) == (9 * DAY, 11 * DAY)


def test_a_page_is_asked_of_the_web_api_with_its_arguments_as_one_json_value(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    sent: list[str] = []

    class Answer:
        def __enter__(self) -> Self:
            return self

        def __exit__(
            self,
            exc_type: type[BaseException] | None,
            exc: BaseException | None,
            traceback: TracebackType | None,
        ) -> Literal[False]:
            return False

        def read(self) -> bytes:
            return json.dumps({"response": {"reviews": [], "cursor": "c+/="}}).encode()

    def urlopen(request: urllib.request.Request, **_: object) -> Answer:
        sent.append(request.full_url)
        return Answer()

    monkeypatch.setattr("fetch_text.urllib.request.urlopen", urlopen)

    def no_sleep(_seconds: float) -> None:
        return None

    monkeypatch.setattr("fetch_text.time.sleep", no_sleep)

    answer = steam_page(7, {"cursor": "a+b/c=", "filter": 1})

    assert answer == {"reviews": [], "cursor": "c+/="}
    url, query = sent[0].split("?", 1)
    assert url == ENDPOINT
    arguments = json.loads(urllib.parse.parse_qs(query)["input_json"][0])
    assert arguments == {"appid": 7, "cursor": "a+b/c=", "filter": 1}


def test_a_refusal_is_waited_out_longer_each_time_and_for_as_long_as_steam_says() -> None:
    assert [next_wait(n, 0, None) for n in (1, 2, 3)] == [2, 4, 8]
    assert next_wait(20, 0, None) == BACKOFF_CAP
    assert next_wait(1, 0, 90) == 90
    assert next_wait(1, PATIENCE - 1, None) is None


def test_a_cursor_that_stops_moving_ends_the_window() -> None:
    calls: list[str] = []

    def page(_app: int, query: Row) -> Row:
        calls.append(query["cursor"])
        return {"reviews": [{"recommendationid": "8", "review": "x"}], "cursor": "stuck"}

    assert fetch(1, {"1": DAY}, page) == {}
    assert calls == ["*", "stuck"]


def test_a_claim_is_cut_from_its_text_by_bytes_and_only_from_the_text_it_was_labelled_on() -> None:
    text = "Ünïcode first. Then the claim."
    start = len("Ünïcode first. ".encode())
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
    assert left == {"deleted": 1, "edited": 1, "no facts": 1, "holds no such bytes": 0}


CENSORED_TEXT = "The story is ♥♥♥♥ boring. Loud."
PLAIN_TEXT = "The story is damn boring. Loud."
CENSORED_CLAIMS = [
    {"review_id": "1", "index": 0, "start": 0, "end": 33},
    {"review_id": "1", "index": 1, "start": 34, "end": 39},
]


def censored_fact(runs: list[list[int]]) -> Row:
    return {
        "review_id": "1",
        "app_id": 7,
        "created": 5,
        "text_sha256": fingerprint(CENSORED_TEXT),
        "censored": runs,
    }


def test_censored_words_are_censored_again_before_a_claim_is_cut() -> None:
    out, left = joined(CENSORED_CLAIMS, [censored_fact([[13, 4]])], {"1": PLAIN_TEXT})

    assert [row["text"] for row in out] == ["The story is ♥♥♥♥ boring.", "Loud."]
    assert left["edited"] == 0


def test_a_text_that_differs_beyond_its_censor_marks_is_still_left_out() -> None:
    out, left = joined(CENSORED_CLAIMS, [censored_fact([[13, 3]])], {"1": PLAIN_TEXT})

    assert out == []
    assert left["edited"] == 2


def test_a_censored_run_past_the_end_of_the_text_is_no_error() -> None:
    out, left = joined(CENSORED_CLAIMS, [censored_fact([[13, 4]])], {"1": "Short now."})

    assert out == []
    assert left["edited"] == 2


def test_a_label_that_cannot_be_cut_is_left_out_and_counted() -> None:
    text = "ünï good."
    claims = [
        {"review_id": "2", "start": 1, "end": 11},
        {"review_id": "2", "start": 0, "end": 40},
        {"review_id": "2", "start": 6, "end": 11},
    ]
    facts = [{"review_id": "2", "text_sha256": fingerprint(text)}]

    out, left = joined(claims, facts, {"2": text})

    assert [row["text"] for row in out] == ["good."]
    assert left["holds no such bytes"] == 2


def test_the_facts_are_read_from_review_facts_jsonl(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    (tmp_path / "claims.jsonl").write_text(
        "".join(json.dumps(claim) + "\n" for claim in CENSORED_CLAIMS), encoding="utf-8"
    )
    (tmp_path / "review-facts.jsonl").write_text(
        json.dumps(censored_fact([[13, 4]])) + "\n", encoding="utf-8"
    )

    def plain(_app: int, _wanted: dict[str, int]) -> dict[str, str]:
        return {"1": PLAIN_TEXT}

    monkeypatch.setattr("fetch_text.fetch", plain)
    written = tmp_path / "out.jsonl"
    monkeypatch.setattr(
        "sys.argv", ["fetch_text.py", "--data", str(tmp_path), "--to", str(written)]
    )

    main()

    rows = [json.loads(line) for line in written.read_text(encoding="utf-8").splitlines()]
    assert [row["text"] for row in rows] == [
        "The story is ♥♥♥♥ boring.",
        "Loud.",
    ]
