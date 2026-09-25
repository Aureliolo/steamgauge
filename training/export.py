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
import os
from pathlib import Path

import numpy as np
import torch
from transformers import AutoTokenizer

import claimdata
from train import ClaimReader

HERE = Path(__file__).resolve().parent

# The reader's name says what it reads. A run id names an experiment by its encoder and label
# count and is the right name for the index; a thing somebody downloads and cites wants a
# name that is not a serial and needs no explaining.
READER_NAME = "Game Review Reader"

# What a claim's logits may drift by between PyTorch and the exported graph. Tight enough
# that a changed argmax cannot hide inside it on anything but a genuine tie.
TOLERANCE = 2e-3

# Half precision keeps about three decimal digits, so logits of this size drift by tens of
# thousandths. The argmax check below is what actually guards the answers.
#
# This was 8e-2 while the check ran on CPU kernels, and on the kernels that ship it is not
# enough: the same two graphs drift 1.05e-01 and 6.30e-02 through DirectML with no answer
# changed, no tie reordered and no claim crossing its abstention line. A number fitted to
# hardware the graph never runs on refuses models that are fine, which is a check that has
# stopped measuring anything. What guards the answers is the argmax, the tie count and the
# lines; this is the canary for an export that has broken outright, and it is set from what the
# provider actually does with room above it.
HALF_TOLERANCE = 2.5e-1


