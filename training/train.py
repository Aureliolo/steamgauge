"""Fine-tunes a multilingual encoder to read one claim.

Two heads on a shared trunk: which subject the claim is about, and whether it is praise, a
complaint, or neither. One forward pass produces both, plus the pooled vector the clustering
uses, which is the reason for fine-tuning the encoder rather than bolting a classifier onto a
frozen one.

The loss is weighted by the labeller's own confidence. A claim they called "low" moves the
model less than one they called "high", because throwing that away and training on a flattened
id discards the only thing the annotator said about their own uncertainty.

    python train.py --backbone xlm-roberta-base
    python train.py --backbone Alibaba-NLP/gte-multilingual-base --epochs 4
"""

from __future__ import annotations

import argparse
import itertools
import json
import math
import re
import subprocess
import time
from collections import defaultdict
from pathlib import Path

import numpy as np
import torch
from torch.utils.data import DataLoader, Dataset
from transformers import AutoConfig, AutoModel, AutoTokenizer, get_linear_schedule_with_warmup

import claimdata

HERE = Path(__file__).resolve().parent
POLARITIES = claimdata.POLARITIES

# What marks the claim inside its window. Two characters no reviewer writes, which the
# tokenizer already knows: a token added to the vocabulary starts from noise and this one has
# to mean something after four thousand training claims, not four hundred thousand.
MARK = "**"


class ShippedTokenizer:
    """Enough of the transformers tokenizer for `Claims` to run on the exported one.

    The window is cut with the tokenizer that will read the claim, and the model directory
    ships `tokenizer.json` rather than a transformers directory. Duplicating the window in a
    second place is how the tool and the trainer come to disagree about what the model saw.
    """

    def __init__(self, tokenizer):
        self.tokenizer = tokenizer

    def __call__(self, text, *rest, **kwargs):
        encoded = self.tokenizer.encode(text, *rest, add_special_tokens=False)
        return {"offset_mapping": encoded.offsets, "input_ids": encoded.ids}


class Claims(Dataset):
    def __init__(
        self,
        claims,
        tokenizer,
        subjects,
        max_length,
        context=False,
        ambiguous_weight=1.0,
        split_wrong_weight=1.0,
        mark=False,
        balance=0.0,
        prefix=False,
        language_balance=0.0,
        second_weight=0.0,
    ):
        self.claims = claims
        self.ambiguous_weight = ambiguous_weight
        self.split_wrong_weight = split_wrong_weight
        # How much of the target the second labeller's answer is, on a claim read twice. The
        # loss otherwise learns one labeller's answer to a contested claim at full confidence
        # and the disagreement, which is the only thing known about such a claim, is thrown
        # away. At 0.5 a split claim is a coin between the two answers; the flag's weight
        # above already discounts it.
        self.second_weight = second_weight
        self.mark = mark
        # What a claim about a rare subject is worth against one about a common subject. The
        # loss treats a `verdict` claim and a `licensing` claim as equally informative when
        # one is a quarter of the set and the other is two in a thousand, and macro F1 is the
        # figure that notices. The exponent is a dial rather than a switch: full inverse
        # frequency makes forty claims of `vr` outweigh four thousand of `verdict` and the
        # model learns to shout about headsets.
        self.balance = {}
        if balance:
            counts = {}
            for claim in claims:
                counts[claim.subject] = counts.get(claim.subject, 0) + 1
            commonest = max(counts.values()) if counts else 1
            self.balance = {
                subject: (commonest / count) ** balance for subject, count in counts.items()
            }
        # The same dial for the language a claim is written in, and for the same reason. The
        # set is 71% English against a library that is 52.5%, and the reader is measurably
        # worse in Simplified Chinese and Korean than in English on evidence that does not
        # overlap. A claim is a claim whichever language it is in; the loss did not think so.
        self.language_balance = {}
        if language_balance:
            spoken = {}
            for claim in claims:
                spoken[claim.language] = spoken.get(claim.language, 0) + 1
            commonest = max(spoken.values()) if spoken else 1
            self.language_balance = {
                name: (commonest / count) ** language_balance for name, count in spoken.items()
            }
        self.tokenizer = tokenizer
        self.subjects = {name: index for index, name in enumerate(subjects)}
        self.polarities = {name: index for index, name in enumerate(POLARITIES)}
        self.max_length = max_length
        # Whether the model reads the claim with the review around it, as the labeller did.
        # "it doesn't" and "same here" are unanswerable alone, and every one of them in the
        # training set is a label the model is asked to reach from text that cannot reach it.
        self.context = context
        # `multilingual-e5-*` was pre-trained on pairs written as "query: " and "passage: ",
        # and a fine-tune that drops them asks the encoder a differently shaped question from
        # the one it learned. Off by default because it is a property of one family of
        # backbones rather than of this task.
        self.prefix = prefix
        # The window depends on the claim and the budget, neither of which moves, so it is cut
        # once rather than once per epoch. It costs a tokenisation of the whole review.
        self.windows = [self.window(claim) for claim in claims] if context else []

    def __len__(self):
        return len(self.claims)

    def window(self, claim):
        """The part of the review the budget can afford, centred on the claim.

        Truncating a pair from the end spends the whole budget on the opening of the review,
        so a claim at the foot of a long one is read beside paragraphs it is nowhere near.
        The budget is the same either way; where it is spent is not, and it is worth the most
        immediately around the claim.
        """
        at = claim.review_offset
        if at < 0:
            return claim.review
        offsets = self.tokenizer(
            claim.review, add_special_tokens=False, return_offsets_mapping=True
        )["offset_mapping"]
        if not offsets:
            return claim.review
        ends = at + len(claim.text)
        first = next((index for index, (_, end) in enumerate(offsets) if end > at), 0)
        last = next(
            (index for index, (start, _) in enumerate(offsets) if start >= ends), len(offsets)
        )
        # Four special tokens on a pair, and the claim is spent twice: once as the first
        # sequence, and again where it sits inside the window.
        spare = max(self.max_length - 2 * (last - first) - 4, 0) // 2
        opens = offsets[max(first - spare, 0)][0]
        closes = offsets[min(last + spare, len(offsets)) - 1][1]
        if not self.mark:
            return claim.review[opens:closes]
        # Where the claim sits inside its window, said in the text itself. A pair alone gives
        # the model the claim and its surroundings but not which sentence of the surroundings
        # is the one being asked about, and in a review that makes the same point twice about
        # two different things, that is the whole question.
        return (
            f"{claim.review[opens:at]}{MARK} {claim.review[at:ends]} {MARK}"
            f"{claim.review[ends:closes]}"
        )

    def pair(self, at):
        """The sequences the model is asked about this claim, in the form it was trained on.

        Every place that asks the model anything goes through here: the trainer, the export,
        the frontier comparison and the confidence sweep. A claim written one way in training
        and another way in scoring is a different question, and the answer still looks like an
        answer, which is how this project lost a night to a tokenizer.
        """
        claim = self.claims[at]
        asked = f"query: {claim.text}" if self.prefix else claim.text
        if not self.context:
            return (asked,)
        window = self.windows[at]
        return asked, f"passage: {window}" if self.prefix else window

    def __getitem__(self, at):
        claim = self.claims[at]
        encoded = self.tokenizer(
            *self.pair(at),
            truncation=True,
            max_length=self.max_length,
            padding="max_length",
            return_tensors="pt",
        )
        return {
            "input_ids": encoded["input_ids"][0],
            "attention_mask": encoded["attention_mask"][0],
            "subject": torch.tensor(self.subjects[claim.subject]),
            "polarity": torch.tensor(self.polarities.get(claim.polarity, 2)),
            "subject_target": self.target(
                self.subjects, claim.subject, claim.second_subject, len(self.subjects)
            ),
            "polarity_target": self.target(
                self.polarities, claim.polarity, claim.second_polarity, len(POLARITIES)
            ),
            "weight": torch.tensor(self.weight_of(claim), dtype=torch.float),
        }

    def target(self, index, first, second, size):
        """The distribution the loss is charged against: one labeller's answer, or two mixed."""
        distribution = torch.zeros(size, dtype=torch.float)
        at = index.get(first, size - 1)
        if second is None or self.second_weight <= 0 or second not in index:
            distribution[at] = 1.0
            return distribution
        distribution[at] = 1.0 - self.second_weight
        distribution[index[second]] += self.second_weight
        return distribution

    def weight_of(self, claim):
        """What this claim is worth in the loss.

        Three things the labeller said about their own answer, rather than one. A claim they
        called contested is one where the sheet does not settle the boundary, and a claim they
        called mis-cut is half a point or two stuck together: training on either at full
        weight teaches the model to reproduce a coin toss confidently.
        """
        weight = claim.weight
        if claim.ambiguous:
            weight *= self.ambiguous_weight
        if claim.split_wrong:
            weight *= self.split_wrong_weight
        return (
            weight
            * self.balance.get(claim.subject, 1.0)
            * self.language_balance.get(claim.language, 1.0)
        )


