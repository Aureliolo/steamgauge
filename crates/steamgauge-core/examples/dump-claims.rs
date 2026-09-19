//! Every claim this build cuts from a game, one per line, for diffing against another build.
//!
//! A splitter change is judged by what it stops emitting, and a count of what it dropped is
//! not evidence that dropping it was right. This writes `<review id>\t<claim>` so two builds
//! can be compared line for line and the difference read rather than counted.
//!
//!     cargo run --release -p steamgauge-core --example dump-claims -- 1248130 > before.tsv

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_id: u32 = std::env::args()
        .nth(1)
        .and_then(|one| one.parse().ok())
        .ok_or("usage: dump-claims <app id>")?;
    let out = std::env::var("OUT_DIR_DATA").unwrap_or_else(|_| "data".to_owned());
    let snapshot = steamgauge_core::embed::latest_snapshot(std::path::Path::new(&out), app_id)?;

    let mut buffer = String::new();
    steamgauge_core::capture::for_each_body(&snapshot, |id, _, text| {
        for claim in steamgauge_core::claims::split(text) {
            buffer.push_str(id);
            buffer.push('\t');
            // One line each, so the claim's own newlines cannot be read as a claim boundary.
            buffer.push_str(&claim.replace('\n', "\\n").replace('\r', ""));
            buffer.push('\n');
        }
        Ok(())
    })?;
    print!("{buffer}");
    Ok(())
}
