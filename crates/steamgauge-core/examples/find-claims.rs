//! Does this corpus say a particular thing, how often, and in which language?
//!
//! A rate per subject answers "what do players talk about". It cannot answer "do they say this",
//! and that is the question a reader of a report reaches for the moment a number surprises them.
//! A subject is twenty-six buckets; a phrase is what somebody actually wrote.
//!
//! Counted by reviewer as well as by claim, because one person saying a thing nine times is one
//! person, which is the same distortion the headline rate exists to avoid.
//!
//!     cargo run --release -p steamgauge-core --example find-claims -- 3553210 "too easy" "no content"
//!     LANGUAGE=japanese SHOW=6 cargo run --release -p steamgauge-core --example find-claims -- 3553210 飽き
//!
//! Matching is by substring, lowercased. That is blunt on purpose: a stemmer would need a
//! language, and the corpora this is pointed at are not in one.

use std::collections::{HashMap, HashSet};

use steamgauge_core::read::Depth;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let app_id: u32 = args
        .next()
        .and_then(|id| id.parse().ok())
        .ok_or("usage: find-claims <app id> <phrase> [phrase...]")?;
    let phrases: Vec<String> = args.map(|one| one.to_lowercase()).collect();
    if phrases.is_empty() {
        return Err("give at least one phrase to look for".into());
    }
    let only = std::env::var("LANGUAGE").ok().filter(|one| !one.is_empty());
    let show: usize = std::env::var("SHOW")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(3);
    let out = std::env::var("OUT").unwrap_or_else(|_| "data".to_owned());

    let snapshot = steamgauge_core::embed::latest_snapshot(std::path::Path::new(&out), app_id)?;

    let mut reviews = 0_usize;
    let mut claims = 0_usize;
    let mut hits: HashMap<&str, usize> = HashMap::new();
    let mut reviewers: HashMap<&str, HashSet<String>> = HashMap::new();
    let mut quoted: HashMap<&str, Vec<String>> = HashMap::new();

    steamgauge_core::capture::for_each_row(&snapshot, |row, text| {
        if only.as_deref().is_some_and(|want| row.language != want) {
            return Ok(());
        }
        reviews += 1;
        for claim in Depth::Deep.claims_of(text) {
            claims += 1;
            let lowered = claim.to_lowercase();
            for phrase in &phrases {
                if lowered.contains(phrase.as_str()) {
                    *hits.entry(phrase).or_default() += 1;
                    reviewers
                        .entry(phrase)
                        .or_default()
                        .insert(row.recommendationid.clone());
                    let seen = quoted.entry(phrase).or_default();
                    if seen.len() < show {
                        seen.push(claim.trim().to_owned());
                    }
                }
            }
        }
        Ok(())
    })?;

    println!(
        "{reviews} reviews, {claims} claims{}",
        only.as_deref()
            .map(|one| format!(", language {one}"))
            .unwrap_or_default()
    );
    #[expect(
        clippy::cast_precision_loss,
        reason = "a corpus is millions of claims, not quadrillions"
    )]
    let share = |count: usize, whole: usize| 100.0 * count as f64 / whole.max(1) as f64;
    for phrase in &phrases {
        let found = hits.get(phrase.as_str()).copied().unwrap_or(0);
        let people = reviewers
            .get(phrase.as_str())
            .map_or(0, std::collections::HashSet::len);
        println!(
            "\n{phrase:?}: {found} claims ({:.2}%), {people} reviewers ({:.2}%)",
            share(found, claims),
            share(people, reviews)
        );
        for one in quoted.get(phrase.as_str()).into_iter().flatten() {
            let cut: String = one.chars().take(180).collect();
            println!("   {cut}");
        }
    }
    Ok(())
}
