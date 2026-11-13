"""The table of a reader's mistakes writes its counts with a comma between thousands.

python -m pytest test_confusion.py
"""

from __future__ import annotations

import sys
from typing import TYPE_CHECKING

import numpy as np

import confusion

if TYPE_CHECKING:
    from pathlib import Path

    import pytest


def test_the_table_writes_its_counts_with_commas(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    # Performance is subject 0 and gameplay subject 1. 1,300 performance claims: 100 read
    # right and 1,200 as gameplay. 2,300 gameplay claims: 1,200 read right and 1,100 as
    # performance.
    truth = np.array([0] * 1300 + [1] * 2300)
    predicted = np.array([0] * 100 + [1] * 1200 + [1] * 1200 + [0] * 1100)
    logits = np.zeros((len(truth), 2))
    logits[np.arange(len(truth)), predicted] = 5.0
    np.savez(
        tmp_path / "fold.npz",
        subjects=np.array(["performance", "gameplay"]),
        app_id=np.arange(len(truth)),
        truth=truth,
        logits=logits,
    )
    monkeypatch.setattr(
        sys,
        "argv",
        ["confusion.py", "--oof", str(tmp_path), "--subject", "gameplay", "--examples", "1100"],
    )

    confusion.main()
    out = capsys.readouterr().out

    performance = next(line for line in out.splitlines() if line.startswith("performance "))
    gameplay = next(line for line in out.splitlines() if line.startswith("gameplay "))
    assert " 1,300  1,200 " in performance
    assert "gameplay 1,200" in performance
    assert " 2,300  2,400 " in gameplay
    assert "performance 1,100" in gameplay
    assert "gameplay: 2,300 true, 2,400 predicted" in out
    assert "read as:  performance 1,100" in out
    assert "labelled:  performance 1,200" in out
    assert "the 1,100 most confident of its 1,200 mistakes" in out
