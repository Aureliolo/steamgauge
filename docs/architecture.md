# Architecture

How SteamGauge is built, for anyone changing it. [How it works](how-it-works.md) says what it does
and [Accuracy](accuracy.md) how well; this page says how the parts fit and the rules they keep.

## The parts

| Part | What it is |
|---|---|
| `crates/steamgauge-app` | The one binary, `steamgauge`: the Tauri window when opened with no arguments, the command line when given any, and the MCP server. |
| `crates/steamgauge-core` | Everything that knows nothing about a window: crawling, splitting, reading, counting, the report, updating itself. |
| `crates/steamgauge-site` | The documentation site, written from the program's own tables. |
| `training/` | Python that trains, measures and exports the reader. [`training/README.md`](../training/README.md) gives the order. |
| `reference/` | The labelled claims, the category sheet, the benchmark keys and the reader sizes table. |
| `tools/` | The browser checks of the window, the report, the gold page and the site, and the release scripts. |

## A claim

- A review is split into **claims**, the separate points it makes, by rules in `claims.rs`. No
  model splits; the rules know sentence ends, lists, headings, ballot templates and markup.
- **A claim is the bytes it covers.** A label, a reading row, a draw and a person's answer each
  carry the span (`claims::Span`) and nothing else. Whether this build still cuts a claim at a
  span is answered by splitting the review again, never by a version stamp.
- `claims::words_at` is the one way a stored span becomes text, and `claims::is_not_a_claim` the
  one test for a piece with nothing in it (no word, an unticked ballot option). The gold draw, the
  gold ingest, the revisit draw and the training export all use both.
- A splitter change moves some spans. Labels at a span the build no longer cuts are left out of
  measurement and training, and counted; the library is read again.

## Downloading

- Reviews come from Steam's web API, `IUserReviewsService/GetAppReviews`, with the census
  arguments in `input_json` and Valve's default filters overridden. No key.
- A crawl is planned in date windows and each window is checked against Valve's own count, then
  walked again when short. A capture is Parquet shards written once and never changed.
- A **sweep** walks in last-edit order back to the previous watermark and writes `sweep-*.parquet`
  beside the shards; `newest.json` says which copy of an edited review counts.
- Each file is written beside its place and renamed when whole, so a stopped job leaves nothing
  half written.
- A game's developer posts come from `ISteamNews/GetNewsForApp`, titles and dates only, kept in
  `announcements.json`. `updates::is_update` decides which are updates.
- Whether the store lists a game as VR-only is kept in `game.json` and told to the labeller and
  the reader alike.
- The library is under the local app data folder (`%LOCALAPPDATA%\com.aureliolo.steamgauge\data`
  on Windows); `STEAMGAUGE_DATA` moves it.

## Reading

- The reader is a multilingual encoder fine-tuned on labelled claims, run as a half-precision ONNX
  graph through ONNX Runtime: DirectML on Windows, Core ML on macOS, the processor otherwise.
- It reads each claim inside a window of its review centred on the claim, because a labeller
  reads it that way and "same here" means nothing alone. A VR-only game's window opens with
  "Played in a VR headset."
- Claims are sorted by length in windows of 16,384 and read in batches of 256; the next batch is
  tokenised while the card runs this one. A reading at a fixed batch size is reproducible to the
  bit, and records the batch size.
- On a card the search encoder and reranker run in a few shapes only: a fixed number of texts, a
  batch short of them filled out with copies of its first, at 64, 128, 256 or 512 tokens. `DirectML`
  prepares itself afresh for each shape it has not met, which cost more than the padding does.
- An answer is cached by review text, claim position and language, so a review posted many times
  is read once.
- **Abstention.** Confidence is the highest class probability. A claim is answered only when it
  clears both its subject's line and its language's, each drawn out of fold so the reader keeps
  three in four right. A language with fewer than 100 labelled claims has no line and declines.
- `steamgauge recount` replays stored readings through the counting without the model, and
  refuses a reading this build would cut differently.

## Counting

- The headline is the **mention rate**: the share of reviews raising a subject. Every other
  percentage names its denominator, and every rate carries a 95% interval.
