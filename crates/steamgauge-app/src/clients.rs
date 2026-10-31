//! Adding the app to the MCP clients a person runs.
//!
//! A client starts a server it is told about in its own configuration, and every client keeps
//! that in a file of its own: the same entry, a command and its arguments, under a different key
//! and in a different shape. So the clients are rows of a table rather than an integration each,
//! and each row's file, key and shape is the one that client's own documentation gives, or its
//! source where the documentation names the file and not its layout. A client whose user-wide
//! configuration is documented nowhere is not in the table.
//!
//! Adding changes the app's own entry and nothing else: every other server, every comment and the
//! file's layout stay as they were, a file that does not parse is refused rather than replaced,
//! and the file is written beside itself and renamed into place, so a crash leaves the old one.
//! Before anything is written, the old and the new text are both read as plain values by a strict
//! parser, and the new has to be the old with the app's entry and nothing more. Anything else is
//! refused. A client that rewrites its own file while it runs is added through its own command
//! instead, and the file is read back to confirm it.

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// The name the app is entered under in every client's configuration.
pub const SERVER: &str = "steamgauge";

/// What a person pastes to any assistant, this table's or not, to have it add the app to itself
/// by the guide the documentation site publishes.
pub const PASTE: &str = concat!(
    "Add SteamGauge to yourself by following ",
    env!("CARGO_PKG_HOMEPAGE"),
    "agent.md,\nthen tell me which file you changed."
);

/// Which system the folders are a person's on, where clients keep their files in different places.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Windows,
    MacOs,
    Linux,
}

impl System {
    /// The system this program runs on.
    #[must_use]
    pub const fn this() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The folders a client's configuration is found from, so a test can stand in for the person's.
#[derive(Debug, Clone)]
pub struct Folders {
    /// The person's home folder.
    pub home: PathBuf,
    /// Where programs keep configuration: `%APPDATA%` on Windows, `~/Library/Application Support`
    /// on macOS, `~/.config` (or `$XDG_CONFIG_HOME`) on Linux.
    pub config: PathBuf,
    /// Where programs keep data of this computer alone: `%LOCALAPPDATA%` on Windows.
    pub local: PathBuf,
    /// The environment variable a client reads its folder from instead, where it is set.
    pub variable: fn(&str) -> Option<PathBuf>,
    /// Where a program the person runs by name is, for a client added through its own command.
    pub program: fn(&str) -> Option<PathBuf>,
    pub system: System,
}

impl Folders {
    /// This person's folders.
    ///
    /// # Errors
    ///
    /// Fails where the system names no home or configuration folder.
    pub fn of_this_user() -> Result<Self> {
        Ok(Self {
            home: dirs::home_dir().context("this system names no home folder")?,
            config: dirs::config_dir().context("this system names no configuration folder")?,
            local: dirs::data_local_dir().context("this system names no local data folder")?,
            variable: |name| {
                std::env::var_os(name)
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
            },
            program: on_the_search_path,
            system: System::this(),
        })
    }

    /// `name`'s folder where it is set, `fallback` otherwise.
    fn or(&self, name: &str, fallback: PathBuf) -> PathBuf {
        (self.variable)(name).unwrap_or(fallback)
    }

    /// `~/.config`, or `$XDG_CONFIG_HOME`, on every system: where the clients that follow the
    /// XDG layout keep their files, Windows and macOS included.
    fn xdg_config(&self) -> PathBuf {
        self.or("XDG_CONFIG_HOME", self.home.join(".config"))
    }

    /// `~/.config` itself, for the clients that name it whatever `XDG_CONFIG_HOME` says.
    fn dot_config(&self) -> PathBuf {
        self.home.join(".config")
    }

