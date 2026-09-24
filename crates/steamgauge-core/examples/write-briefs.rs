//! Writes the three committed briefs from the sheet this build carries, as `steamgauge brief
//! reference` does, without building the app.
//!
//! The app crate pulls in the whole of Tauri, which cannot be built beside a training run, and a
//! rule written while the card is busy still has to reach the sheet the labellers read before
//! its revisit can be drawn.
//!
//!     tools/cargo-beside-training.sh run -p steamgauge-core --example write-briefs -- reference

use steamgauge_core::taxonomy::{Unit, induction_brief, labelling_brief};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(to) = std::env::args().nth(1) else {
        eprintln!("usage: write-briefs <directory>");
        std::process::exit(2);
    };
    let to = std::path::Path::new(&to);
    std::fs::create_dir_all(to)?;
    for (name, text) in [
        ("labelling-brief.txt", labelling_brief(Unit::Review)),
        ("claim-brief.txt", labelling_brief(Unit::Claim)),
        ("induction-brief.txt", induction_brief()),
    ] {
        let path = to.join(name);
        std::fs::write(&path, text)?;
        println!("brief    {}", path.display());
    }
    Ok(())
}
