## What this changes

<!-- What behaviour differs after this merges. -->

## Why

<!-- The problem being solved, not the diff. -->

## Effect on reported numbers

<!-- Delete if none. Anything touching ingestion parameters, the taxonomy, the classifier,
     the denominators or the gold set changes what the tool reports. Say which figures move
     and whether existing corpora stay comparable. -->

## Checks

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --locked --all-targets -- -D warnings`
- [ ] `cargo test --locked --workspace`
- [ ] Claims added to the README are reproducible, with the command or query that produces them