    /// VS Code's folder for the person's own settings.
    fn vs_code(&self) -> PathBuf {
        self.config.join("Code").join("User")
    }
}

/// Where `name` is on the search path, as a shell would find it, or in `~/.local/bin`, where
/// installers that leave the search path alone put their programs.
fn on_the_search_path(name: &str) -> Option<PathBuf> {
    let endings: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_owned())
            .split(';')
            .filter(|ending| !ending.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        vec![String::new()]
    };
    let folders = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(dirs::home_dir().map(|home| home.join(".local").join("bin")));
    for folder in folders.filter(|folder| folder.is_absolute()) {
        for ending in &endings {
            let candidate = folder.join(format!("{name}{ending}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Every client the app can add itself to, each as its own documentation gives its user-wide
/// configuration. `docs/assistants.md` gives the rest, the ones that keep their servers where no
/// documentation says, the command or the steps instead.
pub const CLIENTS: &[Client] = &[
    Client {
        id: "claude-code",
        name: "Claude Code",
        file: |folders| folders.home.join(".claude.json"),
        format: JSON_MCP_SERVERS,
        by: By::Running {
            program: "claude",
            add: &["mcp", "add", "--scope", "user", SERVER, "--"],
            remove: &["mcp", "remove", "--scope", "user", SERVER],
        },
        installed: |folders| vec![folders.home.join(".claude")],
        then: Some("Claude Code starts it in its next session."),
    },
    Client {
        id: "claude-desktop",
        name: "Claude Desktop",
        file: |folders| {
            folders
                .config
                .join("Claude")
                .join("claude_desktop_config.json")
        },
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.config.join("Claude")],
        then: Some("Quit Claude Desktop completely and open it again."),
    },
    Client {
        id: "vscode",
        name: "VS Code",
        file: |folders| folders.vs_code().join("mcp.json"),
        format: Format::json(&["servers"], Shape::Typed),
        by: By::Editing,
        installed: |folders| vec![folders.config.join("Code")],
        then: Some("VS Code asks once whether to trust it, the first time it starts."),
    },
    Client {
        id: "copilot",
        name: "GitHub Copilot CLI",
        file: |folders| copilot_folder(folders).join("mcp-config.json"),
        format: Format::json(&["mcpServers"], Shape::Copilot),
        by: By::Editing,
        installed: |folders| vec![copilot_folder(folders)],
        then: None,
    },
    Client {
        id: "cursor",
        name: "Cursor",
        file: |folders| folders.home.join(".cursor").join("mcp.json"),
        format: Format::json(&["mcpServers"], Shape::Typed),
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".cursor")],
        then: None,
    },
    Client {
        id: "devin",
        name: "Devin Desktop and Devin CLI",
        file: |folders| devin_folder(folders).join("mcp_config.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![devin_folder(folders), folders.home.join(".devin")],
        then: None,
    },
    Client {
        id: "windsurf",
        name: "Windsurf",
        file: |folders| windsurf_folder(folders).join("mcp_config.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![windsurf_folder(folders)],
        then: Some("Refresh the servers in Cascade's MCP panel."),
    },
    Client {
        id: "codex",
        name: "Codex",
        file: |folders| codex_folder(folders).join("config.toml"),
        format: Format::toml(&["mcp_servers"], Holds::Map, Shape::Plain),
        by: By::Editing,
        installed: |folders| vec![codex_folder(folders)],
        then: None,
    },
    Client {
        id: "gemini",
        name: "Gemini CLI",
        file: |folders| gemini_folder(folders).join("settings.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![gemini_folder(folders)],
        then: None,
    },
    Client {
        id: "kiro",
        name: "Kiro",
        file: |folders| kiro_folder(folders).join("settings").join("mcp.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![kiro_folder(folders)],
        then: None,
    },
    Client {
        id: "zed",
        name: "Zed",
        file: |folders| zed_folder(folders).join("settings.json"),
        format: Format::json(&["context_servers"], Shape::Plain),
        by: By::Editing,
        installed: |folders| vec![zed_folder(folders)],
        then: None,
    },
    Client {
        id: "opencode",
        name: "opencode",
        file: |folders| {
            folders.or(
                "OPENCODE_CONFIG",
                folders.xdg_config().join("opencode").join("opencode.json"),
            )
        },
        format: Format::json(&["mcp"], Shape::Local),
        by: By::Editing,
        installed: |folders| vec![folders.xdg_config().join("opencode")],
        then: None,
    },
    Client {
        id: "kilo",
        name: "Kilo Code",
        file: |folders| kilo_folder(folders).join("kilo.jsonc"),
        format: Format::json(&["mcp"], Shape::Local),
        by: By::Editing,
        installed: |folders| vec![kilo_folder(folders)],
        then: None,
    },
    Client {
        id: "cline",
        name: "Cline",
        file: |folders| {
            folders.or(
                "CLINE_MCP_SETTINGS_PATH",
                folders
                    .home
                    .join(".cline")
                    .join("data")
                    .join("settings")
                    .join("cline_mcp_settings.json"),
            )
        },
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".cline")],
        then: None,
    },
    Client {
        id: "roo",
        name: "Roo Code",
        file: |folders| {
            roo_folder(folders)
                .join("settings")
                .join("mcp_settings.json")
        },
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![roo_folder(folders)],
        then: None,
    },
    Client {
        id: "junie",
        name: "Junie",
        file: |folders| folders.home.join(".junie").join("mcp").join("mcp.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".junie")],
        then: None,
    },
    Client {
        id: "qoder",
        name: "Qoder CLI",
        file: |folders| qoder_folder(folders).join("settings.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![qoder_folder(folders)],
        then: Some("Restart Qoder CLI, or run /mcp reload in it."),
    },
    Client {
        id: "amp",
        name: "Amp",
        file: |folders| folders.dot_config().join("amp").join("settings.json"),
        format: Format::json(&["amp.mcpServers"], Shape::Plain),
        by: By::Editing,
        installed: |folders| vec![folders.dot_config().join("amp")],
        then: None,
    },
    Client {
        id: "warp",
        name: "Warp",
        file: |folders| folders.home.join(".warp").join(".mcp.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".warp")],
        then: None,
    },
    Client {
        id: "factory",
        name: "Factory Droid",
        file: |folders| folders.home.join(".factory").join("mcp.json"),
        format: Format::json(&["mcpServers"], Shape::Typed),
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".factory")],
        then: None,
    },
    Client {
        id: "tabnine",
        name: "Tabnine CLI",
        file: |folders| {
            folders
                .home
                .join(".tabnine")
                .join("agent")
                .join("settings.json")
        },
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".tabnine").join("agent")],
        then: Some("Run /mcp reload in a running session."),
    },
    Client {
        id: "tabnine-ide",
        name: "Tabnine in the editor",
        file: |folders| folders.home.join(".tabnine").join("mcp_servers.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".tabnine")],
        then: Some("Tabnine reads it while a project folder is open."),
    },
    Client {
        id: "firebender",
        name: "Firebender",
        file: |folders| folders.home.join(".firebender").join("firebender.json"),
        format: Format::json(&["mcpServers"], Shape::Typed),
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".firebender")],
        then: None,
    },
    Client {
        id: "pi",
        name: "pi",
        file: |folders| pi_folder(folders).join("mcp.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![pi_folder(folders)],
        then: Some("Run /reload in a running session."),
    },
    Client {
        id: "vibe",
        name: "Mistral Vibe",
        file: |folders| vibe_folder(folders).join("config.toml"),
        format: Format::toml(&["mcp_servers"], Holds::List, Shape::Transport),
        by: By::Editing,
        installed: |folders| vec![vibe_folder(folders)],
        then: Some("A project's own .vibe/config.toml is read in place of this one."),
    },
    Client {
        id: "vtcode",
        name: "VT Code",
        file: |folders| vtcode_folder(folders).join("vtcode.toml"),
        format: Format {
            turns_on: Some(&["mcp", "enabled"]),
            ..Format::toml(&["mcp", "providers"], Holds::List, Shape::Enabled)
        },
        by: By::Editing,
        installed: |folders| vec![vtcode_folder(folders)],
        then: Some("This also switches on VT Code's MCP support, which is off until set."),
    },
    Client {
        id: "zeroclaw",
        name: "ZeroClaw",
        file: |folders| zeroclaw_folder(folders).join("config.toml"),
        format: Format::toml(&["mcp", "servers"], Holds::List, Shape::Plain),
        by: By::Editing,
        installed: |folders| vec![zeroclaw_folder(folders)],
        then: Some(
            "An agent uses it once one of its mcp_bundles names steamgauge, as ZeroClaw's MCP guide shows.",
        ),
    },
    Client {
        id: "goose",
        name: "Goose",
        file: |folders| goose_folder(folders).join("config.yaml"),
        format: Format::yaml(&["extensions"], Holds::Map, Shape::Goose),
        by: By::Editing,
        installed: |folders| vec![goose_folder(folders)],
        then: Some("Goose loads it in its next session."),
    },
    Client {
        id: "hermes",
        name: "Hermes Agent",
        file: |folders| hermes_folder(folders).join("config.yaml"),
        format: Format::yaml(&["mcp_servers"], Holds::Map, Shape::Plain),
        by: By::Editing,
        installed: |folders| vec![hermes_folder(folders)],
        then: None,
    },
    Client {
        id: "continue",
        name: "Continue",
        file: |folders| continue_folder(folders).join("config.yaml"),
        format: Format::yaml(&["mcpServers"], Holds::List, Shape::Typed),
        by: By::Editing,
        installed: |folders| vec![continue_folder(folders)],
        then: Some("Continue uses it in agent mode, with Local Config chosen."),
    },
    Client {
        id: "openclaw",
        name: "OpenClaw",
        file: |folders| {
            folders.or(
                "OPENCLAW_CONFIG_PATH",
                folders.home.join(".openclaw").join("openclaw.json"),
            )
        },
        format: Format::json(&["mcp", "servers"], Shape::Plain),
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".openclaw")],
        then: None,
    },
    Client {
        id: "nanobot",
        name: "nanobot",
        file: |folders| folders.home.join(".nanobot").join("config.json"),
        format: Format::json(&["tools", "mcpServers"], Shape::Plain),
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".nanobot")],
        then: None,
    },
    Client {
        id: "autohand",
        name: "Autohand",
        file: |folders| autohand_folder(folders).join("config.json"),
        format: Format {
            holds: Holds::List,
            ..Format::json(&["mcp", "servers"], Shape::Transport)
        },
        by: By::Editing,
        installed: |folders| vec![autohand_folder(folders)],
        then: Some("Restart Autohand."),
    },
    Client {
        id: "deepcode",
        name: "Deep Code",
        file: |folders| folders.home.join(".deepcode").join("settings.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        // Another program keeps its own files in the same folder, so only this one's file says
        // Deep Code is here.
        installed: |_| Vec::new(),
        then: None,
    },
    Client {
        id: "lm-studio",
        name: "LM Studio",
        file: |folders| folders.home.join(".lmstudio").join("mcp.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".lmstudio")],
        then: Some("LM Studio asks before each tool call."),
    },
    Client {
        id: "amazon-q",
        name: "Amazon Q Developer",
        // The file its MCP panel writes, `default.json`, is an agent whose tools must also name a
        // server. This one, which the IDE plugins read unless `useLegacyMcpJson` is turned off,
        // takes the entry alone.
        file: |folders| folders.home.join(".aws").join("amazonq").join("mcp.json"),
        format: JSON_MCP_SERVERS,
        by: By::Editing,
        installed: |folders| vec![folders.home.join(".aws").join("amazonq")],
        then: None,
    },
];

const JSON_MCP_SERVERS: Format = Format::json(&["mcpServers"], Shape::Plain);

fn copilot_folder(folders: &Folders) -> PathBuf {
    folders.or("COPILOT_HOME", folders.home.join(".copilot"))
}

/// Devin Desktop's folder: `%APPDATA%\devin` on Windows, the XDG one elsewhere, macOS included.
fn devin_folder(folders: &Folders) -> PathBuf {
    if folders.system == System::Windows {
        folders.config.join("devin")
    } else {
        folders.xdg_config().join("devin")
    }
}

/// Windsurf's folder before it became Devin Desktop, which a copy not yet renamed still reads.
fn windsurf_folder(folders: &Folders) -> PathBuf {
    folders.home.join(".codeium").join("windsurf")
}

fn codex_folder(folders: &Folders) -> PathBuf {
    folders.or("CODEX_HOME", folders.home.join(".codex"))
}

fn gemini_folder(folders: &Folders) -> PathBuf {
    folders
        .or("GEMINI_CLI_HOME", folders.home.clone())
        .join(".gemini")
}

fn kiro_folder(folders: &Folders) -> PathBuf {
    folders.or("KIRO_HOME", folders.home.join(".kiro"))
}

/// Zed's folder: `%APPDATA%\Zed` on Windows, the XDG one elsewhere, macOS included.
fn zed_folder(folders: &Folders) -> PathBuf {
    if folders.system == System::Windows {
        folders.config.join("Zed")
    } else {
        folders.xdg_config().join("zed")
    }
}

fn kilo_folder(folders: &Folders) -> PathBuf {
    folders.or("KILO_CONFIG_DIR", folders.xdg_config().join("kilo"))
}

/// Roo Code's folder in VS Code's storage for extensions.
fn roo_folder(folders: &Folders) -> PathBuf {
    folders
        .vs_code()
        .join("globalStorage")
        .join("rooveterinaryinc.roo-cline")
}

fn qoder_folder(folders: &Folders) -> PathBuf {
    folders.or("QODER_CONFIG_DIR", folders.home.join(".qoder"))
}

fn pi_folder(folders: &Folders) -> PathBuf {
    folders.or(
        "PI_CODING_AGENT_DIR",
        folders.home.join(".pi").join("agent"),
    )
}

fn vibe_folder(folders: &Folders) -> PathBuf {
    folders.or("VIBE_HOME", folders.home.join(".vibe"))
}

/// VT Code's folder, which differs on every system.
fn vtcode_folder(folders: &Folders) -> PathBuf {
    let own = match folders.system {
        System::Windows => folders.config.join("vinhnx").join("vtcode").join("config"),
        System::MacOs => folders.config.join("com.vinhnx.vtcode"),
        System::Linux => folders.xdg_config().join("vtcode"),
    };
    folders.or("VTCODE_CONFIG", own)
}

fn zeroclaw_folder(folders: &Folders) -> PathBuf {
    folders.or("ZEROCLAW_CONFIG_DIR", folders.home.join(".zeroclaw"))
}

/// Goose's folder: `%APPDATA%\Block\goose\config` on Windows, the XDG one elsewhere.
fn goose_folder(folders: &Folders) -> PathBuf {
    if let Some(root) = (folders.variable)("GOOSE_PATH_ROOT") {
        return root.join("config");
    }
    if folders.system == System::Windows {
        folders.config.join("Block").join("goose").join("config")
    } else {
        folders.xdg_config().join("goose")
    }
}

/// Hermes Agent's folder: `%LOCALAPPDATA%\hermes` on Windows, `~/.hermes` elsewhere.
fn hermes_folder(folders: &Folders) -> PathBuf {
    let own = if folders.system == System::Windows {
        folders.local.join("hermes")
    } else {
        folders.home.join(".hermes")
    };
    folders.or("HERMES_HOME", own)
}

fn continue_folder(folders: &Folders) -> PathBuf {
    folders.or("CONTINUE_GLOBAL_DIR", folders.home.join(".continue"))
}

fn autohand_folder(folders: &Folders) -> PathBuf {
    folders.or("AUTOHAND_HOME", folders.home.join(".autohand"))
}

/// The language a client's configuration is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// JSON, comments and trailing commas allowed, and JSON5's unquoted keys.
    Json,
    Toml,
    Yaml,
}

