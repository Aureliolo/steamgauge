//! The tables the documentation site renders, taken from the definitions the program runs on, so
//! a page cannot describe a command, a tool or an MCP client the program does not have.

use std::path::PathBuf;

use crate::clients::{By, CLIENTS, Folders, Launch, System, with_entry};

/// The command line, as `--help` prints it.
#[must_use]
pub fn command() -> clap::Command {
    crate::cli::command()
}

/// One tool the app's MCP server offers.
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// It only looks: nothing on disk or in the window changes.
    pub read_only: bool,
    /// It removes something that has to be downloaded or worked out again to get back.
    pub destructive: bool,
    /// The JSON schema of what it is given.
    pub input: serde_json::Value,
}

/// Every tool, in the order a client lists them.
#[must_use]
pub fn tools() -> Vec<Tool> {
    crate::ui::tool_reference()
}

/// One MCP client the app adds itself to, with the file it writes on each system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub id: &'static str,
    pub name: &'static str,
    pub windows: String,
    pub macos: String,
    pub linux: String,
    /// What the person still has to do once the file is written, if anything.
    pub then: Option<&'static str>,
    /// The client's own command `connect` runs, for a client that rewrites its own file, with
    /// the program's path as `PROGRAM`.
    pub runs: Option<String>,
    /// The file as `connect` writes it where there was none, with the program's path as
    /// `PROGRAM`, for a client whose file the app writes.
    pub entry: Option<String>,
    /// The language the file is written in: `json`, `toml` or `yaml`.
    pub language: &'static str,
}

/// What stands for the program's path in a client's entry.
pub const PROGRAM: &str = "<path to steamgauge>";

/// Every client, each with its file as a person would type it on each system.
///
/// # Panics
///
/// Never: an empty file always takes an entry, and the tests hold every client to it.
#[must_use]
pub fn clients() -> Vec<Client> {
    let on = |home: &str, config: &str, local: &str, system: System| Folders {
        home: PathBuf::from(home),
        config: PathBuf::from(config),
        local: PathBuf::from(local),
        variable: |_| None,
        program: |_| None,
        system,
    };
    let windows = on(
        "%USERPROFILE%",
        "%APPDATA%",
        "%LOCALAPPDATA%",
        System::Windows,
    );
    let macos = on(
        "~",
        "~/Library/Application Support",
        "~/Library/Application Support",
        System::MacOs,
    );
    let linux = on("~", "~/.config", "~/.local/share", System::Linux);
    // The folders are joined on whatever system builds the site, so each is written back with
    // the separator of the system it describes.
    let written = |path: PathBuf, separator: char| {
        path.to_string_lossy()
            .replace(['/', '\\'], &separator.to_string())
    };
    let launch = Launch {
        program: PROGRAM.to_owned(),
        args: vec!["mcp".to_owned()],
    };
    CLIENTS
        .iter()
        .map(|client| Client {
            id: client.id,
            name: client.name,
            windows: written((client.file)(&windows), '\\'),
            macos: written((client.file)(&macos), '/'),
            linux: written((client.file)(&linux), '/'),
            then: client.then,
            runs: match client.by {
                By::Running { program, add, .. } => {
                    Some(format!("{program} {} {}", add.join(" "), launch.line()))
                }
                By::Editing => None,
            },
            entry: matches!(client.by, By::Editing).then(|| {
                with_entry(client, "", &launch)
                    .expect("an empty file takes the entry")
                    .trim_end()
                    .to_owned()
            }),
            language: client.format.language.name(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_system_names_its_own_folder_with_its_own_separator() {
        let zed = clients()
            .into_iter()
            .find(|client| client.id == "zed")
            .unwrap();
        assert_eq!(zed.windows, r"%APPDATA%\Zed\settings.json");
        assert_eq!(zed.macos, "~/.config/zed/settings.json");
        let desktop = clients()
            .into_iter()
            .find(|client| client.id == "claude-desktop")
            .unwrap();
        assert_eq!(
            desktop.macos,
            "~/Library/Application Support/Claude/claude_desktop_config.json"
        );
        assert_eq!(desktop.linux, "~/.config/Claude/claude_desktop_config.json");
    }

    #[test]
    fn each_entry_is_the_file_connect_writes() {
        let entry = |id: &str| {
            clients()
                .into_iter()
                .find(|client| client.id == id)
                .unwrap()
                .entry
                .unwrap()
        };
        let cursor = entry("cursor");
        assert!(cursor.contains("\"mcpServers\""), "{cursor}");
        assert!(cursor.contains("\"steamgauge\""), "{cursor}");
        assert!(cursor.contains("\"<path to steamgauge>\""), "{cursor}");
        assert!(cursor.contains("\"mcp\""), "{cursor}");
        let codex = entry("codex");
        assert!(codex.contains("[mcp_servers.steamgauge]"), "{codex}");
        assert!(
            codex.contains("command = \"<path to steamgauge>\""),
            "{codex}"
        );
        let claude = clients()
            .into_iter()
            .find(|client| client.id == "claude-code")
            .unwrap();
        assert_eq!(claude.entry, None);
        assert_eq!(
            claude.runs.as_deref(),
            Some("claude mcp add --scope user steamgauge -- \"<path to steamgauge>\" mcp")
        );
    }

    #[test]
    fn the_command_line_and_the_tools_are_the_ones_the_program_runs() {
        assert!(
            command()
                .get_subcommands()
                .any(|command| command.get_name() == "connect")
        );
        let tools = tools();
        assert!(tools.iter().any(|tool| tool.name == "connect_client"));
        assert!(tools.iter().all(|tool| !tool.description.is_empty()));
    }
}
