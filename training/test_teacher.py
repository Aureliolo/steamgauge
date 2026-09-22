"""A teacher taught through adapters has to come back as a plain reader teach.py can load.

python -m pytest test_teacher.py
"""

from __future__ import annotations

import pytest
import torch

pytest.importorskip("peft")
from transformers import Qwen3Config, Qwen3Model  # noqa: E402

from train import ClaimReader, adapt, parameter_groups  # noqa: E402


@pytest.fixture
def backbone(tmp_path):
    """A decoder of the teacher's family, small enough to build without a download."""
    config = Qwen3Config(
        vocab_size=64,
        hidden_size=32,
        intermediate_size=64,
        num_hidden_layers=2,
        num_attention_heads=4,
        num_key_value_heads=2,
        head_dim=8,
        max_position_embeddings=64,
    )
    torch.manual_seed(0)
    Qwen3Model(config).save_pretrained(tmp_path)
    return str(tmp_path)


def test_only_the_adapters_and_the_heads_learn(backbone):
    model = ClaimReader(backbone, 3, pooling="last", dtype=torch.bfloat16)
    adapt(model, 4)
    learning = {name for name, p in model.named_parameters() if p.requires_grad}
    assert learning, "nothing would learn"
    assert all("lora_" in name or not name.startswith("trunk.") for name in learning)
    handed = {id(p) for group in parameter_groups(model, 1e-4, 0.9) for p in group["params"]}
    assert handed == {id(p) for p in model.parameters() if p.requires_grad}


def test_a_merged_teacher_loads_as_a_plain_reader_and_answers_the_same(backbone):
    model = ClaimReader(backbone, 3, pooling="last", dtype=torch.bfloat16)
    adapt(model, 4)
    ids = torch.randint(0, 64, (2, 6))
    mask = torch.ones_like(ids)
    optimiser = torch.optim.AdamW(parameter_groups(model, 1e-2, 1.0))
    subject, _, _ = model(ids, mask)
    subject.sum().backward()
    optimiser.step()

    model.eval()
    before, _, _ = model(ids, mask)
    model.trunk = model.trunk.merge_and_unload()
    state = model.state_dict()

    plain = ClaimReader(backbone, 3, pooling="last", dtype=torch.bfloat16)
    plain.load_state_dict(state)
    plain.eval()
    after, _, _ = plain(ids, mask)
    assert torch.allclose(before, after, atol=5e-2)
