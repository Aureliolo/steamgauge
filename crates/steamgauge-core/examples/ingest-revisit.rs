//! Ingests a labelled revisit set, as `steamgauge ingest-revisit` does, without building the app.
//!
//! Every label is stamped with the sheet the build carries, so a revisit labelled against a
//! sheet newer than the release binary has to be ingested by a build that has it, and while a
//! training run holds the card only the core crate may be built.
//!
//!     tools/cargo-beside-training.sh run -p steamgauge-core --example ingest-revisit -- \
//!         <set dir> <returned dir> <labeller>

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [dir, from, by] = &args[..] else {
        eprintln!("usage: ingest-revisit <set dir> <returned dir> <labeller>");
        std::process::exit(2);
    };
    let report = steamgauge_core::claimset::ingest_revisit(
        std::path::Path::new(dir),
        std::path::Path::new(from),
        by,
    )?;
    println!(
        "revisited {} moved {} unknown {} rejected {}",
        report.accepted,
        report.moved,
        report.unknown.len(),
        report.rejected.len()
    );
    Ok(())
}
