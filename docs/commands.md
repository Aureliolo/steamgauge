# Command line

Started with no arguments, `steamgauge` opens the window. Given a command, it runs it and exits.
On Windows it prints only into a pipe, so in PowerShell add one: `steamgauge --help | Out-Host`.

<!-- steamgauge-site: commands -->

## Downloading and reading

### `crawl`

Download every review Steam serves for a game into a Parquet capture, with the titles and dates of
its developer's posts.

```text
steamgauge crawl [OPTIONS] <APP_ID>
```

- `<APP_ID>`: Steam app ID, as it appears in the store URL.
- `-o, --out <OUT>`: Directory to write captures and crawl state into. Default: `data`.
- `--pace-ms <PACE_MS>`: Milliseconds between requests. Default: `250`.
- `--concurrency <CONCURRENCY>`: Date ranges to download at once. Requests keep the same pace.
  Default: `4`.
- `--shard-target <SHARD_TARGET>`: Reviews in one date range before it is split further. Default:
  `40000`.
- `--restart`: Plan a fresh crawl instead of continuing an unfinished one.

### `sweep`

Fetch every review written or edited since a capture was downloaded or last updated.

An edited review counts in its newer form. Run `read` afterwards so the figures include the new
reviews. The developer's posts are fetched again too.

```text
steamgauge sweep [OPTIONS] <APP_ID>
```

- `<APP_ID>`: Steam app ID, as it appears in the store URL.
- `-o, --out <OUT>`: Directory holding the captures. Default: `data`.
- `--pace-ms <PACE_MS>`: Milliseconds between requests. Default: `250`.

### `read`

Read every claim in each game's newest capture with the trained model.

```text
steamgauge read [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to read.
- `-o, --out <OUT>`: Directory holding the captures. Default: `data`.
- `--reader <READER>`: Reader size, `small` or `standard`. Defaults to the largest the graphics card
  has room for, or `small` without a card.
- `--model <MODEL>`: Directory holding `model.onnx`, `tokenizer.json` and `reader.json`, used in
  place of the size's own.
- `--batch-size <BATCH_SIZE>`: Claims per batch. Lower it on a card with less than 2.5 GB of memory
  free. Default: `256`.
- `--language <LANGUAGE>`: Count only reviews written in this language.
- `--top-helpful <TOP_HELPFUL>`: How many of the most-helpful reviews count as the top of the pile.
  Default: `50`.
- `--depth <DEPTH>`: How closely to read. `deep` splits each review into the points it makes.
  `shallow` is faster and reads each review as one point. One of `deep` or `shallow`. Default:
  `deep`.
- `--card-share <CARD_SHARE>`: The share of the graphics card's time the read may take, above 0 and
  at most 1. Lower it when other work needs the card. The results stay the same. Default: `1`.
- `--afresh`: Read every claim again. Without it, a game the same reader read before has only its
  new and edited reviews read.

### `recount`

Count games again from their saved readings, without the model.

Use it when only the counting has changed, since it skips the hours a read takes. Refused where this
build splits reviews differently from the build that read them, or another reader read them.

```text
steamgauge recount [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to count again.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--reader <READER>`: The reader size that read them, `small` or `standard`. Defaults to the one
  `read` would choose on this machine.
- `--model <MODEL>`: Directory holding the `reader.json` of the reader that read them, in place of
  the size's own.
- `--top-helpful <TOP_HELPFUL>`: How many of the most-helpful reviews count as the top of the pile.
  Default: `50`.

### `report`

Write a self-contained HTML page of what the reading found.

```text
steamgauge report [OPTIONS] <APP_IDS>...
```

- `<APP_IDS>`: Steam app IDs to report on. Several become one page, a section each.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--to <TO>`: Where to write the page. Default: `steamgauge-report.html`.
- `--examples <EXAMPLES>`: Claims quoted per subject, as the evidence behind its rate. Default: `8`.
- `--seed <SEED>`: Changing this quotes different claims. The same seed always quotes the same ones.
  Default: `1`.

### `export-data`

Write a read game's data as CSV and JSON, for a spreadsheet or your own analysis.

The folder holds the files the window's "Export data" saves: subjects, months, points and updates,
each as CSV and JSON. Its `README.txt` says what every column holds.

The CSV is UTF-8 with a byte order mark. A text that starts like a spreadsheet formula gets a ' in
front of it in the CSV only.

```text
steamgauge export-data [OPTIONS] --to <TO> <APP_ID>
```

- `<APP_ID>`: Steam app ID of a read game.
- `-o, --out <OUT>`: Directory holding the captures and their readings. Default: `data`.
- `--to <TO>`, required: The folder to make. Refused where anything is there already.

## The app and AI assistants

### `update`

Update this copy to the newest release, as Update now does in the window.

Only a release whose build provenance verifies is installed. A package install asks for the system
password. The window does not open afterwards.

```text
steamgauge update
```

### `mcp`

Let a Model Context Protocol client steer the app, over standard input and output.

The client starts this command. The tools answer only while the app's window is open. `connect` adds
this command to a client's configuration.

```text
steamgauge mcp
```

### `clients`

List the MCP clients `connect` knows, whether each is installed, and whether it starts this copy.

```text
steamgauge clients
```

### `connect`

Add this copy to an MCP client's configuration, so the client starts it.

Only the app's own entry changes, and a file that does not parse is left alone. Any other client
takes the command `<this program> mcp` in its own settings.

```text
steamgauge connect <CLIENT>
```

- `<CLIENT>`: The client, as `clients` lists it.

### `disconnect`

Remove the app's entry from an MCP client's configuration.

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
- `distinct <APP_ID>`: Draw the reviews of a game least like each other, to find subjects the fixed
  taxonomy lacks.
- `ingest-induced --from <FROM> <APP_ID>`: Check a returned list of induced subjects against the
  handout it came from, and keep the ones that survive.
- `ingest-claims --from <FROM> --by <BY> <APP_ID>`: Merge returned claim labels into a reference
  set.
- `second-opinion [APP_IDS]...`: Draw the share of a labelled set that a second labeller should
  read.
- `compare-labels [APP_IDS]...`: Compare two labellings of the same claims, field by field.
- `revisit [APP_IDS]...`: Draw the claims of a labelled set that a revision of the sheet puts back
  in question.
- `ingest-revisit --from <FROM> --by <BY> <APP_ID>`: Merge revisited labels back. Only the claims
  that were asked about are replaced.
- `measure-claims <APP_IDS>...`: Score stored readings against a claim reference set.
- `ceiling [APP_IDS]...`: Score the model against a label two labellers both reached, and say how
  often they reached one at all.
- `gold --for <PERSON>`: Write the page a person adjudicates claims on, from the held-out games.
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
