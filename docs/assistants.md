# AI assistants

The app is a Model Context Protocol (MCP) server. An assistant can do anything the window can:
find, download and read games, ask what their players say, export, and change settings. The
window shows each step. [MCP tools](tools.md) lists every tool.

## Adding it

Settings lists the assistants on this computer with **Add** and **Remove** beside each. From a
terminal, `steamgauge clients` lists them and `steamgauge connect <name>` and
`steamgauge disconnect <name>` do the same. Either writes one entry into the assistant's own
settings file and leaves the rest of it as it was.

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

## Any other assistant

It starts SteamGauge as this copy's path followed by `mcp`; Settings shows the path ready to copy.
Below, `<path>` stands for it.

- **Claude Code:** `claude mcp add --scope user steamgauge -- "<path>" mcp`
- **JetBrains AI Assistant:** Settings, Tools, AI Assistant, Model Context Protocol (MCP), Add,
  at the Global level, with
  `{"mcpServers": {"steamgauge": {"command": "<path>", "args": ["mcp"]}}}`
- **LM Studio:** the Program tab, Install, Edit `mcp.json`, with the same JSON.
- **Goose:** `goose configure`, Add Extension, Command-line Extension, with `"<path>" mcp`.
- **Continue:** in `~/.continue/config.yaml`, under `mcpServers`, `name: steamgauge`,
  `type: stdio`, `command:` the path and `args: [mcp]`.

## How it connects

- **A local socket**, a named pipe on Windows, that no other user or machine can reach. If the
  app is not running it starts with its window hidden, and closes when the assistant is done
  and its work has finished.
- **Both handshakes**, `initialize` and `server/discover`.
- **HTTP, if you need it.** Off until Settings turns it on: the app then also answers on
  127.0.0.1 at a port you choose, to requests carrying a token Settings shows.