class Pool(Claims):
    """Unlabelled claims, windowed and tokenised exactly as labelled ones are.

    What comes back with each is a teacher's answer, a distribution over subjects and one over
    polarities, where a labelled claim carries one subject and one polarity. Without targets,
    for a teacher reading the pool, it is the inputs alone.
    """

    def __init__(
        self, claims, tokenizer, subjects, max_length, context, mark, prefix, targets=None
    ):
        super().__init__(claims, tokenizer, subjects, max_length, context, mark=mark, prefix=prefix)
        self.targets = targets

    def __getitem__(self, at):
        encoded = self.tokenizer(
            *self.pair(at),
            truncation=True,
            max_length=self.max_length,
            padding="max_length",
            return_tensors="pt",
        )
        item = {
            "input_ids": encoded["input_ids"][0],
            "attention_mask": encoded["attention_mask"][0],
        }
        if self.targets is not None:
            subject, polarity = self.targets
            item["subject"] = torch.from_numpy(subject[at].astype(np.float32))
            item["polarity"] = torch.from_numpy(polarity[at].astype(np.float32))
        return item


def load_targets(path, pool, subjects):
    """A teacher's answers on the pool, checked row for row against the pool they answer.

    The file is written in the pool's order and names every row, because a pool drawn again
    with another seed is a different set of claims in a file of the same name, and answers
    aligned by position to the wrong pool would train a student on noise that looks like
    knowledge.
    """
    targets = np.load(path, allow_pickle=False)
    if list(targets["subjects"]) != list(subjects):
        raise SystemExit(f"{path} answers over {list(targets['subjects'])}, not {subjects}")
    keys = list(
        zip(
            targets["app_id"].tolist(),
            targets["review_id"].tolist(),
            targets["claim_index"].tolist(),
        )
    )
    wanted = [(claim.app_id, claim.review_id, claim.claim_index) for claim in pool]
    if keys != wanted:
        raise SystemExit(f"{path} answers a different pool from the one loaded; read it again")
    return targets["subject"], targets["polarity"]


def symmetric_kl(left, right):
    """How far two distributions over the same classes sit apart, from either side, per row."""
    left = torch.log_softmax(left.float(), dim=-1)
    right = torch.log_softmax(right.float(), dim=-1)
    forward = torch.nn.functional.kl_div(left, right, log_target=True, reduction="none").sum(-1)
    backward = torch.nn.functional.kl_div(right, left, log_target=True, reduction="none").sum(-1)
    return (forward + backward) / 2


def forever(loader):
    """The loader again from the top whenever it runs out. The pool is read at the labelled
    set's pace, one batch per step, and is many times its size; which epoch it is on is of no
    interest, only that every step has a batch."""
    while True:
        yield from loader


def charged(logits, target, class_weight=None):
    """Cross-entropy against a distribution, each class weighted as the hard form weights it.

    On a one-hot target this is exactly what `cross_entropy(weight=..., reduction="none")`
    charges, so a run whose claims were read once is charged what every earlier run was; a
    claim read twice is charged against both answers in the proportion the dataset mixed.
    """
    log_probabilities = torch.log_softmax(logits.float(), dim=-1)
    if class_weight is not None:
        target = target * class_weight
    return -(target * log_probabilities).sum(dim=-1)


def error_regularisation(logits, truth, margin=0.0):
    """What the model pays for being surer of a wrong answer than of a right one.

    Cross-entropy asks the model to be right. It never asks the confidence to rank the right
    answers above the wrong ones, which is the whole of what an abstention rule needs: a model
    that is right 70% of the time and sure of exactly the wrong 30% abstains on nothing useful.
    This is the pairwise hinge of Xin, Tang, Yu and Lin (ACL 2021), pushing the softmax
    response of every wrong claim in the batch below that of every right one, which lowered
    AURC by about a tenth on GLUE-scale BERT tasks with accuracy unchanged.

    Returns a scalar, zero where a batch is all right or all wrong.
    """
    probabilities = torch.softmax(logits.float(), dim=-1)
    confidence = probabilities.max(dim=-1).values
    right = probabilities.argmax(dim=-1) == truth
    if not right.any() or right.all():
        return logits.sum() * 0.0
    wrong_confidence = confidence[~right]
    right_confidence = confidence[right]
    # Every wrong claim against every right one: the batch is a few dozen rows, so the whole
    # outer product costs nothing and says more than a sampled pair would.
    return torch.clamp(
        margin + wrong_confidence.unsqueeze(1) - right_confidence.unsqueeze(0), min=0.0
    ).mean()


