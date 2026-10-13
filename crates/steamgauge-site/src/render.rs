//! One page's Markdown as the site's HTML and as its Markdown twin. A link to another page goes to
//! that page on the site, and a link to any other file of the repository goes to it on GitHub at
//! this version's tag, so a page read on the site never sends anyone to a file it cannot open.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use crate::{PAGES, Page, REPOSITORY};

#[derive(Debug)]
pub struct Site<'a> {
    /// The repository's root folder.
    pub root: &'a Path,
    pub version: &'a str,
}

#[derive(Debug)]
pub struct Rendered {
    /// The page's first top-level heading.
    pub title: String,
    pub html: String,
    pub markdown: String,
    /// The repository's files the page shows, by their path from its root.
    pub assets: BTreeSet<String>,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

pub fn render(site: &Site, page: &Page, text: &str) -> Result<Rendered> {
    let events = without_badges(&Parser::new_ext(text, options()).collect::<Vec<_>>());
    let folder = page
        .source
        .rsplit_once('/')
        .map_or("", |(folder, _)| folder);
    let mut html_events = Vec::with_capacity(events.len());
    let mut twin_events = Vec::with_capacity(events.len());
    let mut slugs = Slugs::default();
    let mut title = None;
    let mut assets = BTreeSet::new();
    for (at, event) in events.iter().enumerate() {
        match event {
            Event::Start(Tag::Heading {
                level,
                id: _,
                classes,
                attrs,
            }) => {
                let words = heading_text(&events[at + 1..]);
                let slug = slugs.next(&words);
                if *level == HeadingLevel::H1 && title.is_none() {
                    title = Some(words);
                }
                html_events.push(Event::Start(Tag::Heading {
                    level: *level,
                    id: Some(slug.into()),
                    classes: classes.clone(),
                    attrs: attrs.clone(),
                }));
                twin_events.push(event.clone());
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let (html, twin) = site
                    .link(folder, dest_url)
                    .with_context(|| format!("{}: the link to {dest_url}", page.source))?;
                // Written back inline, since the twin carries no reference definitions.
                let link_type = match link_type {
                    LinkType::Autolink | LinkType::Email if twin == dest_url.as_ref() => *link_type,
                    _ => LinkType::Inline,
                };
                let link = |dest_url: String| {
                    Event::Start(Tag::Link {
                        link_type,
                        dest_url: dest_url.into(),
                        title: title.clone(),
                        id: id.clone(),
                    })
                };
                html_events.push(link(html));
                twin_events.push(link(twin));
            }
            Event::Start(Tag::Image { .. }) => {
                let (image, path) = site.picture(page, folder, event)?;
                assets.insert(path);
                html_events.push(image.clone());
                twin_events.push(image);
            }
            // A table wider than a phone scrolls inside a box a keyboard can reach.
            Event::Start(Tag::Table(_)) => {
                html_events.push(Event::Html("<div class=\"wide\" tabindex=\"0\">\n".into()));
                html_events.push(event.clone());
                twin_events.push(event.clone());
            }
            Event::End(TagEnd::Table) => {
                html_events.push(event.clone());
                html_events.push(Event::Html("</div>\n".into()));
                twin_events.push(event.clone());
            }
            _ => {
                html_events.push(event.clone());
                twin_events.push(event.clone());
            }
        }
    }
    let title = title.with_context(|| format!("{} has no top-level heading", page.source))?;
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, html_events.into_iter());
    // A code block wider than the page scrolls too, and has to be reachable the same way.
    let html = html.replace("<pre><code", "<pre tabindex=\"0\"><code");
    let mut markdown = String::new();
    pulldown_cmark_to_cmark::cmark(twin_events.iter(), &mut markdown, options())
        .with_context(|| format!("{}: writing its Markdown back", page.source))?;
    markdown.push('\n');
    Ok(Rendered {
        title,
        html,
        markdown,
        assets,
    })
}

pub fn external(destination: &str) -> bool {
    destination.contains("://") || destination.starts_with("mailto:")
}

