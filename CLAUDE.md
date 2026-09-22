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
cargo clippy --all-targets -- -D warnings          # not --all-features: cuda/metal need vendor toolchains
cargo test --workspace
cargo build --release -p steamgauge-app --features directml   # always; without it reads run on CPU
cargo run -p steamgauge-core --example sample-report -- report.html && node tools/report-check/check.mjs report.html
pipx run ruff==0.15.2 check training && pipx run ruff==0.15.2 format --check training
bash -c "cd training && .venv/Scripts/python -m pytest -q"   # from inside training/, never by path
```

## Gotchas

- One build tree, `target/`. A running `steamgauge.exe` (a crawl, `gold --serve`) holds the
  release binary open: the build fails with "Access is denied" among other output, and the next
  command silently runs the old binary. Stop it, rebuild, see `Finished`.
- Check the device line of `embed` or `read` says `directml` before trusting any timing.
- The GPU and system RAM are shared with other work on this machine. Check the card is idle
  before a GPU task, and never run a workspace build beside training: a CUDA OOM with VRAM free
  means system commit ran out.
- A training queue imports `training/train.py` and `claimdata.py` from disk at each launch. While
  one runs, never stash, checkout, rebase or edit them in this tree; use a worktree.
- After any commit under `training/runs/`, read `git show --stat HEAD`: a 17 MB `tokenizer.json`
  has reached a commit twice by two different paths.
- A claim is the byte span it covers, everywhere; nothing carries a version stamp. A splitter
  change costs a full library re-read of about five hours.
- Quote the frozen games, never validation. A change is an improvement only when it clears the
  seed spread `training/sweep.py` prints beside it.
- Nothing is named by an incremented number, and names say what the thing is.
