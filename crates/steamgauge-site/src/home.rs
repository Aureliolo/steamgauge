//! The front page: what the program does, the window, three things it holds itself to, and the
//! way in. It is looked at rather than read, so it is made here rather than written as Markdown,
//! and its Markdown twin says the same in prose.

use std::fmt::Write as _;

use steamgauge_app::reference;

use crate::{REPOSITORY, escape};

const HEADLINE: &str = "Read every review of a game.";

const LEDE: &str = "SteamGauge downloads every Steam review of a game and reads each one on your \
                    own computer: what players praise, what they criticise, how often, and the \
                    reviews behind every figure.";

struct Card {
    kicker: &'static str,
    title: &'static str,
    body: &'static str,
    link: &'static str,
    link_text: &'static str,
}

const CARDS: [Card; 3] = [
    Card {
        kicker: "The census",
        title: "Every review",
        body: "Every review Steam serves for a game, in every language. Each game shows how much \
               of Valve's own total it holds.",
        link: "how-it-works.html#every-review",
        link_text: "What it downloads",
    },
    Card {
        kicker: "The reader",
        title: "Read on your computer",
        body: "Each review is split into the points it makes, and each point is filed under one \
               of 26 subjects as praise or complaint. A point the reader is unsure of stays \
               unfiled. No account, no key, no upload.",
        link: "how-it-works.html#reading-a-review",
        link_text: "How it reads",
    },
    Card {
        kicker: "The evidence",
        title: "Every figure opens",
        body: "Any number opens onto the reviews behind it, and every subject carries how often \
               the reader gets it right on games it never trained on.",
        link: "accuracy.html",
        link_text: "Accuracy",
    },
];

/// Where a way's command or note names the version, filled in with the one being built.
const VERSION: &str = "VERSION";

/// Where a way names the release's downloads.
const RELEASE: &str = "RELEASE";

struct Way {
    id: &'static str,
    name: &'static str,
    command: &'static str,
    note: &'static str,
}

const WAYS: [Way; 5] = [
    Way {
        id: "winget",
        name: "Windows",
        command: "winget install Aureliolo.SteamGauge",
        note: "Windows 10 or 11 on x64. The setup installs for your user alone, with no \
               administrator rights. Reads on any graphics card with DirectX 12, or on the \
               processor.",
    },
    Way {
        id: "scoop",
        name: "Scoop",
        command: "scoop bucket add aureliolo https://github.com/Aureliolo/steamgauge\n\
                  scoop install aureliolo/steamgauge",
        note: "Windows: the portable program, with a Start menu shortcut and `steamgauge` on the \
               `PATH`.",
    },
    Way {
        id: "homebrew",
        name: "macOS",
        command: "brew tap aureliolo/steamgauge https://github.com/Aureliolo/steamgauge\n\
                  brew install --cask aureliolo/steamgauge/steamgauge",
        note: "Apple silicon. Reads through Core ML.",
    },
    Way {
        id: "debian",
        name: "Debian, Ubuntu",
        command: "curl -fLO RELEASE/steamgauge_VERSION-1_amd64.deb\n\
                  sudo apt install ./steamgauge_VERSION-1_amd64.deb",
        note: "x86-64 with glibc 2.39 or later: Ubuntu 24.04, Debian 13. Reads on the processor.",
    },
    Way {
        id: "fedora",
        name: "Fedora",
        command: "sudo dnf install RELEASE/steamgauge-VERSION-1.x86_64.rpm",
        note: "x86-64, Fedora 40 or later. Reads on the processor.",
    },
];

/// Shown first, for the system most people read Steam reviews on.
const FIRST: &str = "winget";

fn filled(text: &str, version: &str) -> String {
    text.replace(
        RELEASE,
        &format!("{REPOSITORY}/releases/download/v{VERSION}"),
    )
    .replace(VERSION, version)
}

const IN_THE_WINDOW: [&str; 3] = [
    "**Add a game** by its name, its store link or its app ID. Every review downloads in the \
     background.",
    "**Read it.** The largest reader the graphics card has room for files every point: minutes \
     on a card, longer on the processor.",
    "**Open a subject** to see its praise, its complaints, the words each side uses, and the \
     reviews behind every figure.",
];

const IN_A_TERMINAL: &str = "\
# every review of Hades II, into ./data
steamgauge crawl 1145350
# every point in them, read on this computer
steamgauge read 1145350
# one page with every figure and the reviews behind it
steamgauge report 1145350";

const SMALLPRINT: &str = "Apache-2.0. Not affiliated with or endorsed by Valve; Steam is a \
                          trademark of Valve Corporation.";

/// The window as the page shows it, in each colour scheme.
#[derive(Debug)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
}

