"""A claim names every subject it covers, and the reader learns to say so beside the first.

python -m pytest test_aspects.py
"""

from __future__ import annotations

import dataclasses

import numpy as np
import pytest
import torch

from test_claimdata import claim
from train import ASPECT_ANSWERS, Claims, aspect_loss, aspect_scores

SUBJECTS = ["audio", "controls", "story", "verdict"]


def answered(**change):
    asked = dataclasses.replace(claim(1), subject="audio", polarity="praise", **change)
    answer, known = Claims([asked], None, SUBJECTS, 16).aspect_target(asked)
    return answer.tolist(), known.tolist()


def test_every_subject_a_claim_covers_is_charged_with_its_own_polarity():
    answer, known = answered(also=(("controls", "complaint"),))
    # absent is 0, and 1 + i is POLARITIES[i]: praise 1, complaint 2.
    assert answer == [1, 2, 0, 0]
    assert all(known)


def test_a_claim_about_one_thing_is_known_to_cover_nothing_else():
    assert answered(also=()) == ([1, 0, 0, 0], [True] * 4)


@pytest.mark.parametrize("split_wrong", [False, True])
def test_a_label_from_before_the_sheet_asked_answers_its_first_subject_alone(split_wrong):
    answer, known = answered(also=None, split_wrong=split_wrong)
    assert answer[0] == 1
    assert known == [True, False, False, False], "never charged as absent where nobody said so"


def test_a_claim_with_nothing_known_costs_nothing():
    logits = torch.randn(2, len(SUBJECTS), ASPECT_ANSWERS)
    answer = torch.tensor([[1, 0, 0, 0], [0, 0, 0, 0]])
    known = torch.tensor([[True] * 4, [False] * 4])
    loss = aspect_loss(logits, answer, known)
    assert loss[0] > 0
    assert loss[1] == 0


def test_the_scores_count_what_lies_beyond_the_first_subject_on_its_own():
    claims = [
        dataclasses.replace(
            claim(1), subject="audio", polarity="praise", also=(("controls", "complaint"),)
        ),
        dataclasses.replace(claim(1, 1), subject="story", polarity="praise", also=()),
    ]
    logits = torch.full((2, len(SUBJECTS), ASPECT_ANSWERS), -9.0)
    said = [[1, 2, 0, 0], [0, 0, 1, 0]]
    for row, answers in enumerate(said):
        for subject, answer in enumerate(answers):
            logits[row, subject, answer] = 9.0
    scores = aspect_scores(logits, claims, SUBJECTS)
    assert scores["all"]["f1"] == pytest.approx(1.0)
    assert scores["beyond_the_first"]["covered"] == 1
    assert scores["beyond_the_first"]["recall"] == pytest.approx(1.0)
    assert scores["polarity_where_both_say_covered"] == pytest.approx(1.0)

    # Missing the second subject costs the part beyond the first and nothing else.
    logits[0, 1] = torch.tensor([9.0, -9.0, -9.0, -9.0])
    missed = aspect_scores(logits, claims, SUBJECTS)
    assert missed["beyond_the_first"]["recall"] == 0.0
    assert missed["per_subject"]["audio"]["f1"] == pytest.approx(1.0)


def test_a_line_per_subject_keeps_the_promise_or_is_not_drawn(tmp_path):
    from export import aspect_lines

    # Six held-out claims. Controls is covered by the three the head is surest of, and called
    # on a fourth it is not: a line at 0.75 keeps every call right. Story is never covered, so
    # no line can keep any promise about it.
    covered = np.array([0.95, 0.9, 0.8, 0.7, 0.2, 0.1])
    logits = np.full((6, len(SUBJECTS), ASPECT_ANSWERS), -50.0)
    logits[:, :, 0] = np.log(1 - covered)[:, None]
    logits[:, :, 1] = np.log(covered)[:, None]
    truth = np.zeros((6, len(SUBJECTS)), dtype=np.int64)
    truth[:3, 1] = 1
    save_folds(tmp_path, logits, truth, first=0)
    lines = aspect_lines(str(tmp_path), SUBJECTS, 0.75)
    assert lines[1] == pytest.approx(0.8)
    assert lines[2] is None


def save_folds(path, logits, truth, first, order=SUBJECTS):
    """One fold whose subject head names `first` first on every claim, in `order`'s order."""
    subject_logits = np.full((len(logits), len(order)), -5.0)
    subject_logits[:, first] = 5.0
    np.savez(
        path / "cv-0.npz",
        logits=subject_logits,
        subjects=np.array(order),
        aspect_logits=logits,
        aspect_truth=truth,
        aspect_known=np.ones(truth.shape, dtype=bool),
    )


def test_a_line_is_fitted_only_beside_the_subject_named_first(tmp_path):
    from export import aspect_lines

    # Twenty claims chiefly about controls, which the head calls covered with certainty and
    # the reader never names again, beside ten about audio where the head calls controls and is
    # right on the three it is least sure of. Counted with the first, a line at 0.85 keeps its
    # promise, 23 of 30; counted where it is used, no line can.
    logits = np.full((30, len(SUBJECTS), ASPECT_ANSWERS), -50.0)
    covered = np.full((30, len(SUBJECTS)), 0.01)
    covered[:20, 1] = 0.99
    covered[20:23, 1] = 0.85
    covered[23:, 1] = 0.9
    logits[:, :, 0] = np.log(1 - covered)
    logits[:, :, 1] = np.log(covered)
    truth = np.zeros((30, len(SUBJECTS)), dtype=np.int64)
    truth[:23, 1] = 1
    subject_logits = np.full((30, len(SUBJECTS)), -5.0)
    subject_logits[:20, 1] = 5.0
    subject_logits[20:, 0] = 5.0
    np.savez(
        tmp_path / "cv-0.npz",
        logits=subject_logits,
        subjects=np.array(SUBJECTS),
        aspect_logits=logits,
        aspect_truth=truth,
        aspect_known=np.ones((30, len(SUBJECTS)), dtype=bool),
    )
    assert aspect_lines(str(tmp_path), SUBJECTS, 0.75)[1] is None


def test_the_folds_order_of_subjects_is_remapped_onto_the_export(tmp_path):
    from export import aspect_lines

    covered = np.array([0.95, 0.9, 0.8, 0.7, 0.2, 0.1])
    logits = np.full((6, len(SUBJECTS), ASPECT_ANSWERS), -50.0)
    logits[:, :, 0] = np.log(1 - covered)[:, None]
    logits[:, :, 1] = np.log(covered)[:, None]
    truth = np.zeros((6, len(SUBJECTS)), dtype=np.int64)
    reversed_order = SUBJECTS[::-1]
    truth[:3, reversed_order.index("controls")] = 1
    save_folds(tmp_path, logits, truth, first=reversed_order.index("verdict"), order=reversed_order)
    lines = aspect_lines(str(tmp_path), SUBJECTS, 0.75)
    assert lines[SUBJECTS.index("controls")] == pytest.approx(0.8)
    assert lines[SUBJECTS.index("story")] is None
