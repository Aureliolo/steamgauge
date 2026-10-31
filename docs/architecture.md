# Architecture

How the parts of SteamGauge fit and the rules they keep, for anyone changing it.

## The parts

| Part | What it is |
|---|---|
| `crates/steamgauge-app` | The one binary, `steamgauge`: the Tauri window when opened with no arguments, the command line when given any, and the MCP server. |
| `crates/steamgauge-core` | Everything that knows nothing about a window: crawling, splitting, reading, counting, the report and the updater. |
| `crates/steamgauge-site` | The documentation site, written from the program's own tables. |
| `training/` | Python that trains, measures and exports the reader. [`training/README.md`](../training/README.md) gives the order. |
| `reference/` | The labelled claims, the category sheet, the benchmark keys and the reader sizes table. |
| `tools/` | The browser checks of the window, the report, the gold page and the site, and the release scripts. |

## A claim

- **Rules split a review into claims**, in `claims.rs`. No model splits.
- **A claim is the bytes it covers.** A label, a reading row, a draw and a person's answer each
  carry the span (`claims::Span`) and nothing else.
- **No version stamp.** Whether this build still cuts a claim at a span is answered by splitting
  the review again.
- `claims::words_at` is the one way a stored span becomes text, and `claims::is_not_a_claim` the
  one test for an empty piece. Every draw, ingest and export uses both.
- **Labelled text is kept** in each game's `reviews.jsonl`, since a review can be edited. A later
  draw that meets an edited copy is refused.
- **After a splitter change**, labels at spans the build no longer cuts are left out of
  measurement and training, and counted. The library is then read again.

## Downloading

- Reviews come from Steam's `IUserReviewsService/GetAppReviews`, with the census arguments in
  `input_json` and Valve's default filters overridden. No key.
- A crawl is planned in date windows, and each window is walked again while it falls short of
  Valve's count. A capture is Parquet shards written once and never changed.
- A **sweep** walks in last-edit order back to the previous watermark into `sweep-*.parquet`.
  `newest.json` says which copy of an edited review counts.
- Every file is written beside its place and renamed when whole.
- Developer posts come from `ISteamNews/GetNewsForApp`, titles and dates only, in
  `announcements.json`. `updates::is_update` decides which are updates.
- Whether the store lists a game as VR-only is kept in `game.json`, and the labeller and reader
  are both told.
- The library is under the local app data folder. `STEAMGAUGE_DATA` moves it.
- **Every pass reads the current format only.** `current_format` converts what an earlier release
  kept, once, and removes what it cannot express rather than guess.

## Reading

- The reader is a half-precision ONNX graph run through ONNX Runtime: DirectML on Windows, Core ML
  on macOS, the processor otherwise.
- It reads each claim inside a window of its review centred on the claim, as a labeller does. A
  VR-only game's window opens with "Played in a VR headset."
- Claims are sorted by length in windows of 16,384 and read in batches of 256. A fresh reading at
  a fixed batch size is the same to the bit, and records the batch size.
- **Reading again** keeps the answers of every review whose text and spans are unchanged, by a
  fingerprint of its text. `read --afresh` and a new reader read every claim.
- On a card the search encoder and reranker run only at fixed shapes, at 64, 128, 256 or 512
  tokens. DirectML prepares itself again for every new shape.
- An answer is cached by review text, claim position and language.
- **Abstention**: a claim is answered only when its top probability clears its subject's line and
  its language's. Both are fitted out of fold to keep three in four right.
- A language with fewer than 100 labelled claims has no line and declines.
- `steamgauge recount` replays stored readings through the counting without the model. It refuses
  a reading this build would cut differently.

## Counting

- The headline is the **mention rate**: the share of reviews raising a subject. Every other
  percentage names its denominator, and every rate carries a 95% interval.
- The corrected share uses the reader's measured errors on a game's labels, from 40 labels up.
- **Words that stand out** are compared with log-odds within each review language and pooled by
  inverse variance. A subject's own name is left out.
- Chinese is cut by `jieba-rs`, Korean by stripping particles and endings, and Japanese into
  character pairs.
- **Themes** group a subject's claims on one side by meaning, over the search vectors. A theme
  starts from the claim with the most others at cosine 0.75 or more.
- Two themes whose centres reach 0.9 merge. Past 3,000 distinct claims, a subject is grouped on a
  hashed sample.
- A theme needs five reviews, and a subject keeps eight a side. `themes.json` holds them for one
  reading and encoder.
- **The paragraph** is written from the themes by Qwen3-4B-Instruct-2507 with int4 weights. It is
  decoded greedily in `writer.rs` on the processor, because DirectML runs it slower and wrongly.
- A sentence with a number or quote the themes do not hold is dropped, and fewer than two left
  means no paragraph. `paragraph.json` holds it for one set of themes and one writer.

What counts as a change:

