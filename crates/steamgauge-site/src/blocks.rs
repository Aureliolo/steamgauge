//! The parts of the README and `docs/` written from the program's own tables, each between a
//! marker naming it and an end marker. Both are HTML comments, so the files read whole on GitHub
//! too.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, Command};
use steamgauge_app::reference;

use crate::PAGES;
use crate::picker::{self, Choice};

const OPEN: &str = "<!-- steamgauge-site: ";
const SHUT: &str = " -->";
const END: &str = "<!-- steamgauge-site: end -->";

/// Prose wraps at 100 columns, as `.markdownlint-cli2.jsonc` holds every Markdown file to.
const WIDTH: usize = 100;

/// `text` with every generated part written afresh. `holder` is the file it came from.
pub fn fill(root: &Path, holder: &str, text: &str) -> Result<String> {
    let mut filled = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        let named = &rest[at + OPEN.len()..];
        let length = named
            .find(SHUT)
            .with_context(|| format!("{holder}: a marker that is never closed"))?;
        let name = &named[..length];
        if name == "end" {
            bail!("{holder}: an end marker with no part open");
        }
        let body = at + OPEN.len() + length + SHUT.len();
        let end = rest[body..]
            .find(END)
            .with_context(|| format!("{holder}: the part `{name}` is never ended"))?
            + body;
        filled.push_str(&rest[..body]);
        filled.push_str("\n\n");
        filled.push_str(generate(root, holder, name)?.trim_end());
        filled.push_str("\n\n");
        filled.push_str(END);
        rest = &rest[end + END.len()..];
    }
    filled.push_str(rest);
    Ok(filled)
}

fn generate(root: &Path, holder: &str, name: &str) -> Result<String> {
    match name {
        "pages" => pages(root, holder),
        "clients" => Ok(clients()),
        "entries" => Ok(entries()),
        "paste" => Ok(format!("```text\n{PASTE}\n```")),
        "tools" => tools(),
        "commands" => commands(),
        _ => bail!("{holder}: no generated part is called `{name}`"),
    }
}

/// The README's list of the documentation's pages.
fn pages(root: &Path, holder: &str) -> Result<String> {
    if holder != "README.md" {
        bail!(
            "{holder}: the list of pages links from the repository root, so only the README holds it"
        );
    }
    let mut text = String::new();
    for (page, path) in PAGES
        .iter()
        .filter(|page| page.nav)
        .filter_map(|page| page.markdown().map(|path| (page, path)))
    {
        let source = std::fs::read_to_string(root.join(path)).with_context(|| path)?;
        let title = source
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .with_context(|| format!("{path} has no top-level heading"))?;
        text.push_str(&wrap(
            &format!("[{title}]({path}): {}.", page.description),
            "- ",
            "  ",
        ));
        text.push('\n');
    }
    Ok(text)
}

/// What a person pastes to an assistant to have it add the app to itself.
pub const PASTE: &str = "Add SteamGauge to yourself by following \
                         https://aureliolo.github.io/steamgauge/agent.md,\nthen tell me which \
                         file you changed.";

/// An assistant the app cannot add itself to, and how it takes the server.
struct Other {
    id: &'static str,
    name: &'static str,
    /// What to type or paste, with the program as `reference::PROGRAM`.
    code: &'static str,
    /// Where it goes.
    note: &'static str,
}

const OTHERS: &[Other] = &[
    Other {
        id: "jetbrains",
        name: "JetBrains AI Assistant",
        code: "{\"mcpServers\": {\"steamgauge\": {\"command\": \"<path to steamgauge>\", \
               \"args\": [\"mcp\"]}}}",
        note: "Settings, Tools, AI Assistant, Model Context Protocol (MCP), Add, at the Global \
               level.",
    },
    Other {
        id: "trae",
        name: "Trae",
        code: "{\"mcpServers\": {\"steamgauge\": {\"command\": \"<path to steamgauge>\", \
               \"args\": [\"mcp\"]}}}",
        note: "Settings, MCP, Add, Raw Config (JSON).",
    },
    Other {
        id: "qoder-ide",
        name: "Qoder",
        code: "{\"mcpServers\": {\"steamgauge\": {\"command\": \"<path to steamgauge>\", \
               \"args\": [\"mcp\"]}}}",
        note: "Settings, MCP, My Servers, Add. Qoder CLI is added by `steamgauge connect qoder`.",
    },
    Other {
        id: "command-code",
        name: "Command Code",
        code: "cmd mcp add --scope user steamgauge -- \"<path to steamgauge>\" mcp",
        note: "In a terminal.",
    },
    Other {
        id: "fast-agent",
        name: "fast-agent",
        code: "mcp:\n  servers:\n    steamgauge:\n      command: <path to steamgauge>\n      \
               args: [mcp]",
        note: "In the `fast-agent.yaml` of the folder it runs in, and in the agent's `servers`.",
    },
    Other {
        id: "other",
        name: "Something else",
        code: "\"<path to steamgauge>\" mcp",
        note: "Any assistant that starts an MCP server from a command. `steamgauge clients` \
               prints this copy's path.",
    },
];

