# Releases

## What a human does

Run **prepare release** from the Actions tab with `patch`, `minor` or `major`, or an exact
version. Only the repository's owner can. It raises the version in `Cargo.toml`, under
`[workspace.package]` where every crate takes it from, in the entry `Cargo.lock` keeps for each
crate, and in the one `fuzz/Cargo.lock` keeps for the crate the fuzz harness builds on, on a
`release/vX.Y.Z` branch with a signed commit, opens the pull request, sets it
to merge itself once its checks are green, and links it in the run summary. Everything after
that is automatic.

Nobody types a version twice and nobody creates a tag by hand, which is the release step that
cannot be checked afterwards and the one most likely to be done from the wrong branch.

### The packaging app

The branch, the commit and the pull request are made as a GitHub App rather than with the job
token. GitHub holds the checks of a pull request the job token opens until someone approves
them, and a merge made with the job token starts no workflow, so the tag would never be cut.
The App is installed on this repository with write access to contents and pull requests. Its
client ID is the variable `PACKAGING_APP_CLIENT_ID` and its private key the secret
`PACKAGING_APP_KEY`, both in the `release-prepare` environment, which only `main` can deploy to.
Each run mints a token from them that lasts an hour and can write contents and pull requests of
this repository alone; the run stops at once if either is missing. The release's **package
managers (main)** job lands the Homebrew cask and the Scoop manifest the same way, as the same
App, so it needs the same two in a second environment, `packages`, which only tags matching
`v*` can deploy to (see [Package managers](#package-managers)).

Three repository settings have to be in place too. **Allow auto-merge** is on, so the pull
request can merge itself. The pull request is opened with the `release` label, which is how the
changelog keeps it out of the next release's notes: the label has to exist, because `gh` fails
on one it cannot find, so deleting it stops a release being prepared rather than quietly putting
the line back. And a re-run after one that stopped half way makes the version commit again on
`main`'s head, on a staging branch of its own, and moves the release branch onto it in one step,
so the pull request it opened stays open and is reused: moved to `main`'s head first, the branch
would have nothing in it, and GitHub closes a pull request that has nothing in it.

## What the changelog says

`tools/release/notes.mjs` writes it, from the files each pull request touched rather than from
its title. Entries are split into what reaches the download, meaning `crates/` (bar each crate's
examples and tests), `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `README.md`, `LICENSE`,
`third-party/` and `tools/release/notices.mjs`, which write the notices each archive carries,
`tools/release/package.sh`, which packs the archives and installers,
`tools/release/package-managers.sh`, which writes the Scoop manifest a release carries, and
`release-build.yml` itself, and what stays in this repository: training, the reference
data, the browser checks, CI and the documents. Most of what lands here is the second kind, and
a title describes a change, not its reach; in one flat list a training run and a change to the
reader read alike.

Run it against any published tag to see what a release carried:

```bash
GITHUB_REPOSITORY=Aureliolo/steamgauge GH_TOKEN="$(gh auth token)" node tools/release/notes.mjs v0.1.0
```

It picks the newest release below the tag, so it reads the same afterwards as it did at the
time. The first release has nothing below it and says it is the first release.

## What happens on the merge

`release-tag.yml` sees a new version on `main` with no matching tag, creates `vX.Y.Z`, and
dispatches `release.yml` on it, since a tag it makes with the job token would otherwise start
nothing. `release.yml` calls `release-build.yml`, which runs the first six jobs, then verifies,
publishes and packages:

1. **gate** refuses to go on unless the version is three numbers (a release is published as the
   latest and is immutable, so a pre-release would stand as the newest release for good), the
   tag matches the workspace version, every crate takes
   that version and `Cargo.lock` agrees, the release commit is reachable from `main`, and that
   commit carries a valid signature. Then it runs the formatter, clippy and the tests with
   `--locked`, as CI does, checks the working tree is still clean, and writes the changelog.
2. **notices** writes each archive's `THIRD-PARTY-NOTICES.txt` with cargo-about, for that
   platform's target and GPU feature from `Cargo.lock` (see below). It compiles nothing, and it
   is a job of its own so that the build runs no tool the binary does not need.
3. **build**, once for each platform, builds the binary with `--locked`, `custom-protocol` (the
   pages served from inside the binary, no developer tools) and the GPU backend that platform
   ships with, and checks the build left the tree as it found it. `tools/release/package.sh`
   then packs the portable archive (the binary, the runtime libraries beside it, the README,
   the licence and the third-party notices) and packages that same build into the platform's
   installers: Tauri's bundler makes the Windows setup program and the macOS disk image, and
   nFPM makes the `.deb` and the `.rpm` from the Linux archive's own files
   (`tools/release/linux-packages.sh`, `tools/release/linux/nfpm.yaml`), with a man page
   help2man writes from the program's `--help`. Both tools are pinned by version and digest. It
   also records the crates `cargo tree` resolves for that target and those features.
4. **sbom** first reads each archive's notices back against that crate list and the files the
   archive holds. Then it builds an SPDX SBOM of each archive: every file in it with its
   SHA-256, and every crate that build compiled, read from `Cargo.lock` cut down to exactly
   those crates, since a binary names nothing on its own. The job checks each SBOM against its
   archive's members and hashes and against the crate list, because an SBOM that lists nothing
   looks exactly like a passing step (`.github/syft.yaml` says what syft reads). Last, it writes
   the Scoop manifest `steamgauge.json` from the Windows archive's checksum
   (`tools/release/package-managers.sh`), and `SHA256SUMS` over the archives, the installers,
   the SBOMs and that manifest.
5. **install**, on each platform, installs that platform's installers the way a person would
   (the setup program silently, the disk image copied to Applications, the `.deb` through apt
   and the `.rpm` through dnf on Fedora), runs the installed program, which has to name the
   version, and starts its window, which has to stay up (`tools/release/install-check.sh`).
   Each installer has to name its maker, licence and package exactly as the check writes them:
   the `.deb`'s control fields, files, copyright file and changelog, the `.rpm`'s header,
   relations, files and changelog, the app's `Info.plist`, and on Windows the setup program's
   and the program's version details and the entry in Apps. Before installing, Debian's lintian
   (on Debian 13) and Fedora's rpmlint (`--strict`) have to pass the `.deb` and the `.rpm`, with
   a warning failing as an error does; what each reports that is so by design is in
   `tools/release/linux/lintian-overrides` and `tools/release/linux/rpmlint.toml`, each with its
   reason. On Windows and Linux it first installs 0.1.2, whose packages are named
   `steam-gauge` and whose setup program registers the publisher `Aurelio`, and the new
   installer has to replace it. On Windows it uninstalls again and checks the program is gone.
6. **attest** signs every file that ships through Sigstore, attests each platform's SBOM against
   its archive and its installers, and gathers the four signed attestations into one JSON Lines
   file. It is the only job with a token that can sign: see below.
7. **verify**, in `release.yml`, rechecks the checksums and verifies every file against that
   JSON Lines file the way a user would, naming `release-build.yml` at this tag as the builder.
   It holds no token that can write.
8. **publish**, in `release.yml`, runs only on a tag. It checks the checksums once more and
   creates the GitHub Release with all thirteen files in one call, because an immutable release
   locks its files the moment it is published.
9. **package managers (write)** downloads the published `SHA256SUMS` and `steamgauge.json`,
   verifies both against the release's attestation, and writes the Homebrew cask, the Scoop
   manifest and the winget manifests from those checksums with
   `tools/release/package-managers.sh` at the tag. The Scoop manifest it writes has to be the
   release's own byte for byte. While the winget job is off, its run summary gives the command
   that submits the first version by hand (see [Package managers](#package-managers)).
10. **package managers** (`package-managers.yml`) installs each of them as a person would and
    runs what it installed: the cask with Homebrew on Apple Silicon macOS, the Scoop manifest by
    the release's address and as a bucket, and the winget manifests with `winget install
    --manifest`, the last two on Windows. winget comes from its own pinned release, because
    the runner image's lags the manifest schema and warns over every header. Each installed
    program has to say `steamgauge X.Y.Z` to `--version` and keep its window up for 20 seconds,
    and each uninstall has to remove it.
11. **package managers (main)** opens a pull request putting the cask and the Scoop manifest on
    `main`, as the packaging App, and merges it once every check has passed; a check that fails,
    or anything else that keeps it from merging, fails the job with the reason.
12. **winget**, once the repository variable `WINGET` is `submit`, checks the setup program the
    winget manifests name against the release, its attestation and its hash, and submits the
    manifests with Microsoft's `wingetcreate` to `microsoft/winget-pkgs`, where Microsoft's
    checks and moderators merge them.

## A dry run

Dispatch **release** on a branch rather than a tag. Every job up to verify runs exactly as it
would for a release, bar the two checks only a tag can pass (its name, and being on `main`), and
publish and every job after it are skipped. The run summary carries the checksums and the notes
a release from there would have. The attestations it makes name the branch as their source, so
none of them can pass for a release's.

## What a release carries

- The installers: `steamgauge-X.Y.Z-windows-x64-setup.exe` (Windows on x86-64 with DirectML,
  installed for the current user, fetching WebView2 if the machine lacks it),
  `steamgauge-X.Y.Z-macos-arm64.dmg` (macOS on Apple Silicon with CoreML), and
  `steamgauge_X.Y.Z-1_amd64.deb` and `steamgauge-X.Y.Z-1.x86_64.rpm` (Linux on x86-64 on the CPU,
  each the package `steamgauge`, declaring the C library and WebKitGTK 4.1 so the package
  manager installs what is missing, and replacing the `steam-gauge` package of 0.1.2 and
  earlier).
- One portable archive per platform holding the same program: `steamgauge-X.Y.Z-<platform>.zip`
  for Windows and `steamgauge-X.Y.Z-<platform>.tar.gz` for the others, where `<platform>` is the
  Rust target without its placeholder vendor: `x86_64-pc-windows-msvc`, `aarch64-apple-darwin`
  and `x86_64-linux-gnu` (`tools/release/names.sh`). Each holds the binary, `DirectML.dll` on
  Windows, `README.md`, `LICENSE` and `THIRD-PARTY-NOTICES.txt`; the Linux one needs WebKitGTK
  4.1 installed.
- An SPDX SBOM of each platform's program, `steamgauge-X.Y.Z-<platform>.spdx.json`.
- `steamgauge.json`, the Scoop manifest, which `scoop install` reads by the release's address.
- `SHA256SUMS`, over the installers, the archives, the SBOMs and the Scoop manifest.
- A Sigstore build-provenance attestation over all of them, and an SBOM attestation tying each
  SBOM to its platform's archive and installers. Both are keyless: there is no signing key
  anywhere, including in CI.
  They are stored on the repository and attached to the release as
  `steamgauge-X.Y.Z.intoto.jsonl`, which is also the file OpenSSF Scorecard looks for. The
  checksum file has no signature of its own beside it; the provenance is that signature.

Releases are immutable, so a published one cannot be edited or replaced.

## Verifying a release

```bash
VERSION=X.Y.Z
FILE="steamgauge-${VERSION}-windows-x64-setup.exe"   # or whichever file you took
gh release download "v${VERSION}" --repo Aureliolo/steamgauge
sha256sum --check --ignore-missing SHA256SUMS
gh attestation verify "${FILE}" --repo Aureliolo/steamgauge \
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

## Package managers

```bash
winget install Aureliolo.SteamGauge
scoop install https://github.com/Aureliolo/steamgauge/releases/latest/download/steamgauge.json
brew tap aureliolo/steamgauge https://github.com/Aureliolo/steamgauge
brew install --cask aureliolo/steamgauge/steamgauge
```

- **Homebrew.** This repository is its own tap. Its name does not start with `homebrew-`, so
  `brew tap` needs its address. `Casks/steamgauge.rb` is a cask rather than a formula, because
  what ships for macOS is an app in a disk image: it installs `SteamGauge.app` from the `.dmg`
  on Apple Silicon with macOS 13 or later, and puts `steamgauge` on the `PATH` through a script
  that runs the program inside the app by its real path. A symlink would not do, because Tauri
  on macOS refuses its own path when that passes through one. There is no cask for Linux, and
  no apt or dnf repository either: Linux takes the release's `.deb` or `.rpm`.
- **Scoop.** `bucket/steamgauge.json` installs the portable Windows archive, puts `steamgauge`
  on the `PATH` by adding its own folder there, and adds a Start menu shortcut for the window.
  Scoop's shim would not do: Scoop makes the shim of a windowed program windowed too, and that
  shim returns at once and passes on none of the program's output. The same
  manifest is attached to every release, which is what the address above reads; this repository
  is also a bucket (`scoop bucket add aureliolo https://github.com/Aureliolo/steamgauge`), which
  `scoop update` then follows.
- **winget.** `Aureliolo.SteamGauge`, published by `Aureliolo`, installs the release's setup
  program, for the current user and silently, under the product code `SteamGauge`, the name
  Tauri's setup program registers its uninstall entry under, with the publisher it registers
  there, `Aurelio Amoroso`, so winget recognises a copy installed from the release page too.

Every hash in them is one `SHA256SUMS` gives, after the release's attestation over that file
has verified. Nothing in `Casks/` or `bucket/` is edited by hand: each release writes both and
lands them on `main` through the packaging App, and a pull request that changes them anyway is
held to what the release they name is signed over (`package-managers.yml`). That pull request
check installs what its own branch's script writes for the latest release, so a change to the
script is installed and run before any release depends on it. Until the first release, neither
folder exists, and the tap and the bucket are empty.

### Setting them up, once

Create an environment named `packages` under Settings, Environments, and limit it to tags
matching `v*`. Put the packaging App's client ID in its variable `PACKAGING_APP_CLIENT_ID` and
the App's private key in its secret `PACKAGING_APP_KEY`, as for `release-prepare`. The pull
request the release opens needs no approval and merges once its checks pass.

### winget

winget takes packages only through a pull request to `microsoft/winget-pkgs`, which
`wingetcreate` opens from a fork. A new package waits on a moderator, so the first version is
submitted by hand, and the **winget** job stays off until the repository variable `WINGET` is
`submit`.

1. Once the first release is published and its **package managers** jobs have passed, its run
   summary gives the two commands, under **winget**. On Windows, with the GitHub CLI and
   `wingetcreate` installed (`winget install GitHub.cli Microsoft.WingetCreate`):

   ```powershell
   gh run download <run id> --repo Aureliolo/steamgauge --name package-managers --dir package-managers
   wingetcreate submit --prtitle "New package: Aureliolo.SteamGauge version X.Y.Z" package-managers/winget
   ```

   `wingetcreate` asks you to sign in to GitHub the first time, and opens the pull request from
   your fork of `winget-pkgs` (`Aureliolo/winget-pkgs`). The run keeps those files for 30 days.
   After that, write them again from the release, in a checkout of its tag:

   ```bash
   gh release download vX.Y.Z --repo Aureliolo/steamgauge --dir release \
     --pattern SHA256SUMS --pattern steamgauge-X.Y.Z.intoto.jsonl
   gh attestation verify release/SHA256SUMS --repo Aureliolo/steamgauge \
     --bundle release/steamgauge-X.Y.Z.intoto.jsonl \
     --signer-workflow Aureliolo/steamgauge/.github/workflows/release-build.yml \
     --source-ref refs/tags/vX.Y.Z --deny-self-hosted-runners
   bash tools/release/package-managers.sh X.Y.Z release/SHA256SUMS package-managers
   ```

2. Wait for a moderator to merge it.
3. Make a **classic** token with only the `public_repo` scope, and an expiry, on the account
   whose fork `wingetcreate` submits from: GitHub lets no fine-grained token open a pull request
   on a repository its owner is not a member of, and `wingetcreate` says so. A classic
   `public_repo` token can push to every public repository of its account, this one included,
   so the safer owner is a GitHub account used for nothing else, with its own fork of
   `microsoft/winget-pkgs` and no access to this repository.
4. Create an environment named `winget`, limited to tags matching `v*`, and put the token in its
   secret `WINGET_TOKEN`.
5. Set the repository variable `WINGET` to `submit`.

From then on every release submits itself as `New version: Aureliolo.SteamGauge version X.Y.Z`,
after the **package managers** jobs have installed it. With `WINGET` set, a missing token fails
the release; when the token expires, the job fails, and a new one is made the same way.

## Operating-system signing

No build carries a certificate that Windows or macOS trusts, so both warn on first launch
(SECURITY.md explains the difference between that and provenance). Windows binaries are to gain
a SignPath Foundation signature once the project is enrolled; nothing in the pipeline does that
yet. macOS has no free certificate authority, so its disk image and archive carry provenance
only.

Windows builds are hardened the same way whether they ship or not (`.cargo/config.toml` and the
app's `build.rs`): Control Flow Guard, compatibility with the hardware shadow stack, and the
compiler's Spectre mitigations for the C that crates compile. BinSkim (`binaries.yml`) reads the
Windows and Linux programs on every pull request and refuses any mitigation missing.

## Versions that cannot be released

A version whose tag exists cannot be released again, whether or not a release is attached to
that tag. The ruleset on `v*` allows no deletion and no update, which is what makes a tag worth
verifying a build against, and the cost is that the number is spent for good: cutting one twice
would leave two different builds answering to one version. Prepare release refuses such a
version, naming it, before it writes a branch.

## When a release job fails

Re-running the release workflow on the tag is the first thing to try, and it is safe: the
publish job asks what the tag already carries before acting, and passes when that is exactly
the thirteen files, and package managers (main) picks its pull request up where it is. **tag
release** can be run by hand too, and starts the release workflow again on a tag that already
exists.

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
- **Expected exactly one file matching ...**, in the build job: Tauri's bundler wrote no
  installer of that kind, or more than one, usually after a tauri-cli upgrade changed its file
  names. Fix `tools/release/package.sh` on `main` and prepare the next version.
- **... holds files the packages give no place** or **nFPM wrote no ...**, in the Linux build
  job: the Linux archive gained a file `tools/release/linux/nfpm.yaml` does not install, or nFPM
  named a package differently. Give the file its place, or fix the name, in
  `tools/release/linux-packages.sh` on `main` and prepare the next version.
- **lintian** or **rpmlint** failing, in the Linux install job: the package breaks a rule of
  Debian's or Fedora's, named in the lines above. Fix the package; an override goes in
  `tools/release/linux/` only with the reason the rule does not apply.
- **... --version said ...**, **The installer put no ... in ...**, **The window exited within 20
  seconds of starting** or **Uninstalling left ... behind**, in an install job: an installer
  installs something that does not run, or does not run as it should, on its own system. That
  is the failure the job exists for; nothing was signed or published.
- **vX.Y.Z already has a release, and it carries ...**: the tag has a release with something
  other than the thirteen files, which means an upload failed part way. A published release is
  not rewritten here, so look at what is attached before deciding anything.
- **... does not hold exactly one SHA-256 for ...**, in package managers (write) or the sbom
  job: a file the cask or a manifest names is missing from `SHA256SUMS`, usually after its name
  changed in `tools/release/package.sh` and not in `tools/release/package-managers.sh`.
- **... --version said ...**, **The window exited within 20 seconds of starting**, or
  **Uninstalling left ... behind**, in a package managers job: Homebrew, Scoop or winget installs
  something that does not run on its own system. The release is out, and nothing has reached
  `main` or `winget-pkgs`; fix it on `main` and release the next version.
- **The packaging App is not set up**, in package managers (main): the `packages` environment
  lacks `PACKAGING_APP_CLIENT_ID` or `PACKAGING_APP_KEY` (see
  [Setting them up, once](#setting-them-up-once)). Add them and re-run the job.
- **Pull request #N failed: ...**, **... conflicts with main** or **... is OPEN, not merged**:
  the pull request putting the cask and the Scoop manifest on `main` did not merge. The message
  names the check or the reason; a re-run of the job picks the pull request up where it is.
- **WINGET is set to submit, but the secret WINGET_TOKEN ...** or a refusal from
  `wingetcreate`: the token is missing or expired (see [winget](#winget)). Make a new one and
  re-run the job.
