//! What preparing a game for search costs on this machine: claims a second, and the longest the
//! card is held by one batch, which every other window on the machine waits behind to be drawn.
//!
//!     cargo run --release -p steamgauge-core --features directml --example prepare-cost -- \
//!         <snapshot of a read game, a copy outside the library>
//!
//! The snapshot's vectors are removed first, so each run prepares all of it.

use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use steamgauge_core::{meaning, model::default_cache_dir, search_models::SearchEncoder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("name the snapshot of a read game")?,
    );
    // Where `meaning` keeps a snapshot's vectors.
    let _ = std::fs::remove_dir_all(snapshot.join("meaning"));

    let loading = Instant::now();
    let mut encoder = SearchEncoder::load(&default_cache_dir())?;
    println!(
        "loaded on {} in {:.1} s",
        encoder.device(),
        loading.elapsed().as_secs_f64()
    );

    let mut batches = 0_u32;
    let mut busy = Duration::ZERO;
    let mut longest = Duration::ZERO;
    let mut held: Vec<Duration> = Vec::new();
    let started = Instant::now();
    let prepared = meaning::prepare(
        &snapshot,
        |texts| {
            let at = Instant::now();
            let vectors = encoder.claims(texts);
            let took = at.elapsed();
            batches += 1;
            busy += took;
            longest = longest.max(took);
            held.push(took);
            vectors
        },
        (&AtomicBool::new(false), 1.0),
        |_| {},
    )?;
    let wall = started.elapsed().as_secs_f64();
    held.sort_unstable();
    let at = |percent: usize| {
        held.get(held.len().saturating_sub(1) * percent / 100)
            .map_or(0, Duration::as_millis)
    };
    let embedded = u32::try_from(prepared.embedded)?;
    println!(
        "{embedded} claims in {wall:.1} s: {:.0} a second; {batches} batches, the card held \
         {:.1} s; a batch held it {} ms at the median, {} ms at the 99th percentile, {} ms at most",
        f64::from(embedded) / wall,
        busy.as_secs_f64(),
        at(50),
        at(99),
        longest.as_millis(),
    );
    Ok(())
}
