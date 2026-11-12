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

/// Why the snapshot is refused, where it lies in a folder spelt like a game's.
///
/// A game's folder in the library holds parts the window may be preparing, and the path can
/// reach it under another name: other letters, or a link.
fn refusal(snapshot: &std::path::Path) -> std::io::Result<Option<String>> {
    for resolved in [
        std::path::absolute(snapshot)?,
        std::fs::canonicalize(snapshot)?,
    ] {
        if let Some(name) = in_a_game_folder(&resolved) {
            return Ok(Some(format!(
                "This example removes a snapshot's vectors, so it refuses one in a folder named \
                 like a game's, here `{name}`. Copy the snapshot folder alone into a folder not \
                 named `appid=<id>`."
            )));
        }
    }
    Ok(None)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("name the snapshot of a read game")?,
    );
    if let Some(why) = refusal(&snapshot)? {
        return Err(why.into());
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

#[cfg(test)]
mod tests {
    use super::refusal;
    use std::path::{Path, PathBuf};

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "steamgauge-prepare-cost-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn snapshot(&self, within: &str) -> PathBuf {
            let snapshot = self.0.join(within);
            std::fs::create_dir_all(&snapshot).unwrap();
            snapshot
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn named(snapshot: &Path) -> Option<String> {
        refusal(snapshot)
            .unwrap()
            .map(|why| why.split('`').nth(1).unwrap().to_owned())
    }

    #[test]
    fn a_snapshot_in_a_folder_spelt_like_a_games_is_refused_in_any_case() {
        let scratch = Scratch::new("named");
        for (within, name) in [
            ("appid=7/snapshot=5", Some("appid=7")),
            ("APPID=8/snapshot=5", Some("APPID=8")),
            ("Appid=9/snapshot=5", Some("Appid=9")),
            ("copy/snapshot=5", None),
            ("appid=/snapshot=5", None),
            ("appid=7x/snapshot=5", None),
        ] {
            assert_eq!(
                named(&scratch.snapshot(within)).as_deref(),
                name,
                "{within}"
            );
        }
    }

    /// A junction made by `mklink`, whose paths are quoted since `cmd` splits at `=`.
    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) {
        use std::os::windows::process::CommandExt as _;
        let made = std::process::Command::new("cmd")
            .raw_arg(format!(
                "/c mklink /J \"{}\" \"{}\"",
                link.display(),
                target.display()
            ))
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
    }

    #[cfg(windows)]
    #[test]
    fn a_snapshot_reached_through_a_junction_is_refused_by_either_name() {
        let scratch = Scratch::new("junction");
        let in_a_game = scratch.snapshot("appid=7/snapshot=5");
        junction(&scratch.0.join("short"), &in_a_game);
        assert_eq!(named(&scratch.0.join("short")).as_deref(), Some("appid=7"));

        scratch.snapshot("big/snapshot=5");
        junction(&scratch.0.join("appid=9"), &scratch.0.join("big"));
        assert_eq!(
            named(&scratch.0.join("appid=9/snapshot=5")).as_deref(),
            Some("appid=9")
        );
        assert_eq!(named(&scratch.0.join("big/snapshot=5")), None);
    }
}
