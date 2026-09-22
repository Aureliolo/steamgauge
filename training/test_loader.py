"""Whether the dataset survives being cut in another process.

`--workers` hands the dataset to worker processes, and on Windows that means pickling it and
importing this module again in each one. A dataset that cannot make that trip fails in the
first seconds of a run, which is a cheap failure and a stupid one to discover at midnight.

python -m pytest training/test_loader.py
"""

from __future__ import annotations

import torch
from torch.utils.data import DataLoader

from test_claimdata import claim
from train import Claims

SUBJECTS = ["verdict", "gameplay", "story"]


class Tokenizer:
    """Enough of a tokenizer to build an item, and picklable, which is the point here."""

    def __call__(self, *texts, **_):
        return {
            "input_ids": torch.zeros(1, 4, dtype=torch.long),
            "attention_mask": torch.ones(1, 4, dtype=torch.long),
            "offset_mapping": [],
        }


def test_a_batch_cut_in_another_process_is_the_batch_this_one_would_have_cut():
    claims = [claim(at) for at in range(8)]
    dataset = Claims(claims, Tokenizer(), SUBJECTS, 4)
    here = next(iter(DataLoader(dataset, batch_size=4, num_workers=0)))
    there = next(iter(DataLoader(dataset, batch_size=4, num_workers=2)))
    for field, mine in here.items():
        assert torch.equal(mine, there[field]), field
