//! Adding the app to the MCP clients a person runs.
//!
//! A client starts a server it is told about in its own configuration, and every client keeps
//! that in a file of its own: the same entry, a command and its arguments, under a different key
//! and in a different shape. So the clients are rows of a table rather than an integration each,
//! and each row's file, key and shape is the one that client's own documentation gives. A client
//! whose user-wide configuration is not documented is not in the table.
//!
//! Adding changes the app's own entry and nothing else: every other server, every comment and the
//! file's layout stay as they were, a file that does not parse is refused rather than replaced,
//! and the file is written beside itself and renamed into place, so a crash leaves the old one.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The name the app is entered under in every client's configuration.
pub const SERVER: &str = "steamgauge";

/// The folders a client's configuration is found from, so a test can stand in for the person's.
#[derive(Debug, Clone)]
pub struct Folders {
    /// The person's home folder.
    pub home: PathBuf,
    /// Where programs keep configuration: `%APPDATA%` on Windows, `~/Library/Application Support`
    /// on macOS, `~/.config` (or `$XDG_CONFIG_HOME`) on Linux.
    pub config: PathBuf,
    /// The environment variable a client reads its folder from instead, where it is set.
    pub variable: fn(&str) -> Option<PathBuf>,
    /// Whether the folders are Windows's, where some clients keep their files in `config` rather
    /// than the XDG folder they use everywhere else.
    pub windows: bool,
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
            variable: |name| {
                std::env::var_os(name)
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
            },
            windows: cfg!(windows),
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
}

/// Every client the app can add itself to, each as its own documentation gives its user-wide
/// configuration. Claude Code is not here: its user-wide servers live in `~/.claude.json`, a file
/// it rewrites itself while it runs, so `claude mcp add` is the way in. Goose and Continue keep
/// YAML, and LM Studio and `JetBrains` AI Assistant document no file; `docs/assistants.md` gives
/// each the command or the steps instead.
pub const CLIENTS: &[Client] = &[
    Client {
        id: "claude-desktop",
        name: "Claude Desktop",
        file: |folders| {
            folders
                .config
                .join("Claude")
                .join("claude_desktop_config.json")
        },
        format: Format::Json {
            container: "mcpServers",
            shape: Shape::Plain,
        },
        installed: |folders| vec![folders.config.join("Claude")],
        then: Some("Quit Claude Desktop completely and open it again."),
    },
    Client {
        id: "cursor",
        name: "Cursor",
        file: |folders| folders.home.join(".cursor").join("mcp.json"),
        format: Format::Json {
            container: "mcpServers",
            shape: Shape::Typed,
        },
        installed: |folders| vec![folders.home.join(".cursor")],
        then: None,
    },
    Client {
        id: "copilot",
        name: "VS Code and GitHub Copilot CLI",
        file: |folders| {
            folders
                .or("COPILOT_HOME", folders.home.join(".copilot"))
                .join("mcp-config.json")
        },
        format: Format::Json {
            container: "mcpServers",
            shape: Shape::Typed,
        },
        installed: |folders| {
            vec![
                folders.or("COPILOT_HOME", folders.home.join(".copilot")),
                folders.config.join("Code"),
            ]
        },
        then: Some("VS Code asks once whether to trust it, the first time it starts."),
    },
    Client {
        id: "devin",
        name: "Devin Desktop (Windsurf)",
        file: |folders| devin_folder(folders).join("mcp_config.json"),
        format: Format::Json {
            container: "mcpServers",
            shape: Shape::Plain,
        },
        installed: |folders| {
            vec![
                devin_folder(folders),
                folders.home.join(".codeium").join("windsurf"),
            ]
        },
        then: None,
    },
    Client {
        id: "codex",
        name: "Codex",
        file: |folders| {
            folders
                .or("CODEX_HOME", folders.home.join(".codex"))
                .join("config.toml")
        },
        format: Format::Toml {
            container: "mcp_servers",
        },
        installed: |folders| vec![folders.or("CODEX_HOME", folders.home.join(".codex"))],
        then: None,
    },
    Client {
        id: "gemini",
        name: "Gemini CLI",
        file: |folders| {
            folders
                .or("GEMINI_CLI_HOME", folders.home.clone())
                .join(".gemini")
                .join("settings.json")
        },
        format: Format::Json {
            container: "mcpServers",
            shape: Shape::Plain,
        },
        installed: |folders| {
            vec![
                folders
                    .or("GEMINI_CLI_HOME", folders.home.clone())
                    .join(".gemini"),
            ]
        },
        then: None,
    },
    Client {
        id: "zed",
        name: "Zed",
        file: |folders| zed_folder(folders).join("settings.json"),
        format: Format::Json {
            container: "context_servers",
            shape: Shape::Plain,
        },
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
        format: Format::Json {
            container: "mcp",
            shape: Shape::Local,
        },
        installed: |folders| vec![folders.xdg_config().join("opencode")],
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
        format: Format::Json {
            container: "mcpServers",
            shape: Shape::Plain,
        },
        installed: |folders| vec![folders.home.join(".cline")],
        then: None,
    },
    Client {
        id: "junie",
        name: "Junie",
        file: |folders| folders.home.join(".junie").join("mcp").join("mcp.json"),
        format: Format::Json {
            container: "mcpServers",
            shape: Shape::Plain,
        },
        installed: |folders| vec![folders.home.join(".junie")],
        then: None,
    },
];