/// One assistant, as the page offers it.
struct Assistant {
    id: String,
    name: String,
    code: String,
    note: String,
}

/// Every assistant: first those the app adds itself to, each by `connect`, then the others.
fn assistants() -> Vec<Assistant> {
    let mut all: Vec<Assistant> = reference::clients()
        .into_iter()
        .map(|client| {
            let files = if client.macos == client.linux {
                format!(
                    "`{}` on macOS and Linux, `{}` on Windows",
                    client.macos, client.windows
                )
            } else {
                format!(
                    "`{}` on macOS, `{}` on Linux, `{}` on Windows",
                    client.macos, client.linux, client.windows
                )
            };
            let then = client
                .then
                .map(|then| format!(" {then}"))
                .unwrap_or_default();
            let does = match &client.runs {
                Some(runs) => format!("Runs `{runs}`"),
                None => format!("Writes {files}"),
            };
            Assistant {
                id: client.id.to_owned(),
                name: client.name.to_owned(),
                code: format!("steamgauge connect {}", client.id),
                note: format!("{does}.{then}"),
            }
        })
        .collect();
    all.extend(OTHERS.iter().map(|other| Assistant {
        id: other.id.to_owned(),
        name: other.name.to_owned(),
        code: other.code.to_owned(),
        note: other.note.to_owned(),
    }));
    all
}

/// Every assistant as a list, which is how the page reads on GitHub; the site shows the same as
/// a picker, from `clients_picker`.
fn clients() -> String {
    let mut text = String::new();
    for assistant in assistants() {
        let said = if assistant.code.contains('\n') {
            format!("**{}**: {}", assistant.name, assistant.note)
        } else {
            format!(
                "**{}**: `{}`. {}",
                assistant.name, assistant.code, assistant.note
            )
        };
        text.push_str(&wrap(&said, "- ", "  "));
        text.push('\n');
        if assistant.code.contains('\n') {
            let _ = write!(text, "\n  ```yaml\n");
            for line in assistant.code.lines() {
                let _ = writeln!(text, "  {line}");
            }
            text.push_str("  ```\n\n");
        }
    }
    text
}

/// Assistants one maker ships in more than one form, offered as one chip, the forms in this
/// order, each under its own name.
const FAMILIES: &[(&str, &str, &[&str])] = &[
    ("claude", "Claude", &["claude-code", "claude-desktop"]),
    ("devin", "Devin and Windsurf", &["devin", "windsurf"]),
    ("tabnine", "Tabnine", &["tabnine", "tabnine-ide"]),
];

/// Every assistant as a row of chips, the command for the one chosen beneath them.
#[must_use]
pub fn clients_picker() -> String {
    let all = assistants();
    let mut choices: Vec<Choice> = Vec::new();
    for assistant in &all {
        if let Some((id, name, members)) = FAMILIES
            .iter()
            .find(|(_, _, members)| members.contains(&assistant.id.as_str()))
        {
            if choices.iter().any(|choice| choice.id == *id) {
                continue;
            }
            let forms: Vec<&Assistant> = members
                .iter()
                .filter_map(|member| all.iter().find(|one| one.id == *member))
                .collect();
            choices.push(Choice {
                id: (*id).to_owned(),
                name: (*name).to_owned(),
                code: forms
                    .iter()
                    .map(|form| format!("# {}\n{}", form.name, form.code))
                    .collect::<Vec<_>>()
                    .join("\n"),
                note: forms
                    .iter()
                    .map(|form| format!("**{}**: {}", form.name, form.note))
                    .collect::<Vec<_>>()
                    .join(" "),
            });
            continue;
        }
        choices.push(Choice {
            id: assistant.id.clone(),
            name: assistant.name.clone(),
            code: assistant.code.clone(),
            note: assistant.note.clone(),
        });
    }
    picker::html("assistant", &choices)
}

