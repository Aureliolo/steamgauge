#!/usr/bin/env bash
# Installs what package.sh made the way a person would, then runs the installed program: it has
# to name the version it was built as, and its window has to stay up. A package that installs and
# then fails to start passes every other check in the release. Each installer also has to say
# who makes it, under which name and licence, exactly as written below, and on Windows and Linux
# it has to replace a copy of 0.1.2, which was packaged and published under other names.
#
#   tools/release/install-check.sh <target> <version> <dir>
#
# The Linux run lints the .deb with lintian in a Debian container ($DEBIAN_IMAGE) and installs it
# on this machine, and lints and installs the .rpm in a Fedora container ($FEDORA_IMAGE, by
# install-check-fedora.sh), so both package formats are linted and installed by their own tools.

# A command whose output is compared is checked by the comparison: if it fails, what it printed
# is not what was expected.
# shellcheck disable=SC2312
set -euo pipefail

target="$1"
version="$2"
dir="$3"

VERSION="${version}"
# shellcheck source=tools/release/names.sh
source "$(dirname "${BASH_SOURCE[0]}")/names.sh"
listed="$(installers "${target}")"
read -r -a installer_names <<< "${listed}"

maintainer="Aurelio Amoroso <19254254+Aureliolo@users.noreply.github.com>"
publisher="Aurelio Amoroso"
copyright="Copyright (c) 2026 Aurelio Amoroso"
homepage="https://github.com/Aureliolo/steamgauge"

# The last release whose packages are named steam-gauge and whose Windows setup registers the
# publisher "Aurelio", by the digests in its signed SHA256SUMS.
previous=0.1.2
previous_url="${homepage}/releases/download/v${previous}"
previous_setup="steamgauge-${previous}-windows-x64-setup.exe"
previous_setup_sha256=63f0551f6fc2945d9e47cab7dfa3016314996ed21d21af4dd2c77aae0cc4c44b
previous_deb="steamgauge_${previous}_amd64.deb"
previous_deb_sha256=d204b1aebbbe1fd42f5ad2dae442281a44237569dd9870c03144856ffc85ae97
previous_rpm="steamgauge-${previous}-1.x86_64.rpm"
previous_rpm_sha256=21a4b3c90475a294349e56002fe0628db1d34d0021fd5baaec12dc7fa5152bb3

fetch_previous() {
  curl -fsSL --proto '=https' --tlsv1.2 --retry 6 --retry-all-errors -o "$3/$1" "${previous_url}/$1"
  echo "$2  $3/$1" | sha256sum --check --strict
}

sorted_words() {
  printf '%s\n' "$@" | LC_ALL=C sort | tr '\n' ' '
}

same() {
  if [[ "$2" != "$3" ]]; then
    echo "$1 is '$2', not '$3'." >&2
    exit 1
  fi
  echo "$1: $2"
}

# Each step says what it is about to do and has four minutes to do it, so a step that waits on
# something no one will ever answer fails as itself rather than as the job's timeout. A Windows
# setup program that waits does so on a window nobody can see on this machine, so what is open
# then, and which process holds it, is printed before giving up.
step() {
  local what="$1"
  shift
  echo "::group::${what}"
  "$@" &
  local pid=$!
  local waited=0
  while kill -0 "${pid}" 2>/dev/null && [[ "${waited}" -lt 240 ]]; do
    sleep 2
    waited=$((waited + 2))
  done
  if kill -0 "${pid}" 2>/dev/null; then
    echo "${what}: still running after four minutes." >&2
    if command -v powershell > /dev/null; then
      powershell -NoProfile -Command \
        "Get-Process | Where-Object MainWindowTitle | Format-Table Id, ProcessName, MainWindowTitle -AutoSize | Out-String -Width 300; Get-CimInstance Win32_Process | Where-Object { \$_.CreationDate -gt (Get-Date).AddMinutes(-6) } | Format-Table ProcessId, ParentProcessId, Name, CommandLine -AutoSize | Out-String -Width 400" >&2
    fi
    exit 1
  fi
  wait "${pid}"
  echo "::endgroup::"
}

