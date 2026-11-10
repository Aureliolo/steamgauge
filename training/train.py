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
import shutil
import subprocess
import time
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, Any, NotRequired, Protocol, TypedDict, cast, final, override

import numpy as np
import numpy.typing as npt
import torch
from torch.utils.data import DataLoader, Dataset
from transformers import AutoModel, AutoTokenizer, get_linear_schedule_with_warmup  # pyright: ignore[reportUnknownVariableType]  # transformers leaves the scheduler's parameters unannotated

import backbones
import claimdata
from typed_calls import manual_seed, save_pretrained

if TYPE_CHECKING:
    from collections.abc import Callable, Iterable, Iterator, Mapping, Sequence

    from transformers import PreTrainedTokenizerBase

    from claimdata import Claim

type Item = dict[str, torch.Tensor]
type FloatArray = npt.NDArray[np.floating[Any]]
type IntArray = npt.NDArray[np.integer[Any]]

HERE = Path(__file__).resolve().parent
POLARITIES = claimdata.POLARITIES

# A claim whose polarity is none of POLARITIES reads as the last one, neutral.
NEUTRAL = 2

# A claim's text is "short" or "medium" below these lengths in characters, "long" beyond them.
SHORT_CLAIM = 40
MEDIUM_CLAIM = 140

# A progress line is printed on the first optimiser step of an epoch and every this many after.
PROGRESS_EVERY = 50

# The window's budget is spent on the claim twice and on four special tokens around a pair.
SPECIAL_TOKENS_ON_A_PAIR = 4

GIGABYTE = 2**30

# The hidden size assumed for a trunk whose config does not say.
DEFAULT_TRUNK_WIDTH = 768

# Forward passes a CUDA graph is warmed with before it is recorded.
WARMUP_ITERATIONS = 3

# The optimiser's weight decay, and the share of the steps the learning rate warms up over.
WEIGHT_DECAY = 0.01
WARMUP_SHARE = 0.1

# Where the aspect logits sit in a reader's output: subject, polarity, pooled vector, aspects.
ASPECT_LOGITS_AT = 3

# Keeps a softened teacher distribution's log finite where it holds an exact zero.
EPSILON = 1e-8

# What the aspect head answers for each subject: absent, or one of the polarities, in that order,
# so an answer of 0 is absent and 1 + i is POLARITIES[i]. Matched by the Rust reader.
ASPECT_ANSWERS = 1 + len(POLARITIES)

# What marks the claim inside its window. Two characters no reviewer writes, which the
# tokenizer already knows: a token added to the vocabulary starts from noise and this one has
# to mean something after four thousand training claims, not four hundred thousand.
MARK = "**"

# What opens the review's window when the game is played only in a headset, matching the Rust
# reader: the one fact about the game the labeller is told, told to the model in the words it
# would read anywhere else. A game played on a screen gets nothing, so its claims read exactly
# as they did before any model was told anything.
HEADSET = "Played in a VR headset."

# Small capitals, which the tokenizer leaves as symbols it never learned where it folds bold and
# fullwidth letters itself, read as the letters they are. The Rust reader folds from this same
# file, so what the model is trained on and what it is asked are one text.
_SMALL_CAPITAL_LETTERS: dict[str, str] = json.loads(
    (HERE.parent / "crates" / "steamgauge-core" / "src" / "small-capitals.json").read_text(
        encoding="utf-8"
    )
)
SMALL_CAPITALS = str.maketrans(_SMALL_CAPITAL_LETTERS)


def plain(text: str) -> str:
    """Return `text` with every small capital read as the letter it is."""
    return text.translate(SMALL_CAPITALS)


class TextTokenizer(Protocol):
    """What `Claims` asks of a tokenizer: a call that returns encodings by name."""

    def __call__(
        self,
        text: str,
        pair: str | None = None,
        /,
        **kwargs: Any,  # noqa: ANN401 mirrors transformers' open keyword set
    ) -> Mapping[str, Any]:
        """Encode `text`, or the sequences `text` and `pair`."""
        ...


class Encoding(Protocol):
    """The parts of a `tokenizers.Encoding` that `ShippedTokenizer` reads."""

    @property
    def offsets(self) -> list[tuple[int, int]]:
        """The character span of each token."""
        ...

    @property
    def ids(self) -> list[int]:
        """The vocabulary id of each token."""
        ...


class Encoder(Protocol):
    """The part of a `tokenizers.Tokenizer` that `ShippedTokenizer` calls."""

    def encode(
        self, sequence: str, pair: str | None = None, *, add_special_tokens: bool = True
    ) -> Encoding:
        """Encode a sequence, or a pair of them."""
        ...


class ShippedTokenizer:
    """Enough of the transformers tokenizer for `Claims` to run on the exported one.

    The window is cut with the tokenizer that will read the claim, and the model directory
    ships `tokenizer.json` rather than a transformers directory. Duplicating the window in a
    second place is how the tool and the trainer come to disagree about what the model saw.
    """

    def __init__(self, tokenizer: Encoder) -> None:
        """Wrap a `tokenizers.Tokenizer` loaded from `tokenizer.json`."""
        self.tokenizer = tokenizer

    def __call__(self, text: str, pair: str | None = None, /, **_options: object) -> dict[str, Any]:
        """Encode like the exported tokenizer, without special tokens."""
        encoded = self.tokenizer.encode(text, pair, add_special_tokens=False)
        return {"offset_mapping": encoded.offsets, "input_ids": encoded.ids}


@dataclass(frozen=True, kw_only=True)
class Reading:
    """How a claim is put to the model: its token budget and what the text carries with it.

    Every place that asks a reader anything reads the claim the way its run was trained to.
    """

    max_length: int
    # Whether the model reads the claim with the review around it, as the labeller did.
    # "it doesn't" and "same here" are unanswerable alone, and every one of them in the
    # training set is a label the model is asked to reach from text that cannot reach it.
    context: bool = False
    mark: bool = False
    # `multilingual-e5-*` was pre-trained on pairs written as "query: " and "passage: ",
    # and a fine-tune that drops them asks the encoder a differently shaped question from
    # the one it learned. Off by default because it is a property of one family of
    # backbones rather than of this task.
    prefix: bool = False
    headset_marker: bool = False

    @classmethod
    def recorded(cls, record: Mapping[str, Any], max_length: int) -> Reading:
        """Return the reading a run's record says its reader was trained on."""
        return cls(
            max_length=max_length,
            context=bool(record.get("context", False)),
            mark=bool(record.get("mark", False)),
            prefix=bool(record.get("prefix", False)),
            headset_marker=bool(record.get("headset_marker", False)),
        )


@dataclass(frozen=True, kw_only=True)
class Weighting:
    """What a claim is worth in the loss beyond the labeller's own confidence in it."""

    ambiguous_weight: float = 1.0
    split_wrong_weight: float = 1.0
    # What a claim about a rare subject is worth against one about a common subject. The
    # loss treats a `verdict` claim and a `licensing` claim as equally informative when
    # one is a quarter of the set and the other is two in a thousand, and macro F1 is the
    # figure that notices. The exponent is a dial rather than a switch: full inverse
    # frequency makes forty claims of `vr` outweigh four thousand of `verdict` and the
    # model learns to shout about headsets.
    balance: float = 0.0
    # The same dial for the language a claim is written in, and for the same reason. The
    # set is 71% English against a library that is 52.5%, and the reader is measurably
    # worse in Simplified Chinese and Korean than in English on evidence that does not
    # overlap. A claim is a claim whichever language it is in. The loss did not think so.
    language_balance: float = 0.0
    # How much of the target the second labeller's answer is, on a claim read twice. The
    # loss otherwise learns one labeller's answer to a contested claim at full confidence
    # and the disagreement, which is the only thing known about such a claim, is thrown
    # away. At 0.5 a split claim is a coin between the two answers. The flag's weight
    # already discounts it.
    second_weight: float = 0.0


# Every claim worth what its labeller said, as the claims a reader is scored on are.
UNWEIGHTED = Weighting()


def rarity(keys: Iterable[str], exponent: float) -> dict[str, float]:
    """Return each key's weight, its commonest key's count over its own raised to `exponent`."""
    counts: dict[str, int] = {}
    for key in keys:
        counts[key] = counts.get(key, 0) + 1
    commonest = max(counts.values()) if counts else 1
    return {key: (commonest / count) ** exponent for key, count in counts.items()}