/// The site's form of each generated part that has one, in place of its Markdown.
#[must_use]
pub fn for_the_site(html: &str) -> String {
    let mut html = html.to_owned();
    for (name, site) in [
        ("clients", clients_picker as fn() -> String),
        ("entries", entries_picker),
    ] {
        let open = format!("{OPEN}{name}{SHUT}");
        let Some(at) = html.find(&open) else {
            continue;
        };
        let Some(length) = html[at..].find(END) else {
            continue;
        };
        html = format!(
            "{}{}{}",
            &html[..at],
            site(),
            &html[at + length + END.len()..]
        );
    }
    html
}

/// The entry `connect` writes, once for each shape the assistants keep it in, with the
/// assistants that keep it so.
fn shapes() -> Vec<(Vec<&'static str>, String, &'static str)> {
    let mut shapes: Vec<(Vec<&'static str>, String, &'static str)> = Vec::new();
    for client in reference::clients() {
        let Some(entry) = client.entry else {
            continue;
        };
        match shapes.iter_mut().find(|(_, written, _)| *written == entry) {
            Some((names, _, _)) => names.push(client.name),
            None => shapes.push((vec![client.name], entry, client.language)),
        }
    }
    shapes
}

/// Each shape of entry as a chip named for who keeps it, the entry beneath.
fn entries_picker() -> String {
    let choices: Vec<Choice> = shapes()
        .into_iter()
        .enumerate()
        .map(|(at, (names, entry, _))| Choice {
            id: at.to_string(),
            name: if names.len() > 2 {
                format!("{} and {} more", names[0], names.len() - 1)
            } else {
                listed_and(&names)
            },
            code: entry,
            note: format!("{}.", listed_and(&names)),
        })
        .collect();
    picker::html("entry", &choices)
}

/// The entry `connect` writes, once for each shape the assistants keep it in.
fn entries() -> String {
    let mut text = String::new();
    for (names, entry, language) in shapes() {
        text.push_str(&wrap(&format!("{}:", listed_and(&names)), "", ""));
        let _ = write!(text, "\n\n```{language}\n{entry}\n```\n\n");
    }
    text
}

fn listed_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The tools in the groups the reference shows them in. A tool the server gains is refused
/// until it is placed here, so the page never lists a tool under no heading.
const TOOL_GROUPS: &[(&str, &[&str])] = &[
    (
        "The library",
        &[
            "overview",
            "library",
            "games",
            "find_games",
            "look_up",
            "art",
            "groups",
            "save_groups",
            "since_last_look",
            "looked",
            "show",
        ],
    ),
    (
        "A read game",
        &[
            "reading",
            "claims_behind",
            "search_game",
            "who_wrote",
            "induced",
            "game_updates",
            "before_after",
            "compare",
        ],
    ),
    (
        "Search by meaning",
        &["meaning_offer", "choose_meaning", "search_by_meaning"],
    ),
    (
        "Jobs",
        &[
            "work",
            "read_offer",
            "queue",
            "queue_reads",
            "queue_updates",
            "stop_job",
            "clear_finished",
            "open_report",
            "export_data",
            "export_report",
        ],
    ),
    (
        "Settings and storage",
        &[
            "settings",
            "save_settings",
            "reader_options",
            "storage",
            "free_room",
            "remove_model",
            "new_http_token",
            "connect_client",
            "disconnect_client",
        ],
    ),
    (
        "Updates",
        &[
            "about",
            "check_for_updates",
            "newer_version",
            "update_state",
            "update_now",
            "show_update_file",
        ],
    ),
];

/// Each tool the MCP server offers, by group, with what it is given.
fn tools() -> Result<String> {
    let tools = reference::tools();
    for tool in &tools {
        if !TOOL_GROUPS
            .iter()
            .any(|(_, names)| names.contains(&tool.name.as_str()))
        {
            bail!(
                "the MCP tool `{}` is in no group: add it to TOOL_GROUPS",
                tool.name
            );
        }
    }
    let mut text = String::new();
    for (group, names) in TOOL_GROUPS {
        let _ = write!(text, "## {group}\n\n");
        for name in *names {
            let tool = tools
                .iter()
                .find(|tool| tool.name == *name)
                .with_context(|| format!("TOOL_GROUPS names `{name}`, which the server lacks"))?;
            tool_card(&mut text, tool);
        }
    }
    Ok(text)
}

