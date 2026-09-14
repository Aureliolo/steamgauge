//! How much of what labellers called a bad split this build still splits that way.
//!
//! A labeller marks a claim `split_wrong` when the piece they were handed makes more than one
//! point, or less than one. Those marks were made against the splitter of the day, and five
//! versions of rules have landed since the first of them. A rule already fixed still shows up
//! in the training summary as a mis-split claim, which makes the splitter look worse than it
//! is and hides whatever is still wrong with it.
//!
//! This re-cuts each flagged claim's review with the splitter this build has and asks whether
//! the piece the labeller objected to is still one piece.
//!
//! Broken out by the subset a claim was drawn in, because the published mis-split rate is over
//! the random draws alone: a teaching draw is chosen for being hard and says nothing about how
//! often an ordinary review is cut wrong.
//!
//!     cargo run --release -p steamgauge-core --example stale-splits -- training/data/claims.jsonl

use std::collections::HashMap;

use steamgauge_core::read::Depth;

#[derive(serde::Deserialize)]
struct Row {
    text: String,
    review: Option<String>,
    split_wrong: Option<bool>,
    app_id: u32,
    subset: Option<String>,
}

/// Whether a claim holds nothing a splitter could have cut on.
///
/// The rules this build has are punctuation and markup. A claim with no line break and at most
/// one comma of any script has offered them nothing, so it is not a rule that is missing: it is
/// a sentence that carries two subjects in ordinary grammar.
fn no_punctuation_to_cut_on(text: &str) -> bool {
    !text.contains('\n')
        && text
            .chars()
            .filter(|c| matches!(c, ',' | '\u{FF0C}' | '\u{3001}'))
            .count()
            <= 1
}

/// Whether a claim's two halves are joined by a word rather than a mark.
///
/// "The story and graphics were outstanding" is two subjects in four words. Cutting on the word
/// would shred every ordinary sentence that uses it, so this counts the shape rather than
/// proposing a rule for it.
fn joined_by_a_conjunction(text: &str) -> bool {
    let lower = text.to_lowercase();
    ["and", "but", "although"].iter().any(|word| {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|part| part == *word)
    })
}

/// What one draw's claims say about the splitter, then and now.
#[derive(Default)]
struct Tally {
    /// Claims labelled in this subset, whether or not anybody objected to the cut.
    seen: usize,
    flagged: usize,
    still: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "training/data/claims.jsonl".to_owned());
    let show: usize = std::env::var("SHOW")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);

    let mut flagged = 0_usize;
    let mut still = 0_usize;
    let mut gone = 0_usize;
    let mut missing = 0_usize;
    let mut by_game: HashMap<u32, (usize, usize)> = HashMap::new();
    let mut by_subset: HashMap<String, Tally> = HashMap::new();
    let mut unreachable_by_punctuation = 0_usize;
    let mut joined = 0_usize;
    let mut shown = 0_usize;

    for line in std::fs::read_to_string(&path)?.lines() {
        let row: Row = serde_json::from_str(line)?;
        let subset = by_subset
            .entry(row.subset.clone().unwrap_or_else(|| "unsaid".to_owned()))
            .or_default();
        subset.seen += 1;
        if row.split_wrong != Some(true) {
            continue;
        }
        subset.flagged += 1;
        flagged += 1;
        let Some(review) = row.review.as_deref() else {
            missing += 1;
            continue;
        };
        let pieces = Depth::Deep.claims_of(review);
        let entry = by_game.entry(row.app_id).or_default();
        if pieces.iter().any(|piece| piece.as_ref() == row.text) {
            still += 1;
            entry.0 += 1;
            subset.still += 1;
            if no_punctuation_to_cut_on(&row.text) {
                unreachable_by_punctuation += 1;
            }
            if joined_by_a_conjunction(&row.text) {
                joined += 1;
            }
            if shown < show {
                shown += 1;
                println!("still one piece: {:?}", row.text);
            }
        } else {
            gone += 1;
            entry.1 += 1;
        }
    }

    println!("{flagged} claims a labeller called badly split");
    if missing > 0 {
        println!("  {missing} carry no review text, so this build cannot re-cut them");
    }
    let judged = still + gone;
    if judged > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a reference set is thousands of claims, not quadrillions"
        )]
        let share = |n: usize| 100.0 * n as f64 / judged as f64;
        println!("  {still} are still one piece ({:.1}%)", share(still));
        println!("  {gone} are cut differently now ({:.1}%)", share(gone));
    }
    if still > 0 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a reference set is thousands of claims, not quadrillions"
        )]
        let share = |n: usize| 100.0 * n as f64 / still as f64;
        println!(
            "  of the ones still cut that way, {unreachable_by_punctuation} ({:.1}%) hold no \
             line break and at most one comma, and {joined} ({:.1}%) join their clauses with a \
             word",
            share(unreachable_by_punctuation),
            share(joined)
        );
    }
    breakdown(by_subset, by_game);
    Ok(())
}

/// The rate one draw at a time, and the games with the most left to fix.
fn breakdown(by_subset: HashMap<String, Tally>, by_game: HashMap<u32, (usize, usize)>) {
    let mut draws: Vec<(String, Tally)> = by_subset.into_iter().collect();
    draws.sort_by_key(|(_, tally)| std::cmp::Reverse(tally.seen));
    println!("\n            claims  called badly split  still cut that way");
    for (name, tally) in draws {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a reference set is thousands of claims, not quadrillions"
        )]
        let share = |n: usize| 100.0 * n as f64 / tally.seen.max(1) as f64;
        println!(
            "{name:>12} {:>7} {:>9} ({:>4.1}%) {:>9} ({:>4.1}%)",
            tally.seen,
            tally.flagged,
            share(tally.flagged),
            tally.still,
            share(tally.still),
        );
    }

    let mut worst: Vec<(u32, (usize, usize))> = by_game.into_iter().collect();
    worst.sort_by_key(|(_, (still, _))| std::cmp::Reverse(*still));
    for (app_id, (still, gone)) in worst.into_iter().take(8) {
        println!("  app {app_id}: {still} still, {gone} fixed");
    }
}
