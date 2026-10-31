# SteamGauge

![SteamGauge's mark, a pressure dial](crates/steamgauge-app/icons/128x128.png)

[![CI](https://github.com/Aureliolo/steamgauge/actions/workflows/ci.yml/badge.svg)](https://github.com/Aureliolo/steamgauge/actions/workflows/ci.yml)
[![CodeQL](https://github.com/Aureliolo/steamgauge/actions/workflows/github-code-scanning/codeql/badge.svg)](https://github.com/Aureliolo/steamgauge/actions/workflows/github-code-scanning/codeql)
[![Scorecard](https://api.scorecard.dev/projects/github.com/Aureliolo/steamgauge/badge)](https://scorecard.dev/viewer/?uri=github.com/Aureliolo/steamgauge)
[![Rust](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2FAureliolo%2Fsteamgauge%2Fmain%2Frust-toolchain.toml&query=%24.toolchain.channel&label=rust&logo=rust&color=b7410e)](rust-toolchain.toml)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24c8db?style=flat&logo=tauri&logoColor=white)](https://tauri.app)
[![Training on Python](https://img.shields.io/badge/dynamic/yaml?url=https%3A%2F%2Fraw.githubusercontent.com%2FAureliolo%2Fsteamgauge%2Fmain%2F.github%2Fworkflows%2Fci.yml&query=%24.jobs.python.steps%5B1%5D.with%5B%27python-version%27%5D&label=training&logo=python&logoColor=white&color=3776ab)](training/)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-2f6f4e?style=flat)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/Aureliolo/steamgauge?style=flat)](https://github.com/Aureliolo/steamgauge/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Aureliolo/steamgauge/total?style=flat)](https://github.com/Aureliolo/steamgauge/releases)
[![SLSA Build L3](https://img.shields.io/badge/SLSA-Build%20L3-2f6f4e?style=flat)](.github/release-process.md#slsa)
[![SBOM SPDX](https://img.shields.io/badge/SBOM-SPDX-2f6f4e?style=flat)](.github/release-process.md#what-a-release-carries)
[![Sigstore attested](https://img.shields.io/badge/Sigstore-attested-2f6f4e?style=flat)](SECURITY.md#releases)

A desktop app that downloads every Steam review of a game and reads each one on your computer. It
shows what players praise and criticise, how often, and the reviews behind every figure.

## Install

Take the installer or portable archive for your system from the
[latest release](https://github.com/Aureliolo/steamgauge/releases/latest), or install it with
Scoop or Homebrew. [Install](docs/install.md) has the commands.

## Documentation

The documentation is at <https://aureliolo.github.io/steamgauge/>.

<!-- steamgauge-site: pages -->

- [Install](docs/install.md): the files for each system, the first launch, checking a download and
  updating.
- [How it works](docs/how-it-works.md): what it downloads, how it reads a review, and what each
  figure counts.
- [Accuracy](docs/accuracy.md): how often the reader is right, against what, in which languages, and
  what the figures cannot tell you.
- [AI assistants](docs/assistants.md): the app's MCP server and the assistants it adds itself to.
- [MCP tools](docs/tools.md): every prompt and tool the MCP server offers and what each is given.
- [Command line](docs/commands.md): every command of the `steamgauge` program.
- [Architecture](docs/architecture.md): how the parts fit and the rules they keep, for anyone
  changing SteamGauge.
- [Privacy](docs/privacy.md): what stays on your computer and the four places the app connects to.

<!-- steamgauge-site: end -->

## Models and dataset

The readers, the search models, the writer and the labelled claims are on Hugging Face, each
release tagged `v1`, `v2` and so on.

| | Repository | DOI |
|---|---|---|
| Reader | [Aureliolo/game-review-reader](https://huggingface.co/Aureliolo/game-review-reader) | [10.57967/hf/10732](https://doi.org/10.57967/hf/10732) |
| Reader, small | [Aureliolo/game-review-reader-small](https://huggingface.co/Aureliolo/game-review-reader-small) | [10.57967/hf/10734](https://doi.org/10.57967/hf/10734) |
| Labelled claims | [Aureliolo/game-review-claims](https://huggingface.co/datasets/Aureliolo/game-review-claims) | [10.57967/hf/10735](https://doi.org/10.57967/hf/10735) |
| Search encoder | [Aureliolo/steamgauge-search-encoder](https://huggingface.co/Aureliolo/steamgauge-search-encoder) | |
| Search reranker | [Aureliolo/steamgauge-search-reranker](https://huggingface.co/Aureliolo/steamgauge-search-reranker) | |
| Writer | [Aureliolo/steamgauge-writer](https://huggingface.co/Aureliolo/steamgauge-writer) | |

The search models are fp16 ONNX exports of Qwen3-Embedding-0.6B and Qwen3-Reranker-0.6B with
their weights unchanged. The writer is Qwen3-4B-Instruct-2507 with its weights rounded to four
bits. The dataset holds no review text and nothing about the authors. Each labelled review
carries a SHA-256 fingerprint of its text, and `training/fetch_text.py` fetches the text from
Steam onto your machine.

## Licence

Apache License 2.0. See [LICENSE](LICENSE).
