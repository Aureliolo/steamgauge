//! What the built site is held to before anything is written: every link inside it lands on a
//! file and a heading that exist, no id is given twice, nothing is fetched from another host, no
//! page runs a script or carries a style the content security policy would drop, and no text
//! holds a character that would hide or reorder what a reader sees.

use std::collections::{BTreeMap, BTreeSet};

use crate::SITE;
use crate::render::external;

/// Everything wrong with the pages, given every file name the site will hold.
pub fn check(pages: &BTreeMap<String, String>, files: &BTreeSet<String>) -> Vec<String> {
    let ids: BTreeMap<&str, BTreeSet<String>> = pages
        .iter()
        .map(|(name, html)| {
            (
                name.as_str(),
                tags(html).iter().filter_map(|tag| tag.get("id")).collect(),
            )
        })
        .collect();
    let mut problems = Vec::new();
    for (name, html) in pages {
        problems.extend(controls(html).map(|at| format!("{name}: {at}")));
        let tags = tags(html);
        let mut seen = BTreeSet::new();
        for id in tags.iter().filter_map(|tag| tag.get("id")) {
            if !seen.insert(id.clone()) {
                problems.push(format!("{name}: the id {id} is given twice"));
            }
        }
        let twin = name.replace(".html", ".md");
        for tag in &tags {
            if tag.name == "script" {
                problems.push(format!("{name}: a script, and the site runs none"));
            }
            if tag.get("style").is_some() {
                problems.push(format!(
                    "{name}: a style attribute on <{}>, which the content security policy drops",
                    tag.name
                ));
            }
            for (attribute, target) in &tag.attributes {
                if !matches!(attribute.as_str(), "href" | "src" | "srcset") {
                    continue;
                }
                let canonical =
                    tag.name == "link" && tag.get("rel").as_deref() == Some("canonical");
                let target = if canonical {
                    match target.strip_prefix(SITE) {
                        Some("") => "index.html".to_owned(),
                        Some(page) => page.to_owned(),
                        None => {
                            problems.push(format!(
                                "{name}: the canonical address {target} is not on {SITE}"
                            ));
                            continue;
                        }
                    }
                } else {
                    target.clone()
                };
                if external(&target) {
                    if tag.name != "a" {
                        problems.push(format!(
                            "{name}: <{}> fetches {target} from another host",
                            tag.name
                        ));
                    }
                    continue;
                }
                let (path, anchor) = match target.split_once('#') {
                    Some((path, anchor)) => (path, Some(decoded(anchor))),
                    None => (target.as_str(), None),
                };
                let path = if path.is_empty() {
                    name.clone()
                } else {
                    decoded(path)
                };
                if !files.contains(&path) {
                    problems.push(format!("{name}: {target} is not in the site"));
                    continue;
                }
                let markdown = std::path::Path::new(&path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"));
                if tag.name == "a" && markdown && path != twin {
                    problems.push(format!(
                        "{name}: {target} links to another page's Markdown rather than the page"
                    ));
                }
                if let Some(anchor) = anchor
                    && !ids
                        .get(path.as_str())
                        .is_some_and(|ids| ids.contains(&anchor))
                {
                    problems.push(format!(
                        "{name}: {target} names a heading {path} does not have"
                    ));
                }
            }
        }
    }
    problems
}

/// Where a text holds a control character, a byte-order mark, or a bidirectional override or
/// isolate, any of which can make what a reader sees differ from what the file says.
pub fn controls(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().enumerate().flat_map(|(number, line)| {
        line.chars()
            .filter(|&c| {
                (c.is_control() && c != '\t' && c != '\r')
                    || c == '\u{feff}'
                    || ('\u{202a}'..='\u{202e}').contains(&c)
                    || ('\u{2066}'..='\u{2069}').contains(&c)
            })
            .map(move |c| format!("line {} holds U+{:04X}", number + 1, u32::from(c)))
    })
}

#[derive(Debug)]
struct Element {
    name: String,
    attributes: Vec<(String, String)>,
}

impl Element {
    fn get(&self, name: &str) -> Option<String> {
        self.attributes
            .iter()
            .find(|(attribute, _)| attribute == name)
            .map(|(_, value)| value.clone())
    }
}

/// Every opening tag, with its attributes unescaped. Comments are skipped; text never holds a
/// bare `<`, since the renderer escapes it.
fn tags(html: &str) -> Vec<Element> {
    let mut found = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        if let Some(comment) = rest.strip_prefix("!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        let name_length = rest
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        if name_length == 0 || !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let name = rest[..name_length].to_ascii_lowercase();
        rest = &rest[name_length..];
        let mut attributes = Vec::new();
        loop {
            rest = rest.trim_start();
            if rest.is_empty() || rest.starts_with('>') || rest.starts_with("/>") {
                break;
            }
            let length = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
                .unwrap_or(rest.len());
            if length == 0 {
                rest = &rest[1..];
                continue;
            }
            let attribute = rest[..length].to_ascii_lowercase();
            rest = &rest[length..];
            let mut value = String::new();
            if let Some(after) = rest.strip_prefix('=') {
                let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'');
                if let Some(quote) = quote {
                    let body = &after[1..];
                    let end = body.find(quote).unwrap_or(body.len());
                    value = unescaped(&body[..end]);
                    rest = body.get(end + 1..).unwrap_or("");
                } else {
                    let end = after
                        .find(|c: char| c.is_whitespace() || c == '>')
                        .unwrap_or(after.len());
                    value = unescaped(&after[..end]);
                    rest = &after[end..];
                }
            }
            attributes.push((attribute, value));
        }
        found.push(Element { name, attributes });
    }
    found
}

