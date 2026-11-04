"""Exports a trained run to ONNX, and refuses to believe it worked without checking.

An fp16 export can quietly disagree with the model it came from. Not by much, and not on
every input, which is exactly what makes it dangerous: the graph loads, the numbers look
plausible, and a category boundary has moved. So the export is followed by a parity check on
real claims, and a disagreement fails the run rather than printing a warning.

    python export.py --run runs/xlm-roberta-base-1788899000
"""

from __future__ import annotations

import argparse
import hashlib
import json
import multiprocessing
import os
import shutil
from concurrent.futures import ProcessPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import numpy as np
import torch
from transformers import AutoTokenizer

import backbones
import claimdata
import sizes
from train import ClaimReader, Claims, Reading, TrunkLoad

if TYPE_CHECKING:
    from collections.abc import Iterable, Iterator, Sequence

    import numpy.typing as npt
    from onnx import GraphProto, ModelProto, TensorProto
    from torch.utils.hooks import RemovableHandle
    from transformers import PreTrainedTokenizerBase
    from transformers.modeling_outputs import BaseModelOutputWithPast

HERE = Path(__file__).resolve().parent

# The token budget a reader without a recorded `max_tokens` is described as having.
DEFAULT_MAX_TOKENS = 128

# A run's `run.json`, `reader.json` or abstention figures: JSON whose keys vary by run.
Record = dict[str, Any]

# How many claims go through a model or graph at once while checking parity.
CHECK_BATCH = 32

# How many claims the graph is traced on. The batch axis is dynamic, so a handful gives the same
# graph as the whole check batch without the exporter crashing on a large one.
TRACE_CLAIMS = 8

# Reordered ties and near-line side changes are tolerable one at a time and not in bulk: more
# than one claim in this many means the graph is not answering where the model would.
BULK_FRACTION = 20

# Bytes copied per read while gathering external weights, so no weight is held whole in memory.
COPY_CHUNK = 64 * 1024 * 1024

# DirectML refuses a tensor of this many elements or more.
DIRECTML_ELEMENTS = 2**32

# A weight matrix has this many dimensions.
MATRIX_DIMS = 2

# The z-score of a 95% interval.
Z_95 = 1.959963985

# A threshold below this multiple of the uniform guess is not abstention.
COIN_FACTOR = 2.0

# The reader's name says what it reads. A run id names an experiment by its encoder and label
# count and is the right name for the index. A thing somebody downloads and cites wants a
# name that is not a serial and needs no explaining.
READER_NAME = "Game Review Reader"

# What a claim's logits may drift by between PyTorch and the exported graph. Tight enough
# that a changed argmax cannot hide inside it on anything but a genuine tie.
TOLERANCE = 2e-3

# Half precision keeps about three decimal digits of each number, so a logit drifts in
# proportion to its size, and the canary is a share of the largest logit rather than a
# distance. It is the canary for a graph that is not the model, and it is asked of the graph on
# the processor, whose half-precision kernels keep what the format keeps: the retrained 4B's
# graph drifted there by 0.22% of a claim's largest logit at most, a tenth of this.
#
# DirectML is where the graph runs, and its half-precision kernels drift further and unevenly:
# the same graph over the same 256 claims drifted a median 0.8% there and 8.7% on one claim,
# with no relation to the size of the logits, while every answer but one tie came back the
# same. A canary on DirectML's worst claim measures that claim's arithmetic rather than the
# graph, and it refused a graph that was the model. What it does to the answers where they are
# given is guarded by the argmax, the tie count and the lines, which are asked on DirectML.
HALF_TOLERANCE = 2.3e-2

# How far from its abstention line, in probability, a claim may sit and still be excused for
# changing side under half precision: the room the lines were first held to.
HALF_LINE_ROOM = 2.5e-1


class LengthFree(torch.nn.Module):
    """A decoder trunk handed its mask and positions ready-made, from tensors alone.

    Left to itself a decoder works out its causal mask and its positions from the sequence
    length as a Python number, and the tracer writes a Python number into the graph as a
    constant: the 4B's graph answered at exactly the length it was traced at and refused every
    other one. Built here from a running count over the attention mask instead, both stay
    questions the graph asks of its input. The library takes a four-dimensional mask as given,
    so its own mask builder, which also leans on `torch.vmap` the tracer cannot follow, never
    runs.
    """

    def __init__(self, trunk: torch.nn.Module) -> None:
        """Wrap `trunk`, the decoder whose mask and positions this builds."""
        super().__init__()
        self.trunk = trunk

    def forward(
        self, input_ids: torch.Tensor, attention_mask: torch.Tensor
    ) -> BaseModelOutputWithPast:
        """Run the trunk over `input_ids` with a mask and positions built from tensors."""
        steps = torch.cumsum(torch.ones_like(attention_mask), dim=1)
        positions = steps - 1
        # Right-padded, so every query row still sees its own real tokens and none is empty.
        causal = steps[:, None, :] <= steps[:, :, None]
        allowed = (causal & attention_mask[:, None, :].bool())[:, None, :, :]
        return self.trunk(
            input_ids=input_ids,
            attention_mask=allowed,
            position_ids=positions,
            cache_position=positions[0],
        )


def trace_graph(
    model: torch.nn.Module,
    input_ids: torch.Tensor,
    attention_mask: torch.Tensor,
    path: Path,
    opset: int,
) -> None:
    """Write a reader's graph, whatever its trunk.

    A decoder trunk is traced through `LengthFree`, and an encoder as it is: an encoder takes
    only a two-dimensional mask and refuses the one `LengthFree` builds. A decoder also keeps a
    cache of past keys for generating text, which a reader that answers once has no use for and
    a traced graph should not carry as outputs.

    A decoder is told apart by its attention layers declaring themselves causal. Its config says
    nothing reliable: an encoder's carries `use_cache` too, for the rare use of it as a decoder,
    and a decoder's own `is_decoder` is left false.
    """
    readers: list[Any] = [
        module
        for module in model.modules()
        if isinstance(trunk := getattr(module, "trunk", None), torch.nn.Module)
        and any(getattr(layer, "is_causal", False) is True for layer in trunk.modules())
    ]
    for reader in readers:
        reader.trunk.config.use_cache = False
        reader.trunk = LengthFree(reader.trunk)
    outputs = ["subject_logits", "polarity_logits", "pooled"]
    if any(getattr(module, "aspects", None) is not None for module in model.modules()):
        outputs.append("aspect_logits")
    try:
        torch.onnx.export(  # pyright: ignore[reportUnknownMemberType]  # torch leaves ScriptFunction generics unknown
            model,
            (input_ids, attention_mask),
            # A string, never a Path: the tracing exporter takes anything else for a stream, and
            # a graph over the 2 GB a protobuf holds can only be written with its weights beside
            # it, in the directory a string names. The 4B traced for eleven minutes and died there.
            os.fspath(path),
            input_names=["input_ids", "attention_mask"],
            output_names=outputs,
            dynamic_axes={
                "input_ids": {0: "batch", 1: "tokens"},
                "attention_mask": {0: "batch", 1: "tokens"},
                **{name: {0: "batch"} for name in outputs},
            },
            opset_version=opset,
            # The tracing exporter rather than the dynamo one. Dynamo produces a graph full of
            # operators ONNX Runtime's CUDA provider does not implement, so it partitions the
            # model and copies tensors between host and device at every boundary: measured, the
            # same weights ran at sixty claims a second on a 4090 and the card sat at a quarter
            # busy. The traced graph is plainer and stays on the card.
            dynamo=False,
        )
    finally:
        for reader in readers:
            reader.trunk = reader.trunk.trunk


