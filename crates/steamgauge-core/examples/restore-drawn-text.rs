//! Puts each game's kept review text back to the text its labels were drawn on.
//!
//! A kept review that reads differently from the text its sample claims were cut from leaves
//! its labels on bytes that hold other words. This rebuilds the text from the samples and the
//! kept copy, moves the labels of a rebuilt review onto their claims, and prints the sums. A
//! review that cannot be rebuilt whole keeps its line. A second run changes nothing.
//!
//!     cargo run --locked -p steamgauge-core --example restore-drawn-text -- reference/claims

use std::path::Path;

use steamgauge_core::drawn_text::{Restored, restore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .ok_or("usage: restore-drawn-text <reference folder>")?;
    let mut games: Vec<_> = std::fs::read_dir(Path::new(&root))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    games.sort();
    let mut total = Restored::default();
    for game in games {
        total += restore(&game)?;
    }
    println!("rebuilt {}", total.rebuilt);
    println!("left_as_kept {}", total.left_as_kept);
    println!("no_kept_text {}", total.no_kept_text);
    println!("labels_moved {}", total.labels_moved);
    Ok(())
}