impl Language {
    /// What a code block of it is marked as.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
        }
    }
}

/// How a client keeps its servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holds {
    /// An object keyed by each server's name.
    Map,
    /// A list of servers, each carrying its `name`.
    List,
}

/// How a client writes one stdio server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `command` and `args`.
    Plain,
    /// The same with `type: stdio`.
    Typed,
    /// `type: local`, `command` holding the program and its arguments, and `enabled: true`.
    Local,
    /// `type: local`, `command`, `args`, and `tools: ["*"]`, every tool the server offers.
    Copilot,
    /// `transport: stdio`, `command` and `args`.
    Transport,
    /// `enabled: true`, `command` and `args`.
    Enabled,
    /// Goose's extension: `name`, `type: stdio`, `cmd`, `args`, `enabled: true` and a `timeout`.
    Goose,
}

impl Shape {
    /// The field holding the program.
    const fn program_field(self) -> &'static str {
        match self {
            Self::Goose => "cmd",
            _ => "command",
        }
    }
}

/// Where in a client's file its servers are kept, and in what form.
#[derive(Debug, Clone, Copy)]
pub struct Format {
    pub language: Language,
    /// The keys from the root to the servers, each one key: `["tools", "mcpServers"]`.
    pub container: &'static [&'static str],
    pub holds: Holds,
    pub shape: Shape,
    /// A switch the servers need turned on, set to true beside the entry and left on its removal.
    pub turns_on: Option<&'static [&'static str]>,
}

impl Format {
    const fn json(container: &'static [&'static str], shape: Shape) -> Self {
        Self {
            language: Language::Json,
            container,
            holds: Holds::Map,
            shape,
            turns_on: None,
        }
    }

    const fn toml(container: &'static [&'static str], holds: Holds, shape: Shape) -> Self {
        Self {
            language: Language::Toml,
            container,
            holds,
            shape,
            turns_on: None,
        }
    }

    const fn yaml(container: &'static [&'static str], holds: Holds, shape: Shape) -> Self {
        Self {
            language: Language::Yaml,
            container,
            holds,
            shape,
            turns_on: None,
        }
    }
}

/// How the app's entry is put in a client's file.
#[derive(Debug, Clone, Copy)]
pub enum By {
    /// The app edits the file itself.
    Editing,
    /// The client rewrites its own file while it runs, so its own command adds the entry: the
    /// program, then `add` followed by the launch, or `remove`.
    Running {
        program: &'static str,
        add: &'static [&'static str],
        remove: &'static [&'static str],
    },
}

/// One MCP client.
#[derive(Debug, Clone, Copy)]
pub struct Client {
    /// What the client is called on the command line: `steamgauge connect cursor`.
    pub id: &'static str,
    /// What the window calls it.
    pub name: &'static str,
    /// Its user-wide configuration file.
    pub file: fn(&Folders) -> PathBuf,
    pub format: Format,
    pub by: By,
    /// Folders any one of which shows the client is on this computer, when its configuration
    /// file is not there yet.
    pub installed: fn(&Folders) -> Vec<PathBuf>,
    /// What the person still has to do once the file is written, if anything.
    pub then: Option<&'static str>,
}

/// The client a person names on the command line.
///
/// # Errors
///
/// Fails, listing every client, where none is called `id`.
pub fn by_id(id: &str) -> Result<&'static Client> {
    CLIENTS
        .iter()
        .find(|client| client.id == id)
        .with_context(|| {
            let known: Vec<&str> = CLIENTS.iter().map(|client| client.id).collect();
            format!(
                "SteamGauge knows no MCP client called {id}; it knows {}",
                known.join(", ")
            )
        })
}

/// Where a client stands with this copy of the app.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum State {
    /// Nothing shows the client is on this computer.
    Absent,
    /// The client is here and its configuration has no entry for the app.
    Installed,
    /// The client starts this copy of the app.
    Added,
    /// The client has an entry for the app that starts another command, such as a copy that has
    /// since moved.
    Elsewhere { command: String },
    /// The client's configuration does not parse, so nothing here will write to it.
    Unreadable { reason: String },
}

/// What the client is told to run: this copy's own path, which no installer puts on the search
/// path everywhere, and `mcp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

impl Launch {
    /// The command for this copy of the app.
    ///
    /// # Errors
    ///
    /// Fails if the system cannot say where this program is.
    pub fn this_copy() -> Result<Self> {
        let program = steady_path(&std::env::current_exe().context("where this program is")?);
        Ok(Self {
            program: program.display().to_string(),
            args: vec!["mcp".to_owned()],
        })
    }

    /// The command as a person types it into a client that takes one line.
    #[must_use]
    pub fn line(&self) -> String {
        std::iter::once(format!("\"{}\"", self.program))
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The path to this program that survives its own updates.
///
/// Scoop installs each version in a folder of its own and points `current` at the newest, so the
/// versioned path would start a copy the next update removes. Every other way the app is
/// installed keeps one path across updates.
fn steady_path(program: &Path) -> PathBuf {
    let (Some(version), Some(name)) = (program.parent(), program.file_name()) else {
        return program.to_path_buf();
    };
    let Some(app) = version.parent() else {
        return program.to_path_buf();
    };
    let current = app.join("current").join(name);
    let scoop = app
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|apps| apps.eq_ignore_ascii_case("apps"));
    if scoop && version.file_name() != Some("current".as_ref()) && current.is_file() {
        current
    } else {
        program.to_path_buf()
    }
}

/// Where `client` stands with `launch`.
#[must_use]
pub fn state(client: &Client, folders: &Folders, launch: &Launch) -> State {
    let file = (client.file)(folders);
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(_)
            if (client.installed)(folders)
                .iter()
                .any(|folder| folder.is_dir()) =>
        {
            return State::Installed;
        }
        Err(_) => return State::Absent,
    };
    match entry_command(client.format, &text) {
        Err(reason) => State::Unreadable { reason },
        Ok(None) => State::Installed,
        Ok(Some(command)) if command == launch.program => State::Added,
        Ok(Some(command)) => State::Elsewhere { command },
    }
}

