//! How many reviews use a term on each side of each subject, by the cut the page counts with.
//!
//! A term that stands out is chosen by comparing its count on one side of a subject with the
//! other side and with everywhere else, and when a term drops off a page after a change to
//! the cut, the counts are what say whether it was the cut or a bar. Each review counts once
//! per side of a subject, as the page counts it.
//!
//!     cargo run --release -p steamgauge-core --example term-count -- <app id> <term>...

use std::collections::{HashMap, HashSet};

/// One filed claim: where it sits in its review, its subject and its polarity.
type Filed = ((u32, u32), String, String);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let Some(app_id) = args.next().and_then(|id| id.parse::<u32>().ok()) else {
        eprintln!("usage: term-count <app id> <term>...");
        std::process::exit(2);
    };
    let terms: Vec<String> = args.collect();
    if terms.is_empty() {
        eprintln!("usage: term-count <app id> <term>...");
        std::process::exit(2);
    }

    let snapshot = steamgauge_core::embed::latest_snapshot(std::path::Path::new("data"), app_id)?;
    let mut filed: HashMap<String, Vec<Filed>> = HashMap::new();
    steamgauge_core::read::for_each_reading(
        &snapshot.join("readings.parquet"),
        |id, at, subject, _, polarity| {
            if let Some(subject) = subject
                && polarity != "neutral"
            {
                filed.entry(id.to_owned()).or_default().push((
                    at,
                    subject.to_owned(),
                    polarity.to_owned(),
                ));
            }
        },
    )?;

    // (term, subject, side) -> reviews
    let mut counts: HashMap<(usize, String, String), u64> = HashMap::new();
    steamgauge_core::capture::for_each_row(&snapshot, |row, text| {
        let Some(claims) = filed.get(&row.recommendationid) else {
            return Ok(());
        };
        let mut heard: HashSet<(usize, String, String)> = HashSet::new();
        for (at, subject, polarity) in claims {
            let Some(claim) = text.get(at.0 as usize..at.1 as usize) else {
                continue;
            };
            for (index, term) in terms.iter().enumerate() {
                if steamgauge_core::said::mentions(claim, term) {
                    heard.insert((index, subject.clone(), polarity.clone()));
                }
            }
        }
        for key in heard {
            *counts.entry(key).or_default() += 1;
        }
        Ok(())
    })?;

    for (index, term) in terms.iter().enumerate() {
        let mut rows: Vec<(&String, &String, u64)> = counts
            .iter()
            .filter(|((at, _, _), _)| *at == index)
            .map(|((_, subject, side), count)| (subject, side, *count))
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.2));
        let everywhere: u64 = rows.iter().map(|row| row.2).sum();
        println!("{term}: {everywhere} reviews everywhere");
        for (subject, side, count) in rows {
            println!("  {subject:<16} {side:<10} {count}");
        }
    }
    Ok(())
}