- The corrected share uses the reader's measured errors on a game's labels, and only from 40
  labels up.
- **Words that stand out** are counted per review language, compared with log-odds within each
  language and pooled by inverse variance. Chinese is cut by a dictionary (`jieba-rs`), Korean by
  stripping particles and endings, Japanese into character pairs. Two forms of one word count as
  one, and a subject's own name in any language is left out.
- **Themes** are the particular things said within a subject: its claims on one side, grouped by
  meaning over the vectors that prepare a game for search by meaning, so finding them runs no
  model and ends that job. A theme starts from the claim with the most others at cosine 0.75 or
  more and takes them; two whose centres reach 0.9 are one. Past 3,000 distinct claims a subject
  is grouped on a sample taken by hash, and every claim is then counted under the nearest theme it
  is that alike. A theme needs five reviews, and a subject keeps eight a side. Under two subjects
  it is one point where half the smaller's claims are in both or the centres reach 0.9, and the
  page lists it once with the other subjects named. Verdicts and off-topic remarks have none.
  `themes.json` beside the reading holds them, and stands only for that reading and encoder.
- The **paragraph** at the top of a game is written from its themes by Qwen3-4B-Instruct-2507,
  exported by `training/export_writer.py` with int4 weights rounded to nearest, and decoded in
  `writer.rs` taking the likeliest token every time, so the same themes give the same
  paragraph. It runs on the processor, about half a minute a game on a desktop one: the graph
  is built for the processor's int4 kernels, and `DirectML` runs it slower and wrongly. It is
  told the eight most widely made points of each side, with their counts, subjects and quotes. A
  sentence giving a number they do not hold, or quoting words none of them holds, is left out,
  and a paragraph with fewer than two sentences left is none. The writer is a download of its
  own, fetched and run only once somebody asks; after that, preparing a game writes its
  paragraph. `paragraph.json` beside the reading stands only for the themes and the writer it
  was written from.
- What counts as a change:

| Comparison | Floor | Bar |
|---|---|---|
| What moved lately: the last three months against the twelve before | 100 and 300 reviews | 3 standard errors and 2 points |
| Either side of an update, 28 days each | 100 reviews a side | 3 standard errors and 2 points |
| One kind of reviewer against the rest | 100 reviews | 4 standard errors and 2 points |
| New reviews since the last look against the year before | 100 and 300 reviews | 4 standard errors and 2 points |

The bar rises with how many shares a comparison tests, so that chance clears it on fewer than one
game in thirty.

- **A group's figures** (`group.rs`) count its read games two ways, and the window switches
  between them. With **each game the same**, a subject's rate is the mean of the games' rates, its
  interval comes from each game's sampling error (for these games, not games like them), and its
  praise and complaint are the mean over games where at least 30 reviews raise it. With **every
  review the same**, the counts are added and divided once, with Wilson's interval. Each subject
  also says how many games raise it in at least one review in a hundred and which game raises it
  most. A game not read yet enters no figure and is named as left out.
- Where fewer than 30 reviews raise a subject, Compare and a group's page show its rate but
  withhold how it splits between praise and complaint, unless the person turns on the switch
  for thin splits; one switch serves both pages, and a split shown that way says what it rests on.

## The window

- It opens on a **cockpit**: what moved since the last look, running work, the library, the
  machine and its models. What was last seen is kept in `last-seen.json` in the library.
- All work is **jobs on three lanes**: downloads and updates, reads and preparations (the card),
  and files. Each lane runs one job at a time, and no job starts on a game another job is using
  or one queued before it is waiting to use, so a read queued with its game's download follows
  it. A job is named as the store names its game, asked for when it is queued where the game has
  no name on disk yet; never by the game's number.
- The reader, the search encoder and the reranker stay loaded between the jobs and searches that
  use them, and are let go after two minutes unused. A read lets go of the encoder first and a
  preparation of the reader, so the card holds no more than one of the two.
- The app's own jobs (keeping games up to date, checking Steam) never run beside the person's: a
  job the person starts pauses them, and they start again afterwards.
