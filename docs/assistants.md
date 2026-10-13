# AI assistants

SteamGauge is an MCP server. An assistant can find, download and read games and ask what their
players say, and the window shows each step. [MCP tools](tools.md) lists every tool.

## Hand it to your assistant

Paste this:

<!-- steamgauge-site: paste -->

```text
Add SteamGauge to yourself by following https://aureliolo.github.io/steamgauge/agent.md,
then tell me which file you changed.
```

<!-- steamgauge-site: end -->

It reads [the guide](agent.md), adds SteamGauge to its own settings, and says which file it
changed.

## Or do it yourself

Settings has **Add** beside every assistant it finds on this computer. From a terminal:

<!-- steamgauge-site: clients -->

- **Claude Desktop**: `steamgauge connect claude-desktop`. Writes
  `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS,
  `~/.config/Claude/claude_desktop_config.json` on Linux,
  `%APPDATA%\Claude\claude_desktop_config.json` on Windows. Quit Claude Desktop completely and open
  it again.
- **Cursor**: `steamgauge connect cursor`. Writes `~/.cursor/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.cursor\mcp.json` on Windows.
- **VS Code and GitHub Copilot CLI**: `steamgauge connect copilot`. Writes
  `~/.copilot/mcp-config.json` on macOS and Linux, `%USERPROFILE%\.copilot\mcp-config.json` on
  Windows. VS Code asks once whether to trust it, the first time it starts.
- **Devin Desktop (Windsurf)**: `steamgauge connect devin`. Writes `~/.config/devin/mcp_config.json`
  on macOS and Linux, `%APPDATA%\devin\mcp_config.json` on Windows.
- **Codex**: `steamgauge connect codex`. Writes `~/.codex/config.toml` on macOS and Linux,
  `%USERPROFILE%\.codex\config.toml` on Windows.
- **Gemini CLI**: `steamgauge connect gemini`. Writes `~/.gemini/settings.json` on macOS and Linux,
  `%USERPROFILE%\.gemini\settings.json` on Windows.
- **Zed**: `steamgauge connect zed`. Writes `~/.config/zed/settings.json` on macOS and Linux,
  `%APPDATA%\Zed\settings.json` on Windows.
- **opencode**: `steamgauge connect opencode`. Writes `~/.config/opencode/opencode.json` on macOS
  and Linux, `%USERPROFILE%\.config\opencode\opencode.json` on Windows.
- **Cline**: `steamgauge connect cline`. Writes `~/.cline/data/settings/cline_mcp_settings.json` on
  macOS and Linux, `%USERPROFILE%\.cline\data\settings\cline_mcp_settings.json` on Windows.
- **Junie**: `steamgauge connect junie`. Writes `~/.junie/mcp/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.junie\mcp\mcp.json` on Windows.
- **Claude Code**: `claude mcp add --scope user steamgauge -- "<path to steamgauge>" mcp`. In a
  terminal.
- **JetBrains AI Assistant**:
  `{"mcpServers": {"steamgauge": {"command": "<path to steamgauge>", "args": ["mcp"]}}}`. Settings,
  Tools, AI Assistant, Model Context Protocol (MCP), Add, at the Global level.
- **LM Studio**:
  `{"mcpServers": {"steamgauge": {"command": "<path to steamgauge>", "args": ["mcp"]}}}`. The
  Program tab, Install, Edit `mcp.json`.
- **Goose**: `"<path to steamgauge>" mcp`. `goose configure`, Add Extension, Command-line Extension,
  with this command.
- **Continue**: In `~/.continue/config.yaml`.

  ```yaml
  mcpServers:
    - name: steamgauge
      type: stdio
      command: <path to steamgauge>
      args: [mcp]
  ```

- **Something else**: `"<path to steamgauge>" mcp`. Any assistant that starts an MCP server from a
  command. `steamgauge clients` prints this copy's path.

<!-- steamgauge-site: end -->

`<path to steamgauge>` is where the program is: `%LOCALAPPDATA%\SteamGauge\steamgauge.exe` from
the Windows setup, `/Applications/SteamGauge.app/Contents/MacOS/steamgauge` on macOS, and
`steamgauge` from Scoop, Homebrew or a Linux package. `steamgauge clients` prints it.

## What it writes

One entry, named `steamgauge`, in the assistant's own settings file; everything else in the file
stays as it was, comments included. `steamgauge disconnect <name>` takes it out again.

<!-- steamgauge-site: entries -->

Claude Desktop, Devin Desktop (Windsurf), Gemini CLI, Cline and Junie:

```json
{
  "mcpServers": {
    "steamgauge": {
      "command": "<path to steamgauge>",
      "args": ["mcp"]
    }
  }
}
```

Cursor and VS Code and GitHub Copilot CLI:

```json
{
  "mcpServers": {
    "steamgauge": {
      "type": "stdio",
      "command": "<path to steamgauge>",
      "args": ["mcp"]
    }
  }
}
```

Codex:

```toml
[mcp_servers.steamgauge]
command = "<path to steamgauge>"
args = ["mcp"]
```

Zed:

```json
{
  "context_servers": {
    "steamgauge": {
      "command": "<path to steamgauge>",
      "args": ["mcp"]
    }
  }
}
```

opencode:

```json
{
  "mcp": {
    "steamgauge": {
      "type": "local",
      "command": ["<path to steamgauge>", "mcp"],
      "enabled": true
    }
  }
}
```

<!-- steamgauge-site: end -->

## How it connects

- **A local socket**, a named pipe on Windows, that no other user or machine can reach. If the
  app is not running it starts with its window hidden, and closes once the assistant is done.
- **Both handshakes**, `initialize` and `server/discover`.
- **HTTP, if you need it.** Off until Settings turns it on: the app then also answers on
  127.0.0.1 at a port you choose, to requests carrying a token Settings shows.
