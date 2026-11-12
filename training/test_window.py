"""The window a claim is read in, cut as the Rust reader cuts it.

python -m pytest test_window.py
"""

from __future__ import annotations

import dataclasses
import json
from pathlib import Path
from typing import TYPE_CHECKING, Any

import pytest

from test_claimdata import claim
from train import Claims, Reading

if TYPE_CHECKING:
    from collections.abc import Mapping

# crates/steamgauge-core/src/reader.rs cuts every one of these cases too, in
# `every_window_case_is_cut_as_the_trainer_cuts_it`.
CASES: list[dict[str, Any]] = json.loads(
    (Path(__file__).resolve().parent.parent / "reference" / "window-cases.json").read_text(
        encoding="utf-8"
    )
)["cases"]


class GivenOffsets:
    """A tokenizer that hands back the offsets a case names, whatever the text."""

    def __init__(self, offsets: list[list[int]]) -> None:
        """Hold the offsets to hand back."""
        self.offsets = [(start, end) for start, end in offsets]

    def __call__(
        self,
        text: str,
        pair: str | None = None,
        /,
        **kwargs: object,
    ) -> Mapping[str, object]:
        """Return the case's offsets."""
        del text, pair, kwargs
        return {"offset_mapping": self.offsets}


def test_every_case_is_read() -> None:
    assert len(CASES) == 8


@pytest.mark.parametrize("case", CASES, ids=[case["name"] for case in CASES])
def test_the_window_is_the_one_the_rust_reader_cuts(case: dict[str, Any]) -> None:
    review: str = case["review"]
    at, length = case["at"], case["length"]
    asked = dataclasses.replace(
        claim(1), review=review, review_offset=at, text=review[at : at + length]
    )
    claims = Claims(
        [asked],
        GivenOffsets(case["offsets"]),
        ["verdict"],
        Reading(max_length=case["budget"], context=True),
    )
    assert claims.cut(asked) == case["window"]
