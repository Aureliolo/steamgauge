"""The one fact about a game the model is told, and that it is told it the way the reader is.

python -m pytest training/test_headset.py
"""

from __future__ import annotations

import dataclasses

import pytest

import claimdata
from test_aspects import NoTokenizer
from test_claimdata import claim
from train import HEADSET, Claims, Reading


def told(*, headset_only: bool, marker: bool) -> str:
    # An offset of -1 keeps the whole review as the window, so no tokenizer is needed to see
    # what opens it.
    asked = dataclasses.replace(claim(1), review_offset=-1, headset_only=headset_only)
    return Claims(
        [asked],
        NoTokenizer(),
        ["verdict"],
        Reading(max_length=16, context=True, headset_marker=marker),
    ).pair(0)[1]


def test_a_headset_game_opens_its_window_by_saying_so_and_no_other_does() -> None:
    review = claim(1).review
    assert told(headset_only=True, marker=True) == f"Played in a VR headset. {review}"
    assert told(headset_only=False, marker=True) == review, "a screen game reads as it always did"
    assert told(headset_only=True, marker=False) == review, (
        "a reader not trained on the fact is not told it"
    )


def test_the_words_are_the_ones_the_rust_reader_writes() -> None:
    # Written out rather than imported: the Rust reader holds the same string in `HEADSET` in
    # crates/steamgauge-core/src/reader.rs, and a test that shared a constant would agree with
    # a change made on one side only.
    assert HEADSET == "Played in a VR headset."


def test_the_fact_is_told_for_every_game_or_for_none() -> None:
    def at(app_id: int, *, headset_only: bool | None) -> claimdata.Claim:
        return dataclasses.replace(claim(app_id), headset_only=headset_only)

    assert claimdata.headset_told([at(1, headset_only=None), at(2, headset_only=None)]) is False
    assert claimdata.headset_told([at(1, headset_only=True), at(2, headset_only=False)]) is True
    with pytest.raises(SystemExit, match="store-facts"):
        claimdata.headset_told([at(1, headset_only=True), at(2, headset_only=None)])
