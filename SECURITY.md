# Security

## Supported versions

Only the latest release gets fixes. A fix ships as a new release, and the app names a newer
release in its window when one is out and offers to install it (see
[Updates from the window](#updates-from-the-window)).

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
stored on the repository, so they verify from the file alone; the build provenance is also
attached alone, as `steamgauge-<version>.provenance.sigstore.json`. Releases are immutable and the
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
certificate authority, so its builds carry provenance only. The macOS app is sealed ad hoc, with
no identity at all: `codesign --verify --deep --strict` on it says no file in the bundle has
changed since the build, and nothing about who built it, which is what provenance answers.

## Updates from the window

**Update now**, and `steamgauge update`, which runs the same code from a terminal, installs a
newer release with no signing key anywhere: what it checks is the
release's Sigstore build provenance, which GitHub Actions signs keyless for each release run.
It downloads the release's file for this computer (the setup program on Windows,
`steamgauge-<version>-macos-arm64.app.tar.gz` on macOS, the `.deb`, the `.rpm` or the portable
archive on Linux) into the app's own data folder, hashing it as it is written, and installs it
only when all of these hold:

- the version is newer than the one running;
- the provenance's certificate chains to Sigstore's Fulcio, carries its certificate transparency
  timestamp, and its signature is in Sigstore's transparency log, Rekor; none of these is
  skipped;
- the certificate was issued by GitHub Actions (`https://token.actions.githubusercontent.com`)
  to exactly
  `https://github.com/Aureliolo/steamgauge/.github/workflows/release-build.yml@refs/tags/v<version>`,
  for the repository and owner by GitHub's numbers for them (1361019156 and 19254254), which
  survive a rename and are not reused by a repository made later under the same name, at the
  ref `refs/tags/v<version>`, on a GitHub-hosted runner;
- the signed statement is SLSA provenance (`https://slsa.dev/provenance/v1`) and names the file
  by its release name with the SHA-256 it arrived with.

The tag in the certificate is the version being installed, so an older release, validly signed
for its own tag, is refused as well as anything signed elsewhere. The provenance is read from
the release's `steamgauge-<version>.provenance.sigstore.json`, and from GitHub's attestations
API only where a release lacks that file; where it came from carries no trust, since it is
verified either way. Sigstore's trusted root, the certificates and keys a signature is checked
against, comes from Sigstore's TUF repository, starting from the TUF root built into the app;
offline, the last root fetched is used, and before any was fetched, the trusted root built into
the app.

On Windows the file stays open, readable by others but not writable, from the moment it is
hashed to the moment the setup program starts. On macOS the app is unpacked beside the
installed one, on the same disk, and the two are exchanged in one step. On Linux the package
manager installs the verified package, and a portable archive's files are renamed into place.
Any failed check installs nothing, says why, and leaves the release page to download from; the
commands under [Releases](#releases) verify such a download by hand.

This protects against a download changed on the way or in storage, against TLS interception,
and against a release published with a stolen token, since a valid signature needs the release
workflow to have run at that version's tag, and every such signing is public in Rekor. It does
not protect against the owner's GitHub account being taken over, or against a compromised
build, both of which would produce a correctly signed release.

## Steering from other programs

`steamgauge mcp` lets Claude Code, or any other program that speaks the Model Context Protocol,
steer the app. Every tool there is something the window can do, including removing games and
models, changing settings and updating the app, so a client is trusted as the person is: any
program that can start `steamgauge mcp` as you can already run the app and its commands as you.

The open app listens on no network port. On Windows it listens on a named pipe whose name is
drawn at random each time the app opens and written only into the app's local data folder, which
is yours: another user's program can neither find the pipe nor take its name first to stand in
for the app. Pipes refuse connections from other machines, and Windows gives other users read
access to a pipe at most, which cannot send a request. On macOS and Linux it listens on a socket
file in a folder only you may enter, inside the app's data folder.

Settings can also have the open app answer the same tools over HTTP, for a program that connects
no other way. It is off until switched on. It listens on 127.0.0.1 alone, at the port chosen, so
no other machine reaches it; but every program on this computer can reach a port there, a web
page in a browser among them, so three checks stand in for the socket's folder:

- every request has to carry a token, 256 bits drawn at random, in its `Authorization` header;
  the token is kept in the app's local data folder, compared in constant time, and replaced with
  **New token**, which shuts out whoever held the old one;
- a request carrying an `Origin` header is refused, which is every request a browser sends on a
  web page's behalf;
- a request whose `Host` is not `127.0.0.1` or `localhost` at that port is refused, which is how a
  web page that had its own name point at 127.0.0.1 would arrive.

Tools that remove something are marked destructive and tools that only look are marked
read-only, so a client that asks before acting knows which to ask about. What the tools return
includes reviews, written by strangers: a client that takes text in a tool's answer for
instructions can be steered by a review.

## Network requests

The app makes requests to four places, and to nothing else:

- **Steam**, for reviews, game names and review totals, the store's search when a game is
  found by its name, each game's store picture (fetched once from Steam's image servers and kept
  on disk; the window itself never reaches the network), and, when the app opens, how many
  reviews each game in the library has now (at most every six hours; a setting turns it off).
- **Hugging Face**, for the readers and the search models, each file fetched from a pinned
  commit and checked against its pinned length and SHA-256 before it is used, and, when the
  library is checked against Steam, for whether newer releases of those models are published.
- **GitHub**, at most once a day, for whether a newer release of SteamGauge is out: one request
  to the release page, whose redirect names the newest version. The redirect is not followed,
  no token is involved, and a setting turns it off. Only when **Update now** is chosen does the
  app download the release's file for this computer and its provenance, and ask the
  attestations API where the release carries no provenance file. The link the window offers
  opens only a release page of this repository.
- **Sigstore**, only during an update, for its current trusted root from
  `https://tuf-repo-cdn.sigstore.dev`, which is itself signed and checked against the TUF root
  built into the app.

## Handling of data

The app keeps the reviews it downloads, what it reads in them and the reports it writes on your
computer, and sends none of them anywhere. It holds no account and no API key.
