# Security

## Supported versions

Only the latest release gets fixes. A fix ships as a new release, which the app offers to
install ([Updates from the window](#updates-from-the-window)).

## Reporting a vulnerability

Report privately through GitHub's [security advisory
form](https://github.com/Aureliolo/steamgauge/security/advisories/new), not in a public issue.
You get an answer within a week. Reports about the build and release pipeline are in scope.

## Releases

- **Where it is built.** `release-build.yml` builds every release on GitHub-hosted runners, from
  a signed commit on `main`, with `--locked` dependencies and no build cache.
- **SLSA Build Level 3**: the build and the signing run in that one reusable workflow, never on a
  developer machine.
- **Every file carries Sigstore build provenance**, and each SPDX SBOM is bound to the file it
  describes. Both are attached as `steamgauge-<version>.intoto.jsonl`.
- **Immutable.** A published release cannot be edited, and the `v*` tags cannot be moved or
  deleted.
- **Package managers.** Homebrew and Scoop install a file only if it matches the `SHA256SUMS` the
  release is signed over.

[What a release carries](.github/release-process.md#what-a-release-carries) lists every file.
Verify a download before running it, with `file` set to the name of any file in the release:

```sh
signer=Aureliolo/steamgauge/.github/workflows/release-build.yml
file=steamgauge-<version>-windows-x64-setup.exe
gh attestation verify "$file" --repo Aureliolo/steamgauge --signer-workflow "$signer" --source-ref refs/tags/v<version> --deny-self-hosted-runners
gh attestation verify "$file" --repo Aureliolo/steamgauge --signer-workflow "$signer" --predicate-type https://spdx.dev/Document/v2.3
gh attestation verify "$file" --repo Aureliolo/steamgauge --signer-workflow "$signer" --bundle steamgauge-<version>.intoto.jsonl
sha256sum --check --ignore-missing SHA256SUMS
```

## What signing does and does not tell you

- **Provenance** (Sigstore attestations) proves a file was built by this repository's workflow
  from a given commit. Every release carries it.
- **Operating-system trust** (a certificate chaining to a CA the system ships) is the only thing
  that silences SmartScreen and Gatekeeper. No build carries it.

So every build warns on first launch ([The first launch](docs/install.md#the-first-launch)).
The warning says only that no certificate authority vouches for the file, not that it was
tampered with. The commands above check that. The macOS app is sealed ad hoc, so
`codesign --verify --deep --strict` shows no file in it changed since the build.

## Updates from the window

**Update now**, and `steamgauge update`, download the release's file for this computer and hash
it as it is written. It is installed only when all of these hold:

- the version is newer than the one running
- the provenance's certificate chains to Sigstore's Fulcio, with its certificate transparency
  timestamp, and its signature is in Sigstore's transparency log, Rekor
- GitHub Actions (`https://token.actions.githubusercontent.com`) issued the certificate to
  `https://github.com/Aureliolo/steamgauge/.github/workflows/release-build.yml@refs/tags/v<version>`
  on a GitHub-hosted runner
- the certificate names the repository and owner by GitHub's numeric ids (1361019156 and
  19254254), which survive a rename and are never reused
- the signed statement is SLSA provenance (`https://slsa.dev/provenance/v1`) naming the file by
  its release name with the SHA-256 it arrived with

The tag in the certificate has to be the version being installed, so an older release signed for
its own tag is refused. Sigstore's trusted root comes from its TUF repository, starting from the
root built into the app. Any failed check installs nothing, says why, and offers the release page.

On Windows the file stays open, readable by others but not writable, from the moment it is hashed
to the moment the setup program starts. On macOS the app is unpacked beside the installed one, on
the same disk, and the two are exchanged in one step. On Linux the package manager installs the
verified package, and a portable archive's files are renamed into place.

This protects against a download changed on the way or in storage, TLS interception, and a
release published with a stolen token. A takeover of the owner's GitHub account or a compromised
build would still produce a correctly signed release.

## Steering from other programs

- **A client is trusted as you are.** `steamgauge mcp` can do anything the window can, and a
  program that can start it can already run the app as you.
- **`steamgauge connect`** writes only the app's entry into the client's file, and leaves a file
  that does not parse alone.
- **No network port by default.** The app listens on a randomly named pipe on Windows, or a socket
  in a folder only you may enter on macOS and Linux.
- **HTTP is opt-in**, on 127.0.0.1 at a chosen port. A request needs the AI assistants page's
  256-bit token, and is refused with an `Origin` header or a `Host` other than `127.0.0.1` or
  `localhost`.
- **Reviews are untrusted text.** A client that takes text in a tool's answer as instructions can
  be steered by a review.

[Privacy](docs/privacy.md) lists every place the app connects to.