class InFullPrecisionOut(torch.nn.Module):
    """Runs the model in half precision and hands back full-precision numbers.

    The arithmetic is where the saving is. The small tensors that come out are not. A graph
    whose outputs are half precision forces every reader of it to know that, and the tool that
    runs this graph should not have to care which precision it was exported at.
    """

    def __init__(self, inner: torch.nn.Module) -> None:
        """Wrap `inner`, the half-precision model."""
        super().__init__()
        self.inner = inner

    def forward(self, input_ids: torch.Tensor, attention_mask: torch.Tensor) -> tuple[Any, ...]:
        """Return `inner`'s outputs widened to full precision."""
        return tuple(read.float() for read in self.inner(input_ids, attention_mask))


def load_reader(run: Path, record: Record) -> ClaimReader:
    """Load the trained reader of `run`, its weights at the precision they were stored at.

    Built as a shape and handed the trained weights, never filled: built from the pretrained
    weights at full precision, the placeholders and the trained weights were two copies of the 4B
    in memory at once, and then a third while the whole model was converted to full precision for
    the reference answers.
    """
    # Pooling is not a weight, so a run trained on the last token loads into a mean-pooling
    # model without complaint and exports a graph that reads its claims differently from the
    # one that was measured. The parity check cannot see it either, because it compares the
    # export against the same wrongly built model.
    model = ClaimReader(
        record["backbone"],
        len(record["subjects"]),
        pooling=record.get("pooling", "mean"),
        aspects=bool(record.get("aspects")),
        load=TrunkLoad(pretrained=False),
    )
    trained = torch.load(run / "model.bin", map_location="cpu", weights_only=True)
    model.load_state_dict(trained, assign=True)
    del trained
    empty = [name for name, weight in model.named_parameters() if weight.is_meta]
    if empty:
        msg = f"{run} holds no weights for {', '.join(empty[:5])}. Not exporting it."
        raise SystemExit(msg)
    return model.eval()


@torch.no_grad()
def full_precision_answers(
    model: torch.nn.Module, input_ids: torch.Tensor, attention_mask: torch.Tensor
) -> npt.NDArray[np.float32]:
    """Return the subject logits the model gives in full precision, whatever it is stored at.

    A layer at a time rather than the whole model at once: each module's own weights are widened
    just before it runs and put back just after, so the arithmetic is full precision throughout
    while memory holds the stored weights and one layer's widening, not a second model twice the
    size. Widening a stored number is exact and so is putting it back, so the answers are the
    whole-model conversion's and the weights come back bit for bit.
    """
    configs, caching = _stop_caching(model)
    widener = _Widener()
    hooks, whole = widener.attach(model)
    for module in whole:
        widener.widen(module)
    try:
        return _answers_in_batches(model, input_ids, attention_mask)
    finally:
        for hook in hooks:
            hook.remove()
        for module in whole:
            widener.narrow(module)
        _restore_caching(configs, caching)


def _stop_caching(model: torch.nn.Module) -> tuple[list[Any], list[Any]]:
    """Switch off every config's key-value cache and return the configs with what they held."""
    # A decoder keeps every layer's keys and values for the text it might go on to write, and
    # this writes none: for the 4B over 256 claims that cache was ten gigabytes.
    configs: list[Any] = [module.config for module in model.modules() if hasattr(module, "config")]
    caching = [getattr(config, "use_cache", None) for config in configs]
    for config in configs:
        if hasattr(config, "use_cache"):
            config.use_cache = False
    return configs, caching


def _restore_caching(configs: list[Any], caching: list[Any]) -> None:
    """Put back the `use_cache` values `_stop_caching` found."""
    for config, cached in zip(configs, caching, strict=True):
        if cached is not None:
            config.use_cache = cached


def _answers_in_batches(
    model: torch.nn.Module, input_ids: torch.Tensor, attention_mask: torch.Tensor
) -> npt.NDArray[np.float32]:
    """Return the model's subject logits over the claims, in the parity check's batches."""
    # All 256 claims at once held the 4B's widest activations for every one of them together.
    return np.concatenate(
        [
            model(input_ids[at : at + CHECK_BATCH], attention_mask[at : at + CHECK_BATCH])[0]
            .float()
            .numpy()
            for at in range(0, len(input_ids), CHECK_BATCH)
        ]
    )


class _Widener:
    """Widens a module's own weights to full precision and remembers what to put back."""

    def __init__(self) -> None:
        """Start with nothing widened."""
        self.stored: dict[torch.nn.Parameter, torch.dtype] = {}

    def widen(self, module: torch.nn.Module, _inputs: object = None) -> None:
        """Widen `module`'s own floating-point weights, noting their stored precision."""
        for weight in module.parameters(recurse=False):
            if weight.is_floating_point() and weight.dtype != torch.float32:
                self.stored[weight] = weight.dtype
                weight.data = weight.data.float()

    def narrow(
        self, module: torch.nn.Module, _inputs: object = None, _output: object = None
    ) -> None:
        """Return `module`'s widened weights to the precision they were stored at."""
        for weight in module.parameters(recurse=False):
            if weight in self.stored:
                weight.data = weight.data.to(self.stored.pop(weight))

    def attach(self, model: torch.nn.Module) -> tuple[list[RemovableHandle], list[torch.nn.Module]]:
        """Hook the trunk's layers to widen as they run and return the hooks and the rest."""
        # Only the trunk goes a layer at a time. The reader casts its pooled vector to its heads'
        # precision before calling them, so a head widened only while it runs is read as narrow
        # first and hands back narrow answers. The heads are a few thousand weights and are
        # widened for the whole call.
        trunk = getattr(model, "trunk", None)
        layered: set[torch.nn.Module] = (
            set(trunk.modules()) if isinstance(trunk, torch.nn.Module) else set()
        )
        whole = [module for module in model.modules() if module not in layered]
        hooks: list[RemovableHandle] = [
            hook
            for module in layered
            if any(True for _ in module.parameters(recurse=False))
            for hook in (
                module.register_forward_pre_hook(self.widen),
                module.register_forward_hook(self.narrow),
            )
        ]
        return hooks, whole


def load_graph(path: Path, *, weights: bool) -> ModelProto:
    """Read an ONNX graph, with its external weights loaded only when `weights` is set."""
    import onnx  # noqa: PLC0415 heavy optional dependency, loaded only when a graph is read

    # onnx annotates its path parameter as a bare PathLike.
    return onnx.load(str(path), load_external_data=weights)  # pyright: ignore[reportUnknownMemberType]


def save_graph(model: ModelProto, path: Path) -> None:
    """Write an ONNX graph to `path`."""
    import onnx  # noqa: PLC0415 heavy optional dependency, loaded only when a graph is written

    # onnx annotates its path parameter as a bare PathLike.
    onnx.save(model, str(path))  # pyright: ignore[reportUnknownMemberType]