class Claims(Dataset[Item]):
    """Labelled claims as fixed-length tensors, with the targets and weights the loss reads."""

    def __init__(
        self,
        claims: Sequence[Claim],
        tokenizer: TextTokenizer,
        subjects: Sequence[str],
        reading: Reading,
        weighting: Weighting = UNWEIGHTED,
    ) -> None:
        """Hold the claims and everything that decides how each one is read and weighted."""
        self.claims = claims
        self.reading = reading
        self.weighting = weighting
        self.balance: dict[str, float] = (
            rarity((claim.subject for claim in claims), weighting.balance)
            if weighting.balance
            else {}
        )
        self.language_balance: dict[str, float] = (
            rarity((claim.language for claim in claims), weighting.language_balance)
            if weighting.language_balance
            else {}
        )
        self.tokenizer = tokenizer
        self.subjects = {name: index for index, name in enumerate(subjects)}
        self.polarities = {name: index for index, name in enumerate(POLARITIES)}
        # The window depends on the claim and the budget, neither of which moves, so it is cut
        # once rather than once per epoch. It costs a tokenisation of the whole review.
        self.windows = [self.window(claim) for claim in claims] if reading.context else []

    def __len__(self) -> int:
        """Return how many claims there are."""
        return len(self.claims)

    def window(self, claim: Claim) -> str:
        """Return the window the model reads, headed by the headset fact where trained on one."""
        cut = self.cut(claim)
        if self.reading.headset_marker and claim.headset_only:
            return f"{HEADSET} {cut}"
        return cut

    def cut(self, claim: Claim) -> str:
        """Return the part of the review the budget can afford, centred on the claim.

        Truncating a pair from the end spends the whole budget on the opening of the review,
        so a claim at the foot of a long one is read beside paragraphs it is nowhere near.
        The budget is the same either way. Where it is spent is not, and it is worth the most
        immediately around the claim.
        """
        at = claim.review_offset
        if at < 0:
            return claim.review
        offsets: list[tuple[int, int]] = self.tokenizer(
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
        spare = max(self.reading.max_length - 2 * (last - first) - SPECIAL_TOKENS_ON_A_PAIR, 0) // 2
        opens = offsets[max(first - spare, 0)][0]
        closes = offsets[min(last + spare, len(offsets)) - 1][1]
        if not self.reading.mark:
            return claim.review[opens:closes]
        # Where the claim sits inside its window, said in the text itself. A pair alone gives
        # the model the claim and its surroundings but not which sentence of the surroundings
        # is the one being asked about, and in a review that makes the same point twice about
        # two different things, that is the whole question.
        return (
            f"{claim.review[opens:at]}{MARK} {claim.review[at:ends]} {MARK}"
            f"{claim.review[ends:closes]}"
        )

    def pair(self, at: int) -> tuple[str, ...]:
        """Return the sequences the model is asked about this claim, in the form it was trained on.

        Every place that asks the model anything goes through here: the trainer, the export,
        the frontier comparison and the confidence sweep. A claim written one way in training
        and another way in scoring is a different question, and the answer still looks like an
        answer, which is how this project lost a night to a tokenizer.
        """
        claim = self.claims[at]
        text = plain(claim.text)
        asked = f"query: {text}" if self.reading.prefix else text
        if not self.reading.context:
            return (asked,)
        window = plain(self.windows[at])
        return asked, f"passage: {window}" if self.reading.prefix else window

    @override
    def __getitem__(self, index: int) -> Item:
        """Return the claim's tokens, targets and loss weight."""
        claim = self.claims[index]
        encoded = self.tokenizer(
            *self.pair(index),
            truncation=True,
            max_length=self.reading.max_length,
            padding="max_length",
            return_tensors="pt",
        )
        return {
            "input_ids": encoded["input_ids"][0],
            "attention_mask": encoded["attention_mask"][0],
            "subject": torch.tensor(self.subjects[claim.subject]),
            "polarity": torch.tensor(self.polarities.get(claim.polarity, NEUTRAL)),
            "subject_target": self.target(
                self.subjects, claim.subject, claim.second_subject, len(self.subjects)
            ),
            "polarity_target": self.target(
                self.polarities, claim.polarity, claim.second_polarity, len(POLARITIES)
            ),
            "weight": torch.tensor(self.weight_of(claim), dtype=torch.float),
            **dict(zip(("aspect", "aspect_known"), self.aspect_target(claim), strict=True)),
        }

    def aspect_target(self, claim: Claim) -> tuple[torch.Tensor, torch.Tensor]:
        """Return, for every subject, what the claim does about it, and whether that is known.

        The answer per subject is absent, or the polarity the claim takes on it. A label from a
        sheet that asked for every subject answers all of them, and so does one the labeller did
        not flag as mis-split, which covers its one subject and nothing else. One flagged under a
        sheet that asked for a single subject answers only that one. The rest are left out of the
        loss rather than charged as absent, which is what the labeller never said.
        """
        size = len(self.subjects)
        answer = torch.zeros(size, dtype=torch.long)
        covers = claim.covers()
        known = torch.full((size,), fill_value=covers is not None, dtype=torch.bool)
        for subject, polarity in (covers or {claim.subject: claim.polarity}).items():
            if subject in self.subjects:
                answer[self.subjects[subject]] = 1 + self.polarities.get(polarity, NEUTRAL)
                known[self.subjects[subject]] = True
        return answer, known

    def target(
        self, index: Mapping[str, int], first: str, second: str | None, size: int
    ) -> torch.Tensor:
        """Return the distribution the loss is charged against, one answer or two mixed."""
        distribution = torch.zeros(size, dtype=torch.float)
        at = index.get(first, size - 1)
        second_weight = self.weighting.second_weight
        if second is None or second_weight <= 0 or second not in index:
            distribution[at] = 1.0
            return distribution
        distribution[at] = 1.0 - second_weight
        distribution[index[second]] += second_weight
        return distribution

    def weight_of(self, claim: Claim) -> float:
        """Return what this claim is worth in the loss.

        Three things the labeller said about their own answer, rather than one. A claim they
        called contested is one where the sheet does not settle the boundary, and a claim they
        called mis-cut is half a point or two stuck together: training on either at full
        weight teaches the model to reproduce a coin toss confidently.
        """
        weight = claim.weight
        if claim.ambiguous:
            weight *= self.weighting.ambiguous_weight
        if claim.split_wrong:
            weight *= self.weighting.split_wrong_weight
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
        self,
        claims: Sequence[Claim],
        tokenizer: TextTokenizer,
        subjects: Sequence[str],
        reading: Reading,
        targets: tuple[FloatArray, FloatArray] | None = None,
    ) -> None:
        """Hold the pool and, for a student, the teacher's answers on it."""
        super().__init__(claims, tokenizer, subjects, reading)
        self.targets = targets

    @override
    def __getitem__(self, index: int) -> Item:
        """Return the claim's tokens and, with a teacher, its answers."""
        encoded = self.tokenizer(
            *self.pair(index),
            truncation=True,
            max_length=self.reading.max_length,
            padding="max_length",
            return_tensors="pt",
        )
        item = {
            "input_ids": encoded["input_ids"][0],
            "attention_mask": encoded["attention_mask"][0],
        }
        if self.targets is not None:
            subject, polarity = self.targets
            item["subject"] = torch.as_tensor(subject[index].astype(np.float32))
            item["polarity"] = torch.as_tensor(polarity[index].astype(np.float32))
        return item


def load_targets(
    path: str | Path, pool: Sequence[Claim], subjects: Sequence[str]
) -> tuple[FloatArray, FloatArray]:
    """Return a teacher's answers on the pool, checked row for row against the pool they answer.

    The file is written in the pool's order and names every row, because a pool drawn again
    with another seed is a different set of claims in a file of the same name, and answers
    aligned by position to the wrong pool would train a student on noise that looks like
    knowledge.
    """
    targets = np.load(path, allow_pickle=False)
    if list(targets["subjects"]) != list(subjects):
        msg = f"{path} answers over {list(targets['subjects'])}, not {subjects}"
        raise SystemExit(msg)
    keys = list(
        zip(
            targets["app_id"].tolist(),
            targets["review_id"].tolist(),
            targets["claim_index"].tolist(),
            strict=False,
        )
    )
    wanted = [(claim.app_id, claim.review_id, claim.claim_index) for claim in pool]
    if keys != wanted:
        msg = f"{path} answers a different pool from the one loaded; read it again"
        raise SystemExit(msg)
    return targets["subject"], targets["polarity"]


def symmetric_kl(left: torch.Tensor, right: torch.Tensor) -> torch.Tensor:
    """Return how far two distributions over the same classes sit apart, per row."""
    left = torch.log_softmax(left.float(), dim=-1)
    right = torch.log_softmax(right.float(), dim=-1)
    forward = torch.nn.functional.kl_div(left, right, log_target=True, reduction="none").sum(-1)
    backward = torch.nn.functional.kl_div(right, left, log_target=True, reduction="none").sum(-1)
    return (forward + backward) / 2


def forever(loader: Iterable[Item]) -> Iterator[Item]:
    """Yield the loader again from the top whenever it runs out.

    The pool is read at the labelled set's pace, one batch per step, and is many times its
    size. Which epoch it is on is of no interest, only that every step has a batch.

    Its shuffling draws from a random state of its own, derived from the seed. The global
    generator is put back after every batch, so the labelled loader's order is the same with
    or without a pool.
    """
    own = torch.Generator().manual_seed(torch.initial_seed() + 1).get_state()
    return _drawn_apart(forever_batches(loader), own)


def forever_batches(loader: Iterable[Item]) -> Iterator[Item]:
    """Yield the loader again from the top whenever it runs out."""
    while True:
        yield from loader


def _drawn_apart(batches: Iterator[Item], state: torch.Tensor) -> Iterator[Item]:
    """Advance `batches` under `state` and leave the global random state as it was found."""
    while True:
        outside = torch.get_rng_state()
        torch.set_rng_state(state)
        try:
            batch = next(batches)
        finally:
            state = torch.get_rng_state()
            torch.set_rng_state(outside)
        yield batch


def recompute_activations(model: ClaimReader) -> None:
    """Keep no activations between the forward pass and the backward.

    They are computed again as the backward needs them. Not reentrant, so the second forward
    draws the same dropout as the first and the gradient is the one the run would have had.
    The card holds little beyond the weights and the optimiser, for about a third more compute.
    """
    model.trunk.gradient_checkpointing_enable(
        gradient_checkpointing_kwargs={"use_reentrant": False}
    )


def average_of(
    model: torch.nn.Module, decay: float, *, on_host: bool
) -> torch.optim.swa_utils.AveragedModel:
    """Return an exponential average of the model's weights.

    It is kept on the card beside them or in the machine's memory. On the host it is the same
    average, a copy of every weight off the card, with the weights carried over the bus at
    every step to update it.
    """
    averaged = torch.optim.swa_utils.AveragedModel(
        model,
        device=torch.device("cpu") if on_host else None,
        multi_avg_fn=torch.optim.swa_utils.get_ema_multi_avg_fn(decay),  # pyright: ignore[reportUnknownArgumentType]  # torch leaves the callback's third parameter unannotated
    )
    if on_host and torch.cuda.is_available():
        # The copy is taken where the model is and only then moved, and the allocator keeps
        # what it freed reserved. Handed back, it is room the card's other users can have.
        torch.cuda.empty_cache()
    return averaged


