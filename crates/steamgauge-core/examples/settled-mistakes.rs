//! The claims two labellers settled and the model read differently, which is the only list of
//! its mistakes nobody can argue with.
//!
//! Agreement with one labeller mixes the model's errors with the labeller's. Where a set has
//! been read twice and both readings reached the same subject, a disagreement is the model's,
//! and a list of those grouped by what it said instead is where a pattern shows up.
//!
//!     cargo run --release -p steamgauge-core --example settled-mistakes -- <app id>... [--quote]

use std::collections::HashMap;

use steamgauge_core::claimset::ClaimLabel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut apps = Vec::new();
    let mut quote = false;
    for arg in std::env::args().skip(1) {
        if arg == "--quote" {
            quote = true;
        } else if let Ok(app_id) = arg.parse::<u32>() {
            apps.push(app_id);
        }
    }
    if apps.is_empty() {
        eprintln!("usage: settled-mistakes <app id>... [--quote]");
        std::process::exit(2);
    }

    // (what both labellers said, what the model said) -> the claims behind it
    let mut mistakes: HashMap<(String, String), Vec<String>> = HashMap::new();
    let (mut settled, mut agreed) = (0_u64, 0_u64);
    // Settled and agreed per language, because a model that reads one language worse is a
    // different problem from one that reads one subject worse, and the totals hide it.
    let mut by_language: HashMap<String, (u64, u64)> = HashMap::new();

    for app_id in apps {
        let reference = steamgauge_core::claimset::default_reference_dir(app_id);
        let read =
            |path: std::path::PathBuf| -> Result<Vec<ClaimLabel>, Box<dyn std::error::Error>> {
                Ok(serde_json::from_slice(&std::fs::read(path)?)?)
            };
        let first = read(reference.join("labels.json"))?;
        let second = read(reference.join("second").join("labels.json"))?;
        let theirs: HashMap<(&str, u16), &ClaimLabel> = second
            .iter()
            .map(|label| ((label.review_id.as_str(), label.index), label))
            .collect();

        let snapshot =
            steamgauge_core::embed::latest_snapshot(std::path::Path::new("data"), app_id)?;
        let mut said: HashMap<(String, (u32, u32)), String> = HashMap::new();
        steamgauge_core::read::for_each_reading(
            &snapshot.join("readings.parquet"),
            |id, at, subject, _, _| {
                if let Some(subject) = subject {
                    said.insert((id.to_owned(), at), subject.to_owned());
                }
            },
        )?;

        let ids: std::collections::HashSet<String> =
            first.iter().map(|label| label.review_id.clone()).collect();
        let texts = steamgauge_core::capture::texts_for(&snapshot, &ids)?;

        for label in &first {
            let Some(other) = theirs.get(&(label.review_id.as_str(), label.index)) else {
                continue;
            };
            if other.subject != label.subject {
                continue;
            }
            let Some(read_as) = said.get(&(label.review_id.clone(), (label.start, label.end)))
            else {
                continue;
            };
            settled += 1;
            let spoken = by_language.entry(label.language.clone()).or_default();
            spoken.0 += 1;
            if read_as == &label.subject {
                agreed += 1;
                spoken.1 += 1;
                continue;
            }
            let text = texts
                .get(label.review_id.as_str())
                .and_then(|text| text.get(label.start as usize..label.end as usize))
                .unwrap_or_default();
            mistakes
                .entry((label.subject.clone(), read_as.clone()))
                .or_default()
                .push(text.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }

    let mut ranked: Vec<((String, String), Vec<String>)> = mistakes.into_iter().collect();
    ranked.sort_by_key(|(pair, claims)| (std::cmp::Reverse(claims.len()), pair.clone()));

    #[expect(clippy::cast_precision_loss, reason = "claim counts are small")]
    let share = if settled > 0 {
        100.0 * agreed as f64 / settled as f64
    } else {
        0.0
    };
    println!("{settled} settled claims answered, {agreed} of them agreed with ({share:.1}%)\n");
    for ((labelled, read_as), claims) in ranked {
        println!(
            "{:>4}  labelled {labelled}, read as {read_as}",
            claims.len()
        );
        if quote {
            for claim in claims.iter().take(6) {
                println!("        {}", claim.chars().take(120).collect::<String>());
            }
        }
    }

    let mut spoken: Vec<(String, (u64, u64))> = by_language.into_iter().collect();
    spoken.sort_by_key(|(name, (total, _))| (std::cmp::Reverse(*total), name.clone()));
    println!("\nby language, over the settled claims it answered");
    for (name, (total, right)) in spoken {
        #[expect(clippy::cast_precision_loss, reason = "claim counts are small")]
        let rate = 100.0 * right as f64 / total as f64;
        println!("  {name:<12} {right:>5} of {total:<5} {rate:>5.1}%");
    }
    Ok(())
}