fn unescaped(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// A URL's percent-escapes put back, as the file names and anchors they stand for.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(byte) = text
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(byte);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(pages: &[(&str, &str)], others: &[&str]) -> Vec<String> {
        let pages: BTreeMap<String, String> = pages
            .iter()
            .map(|(name, html)| ((*name).to_owned(), (*html).to_owned()))
            .collect();
        let files = pages
            .keys()
            .cloned()
            .chain(others.iter().map(|name| (*name).to_owned()))
            .collect();
        check(&pages, &files)
    }

    #[test]
    fn links_to_pages_headings_and_files_that_exist_pass() {
        let problems = site(
            &[
                (
                    "index.html",
                    "<link rel=\"canonical\" href=\"https://aureliolo.github.io/steamgauge/\">\
                     <a href=\"install.html#the-first-launch\">a</a><a href=\"#top\">b</a>\
                     <h1 id=\"top\">T</h1><img src=\"icon.png\" alt=\"\"><a href=\"index.md\">m</a>\
                     <a href=\"https://github.com/x\">g</a>",
                ),
                ("install.html", "<h2 id=\"the-first-launch\">x</h2>"),
            ],
            &["icon.png", "index.md"],
        );
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn a_missing_file_or_heading_is_named() {
        let problems = site(
            &[
                (
                    "index.html",
                    "<a href=\"gone.html\">a</a><a href=\"install.html#nowhere\">b</a>",
                ),
                ("install.html", "<h2 id=\"x\">x</h2>"),
            ],
            &[],
        );
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn a_fetch_from_another_host_a_script_and_a_style_are_refused() {
        let problems = site(
            &[(
                "index.html",
                "<img src=\"https://example.org/x.png\" alt=\"\"><script src=\"x.js\"></script>\
                 <td style=\"text-align: left\">",
            )],
            &["x.js"],
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn an_id_given_twice_and_another_pages_markdown_are_refused() {
        let problems = site(
            &[(
                "index.html",
                "<h2 id=\"a\">a</h2><h2 id=\"a\">a</h2><a href=\"install.md\">x</a>",
            )],
            &["install.md"],
        );
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn comments_and_escaped_text_are_not_tags() {
        let problems = site(
            &[(
                "index.html",
                "<!-- <script> --><p>&lt;script&gt;</p><a href=\"a&amp;b.html\">x</a>",
            )],
            &["a&b.html"],
        );
        assert_eq!(problems, Vec::<String>::new());
    }

    #[test]
    fn hidden_and_reordering_characters_are_found() {
        assert_eq!(controls("plain\ttext\r\n").count(), 0);
        let found: Vec<_> = controls("ok\nsay \u{202e}olleh\u{feff}\n").collect();
        assert_eq!(found, ["line 2 holds U+202E", "line 2 holds U+FEFF"]);
    }

    #[test]
    fn percent_escapes_are_put_back() {
        assert_eq!(decoded("%C3%A9t%C3%A9-%41"), "été-A");
        assert_eq!(decoded("100%"), "100%");
    }
}