def soft_cross_entropy(logits, target, temperature):
    """What a student pays for disagreeing with a teacher's distribution.

    At a temperature above one, both sides are flattened before they are compared, so the
    student learns the teacher's ranking of the wrong answers rather than only its favourite;
    the loss is scaled back by the square of the temperature so its gradient keeps its size.
    """
    if temperature != 1.0:
        target = torch.softmax(torch.log(target.clamp(min=1e-8)) / temperature, dim=-1)
    student = torch.log_softmax(logits.float() / temperature, dim=-1)
    return -(target * student).sum(dim=-1) * temperature**2


class ClaimReader(torch.nn.Module):
    """The trunk, plus the two heads, plus the pooled vector everything else wants."""

    def __init__(
        self,
        backbone: str,
        subjects: int,
        dropout: float = 0.1,
        pooling: str = "mean",
        dtype: torch.dtype | None = None,
    ):
        super().__init__()
        config = AutoConfig.from_pretrained(backbone, trust_remote_code=True)
        self.trunk = AutoModel.from_pretrained(backbone, trust_remote_code=True, torch_dtype=dtype)
        width = getattr(config, "hidden_size", 768)
        self.pooling = pooling
        self.drop = torch.nn.Dropout(dropout)
        self.subject = torch.nn.Linear(width, subjects)
        self.polarity = torch.nn.Linear(width, len(POLARITIES))

    def forward(self, input_ids, attention_mask):
        hidden = self.trunk(input_ids=input_ids, attention_mask=attention_mask).last_hidden_state
        if self.pooling == "last":
            # A decoder reads under a causal mask, so only the last token has seen the whole
            # claim; averaging the rest averages prefixes. Right padding puts that token at
            # the mask's length.
            last = attention_mask.sum(dim=1) - 1
            pooled = hidden[torch.arange(hidden.shape[0], device=hidden.device), last]
        else:
            # Mean pooling rather than the first token: the backbones being compared were not
            # all pretrained with a sentence-level CLS, and a pooling choice that only suits
            # some of them would decide the bake-off instead of the models doing it.
            mask = attention_mask.unsqueeze(-1).to(hidden.dtype)
            pooled = (hidden * mask).sum(dim=1) / mask.sum(dim=1).clamp(min=1e-9)
        # A trunk held in bf16 to fit the card feeds heads kept in fp32, and outside autocast
        # the two do not multiply.
        pooled = pooled.to(self.subject.weight.dtype)
        dropped = self.drop(pooled)
        return self.subject(dropped), self.polarity(dropped), pooled


def adapt(model: ClaimReader, rank: int) -> None:
    """Puts low-rank adapters on every linear layer of the trunk and freezes the rest of it.

    For a backbone too large to fine-tune whole on this card. It is a teacher: what it knows
    reaches the tool through teach.py and a student the tool can run, never as a graph of its
    own. The heads stay whole and trainable."""
    from peft import LoraConfig, get_peft_model

    model.trunk.gradient_checkpointing_enable(
        gradient_checkpointing_kwargs={"use_reentrant": False}
    )
    model.trunk.enable_input_require_grads()
    model.trunk = get_peft_model(
        model.trunk,
        LoraConfig(r=rank, lora_alpha=2 * rank, lora_dropout=0.05, target_modules="all-linear"),
    )


def parameter_groups(model, learning_rate: float, decay: float):
    """Layer-wise learning-rate decay: the heads at the full rate, each encoder layer a factor
    lower than the one above it, the embeddings lowest of all. The layers nearest the input
    carry what pretraining taught and are the ones a full rate at this label count overwrites.
    A decay of 1.0 is one group at one rate."""
    trainable = [(name, p) for name, p in model.named_parameters() if p.requires_grad]
    if decay >= 1.0:
        return [{"params": [p for _, p in trainable], "lr": learning_rate}]
    depth = getattr(model.trunk.config, "num_hidden_layers", 0)
    layered = re.compile(r"\.(?:layer|layers|h)\.(\d+)\.")
    groups: dict[float, list] = {}
    for name, parameter in trainable:
        if not name.startswith("trunk."):
            rate = learning_rate
        elif (found := layered.search(name)) is not None:
            rate = learning_rate * decay ** (depth - int(found.group(1)))
        elif "embed" in name:
            rate = learning_rate * decay ** (depth + 1)
        else:
            rate = learning_rate
        groups.setdefault(rate, []).append(parameter)
    return [{"params": parameters, "lr": rate} for rate, parameters in groups.items()]


def macro_f1(truth, predicted, classes):
    scores = []
    per_class = {}
    for index, name in enumerate(classes):
        hit = int(((predicted == index) & (truth == index)).sum())
        said = int((predicted == index).sum())
        was = int((truth == index).sum())
        if was == 0:
            continue
        precision = hit / said if said else 0.0
        recall = hit / was
        f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
        per_class[name] = {"precision": precision, "recall": recall, "f1": f1, "support": was}
        scores.append(f1)
    return (sum(scores) / len(scores) if scores else 0.0), per_class


def expected_calibration_error(confidence, correct, bins=15):
    """How far the model's stated certainty is from how often it is right.

    A threshold for abstention is only meaningful if 0.6 means roughly 60%, so this is
    reported beside accuracy rather than after somebody asks.
    """
    edges = np.linspace(0.0, 1.0, bins + 1)
    error = 0.0
    for low, high in itertools.pairwise(edges):
        inside = (confidence > low) & (confidence <= high)
        if not inside.any():
            continue
        error += inside.mean() * abs(correct[inside].mean() - confidence[inside].mean())
    return float(error)