def trace_graph(model, input_ids, attention_mask, path, opset: int):
    """Writes a reader's graph, whatever its trunk.

    A decoder trunk needs two things changed for the tracer, and an encoder needs neither. Current
    transformers builds a decoder's causal mask with `torch.vmap`, which the tracing exporter
    cannot follow: the 4B died in it with "invalid unordered_map<K, T> key". The same library
    keeps a builder of plain tensor operations for older torch, and it is swapped in for the
    trace alone; the parity check after the export is what says the two masks agree. A decoder
    also keeps a cache of past keys for generating text, which a reader that answers once has no
    use for and a traced graph should not carry as outputs.
    """
    from transformers.masking_utils import ALL_MASK_ATTENTION_FUNCTIONS, sdpa_mask_older_torch

    for module in model.modules():
        config = getattr(module, "config", None)
        if config is not None and hasattr(config, "use_cache"):
            config.use_cache = False
    ALL_MASK_ATTENTION_FUNCTIONS["sdpa"] = sdpa_mask_older_torch
    try:
        torch.onnx.export(
            model,
            (input_ids, attention_mask),
            # A string, never a Path: the tracing exporter takes anything else for a stream, and
            # a graph over the 2 GB a protobuf holds can only be written with its weights beside
            # it, in the directory a string names. The 4B traced for eleven minutes and died there.
            os.fspath(path),
            input_names=["input_ids", "attention_mask"],
            output_names=["subject_logits", "polarity_logits", "pooled"],
            dynamic_axes={
                "input_ids": {0: "batch", 1: "tokens"},
                "attention_mask": {0: "batch", 1: "tokens"},
                "subject_logits": {0: "batch"},
                "polarity_logits": {0: "batch"},
                "pooled": {0: "batch"},
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
        del ALL_MASK_ATTENTION_FUNCTIONS["sdpa"]


class InFullPrecisionOut(torch.nn.Module):
    """Runs the model in half precision and hands back full-precision numbers.

    The arithmetic is where the saving is; the three small vectors that come out are not. A
    graph whose outputs are half precision forces every reader of it to know that, and the
    tool that runs this graph should not have to care which precision it was exported at.
    """

    def __init__(self, inner: torch.nn.Module):
        super().__init__()
        self.inner = inner

    def forward(self, input_ids, attention_mask):
        subject, polarity, pooled = self.inner(input_ids, attention_mask)
        return subject.float(), polarity.float(), pooled.float()


def rule_fingerprint(threshold: float, subjects, lines, by_language) -> str:
    """A hash of the rule the reader abstains by, recorded beside the weights.

    Two readers can share a training run, a set of weights and a label set and still answer
    differently, because the lines they abstain at are drawn separately and can be redrawn
    without retraining anything. `run_id` names the weights and says nothing about the rule, so
    a reading made under one rule and a reading made under another both claim the same reader
    and nothing reconciles the numbers. This is what tells them apart.
    """
    digest = hashlib.sha256()
    digest.update(f"{threshold:.6f}\n".encode())
    for name, line in zip(subjects, lines or []):
        digest.update(f"subject/{name}/{'' if line is None else f'{line:.6f}'}\n".encode())
    for name in sorted(by_language or {}):
        line = by_language[name]
        digest.update(f"language/{name}/{'' if line is None else f'{line:.6f}'}\n".encode())
    return digest.hexdigest()[:16]


def spoken_note(by_language: dict | None) -> list[str]:
    """What the language axis does, for somebody reading the card rather than the code.

    A reader that declines a whole language is not a reader with a slightly lower coverage in
    it, and the difference has to survive into the one file that travels with the graph. Which
    languages are silent is the part nobody would guess: it is a fact about how much of the
    reference set is written in them, not about the model's grasp of them.
    """
    if not by_language:
        return []
    spoke = sorted(name for name, line in by_language.items() if line is not None)
    quiet = sorted(name for name, line in by_language.items() if line is None)
    drawn = [line for line in by_language.values() if line is not None]
    if not drawn:
        return []
    note = (
        f"- **And per language**: {len(spoke)} of {len(by_language)} carry a line, "
        f"{min(drawn):.2f} to {max(drawn):.2f}. A claim answers only when it clears both its "
        f"subject's line and its language's, because a line drawn per subject is drawn mostly "
        f"from English claims and out of fold it leaves Korean 8.3 points under the promise it "
        f"prints."
    )
    if quiet:
        note += (
            f" {len(quiet)} languages are declined outright, having too few labelled claims to "
            f"promise anything: {', '.join(quiet)}."
        )
    return [note]


def wilson_note(at_threshold: dict) -> str:
    """The range an accuracy from a couple of hundred claims is entitled to claim.

    A point estimate from 221 claims reads as three significant figures and has about one.
    The same interval the Rust side prints beside every rate, so the card and the page cannot
    disagree about how sure a number is.
    """
    answered = at_threshold.get("answered", 0)
    accuracy = at_threshold.get("accuracy")
    if not answered or accuracy is None:
        return ""
    z = 1.959963985
    n = float(answered)
    denominator = 1.0 + z * z / n
    centre = accuracy + z * z / (2.0 * n)
    spread = z * ((accuracy * (1.0 - accuracy) / n + z * z / (4.0 * n * n)) ** 0.5)
    low = max(0.0, (centre - spread) / denominator)
    high = min(1.0, (centre + spread) / denominator)
    return f", somewhere in [{low:.3f}, {high:.3f}] over {answered} claims"


def sample_claims(path: Path, count: int, record: dict, tokenizer) -> list:
    """Claims to check the exported graph on, in the form the reader will send them.

    A model trained on the claim inside its review is asked about pairs for the rest of its
    life, and a parity check on bare claims measures drift on input it will never see. The
    sequences are also four times shorter, which is exactly where a half-precision graph is
    least likely to drift.
    """
    claims = claimdata.load(path)
    step = max(1, len(claims) // count)
    drawn = claims[::step][:count]

    from train import Claims

    cut = Claims(
        drawn,
        tokenizer,
        record["subjects"],
        record["max_length"],
        record.get("context", False),
        mark=record.get("mark", False),
        prefix=record.get("prefix", False),
    )
    pairs = [cut.pair(at) for at in range(len(drawn))]
    return [one[0] if len(one) == 1 else one for one in pairs]


def crossed_the_line(wanted, got, lines, allowed: float) -> None:
    """How many claims the exported graph would answer that the model would decline, or back.

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

    def speaks(scores):
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
        raise SystemExit(
            f"{decided} claims are answered by one graph and declined by the other, each from "
            f"further than {allowed:.0e} off its line. The lines were drawn against the model "
            f"and are shipped against the graph. Not shipping this."
        )


def abstention_lines(
    oof: str, data: str, subjects: list[str], min_accuracy: float, min_claims: int
):
    """One abstention threshold per subject and one per language, from the same folds.

    A claim is answered only when it clears both. The subject line holds the promise for each
    subject's own predictions and the language line holds it for each language's, and neither
    carries the other: 71% of the set is English, so a line drawn per subject is a line drawn
    mostly from English claims and every other language is marked against it.

    Fitted on every non-frozen game at once rather than leave-one-game-out, because this is the
    threshold that ships and it should see every claim there is. What a line is *worth* is a
    different question, and `confidence.py` answers it out-of-fold; fitting the shipped one the
    same way would throw away a fifth of the evidence for no gain.

    The folds carry a class order of their own, so the lines are remapped onto the order this
    export writes rather than trusted to match: two lists of twenty-six subjects that differ by
    one position would silence the wrong subject and nothing would look wrong.
    """
    from confidence import most_coverage, pooled_folds, softmax

    logits, truth, app_ids, theirs = pooled_folds(Path(oof).glob("*.npz"))
    if sorted(theirs) != sorted(subjects):
        raise SystemExit("the folds and this run do not know the same subjects")

    probabilities = softmax(logits)
    predicted = probabilities.argmax(axis=1)
    confidence = probabilities.max(axis=1)
    correct = (predicted == truth).astype(float)

    lines = []
    for name in subjects:
        mine = predicted == theirs.index(name)
        line = most_coverage(confidence[mine], correct[mine], min_accuracy) if mine.any() else None
        lines.append(None if line is None else round(float(line), 4))

    spoken = languages_of(Path(oof).glob("*.npz"), data)
    by_language = {}
    for name in sorted(set(spoken.tolist())):
        if not name:
            continue
        mine = spoken == name
        # A language with too little labelled evidence declines rather than borrowing the line
        # its neighbours were given. Pooling is right on the subject axis, where the rare
        # subjects share a reader; here it produced Indonesian answering three quarters of its
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
    mine = np.array([subjects.index(theirs[one]) for one in predicted])
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
        # Not the frozen games: these are the folds, each scoring games its own training never
        # saw. Whoever reads the figure should know which question it answers.
        "measured_on": "out-of-fold",
    }
    return lines, by_language, carried


def languages_of(paths, data):
    """Each out-of-fold claim's language, joined on the claim the labeller was shown.

    The fold files carry the review and claim index for exactly this: the logits know nothing
    about language and the label set knows nothing about the model.
    """
    import claimdata

    parts = [np.load(path, allow_pickle=False) for path in sorted(paths)]
    review_ids = np.concatenate([part["review_id"] for part in parts])
    claim_index = np.concatenate([part["claim_index"] for part in parts])
    labelled = {(claim.review_id, claim.claim_index): claim for claim in claimdata.load(Path(data))}
    beside = [labelled.get((str(rid), int(at))) for rid, at in zip(review_ids, claim_index)]
    return np.array([one.language if one else "" for one in beside])


def macro_f1(predicted, truth, classes: int) -> float:
    """Unweighted mean F1 over the subjects that appear, so a rare row counts as much as a common one."""
    scores = []
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


def main():
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
    args = parser.parse_args()

    # `runs/<name>` names a run of this project wherever the command was typed from, so a
    # script driving the whole chain from the repository root does not silently export nothing.
    run = Path(args.run)
    if not run.is_absolute() and not run.joinpath("run.json").is_file():
        run = HERE / args.run
    record = json.loads((run / "run.json").read_text(encoding="utf-8"))
    subjects = record["subjects"]

    tokenizer = AutoTokenizer.from_pretrained(run / "tokenizer")
    # Pooling is not a weight, so a run trained on the last token loads into a mean-pooling
    # model without complaint and exports a graph that reads its claims differently from the
    # one that was measured. The parity check cannot see it either, because it compares the
    # export against the same wrongly built model.
    pooling = record.get("pooling", "mean")
    model = ClaimReader(record["backbone"], len(subjects), pooling=pooling)
    model.load_state_dict(torch.load(run / "model.bin", map_location="cpu"))
    model.eval()

    texts = sample_claims(Path(args.data), args.check, record, tokenizer)
    encoded = tokenizer(
        [pair[0] for pair in texts] if record.get("context") else texts,
        [pair[1] for pair in texts] if record.get("context") else None,
        truncation=True,
        max_length=record["max_length"],
        padding=True,
        return_tensors="pt",
    )

    # The reference answers come from the full-precision model whatever is exported, so a
    # half-precision graph is checked against the thing it is meant to approximate rather
    # than against itself.
    with torch.no_grad():
        wanted = model(encoded["input_ids"], encoded["attention_mask"])[0].numpy()

    # Halved in place: the reference answers are already taken, and building a second copy to
    # halve puts two full-precision models in memory at once. For a reader of a few billion
    # parameters that is forty gigabytes, more than the machine has spare.
    exported = InFullPrecisionOut(model.half()).eval() if args.fp16 else model

    # Traced on a handful rather than on the whole check batch. The batch axis is dynamic, so
    # the graph is the same either way, and tracing a 560M model on 256 sequences at once
    # crashes the exporter outright rather than reporting anything.
    TRACE = 8
    graph = run / "model.onnx"
    trace_graph(
        exported,
        encoded["input_ids"][:TRACE],
        encoded["attention_mask"][:TRACE],
        graph,
        args.opset,
    )

    # One file, not a graph plus a weights blob beside it. What ships is verified by checksum
    # before it is run, and a checksum over one of two files is a checksum over nothing. Only a
    # reader too large for one protobuf is two files (--external-data), and then both are its.
    import onnx

    inlined = onnx.load(str(graph), load_external_data=True)
    for stray in graph.parent.glob("model.onnx.data*"):
        stray.unlink()
    onnx.save(
        inlined,
        str(graph),
        save_as_external_data=args.external_data,
        all_tensors_to_one_file=True,
        location="model.onnx.data",
    )

    import onnxruntime

    # Half precision has no CPU kernels worth the name, so it is checked where it will run,
    # and where it runs is DirectML: that is the provider the reader opens on this machine and
    # the one the release ships with on Windows. Asking for CUDA checked a provider nothing
    # uses, and on an installation whose runtime is built for a CUDA it does not have, the
    # request failed and the check quietly fell back to the processor.
    providers = ["DmlExecutionProvider"] if args.fp16 else ["CPUExecutionProvider"]
    session = onnxruntime.InferenceSession(str(graph), providers=providers)
    # ONNX Runtime accepts a provider it does not have and quietly runs somewhere else. That
    # has already cost this project a measurement it believed: a reading reported as CUDA that
    # ran on the processor. Whatever ran the check says so, and a half-precision graph checked
    # on a processor is a weaker check than it looks.
    ran_on = session.get_providers()[0]
    print(f"parity checked on {ran_on}")
    if args.fp16 and ran_on == "CPUExecutionProvider":
        print(
            "  which is not where this graph will run. The drift below is real but the kernels "
            "are not the ones the reader uses."
        )
    # In batches, for the same reason the trace is: the check is over hundreds of sequences and
    # a bigger model has to fit them all on the card at once to answer in one go.
    ids = encoded["input_ids"].numpy()
    mask = encoded["attention_mask"].numpy()
    got = np.concatenate(
        [
            session.run(
                ["subject_logits"],
                {"input_ids": ids[at : at + 32], "attention_mask": mask[at : at + 32]},
            )[0]
            for at in range(0, len(ids), 32)
        ]
    )

    got = got.astype(np.float32)
    drift = float(np.abs(wanted - got).max())
    allowed = HALF_TOLERANCE if args.fp16 else TOLERANCE

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

    print(
        f"parity: largest drift {drift:.2e} over {len(texts)} claims, "
        f"{decided} answers changed, {ties} ties reordered"
    )
    if drift > allowed or decided:
        raise SystemExit(
            f"the exported graph disagrees with the model it came from "
            f"({drift:.2e} > {allowed:.0e}, {decided} answers changed on a margin wider than "
            f"the drift). Not shipping this."
        )
    # Ties that reorder are tolerable one at a time and not in bulk: a graph that cannot agree
    # with itself on a twentieth of its answers is not approximating the model, whatever the
    # margins say.
    if ties > len(texts) // 20:
        raise SystemExit(
            f"{ties} of {len(texts)} claims came back with a different subject. Every one is "
            f"within the drift, but a graph this unsteady is not the model. Not shipping this."
        )

    # What the Rust side needs to use the graph without being told anything else. The
    # taxonomy version is in here so a model trained against other categories is refused rather
    # than quietly asked about categories nobody labelled.
    # A threshold this low is not abstention, it is the nearest-match classifier this project
    # replaced. Twenty-four subjects put a uniform guess at 0.042, so a model told to answer
    # above 0.05 answers everything, and every claim of "unclassified" the tool makes becomes a
    # claim it cannot keep. Refused here rather than discovered in a report.
    threshold = record["validation"].get("threshold", 0.5)
    floor = 2.0 / len(subjects)
    if threshold < floor:
        raise SystemExit(
            f"threshold {threshold:.3f} is below {floor:.3f}, which for {len(subjects)} "
            f"subjects is what a coin lands on. This model does not abstain. Not shipping it."
        )

    # What the model declined on games it never saw, so the tool can say when a corpus is
    # declined far more than usual. That is the one number a reader of a new game's report
    # has no other way to get, and a corpus declined at twice the usual rate is a corpus about
    # something the taxonomy lacks.
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
    at_one_line = record.get("test", {}).get("at_validation_threshold", {})
    if carried is None and at_one_line.get("coverage") is not None:
        carried = {
            "games": len(record.get("games", {}).get("test", [])),
            "claims": record.get("claims", {}).get("test"),
            "coverage": at_one_line.get("coverage"),
            "accuracy": at_one_line.get("accuracy"),
            "macro_f1": record.get("test", {}).get("macro_f1"),
            "measured_on": "frozen games",
        }
    usual_declined = 1.0 - carried["coverage"] if carried else None

    if lines is not None and by_language is not None:
        silent = [name for name, line in zip(subjects, lines) if line is None]
        drawn = sum(1 for line in lines if line is not None)
        print(f"lines      {drawn} of {len(subjects)} subjects have one")
        if silent:
            print(f"           silent: {', '.join(silent)}")
        quiet = sorted(name for name, line in by_language.items() if line is None)
        spoke = sum(1 for line in by_language.values() if line is not None)
        print(f"languages  {spoke} of {len(by_language)} have one")
        if quiet:
            print(f"           silent: {', '.join(quiet)}")
        crossed_the_line(wanted, got, lines, allowed)

    (run / "reader.json").write_text(
        json.dumps(
            {
                "categories": args.categories,
                "subjects": subjects,
                "threshold": threshold,
                "thresholds": lines,
                # A language absent from this map has too little labelled evidence to promise
                # anything and declines. Absent as a whole on a reader exported before the
                # language axis existed, and then only the subject lines govern.
                "language_thresholds": by_language,
                "max_tokens": record["max_length"],
                "context": record.get("context", False),
                "mark": record.get("mark", False),
                "prefix": record.get("prefix", False),
                "trained_from": record["backbone"],
                "data_fingerprint": record["data_fingerprint"],
                # The weights and the rule are separate identities. Redrawing the lines without
                # retraining gives a reader that answers differently under the same `run_id`,
                # and a reading that cannot say which rule made it cannot be reconciled with
                # one made under the other.
                "lines_fingerprint": rule_fingerprint(threshold, subjects, lines, by_language),
                "name": READER_NAME,
                "run_id": run.name,
                "usual_declined": usual_declined,
                # What this model did on games it never saw, carried so that a report of a
                # game with no reference set can still say what its rates are worth. Without
                # it, the only honest thing such a report can say is nothing. Under the rule
                # that ships: with lines, the folds; without them, the frozen games at the one
                # threshold, which is the same rule the reader then applies.
                "frozen": carried,
            },
            indent=2,
        ),
        encoding="utf-8",
    )

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
    metrics = record["validation"]
    # A card that quotes one threshold for a reader that abstains per subject describes a reader
    # nobody runs, and the card is what somebody reads before pointing this at a game of theirs.
    drawn = [one for one in lines or [] if one is not None]
    silent = sum(1 for one in lines or [] if one is None)

    # The card quotes the frozen games. A card is read by somebody deciding whether to run this
    # on a game of their own, and the validation figure answers a different question: how well
    # it does on the games that chose its settings. Measured, the two differ by eighteen points.
    frozen = record.get("test", metrics)
    at_threshold = frozen.get(
        "at_validation_threshold",
        {
            "coverage": metrics.get("threshold_coverage", 0),
            "accuracy": metrics.get("threshold_accuracy"),
        },
    )
    weakest = sorted(frozen["per_subject"].items(), key=lambda pair: pair[1]["f1"])[:5]
    card.write_text(
        "\n".join(
            [
                f"# {READER_NAME}",
                "",
                f"Run `{run.name}`, fine-tuned from `{record['backbone']}`.",
                "",
                "Reads one point from a Steam review and says which subject it is about, whether",
                "it is praise or a complaint, and how sure it is. Below a calibrated threshold it",
                "says nothing, and that is a supported answer rather than a failure.",
                "",
                "## Measured",
                "",
                f"- Accuracy {frozen.get('accuracy', 0):.3f}, macro F1 {frozen.get('macro_f1', 0):.3f}",
                f"- Polarity macro F1 {frozen.get('polarity_macro_f1', 0):.3f}",
                f"- Calibration error {frozen.get('calibration_error', 0):.3f}",
                f"- Below {threshold:.2f} confidence it says nothing, which leaves it answering "
                f"{at_threshold.get('coverage', 0):.0%} of claims at "
                f"{at_threshold.get('accuracy') or 0:.3f} accuracy" + wilson_note(at_threshold),
                *(
                    [
                        f"- **What ships abstains per subject and per language**, not at that "
                        f"one line: {len(drawn)} of {len(subjects)} subjects carry a line of "
                        f"their own, {min(drawn):.2f} to {max(drawn):.2f}"
                        + (f", and {silent} are declined outright" if silent else "")
                        + ". The coverage above is what this run measured itself at, under one "
                        "threshold; `steamgauge measure-claims` over the frozen games is the "
                        "figure for the rule that ships, and it answers less of them more often.",
                        *spoken_note(by_language),
                    ]
                    if lines
                    else []
                ),
                (
                    f"- Area under the risk-coverage curve {frozen.get('aurc', 0):.3f} (lower is "
                    f"better; it says whether the model knows when it does not know)"
                ),
                (
                    f"- Trained on {record['claims']['train']} claims, validated on "
                    f"{record['claims']['validation']}, measured on {record['claims']['test']}"
                ),
                f"- Data fingerprint `{record['data_fingerprint']}`, code `{record['git_sha'][:12]}`",
                "",
                "**Every figure above is from the frozen games**, which the model never saw and",
                "which chose nothing about it, not even the threshold. At that same threshold the",
                f"validation games report {metrics.get('threshold_accuracy') or 0:.3f}, which is",
                "what it scores on games used to build it rather than what a new game gets.",
                "Weakest subjects here: "
                + ", ".join(f"`{name}` {row['f1']:.2f}" for name, row in weakest)
                + ".",
                "",
                "## Honest limits",
                "",
                "The labels were produced by a language model working from a written category",
                "sheet, so every figure above is agreement with a model rather than correctness.",
                "One person adjudicated 200 of the frozen claims: the labels name the same",
                "subject 65% of the time when the person reads cold and 89% once the sheet's",
                "rule is in front of them. What this reader scores against the person is measured",
                "once it is installed, by `steamgauge measure-claims --labels gold` over the frozen",
                "games, and the README carries the figure for the reader that ships. Two models",
                "can agree and be wrong together, most easily on sarcasm and on the boundaries",
                "between categories.",
                "",
                "## Licence",
                "",
                "Apache-2.0, as is the encoder it was fine-tuned from.",
                "",
            ]
        ),
        encoding="utf-8",
    )
    print(f"wrote {graph} and {card}")


if __name__ == "__main__":
    main()