expect_version() {
  local said answer
  answer="$(mktemp)"
  step "Asking $1 for its version" bash -c '"$@" --version > "'"${answer}"'"' _ "$@"
  said="$(cat "${answer}")"
  if [[ "${said}" != "steamgauge ${version}" ]]; then
    echo "$* --version said '${said}', not 'steamgauge ${version}'." >&2
    exit 1
  fi
  echo "${said}"
}

# The window is the program most people run, and a missing library or a broken asset shows up
# only when it starts. Up for a while and not exited is as much as a machine with no one at it
# can tell.
stays_up() {
  echo "Starting the window: $*"
  "$@" &
  local pid=$!
  sleep 20
  if ! kill -0 "${pid}" 2>/dev/null; then
    wait "${pid}" || true
    echo "The window exited within 20 seconds of starting." >&2
    exit 1
  fi
  # On Windows the window is ended with its whole tree, the WebView2 processes it started included,
  # which a signal to the program alone leaves running.
  if [[ -r "/proc/${pid}/winpid" ]]; then
    taskkill //F //T //PID "$(cat "/proc/${pid}/winpid")" > /dev/null 2>&1 || true
  fi
  kill "${pid}" 2>/dev/null || true
  sleep 2
  kill -9 "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
  echo "The window stayed up."
}

# One value PowerShell reads, without the carriage return it ends its output with.
powershell_value() {
  local said
  said="$(powershell -NoProfile -Command "$1")"
  printf '%s' "${said%$'\r'}"
}

# The details Windows shows for a program under Properties, from its version resource.
version_details() {
  local file pair
  file="$(cygpath -w "$1")"
  shift
  for pair in "$@"; do
    same "$(basename "${file}")'s ${pair%%=*}" \
      "$(powershell_value "(Get-Item -LiteralPath '${file}').VersionInfo.${pair%%=*}")" "${pair#*=}"
  done
}

