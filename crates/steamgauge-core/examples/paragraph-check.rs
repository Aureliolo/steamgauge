//! What the writer makes of a game's themes, and how long it takes on this machine's processor,
//! so its paragraphs are read off real games and its time per game is measured, not guessed.
//!
//! The game has to be prepared for search by meaning already (`themes-check` does that), and the
//! writer has to be in the model cache. Prints the brief's length, what the writer wrote, what
//! the check kept of it, and the seconds it took.
//!
//!     cargo run --release -p steamgauge-core --example paragraph-check -- \
//!         <library> <app id>...

use std::sync::atomic::AtomicBool;

use steamgauge_core::{
    embed::latest_snapshot,
    model::default_cache_dir,
    report::crawl_facts,
    themes::{self, Themes},
    writer::{self, Writer},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, games @ ..] = args.as_slice() else {
        return Err("usage: paragraph-check <library> <app id>...".into());
    };
    let root = std::path::Path::new(root);
    let started = std::time::Instant::now();
    let mut loaded = Writer::load(&default_cache_dir())?;
    eprintln!("writer loaded in {:.1} s", started.elapsed().as_secs_f64());
    let stop = AtomicBool::new(false);
    for game in games {
        let app_id: u32 = game.parse()?;
        let snapshot = latest_snapshot(root, app_id)?;
        let found = match Themes::load(&snapshot) {
            Some(found) => found,
            None => themes::find(&snapshot, themes::Closeness::USED)?,
        };
        let reading: serde_json::Value =
            serde_json::from_slice(&std::fs::read(snapshot.join("reading.json"))?)?;
        let reviews = reading["reviews"].as_u64().unwrap_or(0);
        let title = crawl_facts(root, app_id)?.title();
        let Some(brief) = writer::brief(&title, reviews, &found) else {
            println!("\n{title}: no point to write about");
            continue;
        };
        let started = std::time::Instant::now();
        let written = loaded.answer(&brief.prompt, &stop)?.unwrap_or_default();
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "\n{title}, {} characters of brief, written in {seconds:.1} s:\n{written}\n\nkept:\n{}",
            brief.prompt.len(),
            writer::checked(&written, &brief).unwrap_or_else(|| "(nothing)".to_owned())
        );
    }
    Ok(())
}
