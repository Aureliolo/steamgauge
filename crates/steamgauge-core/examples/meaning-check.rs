//! What searching a game by meaning would find, and what it costs to make that possible.
//!
//! Embeds the first `limit` claims of a read game with one encoder, the claims with one prefix
//! and the queries with another, then prints, for each query, the nearest claims with their
//! similarity and the spread of similarity over every claim, so a line between "says this" and
//! "does not" can be read off real text rather than guessed.
//!
//!     cargo run --release -p steamgauge-core --example meaning-check -- \
//!         data 1272080 20000 e5-small "passage: " "query: " "steam deck" "controller support"

use std::collections::HashMap;

use steamgauge_core::{
    embed::Embedder,
    model::{Encoder, Precision, default_cache_dir},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [
        root,
        app_id,
        limit,
        encoder,
        passage,
        query_prefix,
        queries @ ..,
    ] = args.as_slice()
    else {
        return Err(
            "usage: meaning-check <library> <app id> <claims> <encoder> \
                    <claim prefix> <query prefix> <query>..."
                .into(),
        );
    };
    let encoder = [
        Encoder::E5Small,
        Encoder::E5Base,
        Encoder::ArcticMediumV2,
        Encoder::GteBase,
    ]
    .into_iter()
    .find(|known| known.as_str() == encoder)
    .ok_or("no such encoder")?;
    let limit: usize = limit.parse()?;
    let snapshot =
        steamgauge_core::embed::latest_snapshot(std::path::Path::new(root), app_id.parse()?)?;

    let mut spans: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
    steamgauge_core::read::for_each_full_reading(
        &snapshot.join("readings.parquet"),
        |id, at, _, _, _, _| spans.entry(id.to_owned()).or_default().push(at),
    )?;
    let mut claims: Vec<String> = Vec::new();
    steamgauge_core::capture::for_each_row(&snapshot, |row, text| {
        if claims.len() >= limit {
            return Ok(());
        }
        for at in spans.get(&row.recommendationid).into_iter().flatten() {
            if let Some(claim) = text.get(at.0 as usize..at.1 as usize) {
                claims.push(claim.to_owned());
            }
        }
        Ok(())
    })?;
    claims.truncate(limit);

    let mut embedder = Embedder::load(&default_cache_dir(), encoder, Precision::Float16)?;
    println!("{} on {}", encoder.as_str(), embedder.device());
    let started = std::time::Instant::now();
    let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(claims.len());
    for chunk in claims.chunks(64) {
        vectors.extend(embedder.embed_as(chunk, passage)?);
    }
    let took = started.elapsed().as_secs_f64();
    println!(
        "{} claims in {took:.1} s, {:.2} ms a claim",
        claims.len(),
        took * 1000.0 / f64::from(u32::try_from(claims.len())?)
    );

    for query in queries {
        let wanted = &embedder.embed_as(std::slice::from_ref(query), query_prefix)?[0];
        let mut scored: Vec<(f32, usize)> = vectors
            .iter()
            .enumerate()
            .map(|(at, vector)| (vector.iter().zip(wanted).map(|(a, b)| a * b).sum(), at))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let at = |per_thousand: usize| scored[(scored.len() - 1) * per_thousand / 1000].0;
        println!(
            "\n\"{query}\": best {:.3}, top 0.1% {:.3}, top 1% {:.3}, median {:.3}",
            scored[0].0,
            at(1),
            at(10),
            at(500)
        );
        for (rank, (score, index)) in scored.iter().enumerate() {
            if rank < 15 || [30, 60, 100, 200, 400].contains(&rank) {
                let claim: String = claims[*index].chars().take(110).collect();
                println!("  {rank:>4} {score:.3}  {}", claim.replace('\n', " "));
            }
        }
    }
    Ok(())
}
