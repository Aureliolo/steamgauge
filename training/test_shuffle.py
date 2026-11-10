"""The labelled batches keep the order the seed gives, with or without a pool drawn beside them.

python -m pytest test_shuffle.py
"""

from __future__ import annotations

import torch
from torch.utils.data import DataLoader, TensorDataset

from train import forever, pool_loader
from typed_calls import manual_seed


def loader(size: int) -> DataLoader[tuple[torch.Tensor, ...]]:
    return DataLoader(TensorDataset(torch.arange(size)), batch_size=2, shuffle=True, num_workers=0)


def order(batches: DataLoader[tuple[torch.Tensor, ...]]) -> list[int]:
    return [int(one) for (batch,) in batches for one in batch]


def test_the_second_labelled_epoch_is_the_same_with_or_without_a_pool_drawn_between() -> None:
    manual_seed(1)
    labelled = loader(12)
    order(labelled)
    alone = order(labelled)

    manual_seed(1)
    labelled = loader(12)
    pool = forever(pool_loader(TensorDataset(torch.arange(12)), 2, 1, drop_last=False))
    order(labelled)
    next(pool)
    beside_a_pool = order(labelled)

    assert beside_a_pool == alone
