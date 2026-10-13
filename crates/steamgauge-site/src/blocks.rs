//! The parts of the README and `docs/` written from the program's own tables, each between a
//! marker naming it and an end marker. Both are HTML comments, so the files read whole on GitHub
//! too.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, Command};
use steamgauge_app::reference;

use crate::PAGES;

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
        "tools" => Ok(tools()),
        "commands" => Ok(commands()),
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
    for page in PAGES.iter().filter(|page| page.file != "index") {
        let source =
            std::fs::read_to_string(root.join(page.source)).with_context(|| page.source)?;
        let title = source
            .lines()
            .find_map(|line| line.strip_prefix("# "))
            .with_context(|| format!("{} has no top-level heading", page.source))?;
        text.push_str(&wrap(
            &format!("[{title}]({}): {}.", page.source, page.description),
            "- ",
            "  ",
        ));
        text.push('\n');
    }
    Ok(text)
}

/// Each MCP client the app adds itself to, with the file it writes on each system.
fn clients() -> String {
    let mut text = String::new();
    for client in reference::clients() {
        let files = if client.macos == client.linux {
            format!(
                "`{}` on Windows, `{}` on macOS and Linux",
                client.windows, client.macos
            )
        } else {
            format!(
                "`{}` on Windows, `{}` on macOS, `{}` on Linux",
                client.windows, client.macos, client.linux
            )
        };
        let then = client
            .then
            .map(|then| format!(" {}", prose(then)))
            .unwrap_or_default();
        text.push_str(&wrap(
            &format!(
                "**{}**, `connect {}`: {files}.{then}",
                client.name, client.id
            ),
            "- ",
            "  ",
        ));
        text.push('\n');
    }
    text
}

/// Each tool the MCP server offers, with what it is given.
fn tools() -> String {
    let mut text = String::new();
    for tool in reference::tools() {
        let mut said = format!("`{}`: {}", tool.name, full_stop(&prose(&tool.description)));
        if tool.read_only {
            said.push_str(" It only looks.");
        }
        if tool.destructive {
            said.push_str(" What it removes has to be downloaded or worked out again.");
        }
        text.push_str(&wrap(&said, "- ", "  "));
        text.push('\n');
        let required: Vec<&str> = tool.input["required"]
            .as_array()
            .map(|names| names.iter().filter_map(|name| name.as_str()).collect())
            .unwrap_or_default();
        if let Some(properties) = tool.input["properties"].as_object() {
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
                text.push_str(&wrap(&said, "  - ", "    "));
                text.push('\n');
                if values.iter().any(|(_, about)| about.is_some()) {
                    for (value, about) in values {
                        let about = about.map(|about| format!(": {}", full_stop(&prose(&about))));
                        text.push_str(&wrap(
                            &format!("`{value}`{}", about.unwrap_or_default()),
                            "    - ",
                            "      ",
                        ));
                        text.push('\n');
                    }
                }
            }
        }
    }
    text
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

/// The command line, as `steamgauge --help` and each command's `--help` give it.
fn commands() -> String {
    let mut command = reference::command();
    command.build();
    let mut text = String::new();
    if let Some(after) = command.get_after_long_help().or(command.get_after_help()) {
        paragraphs(&mut text, &after.to_string());
    }
    let shown: Vec<&Command> = subcommands(&command).collect();
    for command in &shown {
        let about = command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default();
        text.push_str(&wrap(
            &format!(
                "[`{0}`](#{0}): {1}",
                command.get_name(),
                full_stop(&prose(&about))
            ),
            "- ",
            "  ",
        ));
        text.push('\n');
    }
    for command in shown {
        text.push('\n');
        section(&mut text, command, 2);
    }
    text
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
        let commands = commands();
        assert!(commands.contains("## `crawl`\n"));
        assert!(commands.contains("steamgauge crawl"));
        assert!(commands.contains("`-o, --out <OUT>`"));
        assert!(!commands.contains("--help"));
        assert!(
            commands
                .lines()
                .all(|line| line.starts_with("steamgauge") || line.chars().count() <= WIDTH)
        );
        let tools = tools();
        assert!(tools.contains("- `connect_client`: "));
        assert!(tools.contains("  - `page`, required: Which page of the window to show.\n"));
        assert!(
            tools.contains("    - `cockpit`: What moved, what is running and what needs a look.\n")
        );
        assert!(tools.contains("    - `settings`: How the app runs"));
    }
}
