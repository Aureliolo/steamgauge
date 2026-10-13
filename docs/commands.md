# Command line

Started with no arguments, `steamgauge` opens the window; given a command, it runs it and exits.
On Windows it prints only into a pipe, so in PowerShell add one: `steamgauge --help | Out-Host`.
Scoop and Homebrew put it on the `PATH`; [Install](install.md) has the rest.

Every command that reads or writes a game takes `--out`, the folder that holds its downloads,
`data` by default.

<!-- steamgauge-site: commands -->

## Downloading and reading

### `crawl`

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

### `sweep`

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

### `read`

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

### `recount`

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

### `report`

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

### `export-data`

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

## The app and AI assistants

### `update`

Update this copy to the newest release, as Update now does in the window.

The release's file for the way this copy was installed is downloaded and installed once its build
provenance verifies; nothing is installed otherwise. A setup program installs over this copy after
it exits, a package is installed by its manager after the system's password prompt, and an app or a
folder is replaced in place. The window is not opened afterwards.

```text
steamgauge update
```

### `mcp`

Steer the app from a Model Context Protocol client, which starts this command and speaks to it on
standard input and output.

Every tool is something the window can do, and the person sees it done there. The open app answers,
over a local socket only this user can reach; where it is not running it is opened with its window
hidden, and closes once the last client has gone and its work is done. `connect` adds this command
to a client's configuration.

```text
steamgauge mcp
```

### `clients`

The MCP clients this command can add the app to, whether each is on this computer, and whether it
starts this copy.

```text
steamgauge clients
```

### `connect`

Add this copy to an MCP client's own configuration, so the client starts it.

Only the app's own entry is written: every other server, every comment and the file's layout stay as
they were, and a file that does not parse is left alone. An entry that starts another copy is
pointed at this one. Any other client takes the command `<this program> mcp` in its own settings.

```text
steamgauge connect <CLIENT>
```

- `<CLIENT>`: The client, as `clients` lists it.

### `disconnect`

Take the app out of an MCP client's configuration, leaving everything else in it.

```text
steamgauge disconnect <CLIENT>
```

- `<CLIENT>`: The client, as `clients` lists it.

## Measuring and training the reader

- `claims <APP_ID>`: Split every review into the separate points it makes.
- `embed <APP_ID>`: Embed the captured reviews locally, so nothing is sent anywhere.
- `store-facts [APP_IDS]...`: Ask the store whether each game is played only in a VR headset, and
  keep the answer beside its capture and its reference sets.
- `sample-claims <APP_IDS>...`: Draw reviews at random, split them into claims, and write batches to
  be labelled.
- `declined <APP_IDS>...`: Draw the claims the reader would not answer, for a labeller to teach it
  on.
- `rare-claims <APP_IDS>...`: Draw claims that look like the subjects the labelled set has fewest
  labels for.
- `distinct <APP_ID>`: Draw the reviews of a corpus that are least like each other, for finding what
  this game's players talk about that the fixed taxonomy has no row for.
- `ingest-induced --from <FROM> <APP_ID>`: Check a returned list of induced subjects against the
  handout it came from, and keep the ones that survive.
- `ingest-claims --from <FROM> --by <BY> <APP_ID>`: Merge returned claim labels into a reference
  set.
- `second-opinion [APP_IDS]...`: Draw the share of a labelled set that a second labeller should
  read.
- `compare-labels [APP_IDS]...`: Compare two labellings of the same claims, field by field.
- `revisit [APP_IDS]...`: Draw the claims of a labelled set that a revision of the sheet puts back
  in question.
- `ingest-revisit --from <FROM> --by <BY> <APP_ID>`: Merge revisited labels back, replacing only the
  claims that were asked about.
- `measure-claims <APP_IDS>...`: Score stored readings against a claim reference set.
- `ceiling [APP_IDS]...`: Score the model against a label two labellers both reached, and say how
  often they reached one at all.
- `gold`: Write the page a person adjudicates claims on, from the held-out games.
- `ingest-gold --from <FROM>`: Merge adjudicated answers back, as the only labels in the set written
  by a person.
- `export-training`: Write every labelled claim, with its text, as JSONL for training.
- `export-review-facts --to <TO>`: Write what the published dataset says about every labelled
  review, as JSONL.
- `export-pool <APP_IDS>...`: Write a draw of unlabelled claims, with their text, as JSONL for a
  teacher to read.
- `brief`: Write the category sheet labellers work from, generated from the taxonomy.

Each takes `--help` for its options. [Accuracy](accuracy.md#how-the-labels-are-made) says where each
fits.

<!-- steamgauge-site: end -->
