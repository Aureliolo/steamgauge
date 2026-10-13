# Honest limits

- **Reports default to English, so the headline is a fact about players who write English.**
  The capture is always the whole census, every language in it, and the filter is applied when
  the counting happens rather than when the crawling does, so the choice is reversible and no
  corpus has to be downloaded twice. But a mention rate over English reviews is not a mention
  rate over players: on some corpora English is under half of what was written. Every figure
  says which set it is over, and switching the language recounts from the same capture. The
  reason for the default is that evidence nobody can read is evidence nobody can check, and
  being able to open a rate and read what is behind it is the whole design.

- **The reader is measurably worse in five of the languages it reads, and the reference set
  is 70% English.** Steam reviews of many games are far less English than that, so the
  languages the reader sees most outside English are the ones it learned from least.

  Measured over 40,327 claims held out by cross-validation, each answered by a model of the
  recipe that ships that never trained on the game it came from, over every claim whether or
  not the reader would answer it (`training/language.py`); the last column keeps only the claims
  the labeller marked as sure:

  | | claims | agreement | 95% interval | labeller sure |
  |---|---|---|---|---|
  | english | 28,250 | 75.8% | [75.3, 76.3] | 93.0% |
  | turkish | 529 | 76.0% | [72.2, 79.4] | 95.6% |
  | german | 1,604 | 75.2% | [73.0, 77.2] | 92.0% |
  | polish | 354 | 74.6% | [69.8, 78.8] | 92.4% |
  | french | 1,089 | 74.2% | [71.5, 76.7] | 89.4% |
  | brazilian | 868 | 73.5% | [70.5, 76.3] | 91.7% |
  | **russian** | **1,508** | **71.4%** | **[69.1, 73.6]** | **92.1%** |
  | **spanish** | **841** | **70.2%** | **[67.0, 73.1]** | **87.4%** |
  | **japanese** | **489** | **69.9%** | **[65.7, 73.8]** | **84.5%** |
  | **schinese** | **2,573** | **69.6%** | **[67.8, 71.3]** | **89.6%** |
  | **koreana** | **518** | **65.3%** | **[61.1, 69.2]** | **88.3%** |

  Turkish, German, Polish, French and Brazilian Portuguese match English. **Russian, Spanish,
  Japanese, Simplified Chinese and Korean sit below it on intervals that do not overlap, Korean
  by ten points.** Nothing on a report page says so. Ten of the twenty-nine languages have fewer
  than a hundred held-out claims and are not quotable at all.

  Part of each gap is the labels rather than the reading, since the column is agreement with one
  labeller and carries that labeller's noise. On the claims the labeller was sure of, Russian's
  gap all but closes and Korean's and Simplified Chinese's halve; Spanish and Japanese stay as
  wide.

  **The promise holds in every language.** The reader says a claim it answers is right three
  times in four. A line drawn per subject alone is set mostly by English, which is most of the
  set, so a line is drawn per language as well and a claim is answered only when it clears both.
  The cost is coverage, most of it in the languages read worst: the line a Korean or a Spanish
  claim has to clear is three times the English one. A language with fewer than a hundred
  labelled claims declines rather than borrowing a line fitted on English.

- **The model declines claims it is not sure about, and those are counted rather than hidden.**
  A claim below the threshold gets no subject and is reported as unclassified. That is a real
  answer and an honest one, but it means a mention rate is a rate over the claims the model
  would commit to. The share it declined is printed beside it, and a large one is a finding
  about the corpus rather than a footnote.

  On the held-out games, which it never trained on, the reader that ships answers **91.5%** of
  the labelled claims and agrees with a labeller on **81.5%** of those. Its lines are drawn for
  what they promise, not moved to make that coverage look better, and the share it declines is
  printed beside every rate.

  The model carries what it usually declines, drawn from the folds its abstention lines were
  fitted on, so a corpus that declines far above it can be reported as a finding rather than a
  footnote. The reader that ships carries **10.9%**.

  A game that declines 1.2 times that and five points more is flagged, and the flag has two
  causes: a corpus about something the taxonomy lacks, and a corpus written largely in the
  languages the reference set covers worst, whose lines are highest. Those want opposite
  responses, the first a category and the second more labels, so the page names the languages
  in the corpus the reader has to be surer about than English, and those it has no line for at
  all, with the reviews written in each.

- **A threshold chosen on a few games may not transfer to a new one.** The threshold promises
  an accuracy, and that promise is measured on the games that chose it; fitted on too few games,
  it can fall well short on games held out of training. So every figure in the model card comes
  from the held-out games, and the validation figures stay in the run record where they belong.
  Which games are held out is fixed by a hash of each game's id, so adding games never moves one
  across the line.

