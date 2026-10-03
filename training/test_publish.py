import pytest

import publish

COMMIT = "614241f622f53c4eeff9890bdc4f31cfecc418b3"
HASH = "a" * 64


def test_pinning_a_size_fills_its_repository_commit_and_hashes_and_nothing_else():
    source = publish.READER_RS.read_text(encoding="utf-8")
    hashes = {name: HASH for name in publish.MODEL_FILES}
    pinned = publish.pin_reader(
        source, "small", "SMALL_FILES", "someone/game-review-reader-small", COMMIT, hashes
    )
    assert publish.pins_in(pinned, "SMALL_FILES") == hashes
    assert set(publish.pins_in(pinned, "STANDARD_FILES").values()) == {""}, (
        "pinning one size must leave the other's files alone"
    )
    small = pinned.split('name: "small"')[1].split('name: "standard"')[0]
    assert '"someone/game-review-reader-small"' in small and f'"{COMMIT}"' in small
    standard = pinned.split('name: "standard"')[1]
    assert 'repository: ""' in standard.split("files:")[0]


def test_pinning_a_search_model_fills_its_repository_and_commit():
    source = publish.SEARCH_RS.read_text(encoding="utf-8")
    pinned = publish.pin_search(source, "RERANKER", "someone/steamgauge-search-reranker", COMMIT)
    reranker = pinned.split("pub const RERANKER")[1]
    encoder = pinned.split("pub const ENCODER")[1].split("pub const RERANKER")[0]
    assert '"someone/steamgauge-search-reranker"' in reranker and f'"{COMMIT}"' in reranker
    assert 'repository: ""' in encoder and 'revision: ""' in encoder


def test_the_search_pins_are_read_from_the_tool_itself():
    pins = publish.pins_in(publish.SEARCH_RS.read_text(encoding="utf-8"), "ENCODER")
    assert set(pins) == set(publish.SEARCH_FILES)
    assert all(len(digest) == 64 for digest in pins.values())


def test_only_the_named_fields_are_published():
    publish.checked([{"review_id": "1", "subject": "bugs"}], publish.LABEL_FIELDS, "labels")
    for leak in ("text", "review", "author_steamid", "author", "splitter"):
        with pytest.raises(SystemExit):
            publish.checked([{"review_id": "1", leak: "x"}], publish.LABEL_FIELDS, "labels")
    for fields in (publish.LABEL_FIELDS, publish.JUDGEMENT_FIELDS, publish.REVIEW_FIELDS):
        assert not fields & {"text", "review", "author", "author_steamid", "steamid"}


def test_a_claim_read_again_twice_goes_up_once_from_the_first_reading(tmp_path):
    game = tmp_path / "7"
    for reading, subject in (("second", "story"), ("opus", "bugs")):
        (game / reading).mkdir(parents=True)
        (game / reading / "labels.json").write_text(
            f'[{{"app_id": 7, "review_id": "r", "index": 0, "subject": "{subject}"}}]',
            encoding="utf-8",
        )
    (game / "opus" / "labels.json").write_text(
        '[{"app_id": 7, "review_id": "r", "index": 0, "subject": "bugs"},'
        ' {"app_id": 7, "review_id": "r", "index": 1, "subject": "price"}]',
        encoding="utf-8",
    )
    _, again, _, _ = publish.read_labels(tmp_path)
    assert [(row["index"], row["subject"]) for row in again] == [(0, "story"), (1, "price")]


def row(subject, polarity="praise", ambiguous=False, index=0):
    return {
        "app_id": 1,
        "review_id": "r",
        "index": index,
        "subject": subject,
        "polarity": polarity,
        "ambiguous": ambiguous,
    }


def test_kappa_is_zero_at_chance_and_one_at_perfect_agreement():
    assert publish.kappa([("a", "a"), ("b", "b")]) == 1.0
    # Two labellers who each say `a` half the time and never for the same claim agree
    # exactly as often as chance, so the figure is minus one over one: nothing above it.
    assert publish.kappa([("a", "b"), ("b", "a")]) < 0
    assert publish.kappa([]) == 0.0


def test_agreement_joins_on_the_claim_and_ignores_claims_read_once():
    first = [row("verdict", index=0), row("gameplay", "complaint", index=1), row("story", index=2)]
    again = [row("verdict", index=0), row("story", "complaint", index=1)]
    found = publish.agreement(first, again)
    assert found["claims"] == 2
    assert found["subject"] == 0.5
    assert found["polarity"] == 1.0


def test_the_card_carries_what_the_rows_say():
    card = publish.dataset_card(
        10,
        2,
        {
            "claims": 4,
            "subject": 0.75,
            "subject_kappa": 0.7,
            "polarity": 1.0,
            "polarity_kappa": 1.0,
            "ambiguous": 0.5,
            "contested_kappa": 0.4,
        },
        reviews=3,
        labellers={"model-a": 7, "model-b": 3},
    )
    assert "10 claims from 3 Steam reviews of 2 games" in card
    assert "4 claims have a second label" in card
    assert "| `subject` | 75% | 0.70 |" in card
    assert "| `ambiguous` | 50% | 0.40 |" in card
    assert "model-a (7), model-b (3)" in card
    assert "`splitter`" not in card