/// What adding the app to a client's file did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Added {
    /// The entry was written where there was none.
    Written(PathBuf),
    /// An entry that started something else was replaced.
    Replaced { file: PathBuf, was: String },
    /// The entry was already this copy's, and nothing was written.
    Already(PathBuf),
}

/// What taking the app out of a client's file did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// The entry was taken out.
    Taken(PathBuf),
    /// There was no entry to take out, and nothing was written.
    NoEntry(PathBuf),
}

fn read_or_empty(file: &Path) -> Result<String> {
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).with_context(|| format!("could not read {}", file.display())),
    }
}

/// Adds the app to `client`, or points an existing entry at `launch`.
///
/// # Errors
///
/// Fails if the file does not parse, cannot be read or written, or would change in any other way,
/// and for a client added through its own command, where the command is not here or fails.
pub fn add(client: &Client, folders: &Folders, launch: &Launch) -> Result<Added> {
    let file = (client.file)(folders);
    let text = read_or_empty(&file)?;
    let was = entry_command(client.format, &text).map_err(|reason| refused(&file, &reason))?;
    if was.as_deref() == Some(launch.program.as_str()) {
        return Ok(Added::Already(file));
    }
    match client.by {
        By::Editing => write_in_place(&file, &with_entry(client, &text, launch)?)?,
        By::Running {
            program,
            add,
            remove,
        } => {
            let found = (folders.program)(program).with_context(|| {
                format!(
                    "{}'s `{program}` command is not on this computer's search path, so \
                     SteamGauge could not add itself; run `{program} {} {}` in a terminal",
                    client.name,
                    add.join(" "),
                    launch.line()
                )
            })?;
            if was.is_some() {
                run(&found, remove.iter().map(|part| (*part).to_owned()))?;
            }
            run(
                &found,
                add.iter()
                    .map(|part| (*part).to_owned())
                    .chain(std::iter::once(launch.program.clone()))
                    .chain(launch.args.iter().cloned()),
            )?;
            let now = entry_command(client.format, &read_or_empty(&file)?)
                .map_err(|reason| refused(&file, &reason))?;
            if now.as_deref() != Some(launch.program.as_str()) {
                bail!(
                    "{} said it added SteamGauge, but {} does not name this copy",
                    client.name,
                    file.display()
                );
            }
        }
    }
    Ok(match was {
        None => Added::Written(file),
        Some(was) => Added::Replaced { file, was },
    })
}

/// Runs a client's own command and fails with what it printed where it fails.
fn run(program: &Path, args: impl Iterator<Item = String>) -> Result<()> {
    let output = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .with_context(|| format!("could not run {}", program.display()))?;
    if !output.status.success() {
        let said = String::from_utf8_lossy(&output.stderr);
        let said = if said.trim().is_empty() {
            String::from_utf8_lossy(&output.stdout)
        } else {
            said
        };
        bail!("{} failed: {}", program.display(), said.trim());
    }
    Ok(())
}

/// `text`, a client's configuration, with the app's entry in it.
///
/// # Errors
///
/// Fails if `text` does not parse, or the entry cannot be written without changing anything else.
pub fn with_entry(client: &Client, text: &str, launch: &Launch) -> Result<String> {
    let format = client.format;
    let entry = entry(format, launch);
    let written = match format.language {
        Language::Json => json_with(text, format, &entry)?,
        Language::Toml => toml_with(text, format, &entry)?,
        Language::Yaml => yaml_with(text, format, &entry)?,
    };
    let before = value_of(format.language, text).map_err(|reason| anyhow::anyhow!(reason))?;
    check(format, &written, &expected_with(format, before, &entry))?;
    Ok(written)
}

/// Takes the app's entry out of `client` and leaves everything else as it was.
///
/// # Errors
///
/// Fails if the file does not parse, cannot be read or written, or would change in any other way,
/// and for a client added through its own command, where the command is not here or fails.
pub fn remove(client: &Client, folders: &Folders) -> Result<Removed> {
    let file = (client.file)(folders);
    let Ok(text) = std::fs::read_to_string(&file) else {
        return Ok(Removed::NoEntry(file));
    };
    if entry_command(client.format, &text)
        .map_err(|reason| refused(&file, &reason))?
        .is_none()
    {
        return Ok(Removed::NoEntry(file));
    }
    match client.by {
        By::Editing => {
            let format = client.format;
            let written = match format.language {
                Language::Json => json_without(&text, format)?,
                Language::Toml => toml_without(&text, format)?,
                Language::Yaml => yaml_without(&text, format)?,
            };
            let before =
                value_of(format.language, &text).map_err(|reason| anyhow::anyhow!(reason))?;
            check(format, &written, &expected_without(format, before))?;
            write_in_place(&file, &written)?;
        }
        By::Running {
            program, remove, ..
        } => {
            let found = (folders.program)(program).with_context(|| {
                format!(
                    "{}'s `{program}` command is not on this computer's search path; run \
                     `{program} {}` in a terminal",
                    client.name,
                    remove.join(" ")
                )
            })?;
            run(&found, remove.iter().map(|part| (*part).to_owned()))?;
        }
    }
    Ok(Removed::Taken(file))
}

fn refused(file: &Path, reason: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "{} does not parse ({reason}), so SteamGauge was not added to it; fix the file, or add \
         the entry by hand",
        file.display()
    )
}

/// The new text read back must be what the old one held, changed by the entry alone.
fn check(format: Format, written: &str, expected: &Value) -> Result<()> {
    let got = value_of(format.language, written).map_err(|reason| anyhow::anyhow!(reason))?;
    if got != *expected {
        bail!(
            "writing SteamGauge's entry would have changed something else in the file, so nothing \
             was written; add the entry by hand"
        );
    }
    Ok(())
}

/// One field of the app's entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Field {
    Text(String),
    Texts(Vec<String>),
    True,
    Number(i64),
}

/// The app's entry, field by field in the order it is written.
type Entry = Vec<(&'static str, Field)>;

fn entry(format: Format, launch: &Launch) -> Entry {
    let program = || Field::Text(launch.program.clone());
    let args = || Field::Texts(launch.args.clone());
    let text = |value: &str| Field::Text(value.to_owned());
    let mut fields = match format.shape {
        Shape::Plain => vec![("command", program()), ("args", args())],
        Shape::Typed => vec![
            ("type", text("stdio")),
            ("command", program()),
            ("args", args()),
        ],
        Shape::Local => vec![
            ("type", text("local")),
            (
                "command",
                Field::Texts(
                    std::iter::once(launch.program.clone())
                        .chain(launch.args.iter().cloned())
                        .collect(),
                ),
            ),
            ("enabled", Field::True),
        ],
        Shape::Copilot => vec![
            ("type", text("local")),
            ("command", program()),
            ("args", args()),
            ("tools", Field::Texts(vec!["*".to_owned()])),
        ],
        Shape::Transport => vec![
            ("transport", text("stdio")),
            ("command", program()),
            ("args", args()),
        ],
        Shape::Enabled => vec![
            ("enabled", Field::True),
            ("command", program()),
            ("args", args()),
        ],
        Shape::Goose => vec![
            ("name", text(SERVER)),
            ("type", text("stdio")),
            ("cmd", program()),
            ("args", args()),
            ("enabled", Field::True),
            ("timeout", Field::Number(300)),
        ],
    };
    if format.holds == Holds::List && format.shape != Shape::Goose {
        fields.insert(0, ("name", text(SERVER)));
    }
    fields
}

fn field_value(field: &Field) -> Value {
    match field {
        Field::Text(text) => Value::from(text.as_str()),
        Field::Texts(texts) => Value::from(texts.clone()),
        Field::True => Value::Bool(true),
        Field::Number(number) => Value::from(*number),
    }
}

fn entry_value(entry: &Entry) -> Value {
    Value::Object(
        entry
            .iter()
            .map(|(key, field)| ((*key).to_owned(), field_value(field)))
            .collect(),
    )
}

/// What a client's file holds, as a strict parser reads it. An empty file holds an empty object.
fn value_of(language: Language, text: &str) -> std::result::Result<Value, String> {
    let value = match language {
        Language::Json => jsonc_parser::parse_to_serde_value::<Option<Value>>(
            text,
            &jsonc_parser::ParseOptions::default(),
        )
        .map_err(|error| error.to_string())?
        .unwrap_or(Value::Null),
        Language::Toml => {
            toml_edit::de::from_str::<Value>(text).map_err(|error| error.to_string())?
        }
        Language::Yaml => {
            serde_saphyr::from_str::<Value>(text).map_err(|error| error.to_string())?
        }
    };
    Ok(if value.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        value
    })
}