impl Site<'_> {
    /// Where a link written in `folder` goes on the site and in the Markdown twin.
    fn link(&self, folder: &str, destination: &str) -> Result<(String, String)> {
        if external(destination) || destination.starts_with('#') {
            return Ok((destination.to_owned(), destination.to_owned()));
        }
        let (path, anchor) = match destination.split_once('#') {
            Some((path, anchor)) => (path, Some(anchor)),
            None => (destination, None),
        };
        let path = self.file(folder, path)?;
        let fragment = anchor
            .map(|anchor| format!("#{anchor}"))
            .unwrap_or_default();
        if let Some(page) = PAGES.iter().find(|page| page.source == path) {
            return Ok((
                format!("{}.html{fragment}", page.file),
                format!("{}.md{fragment}", page.file),
            ));
        }
        if let Some(anchor) = anchor
            && Path::new(&path)
                .extension()
                .is_some_and(|extension| extension == "md")
        {
            let text = std::fs::read_to_string(self.root.join(&path))?;
            if !anchors(&text).contains(anchor) {
                bail!("{path} has no heading #{anchor}");
            }
        }
        let kind = if self.root.join(&path).is_dir() {
            "tree"
        } else {
            "blob"
        };
        let url = format!(
            "{REPOSITORY}/{kind}/v{}/{}{fragment}",
            self.version,
            path.trim_end_matches('/')
        );
        Ok((url.clone(), url))
    }

    /// A picture as the site shows it, from its copy under `files/`, and the repository file it
    /// copies.
    fn picture<'a>(
        &self,
        page: &Page,
        folder: &str,
        event: &Event<'a>,
    ) -> Result<(Event<'a>, String)> {
        let Event::Start(Tag::Image {
            dest_url,
            title,
            id,
            ..
        }) = event
        else {
            bail!("{}: not a picture", page.source);
        };
        if external(dest_url) {
            bail!(
                "{}: the picture {dest_url} is on another host, and the site fetches nothing \
                 from one",
                page.source
            );
        }
        let path = self
            .file(folder, dest_url)
            .with_context(|| format!("{}: the picture {dest_url}", page.source))?;
        let image = Event::Start(Tag::Image {
            link_type: LinkType::Inline,
            dest_url: format!("files/{path}").into(),
            title: title.clone(),
            id: id.clone(),
        });
        Ok((image, path))
    }

    /// A path written in `folder`, from the repository root, if it names something there.
    fn file(&self, folder: &str, path: &str) -> Result<String> {
        let mut parts: Vec<&str> = folder.split('/').filter(|part| !part.is_empty()).collect();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        bail!("{path} is outside the repository");
                    }
                }
                part => parts.push(part),
            }
        }
        let joined = parts.join("/");
        if !self.root.join(&joined).exists() {
            bail!("{path} names nothing in the repository");
        }
        Ok(joined)
    }
}

/// Drops every badge, a link whose only content is a picture on another host, since the site
/// fetches nothing from one, and the paragraph that held them once it holds nothing else.
fn without_badges<'a>(events: &[Event<'a>]) -> Vec<Event<'a>> {
    let mut kept = Vec::with_capacity(events.len());
    let mut at = 0;
    while at < events.len() {
        if matches!(events[at], Event::Start(Tag::Link { .. }))
            && let Some(Event::Start(Tag::Image { dest_url, .. })) = events.get(at + 1)
            && external(dest_url)
            && let Some(close) = events[at + 1..]
                .iter()
                .position(|event| matches!(event, Event::End(TagEnd::Image)))
            && matches!(events.get(at + close + 2), Some(Event::End(TagEnd::Link)))
        {
            at += close + 3;
            continue;
        }
        kept.push(events[at].clone());
        at += 1;
    }
    let mut left = Vec::with_capacity(kept.len());
    let mut at = 0;
    while at < kept.len() {
        if matches!(kept[at], Event::Start(Tag::Paragraph))
            && let Some(length) = kept[at + 1..]
                .iter()
                .position(|event| matches!(event, Event::End(TagEnd::Paragraph)))
            && kept[at + 1..at + 1 + length]
                .iter()
                .all(|event| match event {
                    Event::SoftBreak | Event::HardBreak => true,
                    Event::Text(text) => text.trim().is_empty(),
                    _ => false,
                })
        {
            at += length + 2;
            continue;
        }
        left.push(kept[at].clone());
        at += 1;
    }
    left
}

