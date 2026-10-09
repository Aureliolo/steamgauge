# SteamGauge

One binary, `steamgauge` (`crates/steamgauge-app`): opened with no arguments it is the Tauri
desktop app, given arguments it is the pipeline. `crates/steamgauge-core` is everything that
knows nothing about a window. `training/` is Python that produces the ONNX reader the tool runs;
`tools/` drives the report and gold pages in Chrome. `DECISIONS.md` is the live record of every
choice and figure: read its section before touching the classifier, the taxonomy, the splitter
or anything a reported number depends on. `CONTRIBUTING.md` says what is held to a higher bar.

## Commands

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings          # not --all-features: cuda/coreml need vendor toolchains
cargo test --workspace
cargo build --release -p steamgauge-app --features directml   # always; without it reads run on CPU
bash -c "cd tools && npm ci --ignore-scripts"   # once: axe-core, which both browser checks below run on every page
cargo run -p steamgauge-core --example sample-report -- report.html && node tools/report-check/check.mjs report.html
node tools/app-check/check.mjs [--shots <folder>]   # the window against a stand-in core; look at the shots
r="ruff@$(sed -n 's/.*pipx run ruff==\([0-9.]*\) check.*/\1/p' .github/workflows/ci.yml)"; uvx "$r" check training && uvx "$r" format --check training   # the ruff ci.yml pins; the venv's is another
bash -c "cd training && .venv/Scripts/python -m pytest -q"   # from inside training/, never by path
```

The repository is public, so CI runs the slow checks on every pull request: the browser checks,
the install checks on three systems, coverage and mutation testing. Run the fast gates above
before pushing; run a browser check locally only while changing the window or the report it
drives, and read CI for the rest.

## Gotchas

- One build tree, `target/`. A running `steamgauge.exe` (a crawl, `gold --serve`) holds the
  release binary open: the build fails with "Access is denied" among other output, and the next
  command silently runs the old binary. Stop it, rebuild, see `Finished`.
- The Windows release binary is a windowed program: it prints only into a pipe or a file. Git
  Bash gives it a pipe; from PowerShell or Python capture or redirect its output, or it is lost.
- Check the device line of `embed` or `read` says `directml` before trusting any timing.
- The GPU and system RAM are shared with other work on this machine. Check the card is idle
  before a GPU task. A CUDA OOM with VRAM free means system commit ran out: on Windows every
  byte a run reserves on the card is charged to commit too, about 21.5 GB for a 560M run, and
  System event 2004 names who held the rest.
- Beside training, only the core crate builds, and only through
  `tools/cargo-beside-training.sh` (`check`, `clippy`, `test` or `run --example`, with
  `-p steamgauge-core`): two jobs, 25 GB of commit free to start, stopped under 12. Never a
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
- Quote the frozen games, never validation. A change is an improvement only when it clears the
  seed spread `training/sweep.py` prints beside it.
- Nothing is named by an incremented number, and names say what the thing is.
