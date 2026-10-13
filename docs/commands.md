# The command line

The SteamGauge program opens the window when it is started with no arguments, and runs one of
these commands when it is given one. On Windows it is a windowed application, so its output
reaches a terminal only through a pipe: Git Bash makes one by itself, and in PowerShell you add
one, as in `steamgauge --help | Out-Host`. [Install](install.md) says which ways of installing it
put `steamgauge` on the `PATH`. Every command also takes `--help`.

<!-- steamgauge-site: commands -->

Three commands get you a page: `crawl <app id>`, `read <app id>`, `report <app id>`. `embed` is
needed only by `distinct`, and `claims` only writes the split out for something else to read.
Everything else is how the model is measured and improved, and none of it is needed for a report.

Nothing leaves this machine. Reviews are downloaded from Valve and everything after that happens
locally, including the model.

- [`crawl`](#crawl): Download every review Valve will serve for an app into an immutable Parquet
  capture, and the titles and dates of what its developer has posted on Steam.
- [`sweep`](#sweep): Bring a capture up to date: fetch every review written or edited since it was
  crawled, or since it was last brought up to date, and write them beside what is held.
- [`claims`](#claims): Split every review into the separate points it makes.
- [`embed`](#embed): Embed the captured reviews locally, so nothing is sent anywhere.
- [`sample-claims`](#sample-claims): Draw reviews at random, split them into claims, and write
  batches to be labelled.
- [`declined`](#declined): Draw the claims the reader would not answer, for a labeller to teach it
  on.
- [`rare-claims`](#rare-claims): Draw claims that look like the subjects the labelled set has fewest
  labels for.
- [`measure-claims`](#measure-claims): Score stored readings against a claim reference set.
- [`second-opinion`](#second-opinion): Draw the share of a labelled set that a second labeller
  should read.
- [`distinct`](#distinct): Draw the reviews of a corpus that are least like each other, for finding
  what this game's players talk about that the fixed taxonomy has no row for.
- [`ingest-induced`](#ingest-induced): Check a returned list of induced subjects against the handout
  it came from, and keep the ones that survive.
- [`compare-labels`](#compare-labels): Compare two labellings of the same claims, field by field.
- [`ceiling`](#ceiling): Score the model against a label two labellers both reached, and say how
  often they reached one at all.
- [`revisit`](#revisit): Draw the claims of a labelled set that a revision of the sheet puts back in
  question.
- [`ingest-revisit`](#ingest-revisit): Merge revisited labels back, replacing only the claims that
  were asked about.
- [`ingest-claims`](#ingest-claims): Merge returned claim labels into a reference set.
- [`gold`](#gold): Write the page a person adjudicates claims on, from the held-out games.
- [`ingest-gold`](#ingest-gold): Merge adjudicated answers back, as the only labels in the set
  written by a person.
- [`export-training`](#export-training): Write every labelled claim, with its text, as JSONL for
  training.
- [`export-review-facts`](#export-review-facts): Write what the published dataset says about every
  labelled review, as JSONL.
- [`export-pool`](#export-pool): Write a draw of unlabelled claims, with their text, as JSONL for a
  teacher to read.
- [`store-facts`](#store-facts): Ask the store whether each game is played only in a VR headset, and
  keep the answer beside its capture and its reference sets.
- [`read`](#read): Read every claim in a corpus with the trained model.
- [`recount`](#recount): Count a corpus again from the readings already on disk, without the model.
- [`report`](#report): Render a self-contained page from what the reading pass found.
- [`export-data`](#export-data): Write a read game's data as CSV and JSON, for a spreadsheet or your
  own analysis.
- [`update`](#update): Update this copy to the newest release, as Update now does in the window.
- [`mcp`](#mcp): Steer the app from a Model Context Protocol client, which starts this command and
  speaks to it on standard input and output.
- [`clients`](#clients): The MCP clients this command can add the app to, whether each is on this
  computer, and whether it starts this copy.
- [`connect`](#connect): Add this copy to an MCP client's own configuration, so the client starts
  it.
- [`disconnect`](#disconnect): Take the app out of an MCP client's configuration, leaving everything
  else in it.
- [`brief`](#brief): Write the category sheet labellers work from, generated from the taxonomy.

## `crawl`

Download every review Valve will serve for an app into an immutable Parquet capture, and the titles
and dates of what its developer has posted on Steam.

```text
steamgauge crawl [OPTIONS] <APP_ID>
```

- `<APP_ID>`: Steam app ID, as it appears in the store URL.
- `-o, --out <OUT>`: Directory to write captures and crawl state into. Default: `data`.
- `--pace-ms <PACE_MS>`: Milliseconds between requests, enforced globally across all shards.
  Default: `250`.
- `--concurrency <CONCURRENCY>`: Shards to crawl at once. Pacing is global, so this reorders work
  rather than increasing load on Valve. Default: `4`.
- `--shard-target <SHARD_TARGET>`: Reviews per shard before a date window is split further. Default:
  `40000`.
- `--restart`: Plan a fresh crawl instead of continuing an unfinished one.

## `sweep`

Bring a capture up to date: fetch every review written or edited since it was crawled, or since it
was last brought up to date, and write them beside what is held.

One walk in last-edit order finds both arrivals and edits and stops at the watermark, so this is a
few pages rather than a crawl. Nothing already captured is overwritten: an edited review is held in
both forms, and every pass that reads the capture counts the newer one. Read the corpus again
afterwards, since the readings describe the capture as it was. What the developer has posted on
Steam is asked for again too, so the report marks the game's newest updates.

```text
steamgauge sweep [OPTIONS] <APP_ID>
```

- `<APP_ID>`: Steam app ID, as it appears in the store URL.
- `-o, --out <OUT>`: Directory holding the captures. Default: `data`.
- `--pace-ms <PACE_MS>`: Milliseconds between requests. Default: `250`.

## `claims`

Split every review into the separate points it makes.

A review is not one opinion, and a whole-review vector is the average of the ones it holds. This
writes claims.parquet beside the capture, carrying offsets rather than text so the corpus is not
stored twice.

```text
steamgauge claims [OPTIONS] <APP_ID>
```

- `<APP_ID>`: Steam app ID, as it appears in the store URL.
- `-o, --out <OUT>`: Directory holding the capture. Default: `data`.

## `embed`

Embed the captured reviews locally, so nothing is sent anywhere.

```text
steamgauge embed [OPTIONS] <APP_ID>
```

- `<APP_ID>`: Steam app ID whose most recent capture should be embedded.
- `-o, --out <OUT>`: Directory holding the capture. Default: `data`.
- `--batch-size <BATCH_SIZE>`: Reviews per forward pass. Default: `64`.
- `--model <MODEL>`: Which encoder to embed with. A corpus records this, and vectors from two
  encoders are never compared. One of `e5-small`, `e5-base`, `arctic-m2` or `gte-base`. Default:
  `gte-base`.
- `--precision <PRECISION>`: Which build of the graph to run. fp16 matches the full graph and is
  fastest on a GPU; fp32 is the faster of the two on CPU. One of `fp16` or `fp32`. Default: `fp16`.
- `--model-dir <MODEL_DIR>`: Where to cache the model. Defaults to the platform cache directory.
- `--unit <UNIT>`: What to embed: whole reviews, or the separate points they make. One of `claim` or
  `review`. Default: `claim`.

## `sample-claims`

Draw reviews at random, split them into claims, and write batches to be labelled.

Every claim of every drawn review is labelled, never a subset of them: a review labelled in part
cannot say what share of a corpus names no aspect at all, which is the first thing worth knowing
about one.

```text
steamgauge sample-claims [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to draw from. Each gets its own set.
- `-o, --out <OUT>`: Directory holding the capture. Default: `data`.
- `--reviews <REVIEWS>`: Reviews to draw. Default: `700`.
- `--batch-size <BATCH_SIZE>`: Reviews per batch file. Around forty is roughly a hundred and thirty
  claims. Default: `40`.
- `--seed <SEED>`: Changing this draws a different sample. The same seed always draws the same one.
  Default: `1`.
- `--english <ENGLISH>`: Share of each game's draw written in English. The rest is drawn from
  whatever else the corpus holds, so the model is trained on more than one language even though
  reports default to English. Default: `0.7`.
- `--to <TO>`: Where to write the sets. Each game gets a directory under it. Default:
  `reference/claims`.
- `--subset <SUBSET>`: Name this draw as something other than the random one, which puts it in its
  own directory beside the random draw and marks every review with it. A draw that chose its reviews
  by anything but chance is not a sample of the corpus: `--english 0` makes a set that says nothing
  about what share of a corpus is written in Chinese, and adding it to the random draw would quietly
  change what every prevalence figure is a figure about. Naming it keeps it trainable and keeps it
  out of those. Default: `random`.

## `declined`

Draw the claims the reader would not answer, for a labeller to teach it on.

A random draw spends most of its budget on claims the model already reads correctly. Once a reader
exists, the claims it abstains on are worth several times a random claim to label. What that buys is
a better model and never a better measurement: the set is marked `declined` so no prevalence figure
ever counts it, and only games the model is allowed to learn from may be drawn, because a held-out
game taught from is not held out.

```text
steamgauge declined [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to draw from. Each must be a game the model trains on.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--claims <CLAIMS>`: Claims to draw from each game. Default: `300`.
- `--batch-size <BATCH_SIZE>`: Reviews per batch file. Default: `40`.
- `--seed <SEED>`: Changing this draws a different sample. The same seed always draws the same one.
  Default: `1`.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.

## `rare-claims`

Draw claims that look like the subjects the labelled set has fewest labels for.

A random draw lands on the distribution the corpus already has, and the reader's own uncertainty
cannot point at the rarest subjects either: with a few dozen `licensing` claims behind it, it does
not know the subject is there. So the claims are found by their words, with a keyword list per rare
subject, or by `--by-neighbour`. Marked `keyword` or `retrieved`, which trains the model and
measures nothing.

```text
steamgauge rare-claims [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to draw from. Each must be a game the model trains on.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--claims <CLAIMS>`: Claims to draw from each game. Default: `200`.
- `--batch-size <BATCH_SIZE>`: Reviews per batch file. Default: `40`.
- `--seed <SEED>`: Changing this draws a different sample. The same seed always draws the same one.
  Default: `1`.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.
- `--by-neighbour`: Search with the labelled claims instead of with words. Every labelled claim of a
  rare subject is embedded and the corpus is walked for its nearest neighbours, which finds the
  paraphrases no keyword list has and crosses languages a keyword list cannot. Costs a forward pass
  over the corpus per game and lands in `retrieved/`.
- `--model <MODEL>`: Which encoder to search with, when `--by-neighbour`. One of `e5-small`,
  `e5-base`, `arctic-m2` or `gte-base`. Default: `gte-base`.
- `--precision <PRECISION>`: Which build of the encoder to run, when `--by-neighbour`. One of `fp16`
  or `fp32`. Default: `fp16`.
- `--model-dir <MODEL_DIR>`: Where the encoder is cached. Defaults to the platform cache directory.
- `--embed-batch <EMBED_BATCH>`: Claims per forward pass, when `--by-neighbour`. Default: `256`.
- `--only <ONLY>`: Draw for these subjects only. With `--by-neighbour` the rest still vote against,
  so a draw for `licensing` alone is a draw for `licensing` and not for whatever else is rare; with
  keywords the other lists are simply not matched. Which subjects are worth a draw is a measurement
  that moves, and `steamgauge measure-claims` is what says which they are today.

## `measure-claims`

Score stored readings against a claim reference set.

```text
steamgauge measure-claims [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to score. Each is reported separately, because a model that reads one
  game well and another badly is not a model with one accuracy.
- `-o, --out <OUT>`: Directory holding the captures. Default: `data`.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.
- `--labels <LABELS>`: Which labels to score against, as a directory inside each set: `gold` for the
  ones a person adjudicated, `second` for the second labeller's. The set's own labels by default.
  Only a gold figure may be called accuracy; the rest are agreement.

## `second-opinion`

Draw the share of a labelled set that a second labeller should read.

A set labelled once cannot say how reliable it is. This writes the same reviews again, as fresh
batches with no labels in them, for a different labeller to work from blind.

```text
steamgauge second-opinion [OPTIONS] [APP_IDS]...
```

- `<APP_IDS>`: Steam app IDs to draw a second opinion on. Every labelled set when none are named.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.
- `--share <SHARE>`: Share of each set to read again. Drawing again with a larger share grows a
  reading that exists: the reviews already read stay in it and only the rest are handed out.
  Default: `0.1`.
- `--batch-size <BATCH_SIZE>`: Reviews per batch, as with the first draw. Default: `35`.
- `--seed <SEED>`: Changing this asks about different reviews. The same seed asks about the same
  ones. Default: `1`.
- `--subset <SUBSET>`: Which draw to read again, for the teaching sets that live in their own
  subdirectory. The game's random draw by default.
- `--into <INTO>`: Directory inside the set to write the fresh batches into, which is also where the
  reading will be ingested. A set can hold several independent readings, one per labeller; naming
  them keeps one labeller's answers apart from another's. Default: `second`.

## `distinct`

Draw the reviews of a corpus that are least like each other, for finding what this game's players
talk about that the fixed taxonomy has no row for.

A random sample is mostly "great game", because that is what a corpus mostly is. This draws by
farthest-point traversal over the stored vectors instead, so each review says something the others
do not. What reads the handout and names the subjects is a separate step; this is the sample it
reads.

```text
steamgauge distinct [OPTIONS] <APP_ID>
```

- `<APP_ID>`: Steam app ID whose most recent capture to draw from.
- `-o, --out <OUT>`: Directory holding the capture and its embeddings. Default: `data`.
- `--count <COUNT>`: How many reviews to draw. Default: `120`.
- `--seed <SEED>`: Changing this draws a different pool. The same seed draws the same reviews.
  Default: `1`.
- `--to <TO>`: Where to write the handout. Defaults to reference/distinct/\<app id\>.json.

## `ingest-induced`

Check a returned list of induced subjects against the handout it came from, and keep the ones that
survive.

A model asked for subjects will produce subjects. What makes them a finding rather than a list is
the reviews behind each one, so a subject naming reviews the handout never held, or fewer than
three, is refused and says why.

```text
steamgauge ingest-induced [OPTIONS] --from <FROM> <APP_ID>
```

- `<APP_ID>`: Steam app ID the subjects were induced for.
- `--from <FROM>`, required: The returned JSON: an object with `induced_by` and a `subjects` array.
- `--seed <SEED>`: The seed the handout was drawn with, recorded so the same draw can be rebuilt.
  Default: `1`.
- `--handout <HANDOUT>`: The handout the model read. Defaults to reference/distinct/\<app id\>.json.
- `--to <TO>`: Where to write the surviving subjects. Defaults to reference/induced/\<app id\>.json.

## `compare-labels`

Compare two labellings of the same claims, field by field.

```text
steamgauge compare-labels [OPTIONS] [APP_IDS]...
```

- `<APP_IDS>`: Steam app IDs to compare. Every set with a second opinion when none are named.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.
- `--labels <LABELS>`: Which reading to compare the set's own labels against, as a directory inside
  each set. Two labellers who read different shares of a set produce different figures, so the
  answer is only meaningful beside the name of the reading it came from. Default: `second`.

## `ceiling`

Score the model against a label two labellers both reached, and say how often they reached one at
all.

A model cannot be more right than its labels are. Against one labeller it scores about eighty per
cent, which reads like a score out of a hundred and is not: two labellers agree with each other
about eighty-seven per cent of the time, so that is what there is to win. This prints both over one
set of claims, and the claims the two labellers split on separately, since there is no single right
answer on those.

```text
steamgauge ceiling [OPTIONS] [APP_IDS]...
```

- `<APP_IDS>`: Steam app IDs to score. Every set with a second opinion when none are named.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.

## `revisit`

Draw the claims of a labelled set that a revision of the sheet puts back in question.

A taxonomy revision does not invalidate a set: every subject that survives it means what it meant.
What it moves is boundaries, and only claims near a moved one can change. A claim about a subject
the sheet has just learned to name almost always says its name, so naming the words finds the slice:
eight games of "modding" sat under `content` and `updates` and not one of them failed to use the
word.

```text
steamgauge revisit [OPTIONS] [APP_IDS]...
```

- `--words <WORDS>`: Words that put a claim back in question. A claim using any of them is drawn.
  Left out, every claim under the subjects named is drawn, which is what a revision that narrows a
  row rather than naming a new one puts back in question.
- `--subjects <SUBJECTS>`: Only claims currently filed under these subjects. A rule moves a boundary
  between two rows, and a claim on neither side of it cannot cross. Every subject when none are
  named.
- `<APP_IDS>`: Steam app IDs to draw from. Every labelled set when none are named.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.
- `-o, --out <OUT>`: Directory holding the captures, which say whether this build still cuts a claim
  where a label names one: a claim it no longer cuts is not asked again, as the training export does
  not keep it. A game with no capture here holds nothing back. Default: `data`.
- `--batch-size <BATCH_SIZE>`: Reviews per batch file. Default: `40`.

## `ingest-revisit`

Merge revisited labels back, replacing only the claims that were asked about.

```text
steamgauge ingest-revisit [OPTIONS] --from <FROM> --by <BY> <APP_ID>
```

- `<APP_ID>`: Steam app ID whose labels are being revised.
- `--from <FROM>`, required: Directory holding the returned label files, one JSON array per batch.
- `--by <BY>`, required: Which labeller answered the revisit, as the model's own name.
- `--to <TO>`: The reference set. Defaults to reference/claims/\<app id\>.

## `ingest-claims`

Merge returned claim labels into a reference set.

```text
steamgauge ingest-claims [OPTIONS] --from <FROM> --by <BY> <APP_ID>
```

- `<APP_ID>`: Steam app ID whose labels are being merged.
- `--from <FROM>`, required: Directory holding the returned label files, one JSON array per batch.
- `--sheet <SHEET>`: Which version of the category sheet the labeller was working from. Defaults to
  this build's. Name the older one when a revision landed while they were labelling: a label answers
  the sheet its labeller read, and no other.
- `--by <BY>`, required: Which labeller wrote these labels, as the model's own name. Required, and
  not defaulted: the first set written by a second model is the one where a default would be wrong,
  and it is also the one nobody would think to check.
- `--to <TO>`: The reference set. Defaults to reference/claims/\<app id\>.

## `gold`

Write the page a person adjudicates claims on, from the held-out games.

Everything measured so far is a model agreeing with a model, which the docs say and which no
citation can rest on. This draws two kinds of claim: a random sample with no answer shown, which is
the only reading that produces an accuracy figure rather than a ratification, and the claims two
labellers answered differently, with both answers shown, which settles a boundary rather than
measuring one.

```text
steamgauge gold [OPTIONS]
```

- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.
- `-o, --out <OUT>`: Directory holding the captures. A label names a byte span, and only the capture
  can say whether this build's splitter still cuts one there: where it does not, the claim the label
  was written about no longer exists and nobody is asked about it. Without the captures every stored
  label is asked. Default: `data`.
- `--to <TO>`: Where to write the page. It holds review text, so it is never committed; the default
  sits beside the repository's other ignored artefacts. Default: `gold.html`.
- `--blind <BLIND>`: How many claims to draw blind. Default: `1000`.
- `--labels <LABELS>`: Which second reading to take the disagreements from, as a directory inside
  each set. The two readings decide what a person is asked about, so naming a different one asks a
  different question: `opus` is a second model over the whole set, `second` the tenth a second agent
  of the first model read. Default: `second`.
- `--splits <SPLITS>`: Which games' disagreements to include. The blind sample is always held out,
  because that is what makes it a measurement; a disagreement measures nothing and settles a
  boundary, and a boundary settled on one game is settled for every game. One of `none`, `held-out`
  or `everywhere`. Default: `everywhere`.
- `--seed <SEED>`: Changing this draws a different blind sample. The same seed draws the same one.
  Default: `1`.
- `--language <LANGUAGE>`: Only ask about claims in these languages, repeated once per language. A
  person cannot adjudicate a language they do not read, and a question they cannot answer is worse
  than one never asked: it lands in the count and whatever they put is noise in the one label here
  allowed to be called truth. Restricting it makes the sample a random sample of those languages
  rather than of the corpus, and every figure from it has to say so.
- `--serve`: Serve the page from this machine instead of writing it, so every answer lands on disk
  as it is made. Opened as a file, the page keeps answers in the browser and only the Export button
  gets them out, which puts a thousand questions of somebody's judgement behind a button they have
  to remember.
- `--answers <ANSWERS>`: Where the answers are written while serving. `ingest-gold` reads this file.
  Unless named, `gold-answers.json`, or `gold-acceptable.json` with `--acceptable`, because the page
  writes the whole file on every answer and one kind must not replace the other.
- `--port <PORT>`: Port to serve on. 0 picks a free one. Default: `8731`.
- `--rejudge`: Ask again about every gold answer that differs from the first labeller's, with the
  labellers' answers, the person's own and the sheet's rule for each in view. A cold reading says
  how often the labels agree with a person; this says why they differ: a rule accepted on sight was
  a miss and the label stands, a rule rejected is the one that has to change. The new answer
  replaces the old at the next ingest.
- `--acceptable`: Ask, for every gold answer the library's reader gave another subject for, whether
  that subject would also do. The gold answer stays what it is; the judgement is kept beside it, and
  it is what lets a reader be scored as right or acceptable rather than only as matching the one
  subject the person would have picked.
- `--boundary <BOUNDARY>`: Ask only about disagreements on this boundary, written
  `difficulty/gameplay`, and repeated once per boundary. The blind sample is untouched: aiming that
  would make the accuracy figure a fact about the boundaries somebody chose. Four boundaries carry a
  third of every disagreement the two readings produce, and a sitting spent on those settles more of
  the sheet per question than one spread over all of them.

## `ingest-gold`

Merge adjudicated answers back, as the only labels in the set written by a person.

```text
steamgauge ingest-gold [OPTIONS] --from <FROM>
```

- `--from <FROM>`, required: The file the adjudication page exported.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.
- `-o, --out <OUT>`: Directory holding the captures, which is what says whether this build still
  cuts a claim where a label names one. An answers file can be older than that rule, and a gold
  label on a span the splitter has since joined to its neighbour is worse than a missing one,
  because it is scored as truth. Default: `data`.
- `--by <BY>`: Who adjudicated. Recorded per label, like every other labeller. Default: `a person`.

## `export-training`

Write every labelled claim, with its text, as JSONL for training.

The file holds review text and is not for publishing. What gets published is the label set: ids,
offsets and labels, which anyone can rehydrate with this tool.

```text
steamgauge export-training [OPTIONS]
```

- `--from <FROM>`: Where the claim reference sets live. Default: `reference/claims`.
- `-o, --out <OUT>`: Directory holding the captures, which is what says whether this build still
  cuts a claim where a label names one. A label whose span it no longer cuts is not a label of
  anything the reader will be handed, and training on it teaches a string that cannot come back. A
  game with no capture here holds nothing back. Default: `data`.
- `--to <TO>`: Where to write the JSONL. Default: `training/data/claims.jsonl`.

## `export-review-facts`

Write what the published dataset says about every labelled review, as JSONL.

Everything Steam shows about a review except its words and its author: language, dates, whether it
recommends, its votes, how it was bought, and the playtime shown on it. And the SHA-256 of the text
the labels were written against, so anyone who fetches the review from Steam can tell whether it is
still that text.

```text
steamgauge export-review-facts [OPTIONS] --to <TO>
```

- `--from <FROM>`: Where the claim reference sets live. Every review any label names is written.
  Default: `reference/claims`.
- `-o, --out <OUT>`: Directory holding the captures. Default: `data`.
- `--to <TO>`, required: Where to write the JSONL.

## `export-pool`

Write a draw of unlabelled claims, with their text, as JSONL for a teacher to read.

Reviews no reference set holds, from games the model trains on, in the numbers a labeller never
could. A student learns a teacher's answers on them beside the labelled claims. The file holds
review text and is not for publishing. Only games the model already learns from may be drawn: a
held-out game's reviews, even unlabelled, carry what the teacher knows of it into the student, and
the held-out figure would stop being about a game nobody trained on.

```text
steamgauge export-pool [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to draw from. Each must be a game the model trains on.
- `-o, --out <OUT>`: Directory holding the captures. Default: `data`.
- `--reviews <REVIEWS>`: Reviews to draw from each game. Default: `2000`.
- `--english <ENGLISH>`: Share of each game's draw written in English, as the reference draws set
  it. Default: `0.7`.
- `--seed <SEED>`: Changing this draws a different pool. Not the reference draws' seed, so the two
  overlap only by chance, and what overlaps is left out anyway. Default: `11`.
- `--reference <REFERENCE>`: Where the reference sets live, whose reviews are left out of the pool.
  Default: `reference/claims`.
- `--to <TO>`: Where to write the JSONL. Default: `training/data/pool.jsonl`.

## `store-facts`

Ask the store whether each game is played only in a VR headset, and keep the answer beside its
capture and its reference sets.

The one thing about a game the labeller and the reader are both told. `read` asks for a game that
has no answer yet; this fills in every game at once.

```text
steamgauge store-facts [OPTIONS] [APP_IDS]...
```

- `<APP_IDS>`: Steam app IDs. Every captured and every labelled game when none are named.
- `-o, --out <OUT>`: Directory holding the captures. Default: `data`.
- `--reference <REFERENCE>`: Where the claim reference sets live. Default: `reference/claims`.

## `read`

Read every claim in a corpus with the trained model.

```text
steamgauge read [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs whose most recent captures should be read.
- `-o, --out <OUT>`: Directory holding the capture. Default: `data`.
- `--reader <READER>`: Which size of reader, `small` or `standard`. Defaults to the largest this
  machine's card has room for, and to `small` where the build reaches no card.
- `--model <MODEL>`: Directory holding model.onnx, tokenizer.json and reader.json, in place of the
  size's own: models/\<size\> in the working tree if there is one, else the platform cache.
- `--batch-size <BATCH_SIZE>`: Claims per forward pass. The default reads about a tenth faster than
  half of it and asks the card for 2.5 GB; doubling it again buys nothing. Lower it on a small card.
  Default: `256`.
- `--language <LANGUAGE>`: Count only reviews written in this language. The capture stays whole
  either way.
- `--top-helpful <TOP_HELPFUL>`: How many of the most-helpful reviews count as the top of the pile.
  Default: `50`.
- `--depth <DEPTH>`: How closely to read. `deep` takes each review apart into the points it makes;
  `shallow` reads each review as one point, which is faster and understates anyone who wrote more
  than a sentence. Neither drops a review. One of `deep` or `shallow`. Default: `deep`.
- `--card-share <CARD_SHARE>`: The share of the card's time the read may take, for a card other work
  is using: 0.5 rests after each batch as long as the batch kept the card busy. The readings are the
  same; only how long they take changes. Default: `1`.

## `recount`

Count a corpus again from the readings already on disk, without the model.

Everything the page shows is added up when a game is read, so a change to the adding up (which words
stand out, how a month is cut) would otherwise cost every game the hours of a reading again. The
answers do not change; only the counts do. Refused where this build takes a review apart differently
from the build that read it, or the reader named is not the one that answered.

```text
steamgauge recount [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs whose readings should be counted again.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--reader <READER>`: Which size of reader answered, `small` or `standard`. Defaults to the one
  `read` would choose on this machine.
- `--model <MODEL>`: Directory holding the reader.json of the reader that answered, in place of the
  size's own.
- `--top-helpful <TOP_HELPFUL>`: How many of the most-helpful reviews count as the top of the pile.
  Default: `50`.

## `report`

Render a self-contained page from what the reading pass found.

```text
steamgauge report [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to report on. Several become one page, a section each.
- `-o, --out <OUT>`: Directory holding the captures and their classifications. Default: `data`.
- `--to <TO>`: Where to write the page. Default: `steamgauge-report.html`.
- `--examples <EXAMPLES>`: Claims quoted per subject, as the evidence behind its rate. Default: `8`.
- `--seed <SEED>`: Changing this quotes different claims. The same seed always quotes the same ones.
  Default: `1`.

## `export-data`

Write a read game's data as CSV and JSON, for a spreadsheet or your own analysis.

Makes a new folder at --to, the same files the window's "Export data" saves: subjects.csv and .json
(every subject's mention rate, praise, complaints and both, the share of points and its corrected
share, for everyone and for each kind of reviewer), months.csv and .json (reviews, the share
recommending and each subject, month by month), points.csv and .json (every point read, with its
subject, side, sureness and words, and the review it is in: its id, link, day, language, verdict,
time played, Steam Deck, early access, free copy and helpful votes), updates.csv and .json (the
game's updates on Steam), and a README.txt saying what every column holds, which reader read the
game, when, and how far its figures can be trusted. A figure the window does not show as a number is
left empty, and a column says why.

The CSV is UTF-8 with a byte order mark, comma-separated with a CRLF after each row, a field in
double quotes where it must be. A text that starts like a spreadsheet formula (=, +, -, @, a tab or
a carriage return) carries a ' before it in the CSV; the JSON holds it as written.

```text
steamgauge export-data [OPTIONS] --to <TO> <APP_ID>
```

- `<APP_ID>`: Steam app ID of a read game.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--to <TO>`, required: The folder to make. Refused where anything is there already.

## `update`

Update this copy to the newest release, as Update now does in the window.

The release's file for the way this copy was installed is downloaded and installed once its build
provenance verifies; nothing is installed otherwise. A setup program installs over this copy after
it exits, a package is installed by its manager after the system's password prompt, and an app or a
folder is replaced in place. The window is not opened afterwards.

```text
steamgauge update
```

## `mcp`

Steer the app from a Model Context Protocol client, which starts this command and speaks to it on
standard input and output.

Every tool is something the window can do, and the person sees it done there. The open app answers,
over a local socket only this user can reach; where it is not running it is opened with its window
hidden, and closes once the last client has gone and its work is done. `connect` adds this command
to a client's configuration.

```text
steamgauge mcp
```

## `clients`

The MCP clients this command can add the app to, whether each is on this computer, and whether it
starts this copy.

```text
steamgauge clients
```

## `connect`

Add this copy to an MCP client's own configuration, so the client starts it.

Only the app's own entry is written: every other server, every comment and the file's layout stay as
they were, and a file that does not parse is left alone. An entry that starts another copy is
pointed at this one. Any other client takes the command `<this program> mcp` in its own settings.

```text
steamgauge connect <CLIENT>
```

- `<CLIENT>`: The client, as `clients` lists it.

## `disconnect`

Take the app out of an MCP client's configuration, leaving everything else in it.

```text
steamgauge disconnect <CLIENT>
```

- `<CLIENT>`: The client, as `clients` lists it.

## `brief`

Write the category sheet labellers work from, generated from the taxonomy.

Separate from `sample-claims` because a boundary rule can change without anything needing to be
drawn again, and regenerating the sheet should never mean redrawing a sample.

```text
steamgauge brief [OPTIONS]
```

- `--to <TO>`: Where to write it. Both sheets are written when this is a directory. Default:
  `reference`.

<!-- steamgauge-site: end -->
