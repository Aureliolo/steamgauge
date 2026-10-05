# Security

## Supported versions

Only the latest release gets fixes. A fix ships as a new release, and the app names a newer
release in its window when one is out (see [Network requests](#network-requests)).

## Reporting a vulnerability

Report privately through GitHub's [security advisory
form](https://github.com/Aureliolo/steamgauge/security/advisories/new). Please do
not open a public issue for a vulnerability. You get an answer within a week.

Reports about the build and release pipeline are in scope and welcome.

The code that reads review text is fuzzed on every pull request and every week, and a crash is
reported to code scanning; `CONTRIBUTING.md` says how to run the fuzzers yourself.

## Releases

Every release is built by GitHub Actions on GitHub-hosted runners, from a signed commit on
`main`, with `--locked` dependencies and no build cache, by `release-build.yml`. Nothing is built
or signed on a developer machine. Each release carries an installer and a portable archive for
each system (the README's Install section lists them), an SPDX SBOM of each, the Scoop manifest
`steamgauge.json`, and `SHA256SUMS` over all of them, and two kinds of Sigstore attestation:
build provenance over every file, and each SBOM bound to the file it describes. The build and
the signing both run in that one reusable workflow, which is SLSA Build Level 3, and the
attestations are attached to the release as `steamgauge-<version>.intoto.jsonl` as well as
stored on the repository, so they verify from the file alone. Releases are immutable and the
`v*` tags cannot be moved or deleted. Each archive also carries `THIRD-PARTY-NOTICES.txt`, the
licence of every crate compiled into the binary and of ONNX Runtime and, on Windows, DirectML,
checked at release against the crates that build resolved and the files the archive holds. The
Homebrew cask, the Scoop manifest and the winget manifests are written from the release's own
`SHA256SUMS` once its attestation verifies, so Homebrew, Scoop and winget install a file only if
it matches the hash the release is signed over.

Verify a download before running it. `file` is the name of any file in the release:

```sh
signer=Aureliolo/steamgauge/.github/workflows/release-build.yml
file=steamgauge-<version>-windows-x64-setup.exe
gh attestation verify "$file" --repo Aureliolo/steamgauge --signer-workflow "$signer" --source-ref refs/tags/v<version> --deny-self-hosted-runners
gh attestation verify "$file" --repo Aureliolo/steamgauge --signer-workflow "$signer" --predicate-type https://spdx.dev/Document/v2.3
gh attestation verify "$file" --repo Aureliolo/steamgauge --signer-workflow "$signer" --bundle steamgauge-<version>.intoto.jsonl
sha256sum --check --ignore-missing SHA256SUMS
```

[`.github/release-process.md`](.github/release-process.md) describes how a release is cut,
what each job checks, and what the SBOM does and does not name.

## What signing does and does not tell you

Three different things are commonly called signing, and they answer different questions:

- **Provenance** (Sigstore attestations). Proves an asset was built by this repository's
  workflow from a specific commit. Verified deliberately, with the commands above.
- **Operating-system trust** (a certificate chaining to a CA the OS ships). This is the only
  thing that suppresses SmartScreen and Gatekeeper warnings. **No build carries it.**
- **Self-signing.** Proves the bytes came from the holder of a key the operating system has
  never heard of. It changes no warnings.

So every build warns on first launch. Windows shows a SmartScreen prompt you can click past;
macOS refuses until you allow the app under System Settings, Privacy and Security. The README's
Install section shows both. Neither warning is a sign that the download has been tampered with.
Verify provenance with the commands above: the operating system's warning reflects only that no
certificate authority has been paid.

Windows builds are to carry a SignPath Foundation signature, which chains to a CA Windows
trusts, once the project is enrolled; until then they carry provenance only. macOS has no free
certificate authority, so its builds carry provenance only.

## Network requests

The app makes requests to three places, and to nothing else:

- **Steam**, for reviews, game names and review totals, the store's search when a game is
  found by its name, each game's store picture (fetched once from Steam's image servers and kept
  on disk; the window itself never reaches the network), and, when the app opens, how many
  reviews each game in the library has now (at most every six hours; a setting turns it off).
- **Hugging Face**, for the readers and the search models, each file fetched from a pinned
  commit and checked against its pinned length and SHA-256 before it is used, and, when the
  library is checked against Steam, for whether newer releases of those models are published.
- **GitHub**, at most once a day, for whether a newer release of SteamGauge is out: one request
  to the release page, whose redirect names the newest version. The redirect is not followed,
  no API or token is involved, and a setting turns it off. Nothing is downloaded or installed
  by the app; the link the window offers opens only a release page of this repository.

## Handling of data

The app keeps the reviews it downloads, what it reads in them and the reports it writes on your
computer, and sends none of them anywhere. It holds no account and no API key.