fn tool_card(text: &mut String, tool: &reference::Tool) {
    let _ = write!(text, "### `{}`\n\n", tool.name);
    let mut said = full_stop(&prose(&tool.description));
    if tool.read_only {
        said.push_str(" Only looks.");
    }
    if tool.destructive {
        said.push_str(" What it removes has to be downloaded or worked out again.");
    }
    text.push_str(&wrap(&said, "", ""));
    text.push_str("\n\n");
    let required: Vec<&str> = tool.input["required"]
        .as_array()
        .map(|names| names.iter().filter_map(|name| name.as_str()).collect())
        .unwrap_or_default();
    let Some(properties) = tool.input["properties"].as_object() else {
        return;
    };
    if properties.is_empty() {
        return;
    }
    for (name, schema) in properties {
        let mut said = format!("`{name}`");
        if required.contains(&name.as_str()) {
            said.push_str(", required");
        }
        if let Some(description) = schema["description"].as_str() {
            said.push_str(": ");
            said.push_str(&full_stop(&prose(description)));
        }
        let values = values(&tool.input, schema);
        if values.iter().all(|(_, about)| about.is_none()) && !values.is_empty() {
            let names: Vec<String> = values
                .iter()
                .map(|(value, _)| format!("`{value}`"))
                .collect();
            let _ = write!(said, " One of {}.", listed(&names));
        }
        text.push_str(&wrap(&said, "- ", "  "));
        text.push('\n');
        if values.iter().any(|(_, about)| about.is_some()) {
            for (value, about) in values {
                let about = about.map(|about| format!(": {}", full_stop(&prose(&about))));
                text.push_str(&wrap(
                    &format!("`{value}`{}", about.unwrap_or_default()),
                    "  - ",
                    "    ",
                ));
                text.push('\n');
            }
        }
    }
    text.push('\n');
}

/// The values a parameter takes, each with what it means where the schema says, when it is one
/// of a fixed few. schemars writes such a type once under `$defs` and refers to it, as itself or
/// as one choice beside `null` for an optional one.
fn values(root: &serde_json::Value, schema: &serde_json::Value) -> Vec<(String, Option<String>)> {
    let resolved = schema["$ref"]
        .as_str()
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .map_or(schema, |name| &root["$defs"][name]);
    if let Some(names) = resolved["enum"].as_array() {
        return names
            .iter()
            .filter_map(|name| name.as_str().map(|name| (name.to_owned(), None)))
            .collect();
    }
    let choices = resolved["oneOf"]
        .as_array()
        .or_else(|| resolved["anyOf"].as_array())
        .cloned()
        .unwrap_or_default();
    choices
        .iter()
        .flat_map(|choice| match choice["const"].as_str() {
            Some(name) => vec![(
                name.to_owned(),
                choice["description"].as_str().map(str::to_owned),
            )],
            None => values(root, choice),
        })
        .collect()
}

/// The commands in the groups the page shows them in: the first two with every option, the
/// last, which only measures and trains the reader, a line each. A command the program gains is
/// refused until it is placed here.
const COMMAND_GROUPS: &[(&str, bool, &[&str])] = &[
    (
        "Downloading and reading",
        true,
        &["crawl", "sweep", "read", "recount", "report", "export-data"],
    ),
    (
        "The app and AI assistants",
        true,
        &["update", "mcp", "clients", "connect", "disconnect"],
    ),
    (
        "Measuring and training the reader",
        false,
        &[
            "claims",
            "embed",
            "store-facts",
            "sample-claims",
            "declined",
            "rare-claims",
            "distinct",
            "ingest-induced",
            "ingest-claims",
            "second-opinion",
            "compare-labels",
            "revisit",
            "ingest-revisit",
            "measure-claims",
            "ceiling",
            "gold",
            "ingest-gold",
            "export-training",
            "export-review-facts",
            "export-pool",
            "brief",
        ],
    ),
];

