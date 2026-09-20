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
- **`claims/<app id>/second/labels.json`** is a tenth of that set labelled again by a
  different labeller working blind. `steamgauge compare-labels` reads the two together; a set
  labelled once cannot say anything about its own reliability.
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

## What used to be here

Two sets were removed once nothing could read them. The reasoning they were kept for is in
`DECISIONS.md`, which is prose rather than data and says more than the labels did; the labels
themselves are in the history if anyone needs to see one.

Review-level labels, one subject for a whole review plus a secondary, superseded when the unit
became the claim: a review that praises the art and damns the framerate says two things and a
single label records one of them. And claim-level labels written against a sheet whose
categories this build no longer has, which it refused on sight, that being the version guard
working rather than failing.