/// The front page, as the site's HTML.
#[must_use]
pub fn html(version: &str, picture: &Picture) -> String {
    let clients = reference::clients().len();
    let tools = reference::tools().len();
    let mut page = format!(
        "<section class=\"hero\">\n<h1 id=\"steamgauge\">{HEADLINE}</h1>\n\
         <p class=\"lede\">{}</p>\n\
         <p class=\"actions\"><a class=\"button\" href=\"#install\">Install</a>\
         <a class=\"button quiet\" href=\"sample-report.html\">See a sample report</a></p>\n\
         </section>\n\
         <figure class=\"window\">\n<picture>\n\
         <source srcset=\"window-dark.png\" media=\"(prefers-color-scheme: dark)\">\n\
         <img src=\"window-light.png\" width=\"{}\" height=\"{}\" alt=\"The SteamGauge \
         window's cockpit: how many games and reviews are held, and what moved in each game \
         since the last look.\">\n</picture>\n\
         <figcaption>The cockpit, with invented games.</figcaption>\n</figure>\n\
         <div class=\"cards\">\n",
        escape(LEDE),
        picture.width,
        picture.height
    );
    for card in CARDS {
        let _ = writeln!(
            page,
            "<div class=\"card\"><p class=\"kicker\">{}</p><h2>{}</h2><p>{}</p>\
             <a class=\"go\" href=\"{}\">{}</a></div>",
            card.kicker,
            escape(card.title),
            inline(card.body),
            card.link,
            card.link_text
        );
    }
    let _ = write!(
        page,
        "</div>\n<section class=\"step\">\n<h2 id=\"install\">Install</h2>\n{}\n\
         <p class=\"note\">Every download is checked against the SHA-256 the release is signed \
         over. <a href=\"install.html\">Install</a> has the archives, the first launch and \
         upgrading.</p>\n</section>\n\
         <section class=\"step two\">\n<div>\n<h2 id=\"first-game\">A first game</h2>\n<ol>\n",
        picker(version)
    );
    for step in IN_THE_WINDOW {
        let _ = writeln!(page, "<li>{}</li>", inline(step));
    }
    let _ = write!(
        page,
        "</ol>\n</div>\n<div>\n<h2 id=\"in-a-terminal\">Or in a terminal</h2>\n\
         <pre tabindex=\"0\"><code>{}</code></pre>\n\
         <p class=\"note\"><a href=\"commands.html\">The command line</a> has every \
         command.</p>\n</div>\n</section>\n\
         <section class=\"step\">\n<h2 id=\"assistants\">From an AI assistant</h2>\n\
         <p>The app is an MCP server with {tools} tools: an assistant can find, download and \
         read games, and ask what their players say. Settings adds it to any of {clients} \
         assistants with one click, or from a terminal:</p>\n\
         <pre tabindex=\"0\"><code>steamgauge connect claude-desktop</code></pre>\n\
         <p class=\"note\"><a href=\"assistants.html\">AI assistants</a> lists them and the \
         command for any other.</p>\n</section>\n\
         <p class=\"smallprint\">{}</p>\n",
        commented(IN_A_TERMINAL),
        escape(SMALLPRINT)
    );
    page
}

/// The ways to install, one chip each, and the command for the one chosen: radio inputs and
/// sibling selectors, so choosing needs no script, which the site's content security policy
/// forbids. The rules tying each input to its chip and its panel are written with them, so a
/// way added above cannot outrun the stylesheet.
fn picker(version: &str) -> String {
    let mut inputs = String::new();
    let mut chips = String::new();
    let mut panels = String::new();
    let mut rules = String::new();
    for way in WAYS {
        let id = way.id;
        let checked = if id == FIRST { " checked" } else { "" };
        let _ = writeln!(
            inputs,
            "<input type=\"radio\" name=\"way\" id=\"way-{id}\" aria-controls=\"panel-{id}\"{checked}>"
        );
        let _ = write!(chips, "<label for=\"way-{id}\">{}</label>", way.name);
        let _ = write!(
            panels,
            "<div class=\"panel\" id=\"panel-{id}\"><pre tabindex=\"0\"><code>{}</code></pre>\
             <p>{}</p></div>",
            escape(&filled(way.command, version)),
            inline(&filled(way.note, version))
        );
        let _ = write!(
            rules,
            "#way-{id}:checked~.panels #panel-{id}{{display:block}}\
             #way-{id}:checked~.chips label[for=\"way-{id}\"]{{background:var(--accent);\
             border-color:var(--accent);color:var(--accent-ink)}}\
             #way-{id}:focus-visible~.chips label[for=\"way-{id}\"]{{outline:3px solid \
             var(--accent);outline-offset:2px}}"
        );
    }
    format!(
        "<div class=\"picker\">\n{inputs}<div class=\"chips\">{chips}</div>\n\
         <div class=\"panels\">{panels}</div>\n</div>\n<style-rules>{rules}</style-rules>"
    )
}

