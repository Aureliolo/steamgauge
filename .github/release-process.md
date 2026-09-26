# Releases

## What a human does

Run **prepare release** from the Actions tab with `patch`, `minor` or `major`, or an exact
version. It raises the version in `Cargo.toml`, under `[workspace.package]` where every crate
takes it from, and in the entry `Cargo.lock` keeps for each crate, on a `release/vX.Y.Z` branch
with a signed commit, opens the pull request, and links it in the run summary.

That pull request's checks are held at the start. GitHub creates the runs for anything a
workflow opens with the job token but does not start them, so the merge box carries a banner
offering **Approve workflows to run**. Click it, then merge once the checks are green.
Everything after the merge is automatic.

Nobody types a version twice and nobody creates a tag by hand, which is the release step that
cannot be checked afterwards and the one most likely to be done from the wrong branch.

Two repository settings have to be in place for the button to work. The pull request is opened
with the `release` label, which is how the changelog keeps it out of the next release's notes:
the label has to exist, because `gh` fails on one it cannot find, so deleting it stops a release
being prepared rather than quietly putting the line back. And the job token can only open a
pull request while **Allow GitHub Actions to create and approve pull requests** is on, under
Settings, Actions, General.

## What the changelog says

`tools/release/notes.mjs` writes it, from the files each pull request touched rather than from
its title. Entries are split into what reaches the download, meaning `crates/` (bar each crate's
examples and tests), `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `README.md`, `LICENSE`,
`third-party/` and `tools/release/notices.mjs`, which write the notices each archive carries,
and `release-build.yml` itself, and what stays in this repository: training, the reference
data, the browser checks, CI and the documents. Most of what lands here is the second kind, and
a title describes a change, not its reach; in one flat list a training run and a change to the
reader read alike.

Run it against any published tag to see what a release carried:

```bash
GITHUB_REPOSITORY=Aureliolo/steamgauge GH_TOKEN="$(gh auth token)" node tools/release/notes.mjs v0.1.0
```

It picks the newest release below the tag, so it reads the same afterwards as it did at the
time. The first release has nothing below it and lists everything.

## What happens on the merge

`release-tag.yml` sees a new version on `main` with no matching tag, creates `vX.Y.Z`, and
dispatches `release.yml` on it, since a tag it makes with the job token would otherwise start
nothing. `release.yml` calls `release-build.yml`, which runs the first five jobs, then verifies
and publishes:

1. **gate** refuses to go on unless the tag matches the workspace version, every crate takes
   that version and `Cargo.lock` agrees, the release commit is reachable from `main`, and that
   commit carries a valid signature. Then it runs the formatter, clippy and the tests with
   `--locked`, as CI does, checks the working tree is still clean, and writes the changelog.
2. **notices** writes each archive's `THIRD-PARTY-NOTICES.txt` with cargo-about, for that
   platform's target and GPU feature from `Cargo.lock` (see below). It compiles nothing, and it
   is a job of its own so that the build runs no tool the binary does not need.
3. **build**, once for each platform, builds the binary with `--locked` and the GPU backend that
   platform ships with, checks the build left the tree as it found it, and packs the archive:
   the binary, the runtime libraries beside it, the README, the licence and the third-party
   notices. It also records the crates `cargo tree` resolves for that target and those
   features.
4. **sbom** first reads each archive's notices back against that crate list and the files the
   archive holds. Then it builds an SPDX SBOM of each archive: every file in it with its
   SHA-256, and every crate that build compiled, read from `Cargo.lock` cut down to exactly
   those crates, since a binary names nothing on its own. The job checks each SBOM against its
   archive's members and hashes and against the crate list, because an SBOM that lists nothing
   looks exactly like a passing step (`.github/syft.yaml` says what syft reads). Last, it writes
   `SHA256SUMS` over the archives and the SBOMs.
5. **attest** signs every file that ships through Sigstore, attests each SBOM against its
   archive, and gathers the four signed attestations into one JSON Lines file. It is the only
   job with a token that can sign: see below.
6. **verify**, in `release.yml`, rechecks the checksums and verifies every file against that
   JSON Lines file the way a user would, naming `release-build.yml` at this tag as the builder.
   It holds no token that can write.
7. **publish**, in `release.yml`, runs only on a tag. It checks the checksums once more and
   creates the GitHub Release with all eight files in one call, because an immutable release
   locks its files the moment it is published.

## A dry run

Dispatch **release** on a branch rather than a tag. Every job up to verify runs exactly as it
would for a release, bar the two checks only a tag can pass (its name, and being on `main`), and
publish is skipped. The run summary carries the checksums and the notes a release from there
would have. The attestations it makes name the branch as their source, so none of them can
pass for a release's.

## What a release carries

- One archive per platform, `steamgauge-X.Y.Z-<target>.tar.gz`: Windows on x86-64 with
  DirectML, macOS on Apple Silicon with CoreML, and Linux on x86-64 on the CPU, which needs
  WebKitGTK 4.1 installed. Each holds the binary, `DirectML.dll` on Windows, `README.md`,
  `LICENSE` and `THIRD-PARTY-NOTICES.txt`.
- An SPDX SBOM of each archive, `steamgauge-X.Y.Z-<target>.spdx.json`.
- `SHA256SUMS`, over the archives and the SBOMs.
- A Sigstore build-provenance attestation over all seven, and an SBOM attestation tying each
  SBOM to its archive. Both are keyless: there is no signing key anywhere, including in CI.
  They are stored on the repository and attached to the release as
  `steamgauge-X.Y.Z.intoto.jsonl`, which is also the file OpenSSF Scorecard looks for. The
  checksum file has no signature of its own beside it; the provenance is that signature.

Releases are immutable, so a published one cannot be edited or replaced.

## Verifying a release

```bash
VERSION=X.Y.Z
TARGET=x86_64-pc-windows-msvc   # or aarch64-apple-darwin, x86_64-unknown-linux-gnu
gh release download "v${VERSION}" --repo Aureliolo/steamgauge
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify "steamgauge-${VERSION}-${TARGET}.tar.gz" --repo Aureliolo/steamgauge \
  --bundle "steamgauge-${VERSION}.intoto.jsonl" \
  --signer-workflow Aureliolo/steamgauge/.github/workflows/release-build.yml \
  --source-ref "refs/tags/v${VERSION}" \
  --deny-self-hosted-runners
