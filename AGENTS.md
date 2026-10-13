# SteamGauge

One binary, `steamgauge` (`crates/steamgauge-app`): opened with no arguments it is the Tauri
desktop app, whose window is `crates/steamgauge-app/ui/`; given arguments it is the pipeline,
`crawl` (a Parquet capture), `claims` (spans beside it), `read` (the ONNX reader), `report`.
`crates/steamgauge-core` is everything that knows nothing about a window. `crates/steamgauge-site`
builds the documentation site, a front page made in `home.rs` and the pages under `docs/`, with the
app's own tables written into them; the release deploys it to GitHub Pages. `training/` is Python
that produces the reader; `training/README.md` gives the order a model is trained, swept,
exported and published in. `tools/` drives the report, gold and window pages in Chrome and holds
the release scripts. `DECISIONS.md` is the live record of every choice and figure, written as
findings rather than one section per subject: before touching the classifier, the taxonomy, the
splitter or anything a reported number depends on, list its headings with
`grep -n '^#' DECISIONS.md` and read each one that names it. `CONTRIBUTING.md` says what is held
to a higher bar.

## Commands

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings   # not --all-features: cuda/coreml need vendor toolchains
cargo test --locked --workspace
node --test tools/release/release.test.mjs
typos && git ls-files -z '*.sh' | xargs -0 shellcheck --enable=all   # CONTRIBUTING.md lists the other static gates
cargo build --release -p steamgauge-app --features directml   # always; without it reads run on CPU
bash -c "cd tools && npm ci --ignore-scripts"   # once: axe-core, which the browser checks below run on every page
cargo run -p steamgauge-core --example sample-report && node tools/report-check/check.mjs sample-report.html
cargo run -p steamgauge-core --example sample-gold -- sample-gold.html && node tools/gold-check/check.mjs sample-gold.html
node tools/app-check/check.mjs [--shots <folder outside the tree>]   # the window against a stand-in core; look at the shots
cargo run -p steamgauge-site -- write   # after changing a command, its help, an MCP tool or a client
cargo run -p steamgauge-site -- build --out site --shots <shots> --report <sample report> && node tools/site-check/check.mjs site
r="ruff@$(sed -n 's/.*pipx run ruff==\([0-9.]*\) check.*/\1/p' .github/workflows/ci.yml)"; uvx "$r" check training && uvx "$r" format --check training   # the ruff ci.yml pins; the venv's is another
bash -c "cd training && .venv/Scripts/python -m pytest -q"   # from inside training/, never by path
```

The repository is public, so CI runs the slow checks on every pull request: the browser checks,
the install checks on three systems, coverage and mutation testing. Run the fast gates above
before pushing; run a browser check locally only while changing the window, the report or the
gold page it drives, and read CI for the rest.

## Gotchas

- What sits between `<!-- steamgauge-site: <name> -->` and its end marker in the README and
  `docs/` is written by `cargo run -p steamgauge-site -- write` from the program's own tables;
  never edit it by hand. A build refuses a page whose copy is out of date, so a change to a
  command, its help, an MCP tool or a client carries the rewritten page with it.
- One build tree, `target/`. A running `steamgauge.exe` (a crawl, `gold --serve`) holds the
  release binary open: the build fails with "Access is denied" among other output, and the next
  command silently runs the old binary. Stop it, rebuild, see `Finished`.
- The Windows release binary is a windowed program: it prints only into a pipe or a file. Git
  Bash gives it a pipe; from PowerShell or Python capture or redirect its output, or it is lost.
- Check the device line of `embed` or `read` says `directml` before trusting any timing.
- The GPU and system RAM are shared with other work on this machine. Check the card is idle
  (`nvidia-smi`) before a GPU task. A CUDA OOM with VRAM free means system commit ran out: on
  Windows every byte a run reserves on the card is charged to commit too, about 21.5 GB for a
  560M run, and System event 2004 names who held the rest.
- Beside training, only the core crate builds, and only through
  `tools/cargo-beside-training.sh` (`check`, `clippy`, `test` or `run --example`, with
  `-p steamgauge-core`), which holds the job count and the free commit a build needs. Never a
  workspace, app or release build beside training.
- A training queue imports `training/train.py` and `claimdata.py` from disk at each launch. While
  one runs, never stash, checkout, rebase or edit them in this tree; use a worktree. Before
  launching one, run the Python tests: `test_evaluate.py` holds `evaluate` to `@torch.no_grad()`,
  without which the first validation pass allocates over 30 GB and the run dies.
- A run's tokenizer (17 MB, reproducible from the backbone named in `run.json`) never enters a
  commit; `.gitignore` covers both copies. After any commit under `training/runs/`, read
  `git show --stat HEAD` to confirm none did.
- A claim is the byte span it covers, everywhere; nothing carries a version stamp. A splitter
  change costs a full library re-read of about five hours.
- Quote the held-out games (`sweep.py --held-out`), never validation. A change is an improvement
  only when it clears the seed spread `training/sweep.py` prints beside it. `sweep.py --index`
  writes `training/runs/README.md`; never edit it by hand.
- `tools/app-check/stub.js` stands in for the core when the window is checked, answering from
  fixtures shaped as the Rust side sends them. A command or field added on the Rust side is
  added to the stub in the same change, or the check tests a window the app no longer has.
- Nothing is named by an incremented number, and names say plainly what the thing is, never a
  metaphor.
- Public texts (the README, the docs, model and dataset cards, release notes) state what the
  product is and does now, in plain present tense: no history of how it got here and no
  comparison with what it replaced, which belong in `DECISIONS.md`. No figure in them rests on
  a library or capture nobody else can check. The one list of changes is the release notes'
  changelog, built from pull request titles, so a title states what is true after the change
  rather than comparing it with what came before.
- None of the maintainer's other projects is named anywhere in this repository: files,
  comments, commit messages, pull requests. A pattern borrowed from one is described as this
  repository's own choice.
