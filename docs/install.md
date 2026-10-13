# Install

Every [release](https://github.com/Aureliolo/steamgauge/releases/latest) has an installer and a
portable archive for each system.

| System | Installer | Portable archive |
|---|---|---|
| Windows 10 or 11, x64 | `steamgauge-<version>-windows-x64-setup.exe` | `steamgauge-<version>-x86_64-pc-windows-msvc.zip` |
| macOS, Apple silicon | `steamgauge-<version>-macos-arm64.dmg` | `steamgauge-<version>-aarch64-apple-darwin.tar.gz` |
| Debian, Ubuntu, x86-64 | `steamgauge_<version>-1_amd64.deb` | `steamgauge-<version>-x86_64-linux-gnu.tar.gz` |
| Fedora, x86-64 | `steamgauge-<version>-1.x86_64.rpm` | `steamgauge-<version>-x86_64-linux-gnu.tar.gz` |

## With a package manager

Each installs a release's file only if it matches the SHA-256 the release is signed over.

```powershell
# Windows: the installer
winget install Aureliolo.SteamGauge
# Windows: the portable program, with a Start menu shortcut
scoop bucket add aureliolo https://github.com/Aureliolo/steamgauge
scoop install aureliolo/steamgauge
```

```bash
# macOS
brew tap aureliolo/steamgauge https://github.com/Aureliolo/steamgauge
brew install --cask aureliolo/steamgauge/steamgauge
```

Scoop and Homebrew also put `steamgauge` on the `PATH` for the
[command line](commands.md).

## What each system needs

- **Windows**: 10 or 11 on x64. The setup installs for your user alone, without administrator
  rights, and adds Microsoft's WebView2 runtime where it is missing. Reading runs on any graphics
  card with DirectX 12, or on the processor.
- **macOS**: Apple silicon. Open the `.dmg` and drag SteamGauge into Applications. Reading runs
  through Core ML.
- **Linux**: x86-64 with glibc 2.39 or later (Ubuntu 24.04, Debian 13, Fedora 40) and
  WebKitGTK 4.1. The `.deb` and `.rpm` bring WebKitGTK with them; for the archive, install
  `libwebkit2gtk-4.1-0` (Debian, Ubuntu) or `webkit2gtk4.1` (Fedora). Reading runs on the
  processor.

## The first launch

The installers are not signed by Microsoft or Apple, so each system warns once.
[SECURITY.md](../SECURITY.md#what-signing-does-and-does-not-tell-you) says what such a signature
would and would not prove.

- **Windows** shows "Windows protected your PC". Choose **More info**, then **Run anyway**.
- **macOS** says it could not verify SteamGauge. Choose **Done**, then in System Settings,
  Privacy and Security, choose **Open Anyway**. If macOS calls the app damaged instead, check the
  download, then run `xattr -dr com.apple.quarantine /Applications/SteamGauge.app`.

## Checking a download

Every file is listed in `SHA256SUMS` and carries Sigstore build provenance naming the workflow
and commit that built it. [SECURITY.md](../SECURITY.md#releases) has the commands that check both.

## Updating

When a newer version is out, the window names it and offers **Update now**. It installs the new
release's file only once its signed provenance shows SteamGauge's release workflow built it, for
that version's tag, with the SHA-256 it arrived with; anything else stops the update and offers
the release page. [SECURITY.md](../SECURITY.md#updates-from-the-window) lists every check.
`steamgauge update` does the same from a terminal.

| Installed with | Updated by |
|---|---|
| The Windows setup, winget | **Update now**, or `winget upgrade Aureliolo.SteamGauge` |
| The macOS app, Homebrew | **Update now**, or `brew upgrade --cask --greedy steamgauge` |
| The `.deb` or `.rpm` | **Update now**, which asks for your password |
| Scoop | `scoop update steamgauge` |
| A portable archive | **Update now** on Linux; on Windows and macOS, the new archive's program in place of the old |

Your library, settings and downloaded models stay where they are. Settings turns the daily check
for a newer version off.
