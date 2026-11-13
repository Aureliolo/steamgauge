//! What a game's themes come out as at one closeness, so the line at which two claims say the
//! same thing can be read off real text rather than guessed.
//!
//! Prepares the game for search by meaning first where it is not, which fetches the search
//! encoder the first time, then prints each subject's themes on each side with their counts.
//!
//!     cargo run --release -p steamgauge-core --features directml --example themes-check -- \
//!         <library> <app id> [alike [merged [shared]]]

use std::sync::atomic::AtomicBool;

use steamgauge_core::{
    figure_text::thousands,
    meaning::{self, Status},
    model::default_cache_dir,
    search_models::{ENCODER, SearchEncoder},
    themes,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [root, app_id, rest @ ..] = args.as_slice() else {
        return Err("usage: themes-check <library> <app id> [alike [merged [shared]]]".into());
    };
    let used = themes::Closeness::USED;
    let given = |at: usize, or: f32| rest.get(at).map_or(Ok(or), |value| value.parse::<f32>());
    let closeness = themes::Closeness {
        alike: given(0, used.alike)?,
        merged: given(1, used.merged)?,
        shared: given(2, used.shared)?,
    };
    // What an earlier release wrote is brought first, so a game it prepared is not prepared again.
    steamgauge_core::current_format::bring_library(std::path::Path::new(root))?;
    let snapshot =
        steamgauge_core::embed::latest_snapshot(std::path::Path::new(root), app_id.parse()?)?;

    if meaning::status(&snapshot) != Status::Ready {
        let cache = default_cache_dir();
        tokio::runtime::Runtime::new()?.block_on(ENCODER.ensure(&cache, |_| {}))?;
        let mut encoder = SearchEncoder::load(&cache)?;
        eprintln!("preparing on {}", encoder.device());
        let started = std::time::Instant::now();
        let stop = AtomicBool::new(false);
        let prepared = meaning::prepare(
            &snapshot,
            |texts| encoder.claims(texts),
            (&stop, 0.5),
            |_| {},
        )?;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a duration in seconds is a positive whole number"
        )]
        let seconds = thousands(started.elapsed().as_secs_f64().round() as u64);
        eprintln!(
            "{} claims embedded in {seconds} s",
            thousands(prepared.embedded)
        );
    }

    let started = std::time::Instant::now();
    let found = themes::find(&snapshot, closeness)?;
    eprintln!("themes found in {:.1} s", started.elapsed().as_secs_f64());
    for subject in &found.subjects {
        println!(
            "\n{} / {} ({} reviews)",
            subject.subject,
            subject.side,
            thousands(subject.reviews)
        );
        for theme in &subject.themes {
            let also = if theme.also_in.is_empty() {
                String::new()
            } else {
                let named: Vec<&str> = theme
                    .also_in
                    .iter()
                    .take(3)
                    .map(|place| place.subject.as_str())
                    .collect();
                format!("  [also {}]", named.join(", "))
            };
            println!(
                "  {:>5} reviews  {}{also}\n                 “{}”",
                thousands(theme.reviews),
                theme.label.replace('\n', " "),
                theme.quote.text.replace('\n', " ")
            );
        }
    }
    Ok(())
}
