//! Where two readings of the same corpus disagree, claim by claim.
//!
//! Two things about a reader can change without changing what it is: the batch a claim is read
//! in, and the graph it is read through. Neither should move an answer, and in half precision
//! both can, because a batch is padded to its longest claim and a fused kernel rounds where a
//! separate one did not. A rate is too coarse to see it: two readings can decline the same
//! share of the corpus and disagree about which claims.
//!
//! Joins on the claim rather than the row, so it is honest about two files of different
//! lengths: a claim in one and not the other is reported as such rather than shifting every
//! comparison after it by one.
//!
//!     cargo run --release -p steamgauge-core --example diff-readings -- a.parquet b.parquet

use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(left), Some(right)) = (args.next(), args.next()) else {
        eprintln!("usage: diff-readings <a.parquet> <b.parquet>");
        std::process::exit(2);
    };
    let show: usize = std::env::var("SHOW")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);

    let mut was: HashMap<(String, u16), (Option<String>, f32, String)> = HashMap::new();
    steamgauge_core::read::for_each_reading(
        std::path::Path::new(&left),
        |id, index, subject, confidence, polarity| {
            was.insert(
                (id.to_owned(), index),
                (subject.map(str::to_owned), confidence, polarity.to_owned()),
            );
        },
    )?;

    let mut shared = 0_usize;
    let mut only_right = 0_usize;
    let mut subject_moved = 0_usize;
    let mut polarity_moved = 0_usize;
    let mut spoke_up = 0_usize;
    let mut fell_silent = 0_usize;
    let mut drift = 0.0_f64;
    let mut worst = 0.0_f32;
    let mut shown = 0_usize;
    let mut seen: Vec<(String, u16)> = Vec::new();

    steamgauge_core::read::for_each_reading(
        std::path::Path::new(&right),
        |id, index, subject, confidence, polarity| {
            let key = (id.to_owned(), index);
            let Some((before, sure, was_polarity)) = was.get(&key) else {
                only_right += 1;
                return;
            };
            shared += 1;
            seen.push(key);
            drift += f64::from((confidence - sure).abs());
            worst = worst.max((confidence - sure).abs());
            if before.as_deref() != subject {
                subject_moved += 1;
                match (before.as_deref(), subject) {
                    (None, Some(_)) => spoke_up += 1,
                    (Some(_), None) => fell_silent += 1,
                    _ => {}
                }
                if shown < show {
                    shown += 1;
                    println!(
                        "{id}#{index}: {} -> {} ({sure:.3} -> {confidence:.3})",
                        before.as_deref().unwrap_or("declined"),
                        subject.unwrap_or("declined"),
                    );
                }
            }
            if was_polarity != polarity {
                polarity_moved += 1;
            }
        },
    )?;

    for key in seen {
        was.remove(&key);
    }
    let only_left = was.len();

    println!("\n{shared} claims in both");
    if only_left > 0 || only_right > 0 {
        println!("{only_left} only in the first, {only_right} only in the second");
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a library is tens of millions of claims, not quadrillions"
    )]
    let whole = shared.max(1) as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a library is tens of millions of claims, not quadrillions"
    )]
    let share = |count: usize| 100.0 * count as f64 / whole;
    println!(
        "{subject_moved} answers moved ({:.2}%), of which {spoke_up} were declined before and \
         {fell_silent} are declined now",
        share(subject_moved)
    );
    println!(
        "{polarity_moved} polarities moved ({:.2}%)",
        share(polarity_moved)
    );
    println!(
        "confidence drifted {:.2e} on average, {worst:.2e} at worst",
        drift / whole
    );
    Ok(())
}