class BestEpoch:
    """The trained weights as they stood after the epoch that scored best on the validation games.

    Only what training moves is copied, off the card: for a LoRA run that is the adapters and the
    heads, a few megabytes beside a frozen base of eight gigabytes. The first 4B teacher was
    still improving at its third and last epoch, so a longer run is the cheaper test of it. A
    longer run whose last epoch is past its best then keeps the best rather than the last.
    """

    def __init__(self) -> None:
        """Start with nothing kept."""
        self.score: float | None = None
        self.epoch: int | None = None
        self.weights: dict[str, torch.Tensor] = {}

    def offer(self, score: float, epoch: int, model: torch.nn.Module) -> bool:
        """Keep the model's trained weights if `score` beats every epoch offered before."""
        if self.score is not None and score <= self.score:
            return False
        self.score, self.epoch = score, epoch
        self.weights = {
            name: weight.detach().to("cpu", copy=True)
            for name, weight in model.named_parameters()
            if weight.requires_grad
        }
        return True

    @torch.no_grad()
    def restore(self, model: torch.nn.Module) -> None:
        """Copy the kept weights back into the model."""
        # In place, so a run that recorded CUDA graphs over these tensors still reads them.
        for name, weight in model.named_parameters():
            if name in self.weights:
                weight.copy_(self.weights[name].to(weight.device))


def hold_card_memory(gigabytes: float | None, device: str) -> None:
    """Holds what the run keeps on the card to this many gigabytes.

    Resting yields the card's time, not its memory: the standard student given half the card's
    time kept 20.1 GB of its 24 and crowded out the work it shared the card with. Held, the
    allocator frees its own cache before asking for more, and the run fails if it needs more
    than this. What CUDA graphs record stays reserved in their own pools, which the allocator's
    count of live memory leaves out and the hold does not, so a figure is found by holding a
    run to it, never read off `max_memory_allocated`.
    """
    if device != "cuda" or gigabytes is None:
        return
    total = int(torch.cuda.get_device_properties(0).total_memory)  # pyright: ignore[reportUnknownMemberType, reportUnknownArgumentType]  # torch's device-properties class is unresolved in its stubs
    torch.cuda.set_per_process_memory_fraction(min(1.0, gigabytes * GIGABYTE / total))  # pyright: ignore[reportUnknownMemberType]  # torch leaves the fraction unannotated


def rest_after(busy: float, share: float) -> float:
    """Return how long to leave the card idle after keeping it busy this long.

    The run takes `share` of its time that way.
    """
    return busy * (1 - share) / share


class Paced:
    """A loader that leaves the card idle for part of the time its batches keep it busy.

    The card is shared with whatever else the machine is doing, and Windows gives a CUDA process
    no lower priority to yield by, so the run yields by resting between batches. The clock is
    read after a synchronise: kernels run behind the Python that queued them, and the time until
    the next batch is asked for says nothing about the card.
    """

    def __init__(self, loader: DataLoader[Item], share: float, device: str) -> None:
        """Pace `loader` so the run takes `share` of the card's time on `device`."""
        self.loader = loader
        self.share = share
        self.device = device

    @property
    def dataset(self) -> Dataset[Item]:
        """The loader's dataset."""
        return self.loader.dataset

    def __len__(self) -> int:
        """Return how many batches one pass yields."""
        return len(self.loader)

    def __iter__(self) -> Iterator[Item]:
        """Yield the loader's batches, resting after each."""
        if self.share >= 1:
            yield from self.loader
            return
        for batch in self.loader:
            started = time.perf_counter()
            yield batch
            if self.device == "cuda":
                torch.cuda.synchronize()
            time.sleep(rest_after(time.perf_counter() - started, self.share))


def charged(
    logits: torch.Tensor, target: torch.Tensor, class_weight: torch.Tensor | None = None
) -> torch.Tensor:
    """Return cross-entropy against a distribution, each class weighted as the hard form weights it.

    On a one-hot target this is exactly what `cross_entropy(weight=..., reduction="none")`
    charges, so a run whose claims were read once is charged what every earlier run was. A
    claim read twice is charged against both answers in the proportion the dataset mixed.
    """
    log_probabilities = torch.log_softmax(logits.float(), dim=-1)
    if class_weight is not None:
        target = target * class_weight
    return -(target * log_probabilities).sum(dim=-1)


def error_regularisation(
    logits: torch.Tensor, truth: torch.Tensor, margin: float = 0.0
) -> torch.Tensor:
    """Return what the model pays for being surer of a wrong answer than of a right one.

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


def soft_cross_entropy(
    logits: torch.Tensor, target: torch.Tensor, temperature: float
) -> torch.Tensor:
    """Return what a student pays for disagreeing with a teacher's distribution.

    At a temperature above one, both sides are flattened before they are compared, so the
    student learns the teacher's ranking of the wrong answers rather than only its favourite.
    The loss is scaled back by the square of the temperature so its gradient keeps its size.
    """
    if temperature != 1.0:
        target = torch.softmax(torch.log(target.clamp(min=EPSILON)) / temperature, dim=-1)
    student = torch.log_softmax(logits.float() / temperature, dim=-1)
    return -(target * student).sum(dim=-1) * temperature**2


def aspect_loss(logits: torch.Tensor, answer: torch.Tensor, known: torch.Tensor) -> torch.Tensor:
    """Return each claim's cross-entropy over its known subjects, averaged over them.

    A claim with none known costs nothing, which is a teacher's pool row: the teacher answered
    only the subject a claim is chiefly about.
    """
    per_subject = torch.nn.functional.cross_entropy(
        logits.float().reshape(-1, logits.shape[-1]), answer.reshape(-1), reduction="none"
    ).view(answer.shape)
    known = known.float()
    return (per_subject * known).sum(dim=-1) / known.sum(dim=-1).clamp(min=1.0)


@dataclass(frozen=True, kw_only=True)
class TrunkLoad:
    """How a reader's trunk is loaded: in which dtype, and whether with its pretrained weights.

    Without them for a caller about to hand it trained ones.
    """

    dtype: torch.dtype | None = None
    pretrained: bool = True


PRETRAINED = TrunkLoad()
HEAD_DROPOUT = 0.1


class ClaimReader(torch.nn.Module):
    """The trunk, plus the heads, plus the pooled vector everything else wants.

    The subject head names the one subject a claim is chiefly about and the polarity head what
    it does about it. The aspect head, on a reader trained with one, answers for every subject
    whether the claim covers it and with what polarity. It sits beside the other two rather than
    replacing them, so the subject a claim is chiefly about, the abstention drawn on it and every
    figure scored on it mean what they meant.
    """

    def __init__(
        self,
        backbone: str,
        subjects: int,
        pooling: str = "mean",
        *,
        aspects: bool = False,
        load: TrunkLoad = PRETRAINED,
    ) -> None:
        """Load the trunk and put the heads on it, sized for `subjects` subjects."""
        super().__init__()
        # Typed `Any` because peft's wrapper replaces the transformers model in `adapt`.
        self.trunk: Any = (
            backbones.load(AutoModel, backbone, torch_dtype=load.dtype)
            if load.pretrained
            else backbones.shape(backbone, torch_dtype=load.dtype)
        )
        width: int = getattr(self.trunk.config, "hidden_size", DEFAULT_TRUNK_WIDTH)
        self.pooling = pooling
        # Whether the trunk is handed its mask as [batch, query, key]. Handed [batch, key],
        # transformers asks the card whether the mask is all ones before it builds one, and a
        # CUDA graph cannot answer a question. The square mask is built directly, the same mask.
        self.square_mask = False
        self.drop = torch.nn.Dropout(HEAD_DROPOUT)
        self.subject = torch.nn.Linear(width, subjects)
        self.polarity = torch.nn.Linear(width, len(POLARITIES))
        self.aspects = torch.nn.Linear(width, subjects * ASPECT_ANSWERS) if aspects else None

    def forward(
        self, input_ids: torch.Tensor, attention_mask: torch.Tensor
    ) -> tuple[torch.Tensor, ...]:
        """Return subject logits, polarity logits and the pooled vector, then aspect logits."""
        trunk_mask = (
            attention_mask[:, None, :].expand(-1, attention_mask.shape[1], -1)
            if self.square_mask
            else attention_mask
        )
        hidden = self.trunk(input_ids=input_ids, attention_mask=trunk_mask).last_hidden_state
        if self.pooling == "last":
            # A decoder reads under a causal mask, so only the last token has seen the whole
            # claim. Averaging the rest averages prefixes. Right padding puts that token at
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
        read = self.subject(dropped), self.polarity(dropped), pooled
        if self.aspects is None:
            return read
        return (
            *read,
            self.aspects(dropped).view(-1, self.subject.out_features, ASPECT_ANSWERS),
        )


class Passes(torch.nn.Module):
    """The reader as one pass of training reads it. It returns only what the loss uses.

    A CUDA graph replays into the same output buffers every time, so the two R-drop passes are
    one callable rather than two calls, the second of which would overwrite the first's answers.
    Returns the subject and polarity logits, the aspect logits on a reader with an aspect head,
    and then the second pass's subject and polarity logits when `twice`.
    """

    def __init__(self, model: ClaimReader, *, twice: bool, aspects: bool) -> None:
        """Wrap `model`, running it a second time when `twice`."""
        super().__init__()
        self.model = model
        self.twice = twice
        self.aspects = aspects

    @override
    def parameters(self, recurse: bool = True) -> Iterator[torch.nn.Parameter]:
        """Yield the parameters the pass reads."""
        # A graph differentiates every parameter its callable owns and refuses one the pass
        # never reads. A pass that returns no aspects reads nothing of the aspect head.
        head = self.model.aspects
        unread: set[int] = (
            set() if self.aspects or head is None else {id(p) for p in head.parameters()}
        )
        return (p for p in super().parameters(recurse) if id(p) not in unread)

    def forward(
        self, input_ids: torch.Tensor, attention_mask: torch.Tensor
    ) -> tuple[torch.Tensor, ...]:
        """Return what the loss uses, as the class describes."""
        subject, polarity, _, *aspects = self.model(input_ids, attention_mask)
        out = (subject, polarity, *(aspects if self.aspects else ()))
        if self.twice:
            again_subject, again_polarity, *_ = self.model(input_ids, attention_mask)
            out = (*out, again_subject, again_polarity)
        return out


def graphed(
    model: ClaimReader,
    shapes: Mapping[str, tuple[int, int]],
    *,
    rdrop: bool,
    aspects: bool,
    device: str,
) -> dict[str, Callable[..., tuple[torch.Tensor, ...]]]:
    """Record the labelled pass, and the teacher's pass where there is one, as CUDA graphs.

    Each training pass is thousands of small kernels, and eager PyTorch launches them one at a
    time from Python: on a processor other work holds, the card waits on the launches rather
    than the arithmetic. Replayed as a graph, a pass is one launch, forward and backward, and the
    card runs at its own pace. The loss and the optimiser stay eager. They are a few kernels.
    Every batch must have the shape it was recorded at, which the fixed-length padding gives
    and dropping each epoch's short last batch completes.
    """
    model.square_mask = True
    pooler = getattr(model.trunk, "pooler", None)
    if pooler is not None:
        # Never read, since the reader pools for itself, so it never had a gradient. A graph
        # insists every parameter it differentiates takes part.
        pooler.requires_grad_(requires_grad=False)
    names = list(shapes)
    callables = tuple(
        Passes(model, twice=rdrop, aspects=aspects)
        if name == "labelled"
        else Passes(model, twice=False, aspects=False)
        for name in names
    )
    samples = tuple(
        (
            torch.zeros(shapes[name], dtype=torch.long, device=device),
            torch.ones(shapes[name], dtype=torch.long, device=device),
        )
        for name in names
    )
    with torch.amp.autocast(device, dtype=torch.bfloat16, cache_enabled=False):
        recorded = torch.cuda.make_graphed_callables(
            callables, samples, num_warmup_iters=WARMUP_ITERATIONS
        )
    return {
        name: cast("Callable[..., tuple[torch.Tensor, ...]]", graph)
        for name, graph in zip(names, recorded, strict=True)
    }


def adapt(model: ClaimReader, rank: int) -> None:
    """Put low-rank adapters on every linear layer of the trunk and freeze the rest of it.

    For a backbone too large to fine-tune whole on this card. It is a teacher: what it knows
    reaches the tool through teach.py and a student the tool can run, never as a graph of its
    own. The heads stay whole and trainable.
    """
    from peft import LoraConfig, get_peft_model  # noqa: PLC0415 peft is needed only for LoRA runs

    model.trunk.gradient_checkpointing_enable(
        gradient_checkpointing_kwargs={"use_reentrant": False}
    )
    model.trunk.enable_input_require_grads()
    model.trunk = get_peft_model(
        model.trunk,
        LoraConfig(r=rank, lora_alpha=2 * rank, lora_dropout=0.05, target_modules="all-linear"),
    )


def parameter_groups(
    model: ClaimReader, learning_rate: float, decay: float
) -> list[dict[str, Any]]:
    """Return optimiser groups with layer-wise learning-rate decay.

    The heads train at the full rate, each encoder layer a factor lower than the one above it,
    the embeddings lowest of all. The layers nearest the input carry what pretraining taught
    and are the ones a full rate at this label count overwrites. A decay of 1.0 is one group at
    one rate.
    """
    trainable = [(name, p) for name, p in model.named_parameters() if p.requires_grad]
    if decay >= 1.0:
        return [{"params": [p for _, p in trainable], "lr": learning_rate}]
    depth: int = getattr(model.trunk.config, "num_hidden_layers", 0)
    layered = re.compile(r"\.(?:layer|layers|h)\.(\d+)\.")
    groups: dict[float, list[torch.nn.Parameter]] = {}
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


class ClassScore(TypedDict):
    """One class's precision, recall, F1 and how many claims it has."""

    precision: float
    recall: float
    f1: float
    support: int


