//! Fills in the parts of a reading the reader can work out without reading anything again.
//!
//! Some of what a reading carries is not an answer but a restatement of the rule that produced
//! it: which languages had no line, which had a harder one than English. Those cost no forward
//! pass, so a reading made before the field existed can be completed from the reader that made
//! it rather than by spending hours asking the card the same questions twice.
//!
//! The safety is `read_by_rule`. A reading made under a different rule has different *answers*,
//! not just different restatements, and no amount of recomputing fixes that: it has to be read
//! again. So this refuses every reading whose rule is not the installed reader's, and says which.
//!
//!     cargo run --release -p steamgauge-core --example restate-readings -- data models/game-review-reader

use std::path::Path;

use steamgauge_core::reader::Provenance;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "data".to_owned());
    let model = args
        .next()
        .unwrap_or_else(|| "models/game-review-reader".to_owned());

    let provenance: Provenance =
        serde_json::from_slice(&std::fs::read(Path::new(&model).join("reader.json"))?)?;
    if provenance.lines_fingerprint.is_empty() {
        return Err(
            "the installed reader carries no rule fingerprint, so nothing can be \
                    matched against it safely"
                .into(),
        );
    }
    println!(
        "reader     {} rule {}",
        provenance.run_id, provenance.lines_fingerprint
    );

    let english = provenance.line_for_language("english");
    let mut filled = 0;
    let mut wrong_rule = Vec::new();
    let mut already = 0;

    for app in std::fs::read_dir(&out)? {
        let app = app?.path();
        for snapshot in std::fs::read_dir(&app).into_iter().flatten().flatten() {
            let path = snapshot.path().join("reading.json");
            let Ok(raw) = std::fs::read(&path) else {
                continue;
            };
            let Ok(mut held) = serde_json::from_slice::<serde_json::Value>(&raw) else {
                continue;
            };
            let rule = held
                .get("read_by_rule")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if rule != provenance.lines_fingerprint {
                wrong_rule.push((path.clone(), rule.to_owned()));
                continue;
            }
            if held.get("strict_languages").is_some_and(|f| !f.is_null()) {
                already += 1;
                continue;
            }

            let languages: Vec<(String, u64)> = held
                .get("languages")
                .and_then(|f| serde_json::from_value(f.clone()).ok())
                .unwrap_or_default();
            let strict: Vec<(String, u64)> = if english.is_finite() {
                languages
                    .iter()
                    .filter(|(name, _)| {
                        let line = provenance.line_for_language(name);
                        line.is_finite() && line > english
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };

            held["strict_languages"] = serde_json::to_value(&strict)?;
            std::fs::write(&path, serde_json::to_vec_pretty(&held)?)?;
            filled += 1;
        }
    }

    println!("filled     {filled} readings");
    if already > 0 {
        println!("already    {already} had it");
    }
    if !wrong_rule.is_empty() {
        println!(
            "refused    {} made under another rule, which need reading again rather than \
             restating:",
            wrong_rule.len()
        );
        for (path, rule) in wrong_rule.iter().take(10) {
            println!(
                "           {} rule {}",
                path.display(),
                if rule.is_empty() { "none" } else { rule }
            );
        }
    }
    Ok(())
}