/// The command line, as `steamgauge --help` and each command's `--help` give it.
fn commands() -> Result<String> {
    let mut command = reference::command();
    command.build();
    for sub in subcommands(&command) {
        if !COMMAND_GROUPS
            .iter()
            .any(|(_, _, names)| names.contains(&sub.get_name()))
        {
            bail!(
                "the command `{}` is in no group: add it to COMMAND_GROUPS",
                sub.get_name()
            );
        }
    }
    let mut text = String::new();
    for (group, whole, names) in COMMAND_GROUPS {
        let _ = write!(text, "## {group}\n\n");
        for (at, name) in names.iter().enumerate() {
            let sub = subcommands(&command)
                .find(|sub| sub.get_name() == *name)
                .with_context(|| {
                    format!("COMMAND_GROUPS names `{name}`, which the program lacks")
                })?;
            if *whole {
                if at > 0 {
                    text.push('\n');
                }
                section(&mut text, sub, 3);
            } else {
                let usage = sub.clone().render_usage().to_string();
                let usage = usage
                    .trim()
                    .trim_start_matches("Usage:")
                    .trim()
                    .trim_start_matches("steamgauge ")
                    .replace(" [OPTIONS]", "");
                let about = sub.get_about().map(ToString::to_string).unwrap_or_default();
                text.push_str(&wrap(
                    &format!("`{usage}`: {}", full_stop(&prose(&about))),
                    "- ",
                    "  ",
                ));
                text.push('\n');
            }
        }
        if !*whole {
            text.push('\n');
            text.push_str(&wrap(
                "Each takes `--help` for its options. \
                 [Accuracy](accuracy.md#how-the-labels-are-made) says where each fits.",
                "",
                "",
            ));
            text.push('\n');
        }
        text.push('\n');
    }
    Ok(text)
}

fn section(text: &mut String, command: &Command, level: usize) {
    let _ = write!(text, "{} `{}`\n\n", "#".repeat(level), command.get_name());
    if let Some(about) = command.get_long_about().or(command.get_about()) {
        paragraphs(text, &about.to_string());
    }
    let usage = command.clone().render_usage().to_string();
    let usage = usage.trim().trim_start_matches("Usage:").trim();
    let _ = write!(text, "```text\n{usage}\n```\n");
    let arguments: Vec<&Arg> = command
        .get_arguments()
        .filter(|arg| {
            !arg.is_hide_set()
                && !matches!(
                    arg.get_action(),
                    ArgAction::Help
                        | ArgAction::HelpShort
                        | ArgAction::HelpLong
                        | ArgAction::Version
                )
        })
        .collect();
    if !arguments.is_empty() {
        text.push('\n');
    }
    for arg in arguments {
        text.push_str(&wrap(&argument(arg), "- ", "  "));
        text.push('\n');
    }
    for sub in subcommands(command) {
        text.push('\n');
        section(text, sub, (level + 1).min(6));
    }
}

/// The commands a person can give, without clap's own `help`.
fn subcommands(command: &Command) -> impl Iterator<Item = &Command> {
    command
        .get_subcommands()
        .filter(|command| !command.is_hide_set() && command.get_name() != "help")
}

