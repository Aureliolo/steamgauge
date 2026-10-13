# Adding SteamGauge to an AI assistant

This page is for an AI assistant whose person asked it to add SteamGauge to itself. Follow the
steps in order, then tell the person what you did.

## 1. Find the program

Run `steamgauge --version`. On Windows the program prints only into a pipe: in PowerShell, end
each command with `| Out-Host`; Git Bash needs nothing. If it is not found, it is where its
installer put it:

| Installed with | The program |
|---|---|
| The Windows setup, or winget | `%LOCALAPPDATA%\SteamGauge\steamgauge.exe` |
| The macOS app | `/Applications/SteamGauge.app/Contents/MacOS/steamgauge` |
| Scoop, Homebrew, the `.deb` or the `.rpm` | `steamgauge`, on the `PATH` |

If it is in none of those places, it is not installed. Send the person to
[Install](install.md) rather than installing it yourself, unless they ask you to.

## 2. Add it

Run `steamgauge clients`. It lists the assistants SteamGauge can add itself to, whether each is on
this computer and whether it already starts this copy, and ends with the command any other
assistant takes.

- **If you are listed**, run `steamgauge connect <name>` with your name from the first column. It
  writes one entry into your own settings file, leaves the rest of the file as it was, and
  prints the file it wrote.
- **If you are not**, add an MCP server called `steamgauge` to your own settings, started by the
  command on the last line: this copy's path followed by `mcp`. Claude Code takes it as
  `claude mcp add --scope user steamgauge -- "<path>" mcp`.

## 3. Check it

Reload your MCP servers, or ask the person to restart you if you cannot, then call the
`overview` tool. It answers with the library, what is running and what this computer reads with.
If SteamGauge was not open it starts with its window hidden.

## 4. Report

Tell the person which file you changed, and anything `connect` printed for them to do: some
assistants read their settings only when they start.