- Keeping up to date runs while the app is open, minimised included, and never once it is
  closed: the schedule is a loop in the core's process, not in the window. Steam is asked at most
  every hour, six hours, day or week, as Settings chooses (six hours unless changed); a game is
  updated once Steam holds a hundredth more reviews than the library, or on any new review where
  Settings asks for that.
- The window's content policy keeps every network address out of the page. The core fetches what
  the window shows, such as store pictures, and opens save dialogues itself, so the window has no
  permission to reach the network or choose a path.
- A notification, off by default, goes through `notify-rust`, which returns the click.
- **Export** writes a folder of CSV (UTF-8 with a byte order mark, formula injection escaped) and
  JSON with a `README.txt`, by the page's own floors.
- Every page is held to WCAG 2.2 AA in light and dark and at its narrowest width.
- Every page takes the whole window beside the sidebar, with no fixed page width. What would
  run on down a wide screen goes into columns instead: the cockpit's three stacks, panels that
  read in any order (settings, storage, about), a game's themes beside its table, and the points
  behind a figure. Only a paragraph keeps a reading measure.

## Assistants

- The open app is the MCP server, because it owns the library and the work board. Every tool is a
  window command under the same name, and a test fails on a command without a tool.
- `steamgauge mcp` is what a client starts. It answers the client itself, listing the tools
  whether the app is open or not, and hands each call to the open app over its local socket: a
  named pipe with a random name on Windows, a socket in a folder of mode 700 elsewhere, the name
  written only into the user's own data folder.
- The prompts (`prompts.rs`) are requests written out in the tools' own words, text alone, so
  `steamgauge mcp` and the open app both answer them, open or not. A test fails on a prompt that
  names a tool the server lacks.
- **The app answers only while its window is open.** Nothing starts it but the person, and closing
  the window closes it, assistants connected or not. While it is closed every call says so and
  tells the assistant to ask the person; a call the app closed during is reported, never sent
  again.
- **An update closes the window, never an assistant's `steamgauge mcp`.** That copy runs on from
  the program it started from, offering that version's tools, and hands its calls to the new
  window, until the assistant starts it again. On Windows the setup program closes the window
  alone, found by its own window class, and moves the program and `DirectML.dll` a copy still runs
  from aside into a folder of their own under `replaced`, as Windows allows for a running program
  where it refuses a write; the next install and the uninstaller remove what nothing holds.
- The window's **AI assistants** page holds everything about them: the clients on this computer
  with Add and Remove, the line to paste to any other, the command it starts, and the HTTP server.
  It saves only its own two settings, over the others read afresh.
- Over HTTP the same tools answer on 127.0.0.1 when the AI assistants page turns it on, behind a
  256-bit bearer token, with any `Origin` header refused and only `127.0.0.1` or `localhost`
  accepted as `Host`.
- The clients the app adds itself to are rows of one table (`clients.rs`), each from that client's
  own documentation of its user-wide file: which file, the keys down to its servers, whether they
  are a map or a list of named entries, and the entry's shape. Writing keeps everything else in
  the file: JSON with comments through `jsonc-parser`'s syntax tree, TOML through `toml_edit`, YAML
  through `yaml-edit` with the entry written on one line, in flow style, so no indentation can
  misplace it.
- **Every write is checked before it lands.** A strict parser (`serde-saphyr` for YAML) reads the
  old and the new text, and the new has to be the old plus the entry and nothing else; otherwise
  nothing is written. A file that does not parse is refused, and the file is written beside itself
  and renamed over.
- A client that rewrites its own file while it runs, Claude Code, is added and removed through its
  own command (`claude mcp add --scope user`), and its file is read back to confirm.

## Updates and releases

- **No signing key exists.** Every signature is Sigstore's, keyless and tied to the release
  workflow.
- The app asks GitHub's `releases/latest` redirect on every opening and, on the ten-minute loop,
  once six hours have passed, without following it, and again whenever the person chooses Check
  for updates on the About page or Update now, so what is installed is the newest release at that
  moment. `newer-version.json` keeps when it last asked, when GitHub last answered and what it
  named.