- **The tool has to reproduce the training measurement, and when it does not the tool is
  wrong.** They are separate implementations of one question: the trainer builds the window
  around a claim in Python, the tool builds it in Rust over a corpus it split itself. Three
  ways of asking exist so that any two can disagree, and each removes a suspect: Rust over the
  exported claims, Python over the same, and the tool over its own reading of the game. A fault
  in how one of them builds the window around a claim leaves nothing in the output looking
  wrong, and shows only as one of the three answering below the other two, which is the
  argument for keeping all three.

- **Claim share is verbosity-weighted and never a headline.** Counting opinions instead of
  people lets whoever writes most set the numbers, which is the same distortion this tool
  exists to expose at the top of the pile. The headline is always the share of reviews.

- **Nearly two thirds of the reference set is labelled twice, and the rest is labelled once.**
  Reliability is measured on the part read twice and assumed for the rest. That is not the same
  as a set where every label was adjudicated. The silver standard's own error is estimated rather
  than known, and the word accuracy still does not apply: the labels were written by a model, so
  what is measured is consistency between two models.

  Measured over 21,449 claims on every one of the 51 games, with the ten held-out games read
  twice in full: two labellers agree on the subject 88.6% of the time, kappa 0.87, and on
  polarity 94.9%, kappa 0.92.

- **The contested flag measures the labeller as much as the claim.** Two labellers given the
  same definition reached for it on 27.4% and on 43.9% of the same 21,449 claims, kappa 0.46.
  The flag does find the right claims: where neither reached for it the two agree on the
  subject 98.8% of the time, where one of them did 82.4%, and where both did 72.5%. What
  differs is the bar. So a game's contested rate is not compared with another game's, and
  agreement is reported over the contested claims as a floor on how hard the taxonomy is rather
  than as a property of the corpus.

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

- **A claim is split mechanically, and the splitting is sometimes wrong.** Every label records
  whether its labeller thought the claim was cut in the wrong place, and every rule in the
  splitter answers some of those reports: a list of short comma-separated parts, "stunning
  visuals, calm music, epic story", is that many claims rather than one carrying three
  subjects. Re-cut with the splitter this build ships, **9.1%** of the 30,414 randomly drawn
  claims are still cut the way the labeller objected to (`--example stale-splits`). It is a
  floor rather than the rate: a claim the splitter cuts differently from the one the labeller
  read is not thereby cut correctly. The rate is measured rather than assumed, because it is
  the one error in this pipeline that no amount of training fixes.

  A label names the span of the review it was written about, not a position in a list, so the
  splitter can change under a labelled set: a label whose span this build no longer cuts as one
  claim is counted as unjoined and said, rather than scored against whatever sentence now sits
  at its old index. A reading names its claims the same way, so the two are joined by the bytes
  they cover and neither has to be told which rules cut it before it can be believed.

- **How often labellers find a claim genuinely contested varies more than the claims do.**
  28.9% overall, but from 13.5% on one game to **52.1%** on another. Some of that is the games,
  and some of it is labellers reading "two subjects both fit" more or less strictly. It is the
  clearest argument for reading the set twice: contested is the one field with no way to check
  itself.

- **"Every review" means every review Valve will serve, and the request parameters decide how
  many that is.** Three of the API's defaults quietly remove a large slice. `purchase_type`
  defaults to Steam purchases only, dropping activated keys, `filter_offtopic_activity`
  defaults to excluding review bombs, and `languages` defaults to English alone. The reviews the
  first two leave out are not a random slice: a review bomb is negative by definition. A census
  that accepts the defaults understates exactly what it is trying to measure. This one always
  sets all three.

- **The default order is worse than the defaults above.** Steam's default ranks reviews by
  helpfulness, which is the top of the pile, and widens its own range of days until it finds
  something rather than running out, so a walk in that order never reaches the end of a corpus.
  Reading that would reproduce precisely the bias this tool exists to remove. The census
  paginates by creation date instead, and reports the coverage it achieved against Valve's own
  stated total as a figure you can read rather than a claim you have to trust.

- **Some reviews are genuinely unreachable, and a shortfall must never be blamed on that
  without checking.** Reviews from deleted and private accounts are counted in Valve's totals
  and cannot be retrieved by anyone. Steam separately, and intermittently, just stops serving
  a window early: pages arrive full, then one arrives empty long before the window is
  exhausted, which looks exactly like reaching the end. Walking the same window again returns
  everything. A crawler that accepts the first answer therefore undercounts a window at a time
  while reporting every shard as finished, so this one compares each window
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
