# Decisions

What has been settled about this tool, and where each thing actually stands. It exists because
a long build forgets: a decision made once and not written down gets quietly dropped, or worse,
asked again as though it were open. Nothing here is a proposal. Every row was chosen.

State means: **done** is built and in use; **partial** is built for one case and not the rest;
**not built** is exactly that, however long ago it was agreed.

## The product

| Decided | State |
|---|---|
| Rust core, **Tauri desktop shell**, and a CLI beside it | done: one binary, a window when opened with no arguments and the pipeline when given any |
| Compiled binary for Windows, Linux and macOS; the user downloads it, double-clicks it, and works entirely in the UI | built for all three by the release workflow, DirectML on Windows and CoreML on Apple Silicon; the claim reader is fetched by checksum on first use once one is published |
| The UI is first class, not a wrapper over the pipeline, and has to be good enough to look at | the window crawls, reads, shows the counts with their measured error, opens every row onto its claims and every word that stands out onto the claims that use it, draws the timeline, lists the languages, shows the induced subjects, and switches which language is counted without re-crawling |
| Results also export as one self-contained HTML page that fetches nothing | done |
| Name: **SteamGauge**, binary `steamgauge` | done 2026-09-11, renamed from `steam-review-census`. A census counts heads; this reads opinions, and "to gauge opinion" is the phrase for it. `gauge` alone was left to ThoughtWorks' test framework and npm's progress bar |

## What the analysis does

| Decided | State |
|---|---|
| Headline figure is the **mention rate**, always labelled as such | done |
| Every other percentage says which denominator it uses | done |
| **Deep, claim-level by default**: a review is split into the points it makes, and each point carries a category | done; the splitter is `claims-5` and the reading pass counts per claim |
| Shallow is the opt-out, and neither depth drops a review | done; `--depth shallow`, recorded in the reading and named on the page as not comparable |
| Taxonomy is a **fixed sheet plus induced game-specific extras** | the sheet is done; the induction chain runs end to end: `steamgauge distinct` draws the sample, an agent reads it against `reference/induction-brief.txt`, `steamgauge ingest-induced` refuses any subject without three real reviews behind it, and the report shows what survives under the table with its evidence and no invented rate. First run on 296970 (Renowned Explorers): nine subjects returned, nine kept, every one refining a sheet row or naming something the sheet cannot (mood combat, the explorer roster, the oddball enemies, save-scumming), each with four to fourteen reviews behind it |
| Extras are discovered by an **LLM reading an embedding-diverse sample** | the sample is farthest-point traversal over a hash-drawn pool of four thousand vectors, measured to put a mechanics review, a localisation joke, a crash report and a difficulty complaint in its first eight picks. The reading is one agent call per game, 70k tokens on Opus for a 120-review handout, run one at a time behind the labellers |
| Categories are assigned across the full corpus by a **linear probe over embeddings** | superseded: a fine-tuned encoder with abstention, which is a probe that can say no |
| **Corrected prevalence**: the measured error corrects the rate rather than sitting beside it | done, per subject, where the model finds it better than chance |
| **Summarise, per category, what people praise and complain about** | built without a paraphrase: each side of a subject shows the words it uses that the other does not, counted by reviewers and ranked by log-odds z-score against the other side, and every word opens onto the claims it was counted from. Counted during the reading pass in bounded memory. A written summary by a hosted model is the upgrade, when one is configured |
| **Build an overall picture of the game from those summaries** | done as a paragraph assembled from the counts: which subjects are raised most and by what share of reviews, which way each leans, and the words that stand out on each side. Every clause is a number with words around it, nothing is inferred, and a subject nobody raises is not mentioned. A written summary by a hosted model remains the upgrade |
| **Click through from any number to the reviews behind it** | done in the app, every subject opens onto its claims a page at a time; the report quotes eight per subject |
| Helpfulness bias ships as a column on every category | done |
| Irony and ratings that disagree with the text are flagged, not filed away | flagged on labelled reviews only |

## Models and setup

| Decided | State |
|---|---|
| A complete census with **zero setup and no API key**; hosted models are an upgrade, never a requirement | done |
| Embeddings run locally, on ONNX Runtime | done |
| `ort` with DirectML, CoreML and CPU | done |
| Embeddings carry dedupe, taxonomy, search and classification, not just one of them | dedupe done, search partial; classification moved off embeddings onto the trained reader, which is the right call because embeddings are dominated by sentiment and length rather than subject |
| A hosted model does the reading when one is configured | **not built**, there is no API client in the tree |
| The user chooses which model does the sorting, and how closely it reads | **not built** |

## Data and crawling

| Decided | State |
|---|---|
| Raw Parquet capture, kept, because a re-crawl cannot recover edited or deleted reviews | done |
| SQLite for crawl state | done |
| **DuckDB for querying the corpus** | **not built** |
| `author_steamid` kept for every review, as public data | done |
| Adaptive, date-sharded crawl with capped concurrency | done |
| Watermark top-up so a re-crawl does not re-pull old reviews | superseded by the sweep below, which finds arrivals and edits in one walk. The top-up wrote a new snapshot holding only the new reviews, and every pass read the newest snapshot, so a topped-up game counted only what had arrived since |
| **Periodic sweep by last-edit date**, to catch reviews edited since the crawl | done: `steamgauge sweep`, and "Bring it up to date" in the window. One walk in `updated` order, newest first, stopping a day past the watermark (the crawl, or the last sweep). Rows land in `sweep-<unix>.parquet` beside the crawl's shards, never over them, and `newest.json` records which copy of each swept id counts; every reader of the capture goes through one walker that skips the rest. The readings record when the capture last changed, and the page and the window say so when a sweep has landed since they were made |
| Valve's default filters overridden, because they hide 17.3% of negative reviews against 9.6% of positive | done |

## Distribution

| Decided | State |
|---|---|
| Apache-2.0 | done |
| Signed releases with cosign, plus build provenance attestation | done |
| Immutable release artefacts with checksums | done |
| No money spent: self-signed on macOS, and an extra step there is acceptable | accepted |
| Supply-chain hardening in proportion to the project, not the full enterprise set | done |

## Reference sets

| Decided | State |
|---|---|
| Claims labelled by Fable 5.1 agents, one game each, shown the text alone | done for all 51 games, one labeller at a time on the user's instruction |
| **Opus spot-checks the labels** | the blind second reading of a tenth was done by a second Fable agent, not Opus: 1,400 claims over thirty games, subject kappa 0.85. The row said Opus for weeks and was wrong. Opus has since read 26 of the 49 games in full, 16,310 claims, and what that settled is below |
| Roughly 400 labels to start | 38,118 claims over 51 games; the 20,000 target was passed and the draws that followed it were teaching sets rather than more of the same |
| **The test set becomes gold: the user adjudicates it by hand**, a random sample of about a thousand claims labelled blind for a representative accuracy figure, then the roughly four hundred the two labellers disagreed on, to settle the boundaries | decided 2026-09-11. The sheet has landed and the page is built: `steamgauge gold --serve` draws 1,000 blind claims from the frozen games and 194 the two labellers answered differently, serves them on the loopback address, and writes each answer to `gold-answers.json` as it is made. It now waits on nobody but the user. `--labels opus` draws the disagreements from the second model instead, which is 2,116 claims rather than 194, and asks the 72 neither labeller hedged first: a person adjudicates until they stop rather than until the list ends, so the order is most of what the hour buys |
| 30 to 35 mid-size games, mixed sentiment, small corpora acceptable | done and then some: 51 games drawn and labelled |
| Stratified subset trains, random subset measures, and the two are never merged | superseded at claim level: **whole games** are held out and the frozen ones choose nothing. A game's role is fixed by a hash of its own id, so adding games moves none; over the 51 drawn that is 10 frozen (214490, 620980, 774361, 920210, 1057090, 1222670, 1274570, 1466860, 1809540, 2881650), 6 validation (275850, 1295660, 1372880, 1465360, 1601580, 2338770), 35 train. The earlier shuffle reassigned every role on every run, which was found when fifteen games froze a different pair from eleven |
| Measured error **corrects the reported prevalence** | done, on the report page, per subject where the model finds it better than chance |
| The sets are a silver standard, and the README says so rather than calling them gold | done |

## The classifier

Settled after the shipped classifier was opened up in the app and found to be filing "gfg",
"game" and "☺" under Graphics and art. Prototype anchors are not being tuned, they are being
removed.

| Decided | State |
|---|---|
| The unit is a **claim**, not a review. A review is split into the points it makes and each point carries one subject | splitter written and tested |
| A **fine-tuned multilingual encoder** replaces prototype similarity, distilled from Fable labels, exported to ONNX, run on the existing runtime | built, training on each new wave of labels |
| The **backbone is chosen by bake-off**, not by reputation: several candidates, identical labels, identical frozen split, judged on per-category F1 and throughput together | done, below: `gte-multilingual-base` wins every measure but speed among models of its size, and the first model tried at twice that size beats it by more than every other setting in the sweep combined |
| **Calibrated abstention**: a claim below threshold is recorded as unclassified and counted, never folded into `verdict` | built, and the threshold is chosen by **the most coverage available at a promised accuracy**, never by maximising accuracy times coverage, which collapses to answering everything |
| **Polarity is predicted per claim**, and reported per review per subject as praised, criticised or **mixed** | done: a second head on the same trunk, and the report counts praise, complaint and mixed per subject |
| **Mention rate stays the headline** because it is verbosity-proof; claim share is deep-reading only and always labelled as verbosity-weighted | rule |
| Model is **multilingual**, reports default to **English** with a working language switch | done: the model reads every language the labels cover, the window reads English by default with "every language" a choice beside the button and a one-click switch to the other reading, the CLI takes `--language`. A switch is a re-read, because the model has to read the claims it skipped |
| Labelling is **cluster-stratified in round one**, then **active learning** (uncertainty crossed with diversity) | round one is stratified and running; `steamgauge declined` draws the claims the reader abstained on as a teaching set nothing measures, which is the uncertainty half, and it waits on a reading made by the current splitter |
| **One game first**, about 2,000 claims, then decide whether to commit the rest | done, the Frostpunk 2 pilot below, and the rest was committed |
| The 26 games already labelled at review level get **re-labelled at claim level**, so old and new are the same sample | in progress: they are among the 36 in the claim-level run |
| Full MLOps, **reproducible in this repository**: soft targets from labeller confidence, automated label auditing, frozen test set, per-language and per-length slice metrics, calibration, ONNX parity assertion, a CI gate that fails on F1 regression, hash-versioned label sets, plus self-training and multi-seed ensemble as measured experiments | frozen set, calibration, parity assertion and the second-reading audit are done; slice metrics, the CI gate, soft targets and the experiments are **not built** |
| Run tracking is a local JSON log carrying git sha, data hash, config and metrics. No external service | done, `run.json` in every training run |
| Model and dataset published to **Hugging Face**. The dataset is review ids, claim offsets and labels, **never review text**, because this repository holds no review data | script and fetch path built; nothing published yet, so the pin in `reader.rs` is empty |

## What the pilot found

One game, Frostpunk 2, 2,760 claims labelled by three agents against core-4. Everything here
is measured rather than argued.

| Question | Answer |
|---|---|
| Does the chain work end to end? | Yes: labels, training, ONNX export with a parity assertion at 1.7e-05 and zero answers changed, and a corpus pass |
| What do more labels buy? | 430 training claims gave accuracy 0.25 and macro F1 0.17; 1,629 gave **0.46 and 0.41**, with polarity F1 0.21 to 0.62. Still climbing |
| What does labelling cost? | ~305 tokens a claim, so 20,000 claims is about 6M tokens |
| How often is a claim mis-split? | 12% to 17% by three labellers independently, which drove three rounds of splitter fixes |
| What did labellers have nowhere to put? | Atmosphere and feel, by all three, and by six review-level labellers before them |

Then eleven games, 5,574 claims, measured with games held out whole:

| Question | Answer |
|---|---|
| How good is it on a game it has never seen? | Accuracy **0.318**, macro F1 **0.269**, on the frozen games. The pilot's 0.46 was within one game, which is the easy question; the validation games say 0.427, and they helped build the model |
| How much can it answer at a promised 75% accuracy? | **24% of claims** at threshold 0.42, on the games that chose the threshold |
| Does that promise transfer? | **No.** On the two frozen games the same threshold answers **13%** of claims at **0.620**, not 0.756. It was chosen on two validation games and overfits them |
| Is it calibrated? | Better where it matters least: calibration error 0.066 on the frozen games against 0.132 on validation, but area under the risk-coverage curve 0.543 there against 0.386, so its confidence ranks claims worse on a game it has not seen |
| What does it still not know? | `gameplay` has 257 labelled claims in one frozen game and the model answers **none** of them. `controls`, `difficulty` and `content` likewise. `accessibility`, `community`, `compatibility` and `language` score zero F1 for want of labels |

Then fifteen games, 7,680 claims, on a frozen set fixed by hash so it cannot move again:

| Question | Answer |
|---|---|
| How good is it on a game it has never seen? | Accuracy **0.476**, macro F1 **0.385**, polarity F1 0.700, on three frozen games |
| How much does it answer, and how well? | **27% of claims at 0.747**, in [0.696, 0.792] over 316 claims. Four games earlier it was 13% at 0.620 |
| Does the promise transfer now? | **Within a point**: 0.757 promised on validation, 0.747 delivered frozen |
| Why did it start transferring? | Area under the risk-coverage curve fell from 0.543 to **0.351**. The model learned when it does not know, which is the whole of what lets a threshold carry to a new game |

The frozen games are different games from the eleven-game run, because the old split
reassigned them; so the before and after are not over the same reviews. What is not in doubt
is the direction, and that labels are what moved it.

Verified in the tool as before: 315 claims answered at 0.746 against training's 316 at
0.747, one claim apart, which is the half-precision export reordering a single tie. And the
per-game spread says something the pooled figure hides: on the labelled claims, 1057090
declined 61% and agreed on 76.5%; 1466860 declined **80%** and agreed on 70.2%. 1466860 is
the game whose labeller called modding its dominant theme and had nowhere to file it.

Over the whole corpora the picture is softer: 66.5%, 72.9% and 83.4% declined for 1057090,
1466860 and 1601580, against a usual 73.2%. The labelled samples are stratified towards
claims that name a subject, so they show a gap more sharply than the corpus does. The
reader now carries its usual decline rate and the tool says when a corpus is declined at 1.2
times that, which none of these three reach; 1601580 at 1.14 is the nearest, and its labeller
reported a hard seam rather than a missing row.

Then the set was read a second time, a tenth of it, by a different labeller working blind:

| Question | Answer |
|---|---|
| Do two labellers agree on the subject? | **86.4%, kappa 0.85**, over 456 claims read twice across ten games. That is a set two readers agree about, not one reader's habit |
| On polarity? | 93.0%, kappa 0.89 |
| On whether a claim is contested? | **60.3%, kappa 0.28.** The first labeller flagged a fifth of claims, the second three fifths. They are not applying the same bar |
| Does the flag still mean something? | Yes. On the 181 claims neither flagged, the two agree on the subject **100%** of the time; where either flagged, 77.5%. The flag finds the right claims; what differs is how readily each labeller reaches for it |
| Where do they disagree on subject? | `genre` against `verdict` most of all ("great platformer"), then `gameplay` against `genre`, `content` against `gameplay`, `updates` against `verdict`. Every one is a boundary already in `reference/GAPS.md` |

So a contested rate is a fact about a labeller as much as about a game, and the reports say so
rather than comparing it across games as though it were the same measure. The per-field
figures are what `steamgauge compare-labels` prints, and the contested check runs every time.

At thirty games read twice, **1,400 claims**, every figure held: subject 86.9% at kappa
**0.85**, polarity 93.4% at 0.90, contested 74.1% at **0.48** with the first labeller flagging
31% and the second 49%. On the 657 claims neither flagged they agree on the subject 99.2% of
the time, and on the 743 either flagged, 76.0%. Their commonest disagreement is now
`difficulty` against `gameplay`, twenty-five claims of it, ahead of `genre` against `verdict`
(14) and `updates` against `verdict` (10); every one of them is a boundary
`reference/GAPS.md` already holds wording for. Three times the claims, twenty more games, and
the same numbers to a point: that is a silver standard behaving like one.

Then the backbone was put to the bake-off it was always going to face, on sixteen games and
8,608 claims, every candidate on the same split and schedule, judged on the validation games
only so the frozen ones stay unread by the thing choosing:

| Backbone, five epochs | Macro F1 | Accuracy | AURC | Answers at 75% | Claims/s |
|---|---|---|---|---|---|
| `xlm-roberta-base`, what shipped | 0.375 | 0.443 | 0.378 | 23% | 1,431 |
| `microsoft/mdeberta-v3-base` | 0.276 | 0.354 | 0.442 | 16% | 830 |
| `intfloat/multilingual-e5-base` | 0.423 | 0.482 | 0.317 | 32% | 1,428 |
| **`Alibaba-NLP/gte-multilingual-base`** | **0.449** | **0.518** | **0.295** | **34%** | 1,048 |

