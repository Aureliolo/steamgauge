"""Typed wrappers for torch and transformers calls whose signatures the libraries leave unannotated.

Each gap is silenced once here, so a call site elsewhere stays fully checked.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, cast

import torch

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from transformers import PreTrainedModel, PreTrainedTokenizerBase


def manual_seed(seed: int) -> None:
    """Seed torch's generators."""
    torch.manual_seed(seed)  # pyright: ignore[reportUnknownMemberType]  # torch leaves the seed unannotated


def save_pretrained(owner: PreTrainedModel | PreTrainedTokenizerBase, path: Path) -> None:
    """Write a model or tokenizer to a folder."""
    save = cast("Callable[[Path], object]", owner.save_pretrained)  # pyright: ignore[reportUnknownMemberType]  # transformers leaves the path and kwargs unparameterised
    save(path)