- What each release changes is the "In what you download" section of its notes, read from
  GitHub's releases API only when About lists it (on its own where the app asks by itself,
  otherwise on the person's word), and kept while the app runs. `opened-version.json` keeps the
  version the app last opened as and the one before it, so a copy that opens as a newer version
  lists what changed since, and the sidebar points at that list until About is shown.
- The window opens SteamGauge's documentation, repository, issue forms and security policy by
  their exact addresses alone; every other address it may open is a Steam page or a release page.
- An update that has verified a release file writes `verified-install.json` (the version, the
  file and its SHA-256) before installing it, and About says a build was checked only where that
  record names the running version. A copy installed by hand is said not to be checked by
  SteamGauge, with the security policy that says how to check a download.
- About names what SteamGauge is built on from the table of contents of the
  `THIRD-PARTY-NOTICES.txt` the release puts beside the program (in the bundle's Resources on
  macOS, `/usr/share/doc/steamgauge` from a package), and opens that file in the system's viewer
  for text; the window itself opens no file.
- Update now downloads the release's file for the way this copy was installed and installs it
  only after its SLSA provenance verifies (`sigstore-verify`):
  the certificate names `release-build.yml` at the tag being installed, this repository's and
  owner's numeric ids and a GitHub-hosted runner; the digest is the one taken while downloading,
  and the version is strictly newer.
- A release is cut by the **prepare release** workflow, whose version pull request merges itself
  through the packaging App. The tag starts `release.yml`, which calls the reusable
  `release-build.yml` so the provenance names it.
- Each system gets installers and a portable archive: an NSIS setup program on Windows, a disk
  image on macOS, a `.deb` and an `.rpm` (nFPM) on Linux. Every installer is installed and run on
  its own system before anything is signed. An SPDX SBOM is attested against each archive.
- winget, Scoop and Homebrew are written from the release's signed checksums and installed by
  their own managers before they land.
- Below 1.0, a release that renames something people call raises the minor version; anything else
  raises the patch. Pre-release versions are refused.
- Windows builds use Control Flow Guard, shadow-stack compatibility and Spectre mitigations for
  compiled C, and BinSkim reads every program; its accepted findings carry their reasons in
  `binaries.yml`. TLS runs on ring.

## Checks

| Check | What it holds |
|---|---|
| Coverage | The core's own tests cover at least 97% of its lines; the floor only rises. |
| Mutation testing | A surviving mutant on a pull request's changed lines fails it; the weekly run lists the rest in one issue. |
| Fuzzing | The splitter, `words_at`, the reader's window and the term cut are fuzzed on every pull request, with their invariants asserted. |
| Browser checks | The window, the report, the gold page and the site, each against axe-core's WCAG 2.2 A and AA rules. |
| Prose | Vale checks the words people read for inflated and filler phrasing. |
| CodeQL | Every pull request, including those holding no code. |
| Release dry run | `release.yml` run on a branch builds and installs everything without publishing. |

A pull request holding only a version bump or package manifests (`tools/release/scope.mjs`) skips
the build, install and scan jobs, and the release gate takes the checks of the commit that last
held code.

## Labels and measurement

- Labels are written by Claude models from one category sheet, whose text is hashed and recorded
  on every label. A second labeller reads part of every set blind, and every held-out game in
  full.
- Games are split into training, validation and held-out by a hash of their app ID. The held-out
  games choose nothing and are read once; every public figure is theirs.
- Teaching draws (`declined`, `rare-claims`, `retrieved`, `multilingual`) are kept apart from the
  random draws, never counted in a prevalence figure, and refused for any game that is not
  training.
- A change to training is an improvement only when it clears the spread between seeds of one
  configuration, which `training/sweep.py` prints beside it. Nothing is concluded from fewer than
  five seeds.

## Names

Nothing is named by an incremented number. A training run is named for its encoder and label
count, and `training/runs/README.md`, generated by `sweep.py --index`, says what each changed and
scored. A label set is named by its fingerprint, the sheet by the hash of its text.