**Every candidate in that table is about 278M parameters, which is the question the bake-off
did not ask.** It compared four encoders of one size and concluded which family reads claims
best. Asked on 2026-09-12 what a bigger one does, `intfloat/multilingual-e5-large` at 560M
answered 84.9% of validation claims against `gte`'s 68.1% on the same labels, and that is four
times the spread between two seeds. The runner-up family at twice the size beats the winner at
one size by more than every other setting in the sweep put together. A bake-off is only ever an
answer about the axis it varied.

At three epochs the order was the same and the gaps wider, so it is not a schedule effect.
`gte` answers half as many claims again as the shipped backbone at the same promised
accuracy, its confidence ranks claims better (AURC 0.295 against 0.378), and it costs a
quarter of the throughput, which on a corpus of thirteen million claims is an hour on the
card this was measured on. `e5` is the runner-up and as fast as `xlm-roberta`; `mdeberta`
is slower and worse at everything, whatever its reputation. `gte` exports to the same ONNX
graph shape with parity intact (largest drift 8.3e-03 at half precision, no answers changed),
which was the condition for being a candidate at all.

Trained on its own on the same 5,988 training claims and measured on the same four frozen
games (1,800 claims), `gte` answers **37% at 0.803** where `xlm-roberta` answers 28% at
0.762; frozen accuracy 0.608 against 0.507, macro F1 0.512 against 0.405, AURC 0.236 against
0.322. The chain is verified against it as before: the tool answers **666 claims at 0.803**
across the four frozen games, which is training's figure to the claim and to the decimal,
on a different tokenizer and a different architecture from the last time this was checked.
Per game it runs from 74.5% (1466860, the modding game) to 84.2% (1809540), and over the
whole corpora it declines between 54% and 71% against a usual 63%.

Then twenty-three games, 12,011 claims, five frozen (214490 joined them), 2,495 frozen claims:

| Question | Answer |
|---|---|
| How much does it answer, and how well? | **43% of claims at 0.808**, 1,071 claims, at a threshold of 0.78 chosen on the validation games |
| How good is it on a game it has never seen? | Accuracy 0.606, macro F1 0.479 over the five, polarity F1 0.760, calibration error 0.082, AURC **0.217** |
| Does the tool agree? | 1,070 answered at 0.807 across the five corpora, one claim from training's 1,071 at 0.808, which is the half-precision tie again |
| Per game? | 76.3% on 1466860 to 87.0% on 1057090; declined 47% to 63% of labelled claims, and 57% is now the usual figure the reader carries |

Then twenty-seven games, 14,163 claims, six frozen (2881650 joined them), 2,797 frozen claims,
and this is the installed reader:

| Question | Answer |
|---|---|
| How much does it answer, and how well? | **47% of claims at 0.798**, 1,315 claims, at a threshold of 0.77 chosen on the validation games |
| How good is it on a game it has never seen? | Accuracy 0.601, macro F1 0.487 over the six, polarity F1 0.766, calibration error 0.109, AURC **0.210** |
| Does the tool agree? | 1,218 answered at 0.804 over the 2,614 labels it could join. Not to the claim this time, and why is below |
| Per game? | 71.6% on 1274570 to 87.6% on 1057090; declined 42% to 59% of labelled claims, and 53% is now the usual figure the reader carries |

Nine games earlier it answered 37%; the labels are still what moves it. Coverage rose four
points and agreement fell one, which is the threshold moving down the same risk-coverage curve
rather than a better or worse model: AURC, the figure that does not depend on where the
threshold sits, improved from 0.217 to 0.210.

### Eighty per cent of what?

`steamgauge ceiling` answers the question every agreement figure in this file begs. A model
trained on one labeller's reading cannot be more right than two labellers manage with each
other, so 80% against one labeller is a score out of that ceiling and not out of a hundred.
Over the claims read twice **and** answered by the model, split by what each game was to it:

| The model | games | claims | two labellers agree | model, where they did | where they split |
|---|---|---|---|---|---|
| **never saw** | 6 | 194 | **90.2%** | **85.1%** [0.791, 0.897] | 84.2% of 19 |
| chose its threshold on | 4 | 143 | 89.5% | 79.7% | 93.3% of 15 |
| trained on | 20 | 896 | 85.6% | 99.7% | 97.7% of 129 |

Only the first row is a measurement. The third is the model reciting labels it was trained
on, and it is in the table precisely because the gap between 99.7% and 85.1% is what holding
whole games back is for: a tool that pooled all three would report 94.9% and mean nothing by
it. The first pooled figure this was run on did exactly that, before the placement the trainer
uses was ported into the tool and a test pinned the frozen and validation games against it.

So the honest sentence about the current reader is: **on games it has never seen, over claims
it commits to, where two independent labellers reached the same subject, it agrees with them
85.1% of the time**, against a ceiling of 90.2%. A hundred and seventy-five claims is a thin
plank, and the interval says so. Widening it is what the hand-adjudicated set is for; more model-written
second opinions would only move the ceiling, not the floor.

### The splitter changed under the labels, and the labels held

`claims-4` shipped mid-run after all, which the plan above said not to do. What made it
safe is that a label had been carrying the byte span of its claim since the reference sets
were redrawn, so the join in `steamgauge measure-claims` could be moved from claim index to span:
a label whose span the new splitter still cuts as one claim finds its reading wherever that
claim now sits, and a label whose span it no longer cuts is counted as **unjoined** and said,
on the terminal and on the report page, rather than scored against whatever sentence now has
its old index. The span join reproduced the index join to the claim before the splitter
moved, which is what licensed moving it.

The rules, each from a labeller's report in `reference/GAPS.md`: three or more short
comma-separated parts are that many claims; a short answer joins the question before it; a
heading tag opened mid-sentence is emphasis; a line that ends on a comma continues on the
next; an emoticon belongs to the sentence before it; "i.e." and "z.B." are abbreviations
with their dots in; a bare `[list]` line is not a piece; a tagged heading weighs what its
words weigh and keeps its colon.

Measured with the wave5 reader on the same five frozen games: **1,003 claims answered at
0.810** over the 2,361 labels the new splitter still cuts as labelled, against 1,070 at 0.807
over 2,495 under `claims-3`. The 134 labels it no longer cuts, 5.4%, are the comma lists and
the fragments the labellers reported, split or joined as they asked; the agreement on the
rest did not move. So the splitter can improve between labelling runs without a relabel,
at the price of the labels it improves past.

A reading now records the splitter that cut it, and a reading cut by an older one is refused
wherever a claim would be quoted or scored by its index, in the report, the measure and the
window, with the counts themselves left standing, until the game is read again. Before this
the measure would have joined a stale reading silently and reported a number that was wrong
by however many indexes had shifted, which is how it was first run and why the guard exists.

### The splitter has run out of punctuation, and the rest is grammar

Labellers have flagged 5,656 claims of the 37,918 in the set as badly cut, and 4,716 of the
31,019 drawn at random, which is the 15.2% the README quotes. That number is measured against
whatever splitter cut the claim on the day, so five versions of rules later it says more about
the past than about this build. `cargo run --release -p steamgauge-core --example stale-splits`
re-cuts each flagged claim's review with the splitter this build has:

| drawn | claims | called badly split | still cut that way |
|---|---|---|---|
| random | 31,019 | 4,716 (15.2%) | 3,031 (**9.8%**) |
| retrieved | 2,899 | 341 (11.8%) | 202 (7.0%) |
| declined | 2,200 | 289 (13.1%) | 215 (9.8%) |
| mined | 1,800 | 310 (17.2%) | 254 (14.1%) |

A third of every objection is answered, and five points of the fifteen the random draws reported
are rules that have since landed. It is a floor and not the new rate: a claim cut differently
now is not thereby cut correctly, and nothing here can see a claim a rule made worse. The mined
draw is the highest because it is fished for by lexical probe, which catches long sentences that
name several things.

Broken out by language, the same tool answers a question the pooled figure cannot, since four
claims in five are English. **The splitter is not much worse outside English and is best on
Japanese**, which has the least punctuation to work with and the most reason to be feared:

| | called badly split | still cut that way |
|---|---|---|
| japanese | 17.4% | **5.2%** |
| german | 11.7% | 7.5% |
| koreana | 15.8% | 7.8% |
| russian | 15.4% | 8.9% |
| polish | **34.7%** | **9.0%** |
| english | 14.6% | 9.8% |
| schinese | 14.2% | 10.0% |
| brazilian | 18.6% | 13.2% |
| spanish | 15.9% | 14.2% |

Polish is the reason to break it out at all. It was flagged at more than double every other
language and is now below English: `claims-5` answered three quarters of what its labellers
objected to. Spanish and Brazilian Portuguese are the two where the rate has barely moved, and
they are the languages where the reading gap survives the hedged claims being dropped, which is
a coincidence worth one look before it is believed.

What survives is not punctuation. Of the 3,702 claims still cut the way they were objected to,
2,249 (60.8%) hold no line break and at most one comma of any kind, and 1,695 (45.8%) join their
clauses with "and", "but" or "although": "The story and graphics were outstanding" is two
subjects in four words, and "画面配乐玩法都是一流" is three in six characters with nothing
between them. No rule about commas, colons or line breaks reaches any of those. A splitter that
knows sentence terminators and markup and nothing else has reached what it can do, and the
remaining ten per cent is either a grammar problem, which this project deliberately refuses to
put a model into, or a label problem: a claim that really is about two subjects is being asked
for one.

So there is no `claims-6` on the strength of this, and the rate in the training summary should
be read as a historical flag rather than a property of the current cut. What a future splitter
change costs is unchanged: every reading is re-read, and every label whose span it no longer
cuts is dropped from the measurement.

The chain was first verified end to end at eleven games and re-verified at every wave since.
Training measures the frozen games in Python, on the full-precision weights, from the claim
text as labelled. The tool measures them in Rust, on the half-precision ONNX graph, over a
corpus it split itself and joined back to the labels by review id and the span each label
names. At eleven games the two agreed on **221 claims answered and 0.620 agreement**, to the
claim; at twenty-three, on 1,070 against 1,071 at 0.807 against 0.808. That is the splitter,
the export, the join and the reader all reproducing one number, which is the only way to know
that none of them is quietly wrong.

At twenty-seven it is 1,218 at 0.804 against 1,315 at 0.798, and the gap is the splitter
rather than a fault: training reads the claim text as the labeller was shown it, cut by
`claims-3`, while the tool reads the corpus as `claims-4` cuts it, and 183 of the 2,797
labelled claims name spans it no longer cuts. Every figure the model card quotes still comes
from training on the labelled text, which is the measurement that does not depend on the
splitter at all. The two come back into exact agreement when the sets are redrawn under
`claims-4` for `core-6`, and until then the difference is reported rather than smoothed.

So the model card and every report now quote the **frozen** games, never the validation ones.
A card is read by somebody deciding whether to run this on a game of their own, and the
validation figure answers a different question: how well it does on the games that chose its
settings. The two differ by eighteen points, and quoting the flattering one would be the same
kind of lie as a classifier that never abstains.

The first honest measurement of abstention came from fixing how the threshold is picked. Under
the old objective the model answered every claim at 39% accuracy and declined nothing, which
is the same failure as the prototype wearing a trained model's clothes.

The taxonomy is now **core-5**: `atmosphere` added on that evidence, plus rules for comparing a
game with its own predecessor, for praise or blame aimed at the studio, and for a game that
will not start at all. Every earlier reference set and the shipped anchors are core-4 and are
refused by this build, which is the guard working rather than failing.

## The model was being asked a question the labeller never had to answer

Every labeller read each claim inside the review it came from. Every model read it alone. "it
doesn't", "same here", "this too" are unanswerable on their own, and every one of them in the
training set was a label the model was asked to reach from text that cannot reach it.

Measured on the validation games, all three runs identical but for the input:

| input | budget | accuracy | macro F1 | polarity | answers at 75% |
|---|---|---|---|---|---|
| claim alone | 128 | 0.577 | 0.550 | 0.777 | 54% |
| claim alone | 256 | 0.586 | 0.566 | 0.773 | 54% |
| claim in review | 256 | 0.634 | 0.616 | 0.808 | **66%** |

The middle row is the control, and it is the point: the same token budget spent on the claim
alone buys nothing, so the twelve points of coverage come from the context and not from the
larger window. One training run, no labelling, no quota.

The budget is spent **centred on the claim** rather than from the start of the review.
Truncating a pair from the end gives a claim at the foot of a long review the opening
paragraph and nothing it is near, at exactly the same price.

What centring is worth depends entirely on how tight the budget is, which is obvious once
stated and was not obvious before it was measured:

| window | budget | answers at 75% |
|---|---|---|
| from the start of the review | 96 | 57% |
| centred on the claim | 96 | **66%** |
| from the start of the review | 256 | 66% |
| centred on the claim | 128 | 68% |
| centred on the claim | 256 | 66% |

At 96 tokens the head of a review usually misses the claim entirely and centring is worth nine
points. At 256 the head usually reaches it anyway, and centring is worth nothing measurable.
So centring does not beat a long window, it **buys the same answer at half the budget**, and
that is a cost saving rather than an accuracy gain. Between 96 and 256 tokens, centred, every
window scores the same; 128 ships because it is the cheapest of the ones that cannot be told
apart.

What it costs took three measurements to get right, and the first two were wrong in
instructive ways.

1. **On a drawn sample: "about 2x".** Wrong, and wrong in the way sampling is always wrong
   about a corpus. A few hundred drawn claims are distinct by construction, so they said
   deduplication was buying nothing. Over Cyberpunk's 1.55M English claims, 87% are distinct:
   real, but only an eighth of the forward passes.

   Part of that eighth is bought back rather than lost. A claim read in context is a different
   question in a different review, but the same question in every **copy** of the same review,
   because the window is cut from the text: the same text at the same index gives the same
   window and the same answer. So a context reading files its answers by the review's text
   rather than by its id, and "Great game." written as a whole review a thousand times is one
   forward pass again. Every reading records how many claims it counted and how many it
   actually asked about, so the saving is a number in the file rather than an argument, and it
   is not one number: across the eight frozen games it runs from 1% to 12% of the corpus.
2. **By token arithmetic, then on a CPU: "9x", then "7.8x".** A claim is 21 tokens and a claim
   with its review is 168, so the work must be eight times greater. On a CPU it is: 51.7
   claims a second against 6.6.
3. **On the machine that does the reading: 2.35x.** The same corpus, the same graph, the same
   pipeline, only the flag differing: 20.3 seconds against 47.8. A GPU running 21-token
   batches is mostly idle, waiting on launches rather than arithmetic, so longer sequences
   cost far less than their token count. A full library read goes from ninety minutes to about
   three and a half hours, once per model version.

The lesson is the one the project keeps relearning: measure the thing itself, on the hardware
that will do it, not a proxy for it. It held a fourth time. A backbone of twice the parameters
should cost twice the arithmetic, and on the same game through the same pipeline with only
`--model` differing it costs **1.68 times**: 103,026 claims in 124 seconds on the 278M model
and 208 on the 560M one, 830 a second against 495. The card is not the only thing in the loop.

One caveat that applies to every second quoted in this section: the same binary reading the
same game twice hours apart lands up to a fifth apart on this machine, so a ratio is only worth
anything when both sides of it were measured in one sitting, alternating. Every comparison here
was. None of the absolute times are comparable between them.

That measurement found the rest of the loop, which turned out to be the processor rather than
the card. Three things were taken out of it, each measured the same way: the same game, the
same graph, the same batches, three rounds alternating between builds so a warm cache or a
boost clock cannot land on one of them.

| | 107,022 claims | the card |
|---|---|---|
| two passes, tokenising a review once per claim | 282s | 36% busy |
| tokenising each review once, and one walk instead of two | 266s | 45% busy |
| **the next batch tokenised while the card runs this one** | **193s** | **51% busy** |

**Thirty per cent, and not one answer changed**: all nine readings hash identically, which is
the only reason a change like this is worth making. The counting pass used to open the capture
again and split every review a second time, which is worth about 6%; a review is now counted at
the first drain after its last claim was queued, against answers that are already final. The
larger share is that tokenising and the forward pass were taking turns. Tokenising a batch is
most of what a reading spends its processor on, the card waited through all of it, and a
channel one batch deep is enough to overlap them.

`cargo run --release -p steamgauge-core --example encoder-cost` times the processor half on its
own: **2,886 claims a second** on a game of ordinary reviews and **2,295** on one whose reviews
run long, of which cutting the windows is seven eighths and turning the pairs into token ids is
the rest. A whole reading does 530 to 840 a second, so tokenising has three to five times the
headroom it needs and parallelising it across the other thirty cores would buy nothing. What is
left of the processor's share is not tokenising at all.

**The bookkeeping looked like the answer, and was not.** Adding up what a review said means
extracting the terms of every claim of it, hashing them and writing a row each: real work, once
per claim, sitting between the model's batches. On a game of short reviews `nvidia-smi` showed
the card **22% busy** while one core did exactly that, so it was moved to a thread of its own,
and on the same game the card then read **80 to 100%**. Both numbers are true and the conclusion
drawn from them was wrong. Timed properly, alternating, on three games:

| | tokenise ahead | and count behind too |
|---|---|---|
| 107,022 claims | 98s, 107s, 111s | 102s, 120s, 105s |
| 68,917 claims | 51s, 50s, 53s | 50s, 50s, 53s |
| 491,728 claims | 380s, 398s | **424s, 408s** |