/// Devin Desktop's folder: `%APPDATA%\devin` on Windows, the XDG one elsewhere, macOS included.
fn devin_folder(folders: &Folders) -> PathBuf {
    if folders.windows {
        folders.config.join("devin")
    } else {
        folders.xdg_config().join("devin")
    }
}

/// Zed's folder: `%APPDATA%\Zed` on Windows, the XDG one elsewhere, macOS included.
fn zed_folder(folders: &Folders) -> PathBuf {
    if folders.windows {
        folders.config.join("Zed")
    } else {
        folders.xdg_config().join("zed")
    }
}

/// How a client writes one stdio server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `{"command": ..., "args": [...]}`.
    Plain,
    /// The same with `"type": "stdio"`.
    Typed,
    /// `{"type": "local", "command": [program, ...args], "enabled": true}`.
    Local,
}

/// Where in a client's file its servers are kept, and in what format.
#[derive(Debug, Clone, Copy)]
pub enum Format {
    /// JSON, comments allowed, with the servers in the object at this key of the root.
    Json {
        container: &'static str,
        shape: Shape,
    },
    /// TOML, with each server a table under this one.
    Toml { container: &'static str },
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

/// Adds the app to `client`, or points an existing entry at `launch`.
///
/// # Errors
///
/// Fails if the file does not parse, or cannot be read or written.
pub fn add(client: &Client, folders: &Folders, launch: &Launch) -> Result<Added> {
    let file = (client.file)(folders);
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", file.display()));
        }
    };
    let was = entry_command(client.format, &text).map_err(|reason| refused(&file, &reason))?;
    if was.as_deref() == Some(launch.program.as_str()) {
        return Ok(Added::Already(file));
    }
    write_in_place(&file, &with_entry(client, &text, launch)?)?;
    Ok(match was {
        None => Added::Written(file),
        Some(was) => Added::Replaced { file, was },
    })
}

/// `text`, a client's configuration, with the app's entry in it.
///
/// # Errors
///
/// Fails if `text` does not parse.
pub fn with_entry(client: &Client, text: &str, launch: &Launch) -> Result<String> {
    match client.format {
        Format::Json { container, shape } => json_with(text, container, shape, launch),
        Format::Toml { container } => toml_with(text, container, launch),
    }
}

/// Takes the app's entry out of `client`, leaving everything else as it was.
///
/// # Errors
///
/// Fails if the file does not parse, or cannot be read or written.
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
    let written = match client.format {
        Format::Json { container, .. } => json_without(&text, container)?,
        Format::Toml { container } => toml_without(&text, container)?,
    };
    write_in_place(&file, &written)?;
    Ok(Removed::Taken(file))
}