/// The object at `path` below `root`, made where it is missing.
fn made<'a>(root: &'a mut Value, path: &[&str]) -> Option<&'a mut serde_json::Map<String, Value>> {
    let mut at = root.as_object_mut()?;
    for key in path {
        at = at
            .entry((*key).to_owned())
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
            .as_object_mut()?;
    }
    Some(at)
}

/// Where in a list the app's entry is.
fn named(list: &[Value]) -> Option<usize> {
    list.iter()
        .position(|server| server.get("name").and_then(Value::as_str) == Some(SERVER))
}

fn expected_with(format: Format, mut root: Value, entry: &Entry) -> Value {
    let value = entry_value(entry);
    let Some((last, parents)) = format.container.split_last() else {
        return root;
    };
    if let Some(parent) = made(&mut root, parents) {
        match format.holds {
            Holds::Map => {
                if let Some(servers) = parent
                    .entry((*last).to_owned())
                    .or_insert_with(|| Value::Object(serde_json::Map::new()))
                    .as_object_mut()
                {
                    servers.insert(SERVER.to_owned(), value);
                }
            }
            Holds::List => {
                if let Some(list) = parent
                    .entry((*last).to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                {
                    match named(list) {
                        Some(at) => list[at] = value,
                        None => list.push(value),
                    }
                }
            }
        }
    }
    if let Some((last, parents)) = format.turns_on.and_then(<[&str]>::split_last)
        && let Some(parent) = made(&mut root, parents)
    {
        parent.insert((*last).to_owned(), Value::Bool(true));
    }
    root
}

fn expected_without(format: Format, mut root: Value) -> Value {
    let Some((last, parents)) = format.container.split_last() else {
        return root;
    };
    let mut at = root.as_object_mut();
    for key in parents {
        at = at.and_then(|object| object.get_mut(*key)?.as_object_mut());
    }
    match at.and_then(|parent| parent.get_mut(*last)) {
        Some(Value::Object(servers)) => {
            servers.remove(SERVER);
        }
        Some(Value::Array(servers)) => {
            if let Some(found) = named(servers) {
                servers.remove(found);
            }
        }
        _ => {}
    }
    root
}

/// The command the app's entry starts, if the file has one.
fn entry_command(format: Format, text: &str) -> std::result::Result<Option<String>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let root = value_of(format.language, text)?;
    let mut at = Some(&root);
    for key in format.container {
        at = at.and_then(|value| value.get(*key));
    }
    let server = match (format.holds, at) {
        (Holds::Map, Some(servers)) => servers.get(SERVER),
        (Holds::List, Some(Value::Array(list))) => named(list).map(|found| &list[found]),
        _ => None,
    };
    let Some(server) = server else {
        return Ok(None);
    };
    let command = match server.get(format.shape.program_field()) {
        Some(Value::Array(parts)) => parts.first(),
        other => other,
    };
    Ok(Some(
        command
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    ))
}

fn json_field(field: &Field) -> jsonc_parser::cst::CstInputValue {
    use jsonc_parser::cst::CstInputValue as V;
    match field {
        Field::Text(text) => V::String(text.clone()),
        Field::Texts(texts) => V::Array(texts.iter().cloned().map(V::String).collect()),
        Field::True => V::Bool(true),
        Field::Number(number) => V::Number(number.to_string()),
    }
}

fn json_entry(entry: &Entry) -> jsonc_parser::cst::CstInputValue {
    jsonc_parser::cst::CstInputValue::Object(
        entry
            .iter()
            .map(|(key, field)| ((*key).to_owned(), json_field(field)))
            .collect(),
    )
}

fn json_root(text: &str) -> Result<jsonc_parser::cst::CstRootNode> {
    let text = if text.trim().is_empty() { "{}" } else { text };
    jsonc_parser::cst::CstRootNode::parse(text, &jsonc_parser::ParseOptions::default())
        .map_err(|error| anyhow::anyhow!("{error}"))
}

/// The JSON object at `path`, made where it is missing.
fn json_object(
    root: &jsonc_parser::cst::CstRootNode,
    path: &[&str],
) -> Result<jsonc_parser::cst::CstObject> {
    let mut at = root.object_value_or_set();
    for key in path {
        at = at
            .object_value_or_create(key)
            .with_context(|| format!("its `{key}` is not an object"))?;
    }
    Ok(at)
}

/// Where in a JSON list the app's entry is.
fn json_named(list: &jsonc_parser::cst::CstArray) -> Option<(usize, jsonc_parser::cst::CstNode)> {
    list.elements().into_iter().enumerate().find(|(_, node)| {
        node.as_object()
            .and_then(|server| server.get("name"))
            .and_then(|name| name.value())
            .and_then(|name| name.as_string_lit())
            .and_then(|name| name.decoded_value().ok())
            .as_deref()
            == Some(SERVER)
    })
}

fn json_with(text: &str, format: Format, entry: &Entry) -> Result<String> {
    let root = json_root(text)?;
    let (last, parents) = format
        .container
        .split_last()
        .context("a client names where its servers are")?;
    let parent = json_object(&root, parents)?;
    let value = json_entry(entry);
    match format.holds {
        Holds::Map => {
            let servers = parent
                .object_value_or_create(last)
                .with_context(|| format!("its `{last}` is not an object"))?;
            match servers.get(SERVER) {
                Some(existing) => existing.set_value(value),
                None => {
                    servers.append(SERVER, value);
                }
            }
        }
        Holds::List => {
            let servers = parent
                .array_value_or_create(last)
                .with_context(|| format!("its `{last}` is not a list"))?;
            match json_named(&servers) {
                Some((at, old)) => {
                    servers.insert(at, value);
                    old.remove();
                }
                None => {
                    servers.append(value);
                }
            }
        }
    }
    if let Some((last, parents)) = format.turns_on.and_then(<[&str]>::split_last) {
        let parent = json_object(&root, parents)?;
        match parent.get(last) {
            Some(existing) => existing.set_value(jsonc_parser::cst::CstInputValue::Bool(true)),
            None => {
                parent.append(last, jsonc_parser::cst::CstInputValue::Bool(true));
            }
        }
    }
    Ok(with_final_newline(root.to_string()))
}

fn json_without(text: &str, format: Format) -> Result<String> {
    let root = json_root(text)?;
    let Some((last, parents)) = format.container.split_last() else {
        return Ok(text.to_owned());
    };
    let mut at = root.object_value();
    for key in parents {
        at = at.and_then(|object| object.object_value(key));
    }
    if let Some(parent) = at {
        match format.holds {
            Holds::Map => {
                if let Some(entry) = parent
                    .object_value(last)
                    .and_then(|servers| servers.get(SERVER))
                {
                    entry.remove();
                }
            }
            Holds::List => {
                if let Some((_, entry)) =
                    parent.array_value(last).and_then(|list| json_named(&list))
                {
                    entry.remove();
                }
            }
        }
    }
    Ok(with_final_newline(root.to_string()))
}

fn toml_field(field: &Field) -> toml_edit::Item {
    match field {
        Field::Text(text) => toml_edit::value(text.as_str()),
        Field::Texts(texts) => toml_edit::value(
            texts
                .iter()
                .map(String::as_str)
                .collect::<toml_edit::Array>(),
        ),
        Field::True => toml_edit::value(true),
        Field::Number(number) => toml_edit::value(*number),
    }
}

fn toml_entry(entry: &Entry) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    for (key, field) in entry {
        table.insert(key, toml_field(field));
    }
    table
}

/// The TOML table at `path`, made as an implicit one where it is missing.
fn toml_table<'a>(
    document: &'a mut toml_edit::DocumentMut,
    path: &[&str],
) -> Result<&'a mut toml_edit::Table> {
    let mut at = document.as_table_mut();
    for key in path {
        at = at
            .entry(key)
            .or_insert_with(|| {
                let mut table = toml_edit::Table::new();
                table.set_implicit(true);
                toml_edit::Item::Table(table)
            })
            .as_table_mut()
            .with_context(|| format!("its `{key}` is not a table"))?;
    }
    Ok(at)
}

fn toml_named(list: &toml_edit::ArrayOfTables) -> Option<usize> {
    list.iter()
        .position(|server| server.get("name").and_then(toml_edit::Item::as_str) == Some(SERVER))
}