Nothing on the small games, and **6 to 12% slower on the large one**, because resolving each
review's answers and shipping it to another thread costs more than the counting it moves. So the
counting stays where it was, and what this section keeps is the measurement rather than the
change: a card's reported utilisation says what it is doing, not what it is waiting for, and it
is not evidence that anything got faster.

None of this changes what the reader is asked or what it answers. Every build here was checked
by reading the same game and hashing the rows, and a change to how a reading is scheduled that
changes a reading is not a scheduling change.

A figure taken before a correctness fix is not a figure. The first reading of this comparison
was 1.45x, measured while the reader was cutting its window out of padding: it was tokenising
the first 128 tokens of a review rather than all of it, which is both wrong and cheap, and the
bigger model paid more for the extra work than the smaller one did. Every timing in this
section was retaken after the fix.

### The batch was 128 because nothing had measured it

The last unmeasured thing in the reading path, and it was left open on the grounds that a batch
is padded to its longest member, so a different size composes batches differently, and in half
precision that moves a borderline answer. Deciding it during a library read would leave half a
library answered one way and half the other, which is the sort of quiet inconsistency this
project exists to avoid, so it was to be settled first and never was.

Both halves are now measured. The same game of 216,778 claims, the sizes run in a mirrored
order so that anything drifting over the half hour falls on each of them both early and late,
and the card logged every five seconds throughout:

| claims per pass | | | mean | the read's own memory | answers moved from 128 |
|---|---|---|---|---|---|
| 128 | 196s | 171s | 183s | 1.9 GB | |
| 256 | 167s | 164s | **165s** | **2.5 GB** | 74 |
| 512 | 164s | 167s | **165s** | 3.9 GB | 131 |

**256 and 512 are the same speed**, to within less than the spread of either one, and 256 asks
the card for 1.4 GB less. Between two sizes that read a corpus in the same time, the one that
leaves more of the card alone is the better default, and the one that moves fewer answers away
from the library already on disk is better again. So the default is 256, and it stops there
because doubling it a third time changes nothing but the memory.

**512 shipped first, on a comparison 256 was never in.** The runs that chose it were 128
against 512 only, and they left the obvious question of where in between it flattens unasked.
That is a worse mistake than the batch being wrong, because the number it produced was true:
512 really is faster than 128, and quoting that as the reason to prefer 512 skipped the
alternative that is just as fast and half the size.

Before either of those there were three rounds of 128, 256 and 512 run back to back: 184s,
159s, 159s, then 160s, 166s, 179s, then 325s, 309s, 335s. A third round at half the rate of the
first two, in a different order, is not a batch size. **This card is not the reading's alone**:
it carries whatever else the machine is running, and those rounds were taken without any record
of what that was, so what slowed them cannot be recovered. They are discarded rather than
explained.

Nor does it ever go quiet. The intended criterion was that every run start from the idle clock,
and on a desktop that is being used, none of them do: the six above began between 690 and 975
MHz because other windows keep the card clocked. What can be had is the mirrored order, a log
that shows no compute load beyond what each batch accounts for, and the spread quoted beside
the mean rather than hidden by it. The one run that stands out, 196s at 128, is the only one
that began above 900 MHz, which should have made it faster and did not.

What a larger batch buys is not less padding. The window of 16,384 claims is sorted by length
before it is cut into batches, so even a batch of 128 already holds claims of a size and has
almost no padding in it; what the larger batch buys is a card that is not waiting on the next
launch, which is the same thing the context measurement found when it noted that a GPU running
21-token batches is mostly idle. By 256 it has stopped waiting, which is why 512 adds nothing.

The cost in answers is what `diff-readings` was written to say. Each doubling moves about 75
claims of the 216,778: **74 between 128 and 256**, 81 between 256 and 512, and 131 end to end,
with the declines going both ways in roughly equal number and a confidence drift around 2e-4.
Three in ten thousand. So the fear that kept the question open was right in kind and wrong in
size, and the answer is not to hold the batch still but to record it: a reading now says which
size answered it, beside which splitter cut it and which run read it.

None of those are the card. Two readings of that game at one size, from runs hours apart,
differ by **nothing at all**: no answer, no polarity, and a confidence drift of exactly zero,
at 128 and again at 512. A reading is reproducible to the bit at a fixed size, which is what
makes these counts a measurement of the size rather than of the hardware, and it is the same
check that licensed the scheduling work above.

The library was read at 128 and stays that way until the next reader re-reads it, which every
new reader does anyway. A game read in the meantime is read at 256, says so, and differs from
the rest of the library by 74 claims in a game of two hundred thousand.

### A long read does not slow down, and the card was never the reason

Reading the library takes hours, so whether an hour of reading costs more per claim than the
first ten minutes is worth knowing before committing to one. It was also the excuse offered
for three timing rounds that came out at half rate: that half an hour of reading heats the card
until it reads at half the rate. That was asserted, never measured, and it is wrong.

Ten reads of one game back to back, no pause between them, the card logged every five seconds:

| | | | | | | | | | |
|---|---|---|---|---|---|---|---|---|---|
| 164s | 162s | 158s | 164s | 165s | 169s | 162s | 161s | **233s** | **252s** |

The first eight hold inside seven per cent of each other with no direction in them, and they do
it while the card sits at **86 to 89°C for forty minutes**. Heat is not what a long read costs,
because a long read costs nothing: the rate at minute forty is the rate at minute one.

The last two are a different matter, and they are the first time this anomaly has been caught
with instruments on it rather than reconstructed afterwards. It is not the card giving out:

| | utilisation | temperature | clock | power |
|---|---|---|---|---|
| run 8, 161s | 93% | 89°C | 2,356 MHz | 387 W |
| run 9, 233s | 92% | 87°C | 2,686 MHz | 352 W |

The slow one ran **cooler, clocked higher and drew less power** at the same utilisation. A
throttled card clocks down; this one clocked up. Higher clocks on less power is the shape of a
card doing lighter work than it can, which is what interleaving with another workload looks
like from the outside: the time is still accounted busy, and less of it is ours. Which workload
cannot be said, because the log recorded the card and not the processes on it, and that is the
gap to close before the next timing: **per-process GPU engine utilisation, not just the
totals**.

So the rule that came out of the discarded rounds survives, with its reason corrected. Time a
read on this machine and something else can take half of it without the card ever looking
unwell. What it costs to read is not a property of the reader alone.

## The tool was reading a window of nothing, and every answer looked plausible

Found 2026-09-12, an hour after the context model shipped. Training said the reader answered
84.2% of the frozen claims at 76.3%. The tool, running the same graph over the same games,
agreed on **68.4%**. Same model, same threshold, same claims, eight points apart, and on the
worst game nineteen.

Neither the half-precision graph nor the splitter moving under the labels. **A tokenizer file
saved by a trainer carries that trainer's padding and truncation inside it**, and this one was
saved from a run that padded every sequence to 128 tokens. The reader loaded it, and used it
both to encode the pair, which wants padding and truncation, and to find where a claim sits
inside its review, which wants neither. A 144-character review came back as 128 offsets, the
last ninety of them padding at `(0, 0)`. The claim was then looked for among offsets that
describe nothing: for the first claim of a review the window closed to the empty string, and
for later claims it opened past where it closed and fell back to the whole review. Both are
still strings. Every answer was in the usual confidence range, the coverage figure was right to
within a point, and nothing anywhere logged a warning.

The reader now keeps a second tokenizer with padding and truncation turned off, for offsets
alone. On the same claims, through the same graph, Rust agrees 77.2% where Python agrees 76.8%.
Through the whole pipeline, on all eight frozen games read again from their captures, the tool
answers **84.3% of 3,456 labelled claims and agrees on 76.6%, macro F1 0.612**, against the
84.2% at 76.3% and 0.611 training reported for the same games and the same graph. Three tenths
of a point, where it had been eight. The export now strips a trainer's padding out of the
tokenizer it ships, so the next thing to load that file is not handed the same trap, and the
reader refuses to load a tokenizer that cannot say where its tokens are rather than believing
it.

Three things worth keeping. A component configured for one job and reused for another is where
this class of bug lives, and the configuration that bit was three lines above the code that
suffered. The same mistake had already been found and fixed once on the Python side, and the
Rust side was never checked for it. And it was caught only because two independent
implementations of the same measurement existed to disagree:
`cargo run --release -p steamgauge-core --example score-export` scores the reader over the
exported claims, `training/frontier.py reader` scores the same graph through the trainer's own
windowing, and `steamgauge measure-claims` scores what the tool wrote over a corpus it split
itself. Any two of those disagreeing by more than a point is a bug, and each removes one
suspect: the tool's own figure was perfectly plausible on its own.

### transformers 5 is held, and the exporter is what holds it

Renovate offered transformers 4.57 to 5.17 and huggingface_hub 0.36 to 1.31 on 2026-09-14. CI
passed, which means nothing here: CI never trains and never exports. Trained under 5.17, one
epoch of the shipped configuration came out at 0.637 accuracy against `wave11`'s 0.622, so the
training half is fine. The export is not. `.half()` on the model and then torch's TorchScript
ONNX exporter writes a graph whose `LayerNormalization` takes one float input and one half one,
and onnxruntime refuses to load it at all, which is the good failure: loud, immediate, and not a
model that quietly answers differently.

The exporter torch now defaults to does write a loadable graph, and it agrees with the
full-precision model to 8.75e-03 with no answer changed, which is the same range the current
export lives in. Told to keep its weights inside the graph it writes one 1,121 MB file, so the
packaging this file first blamed is not the obstacle. **The obstacle is that the graph does not
run here.** Timed on the same capture through `--model`, which installs nothing: the traced
graph reads 17,305 claims at 909 a second, and the dynamo graph fails four seconds in on a
`Reshape` node inside the DirectML provider. That is the same wall the comment in `export.py`
has described since the exporter was first tried, when the graph ran but partitioned itself
across host and device at sixty claims a second; a provider that cannot run the graph at all is
the harder version of it.

So the prerequisite is not a day on packaging. It is a dynamo graph DirectML will run, which is
somebody else's bug to fix, and the pins in `requirements.txt` and the rules in `renovate.json`
hold the offer until it is. It also needs `onnxscript` back, which left with `optimum`'s tail.

What came out of the same audit: `optimum`, `datasets` and `pyarrow` were in the requirements
and imported nowhere. The lock is thirty entries shorter without them, and `optimum`'s own
major upgrade left the weekly PR with them.

### What is there to run when something looks wrong

Each of these answers one question and is a `cargo run --release -p steamgauge-core --example`
away. They exist because every one of them was, at some point, the thing that would have caught
a day of work going quietly wrong.

| | asks |
|---|---|
| `score-export` | does the reader agree with the trainer on the claims the trainer exported? |
| `encoder-cost` | what does the processor half of a reading cost, and where inside it? |
| `check-readings` | does every reading say what its own rows hold? |
| `diff-readings` | where do two readings of the same corpus disagree, claim by claim? |
| `find-claims` | does this corpus actually say a particular thing, how often, and in which language? |
| `check-draws` | does a handout still name the claims this build cuts, before a labelling run is spent on it? |
| `stale-splits` | how much of what labellers called a bad split does this build still split that way? |
| `mine-check` | what is each fishing line for a starved subject actually catching? |

Three more on the Python side, run against a model directory rather than a capture:

| | asks |
|---|---|
| `training/sweep.py` | which configuration won, and is the gap bigger than the seed spread? |
| `training/confidence.py` | what should confidence be scored by, and where does the line go, subject by subject? |
| `training/confusion.py` | what does the reader mistake for what, and is a weak subject starved or badly bounded? |
| `training/adjust.py` | does shifting the logits by the class priors buy anything? |
| `training/language.py` | how well does the reader read each language, and on how much evidence? |
| `training/lines.py` | is the accuracy the reader promises actually delivered, language by language, and what does each abstention rule cost? |

The last four read out-of-fold logits with `--oof` and should be run that way. Four validation
games give intervals eight points wide and figures that move; five folds
(`crossval.sh`, seven minutes each) give all twenty-eight non-frozen games and intervals of two.

`lines.py` is the one to reach for when a promise is suspected rather than a score: it fits every
policy leave-one-game-out, so a language that reads well because one of its games is easy cannot
fit its own line and then be marked on it. It is how the per-subject rule was found to be keeping
its 75% on average and breaking it in Korean by 8.3 points.

## Most of a sweep is the seed

Forty configurations were trained on one set of labels overnight on 2026-09-11, every one of
them scored on the validation games only. Ranking them and taking the top row is how a sweep is
usually read, and here it would be wrong almost every time.

Train one configuration twice, changing nothing but the seed, and it lands **0.5 to 4.3 points
apart** on how many claims it answers: `win-128` on three seeds gives 64.5%, 68.1% and 68.8%.
The interval printed beside a coverage figure is about 1.8 points, and it is the wrong bar: it
is the noise in scoring one finished model on 2,615 claims and says nothing about training the
same thing again. A configuration has to clear **4.3 points** before it has changed anything.

### Coverage and accuracy do not have the same noise, and one bar for both is wrong

Every refusal in this document is gated on the same sentence: is the gap bigger than the seed
spread? The spread being quoted was three seeds, at another learning rate, on a label set
nineteen thousand claims smaller, and it was carried forward as "about two points" long after
the configuration it described stopped being the one that ships. So it was measured again, on
the configuration that ships and the labels it ships with: five seeds, nothing else changed.

| seed | answered | agreement | macro F1 |
|---|---|---|---|
| 1 | 89.3% | 77.5% | 0.677 |
| 2 | 89.7% | 77.6% | 0.673 |
| 3 | 89.3% | 77.7% | 0.687 |
| 4 | 90.4% | 78.1% | 0.686 |
| 5 | **91.7%** | 77.4% | 0.677 |
| | 2.42 points, sd 1.02 | **0.70 points, sd 0.27** | 1.39 points, sd 0.61 |

Two points of coverage was right. **Seven tenths of a point of agreement was not**, and that is
the finding: these two figures are quoted side by side, refused side by side, and they are not
equally noisy. Coverage moves three and a half times as much. A bar wide enough for coverage,
applied to agreement, throws away every real improvement smaller than three standard deviations,
and this project has been refusing candidates on exactly that arithmetic. The gate is now two
numbers, because it was always two questions.

**Four seeds said 1.11 points and the fifth said 2.42.** The spread nearly doubled on the last
run, which is what a spread estimated from a handful of samples does, and it is worth writing
down next to the number it produced: five is enough to gate a decision on and not enough to
quote to two decimal places. Anything relying on this being tighter than it is should be
measured again with more.

Worse than that, and found on 2026-09-12: **the seed is not the only thing that moves.** Two
pairs in this sweep are the same configuration, the same seed, the same labels and the same
trainer, run twice at different commits that did not touch `train.py`, and they land 2.1 and 1.8
points apart (`e5large` 84.9% against `wave9` 82.8%; `wave8` 75.3% against `lr5e5` 73.5%).
Whatever this card does with non-deterministic kernels, a run is not repeatable to better than
about two points of coverage, so "another seed" understates the noise and a two-point gap is
worth nothing at all. `sweep.py` prints reruns and seeds as separate lists, because a rerun's
own noise inside a figure about configurations is how a sweep lies to you.

The first thing that bar refused: **a learning rate of 3e-5 on the big backbone.** Three seeds
answer 86.7%, 86.5% and 84.7% against 2e-5's 84.9%, 84.1% and 82.8%: a two-point mean gap, which
is exactly what running the same thing twice is worth. It is not a finding, `wave9` stays, and
nothing was re-exported, re-installed or re-read on the strength of it.

The second is the one worth keeping as a lesson, because it looked like a finding for half an
hour. `multilingual-e5-large` was pre-trained on pairs written as `query: ` and `passage: `, and
a fine-tune that drops them is asking the encoder a differently shaped question from the one it
learned, so writing them back in should help. The first run answered **88.9%**, two points clear
of the best run without them and the top of the whole table. Two more seeds answered 86.4% and
85.3%, against 86.7%, 86.5% and 84.7% without: **a mean gap of nine tenths of a point**, and a
spread inside the prefixed configuration of 3.6. The prefixes buy nothing. A well-motivated
change with a mechanism to explain it is exactly the kind that gets adopted on one run.

What was kept from it is the plumbing rather than the setting. The pair was being built in four
places, each repeating the same two lines: the trainer, the export, the frontier comparison and
the confidence sweep, with the Rust reader as a fifth. `Claims.pair` is now the only place that
decides how a claim is written, `--prefix` is an option on it that defaults to off, and the
reader honours the same flag from `reader.json`.

Against that bar, almost nothing the small model was asked changed anything. Everything here
lands inside the band and bought nothing at all: batch 16, every window length from 96 to 256,
marking the claim inside its window, a polarity weight of 0.2, down-weighting the contested and
the mis-cut claims, and eight or twelve epochs against five.

Five settings are genuinely worse. A learning rate of 1e-5 answers 47.8%, three epochs 52.3%, a
batch of 64 54.3%, and a polarity weight of 1.0 59.7%. The fifth is the interesting one:
**weighting the rare subjects up makes macro F1 worse, which is the figure it exists to fix.**
At exponents of 0.3, 0.5 and 0.7 on the inverse frequency, coverage falls to 55%, 47% and 42%
and macro F1 falls with it, 0.612, 0.605 and 0.581 against the baseline's 0.637. A rare row is
rare because the corpus rarely says it, and shouting its few examples louder adds no evidence
about it while costing the rows that had some.

