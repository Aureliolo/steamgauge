//! Builds the documentation site: the README as its home page, every page under `docs/`,
//! each also as Markdown, and `llms.txt`. The parts of those pages that describe the program,
//! its commands, its MCP tools and the clients it adds itself to, are written from the program's
//! own tables, and a build refuses a page whose copy of them is out of date.
//!
//! ```text
//! cargo run -p steamgauge-site -- write
//! cargo run -p steamgauge-site -- build --out site --shots <app-check shots> --report sample-report.html
//! ```

mod blocks;
mod checks;
mod render;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;

/// Where the site is published.
pub const SITE: &str = "https://aureliolo.github.io/steamgauge/";
pub const REPOSITORY: &str = "https://github.com/Aureliolo/steamgauge";

/// One page of the site, in the order the navigation lists them.
#[derive(Debug)]
pub struct Page {
    /// Its file name in the site, without `.html` or `.md`.
    pub file: &'static str,
    /// Its Markdown, from the repository root.
    pub source: &'static str,
    /// Its name in the navigation.
    pub label: &'static str,
    /// What it holds, as the README's list and a search result say it: lower case, no full stop.
    pub description: &'static str,
}

pub const PAGES: &[Page] = &[
    Page {
        file: "index",
        source: "README.md",
        label: "Home",
        description: "a tool that downloads every Steam review of a game and reads each claim in \
                      them on your own machine, to show what players praise and criticise",
    },
    Page {
        file: "install",
        source: "docs/install.md",
        label: "Install",
        description: "the files for each system, the package managers, the first launch, \
                      verifying a download and upgrading",
    },
    Page {
        file: "how-it-works",
        source: "docs/how-it-works.md",
        label: "How it works",
        description: "what the tool counts and why, how a review becomes the claims it makes, how \
                      each claim is read, and what every percentage means",
    },
    Page {
        file: "assistants",
        source: "docs/assistants.md",
        label: "AI assistants",
        description: "the app's MCP server, its tools, the assistants it adds itself to, and the \
                      command for any other",
    },
    Page {
        file: "commands",
        source: "docs/commands.md",
        label: "Command line",
        description: "every command and option of the `steamgauge` program",
    },
    Page {
        file: "accuracy",
        source: "docs/accuracy.md",
        label: "Accuracy",
        description: "how the reader is measured, how it compares with the alternatives, each \
                      size it comes in, the person-checked gold set, and how the reference sets \
                      are made",
    },
    Page {
        file: "limits",
        source: "docs/limits.md",
        label: "Limits",
        description: "where the figures fall short, by language, by labeller and by what Steam \
                      serves",
    },
    Page {
        file: "privacy",
        source: "docs/privacy.md",
        label: "Privacy",
        description: "what stays on your machine and the four places the app makes requests to",
    },
];

