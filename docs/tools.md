# MCP tools

Every prompt and tool the app's MCP server offers. A tool marked "Only looks" says so to the
assistant, and some assistants then use it without asking you first.

## Prompts

Requests you pick from your assistant's own list, such as the commands Claude Code shows under
`/`.

<!-- steamgauge-site: prompts -->

### `what_players_say`

**What players say about a game.** What a game's players praise and criticise, from every one of its
Steam reviews. The game is downloaded and read first where it has to be.

- `game`, required: The game: its name, its Steam store link or its app id.

### `compare_games`

**Compare games.** Where the players of two or more games differ, subject by subject. Each game is
downloaded and read first where it has to be.

- `games`, required: Two or more games, by name, Steam store link or app id, separated by commas.

### `around_an_update`

**How players took an update.** What moved in a game's reviews in the four weeks after one of its
developer's updates against the four weeks before.

- `game`, required: The game: its name, its Steam store link or its app id.
- `update`: The update, by its title or date as Steam lists it. The newest when none is given.

<!-- steamgauge-site: end -->

<!-- steamgauge-site: tools -->

## The library

### `overview`

The cockpit: what changed since the person last looked, what is running, what needs a look and what
this machine reads with. Only looks.

### `library`

The games kept, each with how far it has got (downloaded, read), and where the library folder is.
Only looks.

### `games`

Every game kept, with its figures, its latest change and whether Steam has new reviews for it. Only
looks.

### `find_games`

Finds games on the Steam store by name, with their app ids. Only looks.

- `words`, required: A game's name, or part of it.

### `look_up`

What the Steam store says about a game now: its name, how many reviews it has and their verdict.
Only looks.

- `app_id`, required: The game's Steam app id.

### `art`

A game's header picture from the store. Only looks.

- `app_id`, required: The game's Steam app id.

### `groups`

The groups the person sorts their games into. Only looks.

### `save_groups`

Replaces the groups the person sorts their games into.

- `groups`, required: Every group, as `groups` gives them. A group left out is deleted.

### `since_last_look`

What moved in one game since the person last looked at its page. Only looks.

- `app_id`, required: The game's Steam app id.

### `looked`

Marks a game's page, or the cockpit when no game is named, as seen now, so only later changes count
as new there.

- `app_id`: A game's Steam app id, or none.

### `show`

Brings the window forward on the page asked for. Only looks.

- `app_id`: The game, for a game's page, a subject or a search.
- `app_ids`: The games to compare.
- `group`: The group's name, as `groups` gives it.
- `page`, required: Which page of the window to show.
  - `cockpit`: What moved, what is running and what needs a look.
  - `library`: Every game kept.
  - `finder`: Finding a game on Steam to add.
  - `settings`: How the app runs.
  - `assistants`: The AI assistants it is added to, and the HTTP server.
  - `storage`: What the library and the models take on disk.
  - `about`: This copy's version and the newest release.
  - `game`: One game's page. Needs `app_id`.
  - `compare`: Games side by side. Needs `app_ids`.
  - `group`: What players say across one group of games. Needs `group`.
  - `subject`: The reviews behind one subject of a read game. Needs `app_id` and `subject`.
  - `search`: A search of a read game's claims. Needs `app_id` and `query`.
- `query`: The words to search a game for.
- `side`: "praise", "complaint" or "neutral", to open a subject on one side.
- `subject`: The subject's id, as `reading` gives it.

## A read game

### `reading`

A read game's figures: every subject, how often players raise, praise and criticise it, and the
terms that stand out. Where the game is prepared for search by meaning, also the writer's paragraph
and the themes said most widely on each side. Monthly figures, kinds of reviewer and languages come
when asked for. Only looks.

- `app_id`, required: The game's Steam app id.
- `languages`: Also the languages the reviews are written in.
- `months`: Also how the figures move month by month.
- `themes`: How many themes on each side, the most widely said first, 10 by default. `claims_behind`
  with a theme gives the reviews behind one.
- `who`: Also the kinds of reviewer, and where each differs from the rest.

### `claims_behind`

A page of the claims behind one subject of a read game, each with its review. It can narrow to one
side, term, kind of reviewer or theme. Only looks.

- `app_id`, required: The game's Steam app id.
- `count`, required: How many claims to give.
- `from`, required: How many claims to skip, for the next page.
- `side`: "praise", "complaint" or "neutral", for one side only.
- `subject`, required: The subject's id, as `reading` gives it.
- `term`: Only the claims using this term, as `reading` lists under the subject.
- `theme`: Only the claims of one theme, by its place among the subject's themes on `side`, as
  `reading` gives it.
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

- `app_id`, required: The game's Steam app id.

### `game_updates`

The updates a game's developer has posted on Steam, with what moved in the reviews around each. Only
looks.

- `app_id`, required: The game's Steam app id.

### `before_after`

Each subject in the four weeks before one of a game's updates against the four weeks after. Only
looks.

- `app_id`, required: The game's Steam app id.
- `gid`, required: The update's id, as `game_updates` gives it.
- `kind`: One kind of reviewer only, or none for every reviewer.

### `compare`

Games side by side: each subject's figures for each. Only looks.

- `app_ids`, required: Steam app ids.

### `group_reading`

