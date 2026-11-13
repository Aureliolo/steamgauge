//! What the splitter changes cost the labels already made.
//!
//! A label names the bytes it covers, and the sets were cut by several splitters over the run.
//! Where this build no longer cuts a claim at a label's span, the adjudication page already
//! refuses to ask about it. Training does not: the export joins a label to the text stored in
//! the draw, which is what the splitter said then, so the model is taught spans it will never
//! be handed at read time. The labels are read in the text kept for them, the text their bytes
//! count in.
//!
//! Whether that matters depends on which way the drift went, and the two ways are not
//! symmetric. Where this build now cuts coarser, the old label is a true label of a sentence
//! that still exists inside a larger claim, and training on it teaches something true. Where
//! it cuts finer, one label covers what are now two claims, so a row carrying two subjects is
//! being taught under one. This counts both.
//!
//!     cargo run --release --example recut-labels

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use steamgauge_core::{
    claimset::{ClaimLabel, TEACHING_SETS, spans_cut_in},
    figure_text::thousands,
};

#[derive(Default)]
struct Tally {
    labels: u64,
    still: u64,
    inside: u64,
    covers: u64,
    straddles: u64,
    gone: u64,
}

impl Tally {
    fn print_row(&self, name: &str) {
        println!(
            "{:<14}{:>8}{:>8}{:>8}{:>8}{:>9}{:>7}",
            name,
            thousands(self.labels),
            thousands(self.still),
            thousands(self.inside),
            thousands(self.covers),
            thousands(self.straddles),
            thousands(self.gone)
        );
    }
}

fn main() -> steamgauge_core::Result<()> {
    let reference = Path::new("reference/claims");
    let mut by_subset: BTreeMap<String, Tally> = BTreeMap::new();
    let mut games = 0;
    let mut unkept = 0;

    for entry in std::fs::read_dir(reference)? {
        let dir = entry?.path();
        if dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
            .is_none()
        {
            continue;
        }
        let sets: Vec<std::path::PathBuf> = std::iter::once(dir.clone())
            .chain(TEACHING_SETS.iter().map(|name| dir.join(name)))
            .filter(|set| set.join("labels.json").exists())
            .collect();
        if sets.is_empty() {
            continue;
        }
        let mut labels: Vec<ClaimLabel> = Vec::new();
        for set in sets {
            labels.extend(serde_json::from_slice::<Vec<ClaimLabel>>(&std::fs::read(
                set.join("labels.json"),
            )?)?);
        }
        let wanted: HashSet<String> = labels.iter().map(|label| label.review_id.clone()).collect();
        let Ok(cut) = spans_cut_in(&dir, &wanted) else {
            unkept += 1;
            continue;
        };
        games += 1;

        for label in &labels {
            let tally = by_subset.entry(label.subset.clone()).or_default();
            tally.labels += 1;
            let Some(spans) = cut.get(&label.review_id).map(|held| &held.spans) else {
                tally.gone += 1;
                continue;
            };
            let (start, end) = (label.start, label.end);
            let overlapping: Vec<&(u32, u32)> = spans
                .iter()
                .filter(|(from, to)| *from < end && start < *to)
                .collect();
            match overlapping.as_slice() {
                [] => tally.gone += 1,
                [(from, to)] if *from == start && *to == end => tally.still += 1,
                [(from, to)] if *from <= start && end <= *to => tally.inside += 1,
                [_] => tally.straddles += 1,
                many if many.iter().all(|(from, to)| start <= *from && *to <= end) => {
                    tally.covers += 1;
                }
                _ => tally.straddles += 1,
            }
        }
    }

    println!(
        "{} games with kept text, {} without\n",
        thousands(games),
        thousands(unkept)
    );
    println!(
        "{:<14}{:>8}{:>8}{:>8}{:>8}{:>9}{:>7}",
        "subset", "labels", "still", "inside", "covers", "straddles", "gone"
    );
    let mut total = Tally::default();
    for (subset, tally) in &by_subset {
        tally.print_row(subset);
        total.labels += tally.labels;
        total.still += tally.still;
        total.inside += tally.inside;
        total.covers += tally.covers;
        total.straddles += tally.straddles;
        total.gone += tally.gone;
    }
    total.print_row("all");
    let moved = total.labels - total.still;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a reference set is tens of thousands of labels, not 2^53 of them"
    )]
    let share = 100.0 * moved as f64 / total.labels as f64;
    println!(
        "\n{} of {} labels name a span this build cuts no claim at ({:.1}%). Of those, \
         {} sit inside one claim it now cuts, {} cover more than one whole claim, {} overlap \
         a claim without either containing it or sitting inside it, and {} have no claim \
         overlapping them at all.",
        thousands(moved),
        thousands(total.labels),
        share,
        thousands(total.inside),
        thousands(total.covers),
        thousands(total.straddles),
        thousands(total.gone)
    );
    Ok(())
}