| Comparison | Floor | Bar |
|---|---|---|
| What moved lately: the last three months against the twelve before | 100 and 300 reviews | 3 standard errors and 2 points |
| Either side of an update, 28 days each | 100 reviews a side | 3 standard errors and 2 points |
| One kind of reviewer against the rest | 100 reviews | 4 standard errors and 2 points |
| New reviews since the last look against the year before | 100 and 300 reviews | 4 standard errors and 2 points |

The bar rises with how many shares a comparison tests, so chance clears it on fewer than one game
in thirty.

- **A group with each game the same** (`group.rs`) takes the mean of the games' rates. Praise and
  complaint count only games where at least 30 reviews raise the subject.
- **A group with every review the same** adds the counts and gives Wilson's interval. A game not
  read yet enters no figure and is named as left out.
- Where fewer than 30 reviews raise a subject, Compare and a group's page hide its praise and
  complaint. The person can turn **thin splits** on.

## The window

- It opens on a **cockpit** of what moved since the last look, which `last-seen.json` in the
  library keeps.
- **Jobs run on three lanes**: downloads and updates, reads and preparations, and files. Each lane
  runs one job at a time, and no job starts on a game another job is using or waiting for.
- A job is named as the store names its game, never by its number.
- The reader, the search encoder and the reranker are let go after two minutes unused. The card
  holds the reader or the encoder, never both.
- The app's own jobs (updates, Steam checks) pause while a job the person started runs.
- Updates run only while the app is open, minimised included, from a loop in the core. Steam is
  asked every hour, six hours, day or week, as Settings chooses.
- **The window's content policy keeps every network address out of the page.** The core fetches
  what the window shows and opens save dialogues itself.
- **Export** writes CSV (UTF-8 with a byte order mark, formula injection escaped) and JSON with a
  `README.txt`, by the page's own floors.
- Every page is held to WCAG 2.2 AA in light and dark and at its narrowest width.
- Every page fills the window beside the sidebar, with no fixed width. Wide content goes into
  columns, and only a paragraph keeps a reading measure.

## Assistants

- **The open app is the MCP server.** Every tool is a window command under the same name, and a
  test fails on a command without a tool.
- **`steamgauge mcp`** lists the tools itself and hands each call to the open app. Its socket is a
  randomly named pipe on Windows, or sits in a mode-700 folder elsewhere.
- **The prompts** (`prompts.rs`) are text alone, so they answer whether the app is open or not. A
  test fails on a prompt that names a missing tool.
- **Nothing starts the app but the person**, and closing the window closes it. A call the app
  closed during is reported, never sent again.
- **An update never closes an assistant's `steamgauge mcp`**, which keeps its version's tools until
  restarted. On Windows the setup moves files still in use aside into `replaced`.
- **HTTP** answers on 127.0.0.1 behind a 256-bit bearer token. It refuses any `Origin` header and
  any `Host` but `127.0.0.1` or `localhost`.
- **Clients are rows of one table** (`clients.rs`), each from that client's own documentation of
  its user-wide file.
- **Writing keeps the rest of the file.** JSON with comments goes through `jsonc-parser`, TOML
  through `toml_edit`, and YAML through `yaml-edit` with the entry on one line in flow style.
- **Every write is checked**: a strict parser reads both texts, and the new must be the old plus
  the entry. A file that does not parse is refused.
- Claude Code rewrites its own file while it runs, so it is changed through `claude mcp add
  --scope user` and read back to confirm.

## Updates and releases

- **No signing key exists**, and every signature is Sigstore's, keyless and tied to the release
  workflow. [SECURITY.md](../SECURITY.md#updates-from-the-window) lists what Update now checks.
- The app reads GitHub's `releases/latest` redirect without following it. `newer-version.json`
  keeps when it last asked and what GitHub named.
- What each release changes is the "In what you download" section of its notes, fetched only
  when About lists it. `opened-version.json` keeps the last two versions the app opened as.
- The window opens SteamGauge's own pages by their exact addresses only. Every other address it
  opens is a Steam page or a release page.
- **An update writes `verified-install.json` before installing.** About calls a build checked
  only where that record names the running version.
- About lists dependencies from the `THIRD-PARTY-NOTICES.txt` beside the program and opens that
  file in the system's viewer.
- **Below 1.0**, a release that renames something people call raises the minor version, and
  anything else the patch. Pre-release versions are refused.
- [The release runbook](../.github/release-process.md) covers how a release is cut and built.

## Labels and measurement

- Labels are written by Claude models from one category sheet, whose hash is recorded on every
  label. A second labeller reads part of every set blind, and every held-out game in full.
- Games are split into training, validation and held-out by a hash of their app ID. The held-out
  games choose nothing, and every public figure is theirs.
- Teaching draws (`declined`, `rare-claims`, `retrieved`, `multilingual`) never count in a
  prevalence figure, and are refused for any game outside training.
- [Training](../training/README.md#how-a-change-is-judged) says when a change is an improvement.

## Names

Nothing is named by an incremented number. A training run is named for its encoder and label
count, and a label set by its fingerprint. The sheet is named by the hash of its text.