Two settings are genuinely better, and only one of them is small. **5e-5 answers 73.5%**, 4.7
points clear of the best of the three baseline seeds, and it is a plateau rather than a peak:
7e-5 and 1e-4 answer 73.0% and 73.3%, and three seeds of 5e-5 land between 72.5% and 73.5%. And
**a backbone of twice the size answers 84.9%**, which is eleven points clear of that and four
times the seed spread, on the same labels and the same schedule:

| `multilingual-e5-large`, 560M | answers at 75% | macro F1 | AURC | ECE |
|---|---|---|---|---|
| five epochs, 2e-5 | 84.9% | **0.669** | 0.141 | 0.181 |
| the same on another seed | 84.1% | 0.662 | 0.140 | 0.182 |
| five epochs, 3e-5 | **86.5%** | 0.659 | **0.138** | 0.190 |
| five epochs, 1e-5 | 76.9% | 0.646 | 0.156 | 0.164 |
| three epochs, 2e-5 | 78.2% | 0.656 | 0.150 | **0.135** |
| `bge-m3`, 568M, five epochs, 3e-5 | 75.0% | 0.650 | 0.163 | 0.197 |
| `gte-multilingual-base`, 278M, the same schedule | 68.1% | 0.637 | 0.183 | 0.133 |

The `bge-m3` row is the one that says what this finding is and is not. It is the same size as
the winner, from a different family, on the same labels and the same schedule, and it answers
**ten points fewer claims**. Twice the parameters is not the finding; this backbone is, and a
future candidate has to be tried rather than assumed from its parameter count.

Five 560M encoders at 3e-5 say what the size is worth and what the adaptation is, because four
of these five are `xlm-roberta-large` underneath:

| 560M encoder, five epochs at 3e-5 | answers at 75% | macro F1 |
|---|---|---|
| `multilingual-e5-large` (three seeds) | **86.7%, 86.5%, 84.7%** | 0.659 to 0.673 |
| `multilingual-e5-large-instruct` | 83.2% | 0.671 |
| `xlm-roberta-large`, unadapted | 78.0% | 0.601 |
| `bge-m3` | 75.0% | 0.650 |
| `bge-reranker-v2-m3` | 69.1% | 0.628 |

The architecture is worth 78%. Adapting it for retrieval the way e5 did is worth another six to
nine points on top; adapting it the way `bge-m3` did costs three. The instruct variant of the
winner lands inside the winner's own seed spread and buys nothing. So the thing to carry forward
is not "use a 560M encoder" and not "use XLM-R large": it is that what an encoder was adapted
for decides this, and the only way to know is to train it for eleven minutes and look.

**The last row is the sharpest version of that, and it was a surprise.** `bge-reranker-v2-m3` is
the same graph and the same parameter count as the winner, and it is the only candidate anywhere
that was pretrained as a cross-encoder over text pairs, which is the exact shape of this task:
two sequences in, one judgement out. It was picked for the sweep on that reasoning. It answers
**seventeen points fewer claims than the winner** and nine fewer than the unadapted architecture
it is built on. Pretraining for the right shape is worth less than nothing here; pretraining a
trunk to emit a single relevance scalar appears to discard what twenty-six-way classification
needs, and starts further from useful than a general-purpose embedding model does. Shape of task
is not a reason to expect a backbone to win, and after this it is not accepted as one.

### The four backbones the earlier search missed, all refused

Researched and run 2026-09-12, against the bar every candidate faces: more than 90.3% coverage,
or near 86% at a fraction of the cost. Each got the learning rate its scale had already earned,
5e-5 at base scale and 3e-5 at large.

| candidate | parameters | answers at 75% | macro F1 | AURC |
|---|---|---|---|---|
| **incumbent, `multilingual-e5-large` 3e-5** | 560M | **86.5%** | 0.659 | 0.138 |
| `jhu-clsp/mmBERT-base` | 110M | 70.2% | 0.598 | 0.173 |
| `BAAI/bge-reranker-v2-m3` | 560M | 69.1% | 0.628 | 0.173 |
| `ibm-granite/granite-embedding-311m-multilingual-r2` | 311M | 65.3% | 0.566 | 0.192 |
| `jhu-clsp/mmBERT-small` | 42M | 59.0% | 0.560 | 0.213 |

None is close, and none is cheap enough to make its distance interesting: `mmBERT-base` at a
fifth of the parameters still loses sixteen points, where the whole argument for a small model
would be losing two or three. The incumbent stands.

**The last row was never a contender: it is a diagnostic, and it answered its question.** If a
42M trunk had landed within two points of a 110M one, the ceiling would be the labels and
backbone shopping would be the wrong thing to be doing. It landed **eleven points** below. So
capacity is still binding at this data size, which is a thing worth knowing before anyone
concludes that the 18,907 labels in the current training export are the limit: they are not the
limit yet.

Three runs of the winner land 2.1 points apart, 82.8% to 84.9%, where the small model's baseline
seeds land 4.3 apart, and every rate tried on the big model beats every configuration of the
small one. So this is the backbone and not a lucky run. What
it costs to read a library with is the open question, and cost is the whole argument for this
project over a frontier model, so it is measured on the card that does the reading before it is
chosen. Its calibration is the other: worse than the small model's at every rate, and
calibration is what carries a threshold from the validation games to the frozen ones. Three
epochs is the fallback that keeps the calibration and gives up six points of coverage.

`training/sweep.py` prints that table and that spread from the run records, and it is the first
thing to run before calling any configuration a winner.

The sweep cannot say anything about the frozen games, because no sweep run ever reads them, so
the one question it left open was whether the worse calibration would cost the transfer: the
expected calibration error climbs from 0.066 at 1e-5 to 0.203 at 5e-5, and calibration is
exactly what makes a threshold chosen on the validation games hold on the frozen ones. That has
failed before. **It did not fail here.** Trained again with the frozen games evaluated, the
chosen configuration promised 75.0% and delivered 76.3% on them, and the small model at 5e-5
promised 75.0% and delivered 75.1%. Worse calibration inside the validation games turned out to
be compatible with a threshold that carries; it is still the thing to check, not the thing to
assume.

## What it has to beat

A number with nothing beside it says only that the thing runs. Three baselines now run over
the same claims, the same split and the same selective-prediction protocol, on the **frozen**
games that chose nothing:

| | accuracy | macro F1 | AURC | answers at 75% |
|---|---|---|---|---|
| commonest subject | 0.233 | 0.015 | 0.731 | never reaches the promise |
| bag of words (TF-IDF, word and character n-grams) | 0.441 | 0.347 | 0.339 | 32% |
| nearest subject centroid, untuned backbone | 0.423 | 0.372 | 0.442 | **5%** |
| the trained reader, claim alone, 278M | 0.605 | 0.504 | 0.216 | 58% |
| the same, claim in review, at the rate that suits it | 0.667 | 0.555 | 0.160 | 77% |
| the same again, on a backbone of twice the size | 0.709 | 0.611 | 0.127 | 84% |
| the same again, on nineteen thousand more labels (`wave11`) | **0.737** | **0.673** | **0.106** | **90%** |

The last four rows are one change at a time, each measured on the frozen games, each promising
75% and delivering it: 0.749, 0.751, 0.763 and 0.772. The last row is the reader that ships,
scored here at one threshold so that it stands in the same column as the rows above it; with the
line per subject it carries it answers 82.9% of frozen claims at 80.7% agreement, and the two
frozen games added since the row above it was measured are in its figures and not in theirs. Reading the claim inside its review and training
at the learning rate that suits that is worth nineteen points of coverage; the bigger backbone
is worth seven more on top and most of the macro F1.

**The frozen set grew from eight games to ten on 2026-09-13**, because a game's role comes from
a hash of its id and two games labelled that day fell inside the frozen fifth. Every row above
except the last was measured on the eight; the last was measured on the ten, and so are the
baselines retaken the same day: commonest subject 0.258 and still never reaching the promise,
bag of words 0.494 with macro F1 0.420 answering 42%, nearest centroid 0.429 and 0.373 answering
10%. The shape is the same and the gap the last column measures is wider, not narrower, so
nothing in the argument below turns on which set a row came from; a reader compared across that
line to a tenth of a point would be reading more into it than is there.

The third row is what this project did before it trained anything, and the last column is why
it stopped. Cosine distance to a prototype has no way to say "this is about nothing", so its
confidences carry almost no ordering: asked to be right three times in four, it can answer one
claim in twenty. The bag of words is the honest floor, it takes seconds to fit, and a
278M-parameter encoder that could not clear it would not be earning its electricity.

**The same three baselines on the frontier sample, and a comparison that was not one.** The
table above is the corpus as it comes, a quarter of it `verdict`. The frontier comparison is a
stratified draw, twenty claims a subject, and the README was quoting baseline rows from the
first beside reader and frontier rows from the second. On the sample the baselines are a
different animal: the commonest subject falls to 0.049 accuracy and still never reaches the
promise, the bag of words answers 34% at 0.753 with macro F1 0.412, and the centroid answers 6%
at 0.900 with 0.439. Every row of that table is now the same 471 claims (`baseline.py --key`,
`reference/baselines-frontier-sample.json`), and both distributions are kept because each
answers a different question.

The same table also put each baseline's accuracy over all claims in a column headed "accuracy
where it answers", which read as though the bag of words were right 44.1% of the time on the
third of claims it answered when it was right 75.1% of the time. The error was in this
project's favour, which is the kind that survives longest. Corrected 2026-09-13.

### One threshold for twenty-six subjects is the wrong shape, and it hides the worst of it

Measured 2026-09-12 by `training/confidence.py`, which answers two separate questions: what to
score confidence by, and where to put the line.

**Read it on twenty-eight games, not four, and this is why.** The first run of this study used
the four validation games and 2,615 claims, and every number came back with a game-blocked
interval about **eight points wide**, which is wider than every difference the study exists to
decide. Worse than imprecise, it was misleading: that run put `story` precision at 0.32 and
`genre` at 0.50, and both were small-sample artefacts. On twenty-eight games they are 0.58 and
0.65. Four games can tell you a subject is weak and cannot tell you how weak, and a figure
quoted from four games is a figure that will move.

So `train.py --fold` holds out a cross-validation fold over the non-frozen games instead of the
usual validation set, the frozen games stay frozen in every fold, and `--save-logits` writes
what each fold held out. Five folds, seven minutes each, and every claim of all twenty-eight
non-frozen games comes back answered by a model that never saw its game: **15,138 out-of-fold
claims, and the interval closes to 2.3 points.** No test set was spent to get it. The thresholds
are still cross-fitted on top of that, leave-one-game-out, because a threshold fitted and
reported on the same claims flatters itself.

**The score barely matters. Nothing beats max probability by two points.**

| confidence score, out of fold over 28 games | AUGRC | AURC | one line |
|---|---|---|---|
| max probability (shipped) | 0.0926 | 0.1367 | 85.8% at 0.750 |
| margin over the runner-up | 0.0931 | 0.1372 | 85.4% at 0.750 |
| negative entropy | **0.0920** | 0.1361 | 86.6% at 0.750 |
| max logit | 0.0939 | 0.1424 | 86.7% at 0.749 |
| max logit over its 1-norm | 0.0944 | 0.1403 | **86.9%** at 0.749 |
| max logit over its 2-norm | 0.0927 | 0.1360 | **86.9%** at 0.750 |
| max logit over its 3-norm | 0.0927 | **0.1356** | 86.7% at 0.750 |
| max logit over its 8-norm | 0.0948 | 0.1392 | 84.9% at 0.750 |

The p-norm rows are the ones that were expected to win. Normalising the logit vector by its
p-norm and taking the max is the best-replicated post-hoc result in selective prediction:
Cattelan and Silva (arXiv:2305.15508) ran it across 84 pretrained classifiers and found a good
many have confidence estimators that are simply broken, with ordering far worse than their
accuracy implies, and that this fixes it outright. It buys **1.1 points of coverage, against an
interval of [85.7%, 88.0%]**, and the best p is worse than max probability on AUGRC. The
conclusion is the useful one: this reader's confidence estimator is not one of the broken ones,
and the whole grid from 0.3 to 8 is flat. Max probability stays, now for a reason rather than
for want of trying.

**AUGRC is reported beside AURC from here, and on this data they disagree.** AURC divides by how
much was answered, so it mixes the ordering quality of the score with the accuracy of the
classifier underneath, and Traub et al. (arXiv:2407.01032) found that switching to AUGRC moved
the ranking on five of six datasets. Here AUGRC prefers negative entropy and AURC prefers the
3-norm. On four games they had agreed, which is one more thing four games could not see.

**Where the line goes is the finding.** One threshold gates all twenty-six subjects. Fitting one
per subject, each holding the same 75% floor *for its own predictions*, costs four points of
overall coverage: 81.7% at 0.758 against 85.8% at 0.750. The average says the trade is not worth
it. The average is what hides the problem:

| subject | predicted | one line | a line per subject |
|---|---|---|---|
| `verdict` | 2,912 | 88% at 0.83 | 100% at 0.78 |
| `gameplay` | 2,603 | 84% at 0.73 | 77% at 0.75 |
| `offtopic` | 1,257 | 87% at 0.81 | 100% at 0.77 |
| `content` | 976 | 81% at 0.71 | 70% at 0.75 |
| `genre` | 903 | 84% at 0.70 | 69% at 0.74 |
| `difficulty` | 873 | 82% at **0.66** | 52% at 0.75 |
| `story` | 491 | 82% at **0.64** | 47% at 0.72 |
| `atmosphere` | 332 | 81% at **0.64** | 58% at 0.74 |
| `performance` | 292 | 89% at 0.84 | 100% at 0.78 |
| `policy` | 171 | 83% at **0.60** | 49% at 0.73 |
| `compatibility` | 155 | 89% at 0.70 | 58% at 0.69 |
| `audio` | 150 | 94% at 0.84 | 98% at 0.81 |
| `vr` | 36 | 75% at **0.30** | 25% at 0.00 |
| `accessibility` | 31 | 58% at **0.50** | silent |
| `licensing` | 4 | silent | silent |

**One line keeps the promise on average and breaks it for sixteen of the twenty-six subjects**,
including four of the five commonest. A report that says "38 reviewers found it too hard" is
built on `difficulty` predictions that are right two times in three, not three in four, and the
headline cannot see it because `verdict`, `offtopic` and `price` carry the average. `vr` is the
worst of it: the reader says `vr`, is wrong seven times in ten, and the shipped threshold passes
three quarters of that through.

A line per subject spends its four points on exactly those rows and hands most of them back
where the reader reads well: `verdict` and `offtopic` and `performance` to 100%, `audio` to 98%,
`controls` from 89% to 94%, `price` from 93% to 99%. Nearly every subject then lands between
0.74 and 0.78, which is the promise held subject by subject rather than on average.

**The floor is held per subject, not overall, and that choice is the whole design.** The rule
that maximises overall coverage under one overall floor is to answer the head and abstain on
the entire tail, because that is arithmetic. It would also be a product failure dressed as a
metric win: the rare complaints are the ones worth finding. A per-subject floor cannot buy
coverage that way. A subject that no threshold can make right three times in four goes silent
instead, which is the honest answer and is printed as `silent` rather than folded into an
average.

Subjects with fewer than 40 calibration predictions cannot support a quantile of their own and
share a pooled one. That is the clustered form of the standard Mondrian construction, and with
`licensing` at 32 labels in the entire set it is not optional.

**Built, and it ships when the next reader is exported.** `reader.json` may now carry a
`thresholds` array, one line per subject in `subjects` order, and `read.rs` applies it;
`null` means that subject is declined outright, which is the honest answer for a subject no
threshold can make reliable and is not the same as a low line. A reader exported without the
field keeps the single threshold, which is every reader shipped so far, so nothing has changed
under anyone. `export.py --lines-from <fold logits>` draws them, fitting on every non-frozen
game at once because that is the threshold that ships; what a line is *worth* is the
out-of-fold question above and is answered separately.

It is not switched on by exporting today, because the next export is going to carry the mined
labels and whatever the adjudication settles, and exporting twice to change one thing at a time
would mean measuring the model twice on the frozen games. One export, both changes, one frozen
read.

**Exported with lines as `wave10`, 2026-09-13, and measured on the frozen games.** The same eight
games and the same 3,456 labelled claims wave9 was measured on, by `measure-claims` over readings
made by each reader:

| reader | answered | agreement | macro F1 |
|---|---|---|---|
| `wave9`, one line | 84.3% | 76.6% | 0.612 |
| `wave10`, a line per subject | 83.7% | 79.5% | 0.646 |

Three points of agreement at the same coverage is past the two-point bar a rerun of one
configuration sets. Two games the split has frozen since, labelled after wave9 and trained on by
neither reader, land with the rest: over all ten, 84.7% of 5,266 claims answered at 80.0%, macro F1
0.661. What this run cannot do is divide the gain between its two causes, because it carries both:
the lines, and seventeen thousand labels more than wave9 had, most of them teaching draws aimed
at the starved rows. With 202 `licensing` labels no subject is thin enough to go silent, and all 26
got a line of their own, from `performance` at 0.23 to `vr` at 0.98.