@dataclass(frozen=True)
class GraphOptions:
    """How the graph is written: the precision, where the weights go and the operator set."""

    fp16: bool
    external_data: bool
    opset: int


def build_graph(
    run: Path,
    record: Record,
    input_ids: torch.Tensor,
    attention_mask: torch.Tensor,
    options: GraphOptions,
) -> npt.NDArray[np.float32]:
    """Write `run`'s graph as `model.onnx` and return the answers it has to agree with."""
    fp16 = options.fp16
    external_data = options.external_data
    opset = options.opset
    model = load_reader(run, record)
    # The reference answers come from the full-precision model whatever is exported, so a
    # half-precision graph is checked against the thing it is meant to approximate rather
    # than against itself.
    wanted = full_precision_answers(model, input_ids, attention_mask)

    # Converted in place, a weight at a time: the reference answers are already taken, and
    # building a second copy to convert puts two models in memory at once.
    exported = InFullPrecisionOut(model.half()).eval() if fp16 else model.float()

    # Traced on a handful rather than on the whole check batch. The batch axis is dynamic, so
    # the graph is the same either way, and tracing a 560M model on 256 sequences at once
    # crashes the exporter outright rather than reporting anything.
    graph = run / "model.onnx"
    # Traced into a directory of its own: over 2 GB the tracer writes every weight as a file
    # beside the graph, 252 of them for the 4B, and they are the export's scratch, not the
    # reader.
    traced = run / "traced"
    shutil.rmtree(traced, ignore_errors=True)
    traced.mkdir()
    trace_graph(
        exported,
        input_ids[:TRACE_CLAIMS],
        attention_mask[:TRACE_CLAIMS],
        traced / "model.onnx",
        opset,
    )
    del exported, model

    # One file, not a graph plus a weights blob beside it. What ships is verified by checksum
    # before it is run, and a checksum over one of two files is a checksum over nothing. Only a
    # reader too large for one protobuf is two files (--external-data), and then both are its.
    for stray in graph.parent.glob("model.onnx.data*"):
        stray.unlink()
    if external_data:
        gather_weights(traced, graph)
    else:
        save_graph(load_graph(traced / "model.onnx", weights=True), graph)
    shutil.rmtree(traced)
    return wanted


def most_claims_at_once(graph: Path) -> int | None:
    """Return the most claims the graph can be handed at once on DirectML, or None for no limit.

    DirectML multiplies a batch by each weight as one tensor of batch by rows by columns, and
    refuses one of 2^32 elements or more. Measured on the 4B, whose MLP weights are 2560 by
    9728: 172 claims read and 173 failed, at 8 tokens as at 128, so the limit is the weight's
    and the batch's alone.
    """
    model = load_graph(graph, weights=False)
    weight_sizes = {
        tensor.name: int(np.prod(tensor.dims))
        for tensor in model.graph.initializer
        if len(tensor.dims) == MATRIX_DIMS
    }
    # A weight may reach its product through a transpose rather than stored transposed.
    for node in model.graph.node:
        if node.op_type == "Transpose" and node.input[0] in weight_sizes:
            weight_sizes[node.output[0]] = weight_sizes[node.input[0]]
    largest = max(
        (
            weight_sizes.get(node.input[1], 0)
            for node in model.graph.node
            if node.op_type == "MatMul" and len(node.input) > 1
        ),
        default=0,
    )
    return (DIRECTML_ELEMENTS - 1) // largest if largest else None


# Where each weight starts in the one weights file. ONNX Runtime maps a weight straight from the
# file only when it starts on the system's allocation granularity, which on Windows is 64 KiB,
# and otherwise reads a copy of it into memory, so a reader laid out any tighter holds its
# weights twice while a session opens.
WEIGHT_ALIGNMENT = 64 * 1024


def _tensors(graph: GraphProto) -> Iterator[TensorProto]:
    yield from graph.initializer
    for node in graph.node:
        for attribute in node.attribute:
            if attribute.HasField("t"):
                yield attribute.t
            yield from attribute.tensors
            if attribute.HasField("g"):
                yield from _tensors(attribute.g)
            for inner in attribute.graphs:
                yield from _tensors(inner)


def gather_weights(traced: Path, graph: Path) -> None:
    """Write a traced graph as `graph` and one `model.onnx.data` beside it, a weight at a time.

    Over 2 GB the tracer leaves every weight in a file of its own. Loading them all to save them
    again as one held the 4B's eight gigabytes in memory a second time, beside the model. Copied
    across one by one, the weights never need to be in memory at all.
    """
    import onnx  # noqa: PLC0415 heavy optional dependency, loaded only when a graph is written
    from onnx.external_data_helper import ExternalDataInfo  # noqa: PLC0415 same dependency

    model = load_graph(traced / "model.onnx", weights=False)
    at = 0
    with (graph.parent / "model.onnx.data").open("wb") as out:
        for tensor in _tensors(model.graph):
            if tensor.data_location != onnx.TensorProto.EXTERNAL:
                continue
            info = ExternalDataInfo(tensor)
            at += -at % WEIGHT_ALIGNMENT
            out.seek(at)
            with (traced / info.location).open("rb") as source:
                source.seek(info.offset or 0)
                length = info.length or os.fstat(source.fileno()).st_size - (info.offset or 0)
                left = length
                while left:
                    chunk = source.read(min(left, COPY_CHUNK))
                    if not chunk:
                        msg = f"{info.location} ends {left} bytes short"
                        raise SystemExit(msg)
                    out.write(chunk)
                    left -= len(chunk)
            # By hand: onnx's own setter wants the weight in memory, which is what this avoids.
            del tensor.external_data[:]
            for key, value in (("location", "model.onnx.data"), ("offset", at), ("length", length)):
                entry = tensor.external_data.add()
                entry.key, entry.value = key, str(value)
            at += length
    save_graph(model, graph)


def rule_fingerprint(
    threshold: float,
    subjects: Sequence[str],
    lines: Sequence[float | None] | None,
    by_language: dict[str, float | None] | None,
) -> str:
    """Hash the rule the reader abstains by, recorded beside the weights.

    Two readers can share a training run, a set of weights and a label set and still answer
    differently, because the lines they abstain at are drawn separately and can be redrawn
    without retraining anything. `run_id` names the weights and says nothing about the rule, so
    a reading made under one rule and a reading made under another both claim the same reader
    and nothing reconciles the numbers. This is what tells them apart.
    """
    digest = hashlib.sha256()
    digest.update(f"{threshold:.6f}\n".encode())
    for name, line in zip(subjects, lines or [], strict=False):
        digest.update(f"subject/{name}/{'' if line is None else f'{line:.6f}'}\n".encode())
    languages = by_language or {}
    for name in sorted(languages):
        language_line = languages[name]
        digest.update(
            f"language/{name}/{'' if language_line is None else f'{language_line:.6f}'}\n".encode()
        )
    return digest.hexdigest()[:16]