/// The picker's rules, which the build moves into the stylesheet: the pages carry no inline
/// style, since the content security policy allows only the site's own stylesheet.
#[must_use]
pub fn rules(page: &str) -> (String, String) {
    match page.split_once("<style-rules>") {
        Some((before, rest)) => {
            let (rules, after) = rest.split_once("</style-rules>").unwrap_or((rest, ""));
            (format!("{before}{after}"), rules.to_owned())
        }
        None => (page.to_owned(), String::new()),
    }
}

/// A shell snippet with its comments marked, so the stylesheet can set them apart.
fn commented(code: &str) -> String {
    code.lines()
        .map(|line| {
            if line.starts_with('#') {
                format!("<span class=\"comment\">{}</span>", escape(line))
            } else {
                escape(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The little Markdown the cards and notes use: `code` and **strong**.
fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut code = false;
    let mut strong = false;
    let mut rest = escape(text);
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**") {
            out.push_str(if strong { "</strong>" } else { "<strong>" });
            strong = !strong;
            rest = after.to_owned();
        } else if let Some(after) = rest.strip_prefix('`') {
            out.push_str(if code { "</code>" } else { "<code>" });
            code = !code;
            rest = after.to_owned();
        } else {
            let c = rest.chars().next().unwrap_or_default();
            out.push(c);
            rest = rest[c.len_utf8()..].to_owned();
        }
    }
    out
}

/// The front page as Markdown.
#[must_use]
pub fn text(version: &str) -> String {
    let mut page = format!("# SteamGauge\n\n{HEADLINE} {LEDE}\n\n");
    for card in CARDS {
        let _ = write!(
            page,
            "**{}.** {} See [{}]({}).\n\n",
            card.title,
            card.body,
            card.link_text,
            card.link.replace(".html", ".md")
        );
    }
    page.push_str("## Install\n");
    for way in WAYS {
        let _ = write!(
            page,
            "\n{}: {}\n\n```sh\n{}\n```\n",
            way.name,
            filled(way.note, version),
            filled(way.command, version)
        );
    }
    page.push_str("\n## A first game\n\n");
    for (at, step) in IN_THE_WINDOW.iter().enumerate() {
        let _ = writeln!(page, "{}. {step}", at + 1);
    }
    let _ = write!(
        page,
        "\nOr in a terminal:\n\n```sh\n{IN_A_TERMINAL}\n```\n\n## From an AI assistant\n\n\
         The app is an MCP server. Settings adds it to an assistant, or `steamgauge connect \
         claude-desktop` does from a terminal. See [AI assistants](assistants.md).\n\n\
         {SMALLPRINT}\n"
    );
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_to_install_has_a_chip_a_panel_and_its_rules() {
        let (page, rules) = rules(&picker("9.8.7"));
        for way in WAYS {
            let id = way.id;
            assert!(page.contains(&format!("id=\"way-{id}\"")), "{id}");
            assert!(page.contains(&format!("<label for=\"way-{id}\">")), "{id}");
            assert!(page.contains(&format!("id=\"panel-{id}\"")), "{id}");
            assert!(
                rules.contains(&format!(
                    "#way-{id}:checked~.panels #panel-{id}{{display:block}}"
                )),
                "{id}"
            );
        }
        assert_eq!(page.matches(" checked>").count(), 1);
        assert!(!page.contains("style"), "{page}");
        assert!(page.contains(
            "https://github.com/Aureliolo/steamgauge/releases/download/v9.8.7/\
             steamgauge_9.8.7-1_amd64.deb"
        ));
        assert!(!page.contains(VERSION) && !page.contains(RELEASE));
    }

    #[test]
    fn the_front_page_links_onward_and_its_twin_says_the_same() {
        let page = html(
            "9.8.7",
            &Picture {
                width: 10,
                height: 20,
            },
        );
        for card in CARDS {
            assert!(
                page.contains(&format!("href=\"{}\"", card.link)),
                "{}",
                card.link
            );
        }
        assert!(page.contains("width=\"10\" height=\"20\""));
        let text = text("9.8.7");
        assert!(text.starts_with("# SteamGauge\n\n"), "{text}");
        assert!(text.contains("winget install Aureliolo.SteamGauge"));
        assert!(text.contains("See [Accuracy](accuracy.md)."));
    }

    #[test]
    fn the_little_markdown_becomes_code_and_strong() {
        assert_eq!(
            inline("**Add** `a` <b>"),
            "<strong>Add</strong> <code>a</code> &lt;b&gt;"
        );
    }
}