What players say across one group's read games: each subject's share, praise and complaint, how many
games raise it and which most. Shares come with every game counting the same and with every review
counting the same, beside each game's own figures. Only looks.

- `name`, required: The group's name, as `groups` gives it.

## Search by meaning

### `meaning_offer`

Whether a game is ready for search by meaning, and what preparing it would take on this machine.
Only looks.

- `app_id`, required: The game's Steam app id.

### `choose_meaning`

Sets whether every game read from now on is also prepared for search by meaning.

- `every_game`, required: True to prepare every game read from now on.

### `search_by_meaning`

The claims of a prepared game nearest in meaning to what was searched, beyond those its words found,
best first. Only looks.

- `app_id`, required: A prepared game's Steam app id.
- `query`, required: The meaning to find claims for.

### `writer_offer`

Whether a paragraph is written when a game is prepared, and what the writer would take to download
and run on this machine. Only looks.

### `choose_writer`

Sets whether preparing a game for search by meaning also writes a paragraph of what stands out, with
a model on this machine.

- `write`, required: Whether preparing a game also writes its paragraph.

## Jobs

### `work`

The jobs on the board: what each is, where it stands, how far it has got and how long is left. Gives
every running job and the nearest few of each other state, with how many were left out. Only looks.

- `app_id`: Only the jobs about this game, every one of them.
- `jobs`: How many jobs of each other state: the next queued, the latest ended. The default is 10.

### `read_offer`

What reading a game would take on this machine, for each reader size, and which is recommended. Only
looks.

- `app_id`: The game to price a read of, or none for a typical game.
- `language`: The language to read, or none for every language.

### `queue`

Starts jobs: downloads, updates, reads, recounts, preparations for search by meaning, checks, and
exports to a given path. Returns their ids at once. `work` shows how they go.

- `tasks`, required: The jobs to start. Jobs that share the network, the graphics card or the disk
  run one after another, in the order given.

### `queue_reads`

Queues a read of each game, in the language it was last read in or the one set for first readings.

- `app_ids`, required: Steam app ids.

### `queue_updates`

Queues an update of these games, or every game: new and edited reviews, then a new read of any game
read before.

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

Asks the person where to save a read game's data, then starts writing it. To write to a known path
without asking, use `queue` with an export_data task.

- `app_id`, required: The game's Steam app id.

### `export_report`

Asks the person where to save a report of these games, then starts writing it. To write to a known
path without asking, use `queue` with an export task.

- `app_ids`, required: Steam app ids.

## Settings and storage

### `settings`

The app's settings: the share of the graphics card a read may take, reader size, first-reading
language, checks and library folder. Only looks.

### `save_settings`

Replaces every setting at once, so give all of them.

- `search_every_game`, required: Whether every game read is prepared for search by meaning too.
- `settings`, required: Every setting, as `settings` gives them.

### `reader_options`

The reader sizes this machine can read with, what each costs to download, and the room left for
them. Only looks.

### `storage`

What the library and the models take on disk, game by game and part by part, and the room left on
each drive. Only looks.

### `free_room`

Removes a part of one game or of every game, or a whole game, to free disk space. What it removes
has to be downloaded or worked out again.

- `app_id`: The game to remove from, or none to remove that part from every game.
- `what`, required: What to remove.
  - `reads`: Its readings, which a read makes again.
  - `search`: Its preparation for search by meaning.
  - `earlier`: Its downloads from before the newest one.
  - `partial`: What a stopped download, read or recount left behind.
  - `game`: The whole game, with its crawl records and its place in groups.

### `remove_model`

Removes a downloaded model. What it removes has to be downloaded or worked out again.

- `key`, required: The model's key, as `storage` lists it.

## AI assistants

### `connect_client`

Adds this copy of the app to an MCP client's configuration and returns the settings. The client can
reach the open app from then on.

- `client`, required: The client's id, as settings lists it.

### `disconnect_client`

Takes the app out of an MCP client's configuration and returns the settings.

- `client`, required: The client's id, as settings lists it.

### `new_http_token`

Draws a new token for the HTTP server and returns the settings with it. Clients holding the old
token are shut out.

## Updates

### `about`

This copy's version, whether its build was checked on install, and the version it was updated from.
Also the newest release GitHub last named, who makes SteamGauge, what it is built on and where it is
published. Only looks.

### `check_for_updates`

Asks GitHub now for the newest SteamGauge release and returns what `about` returns.

### `newer_version`

The newer SteamGauge release last heard of, if there is one. Only looks.

### `whats_new`

What each release up to the newest changes, as the About page lists it. With no newer release, it
gives the changes since the version this copy was updated from. Asks GitHub's releases API. Only
looks.

### `saw_what_changed`

Records that the person has seen what changed in the last update, so the sidebar stops pointing at
it.

### `open_notices`

Opens the licences and notices of the open-source software SteamGauge is built on, in the system's
text viewer. Only looks.

### `update_state`

Where an update of SteamGauge stands, or what keeps this copy from updating itself. Only looks.

### `update_now`

Updates SteamGauge to the newest release once its build provenance verifies, then restarts it. The
next call reaches the new version.

### `show_update_file`

Opens the system's file manager at a verified update the app could not install, so the person can
install it by hand.

<!-- steamgauge-site: end -->
