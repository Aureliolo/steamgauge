//! Draws a revisit, as `steamgauge revisit` does, without building the app, and with several
//! questions at once.
//!
//! A round that revises several rules asks a different question of each: the remapping words
//! under `controls` and `accessibility` everywhere, a headset's controllers only in headset
//! games. The command asks one, and a second draw over the same game replaces the first.
//!
//!     tools/cargo-beside-training.sh run -p steamgauge-core --example draw-revisit -- \
//!         <questions.json> [reference dir] [reviews per batch]
//!
//! The questions file is a JSON array of `{"words": [...], "subjects": [...], "apps": [...]}`,
//! each field optional, words or subjects required.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(questions) = args.first() else {
        eprintln!("usage: draw-revisit <questions.json> [reference dir] [reviews per batch]");
        std::process::exit(2);
    };
    let reference = args.get(1).map_or("reference/claims", String::as_str);
    let per_batch = args.get(2).map_or(Ok(40), |n| n.parse())?;
    let questions: Vec<steamgauge_core::claimset::Question> =
        serde_json::from_slice(&std::fs::read(questions)?)?;

    let drawn = steamgauge_core::claimset::draw_revisits(
        std::path::Path::new(reference),
        &questions,
        per_batch,
    )?;
    let mut claims = 0;
    for set in &drawn.sets {
        println!(
            "{:<20} {:>4} reviews {:>5} claims {:>3} batches",
            format!("{}/{}", set.app_id, set.set).trim_end_matches('/'),
            set.report.reviews,
            set.report.claims,
            set.report.batches
        );
        claims += set.report.claims;
    }
    println!(
        "drawn {claims} claims in {} sets; cleared {}",
        drawn.sets.len(),
        drawn.cleared
    );
    Ok(())
}
