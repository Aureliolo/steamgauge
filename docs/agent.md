# Adding SteamGauge to an AI assistant

This page is for an AI assistant whose person asked it to add SteamGauge to itself. Follow the
steps in order, then tell the person what you did.

## 1. Find the program

Run `steamgauge --version`. On Windows the program prints only into a pipe: in PowerShell, end
each command with `| Out-Host`. Git Bash needs nothing. If it is not found, look where its
installer put it:

| Installed with | The program |
|---|---|
| The Windows setup | `%LOCALAPPDATA%\SteamGauge\steamgauge.exe` |
| The macOS app | `/Applications/SteamGauge.app/Contents/MacOS/steamgauge` |
| Scoop, Homebrew, the `.deb` or the `.rpm` | `steamgauge`, on the `PATH` |

If it is in none of those places, it is not installed. Send the person to
[Install](install.md) rather than installing it yourself, unless they ask you to.

## 2. Add it

Run `steamgauge clients`. It lists the assistants SteamGauge can add itself to, and ends with the
command any other assistant takes.

- **If you are listed**, run `steamgauge connect <name>` with your name from the first column. It
  writes one entry into your settings file and prints the file it wrote.
- **If you are not**, add an MCP server called `steamgauge` to your own settings, started by the
  command on the last line.

## 3. Check it

Reload your MCP servers, or ask the person to restart you, then call the `overview` tool.
SteamGauge answers only while its window is open. If the tool says it is not open, ask the person
to open it, then call `overview` again.

## 4. Report

Tell the person which file you changed, and anything `connect` printed for them to do. Some
assistants read their settings only when they start.