/// clap drops the full stop that ends a one-sentence help text, and here a sentence follows it.
fn full_stop(text: &str) -> String {
    let text = text.trim_end();
    if text.is_empty() || text.ends_with(['.', '!', '?', ':']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

fn argument(arg: &Arg) -> String {
    let takes = arg.get_action().takes_values();
    let values = arg.get_value_names().map_or_else(
        || format!("<{}>", arg.get_id().as_str().to_uppercase()),
        |names| {
            names
                .iter()
                .map(|name| format!("<{name}>"))
                .collect::<Vec<_>>()
                .join(" ")
        },
    );
    let label = if arg.is_positional() {
        values
    } else {
        let mut label = String::new();
        if let Some(short) = arg.get_short() {
            let _ = write!(label, "-{short}, ");
        }
        if let Some(long) = arg.get_long() {
            let _ = write!(label, "--{long}");
        }
        if takes {
            label.push(' ');
            label.push_str(&values);
        }
        label
    };
    let mut said = format!("`{label}`");
    if arg.is_required_set() && !arg.is_positional() {
        said.push_str(", required");
    }
    if let Some(help) = arg.get_long_help().or(arg.get_help()) {
        said.push_str(": ");
        said.push_str(&full_stop(&prose(&help.to_string()).replace("\n\n", " ")));
    }
    if takes {
        let choices: Vec<String> = arg
            .get_possible_values()
            .iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| format!("`{}`", value.get_name()))
            .collect();
        if !choices.is_empty() {
            let _ = write!(said, " One of {}.", listed(&choices));
        }
        let defaults: Vec<String> = arg
            .get_default_values()
            .iter()
            .map(|value| format!("`{}`", value.to_string_lossy()))
            .collect();
        if !defaults.is_empty() {
            let _ = write!(said, " Default: {}.", defaults.join(" "));
        }
    }
    said
}

fn listed(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

fn paragraphs(text: &mut String, said: &str) {
    for paragraph in said.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        text.push_str(&wrap(&full_stop(&prose(paragraph)), "", ""));
        text.push_str("\n\n");
    }
}

/// Help text as Markdown: its backticks stay code, and an angle bracket outside them is shown
/// rather than read as HTML.
fn prose(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    let mut code = false;
    for c in text.chars() {
        match c {
            '`' => {
                code = !code;
                shown.push(c);
            }
            '<' if !code => shown.push_str("\\<"),
            '>' if !code => shown.push_str("\\>"),
            _ => shown.push(c),
        }
    }
    shown
}

/// `text` in lines of at most `WIDTH` columns, the first after `first` and the rest after
/// `rest`, never breaking inside a code span nor before a word that would open a list, a
/// heading or a quote at the start of a line.
fn wrap(text: &str, first: &str, rest: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut open = String::new();
    let mut code = false;
    for word in text.split_whitespace() {
        if !open.is_empty() {
            open.push(' ');
        }
        open.push_str(word);
        if word.matches('`').count() % 2 == 1 {
            code = !code;
        }
        if !code {
            words.push(std::mem::take(&mut open));
        }
    }
    if !open.is_empty() {
        words.push(open);
    }
    let mut lines: Vec<Vec<String>> = vec![Vec::new()];
    for word in words {
        let prefix = if lines.len() == 1 { first } else { rest };
        let line = lines.last_mut().expect("one line");
        let length = prefix.chars().count()
            + line
                .iter()
                .map(|word| word.chars().count() + 1)
                .sum::<usize>()
            + word.chars().count();
        if line.is_empty() || length <= WIDTH {
            lines.last_mut().expect("one line").push(word);
            continue;
        }
        let mut next = vec![word];
        if opens_block(&next[0]) {
            let line = lines.last_mut().expect("one line");
            if line.len() > 1 {
                next.insert(0, line.pop().expect("a word"));
            }
        }
        lines.push(next);
    }
    lines
        .iter()
        .enumerate()
        .map(|(at, line)| format!("{}{}", if at == 0 { first } else { rest }, line.join(" ")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A word that, first on a line, would start a list item, a heading or a quote.
fn opens_block(word: &str) -> bool {
    matches!(word, "-" | "+" | "*" | "1." | "1)")
        || word.starts_with('>')
        || (word.len() <= 6 && !word.is_empty() && word.chars().all(|c| c == '#'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    #[test]
    fn a_part_is_written_between_its_markers_and_the_rest_is_left() {
        let text = "before\n\n<!-- steamgauge-site: clients -->\nstale\n<!-- steamgauge-site: end -->\n\nafter\n";
        let filled = fill(root(), "docs/assistants.md", text).unwrap();
        assert!(filled.starts_with("before\n\n<!-- steamgauge-site: clients -->\n\n- **"));
        assert!(filled.ends_with("\n\n<!-- steamgauge-site: end -->\n\nafter\n"));
        assert!(!filled.contains("stale"));
        assert_eq!(fill(root(), "docs/assistants.md", &filled).unwrap(), filled);
    }

    #[test]
    fn every_assistant_is_on_the_list_and_in_the_picker_the_site_swaps_in() {
        let list = clients();
        let picker = clients_picker();
        assert!(
            picker.contains(
                "id=\"pick-assistant-claude\" aria-controls=\"panel-assistant-claude\" checked>"
            ),
            "{picker}"
        );
        assert!(picker.contains(
            "# Claude Code</span>\nsteamgauge connect claude-code\n<span class=\"comment\"># \
             Claude Desktop</span>\nsteamgauge connect claude-desktop"
        ));
        assert!(
            list.contains(
                "`claude mcp add --scope user steamgauge -- \"<path to steamgauge>\" mcp`"
            )
        );
        assert_eq!(
            picker.matches("<label for=\"pick-assistant-claude").count(),
            1
        );
        let in_a_family = |id: &str| FAMILIES.iter().any(|(_, _, members)| members.contains(&id));
        for client in reference::clients() {
            assert!(
                list.contains(&format!("`steamgauge connect {}`", client.id)),
                "{}",
                client.id
            );
            assert!(
                in_a_family(client.id)
                    || picker.contains(&format!("id=\"pick-assistant-{}\"", client.id)),
                "{}",
                client.id
            );
        }
        for other in OTHERS {
            assert!(
                in_a_family(other.id)
                    || picker.contains(&format!("id=\"pick-assistant-{}\"", other.id)),
                "{}",
                other.id
            );
            assert!(other.code.contains(reference::PROGRAM), "{}", other.id);
        }
        let page = "<p>a</p>\n<!-- steamgauge-site: clients -->\n<ul><li>x</li></ul>\n\
                    <!-- steamgauge-site: end -->\n<p>b</p>";
        let swapped = for_the_site(page);
        assert!(
            swapped.starts_with("<p>a</p>\n<div class=\"picker\">"),
            "{swapped}"
        );
        assert!(swapped.ends_with("\n<p>b</p>") && !swapped.contains("<li>x</li>"));
    }

    #[test]
    fn each_shape_of_entry_is_shown_once_with_who_keeps_it() {
        let entries = entries();
        assert!(
            entries.contains("```toml\n[mcp_servers.steamgauge]"),
            "{entries}"
        );
        assert!(entries.contains("Codex:"), "{entries}");
        assert!(entries.contains("```yaml\nextensions:"), "{entries}");
        for client in reference::clients() {
            let Some(entry) = &client.entry else {
                assert!(client.runs.is_some(), "{}", client.id);
                continue;
            };
            assert!(entries.contains(entry.as_str()), "{}", client.id);
            assert!(entries.contains(client.name), "{}", client.id);
        }
    }

    #[test]
    fn an_unknown_or_unended_part_is_refused() {
        assert!(
            fill(
                root(),
                "x.md",
                "<!-- steamgauge-site: nothing -->\n<!-- steamgauge-site: end -->"
            )
            .is_err()
        );
        assert!(fill(root(), "x.md", "<!-- steamgauge-site: tools -->\n").is_err());
        assert!(fill(root(), "x.md", "<!-- steamgauge-site: end -->").is_err());
        assert!(
            fill(
                root(),
                "docs/x.md",
                "<!-- steamgauge-site: pages -->\n<!-- steamgauge-site: end -->"
            )
            .is_err()
        );
    }

    #[test]
    fn lines_wrap_at_the_width_and_never_inside_code_or_before_a_list_marker() {
        let text = format!("{} `a b c` - d", "word ".repeat(30));
        let wrapped = wrap(&text, "- ", "  ");
        assert!(wrapped.lines().all(|line| line.chars().count() <= WIDTH));
        assert!(wrapped.lines().skip(1).all(|line| line.starts_with("  ")));
        assert!(wrapped.contains("`a b c`"));
        assert!(
            !wrapped
                .lines()
                .any(|line| line.trim_start().starts_with("- d"))
        );
    }

    #[test]
    fn angle_brackets_outside_code_are_shown() {
        assert_eq!(prose("a <b> `<c>`"), "a \\<b\\> `<c>`");
    }

    #[test]
    fn every_command_and_tool_is_written_with_what_it_takes() {
        let commands = commands().unwrap();
        assert!(commands.starts_with("## Downloading and reading\n\n### `crawl`\n"));
        assert!(commands.contains("steamgauge crawl"));
        assert!(commands.contains("`-o, --out <OUT>`"));
        assert!(commands.contains("\n- `brief`: "), "{commands}");
        assert!(!commands.contains("`--help`:"));
        assert!(
            commands
                .lines()
                .all(|line| line.starts_with("steamgauge") || line.chars().count() <= WIDTH)
        );
        let tools = tools().unwrap();
        assert!(tools.starts_with("## The library\n\n### `overview`\n"));
        assert!(tools.contains("### `connect_client`\n"));
        assert!(tools.contains("- `page`, required: Which page of the window to show.\n"));
        assert!(
            tools.contains("  - `cockpit`: What moved, what is running and what needs a look.\n")
        );
        assert!(tools.contains("  - `settings`: How the app runs"));
    }
}
