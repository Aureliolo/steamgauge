# How it knows whether it is right

A census that cannot say how often it is wrong is just an opinion with decimal places.

The model is measured against **reference sets**: claims labelled one at a time, stored under
`reference/claims/<app id>/` with the drawn sample beside them. Whole games are held out rather
than whole claims, because two claims from one review are not independent evidence and a score
that mixes them is a score for how well the model repeats itself. A report scores a game
against its own set only when the model never trained on it: on the games it learned from it
reproduces its labels at 99%, and a page that printed that as agreement would advertise its
memory.

Three things are reported together, and separating them is what makes the number mean anything:

- **Agreement**, over the claims the model was willing to answer for.
- **Abstention**, the share it declined. A score that quietly drops those is a score for a
  classifier nobody is running.
- **Contest**, the share the labeller marked as genuinely ambiguous, reported apart from the
  rest. Disagreement there says as much about the taxonomy as about the model.

## What it is worth against the alternatives

Measured over one set of claims drawn from the held-out games, twenty a subject, which chose
nothing about any row: 458 claims, labelled under the category sheet as it stands. The key is
`reference/frontier-key.json`, and the frontier model's answers are
`reference/frontier-opus-answers.json`. Every row abstains where it is unsure, and every row is
scored only on what it answered, because a score that quietly drops the declined claims is a
score for a classifier nobody is running. The readers are scored on DirectML, where the tool
runs them, and the baselines are fitted on the same labels the reader learned from.

| | answers | agreement where it answers | macro F1 |
|---|---|---|---|
| the commonest subject | never reaches the promise | 5.9% | 0.004 |
| TF-IDF bag of words | 48% | 75.9% | 0.508 |
| nearest subject centroid over an untuned encoder | 3% | 83.3% | 0.448 |
| **this reader, 560M parameters** (`e5inst-pool-person-rules-s1`, the one that ships) | **93%** | **80.7%** | **0.765** |
| Claude Opus 5, given the same sheet | 99.3% | 85.9% | 0.860 |

On the whole held-out set, 5,080 claims as they come, the reader that ships answers 91.5% at 81.5%
(macro F1 0.707): a stratified sample is the harder question, and the one above.

**Every row is the same claims, and that is not a detail.** Read on the corpus as it comes, a
quarter of which is `verdict`, the commonest-subject baseline scores 28.8% rather than 5.9% and
TF-IDF answers 46% of claims at 75.1% rather than 48%. A stratified sample is the harder
question and the useful one, because the rows a reader has to get right are the rare ones.
Both sets of figures are kept, in `reference/baselines-frontier-sample.json` and
`reference/baselines-held-out.json`, and a row from one does not belong in a table with a row
from the other.

The third row is why the reader is trained rather than matched against descriptions: cosine
distance to a prototype cannot say "this is about nothing", so at the accuracy it promises it
can answer about one claim in forty.

What separates the reader from the bag of words is reading each claim inside its review, a
pretrained encoder of 560M parameters, forty-five thousand labels, many of them drawn at the
subjects the reader was worst at, and a teacher: a 4B-parameter reader trained on the same
labels, whose answers on a quarter of a million unlabelled claims the small one learns from as
well. Together that is forty-five points of coverage over the bag of words
and a quarter of macro F1. The teacher alone is worth four points of coverage, measured against
the same recipe taught by the small reader's own seeds instead; a sweep of everything else,
some fifty configurations measured the same way, moved nothing outside its own noise.
`DECISIONS.md` has the tables and what each change was worth on its own.

The last row is the one worth being honest about. **A frontier model asked directly is better
than this, by five points of agreement and seven of coverage.** What it is not is
affordable: its answers cost 389,000 tokens for 487 claims, and a single large game holds
millions of claims. This reader does that game on one desktop GPU, offline, for the
electricity. The claim being made is not that a 560M-parameter model beats a frontier one. It
is that it gets most of the way there at four orders of magnitude less cost, and that it can
tell you exactly how far short it falls.