```

The checksum proves the bytes match what the release lists. The attestation proves GitHub
Actions built those bytes from this repository, by the steps in `release-build.yml` at that
tag, on a GitHub-hosted runner, which the checksum alone cannot: a checksum generated alongside
a tampered archive agrees with it perfectly. Add `--predicate-type https://spdx.dev/Document/v2.3`
to check the SBOM attestation instead, and drop `--bundle` to read the same attestations from
GitHub's API.

## SLSA

SLSA Build Level 3 on GitHub Actions means the build runs inside a reusable workflow, so the
identity in the Sigstore certificate names the build steps rather than whatever workflow called
them. `release-build.yml` is that workflow: it checks out the tag, gates, builds, writes the
SBOMs and signs, and `release.yml` only decides when that happens and publishes the result. A
verifier that passes `--signer-workflow .../release-build.yml` is requiring those exact steps,
at the tag the provenance names, and no caller can change them.

Inside it, the build and the signing are separate jobs. `id-token: write` puts the signing
token within reach of every step in the job that holds it, and a Rust build runs the build
script of every crate in the lock file, so only the attest job, which downloads the finished
bytes and signs them, is given that permission. Runners are GitHub-hosted and ephemeral, and
signing is keyless: there is no key anywhere to take. The release build restores no Actions
cache, which is the one way one run can reach into another on this platform: a cache entry can
be written by any job on any branch, including a pull request from a fork. CI keeps its cache;
it signs nothing.

## What the SBOM does not name

ONNX Runtime reaches the binary as a prebuilt library that the `ort-sys` build script
downloads, and `DirectML.dll` ships beside the Windows binary. syft cannot recognise a prebuilt
library as a package, so the SBOM names them only through the `ort` and `ort-sys` crates that
fetched them, whose versions pin theirs, and lists `DirectML.dll` as a file with its hash. The
notices name both, with their versions.

## The third-party notices

Most of the crates in the binary are under MIT, BSD or Apache licences, and each of those asks
that its notice travel with a binary built from the code. `THIRD-PARTY-NOTICES.txt` is that: a
line per crate with the licence it is used under and where its source is, each licence text once
with the crates it is the text for, the NOTICE files the crates carry, then ONNX Runtime's
licence and third-party notices and, in the Windows archive, DirectML's. `tools/release/notices.mjs`
writes it through cargo-about, and `third-party/README.md` says where each text comes from and
what DirectML's licence asks of whoever passes the DLL on.

