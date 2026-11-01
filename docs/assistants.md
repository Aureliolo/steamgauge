# AI assistants

SteamGauge is an MCP server. An assistant can find, download and read games and ask what their
players say. The window shows each step. [MCP tools](tools.md) lists every prompt and tool.

## Hand it to your assistant

Paste this:

<!-- steamgauge-site: paste -->

```text
Add SteamGauge to yourself by following https://aureliolo.github.io/steamgauge/agent.md,
then tell me which file you changed.
```

<!-- steamgauge-site: end -->

## Or do it yourself

The window's **AI assistants** page has **Add** beside every assistant it finds on this computer.
From a terminal:

<!-- steamgauge-site: clients -->

- **Claude Code**: `steamgauge connect claude-code`. Runs
  `claude mcp add --scope user steamgauge -- "<path to steamgauge>" mcp`. Claude Code starts it in
  its next session.
- **Claude Desktop**: `steamgauge connect claude-desktop`. Writes
  `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS,
  `~/.config/Claude/claude_desktop_config.json` on Linux,
  `%APPDATA%\Claude\claude_desktop_config.json` on Windows. Quit Claude Desktop completely and open
  it again.
- **VS Code**: `steamgauge connect vscode`. Writes
  `~/Library/Application Support/Code/User/mcp.json` on macOS, `~/.config/Code/User/mcp.json` on
  Linux, `%APPDATA%\Code\User\mcp.json` on Windows. VS Code asks once whether to trust it, the first
  time it starts.
- **GitHub Copilot CLI**: `steamgauge connect copilot`. Writes `~/.copilot/mcp-config.json` on macOS
  and Linux, `%USERPROFILE%\.copilot\mcp-config.json` on Windows.
- **Cursor**: `steamgauge connect cursor`. Writes `~/.cursor/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.cursor\mcp.json` on Windows.
- **Devin Desktop and Devin CLI**: `steamgauge connect devin`. Writes
  `~/.config/devin/mcp_config.json` on macOS and Linux, `%APPDATA%\devin\mcp_config.json` on
  Windows.
- **Windsurf**: `steamgauge connect windsurf`. Writes `~/.codeium/windsurf/mcp_config.json` on macOS
  and Linux, `%USERPROFILE%\.codeium\windsurf\mcp_config.json` on Windows. Refresh the servers in
  Cascade's MCP panel.
- **Codex**: `steamgauge connect codex`. Writes `~/.codex/config.toml` on macOS and Linux,
  `%USERPROFILE%\.codex\config.toml` on Windows.
- **Gemini CLI**: `steamgauge connect gemini`. Writes `~/.gemini/settings.json` on macOS and Linux,
  `%USERPROFILE%\.gemini\settings.json` on Windows.
- **Kiro**: `steamgauge connect kiro`. Writes `~/.kiro/settings/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.kiro\settings\mcp.json` on Windows.
- **Zed**: `steamgauge connect zed`. Writes `~/.config/zed/settings.json` on macOS and Linux,
  `%APPDATA%\Zed\settings.json` on Windows.
- **opencode**: `steamgauge connect opencode`. Writes `~/.config/opencode/opencode.json` on macOS
  and Linux, `%USERPROFILE%\.config\opencode\opencode.json` on Windows.
- **Kilo Code**: `steamgauge connect kilo`. Writes `~/.config/kilo/kilo.jsonc` on macOS and Linux,
  `%USERPROFILE%\.config\kilo\kilo.jsonc` on Windows.
- **Cline**: `steamgauge connect cline`. Writes `~/.cline/data/settings/cline_mcp_settings.json` on
  macOS and Linux, `%USERPROFILE%\.cline\data\settings\cline_mcp_settings.json` on Windows.
- **Roo Code**: `steamgauge connect roo`. Writes
  `~/Library/Application Support/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json`
  on macOS,
  `~/.config/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json` on
  Linux, `%APPDATA%\Code\User\globalStorage\rooveterinaryinc.roo-cline\settings\mcp_settings.json`
  on Windows.
- **Junie**: `steamgauge connect junie`. Writes `~/.junie/mcp/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.junie\mcp\mcp.json` on Windows.
- **Qoder CLI**: `steamgauge connect qoder`. Writes `~/.qoder/settings.json` on macOS and Linux,
  `%USERPROFILE%\.qoder\settings.json` on Windows. Restart Qoder CLI, or run /mcp reload in it.
- **Amp**: `steamgauge connect amp`. Writes `~/.config/amp/settings.json` on macOS and Linux,
  `%USERPROFILE%\.config\amp\settings.json` on Windows.
- **Warp**: `steamgauge connect warp`. Writes `~/.warp/.mcp.json` on macOS and Linux,
  `%USERPROFILE%\.warp\.mcp.json` on Windows.
- **Factory Droid**: `steamgauge connect factory`. Writes `~/.factory/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.factory\mcp.json` on Windows.
- **Tabnine CLI**: `steamgauge connect tabnine`. Writes `~/.tabnine/agent/settings.json` on macOS
  and Linux, `%USERPROFILE%\.tabnine\agent\settings.json` on Windows. Run /mcp reload in a running
  session.
- **Tabnine in the editor**: `steamgauge connect tabnine-ide`. Writes `~/.tabnine/mcp_servers.json`
  on macOS and Linux, `%USERPROFILE%\.tabnine\mcp_servers.json` on Windows. Tabnine reads it while a
  project folder is open.
- **Firebender**: `steamgauge connect firebender`. Writes `~/.firebender/firebender.json` on macOS
  and Linux, `%USERPROFILE%\.firebender\firebender.json` on Windows.
- **pi**: `steamgauge connect pi`. Writes `~/.pi/agent/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.pi\agent\mcp.json` on Windows. Run /reload in a running session.
- **Mistral Vibe**: `steamgauge connect vibe`. Writes `~/.vibe/config.toml` on macOS and Linux,
  `%USERPROFILE%\.vibe\config.toml` on Windows. A project's own .vibe/config.toml is read in place
  of this one.
- **VT Code**: `steamgauge connect vtcode`. Writes
  `~/Library/Application Support/com.vinhnx.vtcode/vtcode.toml` on macOS,
  `~/.config/vtcode/vtcode.toml` on Linux, `%APPDATA%\vinhnx\vtcode\config\vtcode.toml` on Windows.
  This also switches on VT Code's MCP support, which is off until set.
- **ZeroClaw**: `steamgauge connect zeroclaw`. Writes `~/.zeroclaw/config.toml` on macOS and Linux,
  `%USERPROFILE%\.zeroclaw\config.toml` on Windows. An agent uses it once one of its mcp_bundles
  names steamgauge, as ZeroClaw's MCP guide shows.
- **Goose**: `steamgauge connect goose`. Writes `~/.config/goose/config.yaml` on macOS and Linux,
  `%APPDATA%\Block\goose\config\config.yaml` on Windows. Goose loads it in its next session.
- **Hermes Agent**: `steamgauge connect hermes`. Writes `~/.hermes/config.yaml` on macOS and Linux,
  `%LOCALAPPDATA%\hermes\config.yaml` on Windows.
- **Continue**: `steamgauge connect continue`. Writes `~/.continue/config.yaml` on macOS and Linux,
  `%USERPROFILE%\.continue\config.yaml` on Windows. Continue uses it in agent mode, with Local
  Config chosen.
- **OpenClaw**: `steamgauge connect openclaw`. Writes `~/.openclaw/openclaw.json` on macOS and
  Linux, `%USERPROFILE%\.openclaw\openclaw.json` on Windows.
- **nanobot**: `steamgauge connect nanobot`. Writes `~/.nanobot/config.json` on macOS and Linux,
  `%USERPROFILE%\.nanobot\config.json` on Windows.
- **Autohand**: `steamgauge connect autohand`. Writes `~/.autohand/config.json` on macOS and Linux,
  `%USERPROFILE%\.autohand\config.json` on Windows. Restart Autohand.
- **Deep Code**: `steamgauge connect deepcode`. Writes `~/.deepcode/settings.json` on macOS and
  Linux, `%USERPROFILE%\.deepcode\settings.json` on Windows.
- **LM Studio**: `steamgauge connect lm-studio`. Writes `~/.lmstudio/mcp.json` on macOS and Linux,
  `%USERPROFILE%\.lmstudio\mcp.json` on Windows. LM Studio asks before each tool call.
- **Amazon Q Developer**: `steamgauge connect amazon-q`. Writes `~/.aws/amazonq/mcp.json` on macOS
  and Linux, `%USERPROFILE%\.aws\amazonq\mcp.json` on Windows.
- **JetBrains AI Assistant**:
  `{"mcpServers": {"steamgauge": {"command": "<path to steamgauge>", "args": ["mcp"]}}}`. Settings,
  Tools, AI Assistant, Model Context Protocol (MCP), Add, at the Global level.
- **Trae**: `{"mcpServers": {"steamgauge": {"command": "<path to steamgauge>", "args": ["mcp"]}}}`.
  Settings, MCP, Add, Raw Config (JSON).
- **Qoder**: `{"mcpServers": {"steamgauge": {"command": "<path to steamgauge>", "args": ["mcp"]}}}`.
  Settings, MCP, My Servers, Add. Qoder CLI is added by `steamgauge connect qoder`.
- **Command Code**: `cmd mcp add --scope user steamgauge -- "<path to steamgauge>" mcp`. In a
  terminal.
- **fast-agent**: In the `fast-agent.yaml` of the folder it runs in, and in the agent's `servers`.

  ```yaml
  mcp:
    servers:
      steamgauge:
        command: <path to steamgauge>
        args: [mcp]
  ```

- **Something else**: `"<path to steamgauge>" mcp`. Any assistant that starts an MCP server from a
  command. `steamgauge clients` prints this copy's path.

<!-- steamgauge-site: end -->

`steamgauge clients` prints `<path to steamgauge>`, and [the guide](agent.md) lists where each
installer puts it.

## What it writes

One entry, named `steamgauge`, in the assistant's own settings file. Everything else in the file
stays as it was, comments included. `steamgauge disconnect <name>` takes it out again.

<!-- steamgauge-site: entries -->

Claude Desktop, Devin Desktop and Devin CLI, Windsurf, Gemini CLI, Kiro, Cline, Roo Code, Junie,
Qoder CLI, Warp, Tabnine CLI, Tabnine in the editor, pi, Deep Code, LM Studio and Amazon Q
Developer:

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

VS Code:

```json
{
  "servers": {
    "steamgauge": {
      "type": "stdio",
      "command": "<path to steamgauge>",
      "args": ["mcp"]
    }
  }
}
```

GitHub Copilot CLI:

```json
{
  "mcpServers": {
    "steamgauge": {
      "type": "local",
      "command": "<path to steamgauge>",
      "args": ["mcp"],
      "tools": ["*"]
    }
  }
}
```

Cursor, Factory Droid and Firebender:

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

opencode and Kilo Code:

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

Amp:

```json
{
  "amp.mcpServers": {
    "steamgauge": {
      "command": "<path to steamgauge>",
      "args": ["mcp"]
    }
  }
}
```

Mistral Vibe:

```toml
[[mcp_servers]]
name = "steamgauge"
transport = "stdio"
command = "<path to steamgauge>"
args = ["mcp"]
```

VT Code:

```toml
[mcp]
enabled = true