fn toml_with(text: &str, format: Format, entry: &Entry) -> Result<String> {
    let mut document = text.parse::<toml_edit::DocumentMut>()?;
    let (last, parents) = format
        .container
        .split_last()
        .context("a client names where its servers are")?;
    let parent = toml_table(&mut document, parents)?;
    match format.holds {
        Holds::Map => {
            let servers = parent
                .entry(last)
                .or_insert_with(|| {
                    let mut table = toml_edit::Table::new();
                    table.set_implicit(true);
                    toml_edit::Item::Table(table)
                })
                .as_table_mut()
                .with_context(|| format!("its `{last}` is not a table"))?;
            servers.insert(SERVER, toml_edit::Item::Table(toml_entry(entry)));
        }
        Holds::List => {
            let servers = parent
                .entry(last)
                .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
                .as_array_of_tables_mut()
                .with_context(|| format!("its `{last}` is not a list of tables"))?;
            match toml_named(servers).and_then(|at| servers.get_mut(at)) {
                Some(existing) => *existing = toml_entry(entry),
                None => servers.push(toml_entry(entry)),
            }
        }
    }
    if let Some((last, parents)) = format.turns_on.and_then(<[&str]>::split_last) {
        toml_table(&mut document, parents)?.insert(last, toml_edit::value(true));
    }
    Ok(document.to_string())
}

fn toml_without(text: &str, format: Format) -> Result<String> {
    let mut document = text.parse::<toml_edit::DocumentMut>()?;
    let Some((last, parents)) = format.container.split_last() else {
        return Ok(text.to_owned());
    };
    let mut at = Some(document.as_table_mut());
    for key in parents {
        at = at.and_then(|table| table.get_mut(key)?.as_table_mut());
    }
    if let Some(parent) = at {
        match parent.get_mut(last) {
            Some(toml_edit::Item::Table(servers)) => {
                servers.remove(SERVER);
            }
            Some(toml_edit::Item::ArrayOfTables(servers)) => {
                if let Some(found) = toml_named(servers) {
                    servers.remove(found);
                }
            }
            _ => {}
        }
    }
    Ok(document.to_string())
}

/// The entry as a YAML mapping written on one line, which needs no indentation to fit wherever
/// it goes, and so cannot be misplaced by it.
fn yaml_entry(entry: &Entry) -> yaml_edit::Mapping {
    let mapping = yaml_edit::Mapping::new_flow();
    for (key, field) in entry {
        match field {
            Field::Text(text) => mapping.set(*key, text.as_str()),
            Field::Texts(texts) => {
                let list = yaml_edit::Sequence::new_flow();
                for text in texts {
                    list.push(text.as_str());
                }
                mapping.set(*key, list);
            }
            Field::True => mapping.set(*key, true),
            Field::Number(number) => mapping.set(*key, *number),
        }
    }
    mapping
}

fn yaml_file(text: &str) -> Result<yaml_edit::YamlFile> {
    yaml_edit::YamlFile::from_str(text).map_err(|error| anyhow::anyhow!("{error}"))
}

fn yaml_named(list: &yaml_edit::Sequence) -> Option<usize> {
    (0..list.len()).find(|&at| {
        list.get(at)
            .and_then(|server| server.as_mapping().and_then(|server| server.get("name")))
            .is_some_and(|name| name.to_string().trim().trim_matches(['"', '\'']) == SERVER)
    })
}

fn yaml_with(text: &str, format: Format, entry: &Entry) -> Result<String> {
    let file = yaml_file(text)?;
    let document = file.ensure_document();
    let mut at = document.as_mapping().context("its root is not a mapping")?;
    let mut rest = format.container;
    // Down through what is there. The first key that is missing takes the rest as one value.
    while let [key, after @ ..] = rest {
        if after.is_empty() {
            break;
        }
        match at.get_mapping(key) {
            Some(next) => {
                at = next;
                rest = after;
            }
            None if at.contains_key(key) => bail!("its `{key}` is not a mapping"),
            None => break,
        }
    }
    let [key, below @ ..] = rest else {
        bail!("a client names where its servers are");
    };
    if below.is_empty() {
        match format.holds {
            Holds::Map => match at.get_mapping(key) {
                Some(servers) => servers.set(SERVER, yaml_entry(entry)),
                None if at.contains_key(key) => bail!("its `{key}` is not a mapping"),
                None => {
                    let servers = yaml_edit::Mapping::new_flow();
                    servers.set(SERVER, yaml_entry(entry));
                    at.set(*key, servers);
                }
            },
            Holds::List => match at.get_sequence(key) {
                Some(list) => match yaml_named(&list) {
                    Some(found) => {
                        list.set(found, yaml_entry(entry));
                    }
                    None => list.push(yaml_entry(entry)),
                },
                None if at.contains_key(key) => bail!("its `{key}` is not a list"),
                None => {
                    let list = yaml_edit::Sequence::new_flow();
                    list.push(yaml_entry(entry));
                    at.set(*key, list);
                }
            },
        }
    } else {
        bail!("SteamGauge cannot make `{}` in a YAML file", rest.join("."));
    }
    Ok(with_final_newline(file.to_string()))
}

fn yaml_without(text: &str, format: Format) -> Result<String> {
    let file = yaml_file(text)?;
    let Some((last, parents)) = format.container.split_last() else {
        return Ok(text.to_owned());
    };
    let mut at = file.document().and_then(|document| document.as_mapping());
    for key in parents {
        at = at.and_then(|mapping| mapping.get_mapping(key));
    }
    if let Some(parent) = at {
        match format.holds {
            Holds::Map => {
                if let Some(servers) = parent.get_mapping(last) {
                    servers.remove(SERVER);
                }
            }
            Holds::List => {
                if let Some(list) = parent.get_sequence(last)
                    && let Some(found) = yaml_named(&list)
                {
                    list.remove(found);
                }
            }
        }
    }
    Ok(file.to_string())
}