def risk_coverage(confidence, correct, min_accuracy):
    """Where to stop answering, and the whole curve the choice was made from.

    The obvious objective, accuracy times coverage, is degenerate on a model that is not yet
    good. Coverage rises faster than accuracy falls all the way down, so the product is
    maximised at the bottom of the sweep and the model is told to answer everything. Measured:
    it chose 0.05, which for twenty-four subjects is barely above the 0.042 a uniform guess
    scores, and a corpus of 17,596 claims came back with nothing declined. That is the failure
    this model was built to end, arrived at by arithmetic instead of by cosine distance.

    So the objective is the one selective prediction actually asks for: answer as much as
    possible, on the condition that what you do answer is right at least `min_accuracy` of the
    time. The lowest threshold meeting that condition is the most coverage available at the
    promised quality. When no threshold meets it the model is not good enough to promise it,
    and that is recorded rather than rounded away.
    """
    curve = []
    for floor in np.arange(0.05, 0.991, 0.01):
        sure = confidence >= floor
        if not sure.any():
            continue
        curve.append(
            {
                "threshold": round(float(floor), 3),
                "coverage": float(sure.mean()),
                "accuracy": float(correct[sure].mean()),
            }
        )

    meeting = [point for point in curve if point["accuracy"] >= min_accuracy]
    if meeting:
        best = max(meeting, key=lambda point: point["coverage"])
        return curve, {**best, "met": True}

    # Nothing reaches the bar. The most accurate point available is what there is, and the
    # `met` flag is what stops it being read as though it had cleared it.
    best = max(curve, key=lambda point: point["accuracy"]) if curve else None
    if best is None:
        return curve, {"threshold": 1.0, "coverage": 0.0, "accuracy": None, "met": False}
    return curve, {**best, "met": False}


def selective(confidence_and_predictions, truth, threshold):
    """How much a threshold answers, and how often it is right when it does."""
    confidence, predicted = confidence_and_predictions
    truth = np.asarray(truth)
    sure = confidence >= threshold
    if not sure.any():
        return {"coverage": 0.0, "accuracy": None, "answered": 0}
    correct = (predicted[sure] == truth[sure]).astype(float)
    return {
        "coverage": float(sure.mean()),
        "accuracy": float(correct.mean()),
        "answered": int(sure.sum()),
    }


@torch.no_grad()
def logits_of(model, loader, device):
    """Every claim's subject logits, in the order the loader gives them."""
    model.eval()
    logits = []
    for batch in loader:
        subject, _, _ = model(batch["input_ids"].to(device), batch["attention_mask"].to(device))
        logits.append(subject.float().cpu())
    return torch.cat(logits).numpy()


def confidence_of(model, loader, device):
    """Each claim's best subject and how sure the model is of it."""
    probabilities = torch.softmax(torch.from_numpy(logits_of(model, loader, device)), dim=1).numpy()
    return probabilities.max(axis=1), probabilities.argmax(axis=1)


def area_under_risk_coverage(confidence, correct):
    """How good the confidence ordering is, without reference to any threshold.

    Lower is better. A threshold is a policy; this is the property the policy is drawn from,
    and it is what tells you whether a backbone knows when it does not know. Two models can
    reach the same accuracy and only one of them be usable with abstention.
    """
    order = np.argsort(-confidence)
    ranked = correct[order]
    risks = [1.0 - ranked[: size + 1].mean() for size in range(len(ranked))]
    return float(np.mean(risks)) if risks else 0.0


def scored_twice(predicted, truth, claims, index_of) -> dict:
    """The model against the second labeller, where there is one.

    The first labeller's figure stays the figure. These say how much of what the model gets
    wrong is the two labellers disagreeing rather than the model: scored against the second
    reading, against the claims the two agree on, and as right if either would call it so.
    """
    twice = np.array([claim.second_subject is not None for claim in claims], dtype=bool)
    if not twice.any():
        return {}
    second = np.array(
        [
            index_of.get(claim.second_subject, -1) if twice[at] else -1
            for at, claim in enumerate(claims)
        ]
    )
    correct = (predicted == truth).astype(float)
    right_by_second = (predicted == second).astype(float)
    agreed = twice & (second == truth)
    return {
        "claims": int(twice.sum()),
        "against_first": float(correct[twice].mean()),
        "against_second": float(right_by_second[twice].mean()),
        "against_either": float(np.maximum(correct, right_by_second)[twice].mean()),
        "where_both_agree": {
            "claims": int(agreed.sum()),
            "accuracy": float(correct[agreed].mean()) if agreed.any() else None,
        },
    }


@torch.no_grad()
def evaluate(model, loader, device, subjects, claims, min_accuracy=0.75):
    model.eval()
    subject_logits, polarity_logits = [], []
    for batch in loader:
        subject, polarity, _ = model(
            batch["input_ids"].to(device), batch["attention_mask"].to(device)
        )
        subject_logits.append(subject.float().cpu())
        polarity_logits.append(polarity.float().cpu())

    subject_logits = torch.cat(subject_logits)
    polarity_logits = torch.cat(polarity_logits)
    probabilities = torch.softmax(subject_logits, dim=1).numpy()
    predicted = probabilities.argmax(axis=1)
    index_of = {name: index for index, name in enumerate(subjects)}
    truth = np.array([index_of[claim.subject] for claim in claims])

    confidence = probabilities.max(axis=1)
    correct = (predicted == truth).astype(float)
    macro, per_class = macro_f1(truth, predicted, subjects)

    polarity_index = {name: index for index, name in enumerate(POLARITIES)}
    polarity_truth = np.array([polarity_index.get(claim.polarity, 2) for claim in claims])
    polarity_predicted = polarity_logits.argmax(axis=1).numpy()
    polarity_macro, _ = macro_f1(polarity_truth, polarity_predicted, POLARITIES)

    by_language = defaultdict(list)
    by_length = defaultdict(list)
    for claim, hit in zip(claims, correct):
        by_language[claim.language or "unknown"].append(hit)
        bucket = "short" if len(claim.text) < 40 else "medium" if len(claim.text) < 140 else "long"
        by_length[bucket].append(hit)

    contested = np.array([claim.ambiguous for claim in claims])
    abstention = {}
    for floor in (0.3, 0.5, 0.7, 0.9):
        sure = confidence >= floor
        abstention[str(floor)] = {
            "answers_for": float(sure.mean()),
            "accuracy": float(correct[sure].mean()) if sure.any() else None,
        }

    curve, chosen = risk_coverage(confidence, correct, min_accuracy)

    return {
        "accuracy": float(correct.mean()),
        "macro_f1": macro,
        "read_twice": scored_twice(predicted, truth, claims, index_of),
        "threshold": chosen["threshold"],
        "threshold_coverage": chosen["coverage"],
        "threshold_accuracy": chosen["accuracy"],
        "threshold_met": chosen["met"],
        "min_accuracy": min_accuracy,
        "risk_coverage": curve,
        "aurc": area_under_risk_coverage(confidence, correct),
        "polarity_macro_f1": polarity_macro,
        "calibration_error": expected_calibration_error(confidence, correct),
        "on_clear_cut": float(correct[~contested].mean()) if (~contested).any() else None,
        "on_contested": float(correct[contested].mean()) if contested.any() else None,
        "per_subject": per_class,
        "per_language": {
            name: {"accuracy": float(np.mean(hits)), "claims": len(hits)}
            for name, hits in sorted(by_language.items(), key=lambda pair: -len(pair[1]))
        },
        "per_length": {
            name: {"accuracy": float(np.mean(hits)), "claims": len(hits)}
            for name, hits in by_length.items()
        },
        "abstention": abstention,
    }


