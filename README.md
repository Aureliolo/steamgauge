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

A tool for finding out what players of a game actually think, rather than what the loudest
reviews say.

## Install

The installer and the portable archive for each system are in the
[latest release](https://github.com/Aureliolo/steamgauge/releases/latest). A package manager
installs it too:

```powershell
# Windows, with winget: the installer
winget install Aureliolo.SteamGauge
# Windows, with Scoop: the portable program, with a Start menu shortcut
scoop install https://github.com/Aureliolo/steamgauge/releases/latest/download/steamgauge.json
```

```bash
# macOS on Apple Silicon, with Homebrew: this repository is the tap
brew tap aureliolo/steamgauge https://github.com/Aureliolo/steamgauge
brew install --cask aureliolo/steamgauge/steamgauge
```

[Install](docs/install.md) has the rest: the installer and archive for each system, what each
system needs, the first launch, verifying a download and upgrading.

## Documentation

The documentation is published at <https://aureliolo.github.io/steamgauge/> and lives in
[`docs/`](docs/).

- [Install](docs/install.md): the files for each system, the package managers, the first launch,
  verifying a download and upgrading.
- [How it works](docs/how-it-works.md): what the tool counts and why, how a review becomes the
  claims it makes, how each claim is read, and what every percentage means.
- [Steering it from an AI assistant](docs/assistants.md): the app's MCP server, the assistants
  it adds itself to, and the command for any other.
- [How it knows whether it is right](docs/accuracy.md): how the reader is measured, how it
  compares with the alternatives, each size it comes in, the person-checked gold set, and how
  the reference sets are made.
- [Honest limits](docs/limits.md): where the figures fall short, by language, by labeller and
  by what Steam serves.
- [Data and privacy](docs/privacy.md): what stays on your machine and the four places the app
  makes requests to.

## Models and dataset

The readers, the search models and the labelled claims are on Hugging Face, each release
tagged `v1`, `v2` and so on. The app fetches every file from one pinned commit and checks its
length and SHA-256 before using it.

| | Repository | DOI |
|---|---|---|
| Reader | [Aureliolo/game-review-reader](https://huggingface.co/Aureliolo/game-review-reader) | [10.57967/hf/10732](https://doi.org/10.57967/hf/10732) |
| Reader, small | [Aureliolo/game-review-reader-small](https://huggingface.co/Aureliolo/game-review-reader-small) | [10.57967/hf/10734](https://doi.org/10.57967/hf/10734) |
| Labelled claims | [Aureliolo/game-review-claims](https://huggingface.co/datasets/Aureliolo/game-review-claims) | [10.57967/hf/10735](https://doi.org/10.57967/hf/10735) |
| Search encoder | [Aureliolo/steamgauge-search-encoder](https://huggingface.co/Aureliolo/steamgauge-search-encoder) | |
| Search reranker | [Aureliolo/steamgauge-search-reranker](https://huggingface.co/Aureliolo/steamgauge-search-reranker) | |

The search models are fp16 ONNX exports of Qwen3-Embedding-0.6B and Qwen3-Reranker-0.6B with
their weights unchanged. The dataset holds no review text and nothing about the authors: each
labelled review carries a SHA-256 fingerprint of its text, and `training/fetch_text.py`
fetches the reviews from Steam and puts the text back on your own machine.

## Licence

Apache License 2.0. See [LICENSE](LICENSE).
