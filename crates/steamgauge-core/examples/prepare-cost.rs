//! What preparing a game for search costs on this machine: claims a second, and the longest the
//! card is held by one batch, which every other window on the machine waits behind to be drawn.
//!
//!     cargo run --release -p steamgauge-core --features directml --example prepare-cost -- \
//!         <a copy of a read game's snapshot, in a folder not named appid=<id>>
//!
//! The snapshot's vectors are removed first, so each run prepares all of it.

use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use steamgauge_core::{meaning, model::default_cache_dir, search_models::SearchEncoder};

/// The name of the first parent folder spelt like a game's, `appid=` and digits in any case.
fn in_a_game_folder(snapshot: &std::path::Path) -> Option<String> {
    snapshot
        .ancestors()
        .skip(1)
        .filter_map(|folder| folder.file_name()?.to_str())
        .find(|name| {
            name.get(.."appid=".len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("appid="))
                && name.len() > "appid=".len()
                && name["appid=".len()..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit())
        })
        .map(str::to_owned)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("name the snapshot of a read game")?,
    );
    // A game's folder in the library holds parts the window may be preparing, and the path
    // can reach it under another name: other letters, or a link.
    for resolved in [
        std::path::absolute(&snapshot)?,
        std::fs::canonicalize(&snapshot)?,
    ] {
        if let Some(name) = in_a_game_folder(&resolved) {
            return Err(format!(
                "This example removes a snapshot's vectors, so it refuses one in a folder named \
                 like a game's, here `{name}`. Copy the snapshot folder alone into a folder not \
                 named `appid=<id>`."
            )
            .into());
        }
    }
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
