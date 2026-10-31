# Contributing

Before starting anything substantial, open an issue so the design is settled first. Read the
section of [Architecture](docs/architecture.md) that names what you are changing.

## Working on it

`main` is protected. Everything lands through a pull request that is current with `main`, and
history is linear, so merges are squashed or rebased.

Run the gates CI runs before pushing:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --workspace
pipx run ruff==0.15.2 check training && pipx run ruff==0.15.2 format --check training
bash -c "cd training && python -m pytest -q"
node --test tools/release/release.test.mjs
```

- Clippy runs without `--all-features`, because the cuda and coreml backends need vendor
  toolchains.
- Run pytest from inside `training/`. Given a path from elsewhere on Windows, its collection
  breaks.

CI also checks the dependencies, the spelling and the Markdown, with the versions `ci.yml` pins:

```sh
cargo deny --workspace --locked check                              # advisories, licences, sources
cp Cargo.lock fuzz/ && cargo deny --manifest-path fuzz/Cargo.toml --workspace check
cargo machete                                                      # dependencies nothing uses
typos                                                              # British spelling, _typos.toml
npx markdownlint-cli2                                              # .markdownlint-cli2.jsonc
git ls-files -z '*.sh' | xargs -0 shellcheck --enable=all
```

- **No em dash** in any file, except the ONNX Runtime notices, which are Microsoft's text.
- **Ignored advisories.** An advisory cargo-deny ignores is ignored in `osv-scanner.toml` too,
  with the evidence and an expiry date.
- **CI fails on a crate whose licence** `third-party/about.toml` does not accept. To accept
  another, change `about.toml`, `deny.toml` and `fuzz/deny.toml` in the pull request that needs it.

## The pages in a browser

The window, the report and the site are checked in headless Chrome against axe-core's WCAG 2.2
A and AA rules. They need Chrome (or `CHROME_PATH`) and Node:

```sh
(cd tools && npm ci --ignore-scripts)
cargo run -p steamgauge-core --example sample-report -- report.html
node tools/report-check/check.mjs report.html
node tools/app-check/check.mjs --shots shots
cargo run -p steamgauge-site -- write
cargo run -p steamgauge-site -- build --out site --shots shots --report report.html
node tools/site-check/check.mjs site
```

- **The report check** holds the page to what its own text promises. Change the check when you
  change what the page says it does.
- **The window check** runs against `tools/app-check/stub.js`, which stands in for the core. A
  command or field added on the Rust side is added to the stub in the same change.
- **The site** writes the program's commands, MCP tools and clients into `docs/` with `write`. A
  test fails while a copy is behind, and the build refuses a link to a missing file or heading.

The words a person reads are held to plain prose by [Vale](https://vale.sh), with the style in
`.vale/`:

```sh
node tools/app-check/check.mjs --pages pages
vale README.md docs SECURITY.md training/README.md reference/README.md report.html pages
```

## The release binary

`target/` is the only build directory. Where there is a GPU, build with its backend:

```sh
cargo build --release -p steamgauge-app --features directml
```

A build without the flag replaces the binary with a CPU one. The device `steamgauge embed` prints
on its third line says which you have. On Windows a running `steamgauge.exe` holds its binary
open, so a build fails with "Access is denied" until the run stops.

## Fuzzing

The splitter (`splitter`), a stored span brought to its words (`stored_span`), the reader's window
(`reader_window`) and the terms a claim is counted under (`terms`) are fuzzed. `fuzz/src/lib.rs`
says what each target asserts. CI runs them for ten minutes on every pull request and an hour
every Monday. A crash fails the check and attaches its input as an artefact.

`fuzz/` is a workspace of its own. It needs a nightly toolchain and cargo-fuzz:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run splitter fuzz/corpus/splitter fuzz/seeds -- -dict=fuzz/review.dict -max_len=16384
cargo +nightly fuzz run splitter path/to/crash-input        # replays one input, as from a CI artefact
cargo +nightly fuzz tmin splitter path/to/crash-input       # cuts it to the least that still fails
cargo +nightly fuzz list                                    # the other targets
```

- `fuzz/seeds/` is synthetic. Never add a review from `data/`, which is other people's writing.
- Fix a crash where it happens, with a test beside the code that reproduces it.

## Coverage and mutants

- **`coverage.yml`** fails when the core's tests run fewer of its lines than the floor it names,
  and the floor only rises.
- **`mutants.yml`** fails on a mutant the tests pass in the core's changed lines. Write the test
  that kills it, or add it to `.cargo/mutants.toml` with the reason.

```sh
cargo install cargo-llvm-cov cargo-mutants
cargo llvm-cov -p steamgauge-core --summary-only
git diff origin/main... > change.diff
cargo mutants -p steamgauge-core --in-diff change.diff --jobserver-tasks 4
```

## What is held to a higher bar

Anything affecting a reported number needs more than passing tests:

- **Ingestion parameters** override Valve's defaults for the census. A change here changes every
  figure and can make existing corpora incomparable.
- **Denominators**: a percentage is a mention rate unless labelled otherwise, and a new figure
  states its denominator.
- **The taxonomy, classifier and gold set** change the numbers when they change. Say by how much.
- **Figures in the README and the docs** are reproducible, and the pull request says how.

## Licence

Contributions are accepted under the Apache License 2.0, as covered by section 5 of the licence.
