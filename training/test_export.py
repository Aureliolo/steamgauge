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
    # Right-padded to different lengths, which is what the last-token pooling reads by, and
    # batches of another size and another length than the trace's, which is what the dynamic
    # axes are for. A graph checked only at the length it was traced at passed here and then
    # refused every other length the 4B was asked about.
    model = ClaimReader(backbone, 3, pooling="last").eval()
    traced = torch.randint(0, 64, (2, 6))
    graph = tmp_path / "model.onnx"
    trace_graph(model, traced, torch.ones_like(traced), graph, 17)
    session = onnxruntime.InferenceSession(str(graph), providers=["CPUExecutionProvider"])

    for width, lengths in ((6, [6, 3, 5, 1]), (11, [11, 2, 7]), (3, [3, 1])):
        ids = torch.randint(0, 64, (len(lengths), width))
        mask = torch.tensor([[1] * n + [0] * (width - n) for n in lengths])
        with torch.no_grad():
            wanted = model(ids, mask)[0].numpy()
        got = session.run(
            ["subject_logits"], {"input_ids": ids.numpy(), "attention_mask": mask.numpy()}
        )[0]
        assert np.allclose(wanted, got, atol=1e-4), f"{width} tokens"


def test_the_trace_leaves_the_model_as_it_found_it(backbone, tmp_path):
    model = ClaimReader(backbone, 3, pooling="last").eval()
    trunk = model.trunk
    ids = torch.randint(0, 64, (2, 4))
    trace_graph(model, ids, torch.ones_like(ids), tmp_path / "model.onnx", 17)
    assert model.trunk is trunk


def test_the_exporter_is_handed_a_path_it_can_write_weights_beside(monkeypatch, tmp_path):
    # Only a graph over 2 GB fails on a Path, and no model that size builds in a test, so what
    # is checked is what the exporter is given: anything but a string it treats as a stream.
    handed = []
    monkeypatch.setattr(torch.onnx, "export", lambda model, args, f, **kwargs: handed.append(f))
    trace_graph(torch.nn.Identity(), None, None, tmp_path / "model.onnx", 17)
    assert handed == [str(tmp_path / "model.onnx")]


def test_claims_on_their_line_may_change_side_one_at_a_time_and_not_in_bulk():
    from export import crossed_the_line

    # Forty claims whose best subject sits exactly on its line; in the other graph a rounding
    # takes it just under. One doing so is excused, all of them together are not.
    wanted = np.tile(np.array([[1.0, 0.0, 0.0]]), (40, 1))
    on_the_line = float(np.exp(1.0) / (np.exp(1.0) + 2.0))
    lines = [on_the_line, 0.5, 0.5]
    one = wanted.copy()
    one[0, 0] -= 1e-3
    crossed_the_line(wanted, one, lines, 0.25)
    every = wanted.copy()
    every[:, 0] -= 1e-3
    with pytest.raises(SystemExit, match="unsteady"):
        crossed_the_line(wanted, every, lines, 0.25)
    # One from well off its line is refused on its own.
    with pytest.raises(SystemExit, match="further than"):
        crossed_the_line(np.array([[4.0, 0.0]]), np.array([[0.1, 0.0]]), [0.6, 0.6], 0.01)
