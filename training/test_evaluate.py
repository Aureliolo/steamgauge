"""Validation holds no gradients: with them on, the first validation pass allocates over 30 GB
and the run dies.

python -m pytest test_evaluate.py
"""

from __future__ import annotations

from pathlib import Path


def test_evaluate_runs_under_no_grad():
    source = (Path(__file__).parent / "train.py").read_text(encoding="utf-8")
    assert "@torch.no_grad()\ndef evaluate(" in source
