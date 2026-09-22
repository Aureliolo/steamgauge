"""What the loss is charged against when a claim has been read twice.

python -m pytest training/test_targets.py
"""

from __future__ import annotations

import dataclasses
import json

import torch

import claimdata
from test_claimdata import claim
from train import Claims, charged


class Tokenizer:
    """Enough of a tokenizer for the dataset to build an item: the target needs no text."""

    def __call__(self, *texts, **_):
        return {
            "input_ids": torch.zeros(1, 4, dtype=torch.long),
            "attention_mask": torch.ones(1, 4, dtype=torch.long),
            "offset_mapping": [],
        }


SUBJECTS = ["verdict", "gameplay", "story"]


def dataset(claims, second_weight):
    return Claims(claims, Tokenizer(), SUBJECTS, 4, second_weight=second_weight)


def test_a_claim_read_once_is_a_one_hot_target_whatever_the_weight():
    item = dataset([claim(1)], 0.5)[0]
    assert item["subject_target"].tolist() == [1.0, 0.0, 0.0]
    assert item["polarity_target"].tolist() == [1.0, 0.0, 0.0]


def test_a_claim_read_twice_mixes_the_two_answers_by_the_weight():
    twice = dataclasses.replace(claim(1), second_subject="story", second_polarity="neutral")
    item = dataset([twice], 0.25)[0]
    assert item["subject_target"].tolist() == [0.75, 0.0, 0.25]
    assert item["polarity_target"].tolist() == [0.75, 0.0, 0.25]
    # The hard label the validation figures read is still the first labeller's.
    assert int(item["subject"]) == 0


def test_two_labellers_who_agree_leave_the_target_whole():
    twice = dataclasses.replace(claim(1), second_subject="verdict", second_polarity="praise")
    item = dataset([twice], 0.5)[0]
    assert item["subject_target"].tolist() == [1.0, 0.0, 0.0]


def test_the_weight_off_ignores_the_second_reading():
    twice = dataclasses.replace(claim(1), second_subject="story")
    item = dataset([twice], 0.0)[0]
    assert item["subject_target"].tolist() == [1.0, 0.0, 0.0]


def test_a_one_hot_target_is_charged_exactly_what_the_hard_form_charges():
    logits = torch.tensor([[2.0, -1.0, 0.5], [0.1, 0.2, 3.0]])
    labels = torch.tensor([0, 2])
    class_weight = torch.tensor([1.0, 3.0, 0.5])
    hard = torch.nn.functional.cross_entropy(logits, labels, weight=class_weight, reduction="none")
    soft = charged(logits, torch.nn.functional.one_hot(labels, 3).float(), class_weight)
    assert torch.allclose(hard, soft)


def test_the_export_row_carries_the_second_reading_when_there_is_one(tmp_path):
    rows = [
        {
            "text": "a",
            "review": "a",
            "review_offset": 0,
            "subject": "verdict",
            "app_id": 1,
            "review_id": "r",
            "claim_index": 0,
            "second_subject": "story",
            "second_polarity": "complaint",
        },
        {
            "text": "b",
            "review": "b",
            "review_offset": 0,
            "subject": "verdict",
            "app_id": 1,
            "review_id": "r",
            "claim_index": 1,
            "second_subject": None,
            "second_polarity": None,
        },
    ]
    path = tmp_path / "claims.jsonl"
    path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")
    first, second = claimdata.load(path)
    assert (first.second_subject, first.second_polarity) == ("story", "complaint")
    assert second.second_subject is None