It is refused rather than written short. cargo-about stops on a crate whose licence
`third-party/about.toml` does not accept, and the script on a crate whose only text would be
SPDX's template for a licence that names a copyright holder, on a clarification whose file has
changed, and on an `ort-sys` that links an ONNX Runtime other than the one whose texts
`third-party/` holds. CI writes the notices for all three archives on every pull request, so
those land on the pull request that causes them. At release, the sbom job reads each archive's
notices back: every crate its build resolved that is not this repository's own is listed,
DirectML's licence is there exactly when `DirectML.dll` is and that DLL is the version whose
licence was read, and the archive holds no file the notices and `LICENSE` do not account for.

cargo-about is held at 0.8.4: the prebuilt 0.9 releases fail on the first licence they have to
fetch from a crate's repository (`renovate.json` records it).

## Operating-system signing

No build carries a certificate that Windows or macOS trusts, so both warn on first launch
(SECURITY.md explains the difference between that and provenance). Windows binaries are to gain
a SignPath Foundation signature once the project is enrolled; nothing in the pipeline does that
yet. macOS has no free certificate authority, so its archive carries provenance only.

## Versions that cannot be released

A version whose tag exists cannot be released again, whether or not a release is attached to
that tag. The ruleset on `v*` allows no deletion and no update, which is what makes a tag worth
verifying a build against, and the cost is that the number is spent for good: cutting one twice
would leave two different builds answering to one version. Prepare release refuses such a
version, naming it, before it writes a branch.

## When a release job fails

Re-running the release workflow on the tag is the first thing to try, and it is safe: the
publish job asks what the tag already carries before acting, and passes when that is exactly
the eight files. **tag release** can be run by hand too, and starts the release workflow again
on a tag that already exists.

- **Tag does not match the workspace version**: the tag was created outside `release-tag.yml`,
  at a commit whose `Cargo.toml` says something else. It cannot be taken back, because the
  ruleset refuses a push that deletes a `v*` tag. Go through prepare release for the version
  that should ship and leave the tag standing; like the case below, that number is spent.
- **vX.Y.Z is tagged at another commit**: that version is already spent. Nothing can move the
  tag, so raise the version past it.
- **X sets its own version**, or **Cargo.lock records X**: a crate stopped taking the workspace
  version, or the lock file was not moved with it. Fix it on `main` and prepare the next
  version; this one's tag, if it was made, is spent.
- **Release commit is not reachable from main**: the tag points at a commit that never landed.
- **Release commit carries no valid signature**: every branch requires signed commits, so this
  only fires if that ruleset was bypassed or removed. That is exactly when you want to hear
  about it.
- **Tests or build steps modified the checked-out release source**, or **The build modified**
  it: something writes into the tree, which would mean the archive does not match the tagged
  commit.
- **The SBOM's files are not the members of ...**, **The SBOM's crates are not the ones ...**,
  or **The SBOM for ... does not list ...**: syft read something other than the unpacked
  archive and the cut-down lock file, or a syft upgrade changed what its catalogers see or how
  it names a crate. The SBOM is wrong, not the archive; fix the sbom job and rerun the workflow
  on the tag.
- **Cargo.lock does not hold: ...**: `cargo tree` resolved a crate the lock file does not have,
  which `--locked` should make impossible. Look before rerunning.
- **ort-sys ... links ONNX Runtime ...**, **These crates ship no licence text ...**, **cargo-about
  could not read every crate's licence ...** or **cargo-about refused the crates ...**, in the
  notices job: the same failure CI's notices job reports on a pull request, so it should not
  reach a tag. The message says what to change; fix it on `main` and prepare the next version.
- **The notices in ... do not cover what it carries**: the archive holds a crate, a file or a
  `DirectML.dll` its notices do not account for, and the lines above it say which. A new runtime
  library beside the binary needs its licence in `third-party/` and in `notices.mjs` before it
  can ship.
- **vX.Y.Z already has a release, and it carries ...**: the tag has a release with something
  other than the eight files, which means an upload failed part way. A published release is not
  rewritten here, so look at what is attached before deciding anything.
