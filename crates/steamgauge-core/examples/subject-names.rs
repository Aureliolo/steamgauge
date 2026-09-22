//! The terms most of a language's reviews use about each subject, praise and complaint
//! together, which is what the subject is called in that language.
//!
//! A subject's own name is not a finding, and the page keeps the English one off its rows;
//! this is how the names in the other languages are found rather than guessed, so that the
//! list the page keeps off can be written from the corpus.
//!
//!     cargo run --release -p steamgauge-core --example subject-names -- <app id> <language> [how many]

use std::collections::{HashMap, HashSet};

/// One filed claim: where it sits in its review and its subject.
type Filed = ((u32, u32), String);

/// How many of a language's reviews raise a subject, and how many use each term doing so.
type Counted = (u64, HashMap<String, u64>);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(app_id), Some(language)) = (
        args.next().and_then(|id| id.parse::<u32>().ok()),
        args.next(),
    ) else {
        eprintln!("usage: subject-names <app id> <language> [how many]");
        std::process::exit(2);
    };
    let shown: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(12);

    let snapshot = steamgauge_core::embed::latest_snapshot(std::path::Path::new("data"), app_id)?;
    let mut filed: HashMap<String, Vec<Filed>> = HashMap::new();
    steamgauge_core::read::for_each_reading(
        &snapshot.join("readings.parquet"),
        |id, at, subject, _, polarity| {
            if let Some(subject) = subject
                && polarity != "neutral"
            {
                filed
                    .entry(id.to_owned())
                    .or_default()
                    .push((at, subject.to_owned()));
            }
        },
    )?;

    // subject -> (reviews, term -> reviews)
    let mut counts: HashMap<String, Counted> = HashMap::new();
    steamgauge_core::capture::for_each_row(&snapshot, |row, text| {
        if row.language != language {
            return Ok(());
        }
        let Some(claims) = filed.get(&row.recommendationid) else {
            return Ok(());
        };
        let mut heard: HashSet<(String, String)> = HashSet::new();
        let mut subjects: HashSet<&str> = HashSet::new();
        for (at, subject) in claims {
            let Some(claim) = text.get(at.0 as usize..at.1 as usize) else {
                continue;
            };
            subjects.insert(subject);
            steamgauge_core::said::each_term(claim, |term| {
                heard.insert((subject.clone(), term.to_owned()));
            });
        }
        for subject in subjects {
            counts.entry(subject.to_owned()).or_default().0 += 1;
        }
        for (subject, term) in heard {
            *counts
                .entry(subject)
                .or_default()
                .1
                .entry(term)
                .or_default() += 1;
        }
        Ok(())
    })?;

    let mut subjects: Vec<(&String, &Counted)> = counts.iter().collect();
    subjects.sort_by_key(|(_, (reviews, _))| std::cmp::Reverse(*reviews));
    for (subject, (reviews, terms)) in subjects {
        let mut ranked: Vec<(&String, &u64)> = terms.iter().collect();
        ranked.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
        println!("{subject} ({reviews} {language} reviews)");
        for (term, count) in ranked.into_iter().take(shown) {
            #[expect(
                clippy::cast_precision_loss,
                reason = "review counts are far below 2^53"
            )]
            let share = *count as f64 / *reviews as f64;
            println!("  {share:5.1}%  {count:>6}  {term}", share = share * 100.0);
        }
    }
    Ok(())
}