def git_sha() -> str:
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def run(args) -> dict:
    # The classifier heads start from noise and the training order is shuffled, so two runs of
    # identical settings land a point or two apart. Comparing configurations means separating
    # that from the difference being measured, which means being able to repeat a run.
    torch.manual_seed(args.seed)
    np.random.seed(args.seed)

    claims = claimdata.load(args.data)
    newer = claimdata.labels_newer_than(args.data, HERE.parent / "reference" / "claims")
    if newer:
        print(
            f"WARNING: {len(newer)} label files are newer than {args.data}, the first being "
            f"{newer[0]}; this run learns an export that lacks them. "
            f"`steamgauge export-training` writes a current one.",
            flush=True,
        )
    if args.only_language:
        # Dropped before the split rather than inside the training half, so the frozen games
        # are scored on the same languages they are taught: a model trained on English and
        # measured on everything is being marked on a paper it never sat.
        kept = [claim for claim in claims if claim.language in args.only_language]
        print(
            f"{len(kept)} of {len(claims)} claims are "
            f"{' or '.join(args.only_language)}; the rest are dropped"
        )
        claims = kept
    train, validation, test = claimdata.split_by_game(
        claims, seed=args.split_seed, fold=args.fold, folds=args.folds
    )
    subjects = claimdata.subjects_in(claims)
    print(claimdata.summarise(claims))

    # A learning curve asks what the next game buys, and the only way to read one is against a
    # test set that does not move. Training games are dropped in a fixed hash order so that
    # ten games is the same ten in every run, and the frozen and validation games are never
    # touched: the point is to vary what the model learned from, not what it is asked about.
    if args.train_games:
        kept = sorted(
            {claim.app_id for claim in train},
            key=lambda app_id: claimdata.place(app_id, args.split_seed),
        )[: args.train_games]
        train = [claim for claim in train if claim.app_id in kept]
        print(f"training on {len(kept)} of the training games, by hash order")

    print(f"train {len(train)}  validation {len(validation)}  test {len(test)} (frozen)")

    if args.batch_size % args.accumulate:
        raise SystemExit(
            f"--batch-size {args.batch_size} does not divide into {args.accumulate} passes"
        )
    micro = args.batch_size // args.accumulate

    device = "cuda" if torch.cuda.is_available() else "cpu"
    tokenizer = AutoTokenizer.from_pretrained(args.backbone, trust_remote_code=True)
    if args.lora_rank and args.ema > 0:
        # The average is a second copy of every weight, frozen base included, and the base is
        # the part that does not fit twice.
        raise SystemExit("--ema keeps a copy of the whole model; it cannot run beside --lora-rank")
    model = ClaimReader(
        args.backbone,
        len(subjects),
        pooling=args.pooling,
        dtype=torch.bfloat16 if args.lora_rank else None,
    )
    if args.lora_rank:
        adapt(model, args.lora_rank)
    model = model.to(device)
    if args.pooling == "last" and tokenizer.pad_token_id is None:
        tokenizer.pad_token = tokenizer.eos_token
    # Right padding is what last-token pooling counts on, whatever the tokenizer's own habit.
    tokenizer.padding_side = "right"

    loaders = {
        name: DataLoader(
            Claims(
                part,
                tokenizer,
                subjects,
                args.max_length,
                args.context,
                # Only the training split is reweighted. Scoring a contested claim at less
                # than a whole claim would be marking the model's own exam generously.
                args.ambiguous_weight if name == "train" else 1.0,
                args.split_wrong_weight if name == "train" else 1.0,
                args.mark,
                args.balance if name == "train" else 0.0,
                args.prefix,
                args.language_balance if name == "train" else 0.0,
                args.second_weight if name == "train" else 0.0,
            ),
            batch_size=micro if name == "train" else args.batch_size,
            shuffle=name == "train",
            num_workers=0,
        )
        for name, part in (("train", train), ("validation", validation), ("test", test))
    }

    taught_batches = None
    pool_claims = 0
    if args.pool:
        if not args.pool_targets:
            raise SystemExit(
                "--pool needs --pool-targets: a teacher's answers on it, from teach.py"
            )
        unlabelled = claimdata.load_pool(args.pool)
        targets = load_targets(args.pool_targets, unlabelled, subjects)
        # The pool was drawn over games the model trains on, but a fold's validation games are
        # training games in every other fold, so which games are safe is this run's to decide,
        # not the draw's. A row from a game this run is scored on carries the teacher's
        # knowledge of that game into the student.
        learned = {claim.app_id for claim in train}
        safe = np.array([claim.app_id in learned for claim in unlabelled])
        kept = [claim for claim, ok in zip(unlabelled, safe) if ok]
        if not kept:
            raise SystemExit(f"{args.pool} holds no claim from a game this run trains on")
        print(
            f"pool {len(kept)} of {len(unlabelled)} unlabelled claims, from games this run trains on"
        )
        pool_claims = len(kept)
        taught = DataLoader(
            Pool(
                kept,
                tokenizer,
                subjects,
                args.max_length,
                args.context,
                args.mark,
                args.prefix,
                (targets[0][safe], targets[1][safe]),
            ),
            batch_size=args.pool_batch_size or micro,
            shuffle=True,
            num_workers=0,
        )
        taught_batches = forever(taught)

    per_epoch = math.ceil(len(loaders["train"]) / args.accumulate)
    steps = per_epoch * args.epochs
    optimiser = torch.optim.AdamW(
        parameter_groups(model, args.learning_rate, args.llrd), weight_decay=0.01
    )
    schedule = get_linear_schedule_with_warmup(optimiser, int(steps * 0.1), steps)
    scaler = torch.amp.GradScaler(device, enabled=device == "cuda")
    # An exponential average of the weights along the run, evaluated and saved in place of the
    # last step's. The last step of a run that has memorised its labels is the most
    # over-confident checkpoint it has; the average sits in the flatter part of the basin.
    averaged = (
        torch.optim.swa_utils.AveragedModel(
            model, multi_avg_fn=torch.optim.swa_utils.get_ema_multi_avg_fn(args.ema)
        )
        if args.ema > 0
        else None
    )

    # Rare subjects would otherwise be drowned by `verdict`, which is most of any corpus.
    counts = claimdata.distribution(train)
    weights = torch.tensor(
        [len(train) / (len(subjects) * max(1, counts.get(name, 0))) for name in subjects],
        dtype=torch.float,
        device=device,
    )

    started = time.time()
    for epoch in range(args.epochs):
        model.train()
        running = 0.0
        taken = 0
        optimiser.zero_grad(set_to_none=True)
        for step, batch in enumerate(loaders["train"]):
            with torch.amp.autocast(device, enabled=device == "cuda", dtype=torch.bfloat16):
                input_ids = batch["input_ids"].to(device)
                attention = batch["attention_mask"].to(device)
                subject, polarity, _ = model(input_ids, attention)
                trust = batch["weight"].to(device)
                subject_target = batch["subject_target"].to(device)
                polarity_target = batch["polarity_target"].to(device)
                subject_loss = charged(subject, subject_target, weights)
                polarity_loss = charged(polarity, polarity_target)
                loss = ((subject_loss + args.polarity_weight * polarity_loss) * trust).mean()
                if args.error_reg > 0:
                    # Against the first labeller's answer, which is what the abstention rule
                    # is fitted and scored against; a claim read twice is still one answer to
                    # be surer of than of a wrong one.
                    loss = loss + args.error_reg * error_regularisation(
                        subject, batch["subject"].to(device), args.error_reg_margin
                    )
                if args.rdrop > 0:
                    # The same batch through the dropout again gives a second opinion from
                    # the same weights, and the two are charged for disagreeing. Dropout at
                    # training time and none at inference is a gap this closes: the model is
                    # pushed to answer the same way whichever units are dropped.
                    again_subject, again_polarity, _ = model(input_ids, attention)
                    disagreement = symmetric_kl(subject, again_subject) + (
                        args.polarity_weight * symmetric_kl(polarity, again_polarity)
                    )
                    again_loss = (
                        (
                            charged(again_subject, subject_target, weights)
                            + args.polarity_weight * charged(again_polarity, polarity_target)
                        )
                        * trust
                    ).mean()
                    loss = (loss + again_loss) / 2 + args.rdrop * disagreement.mean()
            scaler.scale(loss / args.accumulate).backward()
            running += float(loss.detach())

            # The gradient is only whole once every pass of the batch has contributed, and
            # clipping a partial one would clip a different quantity than the batch's own norm.
            if (step + 1) % args.accumulate and step + 1 < len(loaders["train"]):
                continue
            if taught_batches is not None:
                # Its own pass rather than rows mixed into the labelled batch, so the card
                # holds one batch's activations at a time and the labelled batch stays the
                # batch every other run learned from. The class weights stay off it: the
                # teacher's distribution is already the reading of a model trained under
                # them, and weighting it again would count the correction twice.
                pooled = next(taught_batches)
                with torch.amp.autocast(device, enabled=device == "cuda", dtype=torch.bfloat16):
                    subject, polarity, _ = model(
                        pooled["input_ids"].to(device), pooled["attention_mask"].to(device)
                    )
                    disagreement = soft_cross_entropy(
                        subject, pooled["subject"].to(device), args.pool_temperature
                    ) + args.polarity_weight * soft_cross_entropy(
                        polarity, pooled["polarity"].to(device), args.pool_temperature
                    )
                scaler.scale(disagreement.mean() * args.pool_weight).backward()
            scaler.unscale_(optimiser)
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            scaler.step(optimiser)
            scaler.update()
            schedule.step()
            optimiser.zero_grad(set_to_none=True)
            if averaged is not None:
                averaged.update_parameters(model)
            taken += 1
            # Counted in optimiser steps rather than passes: every pass that is not the last of
            # its batch has already gone back to the top, so a count of those reports progress
            # only on the passes this line never sees.
            if taken % 50 == 1:
                print(
                    f"  epoch {epoch + 1} step {taken}/{per_epoch} loss {running / (step + 1):.4f}",
                    flush=True,
                )

        metrics = evaluate(
            model, loaders["validation"], device, subjects, validation, args.min_accuracy
        )
        print(
            f"epoch {epoch + 1}: accuracy {metrics['accuracy']:.3f}  "
            f"macro F1 {metrics['macro_f1']:.3f}  polarity {metrics['polarity_macro_f1']:.3f}  "
            f"calibration {metrics['calibration_error']:.3f}"
        )
        answers = (
            "answers nothing at that accuracy"
            if not metrics["threshold_met"]
            else (
                f"answers {metrics['threshold_coverage']:.0%} of claims at "
                f"{metrics['threshold_accuracy']:.3f}"
            )
        )
        print(
            f"  abstains below {metrics['threshold']:.2f}: {answers}"
            f"{'' if metrics['threshold_met'] else f' (wanted {args.min_accuracy:.2f})'}",
            flush=True,
        )

    # The averaged weights are the run's weights from here on: what is scored below, what the
    # frozen games are read with, and what is saved.
    if averaged is not None:
        model.load_state_dict(averaged.module.state_dict())

    # Scored once more outside the loop, so what `run.json` reports is measured from the same
    # weights that get saved rather than from whichever epoch happened to be last.
    metrics = evaluate(
        model, loaders["validation"], device, subjects, validation, args.min_accuracy
    )

    # The frozen games, read once, with the threshold the validation games chose. Nothing here
    # picks anything: the moment a number from this set changes a setting, the set stops being
    # able to answer the only question it exists for. That is why it can be switched off, and
    # why the bake-off switches it off: comparing four backbones on it and then reporting the
    # winner's score from it would be reporting a number the winner was chosen by.
    #
    # It is asked at all because the validation figure was not transferring. Measured on eleven
    # games, a threshold promising 0.756 on the validation games delivered 0.620 on the frozen
    # ones, and a model card quoting the first would advertise an accuracy the tool does not
    # have on a game it has never seen.
    held = None
    if getattr(args, "frozen", True):
        held = evaluate(model, loaders["test"], device, subjects, test, args.min_accuracy)
        kept = confidence_of(model, loaders["test"], device)
        truth = [subjects.index(claim.subject) for claim in test]
        at_threshold = selective(kept, truth, metrics["threshold"])
        held["at_validation_threshold"] = at_threshold
        print(
            f"frozen games: {at_threshold['coverage']:.0%} of claims answered at "
            f"{at_threshold['accuracy']:.3f}"
            if at_threshold["coverage"] > 0
            else "frozen games: nothing cleared the threshold"
        )
        if held["read_twice"]:
            twice = held["read_twice"]
            print(
                f"  on the {twice['claims']} frozen claims read twice: {twice['against_first']:.3f} "
                f"against the first labeller, {twice['against_second']:.3f} against the second, "
                f"{twice['against_either']:.3f} against either, "
                f"{twice['where_both_agree']['accuracy'] or 0:.3f} on the "
                f"{twice['where_both_agree']['claims']} they agree on"
            )
        if (
            metrics["threshold_met"]
            and at_threshold["accuracy"] is not None
            and at_threshold["accuracy"] < args.min_accuracy
        ):
            print(
                f"  WARNING: the threshold promised {args.min_accuracy:.2f} and delivers "
                f"{at_threshold['accuracy']:.3f} on games it has never seen. The promise was "
                f"chosen on {len({claim.app_id for claim in validation})} validation games and "
                f"does not transfer; quote the frozen figure, not the validation one."
            )

    if args.save_logits:
        # The claims are named as well as scored, so folds can be pooled and a pooled set can
        # be checked for a game appearing in two of them, which would mean a claim was answered
        # by a model that had trained on it.
        out_logits = Path(args.save_logits)
        out_logits.parent.mkdir(parents=True, exist_ok=True)
        np.savez_compressed(
            out_logits,
            logits=logits_of(model, loaders["validation"], device),
            truth=np.array([subjects.index(claim.subject) for claim in validation]),
            app_id=np.array([claim.app_id for claim in validation]),
            review_id=np.array([claim.review_id for claim in validation]),
            claim_index=np.array([claim.claim_index for claim in validation]),
            subjects=np.array(subjects),
        )
        print(f"held-out logits written to {out_logits}")

    elapsed = time.time() - started
    record = {
        "backbone": args.backbone,
        "epochs": args.epochs,
        "batch_size": args.batch_size,
        "accumulate": args.accumulate,
        "learning_rate": args.learning_rate,
        "max_length": args.max_length,
        "context": args.context,
        "seed": args.seed,
        "mark": args.mark,
        "prefix": args.prefix,
        "balance": args.balance,
        "language_balance": args.language_balance,
        "only_language": args.only_language,
        "ambiguous_weight": args.ambiguous_weight,
        "split_wrong_weight": args.split_wrong_weight,
        "polarity_weight": args.polarity_weight,
        "second_weight": args.second_weight,
        "read_twice": sum(claim.second_subject is not None for claim in train),
        "error_reg": args.error_reg,
        "error_reg_margin": args.error_reg_margin if args.error_reg else None,
        "ema": args.ema,
        "llrd": args.llrd,
        "rdrop": args.rdrop,
        "pooling": args.pooling,
        "lora_rank": args.lora_rank or None,
        "dtype": "bfloat16" if args.lora_rank else "float32",
        "pool": args.pool,
        "pool_targets": args.pool_targets,
        "pool_claims": pool_claims,
        "pool_weight": args.pool_weight if args.pool else None,
        "pool_temperature": args.pool_temperature if args.pool else None,
        "split_seed": args.split_seed,
        "fold": args.fold,
        "folds": args.folds if args.fold is not None else None,
        "device": device,
        "git_sha": git_sha(),
        "data_fingerprint": claimdata.fingerprint(claims),
        "labels_newer_than_export": len(newer),
        "claims": {"train": len(train), "validation": len(validation), "test": len(test)},
        "games": {
            "train": sorted({claim.app_id for claim in train}),
            "validation": sorted({claim.app_id for claim in validation}),
            "test": sorted({claim.app_id for claim in test}),
        },
        "subjects": subjects,
        "seconds": round(elapsed),
        "validation": metrics,
    }
    if held is not None:
        record["test"] = held

    run_id = args.run_id or f"{args.backbone.replace('/', '-')}-{int(time.time())}"
    out = HERE / "runs" / run_id
    out.mkdir(parents=True, exist_ok=True)
    (out / "run.json").write_text(json.dumps(record, indent=2), encoding="utf-8")
    if args.save:
        if args.lora_rank:
            # Folded into the weights, so the saved state is a plain reader's and teach.py
            # loads it the way it loads any other run.
            model.trunk = model.trunk.merge_and_unload()
        torch.save(model.state_dict(), out / "model.bin")
        tokenizer.save_pretrained(out / "tokenizer")
    print(f"\nwritten to {out}")
    return record