**`wave11` ships, 2026-09-13.** The same command over 37,918 labels, which is every label in the
set as it stood: nineteen thousand more than `wave9` had, and 2,200 of the 2,400 declined
teaching claims. The last 200, on 916440 and 949230, were labelled after the export when the
labeller's quota came back, and they stay out of this reader: two hundred claims in thirty-eight
thousand cannot move a figure whose noise bar is two points wide, and a retrain nobody could
tell from a rerun is not a retrain. Trained anyway on 2026-09-15, to check rather than assume:
`wave12` answers 89.3% at 77.5% against this reader's 90.1% at 77.2%, which is inside the seed
spread on both counts. Two hundred labels in thirty-eight thousand bought nothing measurable,
as expected, and it cost seventeen minutes to stop guessing. They are in the set for whatever
trains next. Ten frozen
games, 5,266 labelled claims:

| reader | labels | answered | agreement | macro F1 |
|---|---|---|---|---|
| `wave10` | 35,718 | 84.7% | 80.0% | 0.661 |
| `wave11` | 37,918 | 82.9% | 80.7% | 0.642 |

**Nothing in that is a finding.** Two points of coverage is what the same configuration run twice
is worth on this card, and the two readers sit inside it in opposite directions. `wave11` ships
because it is the one trained on every label, not because it measured better; a project that
picks whichever rerun landed higher is fitting the frozen games. On the eight games `wave9` was
measured on, the three readers read 84.3% at 76.6%, 83.7% at 79.5% and 81.8% at 79.8%: the
agreement gain from `wave9` holds, the coverage drift between the last two does not signify.

**What the lines themselves cost and bought, one reader and two rules.** Training scores `wave11`
on the same frozen games at the single threshold it transferred from validation, and the tool
scores it with the exported lines. Same weights, same claims, same labels:

| rule | answered | agreement |
|---|---|---|
| one threshold, 0.64 | 90.1% | 77.2% |
| a line per subject | 82.9% | 80.7% |

Seven points of coverage for three and a half of agreement, and the same trade the out-of-fold
study predicted. It is the trade worth making here because the rows it silences are the ones the
reader reads worst, and a mention rate nobody can trust is worth less than a missing one that
says so. `wave10` showed the same shape (88.2% at 78.5% against 84.7% at 80.0%).

**The lines `wave11` ships with**, drawn on five folds of the non-frozen games at a 75% floor,
against the single threshold of 0.64 they replace. No subject is declined outright:

| line | subjects |
|---|---|
| below 0.40 | `mods` 0.18, `verdict` 0.20, `performance` 0.26, `audio` 0.31, `compatibility` 0.36, `language` 0.39 |
| 0.40 to 0.79 | `price` 0.46, `offtopic` 0.55, `controls` 0.64, `community` 0.65, `graphics` 0.71, `updates` 0.76, `bugs` 0.76, `gameplay` 0.78 |
| 0.80 to 0.94 | `monetisation` 0.81, `tutorial` 0.84, `story` 0.84, `multiplayer` 0.90, `licensing` 0.91, `content` 0.91, `accessibility` 0.94 |
| 0.95 and up | `difficulty` 0.95, `policy` 0.95, `genre` 0.96, `atmosphere` 0.97, `vr` 0.99 |

Read it as a map of what the reader knows: it will call `mods` or `verdict` on almost any hint,
and it has to be nearly certain before it says `vr` or `atmosphere`. The rows at the top are the
ones the second labeller and the frontier reader also disagree about most, which is the argument
that the boundary is at fault rather than the reader.

**Two implementations of that rule agree, which is the check that catches a wrong reader.**
Python over the exported claims answers 82.5% of 5,579 frozen claims at 80.3%; the tool over its
own reading of the same games answers 82.9% of the 5,266 whose spans it still cuts, at 80.7%.
Four tenths of a point. Training is no longer the third opinion on this: it scores at one
threshold, so its 90.1% at 77.2% answers a different question, and the third implementation is
now `score-export` when a reader needs checking in a hurry. `frontier.py` applied the single
threshold to every subject until 2026-09-13 and would have quietly reported the wrong reader.

**The reader was also carrying the wrong figures about itself.** `reader.json` ships
`usual_declined` and a block of what the model did on games it never saw, and a report of a game
nobody has labelled prints both: *it answers 90% of them and names the same subject 77% of the
time*. Both came from the training record at the single threshold, so the reader that declines
about a sixth of its claims was advertising a tenth, and the warning for a corpus declined far
above usual measures against exactly that number. It would have fired on ordinary games. Where
lines are exported the figures now come from the same folds the lines were drawn on, 41 games and
32,339 claims under the rule that ships, and carry `measured_on` so nobody has to guess which
question they answer: 80.4% answered at 76.9%. That is a shade under what the frozen games then
measured (82.9% at 80.7%), which is the direction an estimate fitted on its own data should err
in, and it is the estimate available at export time rather than an hour later.

### The weakest subjects are bad boundaries, and the labellers disagree first

The table above named the subjects the shipped threshold should not be answering. The obvious
reading is that the reader has not learned them and needs more labels or more parameters.
`training/confusion.py` was written to test that, because the direction of a confusion says
which cure applies: scattered across many subjects means a starved row, concentrated on one
other means a boundary the sheet has not drawn. Out of fold over twenty-eight games, the five
largest confusions in the whole matrix are concentrated on one other subject each, and then the
labels were checked without the model in the way at all.

| the biggest confusions, both ways, out of fold | claims |
|---|---|
| `difficulty` and `gameplay` | 357 |
| `verdict` and `genre` | 248 |
| `gameplay` and `content` | 245 |
| `offtopic` and `verdict` | 236 |
| `story` and `gameplay` | 163 |

**`story` is over-predicted and it takes it from `gameplay`.** Precision 0.58 on 491
predictions, and 95 of its mistakes were labelled `gameplay`. Its most confident mistakes are
claims that contain the word "story" and are about something else: the main story being short
is `content`, going down a predetermined path is `gameplay`, a game managing without a story is
`gameplay`. **The reader is keying on the word rather than on what the claim is about.**

Then the same question, asked of the labellers alone. Of the 18,907 training labels, 624
contain a story word, and 35 of those also say something about length. Those 35 were labelled:

| where "how long is the story" went | claims |
|---|---|
| `content` | 7 |
| `story` | 5 |
| `gameplay` | 4 |
| `difficulty` | 4 |
| `verdict` | 3 |

Five subjects for one question, and no majority. The sheet has a rule for this and it lives in
the wrong entry: `gameplay` says "How much game there is belongs to content", and the `story`
entry says nothing about amount at all. A rule stated under one category is a rule labellers
apply when they are reading that category's paragraph and not otherwise.

**`genre` is over-predicted too, and here the sheet states the rule and the labels ignore it.**
432 training claims name a kind of game. The sheet is explicit: a judgement with only the kind
of game attached, "excellent city builder", is a `verdict`, and `genre` is for when what kind of
game it is, or which it resembles, is the point. Of the 152 claims that name a kind *and* carry
a judgement word, labellers wrote `genre` 57 times and `verdict` 33. Nearly two to one against
the rule. The reader learns the labels, not the sheet, so it over-predicts `genre` (903 said
against 830 true) and `verdict` loses 151 claims to it, the second largest confusion in the
matrix.

**A third boundary is the largest disagreement between the two labellers and the largest
confusion the reader has.** Over the 1,400 claims read twice, the commonest subject split is
`difficulty` against `gameplay`, 25 claims, and the reader repeats it 357 times out of fold: it
is the biggest pair in the matrix in both directions. The sheet is as explicit here as anywhere:
balance complaints belong to `difficulty` rather than `gameplay`, *including when they name a
specific mechanic as overpowered or useless*. Of the 97 training claims that say something is
overpowered, nerfed, buffed or unbalanced, 50 went to `difficulty` and 18 to `gameplay`. So the
rule is followed half the time, which is the same failure as `genre` and for the same reason: a
rule in prose competes with an instinct, and the instinct is that a claim naming a mechanic is
about the mechanic.

This one wants no sheet edit, because the sheet already says it. It wants the adjudication,
which is exactly what the 192 split claims in `gold.html` are: the claims the two labellers
answered differently, shown to a person who settles them. Two of the three boundaries named
here are already in that file by name.

**So the cure for the reader's weakest subjects is not more labels and not more parameters.**
It is two sheet edits and a relabelling of the claims they touch, and neither edit is a new
category:

1. The `story` boundary gains the amount rule it is missing: how much story there is belongs to
   `content`, the same way how much game there is already does. `story` is what the narrative is
   and whether it is worth following.
2. The `genre` boundary gains the counter-example the rule needs, because stating the principle
   was not enough: "a great roguelike" is `verdict`, "it is a roguelike" is `genre`, and the
   test is whether removing the judgement leaves a claim that still says something.

Both are clarifications rather than new meanings, but both change which claims belong where at
the margin, so they wait for the labeller and are done as one pass with the relabelling rather
than piecemeal. Queued behind the revisit draw. This is also the sharpest argument yet for the
human adjudication the user has taken on: the two boundaries it will settle are already known
by name.

### Things tried that bought nothing, so nobody tries them again

**A better uncertainty score.** Twelve of them, in the table above. Out of fold over
twenty-eight games the spread between best and worst is 2.0 points of coverage against an
interval 2.3 points wide, and the p-norm family that was expected to win is the flattest part
of it.

**Post-hoc logit adjustment, the last long-tail method untried here.** Train with plain
cross-entropy and then subtract `tau * log(prior)` from each logit at inference, which divides
out the prior the model absorbed and is what Bayes says to do where reweighting only
approximates it (Menon et al., ICLR 2021, arXiv:2007.07314). Measured out of fold over
twenty-eight games by `training/adjust.py`, cross-fitted so a subject common in one game cannot
set its own correction:

| tau | macro F1 | accuracy | AUGRC | answers at the promise |
|---|---|---|---|---|
| **0.00** | **0.638** | **0.699** | **0.0926** | **85.8%** at 0.750 |
| 0.10 | 0.635 | 0.697 | 0.0931 | 85.5% at 0.750 |
| 0.25 | 0.635 | 0.694 | 0.0941 | 85.4% at 0.750 |
| 0.50 | 0.632 | 0.692 | 0.0963 | 84.7% at 0.750 |
| 1.00 | 0.621 | 0.676 | 0.1052 | 80.9% at 0.751 |

**Zero wins every column.** Not a trade between macro F1 and coverage, which is what the
literature warns to expect: both get worse, monotonically, from the first step. The starved rows
it is aimed at do not move either, and `vr` goes the wrong way, F1 0.305 down to 0.179.

That is the fourth attempt to fix class imbalance with a decision rule rather than with data:
loss reweighting at exponents 0.3, 0.5 and 0.7, and now this. All four failed, and the reason is
in the table's own left column: `licensing` has **8** held-out claims and `vr` has 23. No
reweighting of a gradient and no shift of a logit invents a class the model has barely seen.
This is the arithmetic behind roadmap item 6, and it is why 1,800 mined candidates are worth
more than any further arithmetic on the ones already labelled.

**Three epochs instead of five.** 0.588 accuracy and 52% coverage against 0.639 and 67%. The
model was not overfitting at five; it was underfitting at three.

**Seven epochs instead of five.** The other side of the same question, measured 2026-09-17 on
the frozen games at the threshold the validation games chose: 91.6% at 0.771 against wave11's
90.1% at 0.772, accuracy 0.7408 against 0.7372, macro F1 0.6783 against 0.6728. Read as a table
of wins it looks like a small gain, and it is not one.

**AURC is flat, 0.1056 against 0.1061, and the calibration error is 12% worse, 0.1910 against
0.1699.** AURC has no threshold in it, so it measures the ordering the abstention line is drawn
through rather than where the line happened to land. Two more epochs did not improve the
ordering; they made the model more confident without making it more right, and the extra
coverage is the line sliding down a curve of the same shape. That is what overfitting looks like
before it reaches the accuracy column, and it is the answer the three-epoch entry above left
open: five is not a floor the model was underfitting against, it is where this model stops
learning and starts hardening.

The rule this leaves behind is worth more than the result. A configuration that moves coverage
and accuracy while AURC stands still has not been improved, it has been re-thresholded, and the
two are told apart by the one column that has no threshold in it.

### A 256-token window is better on every mean and clears nothing, and the fifth seed is why

Measured 2026-09-17, five seeds against the five the shipped configuration already had, every
figure from the frozen games at the threshold the validation games chose. The seeds are paired:
the same seed fixes the same head initialisation and the same shuffle in both configurations, so
the difference can be read seed by seed rather than as two clouds.

| | wave11, 128 tokens | window-256 |
|---|---|---|
| coverage | 90.23 [89.30, 91.72] | **91.80** [90.80, 93.87] |
| accuracy at the line | 0.7760 | 0.7763 |
| accuracy | 0.7407 [0.7365, 0.7440] | **0.7429** [0.7406, 0.7444] |
| macro F1 | 0.6791 [0.6728, 0.6871] | **0.6857** [0.6798, 0.6932] |
| AURC | 0.1052 [0.1029, 0.1073] | **0.1016** [0.1000, 0.1061] |

It wins the mean of all five and separates on none of them.

| seed | wave11 AURC | window-256 | difference |
|---|---|---|---|
| 1 | 0.1061 | 0.1000 | -0.0061 |
| 2 | 0.1073 | 0.1010 | -0.0063 |
| 3 | 0.1040 | 0.1001 | -0.0038 |
| 4 | 0.1029 | 0.1007 | -0.0021 |
| **5** | 0.1059 | **0.1061** | **+0.0002** |

Paired t is -2.96 on 4 df, about p = 0.04; the sign test, which assumes nothing about the shape,
gives four of five at p = 0.19. The whole result turns on whether a t-test with five pairs and
one reversal is worth believing. **Not adopted.** The effect is real and consistently signed and
it sits inside the noise this project has twice been caught by, and it costs a much longer
training run and gradient accumulation to fit a 24 GB card at all.

**The method failure is the part worth keeping.** At three seeds and again at four, this was
written up as "the ranges do not touch" and "the worst window seed beats the best wave11 seed".
Both were true of the seeds in hand and both were wrong, because seed 5 landed at 0.1061, inside
wave11's range, and the pre-registered question (does it stay under wave11's best of 0.1029?)
came back no.

That is the second time in this project a fifth seed has overturned a conclusion drawn at four.
The first was the seed bar itself, declared too generous at 1.11 points until the fifth seed
took it to 2.42. Twice is a pattern: **nothing is concluded from four seeds here, however clean
the four look, and a range that does not overlap at n=4 is not a finding.** The reason it keeps
being the fifth is not mystical, it is that four samples of a quantity with this much spread
routinely look separated by luck, and the check that catches it is the next sample rather than
any amount of rereading the first four.

### The frontier model wins, and that is the finding

Measured 2026-09-11, with Claude Opus 5 as the frontier model. It was given the category sheet
the labellers work from,
471 frozen claims stratified twenty to a subject, each inside the review it came from, and the
same right to abstain the reader has. The shipped reader was then run over **exactly those
claims**, because the reader's usual frozen figure is over the natural distribution, which is
a quarter `verdict`, and two numbers from two distributions are not a comparison:

| on the same 471 claims | answers | accuracy where it answers | macro F1 | polarity |
|---|---|---|---|---|
| Claude Opus 5, zero-shot, given the sheet | **99.6%** | **87.0%** [83.6, 89.7] | **0.873** | 94.1% |
| the reader, claim alone, 278M (2026-09-11) | 60.7% | 74.8% [69.5, 79.5] | 0.525 | 87.4% |
| the same, claim in review, 278M | 74.5% | 72.6% [67.8, 77.0] | 0.578 | 85.1% |
| the same again, 560M (2026-09-12) | 85.4% | 73.6% [69.1, 77.7] | 0.648 | 87.2% |

It is not close, and the gap that closed is coverage rather than accuracy. The bigger backbone
answers a quarter more of the claims at the same accuracy, which the intervals say is the same
accuracy and not a worse one, and reaches two thirds of the frontier model's macro F1 where the
small one reached three fifths. What has not moved is the frontier model answering essentially
every claim, more accurately, on a sheet it was handed once.

Two things to hold onto rather than explain away:

- **The labels were made by a frontier model.** Two labellers agree with each other 90.2% of
  the time on frozen claims, so 87% is close to that ceiling but below it. Some of this figure
  is models of a kind agreeing with each other, and the user's hand-adjudicated gold set is
  what will say how much.
- **What the small model buys is not quality, it is scale.** The 471 claims cost 405,000
  tokens. Cyberpunk alone holds 3.2M claims, which at that rate is about 2.7 billion tokens:
  tens of thousands of dollars, for one game. The reader does the same corpus in about two and
  a half hours on one desktop GPU, for the electricity, with no account and nothing leaving the
  machine. That is the trade, and it should be stated plainly rather than buried.

So the claim this project can honestly make is not that it beats a frontier model. It is that
it gets most of the way there at four orders of magnitude less cost, offline, and that it can
say how far short it falls, because it measured it. The gap is now thirteen points of accuracy
and fifteen of coverage, from twelve and thirty-nine, and it is the number to close.