case "${target}" in
  x86_64-pc-windows-msvc)
    setup="${dir}/${installer_names[0]}"
    home="$(cygpath -u "${LOCALAPPDATA:?}")/SteamGauge"
    folder="$(cygpath -w "${home}")"
    version_details "${setup}" "ProductName=SteamGauge" "LegalCopyright=${copyright}"
    # `//S` reaches the setup program as `/S`: Git Bash takes a lone `/S` for a path and hands the
    # program `S:/`, which it does not know, so it opens its wizard and waits on it.
    previous_dir="$(mktemp -d)"
    fetch_previous "${previous_setup}" "${previous_setup_sha256}" "${previous_dir}"
    step "Installing ${previous} silently" "${previous_dir}/${previous_setup}" //S
    same "The folder ${previous} keeps under Software\\Aurelio" \
      "$(powershell_value "(Get-Item 'HKCU:\\Software\\Aurelio\\SteamGauge').GetValue('')")" "${folder}"
    # A person upgrading opens the setup program's window. Past the welcome page it goes straight
    # to where to install: Tauri's own script asks there whether to uninstall first, by default
    # yes, and that runs the old uninstaller, whose page offers to delete the library.
    echo "::group::Pressing Next in the setup program over ${previous}"
    upgrade_page="$(powershell -NoProfile -File "$(cygpath -w tools/release/setup-next-page.ps1)" -Setup "$(cygpath -w "${setup}")" | tr -d '\r')"
    echo "${upgrade_page}"
    echo "::endgroup::"
    if grep -qi "uninstall" <<<"${upgrade_page}"; then
      echo "Upgrading ${previous} from the setup program's window offers to uninstall it first." >&2
      exit 1
    fi
    if ! grep -q "Choose Install Location" <<<"${upgrade_page}"; then
      echo "Past its welcome page, the setup program over ${previous} does not ask where to install." >&2
      exit 1
    fi
    step "Installing silently over it" "${setup}" //S
    ls -la "${home}"
    for file in steamgauge.exe DirectML.dll LICENSE THIRD-PARTY-NOTICES.txt; do
      test -f "${home}/${file}" || { echo "The installer put no ${file} in ${home}." >&2; exit 1; }
    done
    version_details "${home}/steamgauge.exe" "CompanyName=${publisher}" "FileDescription=SteamGauge" \
      "InternalName=steamgauge" "LegalCopyright=${copyright}" "OriginalFilename=steamgauge.exe" \
      "ProductName=SteamGauge" "ProductVersion=${version}"
    # A windowed program writes to a pipe it is given, which is what the command substitution
    # gives it.
    expect_version "${home}/steamgauge.exe"
    stays_up "${home}/steamgauge.exe"
    taskkill //F //IM steamgauge.exe > /dev/null 2>&1 || true
    # What Apps and winget know the installed copy by, and the folder the next setup program
    # replaces it from, kept under the publisher's name with nothing left under 0.1.2's.
    entry='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\SteamGauge'
    same "The entry in Apps' DisplayName" "$(powershell_value "(Get-ItemProperty '${entry}').DisplayName")" SteamGauge
    same "The entry in Apps' Publisher" "$(powershell_value "(Get-ItemProperty '${entry}').Publisher")" "${publisher}"
    same "The folder kept under Software\\${publisher}" \
      "$(powershell_value "(Get-Item 'HKCU:\\Software\\${publisher}\\SteamGauge').GetValue('')")" "${folder}"
    same "Whether Software\\Aurelio\\SteamGauge is left" \
      "$(powershell_value "Test-Path 'HKCU:\\Software\\Aurelio\\SteamGauge'")" False
    # A setup program run over an install whose program cannot be written for a while, as when a
    # scanner reads a program just written. Unwritable briefly, the setup waits and replaces it;
    # past the setup's minute of patience, it stops with an error and copies nothing, rather than
    # leaving new libraries beside the old program. The read-only flag stands in for the scanner:
    # it refuses the write the same way, and the Restart Manager, which Tauri's setup asks to close
    # whatever holds the program, cannot clear it. The installed program is marked first, so
    # whether it was replaced can be told from its bytes.
    program="${home}/steamgauge.exe"
    shipped="$(sha256sum "${program}" | cut -d' ' -f1)"
    hold() {
      attrib +R "$(cygpath -w "${program}")"
      (sleep "$1" && attrib -R "$(cygpath -w "${program}")") &
      holder=$!
    }
    printf 'marked' >> "${program}"
    hold 10
    step "Installing silently over a program unwritable for ten seconds" "${setup}" //S
    wait "${holder}"
    if [[ "$(sha256sum "${program}" | cut -d' ' -f1)" != "${shipped}" ]]; then
      echo "The setup program left the program it could not write at once unreplaced." >&2
      exit 1
    fi
    echo "The setup program waited for the program to be writable and replaced it."
    printf 'marked' >> "${program}"
    marked="$(sha256sum "${program}" | cut -d' ' -f1)"
    hold 90
    echo "::group::Installing silently over a program unwritable past the setup's patience"
    refused=0
    "${setup}" //S || refused=$?
    echo "::endgroup::"
    wait "${holder}"
    if [[ "${refused}" -eq 0 ]]; then
      echo "The setup program reported success over a program it could not replace." >&2
      exit 1
    fi
    if [[ "$(sha256sum "${program}" | cut -d' ' -f1)" != "${marked}" ]]; then
      echo "The setup program changed the program it was refused, leaving a mixed install." >&2
      exit 1
    fi
    echo "The setup program stopped with exit code ${refused} and left the install as it was."
    step "Installing silently once nothing holds the program" "${setup}" //S
    if [[ "$(sha256sum "${program}" | cut -d' ' -f1)" != "${shipped}" ]]; then
      echo "The setup program did not replace the program once it was free." >&2
      exit 1
    fi
    # The uninstall a quiet uninstaller such as winget runs is the one the setup program
    # registered, so that is the one checked, and then run.
    same "The quiet uninstall the setup program registered" \
      "$(powershell_value "(Get-ItemProperty '${entry}').QuietUninstallString")" "\"${folder}\\uninstall.exe\" /S"
    step "Uninstalling silently" "${home}/uninstall.exe" //S
    # NSIS's uninstaller copies itself to %TEMP% and carries on from there, so it returns before
    # the files are gone.
    for _ in $(seq 60); do
      [[ -e "${home}/steamgauge.exe" ]] || break
      sleep 2
    done
    if [[ -e "${home}/steamgauge.exe" ]]; then
      echo "Uninstalling left ${home}/steamgauge.exe behind." >&2
      exit 1
    fi
    same "Whether uninstalling left its entry in Apps" "$(powershell_value "Test-Path '${entry}'")" False
    ;;
  aarch64-apple-darwin)
    mount="$(mktemp -d)"
    hdiutil attach -nobrowse -readonly -mountpoint "${mount}" "${dir}/${installer_names[0]}"
    mkdir -p "${HOME}/Applications"
    cp -R "${mount}/SteamGauge.app" "${HOME}/Applications/"
    hdiutil detach "${mount}"
    app="${HOME}/Applications/SteamGauge.app"
    for file in LICENSE THIRD-PARTY-NOTICES.txt; do
      test -f "${app}/Contents/Resources/${file}" || { echo "The app carries no ${file}." >&2; exit 1; }
    done
    for pair in "CFBundleName=SteamGauge" "CFBundleIdentifier=com.aureliolo.steamgauge" \
      "CFBundleShortVersionString=${version}" "NSHumanReadableCopyright=${copyright}"; do
      same "The app's ${pair%%=*}" \
        "$(/usr/libexec/PlistBuddy -c "Print :${pair%%=*}" "${app}/Contents/Info.plist")" "${pair#*=}"
    done
    expect_version "${app}/Contents/MacOS/steamgauge"
    stays_up "${app}/Contents/MacOS/steamgauge"
    ;;
  x86_64-unknown-linux-gnu)
    # What both packages install, each adding its own: Debian's copyright file, changelog and
    # lintian's notes, and the RPM's licence where Fedora keeps licences.
    linux_files=(/usr/bin/steamgauge /usr/share/applications/steamgauge.desktop
      /usr/share/doc/steamgauge/README.md /usr/share/doc/steamgauge/THIRD-PARTY-NOTICES.txt
      /usr/share/icons/hicolor/{32x32,128x128,256x256,512x512}/apps/steamgauge.png
      /usr/share/man/man1/steamgauge.1.gz)
    deb="${dir}/${installer_names[0]}"
    dpkg-deb --info "${deb}"
    for pair in "Package=steamgauge" "Version=${version}-1" "Architecture=amd64" \
      "Maintainer=${maintainer}" "Homepage=${homepage}" "Section=utils" "Priority=optional" \
      "Provides=steam-gauge" "Conflicts=steam-gauge" "Replaces=steam-gauge" \
      "Description=Finds out what players of a game think, from every Steam review"; do
      same "The .deb's ${pair%%=*}" "$(dpkg-deb --field "${deb}" "${pair%%=*}" | head -n 1)" "${pair#*=}"
    done
    depends="$(dpkg-deb --field "${deb}" Depends)"
    [[ "${depends}" =~ ^libc6\ \(\>=\ 2\.[0-9]+\),\ libwebkit2gtk-4\.1-0,\ libgtk-3-0$ ]] \
      || { echo "The .deb depends on '${depends}', not the C library, WebKitGTK 4.1 and GTK 3." >&2; exit 1; }
    echo "The .deb's Depends: ${depends}"
    same "The .deb's files" \
      "$(dpkg-deb --contents "${deb}" | awk '$1 ~ /^-/ { print $NF }' | sed 's|^\./|/|; s|^\([^/]\)|/\1|' | LC_ALL=C sort | tr '\n' ' ')" \
      "$(sorted_words "${linux_files[@]}" /usr/share/doc/steamgauge/changelog.Debian.gz \
        /usr/share/doc/steamgauge/copyright /usr/share/lintian/overrides/steamgauge)"
    # Read from the package rather than the system, which may be set to leave documentation out.
    unpacked="$(mktemp -d)"
    dpkg-deb --extract "${deb}" "${unpacked}"
    doc="${unpacked}/usr/share/doc/steamgauge"
    same "The .deb's copyright file's format" "$(head -n 1 "${doc}/copyright")" \
      "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
    grep -qxF "Copyright: 2026 Aurelio Amoroso" "${doc}/copyright" \
      || { echo "The .deb's copyright file does not name Aurelio Amoroso as the holder." >&2; exit 1; }
    grep -qxF "License: Apache-2.0" "${doc}/copyright" \
      || { echo "The .deb's copyright file does not give the licence as Apache-2.0." >&2; exit 1; }
    changelog="$(gzip -dc "${doc}/changelog.Debian.gz")"
    same "The .deb's changelog's entry" "$(head -n 1 <<< "${changelog}")" \
      "steamgauge (${version}-1) unstable; urgency=medium"
    [[ "$(tail -n 1 <<< "${changelog}")" == " -- ${maintainer}  "* ]] \
      || { echo "The .deb's changelog is not signed by ${maintainer}." >&2; exit 1; }
    grep -qxF "Name=SteamGauge" "${unpacked}/usr/share/applications/steamgauge.desktop" \
      || { echo "The .deb's menu entry does not call the program SteamGauge." >&2; exit 1; }

    # Debian's own linter, at Debian's current release, where a warning fails as an error does.
    docker run --rm --volume "${PWD}/${dir}:/packages:ro" "${DEBIAN_IMAGE:?}" bash -euo pipefail -c '
      export DEBIAN_FRONTEND=noninteractive
      apt-get update -qq
      apt-get install --yes --no-install-recommends lintian > /dev/null
      lintian --version
      lintian --fail-on error,warning --display-info "/packages/$1"
    ' _ "${installer_names[0]}"

    previous_dir="$(mktemp -d)"
    fetch_previous "${previous_deb}" "${previous_deb_sha256}" "${previous_dir}"
    sudo apt-get install -y "${previous_dir}/${previous_deb}"
    # shellcheck disable=SC2016 # dpkg-query's own field syntax, not the shell's.
    same "The status of ${previous}'s package" "$(dpkg-query --show --showformat='${db:Status-Status}' steam-gauge)" installed
    sudo apt-get install -y "./${deb}"
    # shellcheck disable=SC2016 # dpkg-query's own field syntax, not the shell's.
    if [[ "$(dpkg-query --show --showformat='${db:Status-Status}' steam-gauge 2>/dev/null || true)" == installed ]]; then
      echo "Installing steamgauge left ${previous}'s steam-gauge installed beside it." >&2
      exit 1
    fi
    sudo dpkg --verify steamgauge
    expect_version /usr/bin/steamgauge
    stays_up xvfb-run --auto-servernum /usr/bin/steamgauge
    # Stopping xvfb-run leaves the program it started running.
    pkill -x steamgauge || true
    sudo apt-get remove -y steamgauge
    if [[ -e /usr/bin/steamgauge ]]; then
      echo "Removing steamgauge left /usr/bin/steamgauge behind." >&2
      exit 1
    fi

    fetch_previous "${previous_rpm}" "${previous_rpm_sha256}" "${previous_dir}"
    here="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
    docker run --rm --volume "${PWD}/${dir}:/packages:ro" --volume "${previous_dir}:/previous:ro" \
      --volume "${here}/install-check-fedora.sh:/install-check-fedora.sh:ro" \
      --volume "${here}/linux/rpmlint.toml:/rpmlint.toml:ro" \
      --env VERSION="${version}" --env RPM="/packages/${installer_names[1]}" \
      --env PREVIOUS="${previous}" --env PREVIOUS_RPM="/previous/${previous_rpm}" \
      --env HOMEPAGE="${homepage}" --env MAINTAINER="${maintainer}" --env VENDOR="${publisher}" \
      --env FILES="$(sorted_words "${linux_files[@]}" /usr/share/licenses/steamgauge/LICENSE)" \
      "${FEDORA_IMAGE:?}" bash /install-check-fedora.sh
    ;;
  *)
    echo "No install check is defined for ${target}." >&2
    exit 1
    ;;
esac