def wilson_note(at_threshold: Record) -> str:
    """Return the 95% interval of an accuracy, as the Rust side prints it beside every rate.

    The card and the page share the interval so they cannot disagree about how sure a number is.
    """
    answered = at_threshold.get("answered", 0)
    accuracy = at_threshold.get("accuracy")
    if not answered or accuracy is None:
        return ""
    z = Z_95
    n = float(answered)
    denominator = 1.0 + z * z / n
    centre = accuracy + z * z / (2.0 * n)
    spread = z * ((accuracy * (1.0 - accuracy) / n + z * z / (4.0 * n * n)) ** 0.5)
    low = max(0.0, (centre - spread) / denominator)
    high = min(1.0, (centre + spread) / denominator)
    return f" (95% interval {low:.3f} to {high:.3f}, {answered:,} claims)"


# The licence each base encoder a reader is fine-tuned from carries. A card that names the wrong
# one tells somebody they may do something they may not, or the reverse.
BASE_LICENCES = {
    "intfloat/multilingual-e5-large-instruct": "MIT",
    "intfloat/multilingual-e5-small": "MIT",
    "intfloat/multilingual-e5-base": "MIT",
    "Qwen/Qwen3-Embedding-4B": "Apache-2.0",
}


def front_matter(fields: Record) -> list[str]:
    """Return the YAML block a Hugging Face card opens with, which the hub lists and filters by."""
    out = ["---"]
    for key, value in fields.items():
        if isinstance(value, list):
            out += [f"{key}:", *(f"- {one}" for one in cast("list[Any]", value))]
        else:
            out.append(f"{key}: {value}")
    return [*out, "---", ""]


def model_card(run: Path, metadata: Record | None = None, title: str = READER_NAME) -> str:
    """Return the card for an exported run, from what the export left beside its graph.

    Built from `run.json`, `reader.json` and the sizes record rather than from the export's
    working state, so a card can be written again when the figures beside it move without
    exporting the graph again, which would not give the same bytes.
    """
    record = json.loads((run / "run.json").read_text(encoding="utf-8"))
    reader = json.loads((run / "reader.json").read_text(encoding="utf-8"))
    subjects = reader["subjects"]
    threshold = reader["threshold"]
    by_language: dict[str, float | None] = reader.get("language_thresholds") or {}
    metrics = record["validation"]
    # The card quotes the held-out games, the ones the run never saw. The validation games chose
    # its settings and score higher, which is not what a new game gets.
    held_out = record.get("test", metrics)
    at_threshold = held_out.get(
        "at_validation_threshold",
        {
            "coverage": metrics.get("threshold_coverage", 0),
            "accuracy": metrics.get("threshold_accuracy"),
        },
    )
    weakest = sorted(held_out["per_subject"].items(), key=lambda pair: pair[1]["f1"])[:5]
    silent = sorted(name for name, line in by_language.items() if line is None)
    base = record["backbone"]
    base_licence = BASE_LICENCES.get(base)
    marked = reader.get("headset_marker")
    return "\n".join(
        [
            *(front_matter(metadata) if metadata else []),
            f"# {title}",
            "",
            f"Classifies one claim from a Steam review: which of {len(subjects)} subjects it is",
            "about, and whether it is praise, a complaint or neutral. Fine-tuned from",
            f"`{base}`, run `{run.name}`. Part of",
            "[SteamGauge](https://github.com/Aureliolo/steamgauge).",
            "",
            "## Use",
            "",
            "- `model.onnx` takes `input_ids` and `attention_mask` for a pair tokenised with",
            (
                "  `tokenizer.json`: the claim, then the text of its review around it, at most "
                f"{reader.get('max_tokens', DEFAULT_MAX_TOKENS)} tokens."
            ),
            *(
                [
                    "  For a game played only in a VR headset, the review text starts with",
                    '  "Played in a VR headset."',
                ]
                if marked
                else []
            ),
            f"- It returns `subject_logits` ({len(subjects)}, in the order of `subjects` in",
            "  `reader.json`), `polarity_logits` (praise, complaint, neutral) and `pooled`.",
            "- `reader.json` holds the confidence each subject (`thresholds`) and each language",
            "  (`language_thresholds`) needs before the reader answers. A claim gets an answer",
            "  when its top subject's probability clears both thresholds. A language with no",
            "  threshold, or missing from the list, gets no answers.",
            "",
            "Subjects: " + ", ".join(f"`{name}`" for name in subjects) + ".",
            "",
            "## Results",
            "",
            f"On {record['claims']['test']:,} claims from games held out of training:",
            "",
            (
                f"- Subject accuracy {held_out.get('accuracy', 0):.3f}, macro F1 "
                f"{held_out.get('macro_f1', 0):.3f}."
            ),
            f"- Polarity macro F1 {held_out.get('polarity_macro_f1', 0):.3f}.",
            (
                f"- Calibration error {held_out.get('calibration_error', 0):.3f}; area under the "
                f"risk-coverage curve {held_out.get('aurc', 0):.3f}."
            ),
            *(
                [
                    (
                        f"- With its thresholds, out of fold over {shipped['claims']:,} claims of "
                        f"{shipped['games']} games: answers {shipped['coverage']:.0%} of claims, "
                        f"accuracy {shipped['accuracy']:.3f}, macro F1 {shipped['macro_f1']:.3f}."
                    )
                ]
                if (shipped := reader.get("held_out"))
                else [
                    f"- With one confidence threshold at {threshold:.2f}: answers "
                    f"{at_threshold.get('coverage', 0):.0%} of claims, accuracy "
                    f"{at_threshold.get('accuracy') or 0:.3f}" + wilson_note(at_threshold) + "."
                ]
            ),
            "- Lowest per-subject F1: "
            + ", ".join(f"`{name}` {row['f1']:.2f}" for name, row in weakest)
            + ".",
            *(["- No answers in: " + ", ".join(silent) + "."] if silent else []),
            "",
            # Every size's card carries every size. The same table as docs/accuracy.md's.
            *(["## Sizes", "", sizes.table(sizes.load()), ""] if sizes.SIZES.is_file() else []),
            "## Training",
            "",
            f"{record['claims']['train']:,} labelled claims, "
            f"with {record['claims']['validation']:,} more for validation"
            + (
                f", from [`{dataset}`](https://huggingface.co/datasets/{dataset})"
                if (dataset := next(iter((metadata or {}).get("datasets", [])), None))
                else ""
            )
            + (
                f", and a larger reader's answers on {pooled:,} unlabelled claims from the same"
                " games' reviews, which are not published"
                if (pooled := record.get("pool_claims"))
                else ""
            )
            + f". SteamGauge commit `{record['git_sha'][:12]}`.",
            "",
            "## Limits",
            "",
            "The training labels were written by Claude models from a category sheet, and the",
            "results above measure agreement with those labels.",
            "",
            "## Licence",
            "",
            "Apache-2.0." + (f" Base model `{base}`: {base_licence}." if base_licence else ""),
            "",
        ]
    )


