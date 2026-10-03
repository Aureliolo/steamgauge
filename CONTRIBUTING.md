# Contributing

Before starting anything substantial, open an issue so the design is settled first.

## Working on it

`main` is protected. Everything lands through a pull request, including maintainer changes
and dependency bumps. Branches must be current with `main` before merging, and history is
linear, so merges are squashed or rebased rather than committed as merges.

Run the same gates CI runs before pushing:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --workspace
pipx run ruff==0.15.2 check training && pipx run ruff==0.15.2 format --check training
bash -c "cd training && python -m pytest -q"
node --test tools/release/release.test.mjs
```

`--locked` fails when `Cargo.lock` no longer matches the manifests. The release build refuses
that too, but only once the tag is made, and a tag cannot be taken back. The last line tests
the scripts that raise the version and write each release's changelog;
`.github/release-process.md` says how a release is cut.

CI also holds the dependencies, the spelling and the Markdown to a standard, each with a tool
whose version `ci.yml` pins:

```sh
cargo deny --workspace --locked check                              # advisories, licences, sources
cargo deny --manifest-path fuzz/Cargo.toml --workspace --locked check
cargo machete                                                      # dependencies nothing uses
typos                                                              # British spelling, _typos.toml
npx markdownlint-cli2                                              # .markdownlint-cli2.jsonc
git ls-files -z '*.sh' | xargs -0 shellcheck --enable=all
```

No file may carry an em dash, except the ONNX Runtime notices, which are Microsoft's text. An
advisory cargo-deny is told to ignore is ignored in `osv-scanner.toml` too, with the evidence
and an expiry: the deny job fails on one that is not, or whose date has passed.

A new dependency brings its licence into every release archive. CI's notices job writes each
archive's `THIRD-PARTY-NOTICES.txt` on every pull request and fails on a crate whose licence
`third-party/about.toml` does not accept, or whose licence text cargo-about cannot find; the
message says which, and `third-party/README.md` says what to do. Accepting another licence is a
decision about what the binary may contain, so make it in the pull request that needs it, in
`deny.toml` and `fuzz/deny.toml` as well, which hold every other crate to the same list.

Not `--all-features`: the cuda and metal backends need vendor toolchains, and the default set
is what ships. The Rust tests run on Linux, macOS and Windows; the Python ones on Windows,
which is the platform `training/requirements.lock` is resolved for. Clippy warnings fail the
build. Run pytest from inside `training/`: given a path from elsewhere on Windows it walks every
sibling of every ancestor, and a shared temp directory changing underneath aborts collection.

The report page carries scripting no Rust test can reach, and rendering no Rust test can see:
a stylesheet rule can flatten a chart or turn a printed page into blocks of ink while the
markup stays exactly right. A fourth gate therefore drives the page in a real browser four
times: on a desktop window, at 420 pixels where the tables are wider than the screen, under
print media with the machine asking for a dark one, and again with the script cut out, which
is what a reader with scripting off is served. It needs Chrome and Node, and nothing else: no
corpus, no model, no network.

```sh
cargo run -p steamgauge-core --example sample-report -- report.html
node tools/report-check/check.mjs report.html
```

Chrome is found in the usual places per platform, or wherever `CHROME_PATH` says. Every
check in it is a promise the page makes in its own prose; if you change what the page says
it does, change the check with it.

The desktop app's window is held to the same bar. `tools/app-check` serves it in headless
Chrome with `stub.js` standing in for the core, answering every command from fixtures shaped as
the Rust side sends them, and presses every page's controls. `--shots <folder>` saves each page,
light and dark, for a person to look at. A command or a field added on the Rust side is added
to the stub in the same change.

```sh
node tools/app-check/check.mjs
```

### One build directory

`target/`, and nothing beside it. The gates above build test binaries under `target/debug` and
never produce a release one, so the only binary anyone runs for real work is
`target/release/steamgauge.exe`. That one binary is both front ends: opened with no arguments it
is the desktop application, and given arguments it is the pipeline. Where there is a GPU,
build it with the backend for it, because embedding a million reviews on a CPU is the
difference between an afternoon and a week:

```sh
cargo build --release -p steamgauge-app --features directml
```

Two things follow. A build without that flag replaces the same file with a CPU one, and the
only sign is the device `steamgauge embed` prints on its third line: read it. And on Windows a
running `steamgauge.exe` holds its own binary open, so a build started mid-crawl fails with
"Access is denied": wait for the run rather than reaching for `--target-dir`, which is how
this repository once ended up with four build trees and twenty gigabytes in them.

Two of those promises are watched rather than read. The page is reloaded with the network
being listened to and fails on any request but the file itself, because a stylesheet pulling
a font would pass any amount of reading the markup for URLs. And every piece of text on it is
measured against whatever is composited behind it, in both themes, because a palette is a set
of tokens until a browser draws it.

### Fuzzing

Everything that reads text a stranger wrote is fuzzed, because a panic in any of it stops the
reading of a whole library: the splitter over a review (`splitter`), a stored span brought to
its words (`stored_span`), the window the reader reads a claim in (`reader_window`) and the
terms a claim is counted under (`terms`). Not crashing is the least of it. `fuzz/src/lib.rs`
says what each target asserts, and most of it is what the join between labels, readings and
claims relies on: a span brought to its words again stays where it is, claims come in order and
never share a span, and a claim holds nothing its span does not.

CI runs them with ClusterFuzzLite (`.clusterfuzzlite/`, `.github/workflows/fuzz.yml`): ten
minutes on every pull request, an hour every Monday on `main`. A crash fails the check,
attaches the input that caused it to the run as an artefact, and shows in code scanning.

`fuzz/` is a workspace of its own, so none of the gates above builds it, and none of it ships.
Running a target needs a nightly toolchain and cargo-fuzz, and it builds the core crate with a
sanitiser, which is not a build to start beside a training run:

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run splitter fuzz/corpus/splitter fuzz/seeds -- -dict=fuzz/review.dict -max_len=16384
cargo +nightly fuzz run splitter path/to/crash-input        # replays one input, as from a CI artefact
cargo +nightly fuzz tmin splitter path/to/crash-input       # cuts it to the least that still fails
cargo +nightly fuzz list                                    # the other targets
```

New inputs go to `fuzz/corpus/<target>/` and crashes to `fuzz/artifacts/<target>/`, both kept
out of git. `fuzz/seeds/` is the committed start, and it is synthetic: never a review from
`data/`, which is other people's writing. A crash is fixed where it happens, with a test beside
the code that reproduces it; the fuzz target is not where it is made to pass.

## What is held to a higher bar

This tool exists to make a percentage mean what it appears to mean, so anything affecting a
reported number needs more than passing tests:

- **Ingestion parameters.** The census depends on overriding Valve's defaults. A change here
  silently changes every downstream figure and can make existing corpora incomparable.
- **Denominators.** A percentage is a mention rate unless labelled otherwise. Do not add a
  figure to the interface or the README without stating which denominator it uses.
- **Taxonomy, classifier and gold set.** Changing any of these changes the numbers. Say by
  how much.
- **Claims in the README.** Every figure quoted must be reproducible, and the pull request
  should say how to reproduce it.

## Licence

Contributions are accepted under the Apache License 2.0, as covered by section 5 of the
licence.