[[mcp.providers]]
name = "steamgauge"
enabled = true
command = "<path to steamgauge>"
args = ["mcp"]
```

ZeroClaw:

```toml
[[mcp.servers]]
name = "steamgauge"
command = "<path to steamgauge>"
args = ["mcp"]
```

Goose:

```yaml
extensions: {steamgauge: {name: steamgauge, type: stdio, cmd: '<path to steamgauge>', args: [mcp], enabled: true, timeout: 300}}
```

Hermes Agent:

```yaml
mcp_servers: {steamgauge: {command: '<path to steamgauge>', args: [mcp]}}
```

Continue:

```yaml
mcpServers: [{name: steamgauge, type: stdio, command: '<path to steamgauge>', args: [mcp]}]
```

OpenClaw:

```json
{
  "mcp": {
    "servers": {
      "steamgauge": {
        "command": "<path to steamgauge>",
        "args": ["mcp"]
      }
    }
  }
}
```

nanobot:

```json
{
  "tools": {
    "mcpServers": {
      "steamgauge": {
        "command": "<path to steamgauge>",
        "args": ["mcp"]
      }
    }
  }
}
```

Autohand:

```json
{
  "mcp": {
    "servers": [
      {
        "name": "steamgauge",
        "transport": "stdio",
        "command": "<path to steamgauge>",
        "args": ["mcp"]
      }
    ]
  }
}
```

<!-- steamgauge-site: end -->

## How it connects

- **Only while the window is open.** The assistant reaches the open app over a local socket that
  no other user or machine can reach. While the window is closed, every tool says so.
- **Through updates.** An update leaves every assistant connected. Until you restart one, it
  offers the tools of the version it started with.
- **HTTP, if you need it.** The AI assistants page turns it on. The app then also answers on
  127.0.0.1 at a port you choose, to requests carrying the token that page shows.