## Where the reader loses, and what it says about where labels should go

A gap averaged over everything says to work harder. Split by subject, on the same 471
stratified frozen claims, it says what to work on. Measured 2026-09-11 against the 278M reader
that was shipped then, with how much of the set each subject has:

| subject | labels | Opus 5 | the reader | the reader declined |
|---|---|---|---|---|
| licensing | 32 | 90% | **0%** | 65% |
| vr | 57 | 94% | 17% | 44% |
| accessibility | 62 | 76% | **0%** | 71% |
| **atmosphere** | **579** | 85% | **5%** | **70%** |
| language | 64 | 83% | 83% | 17% |
| mods | 152 | 95% | 80% | 15% |
| audio | 226 | 100% | 90% | 5% |

**The failure is abstention, not error.** On the rows it loses, the reader is not answering
wrongly; it is declining 44 to 71% of the claims. That is the design working: it knows it does
not know. It also means the coverage figure, not the accuracy figure, is what more labels buy.

Twenty claims a subject is too few to say anything about a subject, though, and reading that
table as if it could was a mistake made first: it said `atmosphere` was broken. Over all 3,769
frozen claims, where `atmosphere` has 205 of them, it scores F1 0.55, which is the middle of
the pack. The per-subject picture from that larger set is the one to trust:

| | labels | frozen claims | F1, 278M | F1, 560M |
|---|---|---|---|---|
| licensing | 32 | 24 | **0.00** | 0.21 |
| accessibility | 62 | 17 | 0.08 | **0.10** |
| community | 98 | 5 | 0.09 | 0.17 |
| vr | 57 | 17 | 0.25 | 0.34 |
| language | 64 | 6 | 0.37 | 0.50 |
| **mods** | **152** | 35 | **0.92** | 0.80 |
| atmosphere | 579 | 209 | 0.55 | 0.65 |
| gameplay | 3,162 | 604 | 0.55 | **0.67** |
| price | 422 | 77 | 0.77 | 0.86 |
| verdict | 4,114 | 877 | 0.70 | **0.81** |

**The two halves of that table have different cures, and the bigger backbone proved it.** Every
row improved with capacity alone, on the same labels, but not equally: the rows with thousands
of labels gained ten to eleven points (`gameplay` 0.55 to 0.67, `verdict` 0.70 to 0.81), and the
starved rows are still broken (`accessibility` 0.08 to 0.10, `community` 0.09 to 0.17). So a
diffuse row was never short of labels, it was short of a model able to tell two overlapping
things apart; and a starved row is short of labels and no backbone will invent them. `mods`
falling from 0.92 to 0.80 on 35 claims is the width of that sample, not a regression.

**Label count predicts the bottom of that table and nothing above it.** Every subject under
about 150 labels is broken, and every one of them is a row the fifteen new games were chosen
to feed. Above 150 the count stops mattering: `mods` scores 0.80 on 152 labels while
`gameplay` scores 0.67 on 3,162, because `mods` announces itself and `gameplay` is where
everything lands that is not something else. `price` on 422 beats `verdict` on 4,114.

So there are two different problems wearing the same face. Starved rows are fixed by labelling
the games that raise them, which is in progress. Diffuse rows were never short of labels, and
capacity moved them where labels had not: what remains for them is boundary text that says what
they are not.

**The cheapest fix for a starved row is not a new game, it is the claims already in the set.**
`mods` went from nothing to 0.92 by being drawn out of `content` and `updates` by its own
words: 213 claims asked, 155 moved. The same draw for the four broken rows, using the words
`licence`, `adaptation`, `faithful`, `accessibility`, `subtitles`, `remap`, `vr`, `headset`,
`toxic` and their neighbours, finds **458 claims across 34 games** already labelled as
something else. At the rate the `mods` pass moved, that is roughly three times the current
labels for `licensing`, `accessibility`, `vr` and `community` combined, without crawling or
drawing a single new review. It is the first thing to spend labelling quota on.

**Spent 2026-09-13, and the rate did not carry over.** 381 of the 527 revisit claims were
relabelled across seven games, and 48 moved, **a net gain of six labels** for the four rows
together: 14 moved in and 8 moved out, most of the traffic being `vr` and `verdict` swapping
places on "worth buying a headset for". The `mods` pass moved 73% because `mods` did not exist
when those claims were first labelled; these four rows did, and the first labeller had mostly
put the claims where they belong. The remaining 146 revisit claims, spread one to fourteen
across thirty games, are left unlabelled on that measurement: at 1.6% net yield they are worth
about two labels, and the labeller slot they would take is worth a hundred on the mined draw.
A revisit is for a subject that was born after its claims were labelled, and for nothing else.

### The reference set is a fifth more English than the corpus it speaks for

Found 2026-09-15, from the outside: another session asked whether a complaint in an English
review of a 63%-Japanese game reflected the game or its translation, and answering it meant
asking what the reader's Japanese is worth. The answer is that nobody knows.

The frozen games cannot answer it: they hold 4,017 English claims and 55 Japanese, so the
Japanese interval runs from 52.3% to 76.6% and is compatible with the reader being fine and
with it being ten points worse. The first instinct was to spend labeller quota on it.

**The evidence already existed.** Every non-frozen game is held out by exactly one
cross-validation fold, the folds are already on disk because the shipped abstention lines are
drawn from them, and the language of each claim is in the label set beside it. Joining the two
gives 32,339 claims answered by a model that never trained on the game they came from, which is
six times the frozen set and thirty times its Japanese. `training/language.py` does the join.

| | library | out-of-fold claims | agreement |
|---|---|---|---|
| english | 52.5% | 22,989 | 70.4% [69.8, 71.0] |
| german | 3.7% | 1,224 | 72.2% [69.6, 74.7] |
| french | 2.6% | 810 | 72.2% [69.0, 75.2] |
| turkish | ~1% | 399 | 71.7% [67.1, 75.9] |
| brazilian | 3.8% | 642 | 66.0% [62.3, 69.6] |
| spanish | 3.7% | 672 | 65.9% [62.3, 69.4] |
| russian | 6.6% | 1,197 | 64.6% [61.8, 67.2] |
| polish | ~1% | 341 | 64.5% [59.3, 69.4] |
| japanese | 63% of one game | 342 | 64.0% [58.8, 68.9] |
| **schinese** | **15.5%** | **2,088** | **63.9% [61.9, 66.0]** |
| **koreana** | 1.9% | 365 | **59.5% [54.3, 64.4]** |

So it is not unmeasured and it is not fine. German, French and Turkish match English or beat
it. **Simplified Chinese is 6.5 points below English on intervals that do not overlap, and
Korean is eleven below.** Chinese is one review in six of the whole library: the second-largest
language in the corpus is read measurably worse than the first, and no report page says so.

The protocol draws "roughly seven claims in ten English" deliberately, so the set is not
inherited from whichever language a corpus happens to favour. That was decided when the library
was smaller and never re-examined against what it grew into: **the library is 52.5% English and
the reference set is 71%.** The deficit tracks it. This is a training-data problem before it is
anything else, and the fix competes with the mined draw for the same labeller quota, which is
the first time languages and starved subjects have wanted the same resource.

### Half of that deficit is the labeller, not the reader

The obvious reading of the table above is that the model is worse in those languages. Measured,
it is not one thing. The labeller's own doubt travels with every claim: `ambiguous` when they
called the boundary contested, `confidence: low` when they hedged. Scoring a reader against a
label its author doubted measures the doubt as much as the reading, and the doubt is not spread
evenly across languages.

Across the eleven languages with enough claims, a language's hedging rate predicts its
agreement almost exactly: **r = -0.78 between low-confidence rate and agreement, and -0.92 with
Polish left out.** The labeller hedged on 67% of Korean and Japanese claims against 56% of
German, French and Turkish.

The obvious objection is that hedging and mis-splitting travel together: a labeller handed a
badly cut claim would hedge on it and flag the cut, so the correlation might be measuring the
splitter of the day rather than the language. It is not. Dropping every claim anybody flagged
as badly cut takes the hedging range from 14.5-27.3% down to 11.6-20.7% and leaves the
correlation where it was, at **-0.80**.

So the same measurement, restricted to the 40% of claims the labeller marked neither contested
nor low-confidence:

| | all claims | settled only | how much of the gap survives |
|---|---|---|---|
| english | 70.4% | 89.3% | |
| tchinese | -5.1 | -1.2 | 24% |
| polish | -5.9 | -2.0 | **34%** |
| koreana | -10.9 | -5.1 | 47% |
| brazilian | -4.3 | -2.4 | 56% |
| schinese | -6.4 | -3.8 | 58% |
| russian | -5.8 | -3.5 | 60% |
| **japanese** | -6.3 | **-7.7** | **122%** |
| **spanish** | -4.5 | **-6.3** | **143%** |

Three different problems wearing one face, and they want different work:

- **Korean, Chinese, Russian, Brazilian Portuguese: about half is labelling.** Korean's ten and
  a half points become five. What is left is real and smaller than it looked.
- **Polish was the splitter, and the splitter is already fixed.** 34.7% of Polish claims were
  flagged badly cut, more than double any other language. But that flag is what a labeller said
  against the splitter of the day, and `stale-splits` broken out by language says only **9.0%
  are still cut that way, below English's 9.8%**. `claims-5` fixed three quarters of it. What
  survives is the labels: they were written on the bad cuts, at the highest low-confidence rate
  of any language (27.3%), and two thirds of Polish's gap disappears once those hedged claims
  are dropped. Polish needs its labels revisited, not its splitter touched.
- **Japanese and Spanish are the reading.** They are the only languages whose gap *grows* when
  the doubtful claims are dropped. That is the model, and it is the case for weighting or for
  labels.

One confound, stated here and settled below: every label here was written by Claude Fable 5.1, so
"the labeller was less sure in Korean" may mean those claims are genuinely harder or may mean
that model is weaker in Korean. A second model reading the same claims separates them, and one
has now done so: see "The labels are not worse in any language".

The lesson about method is worth as much as the finding: the answer to "we have no evidence
about X" was a directory of logits that had been sitting there for four days, written for a
different question. Before spending a resource that cannot be spent twice, check what the last
measurement already paid for.

### Weighting the languages made every language worse, and so did dropping them

Two runs, at opposite ends of the same dial, against `wave11` on the same 4,017 frozen English
claims and the same frozen games. `wave13` gives a claim weight by the inverse of its language's
share, the mirror of the per-subject balance already in `train.py`. `english-only` drops every
non-English claim before the split, so the frozen games are scored on the languages the model
was taught.

| frozen claims | claims | wave11 | wave13, weighted | english only |
|---|---|---|---|---|
| english | 4,017 | **74.0%** | 73.9% | 73.5% |
| russian | 291 | **68.0%** | 66.7% | |
| schinese | 260 | **71.2%** | 70.8% | |
| german | 191 | **81.2%** | 77.5% | |
| polish | 166 | **77.7%** | 77.1% | |
| brazilian | 142 | **70.4%** | 69.0% | |
| spanish | 122 | **75.4%** | 72.1% | |

**Every language with a hundred claims or more got worse under weighting, including the six it
upweighted.** Six of six moving the same way is not a thin-sample accident; it is the same
result the subject axis has already returned four times. Loss reweighting at three exponents and
post-hoc logit adjustment all failed to fix the subject tail, and the reason recorded there
holds here unchanged: no reweighting of a gradient invents evidence the model has barely seen.
Japanese has 348 labelled claims in the whole set and Korean 344. Weighting them more heavily
does not make them more numerous; it makes the model fit them harder, and the frozen games say
what that costs. The gains in the table's tail, French at 47 claims and Italian at 16, are
single claims moving a percentage by two points and should not be read.

The other end of the dial says the same thing from the other side. Training on English alone
costs English **0.5 points of accuracy and 1.3 of coverage**, both inside the measured seed bar,
so the honest reading is that it changes nothing. That answers the question it was run for: a
separate high-accuracy English model has nothing to be more accurate with. One confound is
stated rather than resolved, and it points the same way: `english-only` also saw 29% less data,
so its loss could be volume rather than language. That excuse was available to it and it still
did not win. A better English model needs more English labels, not fewer foreign ones.

So the language axis is not a training problem. English is flat from one end of the weighting
dial to the other, which means the non-English claims are neither helping English nor hurting
it. They are free, and the reader should keep reading them.

What remains open is the **promise**, which is a different mechanism entirely: the line, not the
weights. `training/lines.py` asks whether the per-subject abstention rule that ships keeps its
75% language by language, out of fold and leave-one-game-out, and what a line per language would
cost. That question needs no training run and no new labels.

### The promise is broken in Korean, and the per-subject line barely helps

Measured 2026-09-16 by `training/lines.py` over the `wave11` folds: 32,339 out-of-fold claims
from 41 games, every policy fitted leave-one-game-out and applied to the game left out.

| policy | answers | at | the language it fails hardest |
|---|---|---|---|
| one line for everything | 85.1% | 75.0% | koreana **63.9%**, 11.1 short |
| a line per subject, what ships | 80.4% | 76.7% | koreana **66.7%**, 8.3 short |
| a line per language | 84.7% | 75.0% | polish 73.7%, 1.3 short |
| a line per subject and per language | 75.8% | 78.6% | polish 74.5%, 0.5 short |

**The per-subject line recovers 2.8 of the 11.1 points and leaves the other 8.3.** That is the
finding. It was reasonable to assume a rule drawn per subject would carry the language gap with
it, because a language the reader struggles in should show up as the subjects it struggles in.
It does not. Korean claims are spread across the same subjects as everyone else's and the line
those subjects get is drawn overwhelmingly from English claims, because 71% of the set is
English. A Korean `gameplay` prediction at 0.62 clears a bar set by English `gameplay`
predictions, and then is wrong a third of the time.

So every report of a Korean corpus prints the same sentence about what its rates are worth as an
English one, and for Korean that sentence is false. This is the identical defect the per-subject
line was introduced to fix, on an axis nobody checked.

Coverage and delivered accuracy under the shipped rule against the joint one:

| language | claims | ships | delivered | both lines | delivered |
|---|---|---|---|---|---|
| english | 22,989 | 80.8% | 77.6% | 77.4% | 78.9% |
| schinese | 2,088 | 77.3% | **71.5%** | 66.8% | 76.3% |
| german | 1,224 | 84.4% | 78.1% | 82.4% | 78.8% |
| russian | 1,197 | 78.6% | **71.5%** | 68.3% | 76.4% |
| french | 810 | 82.8% | 78.2% | 81.0% | 78.8% |
| spanish | 672 | 74.9% | 75.0% | 66.4% | 76.7% |
| brazilian | 642 | 80.5% | **73.1%** | 72.7% | 76.4% |
| turkish | 399 | 84.0% | 77.9% | 83.0% | 78.2% |
| koreana | 365 | 73.2% | **66.7%** | 54.8% | 76.0% |
| japanese | 342 | 71.3% | **71.3%** | 60.5% | 75.8% |
| polish | 341 | 83.6% | **69.5%** | 67.7% | 74.5% |
| italian | 217 | 82.0% | **72.5%** | 72.4% | 76.4% |

The trade is legible and it is the one this project already chose once: a language the reader
reads well answers more, and a language it reads badly answers less and stops lying. Korean
coverage falls from 73% to 55%, and what is left is worth the sentence printed under it.

**A line per language alone answers more than the per-subject rule that ships**, 84.7% against
80.4%, at exactly 75%, and hands German 93.2%, French 92.6% and Turkish 94.5% where the shipped
rule gives them 84.4%, 82.8% and 84.0%. It is not the recommendation, because it drops the
per-subject floor and the argument for that floor has not changed: the rule that maximises
coverage under one floor abandons the rare subjects, and the rare complaints are the ones worth
finding. Both lines, and a claim answers only when it clears both.

**What the joint rule costs is 4.6 points of overall coverage**, 80.4% down to 75.8%, and it
over-delivers at 78.6% rather than 75%. The over-delivery is the composition being conservative:
taking the stricter of two lines each drawn at 75% lands above 75%, and some coverage is being
paid for nothing. Fitting the pair jointly would recover part of it and cannot be done on this
evidence, because the cells are (26 subjects x 29 languages) against 344 Korean claims.

**One defect this exposes and does not fix.** Below the evidence bar `mondrian` pools a class
into a shared line, which on the subject axis is right. On the language axis it produces
Indonesian answering 75% of its claims at **33.3% correct**, on eight claims, and Bulgarian the
same on four. A pooled line fitted mostly on English does not hold for a language with eight
claims behind it, and the honest answer for a language with too little evidence to draw a line
is to decline, not to answer at a third right.

### Both lines ship, and a language with too little evidence declines

The rule is the stricter of the two: a claim is answered only when its confidence clears its
subject's line and its language's. `training/export.py` fits both from the same folds and writes
`language_thresholds` beside `thresholds`; `Provenance::bar` takes the maximum. A reader exported
before this existed carries no language map and reads exactly as it did.

Fitted on the `wave11` folds at a 100-claim bar, seventeen languages have a line and twelve
decline. The bars are the finding in one column: `koreana` 0.929 and `polish` 0.926 against
`german` 0.573, `french` 0.603 and `ukrainian` 0.313. The reader has to be nearly certain before
it will say anything about a Korean claim, and that is what keeping the promise costs.

