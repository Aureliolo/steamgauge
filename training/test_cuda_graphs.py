"""What a recorded training pass is built from must read a claim exactly as the eager reader does.

The recording itself needs a card and is measured there; these hold on any machine.

python -m pytest test_cuda_graphs.py
"""

from __future__ import annotations

import pytest
import torch
from transformers import BertConfig, BertModel

from train import ClaimReader, Passes


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


@pytest.fixture
def claims():
    ids = torch.randint(2, 64, (3, 12))
    lengths = torch.tensor([12, 7, 3])
    mask = (torch.arange(12)[None, :] < lengths[:, None]).long()
    return ids * mask, mask


def test_the_square_mask_reads_every_claim_as_the_flat_one_does(backbone, claims):
    ids, mask = claims
    model = ClaimReader(backbone, 4, aspects=True).eval()
    flat = model(ids, mask)
    model.square_mask = True
    square = model(ids, mask)
    for a, b in zip(flat, square, strict=True):
        assert torch.allclose(a, b, atol=1e-5)


def test_the_square_mask_holds_on_a_batch_with_no_padding(backbone, claims):
    ids, _ = claims
    mask = torch.ones_like(ids)
    model = ClaimReader(backbone, 4).eval()
    flat = model(ids, mask)
    model.square_mask = True
    for a, b in zip(flat, model(ids, mask), strict=True):
        assert torch.allclose(a, b, atol=1e-5)


def test_a_pass_returns_what_the_loss_reads_and_nothing_else(backbone, claims):
    ids, mask = claims
    model = ClaimReader(backbone, 4, aspects=True).eval()
    subject, polarity, _, aspects = model(ids, mask)

    once = Passes(model, twice=False, aspects=True)(ids, mask)
    assert len(once) == 3
    for a, b in zip(once, (subject, polarity, aspects), strict=True):
        assert torch.equal(a, b)

    twice = Passes(model, twice=True, aspects=True)(ids, mask)
    assert len(twice) == 5
    for a, b in zip(twice, (subject, polarity, aspects, subject, polarity), strict=True):
        assert torch.equal(a, b)


def test_a_teacher_pass_reads_no_aspects(backbone, claims):
    ids, mask = claims
    model = ClaimReader(backbone, 4, aspects=True).eval()
    assert len(Passes(model, twice=False, aspects=False)(ids, mask)) == 2