#[derive(Parser, Debug)]
#[command(about = "Build SteamGauge's documentation site.")]
enum Cli {
    /// Write the parts of the README and docs/ that come from the program's own tables.
    Write,
    /// Build the site into a folder, refusing a page whose generated parts are out of date.
    Build {
        /// The folder to build into: missing, empty, or an earlier build, which is replaced.
        #[arg(long)]
        out: PathBuf,
        /// The pictures `tools/app-check/check.mjs --shots` took of the window.
        #[arg(long)]
        shots: PathBuf,
        /// The page `cargo run -p steamgauge-core --example sample-report` wrote.
        #[arg(long)]
        report: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::thread::Builder::new()
        .stack_size(steamgauge_app::CLI_STACK)
        .spawn(move || match cli {
            Cli::Write => write(&root),
            Cli::Build { out, shots, report } => build(&root, &out, &shots, &report),
        })?
        .join()
        .map_err(|_| anyhow::anyhow!("the site's build panicked"))?
}

fn write(root: &Path) -> Result<()> {
    for page in PAGES {
        let path = root.join(page.source);
        let text = fs::read_to_string(&path).with_context(|| page.source)?;
        let filled = blocks::fill(root, page.source, &text)?;
        if filled != text {
            fs::write(&path, filled).with_context(|| page.source)?;
            println!("wrote {}", page.source);
        }
    }
    Ok(())
}

fn build(root: &Path, out: &Path, shots: &Path, report: &Path) -> Result<()> {
    let mut problems = Vec::new();
    let mut sources = Vec::new();
    for page in PAGES {
        let text = fs::read_to_string(root.join(page.source)).with_context(|| page.source)?;
        if blocks::fill(root, page.source, &text)? != text {
            problems.push(format!(
                "{}: a generated part is out of date; run `cargo run -p steamgauge-site -- write`",
                page.source
            ));
        }
        problems.extend(checks::controls(&text).map(|at| format!("{}: {at}", page.source)));
        sources.push(text);
    }
    problems.extend(orphans(root)?);
    if !problems.is_empty() {
        bail!(listed(&problems));
    }

    let version = env!("CARGO_PKG_VERSION");
    let site = render::Site { root, version };
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut pages = BTreeMap::new();
    let mut twins = Vec::new();
    let mut assets = BTreeSet::new();
    let window = Window::taken(shots)?;
    for (page, text) in PAGES.iter().zip(&sources) {
        let rendered = render::render(&site, page, text)?;
        let mut content = rendered.html;
        if page.file == "index" {
            let at = content.find("<h2").unwrap_or(content.len());
            content.insert_str(at, &window.figure());
        }
        let html = page_html(page, &rendered.title, &content, version);
        pages.insert(format!("{}.html", page.file), html);
        files.insert(
            format!("{}.md", page.file),
            rendered.markdown.clone().into_bytes(),
        );
        twins.push((page, rendered.title, rendered.markdown));
        assets.extend(rendered.assets);
    }
    for asset in assets {
        let bytes = fs::read(root.join(&asset)).with_context(|| asset.clone())?;
        files.insert(format!("files/{asset}"), bytes);
    }
    files.insert("style.css".into(), include_bytes!("../style.css").to_vec());
    files.insert(
        "icon.png".into(),
        fs::read(root.join("crates/steamgauge-app/icons/128x128.png"))?,
    );
    files.insert("window-light.png".into(), window.light);
    files.insert("window-dark.png".into(), window.dark);
    files.insert(
        "sample-report.html".into(),
        fs::read(report).with_context(|| report.display().to_string())?,
    );
    files.insert("llms.txt".into(), llms(&twins).into_bytes());
    files.insert("llms-full.txt".into(), llms_full(&twins).into_bytes());
    files.insert("sitemap.xml".into(), sitemap().into_bytes());
    // GitHub Pages would otherwise run Jekyll over the folder.
    files.insert(".nojekyll".into(), Vec::new());

    let names: BTreeSet<String> = files.keys().chain(pages.keys()).cloned().collect();
    let mut problems = checks::check(&pages, &names);
    for (name, bytes) in &files {
        if Path::new(name)
            .extension()
            .is_some_and(|extension| extension == "md" || extension == "txt")
        {
            let text = String::from_utf8_lossy(bytes);
            problems.extend(checks::controls(&text).map(|at| format!("{name}: {at}")));
        }
    }
    if !problems.is_empty() {
        bail!(listed(&problems));
    }

    clear(out)?;
    for (name, bytes) in files.into_iter().chain(
        pages
            .into_iter()
            .map(|(name, html)| (name, html.into_bytes())),
    ) {
        let path = out.join(&name);
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(&path, bytes).with_context(|| path.display().to_string())?;
    }
    println!("built {} pages into {}", PAGES.len(), out.display());
    Ok(())
}

fn listed(problems: &[String]) -> String {
    let mut said = format!("{} problem(s) with the site:", problems.len());
    for problem in problems {
        said.push_str("\n  - ");
        said.push_str(problem);
    }
    said
}

/// A page under `docs/` the site does not list would be in the repository and nowhere on the
/// site.
fn orphans(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root.join("docs"))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        let source = format!("docs/{name}");
        if Path::new(&name)
            .extension()
            .is_some_and(|extension| extension == "md")
            && !PAGES.iter().any(|page| page.source == source)
        {
            found.push(format!("{source}: no page of the site lists it"));
        }
    }
    Ok(found)
}

/// Empties the folder a build goes into, which has to be one nothing else lives in.
fn clear(out: &Path) -> Result<()> {
    if !out.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(out)?.peekable();
    if entries.peek().is_none() {
        return Ok(());
    }
    if !out.join(".nojekyll").exists() || !out.join("llms.txt").exists() {
        bail!(
            "{} holds something other than an earlier build of the site; name an empty folder",
            out.display()
        );
    }
    fs::remove_dir_all(out)?;
    Ok(())
}

/// The window as the home page shows it, in each colour scheme.
struct Window {
    light: Vec<u8>,
    dark: Vec<u8>,
    width: u32,
    height: u32,
}

impl Window {
    fn taken(shots: &Path) -> Result<Self> {
        let read = |name: &str| {
            let path = shots.join(name);
            fs::read(&path).with_context(|| {
                format!(
                    "{}: run `node tools/app-check/check.mjs --shots {}` first",
                    path.display(),
                    shots.display()
                )
            })
        };
        let light = read("cockpit-light.png")?;
        let dark = read("cockpit-dark.png")?;
        let (width, height) = png_size(&light)?;
        if png_size(&dark)? != (width, height) {
            bail!("the window's two pictures are not the same size");
        }
        Ok(Self {
            light,
            dark,
            width,
            height,
        })
    }

