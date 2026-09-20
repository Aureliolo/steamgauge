//! Which of a game's labels name a span this build no longer cuts, and what it cuts there now.
//!
//! `measure-claims` counts them. A count says how much of the set the splitter has moved past;
//! it cannot say whether the move was right, and that is the question every splitter change
//! puts. This prints each such label beside the claims this build cuts across the same bytes,
//! so the change can be read: a ballot answer now carrying its heading is a repair, and a
//! sentence now cut in two is a regression.
//!
//! Grouped by what happened, because the two are not the same finding: a label whose bytes
//! now sit inside one larger claim (the splitter joined) and one whose bytes are now cut into
//! several (the splitter split).
//!
//!     cargo run --release -p steamgauge-core --example unjoined-labels -- 774361
//!     SHOW=20 cargo run --release -p steamgauge-core --example unjoined-labels -- 774361

use std::collections::{HashMap, HashSet};

use steamgauge_core::claimset::ClaimLabel;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_id: u32 = std::env::args()
        .nth(1)
        .ok_or("usage: unjoined-labels <app_id>")?
        .parse()?;
    let show: usize = std::env::var("SHOW")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(8);

    let reference = steamgauge_core::claimset::default_reference_dir(app_id);
    let labels: Vec<ClaimLabel> =
        serde_json::from_slice(&std::fs::read(reference.join("labels.json"))?)?;
    let snapshot = steamgauge_core::embed::latest_snapshot(std::path::Path::new("data"), app_id)?;
    let ids: HashSet<String> = labels.iter().map(|label| label.review_id.clone()).collect();
    let texts = steamgauge_core::capture::texts_for(&snapshot, &ids)?;

    let mut cut: HashMap<&str, Vec<std::ops::Range<usize>>> = HashMap::new();
    for (id, text) in &texts {
        cut.insert(id, steamgauge_core::claims::spans(text));
    }

    let mut joined = 0_usize;
    let mut inside = Vec::new();
    let mut split = Vec::new();
    let mut lost = Vec::new();
    for label in &labels {
        let Some(text) = texts.get(&label.review_id) else {
            lost.push((label, "review not in the capture".to_owned()));
            continue;
        };
        let Some((start, end)) = steamgauge_core::claims::words_at(text, label.start, label.end)
        else {
            lost.push((label, "span is not on character boundaries".to_owned()));
            continue;
        };
        let words = start as usize..end as usize;
        let now = &cut[label.review_id.as_str()];
        if now.contains(&words) {
            joined += 1;
            continue;
        }
        let overlapping: Vec<&std::ops::Range<usize>> = now
            .iter()
            .filter(|span| span.start < words.end && words.start < span.end)
            .collect();
        let was = text[words.clone()].replace(['\n', '\r'], " ");
        let are: Vec<String> = overlapping
            .iter()
            .map(|span| text[(*span).clone()].replace(['\n', '\r'], " "))
            .collect();
        match overlapping.len() {
            0 => lost.push((label, format!("nothing cut there: {was:?}"))),
            1 => inside.push((label, was, are)),
            _ => split.push((label, was, are)),
        }
    }

    println!(
        "app {app_id}: {} labels, {joined} still cut as one claim, {} now inside a larger \
         claim, {} now cut into several, {} lost",
        labels.len(),
        inside.len(),
        split.len(),
        lost.len()
    );

    for (title, group) in [
        ("inside a larger claim", &inside),
        ("cut into several", &split),
    ] {
        println!("\n== {title}: {} ==", group.len());
        for (label, was, are) in group.iter().take(show) {
            println!(
                "[{}/{} {}] was: {:?}",
                label.review_id, label.index, label.subject, was
            );
            for claim in are {
                println!("    now: {claim:?}");
            }
        }
    }
    if !lost.is_empty() {
        println!("\n== lost: {} ==", lost.len());
        for (label, why) in lost.iter().take(show) {
            println!(
                "[{}/{} {}] {why}",
                label.review_id, label.index, label.subject
            );
        }
    }
    Ok(())
}