def parse():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument("--backbone", default="xlm-roberta-base")
    parser.add_argument("--epochs", type=int, default=4)
    parser.add_argument("--batch-size", type=int, default=32)
    parser.add_argument(
        "--accumulate",
        type=int,
        default=1,
        help="split each batch into this many passes before the optimiser steps. The batch the "
        "model learns from is unchanged, so a run that will not fit on the card at a longer "
        "window stays comparable with one that does; halving the batch instead would move two "
        "things at once and answer neither.",
    )
    parser.add_argument("--learning-rate", type=float, default=2e-5)
    parser.add_argument("--max-length", type=int, default=128)
    parser.add_argument("--polarity-weight", type=float, default=0.5)
    parser.add_argument(
        "--ema",
        type=float,
        default=0.0,
        help="keep an exponential average of the weights at this decay per step and ship that "
        "instead of the last step; 0 switches it off",
    )
    parser.add_argument(
        "--llrd",
        type=float,
        default=1.0,
        help="layer-wise learning-rate decay: each encoder layer trains at this factor of the "
        "rate of the layer above it, the heads at the full rate; 1 is one rate for all",
    )
    parser.add_argument(
        "--rdrop",
        type=float,
        default=0.0,
        help="pass each batch through the dropout twice and charge the two answers this much "
        "for disagreeing; 0 is off. Doubles the passes, so halve the micro-batch with "
        "--accumulate 2 to keep the card's memory where it was.",
    )
    parser.add_argument(
        "--lora-rank",
        type=int,
        default=0,
        help="teach the backbone through low-rank adapters of this rank on every linear layer, "
        "with the base held in bf16 and gradient checkpointing on; 0 fine-tunes it whole. For a "
        "teacher too large to fine-tune on the card, read by teach.py and never exported.",
    )
    parser.add_argument(
        "--second-weight",
        type=float,
        default=0.0,
        help="on a claim read twice, this share of the target is the second labeller's answer "
        "and the rest the first's; 0 is off and learns the first answer alone. The export "
        "carries the second reading beside the first wherever a set has one.",
    )
    parser.add_argument(
        "--error-reg",
        type=float,
        default=0.0,
        help="charge the subject head this much for being surer of a wrong claim in the batch "
        "than of a right one (Xin et al. ACL 2021); 0 is off. Aimed at the abstention rule "
        "rather than at accuracy: it asks the confidence to rank, which is what coverage at a "
        "promised accuracy is made of.",
    )
    parser.add_argument(
        "--error-reg-margin",
        type=float,
        default=0.0,
        help="how far below a right claim's confidence a wrong one has to sit before the "
        "charge stops. 0 asks only for the order.",
    )
    parser.add_argument(
        "--pooling",
        choices=("mean", "last"),
        default="mean",
        help="how the trunk's states become one vector: the mean over the claim, or the last "
        "token, which is the only one that has seen the whole claim under a causal mask",
    )
    parser.add_argument(
        "--pool",
        default=None,
        help="unlabelled claims from `steamgauge export-pool`, learned from through a "
        "teacher's answers on them (--pool-targets, written by teach.py). One pool batch is "
        "read beside every labelled batch, so the labels and the teacher pull at once.",
    )
    parser.add_argument("--pool-targets", default=None)
    parser.add_argument(
        "--pool-weight",
        type=float,
        default=1.0,
        help="what a pool batch's disagreement with the teacher costs against a labelled "
        "batch's disagreement with its labels",
    )
    parser.add_argument(
        "--pool-temperature",
        type=float,
        default=1.0,
        help="flatten the teacher's and the student's distributions by this before comparing "
        "them, so the ranking of the wrong answers is learned too; 1 compares them as they are",
    )
    parser.add_argument(
        "--pool-batch-size",
        type=int,
        default=None,
        help="pool claims read per optimiser step; the labelled micro-batch's size unless set",
    )
    parser.add_argument("--split-seed", type=int, default=1)
    parser.add_argument(
        "--seed",
        type=int,
        default=1,
        help="seeds the head initialisation and the shuffle, so a run can be repeated. Not "
        "the split seed: which games are held out is a separate decision and never moves.",
    )
    parser.add_argument(
        "--min-accuracy",
        type=float,
        default=0.75,
        help="how often the model must be right on the claims it does answer. The abstention "
        "threshold is the one giving the most coverage at this accuracy; if none reaches it, "
        "that is recorded rather than lowered to whatever the model can manage.",
    )
    parser.add_argument(
        "--context",
        action="store_true",
        help="read each claim with the review around it, as the labeller did, rather than "
        "the claim alone. Costs tokens per claim and therefore throughput; the question is "
        "whether a claim that cannot be answered alone stops being one.",
    )
    parser.add_argument(
        "--train-games",
        type=int,
        default=0,
        help="train on only this many of the training games, chosen in a fixed hash order so "
        "that every run of a learning curve uses the same ones. The validation and frozen "
        "games are untouched, so the curve is read against one unmoving test set.",
    )
    parser.add_argument(
        "--mark",
        action="store_true",
        help="mark the claim where it sits inside its window, as well as giving it as the "
        "first sequence. A pair alone says what the claim is and what surrounds it, but not "
        "which sentence of the surroundings is the one being asked about.",
    )
    parser.add_argument(
        "--prefix",
        action="store_true",
        help="write the pair the way `multilingual-e5-*` was pre-trained on it, as "
        '"query: <claim>" and "passage: <window>". A property of that family of backbones '
        "rather than of this task, so it is off unless asked for.",
    )
    parser.add_argument(
        "--balance",
        type=float,
        default=0.0,
        help="how hard to weight rare subjects up, as an exponent on the ratio between a "
        "subject's count and the commonest subject's. 0 is off, 1 is full inverse frequency, "
        "0.5 is the square root of it. Macro F1 is the figure this moves.",
    )
    parser.add_argument(
        "--only-language",
        action="append",
        default=[],
        help="train and measure on these languages alone, repeated once per language. For "
        "answering whether the multilingual half of the set costs the English half anything, "
        "which is a question about what to ship rather than a way to ship it.",
    )
    parser.add_argument(
        "--language-balance",
        type=float,
        default=0.0,
        help="the same exponent for the language a claim is written in. The set is 71%% "
        "English against a library that is 52.5%%, and the reader reads Simplified Chinese "
        "and Korean measurably worse than English. `training/language.py` is the figure this "
        "is meant to move.",
    )
    parser.add_argument(
        "--ambiguous-weight",
        type=float,
        default=1.0,
        help="what a claim the labeller called genuinely contested is worth in the loss. "
        "Nearly a third of the set is flagged, and the sheet does not settle those "
        "boundaries, so training on them at full weight teaches a confident coin toss.",
    )
    parser.add_argument(
        "--split-wrong-weight",
        type=float,
        default=1.0,
        help="what a claim the labeller called mis-cut is worth in the loss. A sixth of the "
        "set is flagged: two points stuck together, or half of one.",
    )
    parser.add_argument(
        "--fold",
        type=int,
        default=None,
        help="hold out this cross-validation fold instead of the usual validation games. The "
        "frozen games stay frozen. Four validation games cannot settle an abstention rule: "
        "the interval around a coverage figure measured on them is wider than every "
        "difference worth deciding. Run every fold and pool what each held out.",
    )
    parser.add_argument("--folds", type=int, default=5)
    parser.add_argument(
        "--save-logits",
        default=None,
        help="write the held-out claims' subject logits, with their game, review and claim "
        "index, to this .npz. What the confidence and threshold study reads.",
    )
    parser.add_argument("--run-id", default=None)
    parser.add_argument("--save", action="store_true")
    parser.add_argument(
        "--no-frozen",
        dest="frozen",
        action="store_false",
        help="leave the frozen games unread. For runs that compare configurations against each "
        "other: a set used to choose between models cannot also say how the chosen one does.",
    )
    return parser.parse_args()


if __name__ == "__main__":
    run(parse())
