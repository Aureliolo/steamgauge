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

| Assistant | `connect` name | The file it writes |
| --- | --- | --- |
| Claude Desktop | `claude-desktop` | `claude_desktop_config.json` in Claude's settings folder; quit Claude Desktop and open it again |
| Cursor | `cursor` | `~/.cursor/mcp.json` |
| VS Code and GitHub Copilot CLI | `copilot` | `~/.copilot/mcp-config.json`; VS Code asks once whether to trust it |
| Devin Desktop (Windsurf) | `devin` | `mcp_config.json` in Devin's settings folder |
| Codex | `codex` | `~/.codex/config.toml` |
| Gemini CLI | `gemini` | `~/.gemini/settings.json` |
| Zed | `zed` | Zed's `settings.json` |
| opencode | `opencode` | `~/.config/opencode/opencode.json` |
| Cline | `cline` | `~/.cline/data/settings/cline_mcp_settings.json` |
| Junie | `junie` | `~/.junie/mcp/mcp.json` |

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
