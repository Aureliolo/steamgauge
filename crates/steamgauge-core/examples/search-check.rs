//! What a search finds in one read game, and how long it takes to find it.
//!
//! The window runs the same search while somebody waits, and it walks the whole capture, so
//! the time is as much the answer as the counts are.
//!
//!     cargo run --release -p steamgauge-core --example search-check -- data 920210 "steam deck"

use steamgauge_core::search::{Phrase, search};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(root), Some(app_id), Some(query)) = (args.next(), args.next(), args.next()) else {
        return Err("usage: search-check <library> <app id> <words>".into());
    };
    let snapshot = steamgauge_core::embed::latest_snapshot(root.as_ref(), app_id.parse()?)?;
    let phrase = Phrase::new(&query).ok_or("nothing to look for")?;

    let started = std::time::Instant::now();
    let said = search(&snapshot, &phrase)?;
    let took = started.elapsed();

    println!(
        "{} reviews, {} claims: {} praise, {} complaint, {} neutral, in {:.1} s",
        said.reviews,
        said.claims,
        said.praise,
        said.complaint,
        said.neutral,
        took.as_secs_f64()
    );
    for (subject, claims) in &said.subjects {
        println!("  {subject:<14} {claims}");
    }
    for (form, claims) in &said.forms {
        println!("  as \"{form}\" {claims}");
    }
    Ok(())
}
