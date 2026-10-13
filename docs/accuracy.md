# Accuracy

The reader is measured on games it never trained on. Its labels come from a language model, so
most figures here are agreement with those labels; the figures checked against a person are
marked as such.

## Against the alternatives

458 claims from the ten held-out games, twenty per subject. Every row answers only where it is
sure enough and is scored on what it answers. The answers are in `reference/frontier-key.json`.

| | Answers | Agreement where it answers | Macro F1 |
|---|---|---|---|
| The commonest subject | never reaches three in four | 5.9% | 0.004 |
| TF-IDF bag of words | 48% | 75.9% | 0.508 |
| Nearest subject over an untuned encoder | 3% | 83.3% | 0.448 |
| **The reader, 560M parameters** | **93%** | **80.7%** | **0.765** |
| Claude Opus 5, given the same sheet | 99.3% | 85.9% | 0.860 |

On all 5,080 held-out claims as they come, the reader answers 91.5% and agrees on 81.5% of
those (macro F1 0.707).

Claude Opus 5 does better, at about 800 tokens a claim; a large game holds millions of claims,
which the reader reads on one graphics card, offline. The labels were written by Claude Fable 5.1
and, from 27 September 2026, Claude Opus 5.5, so the model it is compared with wrote none of them.

## Each size

The app uses the largest size the graphics card has room for, and without one starts with the
small one and offers the standard one.

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

## Checked by a person

One person answered 200 claims on the page `steamgauge gold --serve` writes: 169 drawn blind, in
English, from the held-out games, and 30 the two labellers disagreed about.

| | Agreement |
|---|---|
| The person and the labels, answering blind | 65.1% (58% to 72%) |
| The person and the second labeller, blind | 63.9% |
| The person and the labels on praise or complaint | 85.8% |
| The reader names the person's subject, of 151 blind claims | **84.8%** (78% to 90%) |
| The reader names a subject the person accepts | **98.6%** |
| The same two, on the 46 claims the labellers disputed | 50.0% and 92.9% |

A claim can fairly sit under two subjects, so the person also judged whether the reader's
subject, where it differed, would do as well. The reader's figures count the set as filed after
the person read each difference again beside the sheet's rules.

## In each language

40,327 claims, each answered by a reader that never trained on its game, over every claim whether
or not the reader would answer it. The last column keeps only the claims the labeller was sure of.

| Language | Claims | Agreement | 95% interval | Labeller sure |
|---|---|---|---|---|
| English | 28,250 | 75.8% | 75.3 to 76.3 | 93.0% |
| Turkish | 529 | 76.0% | 72.2 to 79.4 | 95.6% |
| German | 1,604 | 75.2% | 73.0 to 77.2 | 92.0% |
| Polish | 354 | 74.6% | 69.8 to 78.8 | 92.4% |
| French | 1,089 | 74.2% | 71.5 to 76.7 | 89.4% |
| Brazilian Portuguese | 868 | 73.5% | 70.5 to 76.3 | 91.7% |
| **Russian** | **1,508** | **71.4%** | **69.1 to 73.6** | **92.1%** |
| **Spanish** | **841** | **70.2%** | **67.0 to 73.1** | **87.4%** |
| **Japanese** | **489** | **69.9%** | **65.7 to 73.8** | **84.5%** |
| **Simplified Chinese** | **2,573** | **69.6%** | **67.8 to 71.3** | **89.6%** |
| **Korean** | **518** | **65.3%** | **61.1 to 69.2** | **88.3%** |

The five in bold read below English. The reader keeps its three-in-four promise in each by
answering less in them: a Korean or Spanish claim has to clear a line three times the English
one. A language with fewer than 100 labelled claims declines rather than borrowing English's line.

## How the labels are made

- `sample-claims` draws reviews by a seed, seven in ten English, and splits them into claims.
  Every claim of a drawn review is labelled.
- A labeller sees each claim inside its review and nothing else: not the game, the rating or the
  reader's answer. The one exception is whether the store lists the game as VR-only, which
  `store-facts` records and the reader is told as well.
- Every labeller works from one sheet, which `brief` writes from the taxonomy.
- `ingest-claims` refuses a returned set that does not cover the draw exactly.
- `second-opinion` and `compare-labels` have a second labeller read part of every set, and every
  held-out game in full: 21,449 of 33,615 claims. They agree on the subject 88.6% of the time
  (Cohen's kappa 0.87) and on praise or complaint 94.9% (0.92).
- `declined` and `rare-claims` draw teaching sets where the reader is weakest. They are marked as
  such and never measured on.
- `gold` and `ingest-gold` take a person's answers; `measure-claims` and `ceiling` score readings.
- Whole games are held out, chosen by a hash of their app ID, so two claims from one review never
  sit on both sides.

## Limits

- **English by default.** Figures count English reviews unless you choose otherwise, and on some
  games English is under half of what was written. Every figure names the language it counts, and
  switching recounts from the same download.
- **Unfiled claims.** A rate is a share of the claims the reader would answer. It usually
  declines about 10.9% of claims; a game that declines 1.2 times that and five points more is
  flagged, with the languages behind it.
- **Labels from a model.** Most figures here are agreement with a language model's labels; only
  [the person's](#checked-by-a-person) are accuracy.
- **Mechanical splitting.** Reviews are split into claims by rules, and 9.1% of 30,414 randomly
  drawn claims are still cut where a labeller said they should not be. Training does not fix
  that.
- **Contested is partly the labeller.** Two labellers marked 27.4% and 43.9% of the same claims as
  genuinely ambiguous, so one game's contested rate is not compared with another's.
- **Thin subjects stay thin.** A subject a game barely discusses is read less reliably in that
  game; labelled examples from games that do discuss it are the only fix.
- **Unreachable reviews.** Reviews by deleted and private accounts count in Valve's total and
  cannot be fetched. Each game shows the coverage it reached.
- **A download is a snapshot.** Bringing a game up to date catches new and edited reviews, not
  deleted ones or changed vote counts; downloading it again does.
- **Irony is not read across a game.** A rating that disagrees with its text, "0/10, haven't
  slept in three days", is flagged on labelled reviews only.
- **Counts are a summary.** A count says where to read, not what was meant; every figure opens
  onto the reviews behind it.
