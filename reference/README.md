# What is in here

Labels, and the sheets they were written from. No review text: a label names a review by its
Steam id and the byte span of the claim inside it, so anyone can fetch the review from Steam
and cut the claim out themselves. The drawn samples and batch handouts do hold text, and are
not committed.

## The live set

- **`claims/<app id>/labels.json`** is the reference set: every claim of every drawn review,
  labelled with the subject it is about, whether it is praise or a complaint, whether it is
  ironic, how sure the labeller was, whether the call was genuinely contested, and whether the
  claim was cut in the wrong place. Each label names the bytes it covers, the wording its
  labeller read, and in `produced_by` the labeller who wrote it. Which rules cut the claim is
  not recorded: whether this build still cuts one over those bytes is a question the corpus
  answers, and a stamp saying so would be a second thing to keep in step. Two labellers
  disagree with each other about as often as either disagrees with the truth, so a set that
  could not say which one wrote a label could not be split back apart.
- **`claims/<app id>/second/labels.json`** is that set labelled again by a different labeller
  working blind: every claim of the frozen games, and a share of every other game's.
  `steamgauge compare-labels` reads the two together; a set labelled once cannot say anything
  about its own reliability.
- **`claims/<app id>/gold/labels.json`** is what a person adjudicated, written by
  `steamgauge ingest-gold` from the page `steamgauge gold` produces. It is the only labelling
  in this repository not made by a model, and `steamgauge measure-claims --labels gold` is the
  only measurement here that may be called accuracy rather than agreement. It sits beside the
  set's own labels rather than over them: the point is to compare the two, and overwriting the
  labeller's answer would destroy the comparison being made. Nothing trains on it, and nothing
  should: these are the claims the model is measured against, and a model trained on its own
  test set reports a number about itself.
- **`claims/<app id>/declined/labels.json`** is a teaching set: claims the reader abstained on,
  drawn by `steamgauge declined` and labelled like any other. Every row carries
  `subset: declined`, and nothing that measures prevalence or accuracy reads it, because a set
  drawn for being hard cannot say what share of a corpus mentions price. It exists only for
  training, and only for games the model already learns from: drawing one from a held-out game
  would end the measurement it exists to protect. Only the drawn claims are labelled, though
  the whole review is handed over, since a claim reading "it doesn't" cannot be read alone.
- **`induced/<app id>.json`** holds the subjects a game's own players raise that the fixed
  taxonomy has no row for, each with the reviews that raise it. A subject with fewer than
  three reviews behind it is refused rather than kept with a note.
- **`GAPS.md`** is what the labellers reported: where the sheet contradicted itself, where two
  subjects both fitted, where the splitter cut in the wrong place. Every rule in the current
  splitter and every line waiting for the next taxonomy revision came from an entry here.
- **`claim-brief.txt`**, **`labelling-brief.txt`** and **`induction-brief.txt`** are generated
  from `taxonomy.rs` by `steamgauge brief`. Edit the taxonomy, not the briefs.

## The benchmark

- **`frontier-key.json`** is the benchmark the README's comparison table and every reader's
  accuracy at a coverage are scored on: 487 claims drawn from the frozen games, twenty a
  subject, each named by its game, its review and its label's index, with the subject and
  polarity it is keyed to under the sheet as it stands. `training/frontier.py reader --key`
  scores a reader on it, over the claims this build still cuts.
- **`frontier-opus-answers.json`** is Claude Opus 5's answers to the same claims, given the
  same sheet; `training/frontier.py score` scores them.
- **`baselines-*.json`** are the baselines and readers scored on that key and on the frozen
  games, and **`reader-sizes.json`** the figures the sizes table on the README and on every
  model card is rendered from.