fn with_final_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Writes `text` beside `file` and renames it over, so a crash part way leaves the old file.
fn write_in_place(file: &Path, text: &str) -> Result<()> {
    let folder = file
        .parent()
        .with_context(|| format!("{} is in no folder", file.display()))?;
    std::fs::create_dir_all(folder)
        .with_context(|| format!("could not make {}", folder.display()))?;
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let partial = folder.join(format!(".{name}.steamgauge-partial"));
    std::fs::write(&partial, text)
        .with_context(|| format!("could not write {}", partial.display()))?;
    std::fs::rename(&partial, file).with_context(|| format!("could not replace {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("steamgauge-clients-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn folders(&self) -> Folders {
            Folders {
                home: self.0.join("home"),
                config: self.0.join("config"),
                local: self.0.join("local"),
                variable: |_| None,
                program: |_| None,
                system: System::this(),
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn client(format: Format) -> Client {
        Client {
            id: "test",
            name: "Test",
            file: |folders| folders.home.join(".test").join("mcp.json"),
            format,
            by: By::Editing,
            installed: |folders| vec![folders.home.join(".test")],
            then: None,
        }
    }

    fn toml_client() -> Client {
        Client {
            file: |folders| folders.home.join(".test").join("config.toml"),
            ..client(Format::toml(&["mcp_servers"], Holds::Map, Shape::Plain))
        }
    }

    const PLAIN: Format = JSON_MCP_SERVERS;

    fn launch(program: &str) -> Launch {
        Launch {
            program: program.to_owned(),
            args: vec!["mcp".to_owned()],
        }
    }

    fn read(client: &Client, folders: &Folders) -> String {
        std::fs::read_to_string((client.file)(folders)).unwrap()
    }

    fn json(text: &str) -> serde_json::Value {
        jsonc_parser::parse_to_serde_value::<serde_json::Value>(
            text,
            &jsonc_parser::ParseOptions::default(),
        )
        .unwrap()
    }

    fn written_over(client: &Client, folders: &Folders, before: &str) -> String {
        let file = (client.file)(folders);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, before).unwrap();
        add(client, folders, &launch("/opt/steamgauge")).unwrap();
        read(client, folders)
    }

    #[test]
    fn a_client_is_absent_installed_or_added() {
        let scratch = Scratch::new("state");
        let folders = scratch.folders();
        let client = client(PLAIN);
        let here = launch("/opt/steamgauge");
        assert_eq!(state(&client, &folders, &here), State::Absent);
        std::fs::create_dir_all(folders.home.join(".test")).unwrap();
        assert_eq!(state(&client, &folders, &here), State::Installed);
        add(&client, &folders, &here).unwrap();
        assert_eq!(state(&client, &folders, &here), State::Added);
        assert_eq!(
            state(&client, &folders, &launch("/elsewhere/steamgauge")),
            State::Elsewhere {
                command: "/opt/steamgauge".to_owned()
            }
        );
    }

    #[test]
    fn a_file_that_is_not_there_is_made_with_the_entry_alone() {
        let scratch = Scratch::new("new");
        let folders = scratch.folders();
        let client = client(PLAIN);
        let done = add(&client, &folders, &launch("C:\\SteamGauge\\steamgauge.exe")).unwrap();
        assert!(matches!(done, Added::Written(_)));
        assert_eq!(
            json(&read(&client, &folders)),
            serde_json::json!({"mcpServers": {"steamgauge": {
                "command": "C:\\SteamGauge\\steamgauge.exe", "args": ["mcp"]
            }}})
        );
    }

    #[test]
    fn every_other_server_and_every_comment_is_kept() {
        let scratch = Scratch::new("kept");
        let folders = scratch.folders();
        let client = client(PLAIN);
        let before = "{\n  // the person's own note\n  \"theme\": \"dark\",\n  \"mcpServers\": {\n    \"other\": { \"command\": \"other\", \"args\": [] }, // trailing\n  },\n}\n";
        let after = written_over(&client, &folders, before);
        assert!(after.contains("// the person's own note"), "{after}");
        assert!(after.contains("// trailing"), "{after}");
        let value = json(&after);
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
        assert_eq!(
            value["mcpServers"]["steamgauge"]["command"],
            "/opt/steamgauge"
        );

        assert!(matches!(
            remove(&client, &folders).unwrap(),
            Removed::Taken(_)
        ));
        let removed = read(&client, &folders);
        assert!(removed.contains("// the person's own note"), "{removed}");
        let value = json(&removed);
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
        assert!(value["mcpServers"].get("steamgauge").is_none());
        assert!(matches!(
            remove(&client, &folders).unwrap(),
            Removed::NoEntry(_)
        ));
    }

    #[test]
    fn an_entry_for_another_copy_is_replaced_and_the_same_one_left_alone() {
        let scratch = Scratch::new("replace");
        let folders = scratch.folders();
        let client = client(PLAIN);
        add(&client, &folders, &launch("/old/steamgauge")).unwrap();
        let written = read(&client, &folders);
        assert!(matches!(
            add(&client, &folders, &launch("/old/steamgauge")).unwrap(),
            Added::Already(_)
        ));
        assert_eq!(
            read(&client, &folders),
            written,
            "an unchanged entry is not written again"
        );
        assert_eq!(
            add(&client, &folders, &launch("/new/steamgauge")).unwrap(),
            Added::Replaced {
                file: (client.file)(&folders),
                was: "/old/steamgauge".to_owned()
            }
        );
        assert_eq!(
            json(&read(&client, &folders))["mcpServers"]["steamgauge"]["command"],
            "/new/steamgauge"
        );
    }

    #[test]
    fn a_file_that_does_not_parse_is_refused_and_left_as_it_was() {
        let scratch = Scratch::new("broken");
        let folders = scratch.folders();
        for (client, broken) in [
            (client(PLAIN), "{ \"mcpServers\": { oops"),
            (toml_client(), "[mcp_servers\ncommand = 1"),
            (
                Client {
                    file: |folders| folders.home.join(".test").join("config.yaml"),
                    ..client(Format::yaml(&["mcp_servers"], Holds::Map, Shape::Plain))
                },
                "mcp_servers:\n  a: 1\n  a: 2\n",
            ),
        ] {
            let file = (client.file)(&folders);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, broken).unwrap();
            assert!(
                matches!(
                    state(&client, &folders, &launch("/opt/steamgauge")),
                    State::Unreadable { .. }
                ),
                "{broken}"
            );
            let refusal = add(&client, &folders, &launch("/opt/steamgauge")).unwrap_err();
            assert!(refusal.to_string().contains("does not parse"), "{refusal}");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), broken);
        }
    }

    #[test]
    fn each_shape_is_written_the_way_its_client_reads_it() {
        let scratch = Scratch::new("shapes");
        let folders = scratch.folders();
        let here = launch("/opt/steamgauge");
        let cases: &[(Shape, serde_json::Value)] = &[
            (
                Shape::Typed,
                serde_json::json!({"type": "stdio", "command": "/opt/steamgauge", "args": ["mcp"]}),
            ),
            (
                Shape::Local,
                serde_json::json!({"type": "local", "command": ["/opt/steamgauge", "mcp"], "enabled": true}),
            ),
            (
                Shape::Copilot,
                serde_json::json!({"type": "local", "command": "/opt/steamgauge", "args": ["mcp"], "tools": ["*"]}),
            ),
            (
                Shape::Goose,
                serde_json::json!({"name": "steamgauge", "type": "stdio", "cmd": "/opt/steamgauge", "args": ["mcp"], "enabled": true, "timeout": 300}),
            ),
        ];
        for (shape, entry) in cases {
            let client = client(Format::json(&["servers"], *shape));
            let _ = std::fs::remove_file((client.file)(&folders));
            add(&client, &folders, &here).unwrap();
            assert_eq!(
                json(&read(&client, &folders)),
                serde_json::json!({"servers": {"steamgauge": entry}}),
                "{shape:?}"
            );
            assert_eq!(state(&client, &folders, &here), State::Added, "{shape:?}");
        }
    }

    #[test]
    fn servers_nested_deeper_keep_what_is_beside_them() {
        let scratch = Scratch::new("nested");
        let folders = scratch.folders();
        let client = client(Format::json(&["tools", "mcpServers"], Shape::Plain));
        let after = written_over(
            &client,
            &folders,
            "{ \"tools\": { \"web\": { \"enabled\": true } }, \"model\": \"x\" }",
        );
        assert_eq!(
            json(&after),
            serde_json::json!({"tools": {"web": {"enabled": true}, "mcpServers": {"steamgauge": {
                "command": "/opt/steamgauge", "args": ["mcp"]
            }}}, "model": "x"})
        );
        remove(&client, &folders).unwrap();
        assert_eq!(
            json(&read(&client, &folders))["tools"]["mcpServers"],
            serde_json::json!({})
        );
    }

    #[test]
    fn a_list_of_named_servers_gains_one_and_replaces_its_own() {
        let scratch = Scratch::new("list");
        let folders = scratch.folders();
        let client = client(Format {
            holds: Holds::List,
            ..Format::json(&["mcp", "servers"], Shape::Transport)
        });
        let after = written_over(
            &client,
            &folders,
            "{\"mcp\": {\"servers\": [{\"name\": \"fs\", \"command\": \"npx\"}]}}",
        );
        assert_eq!(
            json(&after)["mcp"]["servers"],
            serde_json::json!([
                {"name": "fs", "command": "npx"},
                {"name": "steamgauge", "transport": "stdio", "command": "/opt/steamgauge", "args": ["mcp"]}
            ])
        );
        add(&client, &folders, &launch("/new/steamgauge")).unwrap();
        let list = json(&read(&client, &folders))["mcp"]["servers"].clone();
        assert_eq!(list.as_array().unwrap().len(), 2, "{list}");
        assert_eq!(list[1]["command"], "/new/steamgauge");
        remove(&client, &folders).unwrap();
        assert_eq!(
            json(&read(&client, &folders))["mcp"]["servers"],
            serde_json::json!([{"name": "fs", "command": "npx"}])
        );
    }

    #[test]
    fn a_toml_entry_keeps_the_rest_of_the_file() {
        let scratch = Scratch::new("toml");
        let folders = scratch.folders();
        let client = toml_client();
        let after = written_over(
            &client,
            &folders,
            "# the person's own note\nmodel = \"o4\"\n\n[mcp_servers.other]\ncommand = \"other\"\n",
        );
        assert!(
            after.starts_with("# the person's own note\nmodel = \"o4\""),
            "{after}"
        );
        assert!(after.contains("[mcp_servers.other]"), "{after}");
        assert!(
            after.contains(
                "[mcp_servers.steamgauge]\ncommand = \"/opt/steamgauge\"\nargs = [\"mcp\"]"
            ),
            "{after}"
        );
        assert_eq!(
            state(&client, &folders, &launch("/opt/steamgauge")),
            State::Added
        );

        remove(&client, &folders).unwrap();
        let removed = read(&client, &folders);
        assert!(!removed.contains("steamgauge"), "{removed}");
        assert!(removed.contains("[mcp_servers.other]"), "{removed}");
    }

    #[test]
    fn a_toml_list_of_tables_gains_one_and_turns_on_what_it_needs() {
        let scratch = Scratch::new("toml-list");
        let folders = scratch.folders();
        let client = Client {
            file: |folders| folders.home.join(".test").join("vtcode.toml"),
            ..client(Format {
                turns_on: Some(&["mcp", "enabled"]),
                ..Format::toml(&["mcp", "providers"], Holds::List, Shape::Enabled)
            })
        };
        let after = written_over(
            &client,
            &folders,
            "# mine\n[[mcp.providers]]\nname = \"time\"\ncommand = \"uvx\"\n",
        );
        assert!(
            after.contains("# mine\n[[mcp.providers]]\nname = \"time\""),
            "{after}"
        );
        assert!(
            after.contains(
                "[[mcp.providers]]\nname = \"steamgauge\"\nenabled = true\ncommand = \"/opt/steamgauge\"\nargs = [\"mcp\"]"
            ),
            "{after}"
        );
        let value = value_of(Language::Toml, &after).unwrap();
        assert_eq!(value["mcp"]["enabled"], true, "{after}");
        assert_eq!(value["mcp"]["providers"][0]["name"], "time");
        assert_eq!(
            state(&client, &folders, &launch("/opt/steamgauge")),
            State::Added
        );
        remove(&client, &folders).unwrap();
        let value = value_of(Language::Toml, &read(&client, &folders)).unwrap();
        assert_eq!(
            value["mcp"]["providers"],
            serde_json::json!([{"name": "time", "command": "uvx"}])
        );
    }

    #[test]
    fn a_yaml_map_keeps_its_comments_and_gains_the_entry_on_one_line() {
        let scratch = Scratch::new("yaml");
        let folders = scratch.folders();
        let client = Client {
            file: |folders| folders.home.join(".test").join("config.yaml"),
            ..client(Format::yaml(&["extensions"], Holds::Map, Shape::Goose))
        };
        let before = "# my goose config\nGOOSE_PROVIDER: openai\nextensions:\n  github:\n    name: GitHub # the hub\n    cmd: npx\n    enabled: true\n";
        let after = written_over(&client, &folders, before);
        assert!(after.starts_with(before), "{after}");
        let value = value_of(Language::Yaml, &after).unwrap();
        assert_eq!(value["extensions"]["github"]["cmd"], "npx");
        assert_eq!(value["extensions"]["steamgauge"]["cmd"], "/opt/steamgauge");
        assert_eq!(value["extensions"]["steamgauge"]["timeout"], 300);
        remove(&client, &folders).unwrap();
        assert_eq!(read(&client, &folders), before);
    }

    #[test]
    fn a_yaml_file_gains_its_container_and_a_yaml_list_its_entry() {
        let scratch = Scratch::new("yaml-new");
        let folders = scratch.folders();
        let map = Client {
            file: |folders| folders.home.join(".test").join("config.yaml"),
            ..client(Format::yaml(&["mcp_servers"], Holds::Map, Shape::Plain))
        };
        for before in [
            "",
            "# only a comment\n",
            "# hermes\nmodel: x\nterminal:\n  backend: local\n",
        ] {
            let after = written_over(&map, &folders, before);
            let value = value_of(Language::Yaml, &after).unwrap();
            assert_eq!(
                value["mcp_servers"]["steamgauge"],
                serde_json::json!({"command": "/opt/steamgauge", "args": ["mcp"]}),
                "{after}"
            );
        }

        let list = Client {
            file: |folders| folders.home.join(".test").join("list.yaml"),
            ..client(Format::yaml(&["mcpServers"], Holds::List, Shape::Typed))
        };
        let before =
            "name: Local\nmcpServers:\n  - name: sqlite # db\n    type: stdio\n    command: npx\n";
        let after = written_over(&list, &folders, before);
        let value = value_of(Language::Yaml, &after).unwrap();
        assert_eq!(value["mcpServers"][0]["name"], "sqlite");
        assert_eq!(value["mcpServers"][1]["name"], "steamgauge");
        assert_eq!(
            state(&list, &folders, &launch("/opt/steamgauge")),
            State::Added
        );
        add(&list, &folders, &launch("/new/steamgauge")).unwrap();
        let value = value_of(Language::Yaml, &read(&list, &folders)).unwrap();
        assert_eq!(value["mcpServers"].as_array().unwrap().len(), 2);
        assert_eq!(value["mcpServers"][1]["command"], "/new/steamgauge");
        remove(&list, &folders).unwrap();
        let value = value_of(Language::Yaml, &read(&list, &folders)).unwrap();
        assert_eq!(value["mcpServers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_container_of_another_kind_is_refused_and_left_as_it_was() {
        let scratch = Scratch::new("kind");
        let folders = scratch.folders();
        let client = client(PLAIN);
        let file = (client.file)(&folders);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{\"mcpServers\": [1, 2]}").unwrap();
        assert!(add(&client, &folders, &launch("/opt/steamgauge")).is_err());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "{\"mcpServers\": [1, 2]}"
        );
    }

    #[test]
    fn a_client_added_by_its_own_command_says_how_when_the_command_is_missing() {
        let scratch = Scratch::new("command");
        let folders = scratch.folders();
        let claude = by_id("claude-code").unwrap();
        let refusal = add(claude, &folders, &launch("/opt/steamgauge"))
            .unwrap_err()
            .to_string();
        assert!(
            refusal.contains(
                "run `claude mcp add --scope user steamgauge -- \"/opt/steamgauge\" mcp`"
            ),
            "{refusal}"
        );
        std::fs::create_dir_all(&folders.home).unwrap();
        std::fs::write(
            folders.home.join(".claude.json"),
            "{\"numStartups\": 3, \"mcpServers\": {\"steamgauge\": {\"type\": \"stdio\", \"command\": \"/opt/steamgauge\", \"args\": [\"mcp\"]}}}",
        )
        .unwrap();
        assert_eq!(
            state(claude, &folders, &launch("/opt/steamgauge")),
            State::Added
        );
        assert!(remove(claude, &folders).is_err());
    }

    #[test]
    fn every_client_has_its_own_name_and_its_own_file() {
        let folders = Scratch::new("table").folders();
        let mut ids = std::collections::HashSet::new();
        let mut files = std::collections::HashSet::new();
        for client in CLIENTS {
            assert!(ids.insert(client.id), "{} twice", client.id);
            assert!(
                files.insert((client.file)(&folders)),
                "{} shares a file",
                client.id
            );
            assert!(
                (client.file)(&folders).starts_with(scratch_root(&folders)),
                "{} is not in the person's folders",
                client.id
            );
        }
        assert!(by_id("cursor").is_ok());
        let refusal = by_id("nothing").unwrap_err().to_string();
        assert!(refusal.contains("codex"), "{refusal}");
    }

    #[test]
    fn every_client_takes_its_entry_into_an_empty_file() {
        for client in CLIENTS {
            let written = with_entry(client, "", &launch("C:\\Program Files\\steamgauge.exe"))
                .unwrap_or_else(|error| panic!("{}: {error}", client.id));
            assert_eq!(
                entry_command(client.format, &written).unwrap().as_deref(),
                Some("C:\\Program Files\\steamgauge.exe"),
                "{}",
                client.id
            );
        }
    }

    fn scratch_root(folders: &Folders) -> PathBuf {
        folders.home.parent().unwrap().to_path_buf()
    }

    #[test]
    fn a_folder_a_client_is_told_in_its_variable_is_followed() {
        let scratch = Scratch::new("variable");
        let folders = Folders {
            variable: |name| (name == "CODEX_HOME").then(|| PathBuf::from("/elsewhere/codex")),
            ..scratch.folders()
        };
        let codex = by_id("codex").unwrap();
        assert_eq!(
            (codex.file)(&folders),
            Path::new("/elsewhere/codex").join("config.toml")
        );
    }

    #[test]
    fn a_scoop_copy_is_named_by_its_current_folder() {
        let scratch = Scratch::new("scoop");
        let apps = scratch.0.join("scoop").join("apps").join("steamgauge");
        for version in ["0.2.0", "current"] {
            std::fs::create_dir_all(apps.join(version)).unwrap();
            std::fs::write(apps.join(version).join("steamgauge.exe"), "").unwrap();
        }
        assert_eq!(
            steady_path(&apps.join("0.2.0").join("steamgauge.exe")),
            apps.join("current").join("steamgauge.exe")
        );
        let installed = scratch
            .0
            .join("Programs")
            .join("SteamGauge")
            .join("steamgauge.exe");
        assert_eq!(steady_path(&installed), installed);
    }
}
