//! The front page: what the program does, the window, three things it holds itself to, and the
//! way in. It is looked at rather than read, so it is made here rather than written as Markdown,
//! and its Markdown twin says the same in prose.

use std::fmt::Write as _;

use steamgauge_app::reference;

use crate::picker::{self, Choice, inline};
use crate::{REPOSITORY, escape};

const HEADLINE: &str = "What players praise and criticise, from every Steam review.";

const LEDE: &str = "SteamGauge downloads a game's reviews and reads each one on your computer. \
                    Every figure opens onto the reviews behind it.";

struct Card {
    title: &'static str,
    body: &'static str,
    link: &'static str,
    link_text: &'static str,
}

const CARDS: [Card; 3] = [
    Card {
        title: "Every review",
        body: "Every review Steam serves for a game, in every language. Each game shows how much \
               of Steam's own total it holds.",
        link: "how-it-works.html#every-review",
        link_text: "What it downloads",
    },
    Card {
        title: "Read on your computer",
        body: "Each review is split into the points it makes, its claims. SteamGauge's model, the \
               reader, files each claim under one of 26 subjects as praise or complaint. A claim \
               it is unsure of stays unfiled. No account, no key, no upload.",
        link: "how-it-works.html#reading-a-review",
        link_text: "How it reads",
    },
    Card {
        title: "Every figure opens",
        body: "Any number opens onto the reviews behind it. Every subject carries how often the \
               reader agrees with labels a language model wrote, on games it never trained on.",
        link: "accuracy.html",
        link_text: "Accuracy",
    },
];

/// The reader sizes table the Accuracy page shows, for the timing the first steps quote.
const READER_SIZES: &str = include_str!("../../../reference/reader-sizes.json");

/// How many claims the largest shipped reader read in one large game on the graphics card, and
/// in how many seconds.
fn card_timing() -> (String, u64) {
    let sizes: serde_json::Value =
        serde_json::from_str(READER_SIZES).expect("reference/reader-sizes.json is JSON");
    let card = sizes["measured"]["card"]
        .as_str()
        .expect("the sizes table says how the card was measured");
    let claims = card
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(" claims)"))
        .map(|(claims, _)| claims.to_owned())
        .expect("the card measurement names its claims as `(N claims)`");
    let seconds = sizes["sizes"]
        .as_array()
        .expect("the sizes table lists sizes")
        .iter()
        .filter(|size| size["ships"].as_bool() == Some(true))
        .filter_map(|size| size["card_seconds"].as_u64())
        .max()
        .expect("a shipped size has a card timing");
    (claims, seconds)
}

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
        id: "windows",
        name: "Windows",
        command: "curl.exe -fLO RELEASE/steamgauge-VERSION-windows-x64-setup.exe\n\
                  .\\steamgauge-VERSION-windows-x64-setup.exe",
        note: "Windows 10 or 11 on x64. The setup installs for your user alone, with no \
               administrator rights. Reads on any graphics card with DirectX 12, or on the \
               processor.",
    },
    Way {
        id: "scoop",
        name: "Scoop",
        command: "scoop bucket add aureliolo https://github.com/Aureliolo/steamgauge\n\
                  scoop install aureliolo/steamgauge",
        note: "The portable Windows program, with a Start menu shortcut and `steamgauge` on the \
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
        note: "x86-64 with glibc 2.39 or later (Ubuntu 24.04, Debian 13). Reads on the processor.",
    },
    Way {
        id: "fedora",
        name: "Fedora",
        command: "sudo dnf install RELEASE/steamgauge-VERSION-1.x86_64.rpm",
        note: "x86-64, Fedora 40 or later. Reads on the processor.",
    },
];

fn filled(text: &str, version: &str) -> String {
    text.replace(
        RELEASE,
        &format!("{REPOSITORY}/releases/download/v{VERSION}"),
    )
    .replace(VERSION, version)
}

fn in_the_window() -> [String; 3] {
    let (claims, seconds) = card_timing();
    [
        "**Add a game** by its name, its store link or its app ID. Every review downloads in the \
         background."
            .to_owned(),
        format!(
            "**Read it.** The largest reader your graphics card holds files every claim. \
             {claims} claims took {seconds} s on an RTX 4090."
        ),
        "**Open a subject** to see its praise, its complaints, the words each side uses, and the \
         reviews behind every figure."
            .to_owned(),
    ]
}

const IN_A_TERMINAL: &str = "\
# every review of Hades II, into ./data
steamgauge crawl 1145350
# every claim in them, read on this computer
steamgauge read 1145350
# one page with every figure and the reviews behind it
steamgauge report 1145350";