The answer cache had to learn about language too. It was keyed on the review hash and claim
index, or on the claim's text alone when reading without context, and "10/10" is the same two
characters in every language on Steam: one cached answer would have been handed to a claim that
clears a different bar.

### The non-English half got 1,717 more claims, and a draw now has to say what it is

Thirteen games gained a `multilingual` draw: 1,717 claims in Chinese (simplified and
traditional), Japanese, Korean, Thai, Russian, German, French, Spanish, Portuguese and Italian.
The per-language lines were fitted on a set where twelve languages had too little evidence to
carry a line at all, which is the fallback the lines exist to replace.

The draw is made with `--english 0.0`, and that selects on **Steam's own language tag**, not on
the text. A review tagged `koreana` and written in English is drawn as Korean. Six of the eight
labellers reported it independently, so it is a property of the data rather than of one batch:
treat a per-language figure as a figure about the tag, which is also what the reader sees at
inference, so the two agree and neither is measuring the script.

A draw like this cannot land beside the random sample. Prevalence figures are computed over
whatever is in the game's directory, so a deliberately skewed selection merged into the random
one silently rewrites what that game is claimed to be about, with nothing in the file recording
the skew. `sample-claims --subset` names the draw and places it in its own subdirectory, and the
name is validated against `TEACHING_SETS`, the four the reader knows: `declined`, `mined`,
`retrieved`, `multilingual`.

### The labels are not worse in any language, and the Korean deficit is the reader's

Opus read 1,062 of the new non-English claims blind, from the same batch files Fable was given,
over six games chosen for how much Chinese, Japanese and Korean they carry. Two models
converging is the only instrument available for the question, because "the labeller hedged in
Korean" and "Korean claims are harder" look identical inside one model's output.

| | claims | subject agreement | kappa |
|---|---|---|---|
| Fable against Opus, English random draws | 1,400 | 86.0% | 0.838 |
| **Fable against Opus, non-English draws** | **1,062** | **87.6%** | **0.864** |

Non-English agreement is not lower. Per language, with Wilson intervals because a thirty-claim
row has no business using a normal approximation:

| language | n | agreement | 95% interval |
|---|---|---|---|
| schinese | 257 | 84.4% | 79.5 to 88.3 |
| german | 136 | 91.2% | 85.2 to 94.9 |
| french | 100 | 86.0% | 77.9 to 91.5 |
| brazilian | 97 | 83.5% | 74.9 to 89.6 |
| russian | 84 | 91.7% | 83.8 to 95.9 |
| koreana | 82 | 84.1% | 74.7 to 90.5 |
| japanese | 78 | 88.5% | 79.5 to 93.8 |
| tchinese | 35 | 97.1% | 85.4 to 99.5 |
| thai | 30 | 86.7% | 70.4 to 94.7 |

**Every interval contains the English figure of 86.0%.** Not one language is distinguishable
from English at this sample size, Korean included. So the Korean supervision is as sound as the
English supervision, and the reader needing 0.929 confidence before it will speak about a Korean
claim is a fact about the reader, not an echo of shaky labels. That is a harder problem than a
labelling problem and it belongs to the model.

Hedging is the one place the two models part. Fable marked 62.2% of Korean claims low-confidence
or ambiguous, the highest of any language with enough claims to say so; Opus hedged on Korean at
63.4% but hedged about as much everywhere (60.7% Chinese, 63.1% Russian, 58.0% French), so its
rate carries no signal about Korean and Fable's does. Fable is more hesitant in Korean than
elsewhere while still landing on the same subject as Opus 84% of the time, which is a
calibration quirk rather than a comprehension one.

What this does not show: that either model is *right*. Two models sharing a blind spot look
exactly like two models agreeing, and nothing here would catch it. The gold page is still the
only instrument that settles correctness rather than convergence.

The disagreements that remain are mostly the sheet, not the language. The two commonest are
`gameplay` against `graphics` and `offtopic` against `policy`, nine each, and both are boundary
questions the sheet does not answer sharply. Those would show up in English too.

### Half the set has been read twice, and the labellers' own doubt sorts it

Opus read 26 of the 49 labelled games in full, blind, from batches regenerated with no labels in
them: 16,310 claims in `<set>/opus`. The run stopped on a weekly quota with 23 games and 12,614
claims still drawn and waiting, so every figure here is over half a corpus and will move.

| | claims | subject | kappa |
|---|---|---|---|
| Fable against Opus, everything read twice | 16,310 | 87.0% | 0.855 |
| frozen games only | 3,271 | 88.4% | 0.868 |
| validation games only | 3,204 | 86.6% | 0.850 |

**The hedge flags are the finding.** Split the same claims by whether either labeller marked
`low` confidence, `ambiguous` or `split_wrong`:

| | claims | subject | kappa |
|---|---|---|---|
| neither flagged | 7,161 | **99.0%** | 0.988 |
| either flagged | 9,149 | 77.7% | 0.756 |

Two models that share no weights land on the same subject 99.0% of the time on the claims both
were sure about. That is not a silver standard on that slice, and it is a claim the first
labelling could not make at any sample size, because one model agreeing with itself is not
evidence. It also means the set sorts itself: the 44% neither reader doubted needs no
adjudication, and the 56% either doubted is where a person's time is worth spending. The gold
page should draw from the second group, not uniformly.

**Three classes carry nearly all the disagreement**, and reading the claims behind them showed
the first diagnosis here was wrong:

| first reader said | claims | held | went instead |
|---|---|---|---|
| genre | 698 | **73.9%** | verdict 128 |
| updates | 1,016 | 81.6% | verdict 81 |
| offtopic | 1,477 | 82.1% | verdict 77 |
| controls | 352 | 95.2% | gameplay 8 |
| audio | 161 | 95.7% | gameplay 1 |

This was recorded as a defect in the sheet. It is mostly not. On `genre` the sheet already
decided the case, in the verdict rule and with the very example at issue: "a judgement with only
the kind of game attached, 'excellent city builder', is a verdict". Nearly all 128 are that exact
shape, "great platformer", "god tier city builder". **Fable was not following a rule that was
already written, and Opus was.** The gameplay rule's blunt "naming the genre belongs to genre"
was read first and won; it now defers to the verdict rule, and the genre rule says plainly that
better-or-worse is a verdict however specific the noun.

`updates` runs the other way. The rule already puts praise and blame aimed at the studio there,
and Fable followed it while Opus read "I hate EA for destroying this franchise" as a verdict.
**Neither labeller is the better one.** They break in opposite directions on different rules,
which is the whole reason a second reading is worth more than a longer first one.

The genuine gaps were narrow and are now closed: a judgement about how the game has changed
since release is `updates` even when no patch or studio is named, and a single word carrying an
attitude is a `verdict` while one carrying none is `offtopic`.

No category changed, so the categories fingerprint does not move and no label is invalidated.
What the amendment does is make a re-ask worth running: `revisit` exists for exactly this, and
the claims to re-ask are the ones the two readings answered differently, not the whole set.

Compare `controls` at 95.2% and `audio` at 95.7%: where the sheet draws a line, two independent
readers find it. That is the test a rule has to pass, and it is now the test for the amended ones.

**Opus hedges more than Fable everywhere**, 50.9% against 39.1%. Comparing hedge rates between
the two models says nothing; comparing one model's rate across languages or subjects does.

Nothing here shows either model is right. Two models sharing a blind spot look exactly like two
models agreeing. What it does give is 2,116 claims the two answered differently, 72 of them with
both readers confident, and that short list is the most valuable thing a person could adjudicate.

### The blind sample was a sample of the schedule, not of the frozen games

Found while costing the rest of the second labelling, and it would have ruined the one figure
this whole project exists to produce.

`draw` treated a claim as a blind candidate only when the second labeller had *not* reached it.
Everything read twice became a control or a disagreement instead. So the accuracy sample was
whatever the second pass had not got to yet, and with 26 of 49 games read that meant **1,000
blind claims from four of the ten frozen games, 486 of them from one game**. A figure computed on
that is a figure about Warhammer 40,000: Rogue Trader wearing the name of the corpus.

It is worse than a bias, because it moves. Finishing those four games, which was the plan an hour
earlier, would have taken the blind pool to **zero**: every frozen claim would have been read
twice and none would have been eligible. The sample would have silently emptied as the labelling
got more complete.

Coverage of the second reading is a fact about scheduling. Every frozen claim is now a blind
candidate, and the draw spreads across all ten games in their own proportions: the largest share
fell from 48.6% to 23.3%.

Fixing that exposed a second bias underneath it, and completing the frozen games is what made it
visible. The disagreements were taken for the split pool before the blind sample was drawn, so
the sample got whatever was left. While four frozen games were unread that looked fine. Once all
ten had been read twice, **1,000 of 1,000 blind claims were ones the two labellers had agreed
on**: the easy half of the corpus, and an accuracy figure over it would have been flattering by
construction.

The blind sample is drawn first now, from every frozen claim, and the disagreements are whatever
it did not take. The draw went from 100% agreed claims to 885 of 1,000, which is the true rate on
frozen English claims rather than an artefact of the order the two pools were filled in.
`the_blind_sample_is_not_only_the_claims_the_labellers_agreed_on` holds it, and also checks that
no claim is asked twice, once without answers and once with them.

The separate pool of agreed-claim controls is retired with it, along with `--settled`. A sample
over every frozen claim already contains the claims both labellers answered the same way, in
their true proportion: 538 of the 1,000. Scoring those apart afterwards is the same check without
a second draw that had to be kept indistinguishable from the first.
`the_blind_sample_does_not_depend_on_where_the_second_labelling_got_to` holds it.

**What this changes about the remaining work.** The blind sample no longer depends on the second
labelling at all, so finishing the other 23 games is no longer a prerequisite for gold. What it
still buys is a corpus-wide agreement figure and more disagreements for a queue that is already
larger than anyone will answer.

### What the gold pass will be, settled before a single question is asked

The adjudication happens once and never again, so everything that decides which claims are put in
front of a person is settled first. Decided 2026-09-19:

- **English only.** The draw would otherwise be 27% Chinese, Russian, German, Japanese, Korean and
  Thai. A question the adjudicator cannot read is worse than one never asked: it sits in the
  count, it cannot be skipped honestly, and whatever goes in it wears the one label here allowed
  to be called truth. The cost is stated rather than hidden: this produces an accuracy figure
  about English, it cannot validate the per-language lines, and the Korean and Chinese promises
  stay measured model against model.
- **1,000 blind claims**, which is roughly plus or minus 2.5 points on the figure. Five hundred
  would be 3.5, wide enough to swallow the difference between two model versions.
- **The 39 answers in amended rows are asked again.** Of the 110 already given, 39 were answered
  under wording that has since moved and the file never recorded which sheet they answered. They
  are set aside in `gold-reask.json`, so the second answer can be read against the first. The
  other 71 are in categories the amendment did not touch and carry over.

Three things had to be fixed before any of that was safe, and all three were the same shape as
bugs this project has already paid for:

- The page recorded the sheet and the splitter; the **exported answers did not**. Every answer now
  carries both, stamped per row rather than per file, because answers get merged and re-exported
  separately. `ingest-gold` refuses a batch answering different wording from the build's.
- `serve` **replaced** the answer file with whatever the page posted. The page posts what browser
  storage holds, and that storage is keyed by the sheet and the size of the draw, so redrawing
  opens an empty session that would have posted nothing over a finished adjudication. Answers now
  merge by claim, and `a_fresh_session_cannot_post_away_a_finished_adjudication` holds it.
- The draw could not see the second model's reading at all, and asked disagreements in file order.

### The clarification moved 344 labels of 627, and cost the right to re-measure them

The 627 claims where the two readings differed inside the amended rows were put back to Fable,
the labeller that wrote the first pass, with the three sharpened rules stated and an explicit
instruction that answering the same way again was a fine outcome. **344 of them moved**, 55%.

That is the size of the problem the amendment was fixing, and it confirms what reading the
claims suggested: Fable was not applying rules that were already written. `genre` against
`verdict` was 226 of the 627, `offtopic` against `verdict` 181, `updates` against `verdict` 153.

The gold queue fell from 1,804 disagreements to 1,575, and the claims neither labeller hedged
from 86 to 57. Those are questions that no longer need a person because the sheet answers them.

**What it cost, which was not free.** The re-ask was not blind. Fable was told the rules, and
those rules are the ones Opus had already been applying, so the second answers were nudged
toward the other reading. Agreement over all 20,072 claims reads 88.5% afterwards against 87.0%
before, and that 1.5 points is the nudge, not a discovery.

So the cross-model figure to quote is the one measured before any of this: **87.0%, kappa 0.854,
over 20,072 claims, both readings independent**. Excluding the re-asked claims and quoting 89.8%
would be worse than useless: those 627 were selected precisely because they disagreed, so
removing them raises the average by construction.

The labels are better and the measurement of them is spent. That is the right way round, because
labels feed the trainer and the agreement figure is only ever a description. It is worth knowing
before the next such trade: a re-ask that names the rule cannot also be an independent reading,
and if both are wanted they have to be different claims.

### The sheet stopped having a name, because the one time it needed bumping it was not

The sheet carried a version somebody chose: `core-4`, `core-5`, `core-6`. Those names appear in
the history above and stay there, because they record what actually happened. Nothing carries one
any more.

The amendment two sections up is why. Boundary rules moved, and `CORE_SPINE_VERSION` stayed at
`core-6`, so labels written before and after claimed to answer the same sheet. Nobody would have
been able to tell them apart afterwards. That is the exact drift the name existed to prevent, and
it failed the first time it was tested, which is what a name assigned by hand does.

It also turned out to be two jobs wearing one string, and that is why the bump was not obvious:

- **What the categories are.** A model's output means whatever the categories mean, so a model
  trained under different ones is answering a different question. `taxonomy::categories()` hashes
  the ids. This is what a stored reading and a trained reader are checked against.
- **What the sheet says.** A label answers the wording its labeller read, boundary rules and all.
  `taxonomy::sheet()` hashes the whole brief. This is recorded on every label, and it is how
  `revisit` finds the ones that predate a clarification.

Today's amendment moves the second and not the first, which is exactly right: a clarified sentence
changes what a labeller should answer and changes nothing a model already emitted. Under one
string there was no way to express that, so the only options were to charge a full library re-read
for a reworded sentence or to say nothing, and saying nothing is what happened.

`core-6` named this same set of category ids, so every reader and reading on disk is accepted by
name rather than refused: `a_reader_written_before_the_rename_still_loads` pins that against the
shipped `reader.json`. `core-5` and earlier held different categories and stay refused, which is
the guard working.

The word `spine` is gone too. It meant the same thing as "the sheet" and "the taxonomy", and three
words for one concept is three chances to think they are different things.

### The ingest destroyed 2,068 labels, and the shape of the bug is worth keeping

`ingest-claims` merges by intersection: it takes the labels that name a claim the target set
drew, and writes the result. Pointed at the wrong set the intersection is empty, so it wrote an
empty file over a finished labelling. Four games lost 2,068 labels that way, 1517290 losing 161
of them without that being noticed in the first report.

Two beliefs made it worse and both were false. `reference/claims/*/labels.json` **is** tracked in
git: only the review text is ignored, so the labels were never gone and `git checkout HEAD --`
returned them with their exact `splitter` and `taxonomy` values. A recovery script written to
rebuild them from `training/data/claims.jsonl` was built on the opposite belief and is deleted.
Check what version control holds before describing anything as lost.

The fix is a refusal, not a warning. `Error::Refused` fires when a merge would place no label at
all while holding labels it could not place: that is the signature of a merge aimed at the wrong
set, and it is never the signature of an honest one, because a real merge places at least one.
`a_merge_that_places_nothing_writes_nothing` holds it. The cost of the refusal is running the
command again with `--to`; the cost of its absence was four games.

### A 256-token window wins on every measure, and on none of them alone

| | frozen at the validation line | accuracy | macro F1 | AURC | minutes |
|---|---|---|---|---|---|
| wave11, 128 tokens | 90.1% at 0.772 | 0.7372 | 0.6728 | 0.1061 | 17 |
| window-256 | **92.0% at 0.776** | **0.7444** | **0.6849** | **0.1000** | 27 |

Every movement is inside its own seed bar: coverage 1.9 points against 2.42, accuracy 0.4
against 0.70, macro F1 1.2 against 1.39. Six measures all moving the same way is the signal, and
it is the same argument used against `wave13` in the other direction, so it has to be accepted
here or withdrawn there. AURC is the one worth most: it has no threshold in it at all, so a
better AURC says the confidence ordering itself improved rather than a line landing luckily.

It costs 59% more training time and needs `--accumulate 2` to fit on a 24 GB card at all, which
is why the flag exists: halving the batch instead would have moved two things at once and
answered neither. Not adopted on one seed. Five seeds decide it.

**The measurement trap this run walked into, recorded because it will recur.** `run.json` holds
two frozen figures and they are not comparable. `test.threshold_coverage` refits the threshold
*on the frozen games themselves* and is the best that set can be made to look; the console line
and `test.at_validation_threshold` score the frozen games at the threshold the *validation* games
chose, which is the only one that says what a new game would get. Reading one run's refit figure
against another run's transferring figure made a run that wins on all six measures look like it
lost 4.3 points of coverage. Quote `at_validation_threshold`, or quote nothing.

