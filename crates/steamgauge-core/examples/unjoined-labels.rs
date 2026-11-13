//! Which of a game's labels name a span this build no longer cuts, and what it cuts there now.
//!
//! `measure-claims` counts them. A count says how much of the set the splitter has moved past.
//! It cannot say whether the move was right, and that is the question every splitter change
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

use steamgauge_core::{claimset::ClaimLabel, figure_text::thousands};

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
    let ids: HashSet<String> = labels.iter().map(|label| label.review_id.clone()).collect();
    let capture =
        steamgauge_core::claimset::spans_cut_now(std::path::Path::new("data"), app_id, &ids)?;
    let kept = match steamgauge_core::claimset::spans_cut_in(&reference, &ids) {
        Ok(kept) => kept,
        Err(steamgauge_core::Error::NoKeptText { .. }) => HashMap::new(),
        Err(other) => return Err(other.into()),
    };

    let mut joined = 0_usize;
    let mut inside = Vec::new();
    let mut split = Vec::new();
    let mut lost = Vec::new();
    for label in &labels {
        let Some(now) = capture.get(&label.review_id) else {
            lost.push((label, "review not in the capture".to_owned()));
            continue;
        };
        if steamgauge_core::claimset::claim_in_capture(&kept, &capture, label).is_some() {
            joined += 1;
            continue;
        }
        if kept
            .get(&label.review_id)
            .is_some_and(|was| was.text != now.text)
        {
            lost.push((
                label,
                "the review reads differently in the capture".to_owned(),
            ));
            continue;
        }
        let text = &now.text;
        let Some((start, end)) = steamgauge_core::claims::words_at(text, label.start, label.end)
        else {
            lost.push((label, "span is not on character boundaries".to_owned()));
            continue;
        };
        let words = start..end;
        let mut overlapping: Vec<&(u32, u32)> = now
            .spans
            .iter()
            .filter(|(from, to)| *from < words.end && words.start < *to)
            .collect();
        overlapping.sort_unstable();
        let was = text[words.start as usize..words.end as usize].replace(['\n', '\r'], " ");
        let are: Vec<String> = overlapping
            .iter()
            .map(|&&(from, to)| text[from as usize..to as usize].replace(['\n', '\r'], " "))
            .collect();
        match overlapping.len() {
            0 => lost.push((label, format!("nothing cut there: {was:?}"))),
            1 => inside.push((label, was, are)),
            _ => split.push((label, was, are)),
        }
    }

    println!(
        "app {app_id}: {} labels, {} still cut as one claim, {} now inside a larger \
         claim, {} now cut into several, {} lost",
        thousands(labels.len() as u64),
        thousands(joined as u64),
        thousands(inside.len() as u64),
        thousands(split.len() as u64),
        thousands(lost.len() as u64)
    );

    for (title, group) in [
        ("inside a larger claim", &inside),
        ("cut into several", &split),
    ] {
        println!("\n== {title}: {} ==", thousands(group.len() as u64));
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
        println!("\n== lost: {} ==", thousands(lost.len() as u64));
        for (label, why) in lost.iter().take(show) {
            println!(
                "[{}/{} {}] {why}",
                label.review_id, label.index, label.subject
            );
        }
    }
    Ok(())
}
