# Releases

## What a human does

First, someone who wrote none of it reads what ships to readers where they meet it: the README
and changed docs on the site, the CLI's help and errors, and the window's changed text. Each
passage that says too much, too vaguely or twice is cut or rewritten before the release.

Then run **prepare release** from the Actions tab with `patch`, `minor` or `major`, or an exact
version. Only the repository's owner can. Everything after that is automatic.

It raises the version in `Cargo.toml` (`[workspace.package]`) and `Cargo.lock` on a
`release/vX.Y.Z` branch, with a signed commit. It opens the pull request, sets it to merge itself,
and links it in the run summary. `tools/release/scope.mjs` proves the pull request changes only
the version, so its builds, installs and scans pass at once (`.github/actions/scope`).

### The packaging app

The branch, the commit and the pull request are made as a GitHub App, because a merge made with
the job token starts no workflow.

- The App is installed on this repository with write access to contents and pull requests.
- Its client ID is the variable `PACKAGING_APP_CLIENT_ID` and its private key the secret
  `PACKAGING_APP_KEY`, in the `release-prepare` environment, which only `main` can deploy to.
- The **package managers (main)** job needs the same two in the `packages` environment (see
  [Setting them up, once](#setting-them-up-once)).

The repository also needs:

- **Allow auto-merge** on, so the pull request can merge itself.
- The `release` label, which keeps the pull request out of the next changelog. `gh` fails without
  it.
- A re-run after one that stopped half way reuses the open pull request.

## What the changelog says

`tools/release/notes.mjs` writes it and sorts entries by the files each pull request touched.
What reaches the download is `crates/` (bar examples and tests), `Cargo.toml`, `Cargo.lock`,
`rust-toolchain.toml`, `README.md`, `LICENSE`, `third-party/`, `tools/release/notices.mjs`,
`tools/release/package.sh`, `tools/release/package-managers.sh` and `release-build.yml`. Everything
else stays in this repository.

To see what a published release carried:

```bash
GITHUB_REPOSITORY=Aureliolo/steamgauge GH_TOKEN="$(gh auth token)" node tools/release/notes.mjs v0.1.0
```

## What happens on the merge

`release-tag.yml` sees a new version on `main` with no tag, creates `vX.Y.Z`, and dispatches
`release.yml`. That calls `release-build.yml`, which runs the first six jobs.

1. **gate** refuses unless the version is three numbers, matches the tag, every crate and
   `Cargo.lock`, and the commit is signed and on `main`. `tools/release/checked.mjs` requires
   `main`'s ruleset checks to have passed on the last commit holding code.
2. **notices** writes each archive's `THIRD-PARTY-NOTICES.txt` with cargo-about, in a job of its
   own so the build runs no tool the binary does not need.
3. **build**, per platform, builds with `--locked`, `custom-protocol` and the platform's GPU
   backend, and checks the tree is unchanged. `tools/release/package.sh` packs the archive and the
   installers: Tauri for the Windows setup and the macOS disk image, nFPM for the `.deb` and `.rpm`.
4. **sbom** checks each archive's notices against its crates and files. It writes and checks an
   SPDX SBOM of each archive, then the Scoop manifest and `SHA256SUMS`.
5. **install**, per platform, installs each installer as a person would
   (`tools/release/install-check.sh`), after lintian and rpmlint (`--strict`) pass the Linux
   packages. The program has to run, keep its window up, and replace an installed 0.1.2.
6. **attest** signs every file through Sigstore and attests each SBOM. It is the only job with a
   token that can sign.
7. **verify**, in `release.yml`, rechecks the checksums and verifies every file as a user would
   and as the app's update does. It holds no token that can write.
8. **publish** runs only on a tag. It creates the release with all fifteen files in one call,
   because an immutable release locks its files once published.
9. **update check** (`update-check.yml`) installs the previous release each way a person would and
   updates it with `steamgauge update`. It runs after publishing, or by hand for any release, so
   a failure is fixed by the next release.
10. **package managers (write)** verifies the published `SHA256SUMS` and `steamgauge.json`, then
    writes the Homebrew cask, the Scoop manifest and the winget manifests from them.
11. **package managers** (`package-managers.yml`) installs each with its own manager, runs it, and
    uninstalls it. winget comes from its own pinned release.
12. **package managers (main)** opens a pull request putting the cask and Scoop manifest on
    `main`, as the packaging App, and merges it once its checks pass.
13. **winget**, once the repository variable `WINGET` is `submit`, checks the setup program
    against the release and submits the manifests to `microsoft/winget-pkgs` with `wingetcreate`.

## A dry run

Dispatch **release** on a branch rather than a tag. Every job up to verify runs, bar the checks
only a tag can pass, and publish and everything after it are skipped. The run summary carries the
checksums and notes a release would have.

A pull request that touches `release.yml`, `release-build.yml` or any workflow they call gets a
dry run before it merges. A missing permission stops the whole run, and only a run shows it.

## What a release carries

- `steamgauge-X.Y.Z-windows-x64-setup.exe`: Windows on x86-64 with DirectML, for the current
  user. It fetches WebView2 if missing.
- `steamgauge-X.Y.Z-macos-arm64.dmg`: macOS on Apple Silicon with CoreML.
- `steamgauge-X.Y.Z-macos-arm64.app.tar.gz`: the app from that disk image, which the app's own
  update unpacks.
- `steamgauge_X.Y.Z-1_amd64.deb` and `steamgauge-X.Y.Z-1.x86_64.rpm`: Linux on x86-64, on the CPU.
  Each declares the C library and WebKitGTK 4.1, and replaces the `steam-gauge` package of 0.1.2.
- `steamgauge-X.Y.Z-<platform>.zip` (Windows) or `.tar.gz`, one portable archive per platform
  (`tools/release/names.sh`). The Windows one also holds `DirectML.dll` and the C++ runtime DLLs.
- `steamgauge-X.Y.Z-<platform>.spdx.json`, an SPDX SBOM of each platform's program.
- `steamgauge.json`, the Scoop manifest.
- `SHA256SUMS`, over all of the above.
- `steamgauge-X.Y.Z.intoto.jsonl`, the build provenance and SBOM attestations. There is no
  signing key anywhere.
- `steamgauge-X.Y.Z.provenance.sigstore.json`, the provenance alone, which the app's update reads.

[SECURITY.md](../SECURITY.md#releases) has the commands that verify a release.

## SLSA

- `release-build.yml` is the reusable workflow the provenance names, so `--signer-workflow` pins
  the build steps. `release.yml` only decides when it runs and publishes the result.
- Only the attest job gets `id-token: write`. Every step of a job holding it can reach the
  signing token, and a Rust build runs every crate's build script.
- The release build restores no Actions cache, since any branch or fork pull request can write
  one. CI keeps its cache and signs nothing.

## What the SBOM does not name

ONNX Runtime arrives as a prebuilt library that `ort-sys` downloads, and syft cannot see it as a
package. The SBOM names it through the `ort` and `ort-sys` crates and lists `DirectML.dll` as a
file with its hash. The notices name both, with their versions.

## The third-party notices

`THIRD-PARTY-NOTICES.txt` carries every crate's licence and NOTICE files, ONNX Runtime's notices,
and in the Windows archive those of DirectML and the C++ runtime DLLs. `tools/release/notices.mjs`
writes it through cargo-about. `third-party/README.md` says where each text comes from.

It is refused rather than written short. A licence `third-party/about.toml` does not accept, a
template-only licence text, a changed clarification file or an unexpected ONNX Runtime stops it.
CI writes the notices on every pull request, so these failures land there.

## Package managers

- **Homebrew** taps this repository. `Casks/steamgauge.rb` puts `steamgauge` on the `PATH`
  through a script, because Tauri refuses its own path through a symlink.
- The cask says `auto_updates`, so `brew upgrade` needs `--greedy`.
- **Scoop** installs the portable Windows archive from `bucket/steamgauge.json`, the same manifest
  every release attaches. It adds its folder to the `PATH`, since Scoop's shim passes no output.
- **winget.** `Aureliolo.SteamGauge` installs the setup program for the current user, under the
  product code `SteamGauge` and publisher `Aurelio Amoroso`, so it recognises a copy installed by
  hand.
- **Linux** takes the release's `.deb` or `.rpm`. There is no apt or dnf repository.

Every hash comes from the verified `SHA256SUMS`. Nothing in `Casks/` or `bucket/` is edited by
hand, and `package-managers.yml` holds a pull request that changes them to the release they name.

### Setting them up, once

Create an environment named `packages` under Settings, Environments, limited to tags matching
`v*`. Put the packaging App's client ID in its variable `PACKAGING_APP_CLIENT_ID` and private key
in its secret `PACKAGING_APP_KEY`.

### winget

A new winget package waits on a moderator, so the first version is submitted by hand. The
**winget** job stays off until the repository variable `WINGET` is `submit`.

1. Once the first release's **package managers** jobs pass, its run summary gives two commands
   under **winget**. Run them on Windows, with `winget install GitHub.cli Microsoft.WingetCreate`
   done first:

   ```powershell
   gh run download <run id> --repo Aureliolo/steamgauge --name package-managers --dir package-managers
   wingetcreate submit --prtitle "New package: Aureliolo.SteamGauge version X.Y.Z" package-managers/winget
   ```

   `wingetcreate` signs in to GitHub the first time and opens the pull request from your fork of
   `winget-pkgs`. The run keeps the files for 30 days. After that, write them again in a checkout
   of the tag:

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
3. Make a **classic** token with only the `public_repo` scope and an expiry, since no
   fine-grained token can open that pull request. Make it on an account used for nothing else,
   with its own fork of `microsoft/winget-pkgs` and no access to this repository.
4. Create an environment named `winget`, limited to tags matching `v*`, with the token in its
   secret `WINGET_TOKEN`.
5. Set the repository variable `WINGET` to `submit`.

From then on every release submits itself. When the token expires the job fails, and a new one is
made the same way.

## Windows hardening

Windows builds use Control Flow Guard, hardware shadow stack compatibility and Spectre mitigations
for compiled C (`.cargo/config.toml`, the app's `build.rs`). BinSkim (`binaries.yml`) reads the
Windows and Linux programs on every pull request and refuses a missing mitigation. Its accepted
findings carry their reasons there.

## Versions that cannot be released

A version whose tag exists is spent, with or without a release, because the ruleset on `v*`
allows no deletion or update. Prepare release refuses such a version before it writes a branch.

## When a release job fails

Re-run the release workflow on the tag first. Publish passes when the tag already carries exactly
the fifteen files, and package managers (main) picks its pull request up where it is. **tag
release** can be run by hand to start the release workflow again on an existing tag.

- **Tag does not match the workspace version**: the tag was made outside `release-tag.yml`. Leave
  it standing and prepare the version that should ship.
- **vX.Y.Z is tagged at another commit**: that version is spent. Raise the version past it.
- **X sets its own version** or **Cargo.lock records X**: a crate stopped taking the workspace
  version, or the lock file lagged. Fix it on `main` and prepare the next version.
- **Release commit is not reachable from main**: the tag points at a commit that never landed.
- **Release commit carries no valid signature**: the signed-commits ruleset was bypassed or
  removed.
- **Tests or build steps modified the checked-out release source** or **The build modified** it:
  something writes into the tree. The archive would not match the tag.
- **Packaging modified the checked-out release source**: Tauri's bundler reformats the app's
  `Cargo.toml`, such as `name ={`. Fix the manifest on `main` and prepare the next version.
- **The SBOM's files are not the members of ...**, **The SBOM's crates are not the ones ...** or
  **The SBOM for ... does not list ...**: syft read the wrong input or changed with an upgrade.
  Fix the sbom job and re-run on the tag.
- **Cargo.lock does not hold: ...**: `cargo tree` resolved a crate the lock file lacks. Look
  before re-running.
- **ort-sys ... links ONNX Runtime ...**, **These crates ship no licence text ...**, **cargo-about
  could not read every crate's licence ...** or **cargo-about refused the crates ...**: the
  message says what to change. Fix it on `main` and prepare the next version.
- **The notices in ... do not cover what it carries**: the archive holds a crate, file or
  `DirectML.dll` its notices miss. A new runtime library needs its licence in `third-party/` and
  `notices.mjs`.
- **... imports ..., which is neither beside it nor part of Windows** or **... does not carry
  every DLL its program needs**: a binary imports a DLL a bare Windows lacks. Ship it with its
  licence, or add a Windows DLL to `WINDOWS` in `tools/release/runtime-dlls.mjs`.
- **Expected exactly one file matching ...**: a tauri-cli upgrade changed the installer names.
  Fix `tools/release/package.sh` on `main` and prepare the next version.
- **... holds files the packages give no place** or **nFPM wrote no ...**: fix
  `tools/release/linux/nfpm.yaml` or `tools/release/linux-packages.sh` on `main` and prepare the
  next version.
- **lintian** or **rpmlint** failing: fix the package. An override goes in `tools/release/linux/`
  only with its reason.
- **... --version said ...**, **The installer put no ... in ...**, **The window exited within 20
  seconds of starting** or **Uninstalling left ... behind**, in an install job: an installer does
  not work on its system. Nothing was signed or published.
- **vX.Y.Z already has a release, and it carries ...**: an upload failed part way. Look at what
  is attached before deciding anything.
- **... does not hold exactly one SHA-256 for ...**: a file name changed in
  `tools/release/package.sh` and not in `tools/release/package-managers.sh`.
- The same install failures **in a package managers job**: the release is out, but nothing reached
  `main` or `winget-pkgs`. Fix it on `main` and release the next version.
- **Updating it with steamgauge update** failing, or the old version still named, in an update
  check job: the previous release cannot update to this one. Its output names the step, so fix
  it on `main` and release the next version.
- **The packaging App is not set up**: add `PACKAGING_APP_CLIENT_ID` and `PACKAGING_APP_KEY` to
  the `packages` environment and re-run the job.
- **Pull request #N failed: ...**, **... conflicts with main** or **... is OPEN, not merged**: the
  message names the reason. A re-run of the job picks the pull request up where it is.
- **WINGET is set to submit, but the secret WINGET_TOKEN ...** or a refusal from `wingetcreate`:
  make a new token (see [winget](#winget)) and re-run the job.
