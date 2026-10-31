# What is in here

Labels, and the sheets they were written from. No review text is committed. A label names a
review by its Steam id and the byte span of its claim, so anyone can fetch it from Steam. The
drawn samples and batch handouts hold text and stay out of the repository.

## The live set

- **`claims/<app id>/labels.json`** is the reference set, covering every claim of every drawn
  review. Each label gives the subject, side, irony, sureness, whether it is contested and
  whether the claim was cut wrongly.
- Each label names the bytes it covers, the wording its labeller read, and in `produced_by` the
  labeller who wrote it. No label records which rules cut its claim.
- **`claims/<app id>/second/labels.json`** is the same set labelled again, blind, by another
  labeller: every claim of the held-out games and a share of every other game's.
  `steamgauge compare-labels` reads the two together.
- **`claims/<app id>/gold/labels.json`** is what a person adjudicated, written by
  `steamgauge ingest-gold`, and nothing trains on it. `steamgauge measure-claims --labels gold`
  is the only measurement here that is accuracy.
- **`claims/<app id>/declined/labels.json`** is a teaching set of claims the reader abstained on,
  drawn by `steamgauge declined` for training games only. Every row carries `subset: declined`,
  and nothing that measures prevalence or accuracy reads it.
- **`induced/<app id>.json`** holds the subjects a game's players raise that the taxonomy has no
  row for, each with the reviews that raise it. A subject needs three reviews.
- **`claim-brief.txt`**, **`labelling-brief.txt`** and **`induction-brief.txt`** are generated
  from `taxonomy.rs` by `steamgauge brief`. Edit the taxonomy, not the briefs.

## The benchmark

- **`frontier-key.json`** holds 487 claims from the held-out games, twenty a subject, each keyed
  to a subject and polarity under the current sheet. The
  [comparison table](../docs/accuracy.md#against-the-alternatives) and the sizes table are
  scored on it.
- **`training/frontier.py reader --key`** scores a reader on the key's claims this build still
  cuts.
- **`frontier-opus-answers.json`** holds Claude Opus 5's answers to the same claims, given the
  same sheet. `training/frontier.py score` scores them.
- **`baselines-*.json`** hold the baselines and readers scored on that key and on the held-out
  games.
- **`reader-sizes.json`** holds the figures the [sizes table](../docs/accuracy.md#each-size) and
  every model card's table are rendered from.
