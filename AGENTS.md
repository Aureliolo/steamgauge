# SteamGauge

`docs/architecture.md` lists the parts and the rules they keep. Read the section that names the
thing before changing the classifier, the taxonomy, the splitter or anything a reported number
depends on. The window is `crates/steamgauge-app/ui/`. The site's front page is `home.rs` in
`crates/steamgauge-site`, and the release deploys the site to GitHub Pages.

There is no decision log. A settled design choice is written into `docs/architecture.md` (or
`training/README.md`) as a present-tense fact. How it got there stays in git and the pull
request.

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

CI runs the slow checks on every pull request: the browser checks, the install checks, coverage
and mutation testing. Run the fast gates above before pushing. Run a browser check locally only
while changing the page it drives.

## Gotchas

- What sits between `<!-- steamgauge-site: <name> -->` and its end marker is written by
  `cargo run -p steamgauge-site -- write`. Never edit it by hand, and commit the rewritten page
  with the change to a command, its help, an MCP tool or a client.
- A running `steamgauge.exe` (a crawl, `gold --serve`) holds the release binary open. The build
  fails with "Access is denied" and the next command runs the old binary, so stop it, rebuild and
  see `Finished`.
- The Windows release binary prints only into a pipe or a file. From PowerShell or Python,
  capture or redirect its output.
- Check the device line of `embed` or `read` says `directml` before trusting any timing.
- The GPU and system RAM are shared with other work on this machine. Check the card is idle
  (`nvidia-smi`) before a GPU task.
- A CUDA OOM with VRAM free means system commit ran out, since card memory counts against commit
  (about 21.5 GB for a 560M run). System event 2004 names who held the rest.
- Beside training, only the core crate builds, through `tools/cargo-beside-training.sh` (`check`,
  `clippy`, `test` or `run --example`, with `-p steamgauge-core`). Never a workspace, app or
  release build beside training.
- A training queue imports `training/train.py` and `claimdata.py` from disk at each launch. While
  one runs, use a worktree rather than stash, checkout, rebase or edit them in this tree.
- Before launching a queue, run the Python tests. `test_evaluate.py` holds `evaluate` to
  `@torch.no_grad()`, without which the first validation pass takes over 30 GB and dies.
- A run's tokenizer (17 MB) never enters a commit. After any commit under `training/runs/`, read
  `git show --stat HEAD` to confirm none did.
- A splitter change costs a full library re-read of about five hours.
- Quote the held-out games (`sweep.py --held-out`), never validation. `sweep.py --index` writes
  `training/runs/README.md`, which is never edited by hand.
- A command or field added on the Rust side goes into `tools/app-check/stub.js` in the same
  change. Otherwise the window check tests a window the app no longer has.
- Nothing is named by an incremented number, and names say plainly what the thing is, never a
  metaphor.
- Public texts (the README, the docs, model and dataset cards, release notes) state what the
  product is and does now. No history, no comparison with what it replaced, and no figure
  resting on a library or capture nobody else can check.
- The release notes' changelog is built from pull request titles, so a title states what is true
  after the change.
- None of the maintainer's other projects is named anywhere in this repository: files,
  comments, commit messages, pull requests. A pattern borrowed from one is described as this
  repository's own choice.