const SMALLPRINT: &str = "Apache-2.0. Not affiliated with or endorsed by Valve. Steam is a \
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
    let assistants = crate::blocks::named_assistants();
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
            "<div class=\"card\"><h2>{}</h2><p>{}</p>\
             <a class=\"go\" href=\"{}\">{}</a></div>",
            escape(card.title),
            inline(card.body),
            card.link,
            card.link_text
        );
    }
    let _ = write!(
        page,
        "</div>\n<section class=\"step\">\n<h2 id=\"install\">Install</h2>\n{}\n\
         <p class=\"note\">Scoop and Homebrew check each download against the release's signed \
         <code>SHA256SUMS</code>. <a href=\"install.html\">Install</a> has the archives, the \
         commands that check the other downloads, the first launch and upgrading.</p>\n\
         </section>\n\
         <section class=\"step two\">\n<div>\n<h2 id=\"first-game\">A first game</h2>\n<ol>\n",
        installs(version)
    );
    for step in in_the_window() {
        let _ = writeln!(page, "<li>{}</li>", inline(&step));
    }
    let _ = write!(
        page,
        "</ol>\n</div>\n<div>\n<h2 id=\"in-a-terminal\">Or in a terminal</h2>\n\
         <pre tabindex=\"0\"><code>{}</code></pre>\n\
         <p class=\"note\"><a href=\"commands.html\">The command line</a> has every \
         command.</p>\n</div>\n</section>\n\
         <section class=\"step\">\n<h2 id=\"assistants\">Hand it to your AI assistant</h2>\n\
         <p>The app is an MCP server with {tools} tools: an assistant can find, download and \
         read games, and ask what their players say. Paste this to yours:</p>\n\
         <pre tabindex=\"0\"><code>{}</code></pre>\n\
         <p class=\"note\"><a href=\"assistants.html\">AI assistants</a> has the setup for \
         {assistants} assistants, and for any other.</p>\n</section>\n\
         <p class=\"smallprint\">{}</p>\n",
        picker::commented(IN_A_TERMINAL),
        escape(reference::PASTE),
        escape(SMALLPRINT)
    );
    page
}

/// The ways to install, one chip each, Windows first, as the system most people read Steam
/// reviews on.
fn installs(version: &str) -> String {
    let choices: Vec<Choice> = WAYS
        .iter()
        .map(|way| Choice {
            id: way.id.to_owned(),
            name: way.name.to_owned(),
            code: filled(way.command, version),
            note: filled(way.note, version),
        })
        .collect();
    picker::html("install", &choices)
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
            "\n### {}\n\n{}\n\n```sh\n{}\n```\n",
            way.name,
            filled(way.note, version),
            filled(way.command, version)
        );
    }
    page.push_str("\n## A first game\n\n");
    for (at, step) in in_the_window().iter().enumerate() {
        let _ = writeln!(page, "{}. {step}", at + 1);
    }
    let _ = write!(
        page,
        "\nOr in a terminal:\n\n```sh\n{IN_A_TERMINAL}\n```\n\n## Hand it to your AI assistant\n\n\
         The app is an MCP server. Paste this to your assistant:\n\n```text\n{}\n```\n\n\
         See [AI assistants](assistants.md) for each assistant's own command.\n\n{SMALLPRINT}\n",
        reference::PASTE
    );
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_to_install_is_a_choice_at_the_version_built() {
        let page = installs("9.8.7");
        for way in WAYS {
            assert!(
                page.contains(&format!("id=\"pick-install-{}\"", way.id)),
                "{}",
                way.id
            );
        }
        assert!(page.contains(
            "id=\"pick-install-windows\" aria-controls=\"panel-install-windows\" checked>"
        ));
        assert!(page.contains(
            "https://github.com/Aureliolo/steamgauge/releases/download/v9.8.7/\
             steamgauge_9.8.7-1_amd64.deb"
        ));
        assert!(!page.contains(VERSION) && !page.contains(RELEASE));
    }

    #[test]
    fn the_first_steps_quote_the_card_timing_the_sizes_table_measured() {
        let (claims, seconds) = card_timing();
        assert!(
            claims.chars().all(|c| c.is_ascii_digit() || c == ','),
            "{claims}"
        );
        assert!(seconds > 0);
        assert!(
            READER_SIZES.contains("RTX 4090"),
            "the steps name the card measured"
        );
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
        assert!(text.contains("steamgauge-9.8.7-windows-x64-setup.exe"));
        assert!(text.contains("See [Accuracy](accuracy.md)."));
    }
}