    fn figure(&self) -> String {
        format!(
            "<figure class=\"window\">\n<picture>\n\
             <source srcset=\"window-dark.png\" media=\"(prefers-color-scheme: dark)\">\n\
             <img src=\"window-light.png\" width=\"{}\" height=\"{}\" alt=\"The SteamGauge \
             window's cockpit: the games held, what moved in each since the last look, what \
             needs attention, and the reader this computer uses.\">\n</picture>\n\
             <figcaption>The window, showing invented games. The page <code>report</code> \
             writes looks like <a href=\"sample-report.html\">this sample</a>, made from \
             invented counts.</figcaption>\n</figure>\n",
            self.width, self.height
        )
    }
}

/// A PNG's width and height, from its header.
fn png_size(bytes: &[u8]) -> Result<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        bail!("not a PNG");
    }
    let number =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    Ok((number(16), number(20)))
}

fn page_html(page: &Page, title: &str, content: &str, version: &str) -> String {
    let title = if page.file == "index" {
        title.to_owned()
    } else {
        format!("{title} | SteamGauge")
    };
    let mut nav = String::new();
    for other in PAGES {
        let current = if other.file == page.file {
            " aria-current=\"page\""
        } else {
            ""
        };
        let _ = writeln!(
            nav,
            "<li><a href=\"{}.html\"{current}>{}</a></li>",
            other.file,
            escape(other.label)
        );
    }
    include_str!("../page.html")
        .replace("{{title}}", &escape(&title))
        .replace("{{description}}", &escape(&sentence(page.description)))
        .replace("{{canonical}}", &canonical(page))
        .replace("{{twin}}", &format!("{}.md", page.file))
        .replace("{{nav}}", nav.trim_end())
        .replace("{{version}}", version)
        .replace("{{content}}", content.trim_end())
}

fn canonical(page: &Page) -> String {
    if page.file == "index" {
        SITE.to_owned()
    } else {
        format!("{SITE}{}.html", page.file)
    }
}

/// A description as a sentence of its own: capitalised, with its full stop.
fn sentence(description: &str) -> String {
    let mut chars = description.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>());
    format!("{}{}.", first.unwrap_or_default(), chars.as_str()).replace('`', "")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The site's index for language models, in the shape llmstxt.org gives it.
fn llms(twins: &[(&Page, String, String)]) -> String {
    let mut text = format!(
        "# SteamGauge\n\n> {}\n\n## Docs\n\n",
        sentence(PAGES[0].description)
    );
    for (page, title, _) in twins {
        let _ = writeln!(
            text,
            "- [{title}]({SITE}{}.md): {}",
            page.file,
            sentence(page.description)
        );
    }
    text
}

fn llms_full(twins: &[(&Page, String, String)]) -> String {
    twins
        .iter()
        .map(|(_, _, markdown)| markdown.trim_end())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
        + "\n"
}

fn sitemap() -> String {
    let mut text = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in PAGES {
        let _ = writeln!(text, "<url><loc>{}</loc></url>", canonical(page));
    }
    text.push_str("</urlset>\n");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_has_its_own_file_and_source() {
        let files: BTreeSet<_> = PAGES.iter().map(|page| page.file).collect();
        let sources: BTreeSet<_> = PAGES.iter().map(|page| page.source).collect();
        assert_eq!(files.len(), PAGES.len());
        assert_eq!(sources.len(), PAGES.len());
        assert_eq!(PAGES[0].file, "index");
    }

    #[test]
    fn every_generated_part_in_the_repository_is_current() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for page in PAGES {
            let text = fs::read_to_string(root.join(page.source)).unwrap();
            assert!(
                blocks::fill(&root, page.source, &text).unwrap() == text,
                "{} is out of date: run `cargo run -p steamgauge-site -- write`",
                page.source
            );
        }
        assert_eq!(orphans(&root).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_description_reads_as_a_sentence_on_its_own() {
        assert_eq!(
            sentence("every command of the `steamgauge` program"),
            "Every command of the steamgauge program."
        );
    }

    #[test]
    fn a_png_gives_its_size_and_anything_else_is_refused() {
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend(1280u32.to_be_bytes());
        header.extend(900u32.to_be_bytes());
        assert_eq!(png_size(&header).unwrap(), (1280, 900));
        assert!(png_size(b"GIF89a").is_err());
    }

    #[test]
    fn a_folder_holding_anything_but_a_build_is_never_emptied() {
        let scratch = std::env::temp_dir().join(format!("steamgauge-site-{}", std::process::id()));
        fs::create_dir_all(&scratch).unwrap();
        fs::write(scratch.join("notes.txt"), "mine").unwrap();
        assert!(clear(&scratch).is_err());
        assert!(scratch.join("notes.txt").exists());
        fs::write(scratch.join(".nojekyll"), "").unwrap();
        fs::write(scratch.join("llms.txt"), "").unwrap();
        clear(&scratch).unwrap();
        assert!(!scratch.exists());
    }
}