def sample_claims(
    path: Path, count: int, record: Record, tokenizer: PreTrainedTokenizerBase
) -> list[Any]:
    """Return claims to check the exported graph on, in the form the reader will send them.

    A model trained on the claim inside its review is asked about pairs for the rest of its
    life, and a parity check on bare claims measures drift on input it will never see. The
    sequences are also four times shorter, which is exactly where a half-precision graph is
    least likely to drift.
    """
    claims = claimdata.load(path)
    step = max(1, len(claims) // count)
    drawn = claims[::step][:count]

    cut = Claims(
        drawn,
        tokenizer,
        record["subjects"],
        Reading.recorded(record, record["max_length"]),
    )
    pairs = [cut.pair(at) for at in range(len(drawn))]
    return [one[0] if len(one) == 1 else one for one in pairs]


def crossed_the_line(
    wanted: npt.NDArray[np.float32],
    got: npt.NDArray[np.float32],
    lines: Sequence[float | None],
    allowed: float,
) -> None:
    """Refuse a graph that answers claims the model would decline, or the reverse.

    The parity check above asks whether the graph picks the same subject. What ships is not the
    subject alone: it is the subject and whether the reader speaks at all, and that second half
    is decided by a confidence against a line. Two graphs can agree on every claim and still
    disagree about which of them they are willing to stand behind, and then a coverage figure
    moves for a reason nobody recorded.

    Only claims the two graphs agree the subject of. Where they disagree, a different line
    applies, and these lines run from 0.18 to 0.99: a claim that reorders a tie between two
    subjects changes side because it changed subject, which the tie count above already
    measures, not because a confidence drifted anywhere.

    Counted like the ties above: a claim sitting within the drift of its own line would cross it
    on any rounding at all, and holding the graph to that is holding it to a distance it cannot
    resolve.
    """

    def speaks(
        scores: npt.NDArray[np.float32],
    ) -> tuple[npt.NDArray[np.intp], npt.NDArray[np.bool_], npt.NDArray[np.float64]]:
        shifted = scores - scores.max(axis=1, keepdims=True)
        probability = np.exp(shifted) / np.exp(shifted).sum(axis=1, keepdims=True)
        best = probability.argmax(axis=1)
        at = np.array([np.inf if lines[b] is None else lines[b] for b in best])
        confidence = probability.max(axis=1)
        return best, confidence >= at, np.abs(confidence - at)

    chose, was, room = speaks(wanted)
    also, now, _ = speaks(got)
    agreed = chose == also
    moved = (was != now) & agreed
    decided = int((moved & (room > allowed)).sum())
    near = int((moved & (room <= allowed)).sum())
    print(
        f"           {decided + near} of {int(agreed.sum())} claims change side, "
        f"{near} of them sitting on the line"
    )
    if decided:
        msg = (
            f"{decided} claims are answered by one graph and declined by the other, each from "
            f"further than {allowed:.2e} off its line. The lines were drawn against the model "
            f"and are shipped against the graph. Not shipping this."
        )
        raise SystemExit(msg)
    # Held to the same bulk rule as the ties: one claim on its line may fall either way, and a
    # twentieth of them falling means the graph is not answering where the model would.
    if near > int(agreed.sum()) // BULK_FRACTION:
        msg = (
            f"{near} of {int(agreed.sum())} claims change side, every one within {allowed:.2e} "
            f"of its line, but a graph this unsteady about when to speak is not the model. Not "
            f"shipping this."
        )
        raise SystemExit(msg)


def aspect_lines(oof: str, subjects: list[str], min_precision: float) -> list[float | None] | None:
    """Return, for each subject, how sure the aspect head must be before it says it is covered.

    The lowest line at which the subject is called with at least `min_precision` among the
    held-out claims whose labels answer it, drawn from every fold at once as the subject lines
    are. None where no line reaches it: the reader then never names that subject beside
    another, and the subject head still names it first. None as a whole when the folds were
    trained without the head.

    Fitted only where the reader applies it: beside the subject the subject head names first,
    which the reader never names again. Fitted over every claim, a subject's line was held up by
    the claims chiefly about it, which the head calls covered almost without fail, and promised
    three in four of its calls beside another subject while keeping fewer than one in two:
    measured on the folds of the first reader with the head, 0.475 where 0.75 was promised, and
    a second subject named on three claims in four of a real corpus.

    The folds carry a class order of their own, remapped onto this export's as the subject
    lines are.
    """
    parts = [np.load(path, allow_pickle=False) for path in sorted(Path(oof).glob("cv-*.npz"))]
    if not parts or any("aspect_logits" not in part for part in parts):
        return None
    theirs = [str(name) for name in parts[0]["subjects"]]
    if any([str(name) for name in part["subjects"]] != theirs for part in parts):
        msg = "the folds do not list their subjects in one order"
        raise SystemExit(msg)
    if sorted(theirs) != sorted(subjects):
        msg = "the folds and this run do not know the same subjects"
        raise SystemExit(msg)
    logits = np.concatenate([part["aspect_logits"] for part in parts])
    truth = np.concatenate([part["aspect_truth"] for part in parts]) > 0
    known = np.concatenate([part["aspect_known"] for part in parts])
    first = np.concatenate([part["logits"] for part in parts]).argmax(axis=1)
    beside = known.copy()
    beside[np.arange(len(first)), first] = False
    shifted = np.exp(logits - logits.max(axis=-1, keepdims=True))
    covered = 1.0 - shifted[..., 0] / shifted.sum(axis=-1)
    lines: list[float | None] = []
    for name in subjects:
        at = theirs.index(name)
        sure, said = covered[beside[:, at], at], truth[beside[:, at], at]
        order = np.argsort(-sure)
        hits = np.cumsum(said[order])
        precision = hits / np.arange(1, len(order) + 1)
        # The longest prefix of the ranking that keeps the promise, so the line is the lowest
        # confidence that still does, and a subject nobody labels beside another gets none.
        keeps = np.nonzero((precision >= min_precision) & said[order])[0]
        lines.append(float(sure[order][keeps[-1]]) if len(keeps) else None)
    return lines


def abstention_lines(
    oof: str, data: str, subjects: list[str], min_accuracy: float, min_claims: int
) -> tuple[list[float | None], dict[str, float | None], Record]:
    """Return one abstention threshold per subject and one per language, from the same folds.

    A claim is answered only when it clears both. The subject line holds the promise for each
    subject's own predictions and the language line holds it for each language's, and neither
    carries the other: 71% of the set is English, so a line drawn per subject is a line drawn
    mostly from English claims and every other language is marked against it.

    Fitted on every game but the held-out ones at once rather than leave-one-game-out, because
    this is the threshold that ships and it should see every claim there is. What a line is
    *worth* is a different question, and `confidence.py` answers it out-of-fold. Fitting the
    shipped one the same way would throw away a fifth of the evidence for no gain.

    The folds carry a class order of their own, so the lines are remapped onto the order this
    export writes rather than trusted to match: two lists of twenty-six subjects that differ by
    one position would silence the wrong subject and nothing would look wrong.
    """
    # confidence opens an ONNX Runtime session module-wide, so it loads only when lines are fitted.
    from confidence import most_coverage, pooled_folds, softmax  # noqa: PLC0415

    logits, truth, app_ids, theirs = pooled_folds(Path(oof).glob("*.npz"))
    if sorted(theirs) != sorted(subjects):
        msg = "the folds and this run do not know the same subjects"
        raise SystemExit(msg)

    probabilities = softmax(logits)
    predicted = probabilities.argmax(axis=1)
    confidence = probabilities.max(axis=1)
    correct = (predicted == truth).astype(float)

    lines: list[float | None] = []
    for name in subjects:
        mine = predicted == theirs.index(name)
        line = most_coverage(confidence[mine], correct[mine], min_accuracy) if mine.any() else None
        lines.append(None if line is None else round(float(line), 4))

    spoken = languages_of(Path(oof).glob("*.npz"), data)
    by_language: dict[str, float | None] = {}
    for name in sorted(set(spoken.tolist())):
        if not name:
            continue
        mine = spoken == name
        # A language with too little labelled evidence declines rather than borrowing the line
        # its neighbours were given. Pooling is right on the subject axis, where the rare
        # subjects share a reader. Here it produced Indonesian answering three quarters of its
        # eight claims at a third right, under a line fitted almost entirely on English.
        if int(mine.sum()) < min_claims:
            by_language[name] = None
            continue
        line = most_coverage(confidence[mine], correct[mine], min_accuracy)
        by_language[name] = None if line is None else round(float(line), 4)

    # What those lines answer and agree on, over the same out-of-fold claims. The run record's
    # own coverage is at one threshold, and a reader that carries it describes a rule it does not
    # apply: every report of an unlabelled game prints these two figures as what its rates are
    # worth, and the warning for a corpus declined far more than usual is measured against them.
    at = np.array([np.inf if line is None else line for line in lines], dtype=float)
    mine = np.array([subjects.index(theirs[int(one)]) for one in predicted])
    # Both bars, because two promises made separately are not one promise made jointly. The
    # per-subject line alone recovers 2.8 of the 11.1 points Korean loses and leaves the rest:
    # a Korean prediction clears a bar drawn from English claims and is then wrong a third of
    # the time, which is the same defect the per-subject line was introduced to fix.
    spoken_at = np.array(
        [np.inf if by_language.get(name) is None else by_language[name] for name in spoken],
        dtype=float,
    )
    answered = (confidence >= at[mine]) & (confidence >= spoken_at)
    carried = {
        "games": len(set(app_ids.tolist())),
        "claims": len(truth),
        "coverage": float(answered.mean()),
        "accuracy": float(correct[answered].mean()) if answered.any() else 0.0,
        "macro_f1": macro_f1(predicted[answered], truth[answered], len(theirs)),
        # Not the held-out games: these are the folds, each scoring games its own training never
        # saw. Whoever reads the figure should know which question it answers.
        "measured_on": "out-of-fold",
    }
    return lines, by_language, carried


def languages_of(paths: Iterable[Path], data: str) -> npt.NDArray[np.str_]:
    """Return each out-of-fold claim's language, joined on the claim the labeller was shown.

    The fold files carry the review and claim index for exactly this: the logits know nothing
    about language and the label set knows nothing about the model.
    """
    parts = [np.load(path, allow_pickle=False) for path in sorted(paths)]
    review_ids = np.concatenate([part["review_id"] for part in parts])
    claim_index = np.concatenate([part["claim_index"] for part in parts])
    labelled = {(claim.review_id, claim.claim_index): claim for claim in claimdata.load(Path(data))}
    beside = [
        labelled.get((str(rid), int(at))) for rid, at in zip(review_ids, claim_index, strict=True)
    ]
    return np.array([one.language if one else "" for one in beside])


def macro_f1(
    predicted: npt.NDArray[np.integer[Any]], truth: npt.NDArray[np.integer[Any]], classes: int
) -> float:
    """Return the unweighted mean F1 over the subjects that appear.

    A rare subject counts as much as a common one.
    """
    scores: list[float] = []
    for one in range(classes):
        hit = ((predicted == one) & (truth == one)).sum()
        said = (predicted == one).sum()
        was = (truth == one).sum()
        if not was:
            continue
        precision = hit / said if said else 0.0
        recall = hit / was
        scores.append(
            0.0 if not (precision + recall) else 2 * precision * recall / (precision + recall)
        )
    return float(np.mean(scores)) if scores else 0.0


def _parse_args() -> argparse.Namespace:
    """Read the command line."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", required=True)
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument("--opset", type=int, default=17)
    parser.add_argument("--check", type=int, default=256)
    parser.add_argument(
        "--categories",
        default="",
        help="which categories these labels were made against; the build's own when empty",
    )
    parser.add_argument(
        "--fp16",
        action="store_true",
        help="export half precision: half the download, and roughly half the arithmetic over "
        "millions of claims. Checked against the full-precision model like any other export.",
    )
    parser.add_argument(
        "--lines-from",
        default=None,
        help="a directory of cross-validation fold logits (`train.py --save-logits`), from "
        "which to draw one abstention threshold per subject and one per language. One threshold "
        "keeps the promise on average and breaks it both subject by subject and language by "
        "language; a claim answers only when it clears both of its lines, and a subject or a "
        "language no threshold can make reliable is declined outright. Without this the reader "
        "carries the one threshold it always has.",
    )
    parser.add_argument(
        "--external-data",
        action="store_true",
        help="keep the weights in model.onnx.data beside the graph instead of inside it. A "
        "protobuf cannot hold more than 2 GB, so a reader above about a billion parameters in "
        "half precision cannot be one file; such a reader is two files, and whatever pins it "
        "has to pin both.",
    )
    parser.add_argument("--min-accuracy", type=float, default=0.75)
    parser.add_argument(
        "--min-language-claims",
        type=int,
        default=100,
        help="a language with fewer out-of-fold claims than this declines every claim rather "
        "than being given a line. Below a hundred the interval on a fitted line is wider than "
        "the promise it is meant to keep, and pooling instead put Indonesian on an English line "
        "and had it answer eight claims at a third right.",
    )
    return parser.parse_args()


def _run_directory(name: str) -> Path:
    """Return the run `name` names, whether it is a path or `runs/<name>` of this project."""
    # `runs/<name>` names a run of this project wherever the command was typed from, so a
    # script driving the whole chain from the repository root does not silently export nothing.
    run = Path(name)
    if not run.is_absolute() and not run.joinpath("run.json").is_file():
        run = HERE / name
    return run


def _encode_claims(
    args: argparse.Namespace, record: Record, tokenizer: PreTrainedTokenizerBase
) -> tuple[list[Any], torch.Tensor, torch.Tensor]:
    """Return the sampled claims with their token ids and attention mask."""
    texts = sample_claims(Path(args.data), args.check, record, tokenizer)
    encoded = tokenizer(
        [pair[0] for pair in texts] if record.get("context") else texts,
        [pair[1] for pair in texts] if record.get("context") else None,
        truncation=True,
        max_length=record["max_length"],
        padding=True,
        return_tensors="pt",
    )
    input_ids = cast("torch.Tensor", encoded["input_ids"])
    attention_mask = cast("torch.Tensor", encoded["attention_mask"])
    return texts, input_ids, attention_mask


def _build_apart(
    run: Path,
    record: Record,
    encoded: tuple[torch.Tensor, torch.Tensor],
    options: GraphOptions,
) -> npt.NDArray[np.float32]:
    """Build the graph in a process of its own and return the reference answers."""
    # Built in a process of its own, which hands back the reference answers and exits. The
    # tracer keeps hold of what it traced: measured on the 4B, the process sat at 24 GB after
    # the model was deleted, and the parity session below, whose weights on the card are charged
    # to the same commit, could not open beside it. A process that has ended holds nothing.
    with ProcessPoolExecutor(1, mp_context=multiprocessing.get_context("spawn")) as apart:
        return apart.submit(build_graph, run, record, encoded[0], encoded[1], options).result()


def _answers_on(
    graph: Path, provider: str, ids: npt.NDArray[np.int64], mask: npt.NDArray[np.int64]
) -> npt.NDArray[np.float32]:
    """Return the graph's subject logits for the claims, run on `provider`."""
    # Loaded after the tracing process has ended, so its memory is not held beside the parity
    # session's.
    import onnxruntime  # noqa: PLC0415 heavy optional dependency, loaded after the build ends

    session = onnxruntime.InferenceSession(str(graph), providers=[provider])
    # ONNX Runtime accepts a provider it does not have and quietly runs somewhere else.
    # That has already cost this project a measurement it believed: a reading reported as
    # CUDA that ran on the processor. Whatever ran the check says so.
    ran_on = session.get_providers()[0]
    print(f"parity asked on {ran_on}")
    if ran_on != provider:
        print(f"  not {provider}, which is where this part of the check belongs")
    # In batches, for the same reason the trace is: the check is over hundreds of sequences
    # and a bigger model has to fit them all on the card at once to answer in one go. Each
    # batch cut to its own longest claim, so the graph is asked at lengths other than the
    # one it was traced at: a graph with the traced length written into it passed this
    # check at that length and refused every other.
    batches: list[npt.NDArray[np.float32]] = []
    for at in range(0, len(ids), CHECK_BATCH):
        width = int(mask[at : at + CHECK_BATCH].sum(axis=1).max())
        batches.append(
            session.run(
                ["subject_logits"],
                {
                    "input_ids": ids[at : at + CHECK_BATCH, :width],
                    "attention_mask": mask[at : at + CHECK_BATCH, :width],
                },
            )[0]
        )
    return np.concatenate(batches).astype(np.float32)


def _check_parity(
    graph: Path,
    wanted: npt.NDArray[np.float32],
    encoded: tuple[torch.Tensor, torch.Tensor],
    *,
    fp16: bool,
) -> npt.NDArray[np.float32]:
    """Refuse a graph that is not the model and return the answers it gives where it runs."""
    ids: npt.NDArray[np.int64] = encoded[0].numpy()
    mask: npt.NDArray[np.int64] = encoded[1].numpy()
    widths = {
        int(mask[at : at + CHECK_BATCH].sum(axis=1).max()) for at in range(0, len(ids), CHECK_BATCH)
    }
    print(f"parity asked at {len(widths)} lengths, traced at {ids.shape[1]}")
    allowed = HALF_TOLERANCE * float(np.abs(wanted).max()) if fp16 else TOLERANCE

    # Whether the graph is the model is asked on the processor, whose kernels keep what the
    # format keeps. See HALF_TOLERANCE for why this is not asked of DirectML.
    faithful = _answers_on(graph, "CPUExecutionProvider", ids, mask)
    drift = float(np.abs(wanted - faithful).max())
    print(f"parity: the graph drifts {drift:.2e} from the model, allowed {allowed:.2e}")
    if drift > allowed:
        msg = (
            f"the exported graph is not the model it came from ({drift:.2e} > {allowed:.2e} "
            f"on the processor's kernels). Not shipping this."
        )
        raise SystemExit(msg)
    # Whether it answers the same where it runs is asked where it runs: a half-precision graph
    # runs on DirectML, the provider the reader opens on Windows and the release ships with.
    # Asking for CUDA checked a provider nothing uses, and on an installation whose runtime is
    # built for a CUDA it does not have, the request failed and fell back to the processor.
    got = _answers_on(graph, "DmlExecutionProvider", ids, mask) if fp16 else faithful
    print(f"parity: largest drift where it runs {float(np.abs(wanted - got).max()):.2e}")
    _check_flips(wanted, got, allowed)
    return got


def _check_flips(
    wanted: npt.NDArray[np.float32], got: npt.NDArray[np.float32], allowed: float
) -> None:
    """Refuse a graph whose answers changed on a decided margin, or too many ties reordered."""
    claims = len(wanted)
    # A changed answer is only a disagreement when the model had an answer to change. Where the
    # best two subjects sit within the drift of each other, the two graphs are not disagreeing
    # about the claim, they are reporting a tie in different arbitrary orders, and forbidding
    # that while permitting the drift that causes it is a contradiction the export cannot
    # satisfy. So a flip is measured against the margin it had to cross.
    changed = wanted.argmax(axis=1) != got.argmax(axis=1)
    ordered = np.sort(wanted, axis=1)
    margin = ordered[:, -1] - ordered[:, -2]
    decided = int((changed & (margin > allowed)).sum())
    ties = int((changed & (margin <= allowed)).sum())

    print(f"parity: over {claims} claims, {decided} answers changed, {ties} ties reordered")
    if decided:
        msg = (
            f"the exported graph answers differently from the model it came from: {decided} "
            f"answers changed on a margin wider than {allowed:.2e}. Not shipping this."
        )
        raise SystemExit(msg)
    # Ties that reorder are tolerable one at a time and not in bulk: a graph that cannot agree
    # with itself on a twentieth of its answers is not approximating the model, whatever the
    # margins say.
    if ties > claims // BULK_FRACTION:
        msg = (
            f"{ties} of {claims} claims came back with a different subject. Every one is "
            f"within the drift, but a graph this unsteady is not the model. Not shipping this."
        )
        raise SystemExit(msg)


def _validated_threshold(record: Record, subjects: Sequence[str]) -> float:
    """Return the run's abstention threshold, refusing one that is not abstention."""
    # A threshold this low is not abstention, it is the nearest-match classifier this project
    # replaced. Twenty-four subjects put a uniform guess at 0.042, so a model told to answer
    # above 0.05 answers everything, and every claim of "unclassified" the tool makes becomes a
    # claim it cannot keep. Refused here rather than discovered in a report.
    threshold = record["validation"].get("threshold", 0.5)
    floor = COIN_FACTOR / len(subjects)
    if threshold < floor:
        msg = (
            f"threshold {threshold:.3f} is below {floor:.3f}, which for {len(subjects)} "
            f"subjects is what a coin lands on. This model does not abstain. Not shipping it."
        )
        raise SystemExit(msg)
    return threshold


@dataclass(frozen=True)
class Abstention:
    """The rule a reader abstains by, as `reader.json` carries it."""

    threshold: float
    lines: list[float | None] | None
    by_language: dict[str, float | None] | None
    carried: Record | None
    aspects_at: list[float | None] | None


def _held_out_at_one_line(record: Record) -> Record | None:
    """Return what the model did on held-out games at the one threshold, if it recorded that."""
    at_one_line = record.get("test", {}).get("at_validation_threshold", {})
    if at_one_line.get("coverage") is None:
        return None
    return {
        "games": len(record.get("games", {}).get("test", [])),
        "claims": record.get("claims", {}).get("test"),
        "coverage": at_one_line.get("coverage"),
        "accuracy": at_one_line.get("accuracy"),
        "macro_f1": record.get("test", {}).get("macro_f1"),
        "measured_on": "held-out games",
    }


def _aspect_thresholds(args: argparse.Namespace, subjects: list[str]) -> list[float | None]:
    """Return the aspect head's line for each subject and say how many can be named."""
    drawn = aspect_lines(args.lines_from, subjects, args.min_accuracy) if args.lines_from else None
    # Without folds there is nothing to draw a line from, and the head is taken at its
    # word: it names a subject it thinks more likely covered than not.
    aspects_at: list[float | None] = [0.5] * len(subjects) if drawn is None else drawn
    nameable = sum(line is not None for line in aspects_at)
    print(f"aspects    {nameable} of {len(subjects)} subjects can be named beside another")
    return aspects_at


def _draw_abstention(
    args: argparse.Namespace, record: Record, subjects: list[str], threshold: float
) -> Abstention:
    """Draw the lines the reader abstains by and gather what it carries about its own record."""
    # What the model declined on games it never saw, so the tool can say when a corpus is
    # declined far more than usual. That is the one number a reader of a new game's report
    # has no other way to get, and a corpus declined at twice the usual rate is a corpus about
    # something the taxonomy lacks.
    lines: list[float | None] | None
    by_language: dict[str, float | None] | None
    carried: Record | None
    lines, by_language, carried = (
        abstention_lines(
            args.lines_from,
            args.data,
            subjects,
            args.min_accuracy,
            args.min_language_claims,
        )
        if args.lines_from
        else (None, None, None)
    )
    if carried is None:
        carried = _held_out_at_one_line(record)
    aspects_at = _aspect_thresholds(args, subjects) if record.get("aspects") else None
    return Abstention(threshold, lines, by_language, carried, aspects_at)


def _report_lines(subjects: list[str], rule: Abstention) -> None:
    """Print how many subjects and languages have a line and which are silent."""
    lines = rule.lines
    by_language = rule.by_language
    if lines is None or by_language is None:
        return
    silent = [name for name, line in zip(subjects, lines, strict=True) if line is None]
    drawn = sum(1 for line in lines if line is not None)
    print(f"lines      {drawn} of {len(subjects)} subjects have one")
    if silent:
        print(f"           silent: {', '.join(silent)}")
    quiet = sorted(name for name, line in by_language.items() if line is None)
    spoke = sum(1 for line in by_language.values() if line is not None)
    print(f"languages  {spoke} of {len(by_language)} have one")
    if quiet:
        print(f"           silent: {', '.join(quiet)}")


def _write_reader(
    run: Path, record: Record, categories: str, graph: Path, rule: Abstention
) -> None:
    """Write `reader.json`, what the Rust side needs to use the graph."""
    subjects = record["subjects"]
    usual_declined = 1.0 - rule.carried["coverage"] if rule.carried else None
    (run / "reader.json").write_text(
        json.dumps(
            {
                "categories": categories,
                "subjects": subjects,
                "threshold": rule.threshold,
                "thresholds": rule.lines,
                # A language absent from this map has too little labelled evidence to promise
                # anything and declines. Absent as a whole on a reader exported before the
                # language axis existed, and then only the subject lines govern.
                "language_thresholds": rule.by_language,
                "max_tokens": record["max_length"],
                "max_batch": most_claims_at_once(graph),
                "context": record.get("context", False),
                "mark": record.get("mark", False),
                "prefix": record.get("prefix", False),
                "headset_marker": record.get("headset_marker", False),
                # How sure the aspect head must be that a claim covers each subject before the
                # reader names it beside the first. Absent on a reader without the head, which
                # names one subject per claim.
                **({"aspect_thresholds": rule.aspects_at} if record.get("aspects") else {}),
                "trained_from": record["backbone"],
                "data_fingerprint": record["data_fingerprint"],
                # The weights and the rule are separate identities. Redrawing the lines without
                # retraining gives a reader that answers differently under the same `run_id`,
                # and a reading that cannot say which rule made it cannot be reconciled with
                # one made under the other.
                "lines_fingerprint": rule_fingerprint(
                    rule.threshold, subjects, rule.lines, rule.by_language
                ),
                "name": READER_NAME,
                "run_id": run.name,
                "usual_declined": usual_declined,
                # What this model did on games it never saw, carried so that a report of a
                # game with no reference set can still say what its rates are worth. Without
                # it, the only honest thing such a report can say is nothing. Under the rule
                # that ships: with lines, the folds, and without them, the held-out games at the one
                # threshold, which is the same rule the reader then applies.
                "held_out": rule.carried,
            },
            indent=2,
        ),
        encoding="utf-8",
    )


def main() -> None:
    """Export a run to ONNX, check the graph against the model and write the reader's files."""
    args = _parse_args()
    run = _run_directory(args.run)
    record = json.loads((run / "run.json").read_text(encoding="utf-8"))
    subjects = record["subjects"]

    tokenizer = backbones.load(AutoTokenizer, run / "tokenizer")
    _texts, input_ids, attention_mask = _encode_claims(args, record, tokenizer)
    encoded = (input_ids, attention_mask)

    graph = run / "model.onnx"
    options = GraphOptions(fp16=args.fp16, external_data=args.external_data, opset=args.opset)
    wanted = _build_apart(run, record, encoded, options)
    got = _check_parity(graph, wanted, encoded, fp16=args.fp16)

    # What the Rust side needs to use the graph without being told anything else. The
    # taxonomy version is in here so a model trained against other categories is refused rather
    # than quietly asked about categories nobody labelled.
    threshold = _validated_threshold(record, subjects)
    rule = _draw_abstention(args, record, subjects, threshold)
    _report_lines(subjects, rule)
    if rule.lines is not None and rule.by_language is not None:
        crossed_the_line(wanted, got, rule.lines, HALF_LINE_ROOM if args.fp16 else TOLERANCE)
    _write_reader(run, record, args.categories, graph, rule)

    # The reader wants three files beside each other, and so does publishing. The tokenizer
    # directory the trainer saved holds more than the one file the tool reads.
    #
    # Padding and truncation are stripped on the way out. A tokenizer file saved from a run
    # that padded to 128 carries that instruction, so anything that loads it to ask a question
    # other than "encode this pair" gets 128 entries whatever the text says, the tail of them
    # padding at offset (0, 0). That has already cost this project a reader that built its
    # window out of padding and answered anyway. What a batch needs is the caller's business,
    # not the file's.
    shipped = json.loads((run / "tokenizer" / "tokenizer.json").read_text(encoding="utf-8"))
    shipped["padding"] = None
    shipped["truncation"] = None
    (run / "tokenizer.json").write_text(json.dumps(shipped, ensure_ascii=False), encoding="utf-8")

    card = run / "MODEL_CARD.md"
    card.write_text(model_card(run), encoding="utf-8")
    print(f"wrote {graph} and {card}")


if __name__ == "__main__":
    main()