def macro_f1(
    truth: IntArray, predicted: IntArray, classes: Sequence[str]
) -> tuple[float, dict[str, ClassScore]]:
    """Return the mean F1 over the classes that occur in `truth`, and each class's scores."""
    scores: list[float] = []
    per_class: dict[str, ClassScore] = {}
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


def expected_calibration_error(
    confidence: FloatArray, correct: FloatArray, bins: int = 15
) -> float:
    """Return how far the model's stated certainty is from how often it is right.

    A threshold for abstention is only meaningful if 0.6 means roughly 60%, so this is
    reported beside accuracy rather than after somebody asks.
    """
    edges = np.linspace(0.0, 1.0, bins + 1)
    error = 0.0
    for low, high in itertools.pairwise(edges):
        inside = (confidence > low) & (confidence <= high)
        if not inside.any():
            continue
        error += float(inside.mean() * abs(correct[inside].mean() - confidence[inside].mean()))
    return error


class CurvePoint(TypedDict):
    """What answering every claim at or above one confidence threshold achieves."""

    threshold: float
    coverage: float
    accuracy: float


class ChosenThreshold(TypedDict):
    """The threshold chosen from the curve, and whether it met the promised accuracy."""

    threshold: float
    coverage: float
    accuracy: float | None
    met: bool


def risk_coverage(
    confidence: FloatArray, correct: FloatArray, min_accuracy: float
) -> tuple[list[CurvePoint], ChosenThreshold]:
    """Return where to stop answering, and the whole curve the choice was made from.

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
    curve: list[CurvePoint] = []
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
        return curve, {
            "threshold": best["threshold"],
            "coverage": best["coverage"],
            "accuracy": best["accuracy"],
            "met": True,
        }

    # Nothing reaches the bar. The most accurate point available is what there is, and the
    # `met` flag is what stops it being read as though it had cleared it.
    if not curve:
        return curve, {"threshold": 1.0, "coverage": 0.0, "accuracy": None, "met": False}
    best = max(curve, key=lambda point: point["accuracy"])
    return curve, {
        "threshold": best["threshold"],
        "coverage": best["coverage"],
        "accuracy": best["accuracy"],
        "met": False,
    }


class Selective(TypedDict):
    """How much a threshold answers, and how often it is right when it does."""

    coverage: float
    accuracy: float | None
    answered: int


def selective(
    confidence_and_predictions: tuple[FloatArray, IntArray],
    truth: Sequence[int] | IntArray,
    threshold: float,
) -> Selective:
    """Return how much a threshold answers, and how often it is right when it does."""
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
def logits_of(model: ClaimReader, loader: Iterable[Item], device: str) -> FloatArray:
    """Return every claim's subject logits, in the order the loader gives them."""
    model.eval()
    logits: list[torch.Tensor] = []
    for batch in loader:
        subject, *_ = model(batch["input_ids"].to(device), batch["attention_mask"].to(device))
        logits.append(subject.float().cpu())
    return torch.cat(logits).numpy()


@torch.no_grad()
def aspect_logits_of(model: ClaimReader, loader: Iterable[Item], device: str) -> FloatArray:
    """Return every claim's aspect logits, subjects by answers, in the loader's order."""
    model.eval()
    logits: list[torch.Tensor] = []
    for batch in loader:
        read = model(batch["input_ids"].to(device), batch["attention_mask"].to(device))
        logits.append(read[ASPECT_LOGITS_AT].float().cpu())
    return torch.cat(logits).numpy()


def confidence_of(
    model: ClaimReader, loader: Iterable[Item], device: str
) -> tuple[FloatArray, IntArray]:
    """Return each claim's best subject and how sure the model is of it."""
    probabilities = torch.softmax(torch.as_tensor(logits_of(model, loader, device)), dim=1).numpy()
    return probabilities.max(axis=1), probabilities.argmax(axis=1)


def area_under_risk_coverage(confidence: FloatArray, correct: FloatArray) -> float:
    """Return how good the confidence ordering is, without reference to any threshold.

    Lower is better. A threshold is a policy. This is the property the policy is drawn from,
    and it is what tells you whether a backbone knows when it does not know. Two models can
    reach the same accuracy and only one of them be usable with abstention.
    """
    order = np.argsort(-confidence)
    ranked = correct[order]
    # Claims of equal confidence are answered or declined together, so the curve has one point
    # for the whole tie and its height cannot depend on the order the tied claims arrived in.
    ordered = confidence[order]
    ends = np.append(np.flatnonzero(ordered[:-1] > ordered[1:]) + 1, len(ordered))
    risks = [1.0 - ranked[:size].mean() for size in ends.tolist() if size]
    return float(np.mean(risks)) if risks else 0.0


class Agreement(TypedDict):
    """How the model fares on the claims two labellers answered the same way."""

    claims: int
    accuracy: float | None


class ReadTwice(TypedDict):
    """The model's accuracy against each labeller of the claims that were read twice."""

    claims: int
    against_first: float
    against_second: float
    against_either: float
    where_both_agree: Agreement


@final
class Nothing(TypedDict):
    """The empty answer, where there was nothing to score."""