### An option the reviewer left blank is not a hard claim, it is an unanswerable one

Steam reviews are full of ballot templates: a list of options with boxes, one ticked. The
splitter has collapsed these to the ticked line since `claims-5`, but only 12,112 of the
reference set's 31,019 labels were cut by it. `claims-3` cut 15,210 and `claims-4` cut 3,697,
and both kept every blank option as a claim of its own. So the set still holds thousands of
fragments whose text means the opposite of what it says: `☐ Worth the price` is the reviewer
saying the game was not.

They surfaced in the gold draw, where they are far more concentrated than their share of the
set: 36 of 2,575 questions overall, but 16 of the 57 sure disagreements at the front of the
queue. That ratio is the finding. Two labellers reading an unchosen option almost never land
the same way, because there is no right answer to land on, so every one of them is promoted
into exactly the section reserved for the questions worth a person's time.

Held back at the draw and refused at ingest, both through one predicate in the splitter that
owns the box characters. Both ends, because the draw only protects a page generated after this
and the answers file already on disk was not. 218 labels held back; the five already answered
were all called `offtopic`, which is the only answer available and still not a true one.

Not fixed by re-cutting. Labels and the adjudicator's answers are keyed to `(review_id,
index)`, so a new splitter run renumbers the claims underneath both. The filter is where the
fix belongs until there is a reason to re-cut everything at once.

**The part not fixed:** these fragments are in the training data too, where nothing filters
them. `wave11` was trained on a set that is 61% pre-ballot-handling. Whether that costs
anything is a measurement nobody has made, and it only matters at the next retrain.

## The corpus stopped being a corpus of games people like

Measured 2026-09-11 over all 51 captures, 7.5M reviews. Before the fifteen chosen games
landed, 24 of 36 were Very or Overwhelmingly Positive, the median was 86%, and **nothing in
the set was Overwhelmingly Negative**. A classifier trained on that has never read a corpus
where the complaint is the point.

| positive share | games |
|---|---|
| below 40% | 4 |
| 40 to 69% | 10 |
| 70 to 84% | 11 |
| 85% and up | 26 |

The median is still 85%, which is what Steam is: a store where most reviewed games are liked.
What changed is the tail. Fourteen games now sit below 70% and seven below 50%, against
almost none before, and the floor is The Day Before at **15.7%** across 23,177 reviews, which
is the only Overwhelmingly Negative corpus of any size on the platform.

That matters for two different reasons and they are worth keeping apart. A model that has
only read praise has never learned what a complaint about `story` looks like as against a
complaint about `bugs`. And a tool that reports a mention rate has to work on the game that is
one long argument, not only on the game everyone agrees about.

## Where this is going

Settled 2026-09-11: **the dataset is the thing, and the benchmark comes before the polish.**
The aim is the reference people cite for what players say about games, which is a higher bar
than a tool that works. Measured against that bar, four of the seven things such a dataset
needs are already exceeded or met: it is reproducible without redistributing a word anybody
wrote, every label names the splitter and taxonomy it was made under, two labellers read a
tenth of it blind and agree at kappa 0.85, and the test games are fixed by hash and chose
nothing. Two are missing, and no amount of further labelling closes either.

1. **Human-adjudicated labels. Confirmed 2026-09-12: the user is doing this.** Everything so
   far is a model agreeing with a model, which the README says plainly and which no citation
   can rest on. The user adjudicates: a random thousand from the frozen games, labelled blind,
   for an accuracy figure that means what it says; then the claims the two labellers split on,
   shown both answers, to settle the boundaries. After the sheet was settled, never before.

   A useful thing to know before it starts: partial answers are worth something, so stopping
   early is not wasted work. Prediction-powered inference takes a small human-labelled sample
   and a large model-labelled one and returns an interval on the human-truth figure that is
   valid however wrong the model is, and tighter than the human sample alone would give
   ([arXiv:2301.09633](https://arxiv.org/abs/2301.09633)). The human sample here is the
   adjudicated claims and the model-labelled one is every claim label already written, so each
   answered question narrows the interval from the first one onward, and there is no threshold
   below which the exercise has produced nothing.

   **The tool exists as of 2026-09-11.** `steamgauge gold` writes one self-contained page
   holding 1,000 blind claims from the frozen games and every claim the two labellers split on
   there, which as the set has grown to ten frozen games is 194 of them. A letter picks a
   subject, a digit the polarity, and a claim with both moves on by itself; answers are kept as
   they are made, because fourteen hundred claims is not one sitting. `steamgauge ingest-gold`
   reads them back and prints the share that matches the labeller already on record, which is
   the first figure in this project that may be called accuracy. Chrome drives the page in CI.

   **`--language`, because the adjudicator reads two of them.** A question somebody cannot
   answer is worse than one never asked: it sits in the count, it cannot be declined honestly,
   and whatever they put is noise wearing the only label here allowed to be called truth. The
   draw restricted to English and German is 1,144 questions, 1,117 and 27. What it produces is a
   random sample of those two languages rather than of the corpus, and every figure from it has
   to say so.

   **`--settled`, because nothing else checks the labels at the easy end.** Every other question
   is drawn from a claim one labeller read alone or two read differently, so the set measures
   the reader exactly where the labellers were unsure and nowhere else. Fifty claims both
   labellers agreed about are mixed in unmarked, at their own reviews' places in the order, and
   they are the only thing that can test the assumption the whole silver standard rests on: that
   two labellers agreeing means both were right rather than both wrong the same way. Scoring
   them separately from the blind draw is not optional, because they are the easy end by
   construction.

   **What holds the page is a flag pressed last, not a flag existing.** The first rule was that
   any flagged claim waits for an arrow, which meant flagging and then answering, the ordinary
   order, left the reader pressing a key nothing told them about on every flagged claim. The
   page now advances when the polarity completes the answer and waits only when the flag came
   after it, which is the case the rule was for.

   **`--serve` as of 2026-09-12, and it is the way to run it.** The page as written keeps
   answers in `localStorage` and only the Export button gets them out, which puts the sole copy
   of a thousand questions of somebody's own judgement somewhere a cleared cache loses, and
   makes the person answering into the backup. Asked to open the page, the user's first
   question was whether they had to export, and the second was that if so it is bad design.
   They were right. `steamgauge gold --serve` binds the loopback address and takes every answer
   as it is made, writing beside the target and renaming so a crash mid-write cannot leave a
   fragment where the adjudication was, and the page reads the file back when it opens: the
   disk is the copy that matters and the browser is a cache. Nothing is exported and nothing
   leaves the machine. Opened as a file the page behaves exactly as before, because a page that
   needs a server to show a question would be a worse page.

   Frozen-only gives 27 splits rather than the four hundred first estimated, because a tenth
   of each set is read twice and 271 claims at 93.2% agreement leaves 27. A disagreement
   settles a boundary and measures nothing, so it may come from any game, and the default
   draws from all of them: 1,000 blind and 192 split, out of 1,400 claims read twice.
2. **A comparison against the alternatives. Done, and it does not flatter this project.**
   Four baselines run over the same frozen claims and the same abstention protocol, in "What
   it has to beat" above, including Claude Opus 5 asked the same question directly. It answers
   99.6% of them at 87.0% where this reader answers 60.7% at 74.8%. The honest claim is not
   that this beats a frontier model but that it gets most of the way there for the electricity
   rather than for tens of thousands of dollars a game, and can say how far short it falls.

Then, in order:

3. **`core-6` and `claims-5` together, once.** `reference/GAPS.md` holds the wording for every
   rule, each traced to a labeller who could not see the others. The contested rate of 29% and
   the `difficulty` against `gameplay` confusion say the sheet is the ceiling now, not the
   model. Measure the relabel cost on one game before paying it for thirty-six.
4. **Games chosen for the rows that are starved**, not more games at random. `licensing` has
   32 claims over four games, `vr` 40, `accessibility` 48, and all three score zero. A random
   game costs the same as a chosen one and buys almost none of them. Fifteen such games are
   captured and drawn as of 2026-09-11: two VR games, two about accessibility, five licensed,
   and four at the negative end of a distribution that was 24 Very Positive games out of 36.
   One is labelled (546560, 675 claims, 16 of them `vr`, which is 40% more `vr` than the whole
   set held). The other fourteen are drawn and waiting: 14,036 claims against the 19,582 the
   set holds. What stopped it was the labelling quota, not the plan.
5. **Draw the claims the reader cannot answer, not more claims at random. Built 2026-09-12.**
   Every set so far is a random draw, which is what makes prevalence measurable and is the
   right default. But once a reader exists, the claims it abstains on are worth several times a
   random claim to train from. `steamgauge declined` draws them, uniformly rather than from the
   least confident, because the bottom of a confidence ordering is mostly text with nothing in
   it. Three things keep it from contaminating anything: every row carries `subset: declined`
   and no prevalence figure counts one, the draw refuses any game the model does not already
   train on, and a review already in that game's random set is never drawn twice. It waits on a
   reading made by the current splitter, which the finalise pass produces.
6. **Mine the twenty million unread claims for the starved rows. Settled 2026-09-12: this
   happens, and the lexical half of it is built.** Item 4 chooses whole games in the hope that
   they hold `vr` and `licensing` claims; this goes straight at the claims themselves.
   Retrieval-based selection for class-imbalanced data is the published form
   ([arXiv:2307.14899](https://arxiv.org/pdf/2307.14899)), and the argument for it is
   arithmetic: reweighting redistributes a gradient that 32 `licensing` claims do not contain,
   which is why every weighting scheme tried here made macro F1 worse. Rows drawn this way are
   not a random sample and no prevalence figure may count them, exactly as with the declined
   draw, and `steamgauge mine` marks every one of them `mined`.

   **`steamgauge mine` ships as of 2026-09-12**, with a written probe list per starved subject
   in `mine.rs`, a round-robin quota so a game rich in one subject cannot eat the draw, and the
   same refusal as the declined draw to touch a game held back from training. The retrieval
   half, which is the stronger one, is still to build: probes are lexical and mostly ride on
   borrowed tokens, so a Russian review complaining about subtitles is not caught.

   **Read what a probe catches before spending a labeller on it**, with
   `cargo run --release -p steamgauge-core --example mine-check -- <app id>`. Four of the eight
   lines were wrong on their first outing and the corpus said so within a minute: "accessible"
   is how reviewers say a game is easy to get into, "launcher" is a weapon in a shooter, "mods"
   is an in-game upgrade tree in half the library, and "vive" is "long live" in French. Those
   four narrowings cut the `mods` line from 5,513 claims to 778 and the `policy` line from 367
   to 60, and what is left reads like the subject it is aimed at. The test
   `the_wrong_sense_of_a_word_does_not_take_a_line` holds each of them.

   **Which games to mine is most of the yield, and `mine-check` answers that too.** Run over
   all 35 training games it found the rows are not spread thinly across the library, they are
   concentrated: 546560 holds 73,096 `vr` candidates where 548430 holds 150, 553850 holds
   11,916 `policy` where most games hold under 50, 990080 holds 5,139 `licensing` where the
   median game holds 60. Nine games cover all eight rows, and a draw of 200 claims from each
   returns **1,800 candidates balanced 205 to 231 per subject**, against label counts that
   today run from 32 to 214. Drawn 2026-09-12 and waiting on the labeller.

   **The first six games labelled, 2026-09-12: half of every mined claim lands on a starved
   row.** 1,200 labels, 595 on the eight rows the draw was cast for, against roughly 3% for a
   random draw. `mods` gained 186 labels on 146 held before, `compatibility` 97 on 164,
   `policy` 91 on 214, `language` 85 on 63. Two things the yield says that the candidate
   counts did not:

   - **`licensing` is 12 labels from one game and none from five**, because the line fires on
     adaptation vocabulary and only 990080 adapts anything. The row cannot be filled from games
     picked for other rows; it needs a sports game or a tie-in, and a draw aimed at it alone.
   - **`vr` is rare even in headset games.** 546560 holds 73,096 candidates and gave 8 labels;
     629730 held 14,102 and gave 5. In a game that only exists in a headset, mentioning the
     headset is not a claim about it: "the VR combat is tight" is `gameplay`. What the sheet
     calls `vr` is comfort, tracking and which headsets work, and reviewers of a headset game
     say those things about as rarely as anyone else. 45 labels from six games doubles the row,
     and it is not going to be filled by mining headset games harder.

   **The retrieval half is built and it needed the common subjects to work.** `steamgauge mine
   --by-neighbour` embeds every labelled claim and walks the corpus for the ones that sit nearer
   a starved subject's labelled claims than any common subject's. The first version fished with
   the starved subjects alone, and a quarter of what it caught was "great game": a short generic
   claim sits near every short claim, and one short query on a line pulled in every short claim
   in the corpus. Putting `verdict` and `gameplay` in the water fixed it outright, which is the
   whole difference between "nearest to `vr`" and "nearer to `vr` than to anything else". It
   crosses languages the probes cannot: the same run caught "we need chinese", "我们需要中文"
   and "Necesitamos chino" for `language`, and "Ryzen 2700, RTX 2070, 16GB RAM" for
   `compatibility`, which no word list holds. The narrowest margin per line is printed and is
   the figure to read: negative means the line scraped the floor for a subject the game does
   not hold, and its catch will mostly say `gameplay`.

   **Labelled 2026-09-13, and retrieval beats the probes on every row.** Same games, same
   budget of 200 claims each, the labeller told nothing about how either was drawn:

   | draw | games | labels | on a starved row | `vr` | `accessibility` | `community` | `licensing` |
   |---|---|---|---|---|---|---|---|
   | lexical probes | 9 | 1,800 | 848 (47%) | 56 | 44 | 67 | 13 |
   | retrieval, by margin | 10 | 2,000 | 1,328 (66%) | 93 | 139 | 126 | 19 |

   Game by game the gap is wider than the totals show, because the two draws were run on the
   same corpora and the retrieval draw could not take a review the probes had already taken.
   On 553850 the probes landed 39% and retrieval 74%; on 438100, 68% against 90%; on 546560,
   `vr` went from 8 labels to 39, because retrieval finds the comfort-and-tracking sense the
   sheet means where the probe found the word. On 296970, a game `mine-check` said held almost
   nothing, retrieval landed 70 of 200. Before either draw `accessibility` had 60 labels, `vr`
   41 and `community` 98; the probes added 44, 56 and 67, and retrieval 139, 93 and 126.

   `licensing` did not move under either method, 13 and 19, and that is the same finding as
   before: it needs games that adapt something, and none of these do. It is the one row the
   next draw has to be aimed at by choosing games rather than by choosing claims.

   **So it was, 2026-09-13, and the row went from 64 labels to 202.** Six games that adapt
   something (two football games, a football manager, a basketball game, a rally game and a
   card game) were drawn with `--only licensing`, 150 claims each, every other subject's
   labelled claims voting against as before. Only the twenty `licensing` queries cast, and the
   narrowest margin was -0.031, which is the scrape-the-floor figure the run prints for a
   subject the game barely holds; the draw was still worth labelling, and a word probe over
   the drawn text (licence, official, real team) said which games before a labeller was spent:

   | game | licence words in the draw | `licensing` labels of 150 |
   |---|---|---|
   | 3551340 | 34 | 30 |
   | 1665460 | 20 | 36 |
   | 2669320 | 18 | 33 |
   | 1449850 | 18 | 22 |
   | 690790 | 14 | 15 |
   | 2878980 | 4 | 2 |

   138 `licensing` labels from 899 claims, at a cost of six labellers, against 32 from the
   whole random set. The basketball game is the instructive miss: it holds every licence there
   is, so nobody reviewing it mentions one, and the retrieval had nothing to find. A row that
   is starved because the games do not hold it is filled by choosing games, and the word probe
   is a free check on whether a chosen game actually holds it.

   Two labellers on these sets hit the same seam independently, a real competition that is
   missing (`licensing` by the RULE, `content` by the description), and each drew the line
   differently; `reference/GAPS.md` has the entry and the rule that settles it.

   Two things about the run itself. It was slow twice for reasons that had nothing to do with
   the method: the first release binary was built without `--features directml` and ran the
   encoder on the CPU, and the second embedded claims in review order, where every batch holds
   one wall of text that pads two hundred and fifty short claims to five hundred tokens. Sorted
   by length, the way the embedding pass already does, it runs at eleven hundred claims a
   second and a three-million-claim game takes forty-five minutes. And a review drawn under one
   teaching set was not excluded from the next, which would have labelled it twice; one had
   been, and the fix reads every teaching set's sample before drawing.
7. **Publishing**, which is the user's decision and not near.
8. **Induced per-game categories** for the remaining games, one agent call of about 70k tokens
   each, behind everything else.

Built since this list was first written: the report page on readings, the polarity split,
corrected prevalence, the second reading and its comparison, the fetch-by-checksum path, the
words that stand out on each side of a subject, the paragraph, the bake-off, the timeline,
languages and induced subjects in the window, the sweep, the language switch, `claims-4` and
the span join that let it ship mid-run, `claims-5` after it, the adjudication page and the
ingest behind it, the frontier comparison, the configuration sweep and the tool that reads it
(`training/sweep.py`), the teaching draw, the abstention line per subject, and the reading
batch settled at 256.
