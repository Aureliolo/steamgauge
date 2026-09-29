"""A run that keeps its best epoch ships that epoch's weights and copies only what it trained."""

from __future__ import annotations

import torch

from train import BestEpoch


def model() -> torch.nn.Module:
    torch.manual_seed(0)
    built = torch.nn.Sequential(torch.nn.Linear(4, 4), torch.nn.Linear(4, 2))
    # The first layer stands for a LoRA run's frozen base: the part that does not fit twice.
    built[0].requires_grad_(False)
    return built


def step(built: torch.nn.Module, by: float) -> None:
    with torch.no_grad():
        for weight in built.parameters():
            if weight.requires_grad:
                weight.add_(by)


def test_the_best_epoch_comes_back_after_worse_ones():
    built = model()
    best = BestEpoch()
    step(built, 1.0)
    assert best.offer(0.60, 1, built)
    step(built, 1.0)
    assert best.offer(0.64, 2, built)
    at_best = {name: weight.clone() for name, weight in built.named_parameters()}
    step(built, 1.0)
    assert not best.offer(0.62, 3, built)

    best.restore(built)

    assert best.epoch == 2 and best.score == 0.64
    for name, weight in built.named_parameters():
        assert torch.equal(weight, at_best[name]), name


def test_a_tie_keeps_the_earlier_epoch():
    built = model()
    best = BestEpoch()
    assert best.offer(0.64, 1, built)
    assert not best.offer(0.64, 2, built)
    assert best.epoch == 1


def test_only_the_trained_weights_are_copied():
    built = model()
    best = BestEpoch()
    best.offer(0.5, 1, built)

    assert set(best.weights) == {"1.weight", "1.bias"}
    assert all(weight.device.type == "cpu" for weight in best.weights.values())


def test_restoring_writes_into_the_same_tensors():
    # A run recorded as CUDA graphs reads its weights at fixed addresses, so the best epoch is
    # copied back into them rather than put in their place.
    built = model()
    best = BestEpoch()
    best.offer(0.5, 1, built)
    before = {name: weight.data_ptr() for name, weight in built.named_parameters()}
    step(built, 1.0)

    best.restore(built)

    assert {name: weight.data_ptr() for name, weight in built.named_parameters()} == before