The two models in that table are deliberately different ones. The labels this reader was
trained from were written by **Claude Fable 5.1**, and from 27 September 2026 by **Claude Opus
5.5**; the model it is measured against is **Claude Opus 5**, which wrote none of them. A
teacher scoring its own student would make the gap meaningless, and the gap is the point.

Every figure in that table is agreement with those labels, and the labels are a model's. The
one person who has adjudicated 200 of them, answering cold, agrees with the labels on 65% of
subjects and with the second labeller on 64%, which is the section below and the number to hold
the table against.

## The reader in each size

The reader comes in more than one size, and the desktop app picks the largest one the machine's
graphics card has room for. Where there is no card to use it starts with the smallest and offers
the most accurate beside it, saying how long each would take on that computer. Every size below
learned from the same labels and the same teacher, a 4B, and every figure in a column was
measured the same way for every row. There is no larger size because the teacher itself, trained
on exactly these labels, answers its surest claims less accurately than `standard` does.

<!-- reader sizes: rendered from reference/reader-sizes.json by training/sizes.py -->
| Reader | Parameters | Download | Accuracy at 80% coverage | Accuracy at 90% coverage | GPU memory | One large game on the GPU | One small game on the CPU |
|---|---|---|---|---|---|---|---|
| small | 118M | 244 MB | 77.9% | 73.8% | 1.2 GB | 62 s | 36 s |
| standard | 559M | 1.1 GB | 84.7% | 82.0% | 2.6 GB | 125 s | 285 s |

- **Accuracy at a coverage:** 458 claims from ten games held out of training, labelled under the
  current category sheet. Each reader answers the given share of claims it is most confident about,
  scored on DirectML against `reference/frontier-key.json` (`frontier.py reader`).
- **GPU memory and one large game on the GPU:** one read of game 920210 (117,664 claims) through the
  desktop app on DirectML, on an otherwise idle NVIDIA GeForce RTX 4090. Memory is the peak during
  the read minus what the GPU held before.
- **One small game on the CPU:** 1,416 claims of game 1888930 on the CPU only, loading included, on
  an AMD Ryzen 9 5950X with other work running.
<!-- end of reader sizes -->

## How a person turns silver into gold

`steamgauge gold` writes one page, holding a blind random sample of claims from the games the
model never saw and the claims two labellers answered differently. Blind means blind: a claim
drawn for measurement carries no answer, because an answer on the page is an answer in the
reader's head, and a figure produced by agreeing with a suggestion is a ratification rather
than a measurement.

The page marks each claim inside the review it came from and keeps answers as they are made. A
letter picks a subject, a digit picks the polarity, and a claim with both moves on by itself; a
thousand claims is not one sitting and a closed tab must not cost a night's work.

**`steamgauge gold --serve` is the way to run it.** The page is served from the loopback
address and every answer lands in a file on disk before the next question is drawn, so the disk
is the copy that matters and the browser is a cache: reopen it anywhere and it carries on where
the file ends. Nothing leaves the machine, and there is nothing to remember to press. Opened as
a plain file instead, the page still works and still asks nothing of the network, but the
browser is then the only copy until the Export button is pressed, which is a bad place for the
only copy of somebody's own judgement.

`steamgauge ingest-gold` reads those answers back, files them beside the labels already there
rather than over them, and prints the share that agrees. That share is the one number here
that may be called accuracy rather than agreement.

**What it says.** One person has adjudicated **200 claims**: 169 drawn blind, in English, from
the held-out games, and 30 the two labellers were sure of and still disagreed about. Answering
cold, before seeing any label, the person named the same subject as the labels the silver
standard is made of on **65.1%** of the blind claims (58% to 72% with 95% confidence), and as
the second labeller on 63.9%; where the two labellers agree with each other, 149 of the 169,
they agree with the person 68.5%. Polarity holds at 85.8%. Two models agree with each other a
good deal more than either agrees with a person, and "both labellers said so" is right about two
times in three. Most of the difference sits at the boundaries the gap list already names:
content against gameplay, genre against verdict, story against gameplay and content. Those
first answers are the gold set as committed in `4c07445`.

