"""A decoder reader has to trace to a graph that answers what the model answers.

python -m pytest test_export.py
"""

from __future__ import annotations

import numpy as np
import pytest
import torch

onnxruntime = pytest.importorskip("onnxruntime")
from transformers import Qwen3Config, Qwen3Model  # noqa: E402

from export import trace_graph  # noqa: E402
from train import ClaimReader  # noqa: E402


@pytest.fixture
def backbone(tmp_path):
    """A decoder of the 4B's family, small enough to build without a download."""
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
    Qwen3Model(config).save_pretrained(tmp_path / "trunk")
    return str(tmp_path / "trunk")


def test_a_decoder_reader_traces_and_answers_what_the_model_answers(backbone, tmp_path):
    # Right-padded to different lengths, which is what the last-token pooling reads by, and a
    # batch of another size than the trace's, which is what the dynamic axes are for.
    model = ClaimReader(backbone, 3, pooling="last").eval()
    lengths = [6, 3, 5, 1]
    ids = torch.randint(0, 64, (len(lengths), 6))
    mask = torch.tensor([[1] * n + [0] * (6 - n) for n in lengths])
    graph = tmp_path / "model.onnx"
    trace_graph(model, ids[:2], mask[:2], graph, 17)

    with torch.no_grad():
        wanted = model(ids, mask)[0].numpy()
    session = onnxruntime.InferenceSession(str(graph), providers=["CPUExecutionProvider"])
    got = session.run(
        ["subject_logits"], {"input_ids": ids.numpy(), "attention_mask": mask.numpy()}
    )[0]
    assert np.allclose(wanted, got, atol=1e-4)


def test_the_trace_leaves_the_library_as_it_found_it(backbone, tmp_path):
    from transformers.masking_utils import ALL_MASK_ATTENTION_FUNCTIONS, sdpa_mask

    model = ClaimReader(backbone, 3, pooling="last").eval()
    ids = torch.randint(0, 64, (2, 4))
    trace_graph(model, ids, torch.ones_like(ids), tmp_path / "model.onnx", 17)
    assert ALL_MASK_ATTENTION_FUNCTIONS["sdpa"] is sdpa_mask


def test_the_exporter_is_handed_a_path_it_can_write_weights_beside(monkeypatch, tmp_path):
    # Only a graph over 2 GB fails on a Path, and no model that size builds in a test, so what
    # is checked is what the exporter is given: anything but a string it treats as a stream.
    handed = []
    monkeypatch.setattr(torch.onnx, "export", lambda model, args, f, **kwargs: handed.append(f))
    trace_graph(torch.nn.Identity(), None, None, tmp_path / "model.onnx", 17)
    assert handed == [str(tmp_path / "model.onnx")]