fn refused(file: &Path, reason: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "{} does not parse ({reason}), so SteamGauge was not added to it; fix the file, or add \
         the entry by hand",
        file.display()
    )
}

/// The command the app's entry starts, if the file has one.
fn entry_command(format: Format, text: &str) -> std::result::Result<Option<String>, String> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    match format {
        Format::Json { container, shape } => {
            let value = jsonc_parser::parse_to_serde_value::<Option<serde_json::Value>>(
                text,
                &jsonc_parser::ParseOptions::default(),
            )
            .map_err(|error| error.to_string())?;
            let Some(entry) = value
                .as_ref()
                .and_then(|root| root.get(container))
                .and_then(|servers| servers.get(SERVER))
            else {
                return Ok(None);
            };
            let command = match shape {
                Shape::Local => entry
                    .get("command")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|parts| parts.first()),
                Shape::Plain | Shape::Typed => entry.get("command"),
            };
            Ok(Some(
                command
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ))
        }
        Format::Toml { container } => {
            let document = text
                .parse::<toml_edit::DocumentMut>()
                .map_err(|error| error.to_string())?;
            Ok(document
                .get(container)
                .and_then(|servers| servers.get(SERVER))
                .map(|entry| {
                    entry
                        .get("command")
                        .and_then(toml_edit::Item::as_str)
                        .unwrap_or_default()
                        .to_owned()
                }))
        }
    }
}

fn json_entry(shape: Shape, launch: &Launch) -> jsonc_parser::cst::CstInputValue {
    use jsonc_parser::cst::CstInputValue as V;
    let args = || V::Array(launch.args.iter().cloned().map(V::String).collect());
    V::Object(match shape {
        Shape::Plain => vec![
            ("command".to_owned(), V::String(launch.program.clone())),
            ("args".to_owned(), args()),
        ],
        Shape::Typed => vec![
            ("type".to_owned(), V::String("stdio".to_owned())),
            ("command".to_owned(), V::String(launch.program.clone())),
            ("args".to_owned(), args()),
        ],
        Shape::Local => vec![
            ("type".to_owned(), V::String("local".to_owned())),
            (
                "command".to_owned(),
                V::Array(
                    std::iter::once(&launch.program)
                        .chain(&launch.args)
                        .cloned()
                        .map(V::String)
                        .collect(),
                ),
            ),
            ("enabled".to_owned(), V::Bool(true)),
        ],
    })
}

fn json_root(text: &str) -> Result<jsonc_parser::cst::CstRootNode> {
    let text = if text.trim().is_empty() { "{}" } else { text };
    jsonc_parser::cst::CstRootNode::parse(text, &jsonc_parser::ParseOptions::default())
        .map_err(|error| anyhow::anyhow!("{error}"))
}

fn json_with(text: &str, container: &str, shape: Shape, launch: &Launch) -> Result<String> {
    let root = json_root(text)?;
    let Some(servers) = root.object_value_or_set().object_value_or_create(container) else {
        bail!("its `{container}` is not an object");
    };
    let entry = json_entry(shape, launch);
    match servers.get(SERVER) {
        Some(existing) => existing.set_value(entry),
        None => {
            servers.append(SERVER, entry);
        }
    }
    Ok(with_final_newline(root.to_string()))
}

fn json_without(text: &str, container: &str) -> Result<String> {
    let root = json_root(text)?;
    if let Some(entry) = root
        .object_value()
        .and_then(|object| object.object_value(container))
        .and_then(|servers| servers.get(SERVER))
    {
        entry.remove();
    }
    Ok(with_final_newline(root.to_string()))
}

