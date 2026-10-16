# MCP tools

Every prompt and tool the app's MCP server offers, written from the server's own definitions. A
tool that only looks tells the assistant so, and some assistants then use it without asking you
first.

## Prompts

Requests you pick from your assistant's own list, such as the commands Claude Code shows under
`/`. Each asks the assistant to work through the tools below, and the window shows each step.

<!-- steamgauge-site: prompts -->

### `what_players_say`

**What players say about a game.** What a game's players praise and criticise, from every one of its
Steam reviews, downloading and reading it first where it has to be.

- `game`, required: The game: its name, its Steam store link or its app id.

### `compare_games`

**Compare games.** Where the players of two or more games differ, subject by subject, downloading
and reading each first where it has to be.

- `games`, required: Two or more games, by name, Steam store link or app id, separated by commas.

### `around_an_update`

**How players took an update.** What moved in a game's reviews in the four weeks after one of its
developer's updates against the four weeks before.

- `game`, required: The game: its name, its Steam store link or its app id.
- `update`: The update, by its title or date as Steam lists it; the newest where none is given.

<!-- steamgauge-site: end -->

<!-- steamgauge-site: tools -->

## The library

### `overview`

The cockpit: what moved since last looked, what is running, what needs a look, and what this machine
reads with. Only looks.

### `library`

The games kept, each with how far it has got (downloaded, read), and where the library folder is.
Only looks.

### `games`

Every game kept, with its figures, its newest movement and whether Steam has new reviews for it: the
library page's rows. Only looks.

### `find_games`

Finds games on the Steam store by name, with their app ids. Only looks.

- `words`, required: A game's name, or part of it.

### `look_up`

What the Steam store says about a game now: its name, how many reviews it has and their verdict.
Only looks.

- `app_id`, required: The game's Steam app id, as in its store address.

### `art`

A game's header picture from the store. Only looks.

- `app_id`, required: The game's Steam app id, as in its store address.

### `groups`

The groups the person sorts their games into. Only looks.

### `save_groups`

Replaces the groups the person sorts their games into.

- `groups`, required: Every group, as `groups` gives them; a group left out is deleted.

### `since_last_look`

What moved in one game since the person last looked at its page. Only looks.

- `app_id`, required: The game's Steam app id, as in its store address.

### `looked`

Marks a game's page, or the cockpit where no game is named, as seen by the person now, which resets
what counts as new there.

- `app_id`: A game's Steam app id, or none.

### `show`

Puts a page of the window in front of the person, bringing the window forward. Only looks.

- `app_id`: The game, for a game's page, a subject or a search.
- `app_ids`: The games to compare.
- `page`, required: Which page of the window to show.
  - `cockpit`: What moved, what is running and what needs a look.
  - `library`: Every game kept.
  - `finder`: Finding a game on Steam to add.
  - `settings`: How the app runs.
  - `assistants`: The assistants it is added to, and answering over HTTP.
  - `storage`: What the library and the models take on disk.
  - `about`: This copy's version and the newest release, with the button that asks for it.
  - `game`: One game's page; needs `app_id`.
  - `compare`: Games side by side; needs `app_ids`.
  - `subject`: The reviews behind one subject of a read game; needs `app_id` and `subject`.
  - `search`: A read game's claims that say something; needs `app_id` and `query`.
- `query`: The words to search a game for.
- `side`: "praise", "complaint" or "neutral", to open a subject on one side.
- `subject`: The subject's id, as `reading` gives it.

## A read game

### `reading`

A read game's figures: every subject its players talk about, how often, praised and criticised, with
the terms that stand out and how the figures move month by month. Only looks.

- `app_id`, required: The game's Steam app id, as in its store address.

### `claims_behind`

A page of the claims behind one subject of a read game, with the review each comes from, narrowed to
one side, a term or a kind of reviewer if asked. Only looks.

- `app_id`, required: The game's Steam app id.
- `count`, required: How many claims to give.
- `from`, required: How many claims to skip, for the next page.
- `side`: "praise", "complaint" or "neutral", for one side only.
- `subject`, required: The subject's id, as `reading` gives it.
- `term`: Only the claims using this term, as `reading` lists under the subject.
- `who`: Only one kind of reviewer's claims.

### `search_game`

Every claim of a read game that says the words searched, counted by side and subject, with a page of
them. Only looks.

- `app_id`, required: The game's Steam app id.
- `count`, required: How many claims to give.
- `from`, required: How many claims to skip, for the next page.
- `query`, required: The words to find.
- `side`: "praise", "complaint" or "neutral", to page through one side only.
- `subject`: A subject's id, to page through the claims filed under it only.

### `who_wrote`

One kind of reviewer beside another, or beside everyone else: how each subject's figures differ
between them. Only looks.

- `app_id`, required: The game's Steam app id.
- `others`: The kind to set them beside, or none for everyone else.
- `these`, required: The kind of reviewer to look at, one of the kinds `reading` lists under `who`.

### `induced`

What a read game's players talk about that the fixed subjects do not name, with quoted reviews,
where that has been worked out. Only looks.

- `app_id`, required: The game's Steam app id, as in its store address.

### `game_updates`

The updates a game's developer has posted on Steam, with what moved in the reviews around each. Only
looks.

- `app_id`, required: The game's Steam app id, as in its store address.

### `before_after`

Each subject in the four weeks before one of a game's updates against the four weeks after. Only
looks.

