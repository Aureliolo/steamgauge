//! The tables the documentation site renders, taken from the definitions the program runs on, so
//! a page cannot describe a command, a tool or an MCP client the program does not have.

use std::path::PathBuf;

use crate::clients::{CLIENTS, Folders};

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
}

/// Every client, each with its file as a person would type it on each system.
#[must_use]
pub fn clients() -> Vec<Client> {
    let on = |home: &str, config: &str, windows: bool| Folders {
        home: PathBuf::from(home),
        config: PathBuf::from(config),
        variable: |_| None,
        windows,
    };
    let windows = on("%USERPROFILE%", "%APPDATA%", true);
    let macos = on("~", "~/Library/Application Support", false);
    let linux = on("~", "~/.config", false);
    // The folders are joined on whatever system builds the site, so each is written back with
    // the separator of the system it describes.
    let written = |path: PathBuf, separator: char| {
        path.to_string_lossy()
            .replace(['/', '\\'], &separator.to_string())
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