fn toml_with(text: &str, container: &str, launch: &Launch) -> Result<String> {
    let mut document = text.parse::<toml_edit::DocumentMut>()?;
    let servers = document
        .entry(container)
        .or_insert_with(|| {
            let mut table = toml_edit::Table::new();
            table.set_implicit(true);
            toml_edit::Item::Table(table)
        })
        .as_table_mut()
        .with_context(|| format!("its `{container}` is not a table"))?;
    let mut entry = toml_edit::Table::new();
    entry.insert("command", toml_edit::value(launch.program.as_str()));
    entry.insert(
        "args",
        toml_edit::value(
            launch
                .args
                .iter()
                .map(String::as_str)
                .collect::<toml_edit::Array>(),
        ),
    );
    servers.insert(SERVER, toml_edit::Item::Table(entry));
    Ok(document.to_string())
}

fn toml_without(text: &str, container: &str) -> Result<String> {
    let mut document = text.parse::<toml_edit::DocumentMut>()?;
    if let Some(servers) = document
        .get_mut(container)
        .and_then(toml_edit::Item::as_table_mut)
    {
        servers.remove(SERVER);
    }
    Ok(document.to_string())
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
                variable: |_| None,
                windows: cfg!(windows),
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
            installed: |folders| vec![folders.home.join(".test")],
            then: None,
        }
    }

    fn toml_client() -> Client {
        Client {
            file: |folders| folders.home.join(".test").join("config.toml"),
            ..client(Format::Toml {
                container: "mcp_servers",
            })
        }
    }

    const PLAIN: Format = Format::Json {
        container: "mcpServers",
        shape: Shape::Plain,
    };

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
        let file = (client.file)(&folders);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let before = "{\n  // the person's own note\n  \"theme\": \"dark\",\n  \"mcpServers\": {\n    \"other\": { \"command\": \"other\", \"args\": [] }, // trailing\n  },\n}\n";
        std::fs::write(&file, before).unwrap();

        add(&client, &folders, &launch("/opt/steamgauge")).unwrap();
        let after = read(&client, &folders);
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
        let client = client(PLAIN);
        let file = (client.file)(&folders);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{ \"mcpServers\": { oops").unwrap();
        assert!(matches!(
            state(&client, &folders, &launch("/opt/steamgauge")),
            State::Unreadable { .. }
        ));
        let refusal = add(&client, &folders, &launch("/opt/steamgauge")).unwrap_err();
        assert!(refusal.to_string().contains("does not parse"), "{refusal}");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "{ \"mcpServers\": { oops"
        );
    }

    #[test]
    fn each_shape_is_written_the_way_its_client_reads_it() {
        let scratch = Scratch::new("shapes");
        let folders = scratch.folders();
        let typed = client(Format::Json {
            container: "servers",
            shape: Shape::Typed,
        });
        add(&typed, &folders, &launch("/opt/steamgauge")).unwrap();
        assert_eq!(
            json(&read(&typed, &folders)),
            serde_json::json!({"servers": {"steamgauge": {
                "type": "stdio", "command": "/opt/steamgauge", "args": ["mcp"]
            }}})
        );

        let local = Client {
            file: |folders| folders.home.join("opencode.json"),
            ..client(Format::Json {
                container: "mcp",
                shape: Shape::Local,
            })
        };
        add(&local, &folders, &launch("/opt/steamgauge")).unwrap();
        assert_eq!(
            json(&read(&local, &folders)),
            serde_json::json!({"mcp": {"steamgauge": {
                "type": "local", "command": ["/opt/steamgauge", "mcp"], "enabled": true
            }}})
        );
        assert_eq!(
            state(&local, &folders, &launch("/opt/steamgauge")),
            State::Added
        );
    }

    #[test]
    fn a_toml_entry_keeps_the_rest_of_the_file() {
        let scratch = Scratch::new("toml");
        let folders = scratch.folders();
        let client = toml_client();
        let file = (client.file)(&folders);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            "# the person's own note\nmodel = \"o4\"\n\n[mcp_servers.other]\ncommand = \"other\"\n",
        )
        .unwrap();

        add(&client, &folders, &launch("/opt/steamgauge")).unwrap();
        let after = read(&client, &folders);
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
