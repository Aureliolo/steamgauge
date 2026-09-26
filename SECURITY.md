# Security

## Reporting a vulnerability

Report privately through GitHub's [security advisory
form](https://github.com/Aureliolo/steamgauge/security/advisories/new). Please do
not open a public issue for a vulnerability.

There is no release yet, so there is nothing deployed to attack and no supported version to
patch. Reports about the build and release pipeline are in scope and welcome.

## Releases

Every release is built by GitHub Actions on GitHub-hosted runners, from a signed commit on
`main`, with `--locked` dependencies and no build cache, by `release-build.yml`. Nothing is built
or signed on a developer machine. Each release carries an archive per platform, an SPDX SBOM of
each archive, and `SHA256SUMS` over all of them, and two kinds of Sigstore attestation: build
provenance over every file, and each SBOM bound to its archive. The build and the signing both
run in that one reusable workflow, which is SLSA Build Level 3, and the attestations are
attached to the release as `steamgauge-<version>.intoto.jsonl` as well as stored on the
repository, so they verify from the file alone. Releases are immutable and the `v*` tags cannot
be moved or deleted. Each archive also carries `THIRD-PARTY-NOTICES.txt`, the licence of every
crate compiled into the binary and of ONNX Runtime and, on Windows, DirectML, checked at release
against the crates that build resolved and the files the archive holds.

Verify an archive before running it:

```sh
signer=Aureliolo/steamgauge/.github/workflows/release-build.yml
gh attestation verify steamgauge-<version>-<target>.tar.gz --repo Aureliolo/steamgauge --signer-workflow "$signer" --source-ref refs/tags/v<version> --deny-self-hosted-runners
gh attestation verify steamgauge-<version>-<target>.tar.gz --repo Aureliolo/steamgauge --signer-workflow "$signer" --predicate-type https://spdx.dev/Document/v2.3
gh attestation verify steamgauge-<version>-<target>.tar.gz --repo Aureliolo/steamgauge --signer-workflow "$signer" --bundle steamgauge-<version>.intoto.jsonl
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

So every build will warn on first launch. Windows will show a SmartScreen prompt you can
click past; macOS will refuse outright until you allow the app under System Settings,
Privacy and Security. That is expected, and it is not a signal that the download has been
tampered with. Verify provenance with the commands above rather than relying on the
operating system's opinion, which here reflects only that nobody has paid a certificate
authority.

Windows builds are to carry a SignPath Foundation signature, which chains to a CA Windows
trusts, once the project is enrolled; until then they carry provenance only. macOS has no free
certificate authority, so its builds will carry provenance only for good.

## Handling of credentials and data

The tool stores review corpora locally and never transmits them anywhere except, when a
hosted model is explicitly configured, the sampled subset sent to that provider for
labelling and summarisation. API keys are read from the environment or the local
configuration directory and are never written into a corpus, a log or a crash report.
