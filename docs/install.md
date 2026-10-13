# Install

Download SteamGauge from the [latest release](https://github.com/Aureliolo/steamgauge/releases/latest).
Each system has an installer and a portable archive with the program in it:

| System | Installer | Portable archive |
|---|---|---|
| Windows 10 or 11, x64 | `steamgauge-<version>-windows-x64-setup.exe` | `steamgauge-<version>-x86_64-pc-windows-msvc.zip` |
| macOS on Apple Silicon | `steamgauge-<version>-macos-arm64.dmg` | `steamgauge-<version>-aarch64-apple-darwin.tar.gz` |
| Linux on x86-64, Debian and Ubuntu | `steamgauge_<version>-1_amd64.deb` | `steamgauge-<version>-x86_64-linux-gnu.tar.gz` |
| Linux on x86-64, Fedora | `steamgauge-<version>-1.x86_64.rpm` | `steamgauge-<version>-x86_64-linux-gnu.tar.gz` |

Or install it with a package manager. Each takes a release's file only if it matches the hash
the release is signed over.

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

Scoop also takes this repository as a bucket, which `scoop update` then follows:
`scoop bucket add aureliolo https://github.com/Aureliolo/steamgauge`, then
`scoop install aureliolo/steamgauge`. Homebrew and Scoop both put `steamgauge` on the `PATH` for
the pipeline.

**Windows.** Windows 10 or 11 on x64. The setup installs SteamGauge for your user alone, with no
administrator rights, and installs Microsoft's WebView2 runtime where it is missing. Reviews are
read on any graphics card with DirectX 12, and on the processor where there is none. The program
is a windowed application, so it opens no console window. Given arguments from a terminal it
runs the pipeline, and its output reaches the terminal through a pipe: Git Bash makes one by
itself, and in PowerShell you add one, as in `steamgauge --help | Out-Host`.

**macOS.** A Mac with Apple Silicon. Open the `.dmg` and drag SteamGauge into Applications.

**Linux.** x86-64, and reading runs on the processor. The program needs glibc 2.39 or later
(Ubuntu 24.04, Debian 13, Fedora 40 and later) and WebKitGTK 4.1 for its window. The `.deb` and
the `.rpm` declare both, so the package manager installs what is missing with SteamGauge
(`sudo apt install ./steamgauge_<version>-1_amd64.deb`, or
`sudo dnf install ./steamgauge-<version>-1.x86_64.rpm`). The portable archive needs WebKitGTK
installed already: `libwebkit2gtk-4.1-0` on Debian and Ubuntu, `webkit2gtk4.1` on Fedora.

## The first launch

The installers carry no certificate from Microsoft or Apple, so both systems warn the first time
SteamGauge opens. [SECURITY.md](../SECURITY.md#what-signing-does-and-does-not-tell-you) explains
what that certificate does and does not prove.

- **Windows** shows a blue SmartScreen box, "Windows protected your PC", naming an unrecognised
  app. Choose **More info**, then **Run anyway**.
- **macOS** says Apple could not verify that SteamGauge is free of malware, and offers only
  **Done** and **Move to Bin**. Choose **Done**, open System Settings, Privacy and Security,
  and choose **Open Anyway** beside the line about SteamGauge, then confirm with your password.
  Where macOS instead calls the app damaged, it is the quarantine flag on an app Apple has not
  notarised; after verifying the download, `xattr -dr com.apple.quarantine /Applications/SteamGauge.app`
  removes it.

Either way it asks once.

## Verifying a download

Every file in a release is listed in `SHA256SUMS` and carries Sigstore build provenance naming
the workflow and the commit it was built from. [SECURITY.md](../SECURITY.md#releases) has the
commands that check a file against both.

## Upgrading

When a newer version is out, the window names it under the menu and offers **Update now**, which
downloads the new release's file for this computer and installs it:

- **Windows**: the setup program, which installs over this copy without asking anything, keeps
  your shortcuts, and opens the new version when it is done.
- **macOS**: the new app, put in place of the old one in a single step, and opened again.
- **Linux**: the `.deb` or the `.rpm`, installed by the system's package manager once you give
  your password; where no password prompt is available, or it is closed, the window offers the
  downloaded package to install yourself. A portable archive unpacked into a folder of your own
  has its files replaced.

The file is installed only when the release's signed build provenance shows that SteamGauge's
release workflow built it, for that version's tag, under the file's own name and with the
SHA-256 it arrived with. Anything else stops the update with the reason and nothing installed,
and the window offers the release page instead. [SECURITY.md](../SECURITY.md#updates-from-the-window)
lists every check.

`steamgauge update` does the same from a terminal, without opening the window afterwards.

A copy Scoop installed, or a portable archive on Windows or macOS, is updated the way it was put
there: `scoop update steamgauge`, or the new archive's program in place of the old one. winget
and Homebrew install the same setup program and app the window updates, so the window updates
those copies too; `winget upgrade Aureliolo.SteamGauge` and `brew upgrade --cask --greedy
steamgauge` work as well. Settings turns the daily question off, and with it the notice. Your
library, your settings and the models already downloaded stay where they are.
