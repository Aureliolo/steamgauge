# Accuracy

The reader is measured on games it never trained on, against labels a language model wrote. Most
figures here are agreement with those labels. The figures checked against a person are marked as
such.

## Against the alternatives

457 claims from the ten held-out games, twenty per subject. Every row answers only where it is
sure enough and is scored on what it answers. The answers are in `reference/frontier-key.json`.

| | Answers | Agreement where it answers | Macro F1 |
|---|---|---|---|
| The commonest subject | 100% | 5.9% | 0.004 |
| TF-IDF bag of words | 48% | 75.5% | 0.508 |
| Nearest subject over an untuned encoder | 3% | 83.3% | 0.450 |
| The reader, standard (559M parameters) | 93% | 80.6% | 0.765 |
| Claude Opus 5, given the same sheet | 99.3% | 85.9% | 0.860 |

On all 5,029 held-out claims, the reader answers 91.6% and agrees on 81.5% of those (macro F1
0.707).

Claude Opus 5 costs about 800 tokens a claim, and a large game holds millions of claims. The
reader reads them on one graphics card, offline. The labels were written by Claude Fable 5.1 and
Claude Opus 5.5, so the model compared here wrote none of them.

## Each size

<!-- reader sizes: rendered from reference/reader-sizes.json by training/sizes.py -->
| Reader | Parameters | Download | Accuracy at 80% coverage | Accuracy at 90% coverage | GPU memory | One large game on the GPU | One small game on the CPU |
|---|---|---|---|---|---|---|---|
| small | 118M | 244 MB | 78.1% | 74.0% | 1.2 GB | 62 s | 36 s |
| standard | 559M | 1.1 GB | 84.4% | 82.0% | 2.6 GB | 125 s | 285 s |

- **Accuracy at a coverage:** 457 claims from ten games held out of training, labelled under the
  current category sheet. Each reader answers the given share of claims it is most confident about,
  scored on DirectML against `reference/frontier-key.json` (`frontier.py reader`).
- **GPU memory and one large game on the GPU:** one read of game 920210 (117,664 claims) through the
  desktop app on DirectML. The GPU was an otherwise idle NVIDIA GeForce RTX 4090. Memory is the peak
  during the read minus what the GPU held before.
- **One small game on the CPU:** 1,416 claims of game 1888930 on the CPU only, with loading counted.
  The CPU was an AMD Ryzen 9 5950X with other work running.
<!-- end of reader sizes -->

## Checked by a person

Two people answered claims from the held-out games on the page `steamgauge gold --serve` writes.

### English

One person answered 200 claims. 169 were drawn blind and 30 were claims the two labellers
disagreed about. The last one is left out of the figures.

| | Agreement |
|---|---|
| The person and the labels, blind | 65.1% (58% to 72%) |
| The person and the second labeller, blind | 63.9% |
| The person and the labels on praise or complaint | 85.8% |
| The reader names the person's subject, of 151 blind claims | **84.8%** (78% to 90%) |
| The reader names a subject the person accepts | **98.6%** |
| The same two, on the 46 claims the labellers disputed | 50.0% and 92.9% |

A claim can fairly sit under two subjects, so the person also judged whether the reader's
subject, where it differed, would do as well.

### Russian

A second person answered 255 Russian claims, blind.

| | Agreement |
|---|---|
| The person and the labels | 68.6% (63% to 74%) |
| The person and the second labeller | 70.2% |
| The person and the labels on praise or complaint | 85.1% |
| The reader names the person's subject, of the 230 it answers | **70.4%** (64% to 76%) |

## In each language

40,327 claims from games each reader never trained on, scored on every claim, including those it
would decline. The last column keeps only the claims the labeller was sure of.

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

The five in bold read below English, so the reader answers fewer claims in them to keep agreement
at three in four. A language with fewer than 100 labelled claims declines every claim.

## How the labels are made

- `sample-claims` draws reviews by a seed, seven in ten English, and every claim of a drawn review
  is labelled.
- A labeller sees each claim inside its review and nothing else: not the game, the rating or the
  reader's answer. The one exception is whether the game is VR-only, which `store-facts` records.
- Every labeller works from one sheet, which `brief` writes from the taxonomy.
- `second-opinion` and `compare-labels` have a second labeller read part of every set and every
  held-out game in full: 21,449 of 33,615 claims. They agree on the subject 88.6% of the time
  (Cohen's kappa 0.87) and on praise or complaint 94.9% (0.92).
- `declined` and `rare-claims` draw teaching sets where the reader is weakest. They are never
  measured on.
- `gold --for <name>` writes the page one person answers, and `ingest-gold` files the answers.
  `compare-labels --base gold/<one> --labels gold/<other>` says how often two people agree.
- `measure-claims` and `ceiling` score readings.
- Whole games are held out, chosen by a hash of their app ID.

## Limits

- **Unfiled claims** are usually about 10.9%. A game that declines 1.2 times that and five points
  more is flagged, with the languages behind it.
- **Mechanical splitting.** Rules split reviews into claims, and 9.1% of 30,414 random claims are
  cut where a labeller said they should not be.
- **Ambiguity depends on the labeller.** Two labellers marked 27.4% and 43.9% of the same claims
  as ambiguous, so ambiguity rates are not compared across games.
- **Thin subjects stay thin.** A subject a game barely discusses is read less reliably in that
  game.
- **A download is a snapshot.** An update misses deleted reviews and changed vote counts, which
  downloading the game again catches.
- **Irony is not read across a game.** A rating that disagrees with its text, "0/10, haven't
  slept in three days", is flagged on labelled reviews only.
