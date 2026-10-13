//! One page's Markdown as the site's HTML and as its Markdown twin. A link to another page goes to
//! that page on the site, and a link to any other file of the repository goes to it on GitHub at
//! this version's tag, so a page read on the site never sends anyone to a file it cannot open.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result, bail};
use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use crate::{Layout, PAGES, Page, REPOSITORY};

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
    /// Each second-level heading, as its anchor and its words, for the page's contents.
    pub contents: Vec<(String, String)>,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
}

pub fn render(site: &Site, page: &Page, text: &str) -> Result<Rendered> {
    let source = page.markdown().unwrap_or("the front page");
    let events: Vec<Event<'_>> = Parser::new_ext(text, options()).collect();
    let folder = source.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut html_events = Vec::with_capacity(events.len());
    let mut twin_events = Vec::with_capacity(events.len());
    let mut slugs = Slugs::default();
    let mut title = None;
    let mut assets = BTreeSet::new();
    let mut contents = Vec::new();
    let mut card = false;
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
                    title = Some(words.clone());
                }
                if *level == HeadingLevel::H2 {
                    contents.push((slug.clone(), words));
                }
                if page.layout == Layout::Cards {
                    card_edge(&mut html_events, &mut card, *level);
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
                    .with_context(|| format!("{source}: the link to {dest_url}"))?;
                let link_type = written_back(*link_type, twin == dest_url.as_ref());
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
                let (image, path) = site.picture(source, folder, event)?;
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
    if card {
        html_events.push(Event::Html("</section>\n".into()));
    }
    let title = title.with_context(|| format!("{source} has no top-level heading"))?;
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, html_events.into_iter());
    // A code block wider than the page scrolls too, and has to be reachable the same way.
    let html = html.replace("<pre><code", "<pre tabindex=\"0\"><code");
    let mut markdown = String::new();
    pulldown_cmark_to_cmark::cmark(twin_events.iter(), &mut markdown, options())
        .with_context(|| format!("{source}: writing its Markdown back"))?;
    markdown.push('\n');
    Ok(Rendered {
        title,
        html,
        markdown,
        assets,
        contents,
    })
}

/// On a reference page, closes the card a heading ends and opens the one a third-level heading
/// starts.
fn card_edge(events: &mut Vec<Event<'_>>, open: &mut bool, level: HeadingLevel) {
    if !matches!(level, HeadingLevel::H2 | HeadingLevel::H3) {
        return;
    }
    if *open {
        events.push(Event::Html("</section>\n".into()));
    }
    *open = level == HeadingLevel::H3;
    if *open {
        events.push(Event::Html("<section class=\"card\">\n".into()));
    }
}

/// How a link is written back into the twin: inline, since the twin carries no reference
/// definitions, unless it is an address written as itself and left unchanged.
fn written_back(link_type: LinkType, unchanged: bool) -> LinkType {
    match link_type {
        LinkType::Autolink | LinkType::Email if unchanged => link_type,
        _ => LinkType::Inline,
    }
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
        if let Some(page) = PAGES
            .iter()
            .find(|page| page.markdown() == Some(path.as_str()))
        {
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
        source: &str,
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
            bail!("{source}: not a picture");
        };
        if external(dest_url) {
            bail!(
                "{source}: the picture {dest_url} is on another host, and the site fetches \
                 nothing from one"
            );
        }
        let path = self
            .file(folder, dest_url)
            .with_context(|| format!("{source}: the picture {dest_url}"))?;
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
    use crate::Source;

    fn site() -> Site<'static> {
        Site {
            root: Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
            version: "1.2.3",
        }
    }

    fn page(source: &'static str) -> Page {
        Page {
            file: "test",
            source: Source::Markdown(source),
            label: "Test",
            description: "a test",
            layout: Layout::Prose,
        }
    }

    #[test]
    fn a_reference_is_cards_and_every_page_lists_its_sections() {
        let reference = Page {
            layout: Layout::Cards,
            ..page("docs/test.md")
        };
        let rendered = render(
            &site(),
            &reference,
            "# T\n\nIntro.\n\n## Games\n\n### `games`\n\nOne.\n\n### `look_up`\n\nTwo.\n\n\
             ## Jobs\n\n### `queue`\n\nThree.\n",
        )
        .unwrap();
        assert_eq!(rendered.html.matches("<section class=\"card\">").count(), 3);
        assert_eq!(rendered.html.matches("</section>").count(), 3);
        assert!(
            rendered
                .html
                .contains("Two.</p>\n</section>\n<h2 id=\"jobs\">")
        );
        assert_eq!(
            rendered.contents,
            [
                ("games".to_owned(), "Games".to_owned()),
                ("jobs".to_owned(), "Jobs".to_owned())
            ]
        );
        assert!(!rendered.markdown.contains("section"));
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
    fn a_picture_is_copied_in_and_one_from_another_host_is_refused() {
        let rendered = render(
            &site(),
            &page("docs/test.md"),
            "# T\n\n![Mark](../crates/steamgauge-app/icons/128x128.png)\n",
        )
        .unwrap();
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
                &page("docs/test.md"),
                "# T\n\n[![x](https://example.org/x.png)](https://example.org)\n"
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