Every answer that differed from the labels was then read again, with both labellers' answers and
the sheet's own rule for each category on the card, and the set as filed carries the second
answer. Most of the changes were rules the sheet already had and the person had not applied. One
rule did not carry: the sheet files "the best roguelike out there" under verdict, and with that
sentence in view the person still read "Best Metroidvania I played" as genre. Against the set as
filed the labels name the person's subject on **82.2%** of its 197 claims. That figure is not
blind and is not quoted as accuracy; it says how far the labels and a person agree once the
person is applying the same sheet.

**Right, or something the person would also accept.** A claim can fairly sit under two
subjects, and exact agreement counts every second subject as wrong. So the person has also
judged, for each gold claim where the reader named another subject, whether that one would also
do. Against the 151 blind claims as filed, the reader that ships names the person's own subject
on **84.8%** of those it answers (78% to 90%) and one the person accepts on **98.6%**; on the 46
claims the two labellers disagreed about, 50.0% and 92.9%. `DECISIONS.md` has the whole pass.

## How the reference sets are made

They are a **silver standard**, not a gold one, and the distinction decides what every number
downstream may be called. A gold standard is adjudicated by people. These labels are written by
a language model reading one claim at a time, which makes the set good enough to train a model
on and to measure against, and never good enough to quote as truth. Every manifest records
`human_verified: false`, and until that changes the tool reports **agreement** and refuses the
word accuracy.

Which model wrote them is recorded on every label, in `produced_by`, and printed with every
result. A set labelled by one model and a set labelled by another are not the same evidence and
must not be pooled without saying so; the sets shipped here were written by Claude Fable 5.1,
and every label written from 27 September 2026 by Claude Opus 5.5.

What the set spends its size on is games rather than depth. A hundred reviews of one title
would say nothing about whether a category survives contact with a corpus it was not built
from. So the set runs to thousands of labels spread across dozens of games of different genres
and different overall sentiment, and the figure it exists to produce is the one measured on a
game the model never saw. Nearly two thirds of it is labelled twice by different labellers,
which is what lets the set report its own reliability rather than only its agreement with a
classifier.

The mix of languages is chosen rather than inherited. A corpus is whatever languages its
players happen to write in, and drawing straight from it would train the model mostly on
whichever one that is. Roughly seven claims in ten are English and the rest are drawn from
everything else the corpus holds, so the model is trained on the languages the reports do not
default to. Whether it reads them as well is a separate question, it is answered for four of
them, and [Honest limits](limits.md) says what the answer is.

The protocol is fixed so it can be repeated, and so a disagreement with it is about the method
rather than about somebody's afternoon:

- **The sample is drawn before anyone reads anything.** `steamgauge sample-claims` takes a seed and
  draws reviews per game with a fixed share of English, then splits each into its claims. The
  same seed against the same capture draws the same reviews, so a set can be rebuilt without
  being stored.
- **Every claim of a drawn review is labelled, never a subset of them.** A review labelled in
  part cannot say what share of a corpus names no aspect at all, which is the first thing worth
  knowing about one.
- **A set drawn to teach the model is marked as one and measures nothing.** A random draw
  spends most of its budget on claims the reader already gets right, so `steamgauge declined`
  draws instead from the claims it abstained on, uniformly rather than from the least confident
  of them, because the bottom of a confidence ordering is mostly text with nothing in it. Every
  row lands with `subset: declined`, which keeps it out of every prevalence figure; only the
  claims drawn are asked about, though the whole review is still handed over, because a claim
  reading "it doesn't" cannot be labelled without the sentence before it; and only games the
  model already trains on may be drawn, because a held-out game taught from is not held out.
- **Labellers are shown the claim inside the review it came from, and nothing else.** Not the
  game, not whether the reviewer recommended it, not what the model guessed. The prediction is
  withheld because anyone shown a proposed answer agrees with it more than someone reading
  cold. The rating and the game are withheld for a different reason: the model does not see
  them either, so a label made from more than the tool can read would measure the gap in what
  the two were shown. The surrounding review is shown because a claim reading "it doesn't" is
  not interpretable alone.
