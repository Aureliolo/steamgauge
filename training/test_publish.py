import publish


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
        "abcd",
        {
            "claims": 4,
            "subject": 0.75,
            "subject_kappa": 0.7,
            "polarity": 1.0,
            "polarity_kappa": 1.0,
            "contested_kappa": 0.4,
        },
        unstamped=1,
    )
    assert "4 of the claims are labelled a second time" in card
    assert "75% of the time (Cohen's kappa 0.70)" in card
    assert "1 of the second readings predate" in card
    assert "`splitter`" not in card, (
        "no row carries a splitter field; the card must not promise one"
    )
