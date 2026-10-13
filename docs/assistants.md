# Steering it from an AI assistant

Any AI assistant that speaks the Model Context Protocol (MCP) can steer the app. Every tool is
something the window can do, from finding, downloading and reading a game to searching its
reviews, exporting, changing settings and removing things, and the window shows each one done.
The open app answers; where it is not running, it opens with its window hidden and closes once
the program is finished with it and its work is done. It listens on a local socket that no other
user and no other machine can reach, and speaks both the `initialize` handshake and the
`server/discover` one. For a program that only connects over HTTP, Settings can also have the
open app answer on 127.0.0.1, at a port you choose, to requests that carry a token Settings
shows; it is off until you switch it on.

Settings lists the assistants on this computer that SteamGauge can add itself to, with **Add**
and **Remove** beside each, and `steamgauge clients`, `steamgauge connect <client>` and
`steamgauge disconnect <client>` do the same from a terminal. Either writes one entry into the
assistant's own configuration file and leaves everything else in it as it was.

<!-- steamgauge-site: clients -->

- **Claude Desktop**, `connect claude-desktop`: `%APPDATA%\Claude\claude_desktop_config.json` on
  Windows, `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS,
  `~/.config/Claude/claude_desktop_config.json` on Linux. Quit Claude Desktop completely and open it
  again.
- **Cursor**, `connect cursor`: `%USERPROFILE%\.cursor\mcp.json` on Windows, `~/.cursor/mcp.json` on
  macOS and Linux.
- **VS Code and GitHub Copilot CLI**, `connect copilot`: `%USERPROFILE%\.copilot\mcp-config.json` on
  Windows, `~/.copilot/mcp-config.json` on macOS and Linux. VS Code asks once whether to trust it,
  the first time it starts.
- **Devin Desktop (Windsurf)**, `connect devin`: `%APPDATA%\devin\mcp_config.json` on Windows,
  `~/.config/devin/mcp_config.json` on macOS and Linux.
- **Codex**, `connect codex`: `%USERPROFILE%\.codex\config.toml` on Windows, `~/.codex/config.toml`
  on macOS and Linux.
- **Gemini CLI**, `connect gemini`: `%USERPROFILE%\.gemini\settings.json` on Windows,
  `~/.gemini/settings.json` on macOS and Linux.
- **Zed**, `connect zed`: `%APPDATA%\Zed\settings.json` on Windows, `~/.config/zed/settings.json` on
  macOS and Linux.
- **opencode**, `connect opencode`: `%USERPROFILE%\.config\opencode\opencode.json` on Windows,
  `~/.config/opencode/opencode.json` on macOS and Linux.
- **Cline**, `connect cline`: `%USERPROFILE%\.cline\data\settings\cline_mcp_settings.json` on
  Windows, `~/.cline/data/settings/cline_mcp_settings.json` on macOS and Linux.
- **Junie**, `connect junie`: `%USERPROFILE%\.junie\mcp\mcp.json` on Windows,
  `~/.junie/mcp/mcp.json` on macOS and Linux.

<!-- steamgauge-site: end -->

Any other assistant starts SteamGauge with this copy's own path followed by `mcp`, which
Settings shows ready to copy; below, `<path>` stands for it. Most take it like this:

- **Claude Code:** `claude mcp add --scope user steamgauge -- "<path>" mcp`.
- **JetBrains AI Assistant:** Settings, Tools, AI Assistant, Model Context Protocol (MCP), Add,
  with the server level Global, and the JSON
  `{"mcpServers": {"steamgauge": {"command": "<path>", "args": ["mcp"]}}}`.
- **LM Studio:** the Program tab, Install, Edit `mcp.json`, and the same JSON.
- **Goose:** `goose configure`, Add Extension, Command-line Extension, with the command
  `"<path>" mcp`.
- **Continue:** in `~/.continue/config.yaml`, an item under `mcpServers` with
  `name: steamgauge`, `type: stdio`, `command:` the path, and `args: [mcp]`.

## The tools

Each tool, with what it is given. A tool tells the assistant whether it only looks and whether
it removes something, and some assistants use that to decide when to ask you first.

<!-- steamgauge-site: tools -->

- `library`: The games kept, each with how far it has got (downloaded, read), and where the library
  folder is. It only looks.
- `games`: Every game kept, with its figures, its newest movement and whether Steam has new reviews
  for it: the library page's rows. It only looks.
- `overview`: The cockpit: what moved since last looked, what is running, what needs a look, and
  what this machine reads with. It only looks.
- `look_up`: What the Steam store says about a game now: its name, how many reviews it has and their
  verdict. It only looks.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `find_games`: Finds games on the Steam store by name, with their app ids. It only looks.
  - `words`, required: A game's name, or part of it.
- `art`: A game's header picture from the store. It only looks.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `reading`: A read game's figures: every subject its players talk about, how often, praised and
  criticised, with the terms that stand out and how the figures move month by month. It only looks.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `induced`: What a read game's players talk about that the fixed subjects do not name, with quoted
  reviews, where that has been worked out. It only looks.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `who_wrote`: One kind of reviewer beside another, or beside everyone else: how each subject's
  figures differ between them. It only looks.
  - `app_id`, required: The game's Steam app id.
  - `others`: The kind to set them beside, or none for everyone else.
  - `these`, required: The kind of reviewer to look at, one of the kinds `reading` lists under
    `who`.
- `claims_behind`: A page of the claims behind one subject of a read game, with the review each
  comes from, narrowed to one side, a term or a kind of reviewer if asked. It only looks.
  - `app_id`, required: The game's Steam app id.
  - `count`, required: How many claims to give.
  - `from`, required: How many claims to skip, for the next page.
  - `side`: "praise", "complaint" or "neutral", for one side only.
  - `subject`, required: The subject's id, as `reading` gives it.
  - `term`: Only the claims using this term, as `reading` lists under the subject.
  - `who`: Only one kind of reviewer's claims.
- `search_game`: Every claim of a read game that says the words searched, counted by side and
  subject, with a page of them. It only looks.
  - `app_id`, required: The game's Steam app id.
  - `count`, required: How many claims to give.
  - `from`, required: How many claims to skip, for the next page.
  - `query`, required: The words to find.
  - `side`: "praise", "complaint" or "neutral", to page through one side only.
  - `subject`: A subject's id, to page through the claims filed under it only.
- `meaning_offer`: Whether a game is ready for search by meaning, and what preparing it would take
  on this machine. It only looks.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `choose_meaning`: Whether every game read from now on is also prepared for search by meaning.
  - `every_game`, required: Whether every game read from now on is prepared for search by meaning
    too.
- `search_by_meaning`: The claims of a prepared game nearest in meaning to what was searched, beyond
  those its words found, best first. It only looks.
  - `app_id`, required: The game's Steam app id, which has to be prepared for search by meaning.
  - `query`, required: What to find claims meaning.
- `read_offer`: What reading a game would take on this machine, for each reader size, and which is
  recommended. It only looks.
  - `app_id`: The game to price a read of, or none for a typical game.
  - `language`: The language to read, or none for every language.
- `game_updates`: The updates a game's developer has posted on Steam, with what moved in the reviews
  around each. It only looks.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `before_after`: Each subject in the four weeks before one of a game's updates against the four
  weeks after. It only looks.
  - `app_id`, required: The game's Steam app id.
  - `gid`, required: The update's id, as `game_updates` gives it.
  - `kind`: One kind of reviewer only, or none for every reviewer.
- `since_last_look`: What moved in one game since the person last looked at its page. It only looks.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `looked`: Marks a game's page, or the cockpit where no game is named, as seen by the person now,
  which resets what counts as new there.
  - `app_id`: A game's Steam app id, or none.
- `compare`: Games side by side: each subject's figures for each. It only looks.
  - `app_ids`, required: Steam app ids.
- `groups`: The groups the person sorts their games into. It only looks.
- `save_groups`: Replaces the groups the person sorts their games into.
  - `groups`, required: Every group, as `groups` gives them; a group left out is deleted.
- `work`: Every job on the board: what it is, where it stands, how far it has got and how long is
  left. It only looks.
- `queue`: Starts jobs: downloads, updates, reads, recounts, preparations for search by meaning,
  checks, and exports written to a path given here. Returns their ids at once; work follows them.
  - `tasks`, required: The jobs to start, run in the order given within each lane: downloads and
    checks share the network, reads and preparations the graphics card, exports the disk.
- `queue_reads`: Reads these games, each in the language it was last read in, or in the one chosen
  for first readings.
  - `app_ids`, required: Steam app ids.
- `queue_updates`: Updates these games, or every game: what was written or edited since, then a read
  where one was read.
  - `app_ids`: Steam app ids, or none for every game in the library.
- `stop_job`: Stops a job, or takes it off the board if it has not started.
  - `id`, required: The job's id, from `queue` or `work`.
- `clear_finished`: Takes every finished job off the board.
- `open_report`: Opens a finished report job's page in the person's browser.
  - `id`, required: The job's id, from `queue` or `work`.
- `export_data`: Asks the person, in the system's save dialog, where to save a read game's data,
  then starts writing it. queue with an export_data task writes to a path without asking.
  - `app_id`, required: The game's Steam app id, as in its store address.
- `export_report`: Asks the person, in the system's save dialog, where to save a report of these
  games, then starts writing it. queue with an export task writes to a path without asking.
  - `app_ids`, required: Steam app ids.
- `settings`: How the app runs: the share of the graphics card a read may take, the reader size, the
  first-reading language, the checks it makes, and where the library is. It only looks.
- `save_settings`: Replaces every setting; give all of them, as settings returns them.
  - `search_every_game`, required: Whether every game read is prepared for search by meaning too.
  - `settings`, required: Every setting, as `settings` gives them.
- `new_http_token`: Draws a new token for the HTTP server, shutting out every client that holds the
  old one, and returns the settings with it.
- `connect_client`: Adds this copy of the app to another MCP client's own configuration, as settings
  lists the clients on this computer, leaving every other entry in it; the client starts the app
  from then on. Returns the settings with it.
  - `client`, required: The client's id, as settings lists it.
- `disconnect_client`: Takes the app out of an MCP client's configuration, leaving every other entry
  in it, and returns the settings without it.
  - `client`, required: The client's id, as settings lists it.
- `reader_options`: The reader sizes this machine can read with, what each costs to download, and
  the room left for them. It only looks.
- `storage`: What the library and the models take on disk, game by game and part by part, and the
  room left on each drive. It only looks.
- `free_room`: Removes part of a game, or of every game: its readings, its preparation for search by
  meaning, its earlier downloads, what work stopped part way left, or the whole game. What it
  removes has to be downloaded or worked out again.
  - `app_id`: The game to remove from, or none to remove that part from every game.
  - `what`, required: What to remove.
    - `reads`: Its readings, which a read makes again.
    - `search`: Its preparation for search by meaning.
    - `earlier`: Its earlier downloads, kept from before its newest one.
    - `partial`: What work stopped part way left: a download, a read or a recount.
    - `game`: The whole game: every file, its crawl records and its place in groups.
- `remove_model`: Removes a downloaded model; a read that needs it downloads it again. What it
  removes has to be downloaded or worked out again.
  - `key`, required: The model's key, as `storage` lists it.
- `newer_version`: The newer SteamGauge release last heard of, if there is one. It only looks.
- `update_state`: Where an update of SteamGauge stands, or what keeps this copy from updating
  itself. It only looks.
- `update_now`: Updates SteamGauge to the newest release once its build provenance verifies, then
  restarts it, which ends this connection.
- `show_update_file`: Shows a verified update the app could not install in the system's file
  manager, for installing it by hand.
- `show`: Puts a page of the window in front of the person, opening the window if it is hidden. It
  only looks.
  - `app_id`: The game, for a game's page, a subject or a search.
  - `app_ids`: The games to compare.
  - `page`, required: Which page of the window to show.
    - `cockpit`: What moved, what is running and what needs a look.
    - `library`: Every game kept.
    - `finder`: Finding a game on Steam to add.
    - `settings`: How the app runs, and the assistants it is added to.
    - `storage`: What the library and the models take on disk.
    - `game`: One game's page; needs `app_id`.
    - `compare`: Games side by side; needs `app_ids`.
    - `subject`: The reviews behind one subject of a read game; needs `app_id` and `subject`.
    - `search`: A read game's claims that say something; needs `app_id` and `query`.
  - `query`: The words to search a game for.
  - `side`: "praise", "complaint" or "neutral", to open a subject on one side.
  - `subject`: The subject's id, as `reading` gives it.

<!-- steamgauge-site: end -->