- **The sheet every labeller works from is generated from the taxonomy**, so a boundary rule
  exists in exactly one place and every labeller is given the same one. It is never changed
  mid-run: half a set labelled against a revised sheet is half a set nobody can compare.
- **Labelling runs in parallel, one labeller per game**, each working batch by batch and writing
  each batch out before opening the next.
- **Every label carries seven fields**: the subject the claim is chiefly about, whether it is
  praise, a complaint or neither, every other subject it covers with a polarity for each, whether
  the text is ironic, how sure the labeller was, whether the call was genuinely contested, and
  whether the claim was cut in the wrong place. The last two are read back:
  agreement is reported separately over the contested claims, and the mis-split rate is what
  drives the splitting rules.
- **Every game the model is measured on is read again by a different labeller, blind, in full,
  and a share of every other set is too.** That is 21,449 of the 33,615 claims the random draws
  hold, nearly two thirds, and every one of the 5,579 the held-out games hold.
  `steamgauge second-opinion` draws the same reviews as fresh batches with no labels in them,
  and `steamgauge compare-labels` reads the two labellings together. It reports each field apart
  from the others, because they fail differently: subject is a judgement about the claim, and
  `ambiguous` is a judgement about the taxonomy. Beside every percentage is Cohen's kappa, which
  is what the percentage cannot tell you: a corpus is mostly `verdict` and `offtopic`, so two
  labellers who never read a claim would still agree most of the time by landing on the
  commonest subject.
- **What comes back is checked rather than trusted.** `steamgauge ingest-claims` refuses a set that
  does not cover the drawn sample exactly: claims nobody labelled, labels naming claims nobody
  drew, subjects the taxonomy does not have, claims labelled twice, and labels whose judgements
  were never made. A judgement left out is dropped rather than defaulted, because a `false`
  nobody wrote is a figure nobody stood behind.

These rules keep those figures honest:

- **Agreement is not accuracy.** Where the labels were written by a model, what gets measured
  is consistency between two models, and the tool says so on every report until a person has
  checked part of the set. Two models can be wrong together, most easily on sarcasm and on the
  boundaries between categories, which is exactly where classifiers are weakest.
- **Every rate carries a 95% interval.** Reference sets are small. At fifty reviews the honest
  band around a rate is roughly twenty-five points wide, and a bare percentage invites
  conclusions the counts cannot support.
- **A category that is wrong says what it is wrong about.** Knowing a category scores badly
  says nothing about what to do; knowing it is read as one particular other category is a
  boundary the taxonomy has not settled, and no amount of fitting will settle it. Every
  evaluation names the category each one is most often mistaken for, where that is a pattern
  rather than a single review.
- **Every row says how well it is measured, not just the page.** One agreement figure for a
  whole taxonomy hides the shape of the error: the same run finds nine mentions in ten of one
  category and one in five of another. Each category carries its own precision and recall
  from the reference set, and a category the classifier is measured to miss most of is marked
  as a floor rather than a count.
- **A difference the reader cannot see is not marked as one.** The page draws two kinds of
  claim on top of its numbers, and each is earned rather than automatic. Across games, a
  category is outlined as belonging to one of them only where that game clears every other by
  more than the rounding in the printed figures. Within a game, a category is coloured as warm
  or cold only where the interval on the reviews raising it clears the game's own baseline, so
  eight reviews all recommending the game is left as the 100% it is instead of dressed as the
  finding a thousand reviews at 98% would be.
- **Reference sets span several games, and whole games are held out.** A model trained on one
  game carries that game's vocabulary. Sets are labelled across games of different genres and
  different overall sentiment, and the split holds out whole games rather than whole claims,
  so the figure says whether a subject travels rather than whether the model can repeat itself.
  Numbers from a single corpus describe that corpus and nothing else. The same figure is
  reported again over the claims the labeller called clear-cut and over the ones it called
  contested, because those answer different questions: a model that genuinely cannot reach a
  game is worse where the reading was easy, and a taxonomy that does not fit a game is worse
  only where the labeller could not place the claim either.
