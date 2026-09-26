# SteamGauge

[![CI](https://github.com/Aureliolo/steamgauge/actions/workflows/ci.yml/badge.svg)](https://github.com/Aureliolo/steamgauge/actions/workflows/ci.yml)
[![CodeQL](https://github.com/Aureliolo/steamgauge/actions/workflows/github-code-scanning/codeql/badge.svg)](https://github.com/Aureliolo/steamgauge/actions/workflows/github-code-scanning/codeql)
[![Scorecard](https://api.scorecard.dev/projects/github.com/Aureliolo/steamgauge/badge)](https://scorecard.dev/viewer/?uri=github.com/Aureliolo/steamgauge)
[![Rust](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2FAureliolo%2Fsteamgauge%2Fmain%2Frust-toolchain.toml&query=%24.toolchain.channel&label=rust&logo=rust&color=b7410e)](rust-toolchain.toml)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24c8db?style=flat&logo=tauri&logoColor=white)](https://tauri.app)
[![Training on Python](https://img.shields.io/badge/dynamic/yaml?url=https%3A%2F%2Fraw.githubusercontent.com%2FAureliolo%2Fsteamgauge%2Fmain%2F.github%2Fworkflows%2Fci.yml&query=%24.jobs.python.steps%5B1%5D.with%5B%27python-version%27%5D&label=training&logo=python&logoColor=white&color=3776ab)](training/)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-2f6f4e?style=flat)](LICENSE)

> **Unreleased and in development.** There is no installer and no release yet. This file
> describes what the tool is and how it is meant to be judged, not a running product. What
> currently works, and what every figure is currently worth, is measured by the tool itself
> and recorded next to the data it was measured from: see the reference sets under
> `reference/` and the release notes when releases begin.

A tool for finding out what players of a game actually think, rather than what the loudest
reviews say.

## The problem it solves

Steam sorts reviews by helpfulness. If you read the top forty to judge a game, you are
reading the reviews other people upvoted, which are also the longest and angriest ones. That
is a measure of agreement, not of how common an opinion is.

The gap is large enough to change conclusions. Measured on real corpora, a theme can appear
in half or more of the fifty Steam ranks most helpful and in a fifth of all of them. Both numbers
are true. Only the second is a fact about players. The tool reports the ratio between the two
as a **bias factor**, so the distortion is a number you can read rather than an argument you
have to have.

The only way to remove the argument is to hold every review and count.

## What it does

- Take one Steam app ID, or a whole list of them.
- Download **every** review for those games, not a sample, and bring the capture up to date
  later without downloading it again.
- Split each review into the points it makes and sort each point into a subject, with a
  measured error and an honest "cannot tell".
- Show, per subject, the words the praise uses and the complaints use, counted by reviewer.
- Build a picture of the game in a paragraph from those counts.
- Click through from any number, anywhere, to the actual reviews behind it.

Results come out as one self-contained page: every rate, the reviews behind it, and what the
classifier is measured to get wrong, in a single file you can open from disk, send to someone,
or print. It fetches nothing, because a corpus that never left your machine should not start
leaving it the moment somebody looks at it.

You choose which model does the sorting, and how closely it reads.

### What a percentage means here

The headline figure for a category is a **mention rate**: the share of all reviews in the
corpus that say something about it. A review that covers two topics counts towards both, so
mention rates across categories add up to more than 100%, and they are meant to. This is the
number people usually have in mind when they ask how common a complaint is.

Where a figure is not a mention rate, it is labelled. Two others appear:

- **Primary share**, the share of reviews whose single main subject is that category. These
  are exhaustive and add up to the number of reviews.
- **Claim share**, used in deep reading, where the unit counted is an individual opinion
  rather than a whole review.

No percentage is ever shown without saying which of the three it is. The point of counting
everything is lost if the denominator is ambiguous.

### The unit is a claim, not a review

A review is not one opinion. "Looks incredible, runs like a slideshow, and the story is the
best in the series" is three, about three different things, and a single vector for the whole
review is their average: a point that belongs to none of them. So every review is first split
into the separate points it makes, and a subject belongs to a point rather than to a review.

That is what keeps the arithmetic honest at both ends. A review that makes twelve points
contributes to twelve subjects instead of being flattened into one. A point carries the subjects
it names and no others, which is the part that matters more than it sounds: a two-word review
cannot be filed under four topics, because it does not contain four, and "great music, awful
controls" is one point about two things, each with what it says of it.

A review's subjects are the union of its claims' subjects, and it still counts once towards
each of them. The headline is a mention rate, a share of reviews, and it stays that way
deliberately: counting claims instead would let whoever writes most set the numbers, which is
the same distortion the tool exists to expose at the top of the pile.

### What reads a claim

A model trained on labelled claims, and nothing else. It says which subject a claim is about,
whether it is praise, a complaint or neither, and how sure it is. Below a calibrated threshold
it says nothing at all, and those claims are reported as unclassified rather than filed under
whichever category happened to be nearest.

That last sentence is the whole of what changed. The previous classifier compared a review to
twenty-four category prototypes and kept the nearest ones. A prototype comparison has no way
to express "this is about nothing", so every string got a subject: a review reading "gfg" was
filed under graphics and art, and one reading "this game is a lot of fun" under community and
players. Those are not edge cases. Steam is full of two-word reviews, and each one of them was
adding a fraction of a percent to a rate that was supposed to be a fact about players.

The model is fine-tuned from a multilingual encoder and shipped as an ONNX graph, so it runs
on the same local runtime as everything else. No API key, no network, no account. Which
encoder it starts from is decided by measurement across candidates on identical labels and an
identical split, judged on accuracy, throughput, and how well its confidence tracks whether it
is right, because a model that cannot tell when it is guessing cannot be allowed to abstain.

**The threshold is chosen by what it promises, not by how much it answers.** The obvious way
to pick one, maximising accuracy times coverage, has a degenerate optimum on a model that is
not yet good: coverage rises faster than accuracy falls all the way down, so the sweep settles
at its own floor and the model is told to answer everything. Measured, it chose 0.05, which
across twenty-five subjects is barely above the 0.04 a uniform guess scores, and a corpus of
17,596 claims came back with nothing declined at all. That is the prototype's failure wearing
a trained model's clothes. The threshold is now the one giving the most coverage at a promised
accuracy, and when no threshold reaches that accuracy the model is recorded as not good enough
rather than quietly lowered to whatever it can manage.

Some categories are defined by what a claim does *not* say. A bare verdict and a claim that
says nothing about the game are both statements that no aspect was named, and on a corpus of
real reviews they are the commonest labels there are. A verdict can also stand beside an aspect,
"a few bugs, but I still find it fun", so the verdict row counts every claim that judges the
whole game, not only the ones that do nothing else.

Where two categories genuinely overlap, the taxonomy settles it with a written rule rather
than leaving each labeller to decide: replayability and repetitiveness are amount-of-content,
balance is difficulty, animation speed is graphics, port requests are compatibility, sequel
requests are a verdict, a system nobody explains is tutorial however good the system is,
calling a game co-op is naming a kind of game and belongs to genre, and a protest about
anything the publisher did belongs to policy even when it names no particular term. The rules
ship with the taxonomy and generate the sheet every labeller works from, so a boundary can
only be defined in one place.

Categories are added when labellers report having nowhere to put something, not when
somebody thinks of one. Every category in the current sheet was asked for by the people
labelling against the previous one.

### Depth is how closely each review is read

Every review is analysed. Depth does not decide how many are included, it decides how finely
each one is taken apart. **Deep** is the default and is described above: a review becomes the
points it makes. **Shallow** treats the whole review as one point, which is faster and
systematically understates anyone who wrote more than a sentence. Neither setting drops a
review.

### Praised, criticised, or both

A thumb is attached to a review, not to a subject. Somebody who loves the art and despairs of
the framerate has one thumb and two opposite opinions, and crediting both subjects with the
same verdict is a straightforward misreading of what they wrote.

So polarity belongs to the claim, and is reported per review per subject: of the reviews that
discuss performance, the share that criticise it, the share that praise it, and the share that
do both. **Mixed** is a real answer and appears as one. It is the most interesting thing a long
review has to say, and any tool that forces it to a single sign is throwing that away.

Claim-level polarity is available underneath, for reading rather than for headlines, and is
labelled as what it is: a count of opinions, which the most talkative reviewers dominate.

### What they said about it

A fifth of reviewers complaining about performance is a count. Whether they mean stutter,
crashes or load times is the thing a reader opened the row for, and no local model is asked to
paraphrase anybody to say it. Instead, each side of a subject shows the **words that stand
out**: the terms its complaints use far more than its praise does, and the reverse, each with
the number of reviewers who used it. "stutter 84, crashes 61, memory leak 23" under
performance is what was said, counted, and every term opens onto the exact claims it was
counted from.

The comparison is complaint against praise within one subject rather than against the corpus,
because against the corpus a subject's vocabulary is mostly its own name: "fps" stands out in
every performance claim and tells nobody anything. The ranking is a log-odds z-score with a
half-count prior, so a word three reviewers used and nobody on the other side did does not
outrank one three hundred used against ten, and a side with a handful of reviews shows nothing
rather than promoting whatever those few happened to write. Counts are by reviewer, once per
review however often it repeats itself, for the same reason the headline is a mention rate.

The comparison is also made within each language and then pooled, because a library written
in thirty languages is a library whose speakers do not praise and complain in the same
proportions: against the whole other side, "historia" stood out in the praise of a story
that Spanish speakers happened to like, and said nothing but "story". Each language's praise
is compared with its own complaints, and a word a language uses either way contributes
nothing. What a subject is called in each language stays off its rows as the English label
does, read from the corpus rather than translated: "сюжет" under story says what the row said.

A term is a word or a pair of adjacent words. Chinese, which a sixth of the library is
written in, is cut into words by a dictionary (jieba, with the words of the trade added, since a
general dictionary reads 掉帧 as "drop" and "frame"); Korean is spaced words with the particle
taken off; Japanese, which has no dictionary here, is cut into pairs of adjacent characters,
the best that can be done without one. Everything the page shows is added up when a game is
read, so a change to the adding up
does not cost the hours of a reading again: `steamgauge recount` replays the stored readings
through the same counting in seconds, and refuses if this build takes a review apart
differently from the build that read it.

### Ratings that disagree with the text

A thumbs-down is not always a complaint. "0/10, haven't slept in three days" is praise wearing
a costume, and counting it as negative quietly poisons every number downstream. The reverse is
just as common: a recommendation that is really a warning not to buy yet.

These are **flagged, not filed away**. What a labeller flags is what the text does, judged
from the words alone: they are never shown whether the reviewer recommended the game, so that
they cannot be led by it, and so they are in no position to report that the two disagree.
Putting that flag next to the rating is what finds the disagreement, and that is arithmetic
rather than a judgement. The review still sorts by what it is actually about, so the praise
buried in a joke review still counts as praise of whatever it praises.

The flag lives on labelled reviews and nowhere else, which is deliberate rather than an
omission waiting to be filled. Reading it off a whole corpus needs a classifier measured to
find irony, and nothing here has measured that yet. What the reference sets buy in the
meantime is the rate: how often, in reviews drawn at random, the text and the rating point
opposite ways at all.

## How it knows whether it is right

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

### What it is worth against the alternatives

Measured over one set of 487 claims drawn from the frozen games on 2026-09-22, twenty a
subject, which chose nothing about any row. Every row abstains where it is unsure, and every
row is scored only on what it answered, because a score that quietly drops the declined claims
is a score for a classifier nobody is running.

| | answers | agreement where it answers | macro F1 |
|---|---|---|---|
| the commonest subject | never reaches the promise | 4.1% | 0.003 |
| TF-IDF bag of words | 40% | 75.1% | 0.474 |
| nearest subject centroid over an untuned encoder | 6% | 78.6% | 0.465 |
| **this reader, 560M parameters** (`e5inst-pool-qwen4b-licensed-s1`, the one that ships) | **91%** | **80.5%** | **0.751** |
| Claude Opus 5, given the same sheet | 99.4% | 87.6% | 0.875 |

The reader it replaced, `e5-29006`, answered 84% of the same claims at 78.4% (macro F1 0.691).
On the previous draw of this sample the earlier 278M reader answered 61% at 74.8% (macro F1
0.525) and the previous 560M export 79% at 77.2% (0.652); those rows are not repeated here
because they were measured on other claims. On the whole frozen set the reader that ships
answers 89.6% at 81.6%, where `e5-29006` answered 79.9% at 81.9%: a stratified sample is the
harder question, and the one above.

**Every row is the same claims, and that is not a detail.** Read on the corpus as it comes, a
quarter of which is `verdict`, the commonest-subject baseline scores 27.2% rather than 4.1% and
TF-IDF answers 42% of claims at 75.2% rather than 40%. A stratified sample is the harder
question and the useful one, because the rows a reader has to get right are the rare ones.
Both sets of figures are kept, in `reference/baselines-frontier-sample.json` and
`reference/baselines-frozen.json`, and a row from one does not belong in a table with a row
from the other.

The third row is what this project did before it trained anything, and it is why the rebuild
happened: cosine distance to a prototype cannot say "this is about nothing", so at the accuracy
it promises it can answer one claim in sixteen.

What separates the reader from the bag of words is reading each claim inside its review, a
pretrained encoder of 560M parameters, forty-two thousand labels, many of them drawn at the
subjects the reader was worst at, and a teacher: a 4B-parameter reader trained on the same
labels, whose answers on a quarter of a million unlabelled claims the small one learns from
as well. Together that is fifty-one points of coverage over the bag of words and twenty-eight
hundredths of macro F1. The teacher alone is worth four points of coverage, measured against
the same recipe taught by the small reader's own seeds instead; a sweep of everything else,
some fifty configurations measured the same way, moved nothing outside its own noise.
`DECISIONS.md` has the tables and what each change was worth on its own.

The last row is the one worth being honest about. **A frontier model asked directly is better
than this, by seven points of agreement and nine of coverage.** What it is not is
affordable: that comparison cost 389,000 tokens for 487 claims, and a single large game holds
three million claims. This reader does that game on one desktop GPU, offline, for the
electricity. The claim being made is not that a 560M-parameter model beats a frontier one. It
is that it gets most of the way there at four orders of magnitude less cost, and that it can
tell you exactly how far short it falls.

The two models in that table are deliberately different ones. The labels this reader was
trained from were written by **Claude Fable 5.1**, and the model it is measured against is
**Claude Opus 5**, which wrote none of them. A teacher scoring its own student would make the
gap meaningless, and the gap is the point.

Every figure in that table is agreement with those labels, and the labels are a model's. The
one person who has adjudicated 200 of them agrees with the labels on 65% of subjects and with
the frontier model on 64%, which is the section below and the number to hold the table
against.

That is not a projection. The library this was built against is **51 games, 7.5 million
reviews, 20.1 million claims**, all of it read by this model on one card, and the counts and
the rows behind them reconcile game by game (`--example check-readings`).

### How a person turns silver into gold

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
rather than over them, and prints the share that agrees. That share is the first number this
project can call accuracy rather than agreement.

**What it says.** One person adjudicated **200 claims** on 2026-09-21: the 30 the two
labellers were surest and still disagreed about, and 169 drawn blind, English and from the
frozen games. Against the blind answers the labels the whole silver standard is made of name
the same subject **65.1%** of the time (somewhere in 58% to 72% with 95% confidence), the
second labeller 63.9%, and where the two labellers had agreed with each other, which is 149 of
the 169, they agree with the person 68.5%. Polarity holds at 85.8%. The reader that ships
answers 93.5% of those claims and names the person's subject on **67.9%** of them (60% to
75%); the reader before it answered 81.1% at 70.1%, a difference 169 claims cannot tell from
none, and both agree with the labels on about 82% of the frozen set. Two models agree with
each other a good deal more than either agrees with a person, and "both labellers said so" is
right about two times in three. Most of the difference is the boundaries the gap list already names: content
against gameplay, genre against verdict, story against gameplay and content.

Those are the cold figures. The person then read every answer that differed from the labels
again, 77 of the 200, with both labellers' answers and the sheet's own rule for each category
on the card, and moved 49 of them, 44 of the 59 blind ones: mostly rules the sheet already had
and the person had not applied, or misses. The set as filed carries the second answer, so
against it the labels name the same subject **89.3%** of the time on the blind claims and the
reader that ships **79.9%** of what it answers (159 of 170 claims; the reader before it 76.8% of
138). Those are not blind figures and are not quoted as
accuracy; they say how far the labels and a person agree once the person is applying the same
sheet. One rule did not carry either way: the sheet files "the best roguelike out there" under
verdict, and with that sentence in view the person still read "Best Metroidvania I played" as
genre. `DECISIONS.md` has the whole pass.

### How the reference sets are made

They are a **silver standard**, not a gold one, and the distinction decides what every number
downstream may be called. A gold standard is adjudicated by people. These labels are written by
a language model reading one claim at a time, which makes the set good enough to train a model
on and to measure against, and never good enough to quote as truth. Every manifest records
`human_verified: false`, and until that changes the tool reports **agreement** and refuses the
word accuracy.

Which model wrote them is recorded per set, in `produced_by`, and printed with every result.
A set labelled by one model and a set labelled by another are not the same evidence and must
not be pooled without saying so; the sets shipped here were written by Claude Fable 5.1.

What the set spends its size on is games rather than depth. A hundred reviews of one title
would say nothing about whether a category survives contact with a corpus it was not built
from. So the set runs to thousands of labels spread across dozens of games of different genres
and different overall sentiment, and the figure it exists to produce is the one measured on a
game the model never saw. A tenth of it is labelled twice by different labellers, which is what
lets the set report its own reliability rather than only its agreement with a classifier.

The mix of languages is chosen rather than inherited. A corpus is whatever languages its
players happen to write in, and drawing straight from it would train the model mostly on
whichever one that is. Roughly seven claims in ten are English and the rest are drawn from
everything else the corpus holds, so the model is trained on the languages the reports do not
default to. Whether it reads them as well is a separate question, it is answered for four of
them, and the limits below say what the answer is.

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
  drives the splitting rules. Three rounds of them came from labellers reporting it.
- **Every game the model is measured on is read again by a different labeller, blind, in full,
  and a share of every other set is too.** That is 21,449 of the 33,615 claims the random draws
  hold, nearly two thirds, and every one of the 5,579 the frozen games hold. `steamgauge second-opinion` draws the same reviews as fresh
  batches with no labels in them, and `steamgauge compare-labels` reads the two labellings
  together. It reports each field apart from the others, because they fail
  differently: subject is a judgement about the claim, and `ambiguous` is a judgement about the
  taxonomy. Beside every percentage is Cohen's kappa, which is what the percentage cannot tell
  you: a corpus is mostly `verdict` and `offtopic`, so two labellers who never read a claim
  would still agree most of the time by landing on the commonest subject.
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

## Honest limits

- **Reports default to English, so the headline is a fact about players who write English.**
  The capture is always the whole census, every language in it, and the filter is applied when
  the counting happens rather than when the crawling does, so the choice is reversible and no
  corpus has to be downloaded twice. But a mention rate over English reviews is not a mention
  rate over players: on some corpora English is under half of what was written. Every figure
  says which set it is over, and switching the language recounts from the same capture. The
  reason for the default is that evidence nobody can read is evidence nobody can check, and
  being able to open a rate and read what is behind it is the whole design.

- **The reader is measurably worse in seven of the languages it reads, and the library is only
  half English.** The library is **52.5% English** over 7.5 million reviews, with Simplified
  Chinese at 15.5%, Russian at 6.6% and a long tail after that. The reference set drawn to
  stand for it is 71% English: a fifth more English than the corpus it speaks about, which was
  a readability choice made when the library was smaller and never re-examined.

  Measured over 32,339 claims held out by cross-validation, each answered by a model that never
  trained on the game it came from (`training/language.py`):

  | | claims | agreement | 95% interval |
  |---|---|---|---|
  | english | 22,989 | 70.4% | [69.8, 71.0] |
  | german | 1,224 | 72.2% | [69.6, 74.7] |
  | french | 810 | 72.2% | [69.0, 75.2] |
  | turkish | 399 | 71.7% | [67.1, 75.9] |
  | brazilian | 642 | 66.0% | [62.3, 69.6] |
  | spanish | 672 | 65.9% | [62.3, 69.4] |
  | russian | 1,197 | 64.6% | [61.8, 67.2] |
  | polish | 341 | 64.5% | [59.3, 69.4] |
  | japanese | 342 | 64.0% | [58.8, 68.9] |
  | **schinese** | **2,088** | **63.9%** | **[61.9, 66.0]** |
  | **koreana** | **365** | **59.5%** | **[54.3, 64.4]** |

  German, French and Turkish match English or beat it. **Simplified Chinese is 6.5 points below
  it on intervals that do not overlap, and Korean is eleven points below.** Chinese is one
  review in six of the library, so this is not a tail case: it is the second-largest language
  in the corpus, read worse than the first, and nothing on a report page currently says so.
  Russian, Spanish, Brazilian Portuguese, Polish and Japanese sit four to six points down with
  the same picture.

  Twelve of the twenty-nine languages have fewer than a hundred held-out claims and are not
  quotable at all. Of the 52 games read, three have a commonest language that is not English,
  and one of them is 63% Japanese.

  Two separate things were wrong here, and they wanted different fixes.

  **The promise was broken, and that is fixed.** The reader says a claim it answers is right
  three times in four. Measured out of fold under the rule that used to ship, a line per subject,
  Korean delivered **66.7%** against that promise, Polish 69.5%, Chinese 71.5%, Japanese 71.3%.
  The per-subject line does not carry the language gap with it, because 71% of the set is English
  and so is the line every other language is marked against. The line is now drawn per language
  as well, and a claim is answered only when it clears both: every language lands at or above
  75%, Korean at 76.0%. The reader pays for it in coverage, 80.4% down to 75.8% overall and
  Korean 73% down to 55%, which is the trade this project has taken every other time it has been
  offered. A language with fewer than a hundred labelled claims now declines rather than
  borrowing a line fitted on English, which had been answering eight Indonesian claims at 33%.

  **Most of what is left is the labels, not the reading.** Nothing above changes the agreement
  column, and the column itself was the question. Those figures are agreement with one
  labeller, so they carry that labeller's noise as well as the reader's error. Since then the
  sets have been read a second time, blind, and on the claims **two** labellers settled the
  Chinese gap is not there: 93.0% of 171 Chinese claims against 90.0% of 3,336 English ones
  over every set read twice, and over the ten frozen games, now read twice in full, 86.9% of
  153 Chinese claims against 85.5% of 2,767 English ones. What differs is the labellers, who
  agree on 89.1% of English claims and 83.8% of Chinese ones. Korean's settled sample is 35
  claims, at 80.0%, and says nothing either way yet. The Chinese samples are
  small enough to hide a point or two, and small enough to rule out the six the column shows;
  the correction a noisier language needs is a second reading of it, which is what is being
  bought, rather than a heavier gradient, which was tried and is off.

- **The model declines claims it is not sure about, and those are counted rather than hidden.**
  A claim below the threshold gets no subject and is reported as unclassified. That is a real
  answer and an honest one, but it means a mention rate is a rate over the claims the model
  would commit to. The share it declined is printed beside it, and a large one is a finding
  about the corpus rather than a footnote.

  **That share used to be most of the corpus and is now a fifth of it.** Labelled across
  fifty-one games, the model answers **80%** of the labelled claims in games it has never seen
  and agrees with a labeller on **82%** of those. Sixteen games ago it answered an eighth of
  them at 62%. What moved it, measured one change at a time on games it never saw: more labels,
  then reading each claim inside the review it came from and training at the rate that suits
  that (58% to 77%), then a backbone twice the size (77% to 84%), then nineteen thousand more
  labels, most of them drawn at the subjects it read worst, an abstention line per subject
  instead of one for all of them (84% answered at 76% agreement, to 83% at 81%), a line per
  language on top of that (83% at 81%, to 78% at 82%), and then thirteen hundred more labels
  and a splitter that cuts a lowercase review into its sentences (78% at 82%, to **80% at
  82%**, macro F1 0.652). A threshold moved to make the number look better would be the old
  classifier again, and the share it declines is still printed beside every rate.

  **The last of those spent coverage on purpose and this is what it bought.** The language line
  declined 240 answers the subject line had allowed, and those 240 were right **55%** of the
  time against 80.7% for the answered set as a whole. That is the population it was aimed at:
  answers the reader was confident enough to give in a language where the confidence was not
  earned. Macro F1 went up rather than down, 0.642 to 0.645, which is the check that the
  accuracy was not bought by quietly abandoning the rare subjects.

  The model carries what it usually declines, drawn from the folds its abstention lines were
  fitted on, so a corpus that declines far above it can be reported as a finding rather than a
  footnote. It now carries **25.0%**, where the subject-only rule carried 19.6%.

  Read across the whole library under that rule, 52 games and 20.2 million claims, the average
  game declines **22.6%** against the 25.0% it carries, the spread runs 16.6% to 31.0%, and
  exactly one game reaches the 1.2 times that trips the warning. Under the subject-only rule the
  same was true of a different one, a card game at 25.1% against a 19.6% expectation. The figure
  travels, and it is still pitched high enough to stay quiet on ordinary games.

  **The one it fires on is a different game than before, and why is the interesting part.**
  1449850 declines 31.0%, and almost none of that is the twelve languages declined outright,
  which are 1.1% of its reviews. It is that the corpus is 12% Simplified Chinese and 8% Korean,
  the two languages whose lines are highest, 0.839 and 0.929 against English's 0.660. So the
  warning now has two causes where it had one: a corpus about something the taxonomy lacks, and a
  corpus written in the languages the reference set covers worst. Those want opposite responses,
  the first a category and the second more labels, and **nothing on the page yet tells a reader
  which of the two they are looking at**.

- **A threshold chosen on a few games may not transfer to a new one.** The threshold promises
  an accuracy, and that promise is measured on the games that chose it. On eleven games the
  frozen ones delivered eighteen points less than promised; on twenty-seven they deliver four
  points more. So every figure in the model card comes from the frozen games, and the
  validation figures stay in the run record where they belong. Which games are frozen is fixed
  by a hash of each game's id, so adding games never moves one across the line.

- **The tool has to reproduce the training measurement, and when it does not the tool is
  wrong.** They are separate implementations of one question: the trainer builds the window
  around a claim in Python, the tool builds it in Rust over a corpus it split itself. Three
  ways of asking exist so that any two can disagree, and each removes a suspect: Rust over the
  exported claims, Python over the same, and the tool over its own reading of the game. On
  2026-09-12 the first two agreed to a fifth of a point and the third was eight below, which
  turned out to be the reader building its window out of the padding inside a tokenizer file.
  Nothing about the output looked wrong, which is the argument for keeping all three. Fixed,
  the tool reads the frozen games from their captures and reports 82.9% answered at 80.7% where
  Python over the exported claims reports 82.5% at 80.3%: four tenths of a point across two
  languages, two windowings and two corpora. Training's own figure is no longer the third
  opinion, because it scores at a single threshold and the reader that ships abstains per
  subject; at that single threshold it reports 90.1% at 77.2% on the same games, which is the
  cost of the lines rather than a disagreement.

- **Claim share is verbosity-weighted and never a headline.** Counting opinions instead of
  people lets whoever writes most set the numbers, which is the same distortion this tool
  exists to expose at the top of the pile. The headline is always the share of reviews.

- **A tenth of the reference set is labelled twice, and the rest is labelled once.** Reliability
  is measured on that tenth and assumed for the rest. It is a far better position than having
  no second reading at all, and it is not the same as a set where every label was adjudicated.
  The silver standard's own error is estimated rather than known, and the word accuracy still
  does not apply: the labels were written by a model, so what is measured is consistency
  between two models.

  Measured over 21,449 claims on every one of the 51 games, with the ten frozen games read
  twice in full: two labellers agree on the subject 88.6% of the time, kappa 0.87, and on
  polarity 94.9%, kappa 0.92. Those are figures a set can stand on, and they did not move when
  the set grew from ten games to thirty to fifty-one, nor when the second reading went from a
  share of each set to all of the frozen ones.

- **The contested flag measures the labeller as much as the claim.** Two labellers given the
  same definition reached for it on 27.4% and on 43.9% of the same 21,449 claims, kappa 0.46.
  The flag does find the right claims: where neither reached for it the two agree on the
  subject 98.8% of the time, where one of them did 82.4%, and where both did 72.5%. What
  differs is the bar. So a game's contested rate is not compared with another game's, and agreement is
  reported over the contested claims as a floor on how hard the taxonomy is rather than as a
  property of the corpus.

- **A labeller is told one thing about the game, and the model is told the same thing.**
  Withholding the game keeps the label answerable from what the model reads. The one exception
  is whether the store lists the game as played only in a VR headset: two of the sheet's rules
  turn on it ("if you have a headset, get this" divides no readers of a headset-only game; motion
  sickness with nothing named means a headset in one), the text rarely says it, and so the
  handout carries `headset_only` and the model's window opens with "Played in a VR headset." on
  those games. `steamgauge store-facts` asks the store and keeps the answer beside each game.
  A labeller is still given one game at a time, and a corpus with a strong accent can still give
  its game away beyond that one fact; the effect runs one way, towards labels the model cannot
  reproduce, so it understates the model rather than flattering it.

- **A claim is split mechanically, and the splitting is sometimes wrong.** Across the fifty-one
  randomly drawn sets it is **14.5%** of claims, between 6.8% and 40.4% depending on the game,
  and every rule in the splitter came from one of those reports. The commonest failure was a
  sentence that names three subjects at once: "stunning visuals, calm music, epic story" was
  one claim carrying three, so two of them went uncounted; a list of short comma-separated
  parts is now that many claims. The rate is measured rather than assumed, because it is the
  one error in this pipeline that no amount of training fixes.

  Those reports were made against six earlier sets of rules, and every round of fixes since
  answered some of them. Re-cutting all 30,414 randomly drawn claims with the splitter this
  build ships leaves **9.1%** cut the way the labeller objected to, so more than five points of
  the 14.5 are rules that have since landed (`--example stale-splits`). It is a floor rather than the
  new rate: a claim the splitter now cuts differently is not thereby cut correctly, and nothing
  here can see a claim that was cut well before a rule and badly after it.

  A label names the span of the review it was written about, not a position in a list, so the
  splitter can change under a labelled set: a label whose span this build no longer cuts as one
  claim is counted as unjoined and said, rather than scored against whatever sentence now sits
  at its old index. A reading names its claims the same way, so the two are joined by the bytes
  they cover and neither has to be told which rules cut it before it can be believed.

- **How often labellers find a claim genuinely contested varies more than the claims do.**
  28.9% overall, but from 13.5% on one game to **52.1%** on another. Some of that is the games,
  and some of it is labellers reading "two subjects both fit" more or less strictly. It is the
  clearest argument for the double-labelled tenth: contested is the one field with no way to
  check itself.

- **"Every review" means every review Valve will serve, and the request parameters decide how
  many that is.** Two API defaults quietly remove a large and biased slice. `purchase_type`
  defaults to Steam purchases only, dropping activated keys, and `filter_offtopic_activity`
  defaults to excluding review bombs. On a million-review title, asking for everything can
  return a tenth more reviews than the defaults do, and the loss is not even: substantially
  more negative reviews disappear than positive ones. A census that accepts the defaults
  understates exactly what it is trying to measure. This one always sets both.

- **The default pagination is worse than the defaults above.** Steam's `filter=all` is
  helpfulness-ranked and stalls almost immediately, returning a fraction of a percent of a
  corpus drawn entirely from the top of the pile before it starts repeating its cursor.
  Reading that would reproduce precisely the bias this tool exists to remove. The census
  paginates by creation date instead, and reports the coverage it achieved against Valve's own
  stated total as a figure you can read rather than a claim you have to trust.

- **Some reviews are genuinely unreachable, and a shortfall must never be blamed on that
  without checking.** Reviews from deleted and private accounts are counted in Valve's totals
  and cannot be retrieved by anyone. Steam separately, and intermittently, just stops serving
  a window early: pages arrive full, then one arrives empty long before the window is
  exhausted, which looks exactly like reaching the end. Walking the same window again returns
  everything. A crawler that accepts the first answer therefore undercounts by a quarter of a
  window at a time while reporting every shard as finished, so this one compares each window
  against Valve's count for it, walks it again when it lands short, and refuses to mark a
  window complete while it stays short. Coverage is reported per corpus either way, because
  the first number is the one worth doubting.

- **The target moves.** New reviews arrive constantly and old ones are edited, so a corpus is
  a snapshot with a timestamp, and it is brought up to date rather than rebuilt: one walk in
  last-edit order fetches everything written or changed since, and stops there. Nothing
  already captured is overwritten. An edited review is held in both forms, with a record of
  which one counts, so a capture never loses what a review used to say. Deleted reviews and
  drifting vote counts are what a sweep cannot see, and only a full re-crawl catches those.

- **A category with no labelled examples is only as good as its description.** Fitting cannot
  invent evidence. Where a game barely discusses something, that category stays weak for that
  game, and the fix is labelled examples from a game that does discuss it, not a cleverer
  scoring rule.

- **Counting words is not understanding them.** A category assignment is a useful summary and
  a starting point for reading, never a substitute for it. That is why drilling down to the
  underlying reviews is a first-class feature and not an afterthought.

## Data and privacy

This repository holds no review data. Reviews belong to the people who wrote them and to
Valve, and they are downloadable by anyone with the app ID, so there is nothing to gain from
redistributing them here.

By default nothing you pull leaves your machine. The crawler talks to Valve, and the reading
runs on a model on your own card, so a complete census is possible with no account, no key and
no network beyond Steam itself. Reports are the same: one file with no
fonts, scripts or stylesheets fetched from anywhere, so reading a result is not a way of
publishing it.

Configuring a hosted model changes that in a specific and bounded way: the text of a sampled
subset of reviews is sent to that provider to induce categories, label examples and write
summaries. The full corpus is never sent. Which provider, and whether to use one at all, is
your choice, and the tool records what ran against every number it reports.

Steam review text, author IDs and profiles are public. A local corpus of millions of accounts
is still personal data, so it stays on your disk, and exporting a corpus offers
de-identification.

## Licence

Apache License 2.0. See [LICENSE](LICENSE).