def scored_twice(
    predicted: IntArray,
    truth: IntArray,
    claims: Sequence[Claim],
    index_of: Mapping[str, int],
) -> ReadTwice | Nothing:
    """Score the model against the second labeller, where there is one.

    The first labeller's figure stays the figure. These say how much of what the model gets
    wrong is the two labellers disagreeing rather than the model: scored against the second
    reading, against the claims the two agree on, and as right if either would call it so.
    """
    twice = np.array([claim.second_subject is not None for claim in claims], dtype=bool)
    if not twice.any():
        return {}
    second = np.array(
        [
            index_of.get(claim.second_subject, -1) if claim.second_subject is not None else -1
            for claim in claims
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


class AspectF1(TypedDict):
    """Precision, recall and F1 of the aspect head over one slice of the (claim, subject) pairs."""

    covered: int
    called: int
    precision: float | None
    recall: float | None
    f1: float | None


class AspectScores(TypedDict):
    """How well the aspect head covers the subjects, overall and per subject."""

    all: AspectF1
    beyond_the_first: AspectF1
    macro_f1: float | None
    polarity_where_both_say_covered: float | None
    per_subject: dict[str, AspectF1]


def aspect_scores(
    logits: torch.Tensor, claims: Sequence[Claim], subjects: Sequence[str]
) -> AspectScores | Nothing:
    """Score how well the aspect head says which subjects each claim covers.

    Scored only where the label answers: every subject of a claim whose label says what it
    covers, and the first subject alone of one from a sheet that asked for a single subject.
    `beyond_the_first` is the part the subject head could never answer, the other
    subjects a claim covers, which is the reason the head exists.
    """
    index_of = {name: index for index, name in enumerate(subjects)}
    predicted = logits.argmax(dim=-1).numpy()
    rows: list[tuple[int, bool, int, int]] = []
    for at, claim in enumerate(claims):
        covers = claim.covers()
        for subject, slot in index_of.items():
            if covers is None and subject != claim.subject:
                continue
            said = (covers or {claim.subject: claim.polarity}).get(subject)
            answer = (
                0
                if said is None
                else 1 + (POLARITIES.index(said) if said in POLARITIES else NEUTRAL)
            )
            rows.append((slot, subject == claim.subject, answer, int(predicted[at, slot])))
    if not rows:
        return {}
    index, first, truth, guess = (np.array(column) for column in zip(*rows, strict=True))
    present, called = truth > 0, guess > 0

    def f1(where: npt.NDArray[np.bool_]) -> AspectF1:
        """Score the pairs `where` selects."""
        hits = int((present & called & where).sum())
        wanted, given = int((present & where).sum()), int((called & where).sum())
        precision = hits / given if given else None
        recall = hits / wanted if wanted else None
        return {
            "covered": wanted,
            "called": given,
            "precision": precision,
            "recall": recall,
            "f1": (
                2 * precision * recall / (precision + recall)
                if precision and recall
                else 0.0
                if wanted
                else None
            ),
        }

    per_subject = {name: f1(index == at) for name, at in index_of.items()}
    scored = [row["f1"] for row in per_subject.values() if row["covered"] and row["f1"] is not None]
    both = present & called
    return {
        "all": f1(np.ones_like(present)),
        "beyond_the_first": f1(~first),
        "macro_f1": float(np.mean(scored)) if scored else None,
        "polarity_where_both_say_covered": (
            float((truth[both] == guess[both]).mean()) if both.any() else None
        ),
        "per_subject": per_subject,
    }


class Abstention(TypedDict):
    """How many claims answer at one confidence floor, and how often those are right."""

    answers_for: float
    accuracy: float | None


class Accuracy(TypedDict):
    """Accuracy over one slice of the claims, and how many claims it holds."""

    accuracy: float
    claims: int


class Metrics(TypedDict):
    """Everything `evaluate` reports about a reader."""

    accuracy: float
    macro_f1: float
    aspects: AspectScores | Nothing | None
    read_twice: ReadTwice | Nothing
    threshold: float
    threshold_coverage: float
    threshold_accuracy: float | None
    threshold_met: bool
    min_accuracy: float
    risk_coverage: list[CurvePoint]
    aurc: float
    polarity_macro_f1: float
    calibration_error: float
    on_clear_cut: float | None
    on_contested: float | None
    per_subject: dict[str, ClassScore]
    per_language: dict[str, Accuracy]
    per_length: dict[str, Accuracy]
    abstention: dict[str, Abstention]
    at_validation_threshold: NotRequired[Selective]


@dataclass(frozen=True)
class Split:
    """One split's batches and its claims, in the same order."""

    loader: Iterable[Item]
    claims: Sequence[Claim]


@torch.no_grad()
def evaluate(
    model: ClaimReader,
    split: Split,
    device: str,
    subjects: Sequence[str],
    min_accuracy: float = 0.75,
) -> Metrics:
    """Score the model on the split's claims: accuracy, F1, calibration, abstention and more."""
    model.eval()
    claims = split.claims
    subject_parts: list[torch.Tensor] = []
    polarity_parts: list[torch.Tensor] = []
    aspect_parts: list[torch.Tensor] = []
    for batch in split.loader:
        subject, polarity, *read = model(
            batch["input_ids"].to(device), batch["attention_mask"].to(device)
        )
        subject_parts.append(subject.float().cpu())
        polarity_parts.append(polarity.float().cpu())
        if len(read) > 1:
            aspect_parts.append(read[1].float().cpu())

    subject_logits = torch.cat(subject_parts)
    polarity_logits = torch.cat(polarity_parts)
    probabilities = torch.softmax(subject_logits, dim=1).numpy()
    predicted = probabilities.argmax(axis=1)
    index_of = {name: index for index, name in enumerate(subjects)}
    truth = np.array([index_of[claim.subject] for claim in claims])

    confidence = probabilities.max(axis=1)
    correct = (predicted == truth).astype(float)
    macro, per_class = macro_f1(truth, predicted, subjects)

    polarity_index = {name: index for index, name in enumerate(POLARITIES)}
    polarity_truth = np.array([polarity_index.get(claim.polarity, NEUTRAL) for claim in claims])
    polarity_predicted = polarity_logits.argmax(dim=1).numpy()
    polarity_macro, _ = macro_f1(polarity_truth, polarity_predicted, POLARITIES)

    by_language: defaultdict[str, list[float]] = defaultdict(list)
    by_length: defaultdict[str, list[float]] = defaultdict(list)
    for claim, hit in zip(claims, correct, strict=False):
        by_language[claim.language or "unknown"].append(float(hit))
        bucket = (
            "short"
            if len(claim.text) < SHORT_CLAIM
            else "medium"
            if len(claim.text) < MEDIUM_CLAIM
            else "long"
        )
        by_length[bucket].append(float(hit))

    contested = np.array([claim.ambiguous for claim in claims])
    abstention: dict[str, Abstention] = {}
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
        "aspects": aspect_scores(torch.cat(aspect_parts), claims, subjects)
        if aspect_parts
        else None,
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
    """Return the commit the run was made from, or "unknown" outside a git checkout."""
    git = shutil.which("git")
    if git is None:
        return "unknown"
    try:
        return subprocess.check_output([git, "rev-parse", "HEAD"], text=True).strip()  # noqa: S603 fixed arguments to the resolved git
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


@dataclass(frozen=True)
class Labelled:
    """The claims one run learns from and is scored on, split by game."""

    claims: list[Claim]
    headset_marker: bool
    newer: list[Path]
    train: list[Claim]
    validation: list[Claim]
    test: list[Claim]
    subjects: list[str]


def labelled(args: argparse.Namespace) -> Labelled:
    """Load the labels, keep the languages and training games asked for, and split them by game."""
    claims = claimdata.load(args.data)
    headset_marker = claimdata.headset_told(claims)
    newer = claimdata.labels_newer_than(args.data, HERE.parent / "reference" / "claims")
    if newer:
        print(
            f"WARNING: {len(newer)} label files are newer than {args.data}, the first being "
            f"{newer[0]}; this run learns an export that lacks them. "
            f"`steamgauge export-training` writes a current one.",
            flush=True,
        )
    if args.only_language:
        # Dropped before the split rather than inside the training half, so the held-out games
        # are scored on the same languages they are taught: a model trained on English and
        # measured on everything is being marked on a paper it never sat.
        in_language = [claim for claim in claims if claim.language in args.only_language]
        print(
            f"{len(in_language)} of {len(claims)} claims are "
            f"{' or '.join(args.only_language)}; the rest are dropped"
        )
        claims = in_language
    train, validation, test = claimdata.split_by_game(
        claims, seed=args.split_seed, fold=args.fold, folds=args.folds
    )
    subjects = claimdata.subjects_in(claims)
    print(claimdata.summarise(claims))

    # A learning curve asks what the next game buys, and the only way to read one is against a
    # test set that does not move. Training games are dropped in a fixed hash order so that
    # ten games is the same ten in every run, and the held-out and validation games are never
    # touched: the point is to vary what the model learned from, not what it is asked about.
    if args.train_games:
        kept_games = sorted(
            {claim.app_id for claim in train},
            key=lambda app_id: claimdata.place(app_id, args.split_seed),
        )[: args.train_games]
        train = [claim for claim in train if claim.app_id in kept_games]
        print(f"training on {len(kept_games)} of the training games, by hash order")

    print(f"train {len(train)}  validation {len(validation)}  test {len(test)} (held out)")
    return Labelled(
        claims=claims,
        headset_marker=headset_marker,
        newer=newer,
        train=train,
        validation=validation,
        test=test,
        subjects=subjects,
    )


def micro_batch(args: argparse.Namespace) -> int:
    """Return how many claims one pass reads, refusing a batch or card setting that cannot run."""
    if args.batch_size % args.accumulate:
        msg = f"--batch-size {args.batch_size} does not divide into {args.accumulate} passes"
        raise SystemExit(msg)
    micro: int = args.batch_size // args.accumulate
    if not 0 < args.card_share <= 1:
        msg = f"--card-share {args.card_share} is not a share of the card's time"
        raise SystemExit(msg)
    if args.card_memory is not None and args.card_memory <= 0:
        msg = f"--card-memory {args.card_memory} GB holds nothing"
        raise SystemExit(msg)
    return micro


def refuse_graphs(args: argparse.Namespace, device: str) -> None:
    """Refuse CUDA graphs where there is no card, or a reader no graph has been shown to replay."""
    if args.cuda_graphs and device != "cuda":
        msg = "--cuda-graphs records passes on a card, and there is none"
        raise SystemExit(msg)
    if args.cuda_graphs and (args.lora_rank or args.pooling == "last"):
        # A decoder's causal mask is built through the same question the square mask avoids for
        # an encoder, and no decoder has been recorded to show it replays.
        msg = "--cuda-graphs is for an encoder read by mean pooling"
        raise SystemExit(msg)


def refuse_averaging(args: argparse.Namespace) -> None:
    """Refuse a weight average beside the settings it cannot run with."""
    if args.lora_rank and args.ema > 0:
        # The average is a second copy of every weight, frozen base included, and the base is
        # the part that does not fit twice.
        msg = "--ema keeps a copy of the whole model; it cannot run beside --lora-rank"
        raise SystemExit(msg)
    if args.keep_best and args.ema > 0:
        # The average is scored only once, after the last epoch, so there is no best of it to keep.
        msg = "--keep-best chooses among epochs; --ema replaces them all with their average"
        raise SystemExit(msg)


def prepared_reader(
    args: argparse.Namespace,
    subjects: Sequence[str],
    tokenizer: PreTrainedTokenizerBase,
    device: str,
) -> ClaimReader:
    """Return the reader to train on `device`, and set the tokenizer's padding for it."""
    model = ClaimReader(
        args.backbone,
        len(subjects),
        pooling=args.pooling,
        aspects=args.aspects,
        load=TrunkLoad(dtype=torch.bfloat16 if args.lora_rank else None),
    )
    if args.lora_rank:
        adapt(model, args.lora_rank)
    elif args.recompute_activations:
        recompute_activations(model)
    model = model.to(device)
    if args.pooling == "last" and tokenizer.pad_token_id is None:  # pyright: ignore[reportUnknownMemberType]  # transformers types the id as a union with an unresolved member
        tokenizer.pad_token = tokenizer.eos_token  # pyright: ignore[reportUnknownMemberType]  # same union
    # Right padding is what last-token pooling counts on, whatever the tokenizer's own habit.
    tokenizer.padding_side = "right"
    return model


def reading_of(args: argparse.Namespace, *, headset_marker: bool) -> Reading:
    """Return how this run reads a claim, labelled or from the pool alike."""
    return Reading(
        max_length=args.max_length,
        context=args.context,
        mark=args.mark,
        prefix=args.prefix,
        headset_marker=headset_marker,
    )


def split_loaders(
    args: argparse.Namespace,
    data: Labelled,
    tokenizer: PreTrainedTokenizerBase,
    micro: int,
    device: str,
) -> tuple[dict[str, Claims], dict[str, Paced]]:
    """Return each split's claims and the paced loader that batches them."""
    reading = reading_of(args, headset_marker=data.headset_marker)
    # Only the training split is reweighted. Scoring a contested claim at less than a whole
    # claim would be marking the model's own exam generously.
    reweighted = Weighting(
        ambiguous_weight=args.ambiguous_weight,
        split_wrong_weight=args.split_wrong_weight,
        balance=args.balance,
        language_balance=args.language_balance,
        second_weight=args.second_weight,
    )
    datasets = {
        name: Claims(
            part,
            tokenizer,
            data.subjects,
            reading,
            reweighted if name == "train" else UNWEIGHTED,
        )
        for name, part in (
            ("train", data.train),
            ("validation", data.validation),
            ("test", data.test),
        )
    }
    loaders = {
        name: Paced(
            DataLoader(
                dataset,
                batch_size=micro if name == "train" else args.batch_size,
                shuffle=name == "train",
                # A recorded pass takes only the shape it was recorded at. The short batch an
                # epoch ends on is fewer claims than one pass, a different few each epoch.
                drop_last=name == "train" and args.cuda_graphs,
                num_workers=0,
            ),
            args.card_share,
            device,
        )
        for name, dataset in datasets.items()
    }
    return datasets, loaders


def taught_batches_of(
    args: argparse.Namespace, data: Labelled, tokenizer: PreTrainedTokenizerBase, micro: int
) -> tuple[Iterator[Item] | None, int]:
    """Return the teacher's batches on the pool, without end, and how many pool claims they hold."""
    if not args.pool:
        return None, 0
    if not args.pool_targets:
        msg = "--pool needs --pool-targets: a teacher's answers on it, from teach.py"
        raise SystemExit(msg)
    unlabelled = claimdata.load_pool(args.pool)
    if data.headset_marker and claimdata.headset_told(unlabelled) is not True:
        msg = (
            f"{args.pool} does not say which games are played in a headset and the labels "
            f"do; `steamgauge export-pool` again after `steamgauge store-facts`"
        )
        raise SystemExit(msg)
    targets = load_targets(args.pool_targets, unlabelled, data.subjects)
    # The pool was drawn over games the model trains on, but a fold's validation games are
    # training games in every other fold, so which games are safe is this run's to decide,
    # not the draw's. A row from a game this run is scored on carries the teacher's
    # knowledge of that game into the student.
    learned = {claim.app_id for claim in data.train}
    safe = np.array([claim.app_id in learned for claim in unlabelled])
    pool_kept = [claim for claim, ok in zip(unlabelled, safe, strict=True) if ok]
    if not pool_kept:
        msg = f"{args.pool} holds no claim from a game this run trains on"
        raise SystemExit(msg)
    print(
        f"pool {len(pool_kept)} of {len(unlabelled)} unlabelled claims, "
        "from games this run trains on"
    )
    taught = DataLoader(
        Pool(
            pool_kept,
            tokenizer,
            data.subjects,
            reading_of(args, headset_marker=data.headset_marker),
            (targets[0][safe], targets[1][safe]),
        ),
        batch_size=args.pool_batch_size or micro,
        shuffle=True,
        drop_last=args.cuda_graphs,
        num_workers=0,
    )
    return forever(taught), len(pool_kept)


@dataclass(frozen=True)
class Setup:
    """What a run has put together before its first step."""

    args: argparse.Namespace
    data: Labelled
    tokenizer: PreTrainedTokenizerBase
    model: ClaimReader
    device: str
    datasets: dict[str, Claims]
    loaders: dict[str, Paced]
    taught_batches: Iterator[Item] | None
    pool_claims: int


@dataclass(frozen=True)
class Optimisation:
    """The optimiser and what steps beside it: the schedule, the loss scaler and the average."""

    optimiser: torch.optim.Optimizer
    schedule: torch.optim.lr_scheduler.LRScheduler
    scaler: torch.amp.GradScaler
    averaged: torch.optim.swa_utils.AveragedModel | None


def optimisation_for(setup: Setup, steps: int) -> Optimisation:
    """Return the optimiser, its warm-up schedule over `steps`, the scaler and the average."""
    args = setup.args
    # Fused on the card: the same update in one kernel, where the default builds temporaries the
    # size of every parameter at each step. On Windows the card's memory is charged against the
    # machine's commit as well, so those temporaries are gigabytes the rest of the machine loses,
    # and the run that died of "out of memory" with the card half empty died in them.
    optimiser = torch.optim.AdamW(
        parameter_groups(setup.model, args.learning_rate, args.llrd),
        weight_decay=WEIGHT_DECAY,
        fused=setup.device == "cuda",
    )
    schedule = get_linear_schedule_with_warmup(optimiser, int(steps * WARMUP_SHARE), steps)
    scaler = torch.amp.GradScaler(setup.device, enabled=setup.device == "cuda")
    # An exponential average of the weights along the run, evaluated and saved in place of the
    # last step's. The last step of a run that has memorised its labels is the most
    # over-confident checkpoint it has. The average sits in the flatter part of the basin.
    averaged = (
        average_of(setup.model, args.ema, on_host=args.average_on_host) if args.ema > 0 else None
    )
    return Optimisation(optimiser, schedule, scaler, averaged)


def class_weights(data: Labelled, device: str) -> torch.Tensor:
    """Return each subject's weight in the loss, the rarer in the training split the heavier."""
    # Rare subjects would otherwise be drowned by `verdict`, which is most of any corpus.
    counts = claimdata.distribution(data.train)
    return torch.tensor(
        [
            len(data.train) / (len(data.subjects) * max(1, counts.get(name, 0)))
            for name in data.subjects
        ],
        dtype=torch.float,
        device=device,
    )


def training_passes(setup: Setup, micro: int) -> dict[str, Callable[..., tuple[torch.Tensor, ...]]]:
    """Put the reader in training and return its passes, recorded as CUDA graphs where asked."""
    args, model = setup.args, setup.model
    model.train()
    passes: dict[str, Callable[..., tuple[torch.Tensor, ...]]] = {
        "labelled": Passes(model, twice=args.rdrop > 0, aspects=args.aspects)
    }
    if setup.taught_batches is not None:
        passes["taught"] = Passes(model, twice=False, aspects=False)
    if args.cuda_graphs:
        passes = graphed(
            model,
            {
                "labelled": (micro, args.max_length),
                **(
                    {"taught": (args.pool_batch_size or micro, args.max_length)}
                    if setup.taught_batches is not None
                    else {}
                ),
            },
            rdrop=args.rdrop > 0,
            aspects=args.aspects,
            device=setup.device,
        )
        # A replay hands back its gradients in the graph's own buffers, and a parameter with no
        # gradient yet keeps the buffer itself as its gradient: the next replay then overwrites
        # the passes before it rather than adding to them, measured at a third of the batch's
        # gradient lost. Gradients held from the start and zeroed in place are only ever added to.
        for parameter in model.parameters():
            if parameter.requires_grad:
                parameter.grad = torch.zeros_like(parameter)
    return passes


@dataclass(frozen=True)
class Training:
    """Everything an epoch of optimiser steps reads."""

    setup: Setup
    optimisation: Optimisation
    passes: dict[str, Callable[..., tuple[torch.Tensor, ...]]]
    class_weights: torch.Tensor
    autocast_options: dict[str, Any]
    per_epoch: int


def labelled_loss(training: Training, batch: Item) -> torch.Tensor:
    """Return the loss on one labelled pass: both heads, the aspects and the regularisers asked."""
    args, device = training.setup.args, training.setup.device
    weights = training.class_weights
    input_ids = batch["input_ids"].to(device)
    attention = batch["attention_mask"].to(device)
    subject, polarity, *read = training.passes["labelled"](input_ids, attention)
    aspects = read.pop(0) if args.aspects else None
    trust = batch["weight"].to(device)
    subject_target = batch["subject_target"].to(device)
    polarity_target = batch["polarity_target"].to(device)
    subject_loss = charged(subject, subject_target, weights)
    polarity_loss = charged(polarity, polarity_target)
    loss: torch.Tensor = ((subject_loss + args.polarity_weight * polarity_loss) * trust).mean()
    if aspects is not None:
        loss = (
            loss
            + args.aspect_weight
            * (
                aspect_loss(
                    aspects,
                    batch["aspect"].to(device),
                    batch["aspect_known"].to(device),
                )
                * trust
            ).mean()
        )
    if args.error_reg > 0:
        # Against the first labeller's answer, which is what the abstention rule is fitted and
        # scored against. A claim read twice is still one answer to be surer of than of a wrong
        # one.
        loss = loss + args.error_reg * error_regularisation(
            subject, batch["subject"].to(device), args.error_reg_margin
        )
    if args.rdrop > 0:
        # The same batch through the dropout again gives a second opinion from the same
        # weights, and the two are charged for disagreeing. Dropout at training time and none
        # at inference is a gap this closes: the model is pushed to answer the same way
        # whichever units are dropped.
        again_subject, again_polarity = read
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
    return loss


def taught_backward(training: Training, batches: Iterator[Item]) -> None:
    """Add the gradient of one pass over the teacher's next batch to the labelled batch's."""
    # Its own pass rather than rows mixed into the labelled batch, so the card holds one
    # batch's activations at a time and the labelled batch stays the batch every other run
    # learned from. The class weights stay off it: the teacher's distribution is already the
    # reading of a model trained under them, and weighting it again would count the correction
    # twice.
    args, device = training.setup.args, training.setup.device
    pooled = next(batches)
    with torch.amp.autocast(device, **training.autocast_options):
        subject, polarity = training.passes["taught"](
            pooled["input_ids"].to(device), pooled["attention_mask"].to(device)
        )
        disagreement = soft_cross_entropy(
            subject, pooled["subject"].to(device), args.pool_temperature
        ) + args.polarity_weight * soft_cross_entropy(
            polarity, pooled["polarity"].to(device), args.pool_temperature
        )
    taught_loss: torch.Tensor = disagreement.mean() * args.pool_weight
    training.optimisation.scaler.scale(taught_loss).backward()  # pyright: ignore[reportUnknownMemberType]  # torch leaves Tensor.backward's parameters partly unresolved


def optimiser_step(training: Training) -> None:
    """Clip the batch's whole gradient, step the optimiser and schedule, and update the average."""
    optimisation, model = training.optimisation, training.setup.model
    optimisation.scaler.unscale_(optimisation.optimiser)
    torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
    optimisation.scaler.step(optimisation.optimiser)
    optimisation.scaler.update()
    optimisation.schedule.step()
    optimisation.optimiser.zero_grad(set_to_none=not training.setup.args.cuda_graphs)
    if optimisation.averaged is not None:
        optimisation.averaged.update_parameters(model)


def train_epoch(training: Training, epoch: int) -> None:
    """Run one epoch of optimiser steps over the training split, printing progress on the way."""
    setup, optimisation = training.setup, training.optimisation
    args, device = setup.args, setup.device
    loader = setup.loaders["train"]
    setup.model.train()
    # Summed on the card: read back after every pass, the loss would make the processor
    # wait for the card before it could queue the next pass.
    running = torch.zeros((), device=device)
    taken = 0
    optimisation.optimiser.zero_grad(set_to_none=not args.cuda_graphs)
    for step, batch in enumerate(loader):
        with torch.amp.autocast(device, **training.autocast_options):
            loss = labelled_loss(training, batch)
        per_pass: torch.Tensor = loss / args.accumulate
        optimisation.scaler.scale(per_pass).backward()  # pyright: ignore[reportUnknownMemberType]  # torch leaves Tensor.backward's parameters partly unresolved
        running += loss.detach()

        # The gradient is only whole once every pass of the batch has contributed, and
        # clipping a partial one would clip a different quantity than the batch's own norm.
        if (step + 1) % args.accumulate and step + 1 < len(loader):
            continue
        if setup.taught_batches is not None:
            taught_backward(training, setup.taught_batches)
        optimiser_step(training)
        taken += 1
        # Counted in optimiser steps rather than passes: every pass that is not the last of
        # its batch has already gone back to the top, so a count of those reports progress
        # only on the passes this line never sees.
        if taken % PROGRESS_EVERY == 1:
            # What the run holds on the card, which on Windows it also holds of the machine's
            # memory: the figure to read when the machine runs short, beside the step it ran
            # short at.
            card_note = (
                f" card {torch.cuda.memory_reserved() / GIGABYTE:.1f} GB"
                if device == "cuda"
                else ""
            )
            print(
                f"  epoch {epoch + 1} step {taken}/{training.per_epoch} "
                f"loss {running.item() / (step + 1):.4f}{card_note}",
                flush=True,
            )


def report_epoch(metrics: Metrics, epoch: int, min_accuracy: float) -> None:
    """Print what the validation games say of the reader after an epoch."""
    print(
        f"epoch {epoch + 1}: accuracy {metrics['accuracy']:.3f}  "
        f"macro F1 {metrics['macro_f1']:.3f}  polarity {metrics['polarity_macro_f1']:.3f}  "
        f"calibration {metrics['calibration_error']:.3f}"
    )
    aspect_report = metrics["aspects"]
    if aspect_report is not None and "all" in aspect_report:
        beyond = aspect_report["beyond_the_first"]
        print(
            f"  aspects: F1 {aspect_report['all']['f1']:.3f}, beyond the first subject "
            f"F1 {beyond['f1'] or 0:.3f} on {beyond['covered']} covered"
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
        f"{'' if metrics['threshold_met'] else f' (wanted {min_accuracy:.2f})'}",
        flush=True,
    )


def report_read_twice(twice: ReadTwice | Nothing) -> None:
    """Print how the reader does on the held-out claims two labellers read."""
    if "claims" in twice:
        print(
            f"  on the {twice['claims']} held-out claims read twice: "
            f"{twice['against_first']:.3f} against the first labeller, "
            f"{twice['against_second']:.3f} against the second, "
            f"{twice['against_either']:.3f} against either, "
            f"{twice['where_both_agree']['accuracy'] or 0:.3f} on the "
            f"{twice['where_both_agree']['claims']} they agree on"
        )


def held_out(setup: Setup, metrics: Metrics) -> Metrics | None:
    """Read the held-out games once, at the threshold the validation games chose.

    Nothing here picks anything: the moment a number from this set changes a setting, the set
    stops being able to answer the only question it exists for. That is why it can be switched
    off, and why the bake-off switches it off: comparing four backbones on it and then
    reporting the winner's score from it would be reporting a number the winner was chosen by.

    It is asked at all because the validation figure was not transferring. Measured on eleven
    games, a threshold promising 0.756 on the validation games delivered 0.620 on the held-out
    ones, and a model card quoting the first would advertise an accuracy the tool does not
    have on a game it has never seen.
    """
    args, data, model, device = setup.args, setup.data, setup.model, setup.device
    if not getattr(args, "held_out", True):
        return None
    on_test = Split(setup.loaders["test"], data.test)
    held = evaluate(model, on_test, device, data.subjects, args.min_accuracy)
    answers_on_test = confidence_of(model, setup.loaders["test"], device)
    truth = [data.subjects.index(claim.subject) for claim in data.test]
    at_threshold = selective(answers_on_test, truth, metrics["threshold"])
    held["at_validation_threshold"] = at_threshold
    print(
        f"held-out games: {at_threshold['coverage']:.0%} of claims answered at "
        f"{at_threshold['accuracy']:.3f}"
        if at_threshold["coverage"] > 0
        else "held-out games: nothing cleared the threshold"
    )
    report_read_twice(held["read_twice"])
    if (
        metrics["threshold_met"]
        and at_threshold["accuracy"] is not None
        and at_threshold["accuracy"] < args.min_accuracy
    ):
        print(
            f"  WARNING: the threshold promised {args.min_accuracy:.2f} and delivers "
            f"{at_threshold['accuracy']:.3f} on games it has never seen. The promise was "
            f"chosen on {len({claim.app_id for claim in data.validation})} validation games and "
            f"does not transfer; quote the held-out figure, not the validation one."
        )
    return held


def write_logits(setup: Setup, out_logits: Path) -> None:
    """Write the validation claims' logits, each named by game, review and claim, to an .npz.

    The claims are named as well as scored, so folds can be pooled and a pooled set can be
    checked for a game appearing in two of them, which would mean a claim was answered by a
    model that had trained on it.
    """
    data, model, device = setup.data, setup.model, setup.device
    loader = setup.loaders["validation"]
    out_logits.parent.mkdir(parents=True, exist_ok=True)
    # A reader with an aspect head also leaves what it said for every subject and what each
    # label answers, which is what a line per subject is drawn from.
    aspect_arrays: dict[str, Any] = {}
    if setup.args.aspects:
        answered = [setup.datasets["validation"].aspect_target(claim) for claim in data.validation]
        aspect_arrays = {
            "aspect_logits": aspect_logits_of(model, loader, device),
            "aspect_truth": np.stack([answer.numpy() for answer, _ in answered]),
            "aspect_known": np.stack([known.numpy() for _, known in answered]),
        }
    np.savez_compressed(
        out_logits,
        logits=logits_of(model, loader, device),
        truth=np.array([data.subjects.index(claim.subject) for claim in data.validation]),
        app_id=np.array([claim.app_id for claim in data.validation]),
        review_id=np.array([claim.review_id for claim in data.validation]),
        claim_index=np.array([claim.claim_index for claim in data.validation]),
        subjects=np.array(data.subjects),
        **aspect_arrays,
    )
    print(f"held-out logits written to {out_logits}")


def record_of(
    setup: Setup,
    metrics: Metrics,
    held: Metrics | None,
    kept_epoch: int | None,
    elapsed: float,
) -> dict[str, Any]:
    """Return what `run.json` records: the configuration, the data it ran on and its scores."""
    args, data = setup.args, setup.data
    train, validation, test = data.train, data.validation, data.test
    record: dict[str, Any] = {
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
        "headset_marker": data.headset_marker,
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
        "kept_epoch": kept_epoch,
        "llrd": args.llrd,
        "rdrop": args.rdrop,
        "pooling": args.pooling,
        "aspects": args.aspects,
        "aspect_weight": args.aspect_weight if args.aspects else None,
        "lora_rank": args.lora_rank or None,
        "dtype": "bfloat16" if args.lora_rank else "float32",
        "pool": args.pool,
        "pool_targets": args.pool_targets,
        "pool_claims": setup.pool_claims,
        "pool_weight": args.pool_weight if args.pool else None,
        "pool_temperature": args.pool_temperature if args.pool else None,
        "split_seed": args.split_seed,
        "fold": args.fold,
        "folds": args.folds if args.fold is not None else None,
        "device": setup.device,
        "git_sha": git_sha(),
        "data_fingerprint": claimdata.fingerprint(data.claims),
        "labels_newer_than_export": len(data.newer),
        "claims": {"train": len(train), "validation": len(validation), "test": len(test)},
        "games": {
            "train": sorted({claim.app_id for claim in train}),
            "validation": sorted({claim.app_id for claim in validation}),
            "test": sorted({claim.app_id for claim in test}),
        },
        "subjects": data.subjects,
        "seconds": round(elapsed),
        "validation": metrics,
    }
    if held is not None:
        record["test"] = held
    return record


def write_run(setup: Setup, record: dict[str, Any]) -> None:
    """Write the run's record and, when asked, its weights and tokenizer, to its directory."""
    args, model = setup.args, setup.model
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
        save_pretrained(setup.tokenizer, out / "tokenizer")
    print(f"\nwritten to {out}")


def run(args: argparse.Namespace) -> dict[str, Any]:
    """Train and score one configuration, write its run directory and return its record."""
    # The classifier heads start from noise and the training order is shuffled, so two runs of
    # identical settings land a point or two apart. Comparing configurations means separating
    # that from the difference being measured, which means being able to repeat a run.
    manual_seed(args.seed)
    np.random.seed(args.seed)  # noqa: NPY002 seeds the global generator that library code draws from

    data = labelled(args)
    micro = micro_batch(args)

    device = "cuda" if torch.cuda.is_available() else "cpu"
    hold_card_memory(args.card_memory, device)
    refuse_graphs(args, device)
    tokenizer: PreTrainedTokenizerBase = backbones.load(AutoTokenizer, args.backbone)
    refuse_averaging(args)
    model = prepared_reader(args, data.subjects, tokenizer, device)
    datasets, loaders = split_loaders(args, data, tokenizer, micro, device)
    taught_batches, pool_claims = taught_batches_of(args, data, tokenizer, micro)
    setup = Setup(
        args=args,
        data=data,
        tokenizer=tokenizer,
        model=model,
        device=device,
        datasets=datasets,
        loaders=loaders,
        taught_batches=taught_batches,
        pool_claims=pool_claims,
    )

    per_epoch = math.ceil(len(loaders["train"]) / args.accumulate)
    optimisation = optimisation_for(setup, per_epoch * args.epochs)
    weights = class_weights(data, device)
    passes = training_passes(setup, micro)
    training = Training(
        setup=setup,
        optimisation=optimisation,
        passes=passes,
        class_weights=weights,
        # A graph replays under an autocast that caches no casts, since a cast cached at
        # recording would be a buffer the replay never refills.
        autocast_options={
            "enabled": device == "cuda",
            "dtype": torch.bfloat16,
            "cache_enabled": not args.cuda_graphs,
        },
        per_epoch=per_epoch,
    )

    best = BestEpoch() if args.keep_best else None
    started = time.time()
    on_validation = Split(loaders["validation"], data.validation)
    for epoch in range(args.epochs):
        train_epoch(training, epoch)
        metrics = evaluate(model, on_validation, device, data.subjects, args.min_accuracy)
        report_epoch(metrics, epoch, args.min_accuracy)
        # Macro F1 rather than accuracy: the rows a reader has to get right are the rare ones,
        # and accuracy is a quarter `verdict`. Only the validation games choose. The held-out ones
        # never see this.
        if best is not None and best.offer(metrics["macro_f1"], epoch + 1, model):
            print(f"  best so far: epoch {epoch + 1}, kept", flush=True)

    if best is not None:
        best.restore(model)
        print(f"kept epoch {best.epoch} of {args.epochs}: validation macro F1 {best.score:.3f}")

    # The averaged weights are the run's weights from here on: what is scored below, what the
    # held-out games are read with, and what is saved.
    if optimisation.averaged is not None:
        averaged_module: torch.nn.Module = optimisation.averaged.module  # pyright: ignore[reportUnknownMemberType]  # AveragedModel.module is unannotated in torch
        model.load_state_dict(averaged_module.state_dict())

    # Scored once more outside the loop, so what `run.json` reports is measured from the same
    # weights that get saved rather than from whichever epoch happened to be last.
    metrics = evaluate(model, on_validation, device, data.subjects, args.min_accuracy)
    held = held_out(setup, metrics)
    if args.save_logits:
        write_logits(setup, Path(args.save_logits))

    elapsed = time.time() - started
    record = record_of(setup, metrics, held, best.epoch if best is not None else None, elapsed)
    write_run(setup, record)
    return record


def parse() -> argparse.Namespace:
    """Read the command line into the settings of one run."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=str(HERE / "data" / "claims.jsonl"))
    parser.add_argument(
        "--backbone",
        default="xlm-roberta-base",
        help="a model pinned in backbones.py, or a directory such as the one tapt.py writes",
    )
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
        "--aspects",
        action="store_true",
        help="train an aspect head beside the subject head, answering for every subject whether "
        "the claim covers it and with what polarity",
    )
    parser.add_argument("--aspect-weight", type=float, default=1.0)
    parser.add_argument(
        "--ema",
        type=float,
        default=0.0,
        help="keep an exponential average of the weights at this decay per step and ship that "
        "instead of the last step; 0 switches it off",
    )
    parser.add_argument(
        "--keep-best",
        action="store_true",
        help="ship the weights of the epoch with the best validation macro F1 rather than the "
        "last epoch's",
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
    parser.add_argument(
        "--card-share",
        type=float,
        default=1.0,
        help="the share of the card's time the run may take, for a card other work is using: "
        "0.5 rests after each batch as long as the batch kept the card busy. Changes nothing "
        "the run learns, only how long it takes. It yields no memory: --card-memory does",
    )
    parser.add_argument(
        "--card-memory",
        type=float,
        default=None,
        help="the most the run may keep on the card, in GB, for a card other work is using. Above "
        "what the run needs live it changes nothing the run learns; below it the run fails",
    )
    parser.add_argument(
        "--recompute-activations",
        action="store_true",
        help="keep no activations on the card between the forward pass and the backward, "
        "computing them again instead: the same gradient for about a third more compute",
    )
    parser.add_argument(
        "--average-on-host",
        action="store_true",
        help="keep the averaged weights (--ema) in the machine's memory rather than on the "
        "card: the same average, a copy of every weight off the card",
    )
    parser.add_argument(
        "--cuda-graphs",
        action="store_true",
        help="record each training pass, forward and backward, as a CUDA graph and replay it: "
        "one launch a pass rather than thousands from Python, so the card runs at its own pace "
        "whatever holds the processor. Drops each epoch's short last batch",
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
        "that every run of a learning curve uses the same ones. The validation and held-out "
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
        "held-out games stay held out. Four validation games cannot settle an abstention rule: "
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
        "--no-held-out",
        dest="held_out",
        action="store_false",
        help="leave the held-out games unread. For runs that compare configurations against each "
        "other: a set used to choose between models cannot also say how the chosen one does.",
    )
    return parser.parse_args()


if __name__ == "__main__":
    run(parse())
