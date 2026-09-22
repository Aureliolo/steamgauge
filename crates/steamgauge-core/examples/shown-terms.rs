//! Every term every counted game shows, and the three things a reader would object to.
//!
//! The rules about what a term list may hold are easy to state and easy to break somewhere
//! else: a list must not show one word in two forms, must not show a word that only turns what
//! follows, and a term of one or two letters is usually an abbreviation and occasionally a
//! fragment of something. A change to the cut is checked on the page it was found on, and this
//! asks the same question of all fifty-odd pages at once.
//!
//! Reads what is on disk. Nothing is counted here, so a change to the cut needs `recount`
//! before this says anything about it.
//!
//!     cargo run --release -p steamgauge-core --example shown-terms [-- --all]

use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let everything = std::env::args().any(|arg| arg == "--all");
    let data = std::path::Path::new("data");

    let mut lists = 0_u32;
    let mut two_forms = Vec::new();
    let mut turning = Vec::new();
    let mut short: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
    let mut games = 0_u32;

    let mut apps: Vec<u32> = Vec::new();
    for entry in std::fs::read_dir(data)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(app_id) = name.strip_prefix("appid=").and_then(|id| id.parse().ok()) {
            apps.push(app_id);
        }
    }
    apps.sort_unstable();

    for app_id in apps {
        let Ok(snapshot) = steamgauge_core::embed::latest_snapshot(data, app_id) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(snapshot.join("reading.json")) else {
            continue;
        };
        let counted: steamgauge_core::read::ReadReport = serde_json::from_slice(&bytes)?;
        games += 1;
        for row in &counted.said {
            for (side, terms) in [("praise", &row.praised), ("complaint", &row.criticised)] {
                if terms.is_empty() {
                    continue;
                }
                lists += 1;
                for (at, term) in terms.iter().enumerate() {
                    let where_it_is = format!("{app_id} {} {side}", row.subject);
                    if steamgauge_core::said::turns_what_follows(&term.text) {
                        turning.push(format!("{where_it_is}: {}", term.text));
                    }
                    if spaced(&term.text) && term.text.chars().count() < 3 {
                        short
                            .entry(term.text.clone())
                            .or_default()
                            .push((app_id, format!("{} {side}", row.subject)));
                    }
                    for other in &terms[at + 1..] {
                        if steamgauge_core::said::one_word_inflected(&term.text, &other.text) {
                            two_forms
                                .push(format!("{where_it_is}: {} and {}", term.text, other.text));
                        }
                    }
                }
            }
        }
    }

    println!("{lists} term lists over {games} counted games\n");
    report("one word in two forms", &two_forms, everything);
    report("a word that only turns what follows", &turning, everything);

    println!(
        "\n{} terms of one or two letters, which are worth reading rather than counting",
        short.values().map(Vec::len).sum::<usize>()
    );
    for (term, seen) in &short {
        let first = seen
            .iter()
            .take(3)
            .map(|(app_id, row)| format!("{app_id} {row}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {term:<6} {:>3}  {first}", seen.len());
    }
    Ok(())
}

/// Whether a term is written in a script that puts spaces between words, so that its length
/// says something: two Chinese characters are a word and two English letters are not.
fn spaced(term: &str) -> bool {
    !term
        .chars()
        .any(steamgauge_core::claims::writes_without_spaces)
}

fn report(what: &str, found: &[String], everything: bool) {
    println!("{what}: {}", found.len());
    let shown = if everything { found.len() } else { 10 };
    for line in found.iter().take(shown) {
        println!("  {line}");
    }
    if found.len() > shown {
        println!("  ... and {} more, with --all", found.len() - shown);
    }
}
