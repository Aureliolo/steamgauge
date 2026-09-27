"""Holding less on the card must not change what a run learns.

python -m pytest test_card_memory.py
"""

from __future__ import annotations

import pytest
import torch
from transformers import BertConfig, BertModel

from train import ClaimReader, average_of, recompute_activations


@pytest.fixture
def backbone(tmp_path):
    """An encoder small enough to build without a download."""
    config = BertConfig(
        vocab_size=64,
        hidden_size=32,
        intermediate_size=64,
        num_hidden_layers=2,
        num_attention_heads=4,
        max_position_embeddings=64,
    )
    torch.manual_seed(0)
    BertModel(config).save_pretrained(tmp_path)
    return str(tmp_path)


def gradients(model, ids, mask):
    model.train()
    torch.manual_seed(1)
    subject, polarity, *_ = model(ids, mask)
    (subject.square().sum() + polarity.square().sum()).backward()
    return {name: p.grad.clone() for name, p in model.named_parameters() if p.grad is not None}


def test_recomputed_activations_give_the_same_gradient_dropout_included(backbone):
    ids = torch.randint(0, 64, (3, 10))
    mask = torch.ones_like(ids)
    torch.manual_seed(2)
    kept = ClaimReader(backbone, 4)
    state = kept.state_dict()
    recomputed = ClaimReader(backbone, 4)
    recomputed.load_state_dict(state)
    recompute_activations(recomputed)
    assert recomputed.trunk.is_gradient_checkpointing

    before = gradients(kept, ids, mask)
    after = gradients(recomputed, ids, mask)
    assert before.keys() == after.keys()
    for name, grad in before.items():
        assert torch.allclose(grad, after[name], atol=1e-6), name


def test_an_average_kept_on_the_host_is_the_same_average(backbone):
    model = ClaimReader(backbone, 4)
    on_card = average_of(model, 0.9, on_host=False)
    on_host = average_of(model, 0.9, on_host=True)
    assert {p.device.type for p in on_host.module.parameters()} == {"cpu"}
    for _ in range(3):
        with torch.no_grad():
            for p in model.parameters():
                p.add_(torch.randn_like(p))
        on_card.update_parameters(model)
        on_host.update_parameters(model)
    for a, b in zip(on_card.module.parameters(), on_host.module.parameters(), strict=True):
        assert torch.allclose(a, b)
