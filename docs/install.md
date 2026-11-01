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

```powershell
# Windows: the portable program, with a Start menu shortcut
scoop bucket add aureliolo https://github.com/Aureliolo/steamgauge
scoop install aureliolo/steamgauge
```

```bash
# macOS
brew tap aureliolo/steamgauge https://github.com/Aureliolo/steamgauge
brew install --cask aureliolo/steamgauge/steamgauge
```

Scoop and Homebrew also put `steamgauge` on the `PATH` for the [command line](commands.md).

## What each system needs

- **Windows** 10 or 11 on x64. The setup installs for your user without administrator rights and
  adds the WebView2 runtime where it is missing.
- **macOS** on Apple silicon. Open the `.dmg` and drag SteamGauge into Applications.
- **Linux** on x86-64 with glibc 2.39 or later (Ubuntu 24.04, Debian 13, Fedora 40). For the
  archive, install WebKitGTK 4.1: `libwebkit2gtk-4.1-0` (Debian, Ubuntu) or `webkit2gtk4.1`
  (Fedora).

Reading runs on any DirectX 12 graphics card on Windows, through Core ML on macOS, and on the
processor on Linux or without a card.

## The first launch

The installers are not signed by Microsoft or Apple, so each system warns once.
[SECURITY.md](../SECURITY.md#what-signing-does-and-does-not-tell-you) says what that means.

- **Windows** shows "Windows protected your PC". Choose **More info**, then **Run anyway**.
- **macOS** says it could not verify SteamGauge. Choose **Done**, then **Open Anyway** in System
  Settings, Privacy and Security.
- **macOS** may call the app damaged instead. Check the download, then run
  `xattr -dr com.apple.quarantine /Applications/SteamGauge.app`.

## Checking a download

Every file is listed in `SHA256SUMS` and carries signed build provenance.
[SECURITY.md](../SECURITY.md#releases) has the commands that check both.

## Updating

SteamGauge asks GitHub for a newer version when it opens and every six hours while it stays open.
When one is out, the window offers **Update now**, which installs it only once its signature
checks out. [SECURITY.md](../SECURITY.md#updates-from-the-window) lists the checks.
`steamgauge update` does the same from a terminal.

| Installed with | Updated by |
|---|---|
| The Windows setup | **Update now** |
| The macOS app, Homebrew | **Update now**, or `brew upgrade --cask --greedy steamgauge` |
| The `.deb` or `.rpm` | **Update now**, which asks for your password |
| Scoop | `scoop update steamgauge` |
| A portable archive | **Update now** on Linux. On Windows and macOS, put the new archive's program in place of the old |

Your library, settings and models stay where they are. Settings turns the check off.