- `app_id`, required: The game's Steam app id.
- `gid`, required: The update's id, as `game_updates` gives it.
- `kind`: One kind of reviewer only, or none for every reviewer.

### `compare`

Games side by side: each subject's figures for each. Only looks.

- `app_ids`, required: Steam app ids.

## Search by meaning

### `meaning_offer`

Whether a game is ready for search by meaning, and what preparing it would take on this machine.
Only looks.

- `app_id`, required: The game's Steam app id, as in its store address.

### `choose_meaning`

Whether every game read from now on is also prepared for search by meaning.

- `every_game`, required: Whether every game read from now on is prepared for search by meaning too.

### `search_by_meaning`

The claims of a prepared game nearest in meaning to what was searched, beyond those its words found,
best first. Only looks.

- `app_id`, required: The game's Steam app id, which has to be prepared for search by meaning.
- `query`, required: What to find claims meaning.

## Jobs

### `work`

Every job on the board: what it is, where it stands, how far it has got and how long is left. Only
looks.

### `read_offer`

What reading a game would take on this machine, for each reader size, and which is recommended. Only
looks.

- `app_id`: The game to price a read of, or none for a typical game.
- `language`: The language to read, or none for every language.

### `queue`

Starts jobs: downloads, updates, reads, recounts, preparations for search by meaning, checks, and
exports written to a path given here. Returns their ids at once; work follows them.

- `tasks`, required: The jobs to start, run in the order given within each lane: downloads and
  checks share the network, reads and preparations the graphics card, exports the disk.

### `queue_reads`

Reads these games, each in the language it was last read in, or in the one chosen for first
readings.

- `app_ids`, required: Steam app ids.

### `queue_updates`

Updates these games, or every game: what was written or edited since, then a read where one was
read.

- `app_ids`: Steam app ids, or none for every game in the library.

### `stop_job`

Stops a job, or takes it off the board if it has not started.

- `id`, required: The job's id, from `queue` or `work`.

### `clear_finished`

Takes every finished job off the board.

### `open_report`

Opens a finished report job's page in the person's browser.

- `id`, required: The job's id, from `queue` or `work`.

### `export_data`

Asks the person, in the system's save dialog, where to save a read game's data, then starts writing
it. queue with an export_data task writes to a path without asking.

- `app_id`, required: The game's Steam app id, as in its store address.

### `export_report`

Asks the person, in the system's save dialog, where to save a report of these games, then starts
writing it. queue with an export task writes to a path without asking.

- `app_ids`, required: Steam app ids.

## Settings and storage

### `settings`

How the app runs: the share of the graphics card a read may take, the reader size, the first-reading
language, the checks it makes, and where the library is. Only looks.

### `save_settings`

Replaces every setting; give all of them, as settings returns them.

- `search_every_game`, required: Whether every game read is prepared for search by meaning too.
- `settings`, required: Every setting, as `settings` gives them.

### `reader_options`

The reader sizes this machine can read with, what each costs to download, and the room left for
them. Only looks.

### `storage`

What the library and the models take on disk, game by game and part by part, and the room left on
each drive. Only looks.

### `free_room`

Removes part of a game, or of every game: its readings, its preparation for search by meaning, its
earlier downloads, what work stopped part way left, or the whole game. What it removes has to be
downloaded or worked out again.

- `app_id`: The game to remove from, or none to remove that part from every game.
- `what`, required: What to remove.
  - `reads`: Its readings, which a read makes again.
  - `search`: Its preparation for search by meaning.
  - `earlier`: Its earlier downloads, kept from before its newest one.
  - `partial`: What work stopped part way left: a download, a read or a recount.
  - `game`: The whole game: every file, its crawl records and its place in groups.

### `remove_model`

Removes a downloaded model; a read that needs it downloads it again. What it removes has to be
downloaded or worked out again.

- `key`, required: The model's key, as `storage` lists it.

## AI assistants

### `connect_client`

Adds this copy of the app to another MCP client's own configuration, as settings lists the clients
on this computer, leaving every other entry in it; the client reaches the open app from then on.
Returns the settings with it.

- `client`, required: The client's id, as settings lists it.

### `disconnect_client`

Takes the app out of an MCP client's configuration, leaving every other entry in it, and returns the
settings without it.

- `client`, required: The client's id, as settings lists it.

### `new_http_token`

Draws a new token for the HTTP server, shutting out every client that holds the old one, and returns
the settings with it.

## Updates

### `about`

This copy's version and details, who makes SteamGauge, the newest release GitHub last named, when it
was last asked and answered, the version this copy was updated from, and where SteamGauge is
published. Only looks.

### `check_for_updates`

Asks GitHub now for the newest SteamGauge release, as the About page's button does, and returns what
about returns.

### `newer_version`

The newer SteamGauge release last heard of, if there is one. Only looks.

### `whats_new`

What each release changes in what people download, as the About page lists it: up to the newer
release last heard of, or, with none, since the version this copy was updated from. Asks GitHub's
releases API. Only looks.

### `saw_what_changed`

Records that the person has been shown what changed since the version this copy was updated from, so
the sidebar stops pointing at it.

### `update_state`

Where an update of SteamGauge stands, or what keeps this copy from updating itself. Only looks.

### `update_now`

Updates SteamGauge to the newest release once its build provenance verifies, then restarts it; the
next call reaches the new version.

### `show_update_file`

Shows a verified update the app could not install in the system's file manager, for installing it by
hand.

<!-- steamgauge-site: end -->