/// The words of the heading the events open on.
fn heading_text(events: &[Event<'_>]) -> String {
    let mut words = String::new();
    for event in events {
        match event {
            Event::End(TagEnd::Heading(_)) => break,
            Event::Text(text) | Event::Code(text) => words.push_str(text),
            _ => {}
        }
    }
    words
}

/// The anchor GitHub gives a heading, so a link written for the repository works on the site.
pub fn slug(words: &str) -> String {
    words
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Anchors in the order a page gives them, the second of a name with `-1` after it.
#[derive(Debug, Default)]
struct Slugs(HashMap<String, usize>);

impl Slugs {
    fn next(&mut self, words: &str) -> String {
        let base = slug(words);
        let seen = self.0.entry(base.clone()).or_insert(0);
        let slug = if *seen == 0 {
            base
        } else {
            format!("{base}-{seen}")
        };
        *seen += 1;
        slug
    }
}

/// Every heading anchor a Markdown file has.
fn anchors(text: &str) -> BTreeSet<String> {
    let events: Vec<Event<'_>> = Parser::new_ext(text, options()).collect();
    let mut slugs = Slugs::default();
    events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event, Event::Start(Tag::Heading { .. })))
        .map(|(at, _)| slugs.next(&heading_text(&events[at + 1..])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site() -> Site<'static> {
        Site {
            root: Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
            version: "1.2.3",
        }
    }

    fn page(source: &'static str) -> Page {
        Page {
            file: "test",
            source,
            label: "Test",
            description: "a test",
        }
    }

    #[test]
    fn headings_take_githubs_anchors_and_a_repeat_is_numbered() {
        let rendered = render(
            &site(),
            &page("docs/test.md"),
            "# The `crawl` command\n\n## What it does?\n\n## What it does?\n",
        )
        .unwrap();
        assert_eq!(rendered.title, "The crawl command");
        assert!(rendered.html.contains("<h1 id=\"the-crawl-command\">"));
        assert!(rendered.html.contains("<h2 id=\"what-it-does\">"));
        assert!(rendered.html.contains("<h2 id=\"what-it-does-1\">"));
        assert!(!rendered.markdown.contains('{'));
    }

    #[test]
    fn a_page_links_to_a_page_and_anything_else_goes_to_github_at_the_tag() {
        let rendered = render(
            &site(),
            &page("docs/test.md"),
            "# T\n\n[a](install.md#verifying-a-download), [b](../SECURITY.md#releases), \
             [c](../training/), [d](https://example.org/x.md)\n",
        )
        .unwrap();
        assert!(
            rendered
                .html
                .contains("href=\"install.html#verifying-a-download\"")
        );
        assert!(
            rendered
                .markdown
                .contains("(install.md#verifying-a-download)")
        );
        assert!(rendered.html.contains(
            "href=\"https://github.com/Aureliolo/steamgauge/blob/v1.2.3/SECURITY.md#releases\""
        ));
        assert!(
            rendered
                .html
                .contains("href=\"https://github.com/Aureliolo/steamgauge/tree/v1.2.3/training\"")
        );
        assert!(rendered.html.contains("href=\"https://example.org/x.md\""));
    }

    #[test]
    fn a_link_to_nothing_or_to_a_missing_heading_is_refused() {
        let refused = |text: &str| render(&site(), &page("docs/test.md"), text).is_err();
        assert!(refused("# T\n\n[a](nothing-here.md)\n"));
        assert!(refused("# T\n\n[a](../SECURITY.md#no-such-heading)\n"));
        assert!(refused("# T\n\n[a](../../outside.md)\n"));
    }

    #[test]
    fn badges_go_and_a_picture_from_another_host_is_refused() {
        let rendered = render(
            &site(),
            &page("README.md"),
            "# T\n\n[![CI](https://example.org/badge.svg)](https://example.org)\n\
             [![L](https://example.org/l.svg)](LICENSE)\n\n\
             ![Mark](crates/steamgauge-app/icons/128x128.png)\n",
        )
        .unwrap();
        assert!(!rendered.html.contains("example.org"));
        assert_eq!(rendered.html.matches("<p>").count(), 1);
        assert!(
            rendered
                .html
                .contains("src=\"files/crates/steamgauge-app/icons/128x128.png\"")
        );
        assert!(
            rendered
                .assets
                .contains("crates/steamgauge-app/icons/128x128.png")
        );
        assert!(
            render(
                &site(),
                &page("README.md"),
                "# T\n\n![x](https://example.org/x.png)\n"
            )
            .is_err()
        );
    }

    #[test]
    fn tables_and_code_scroll_in_a_box_a_keyboard_reaches() {
        let rendered = render(
            &site(),
            &page("docs/test.md"),
            "# T\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```text\nx\n```\n",
        )
        .unwrap();
        assert!(
            rendered
                .html
                .contains("<div class=\"wide\" tabindex=\"0\">\n<table>")
        );
        assert!(rendered.html.contains("<pre tabindex=\"0\"><code"));
    }

    #[test]
    fn a_slug_keeps_letters_digits_hyphens_and_underscores() {
        assert_eq!(
            slug("SLSA Build L3: what it means"),
            "slsa-build-l3-what-it-means"
        );
        assert_eq!(slug("held_out, not frozen"), "held_out-not-frozen");
    }
}
