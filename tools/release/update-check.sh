#!/usr/bin/env bash
# Installs the release before this one the way a person installs it, then updates it with
# `steamgauge update`, which runs what Update now runs in the window: it fetches this release's
# file from the release download host, verifies it against this release's build provenance, and
# installs it. Afterwards this release has to be what is installed, and has to name itself. It
# runs once the release is published, because the update fetches only what that host serves.
#
#   tools/release/update-check.sh <kind> <previous> <version> <dir>
#
# <kind> is windows-setup, macos-app, linux-deb or linux-archive. <dir> holds the previous
# release's SHA256SUMS and the file that kind installs, as the release published them.

# A command whose output is compared is checked by the comparison: if it fails, what it printed
# is not what was expected.
# shellcheck disable=SC2312
set -euo pipefail

kind="$1"
previous="$2"
version="$3"
dir="$4"

same() {
  if [[ "$2" != "$3" ]]; then
    echo "$1 is '$2', not '$3'." >&2
    exit 1
  fi
  echo "$1: $2"
}

# Each step says what it is about to do and has four minutes to do it, so a step that waits on
# something no one will ever answer, a prompt on a desktop nobody sees, fails as itself rather
# than as the job's timeout.
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
    kill "${pid}" 2>/dev/null || true
    echo "::endgroup::"
    echo "${what} was still running after four minutes." >&2
    exit 1
  fi
  wait "${pid}"
  echo "::endgroup::"
}

# Sets `said` to what a copy answers to --version. Run in place rather than inside `$(...)`,
# which would take the step's log lines for part of the answer.
said_version() {
  local answer
  answer="$(mktemp)"
  # The inner shell expands its own arguments, handed to it after the script.
  # shellcheck disable=SC2016
  step "Asking $1 for its version" bash -c '"$1" --version > "$2"' _ "$1" "${answer}"
  said="$(cat "${answer}")"
}

# The file, refused unless it is the one the previous release's signed SHA256SUMS names.
verified() {
  local want got
  want="$(awk -v name="$1" '$2 == name { print $1 }' "${dir}/SHA256SUMS")"
  # Read from standard input: given a name with a backslash in it, as every Windows path has,
  # sha256sum escapes the name and starts its line with a backslash of its own.
  if command -v sha256sum > /dev/null; then
    got="$(sha256sum < "${dir}/$1" | cut -d ' ' -f 1)"
  else
    got="$(shasum -a 256 < "${dir}/$1" | cut -d ' ' -f 1)"
  fi
  if [[ -z "${want}" || "${got}" != "${want}" ]]; then
    echo "$1 is not the file the ${previous} release's SHA256SUMS names." >&2
    exit 1
  fi
  echo "${dir}/$1"
}

updated() {
  said_version "$1"
  same "What the installed copy says before the update" "${said}" "steamgauge ${previous}"
  step "Updating it with steamgauge update" "$1" update
}

case "${kind}" in
  windows-setup)
    setup="$(verified "steamgauge-${previous}-windows-x64-setup.exe")"
    # `//S` reaches the setup program as `/S`: Git Bash takes a lone `/S` for a path.
    step "Installing ${previous} silently" "${setup}" //S
    program="$(cygpath -u "${LOCALAPPDATA:?}")/SteamGauge/steamgauge.exe"
    updated "${program}"
    # The setup program the update started installs once that copy has closed, passively and
    # opening nothing; its entry in Apps names the version it installed when it is done.
    entry='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\SteamGauge'
    installed=""
    for _ in $(seq 120); do
      installed="$(powershell -NoProfile -Command "(Get-ItemProperty '${entry}').DisplayVersion" | tr -d '\r')"
      [[ "${installed}" == "${version}" ]] && break
      sleep 2
    done
    same "The version Apps names" "${installed}" "${version}"
    for _ in $(seq 60); do
      tasklist //FI "IMAGENAME eq steamgauge-${version}-windows-x64-setup.exe" | grep -qi setup || break
      sleep 2
    done
    said_version "${program}"
    same "What the installed copy says after the update" "${said}" "steamgauge ${version}"
    ;;
  macos-app)
    image="$(verified "steamgauge-${previous}-macos-arm64.dmg")"
    mount="$(mktemp -d)"
    hdiutil attach -nobrowse -readonly -mountpoint "${mount}" "${image}"
    cp -R "${mount}/SteamGauge.app" /Applications/
    hdiutil detach "${mount}"
    app=/Applications/SteamGauge.app
    updated "${app}/Contents/MacOS/steamgauge"
    same "The app's CFBundleShortVersionString" \
      "$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "${app}/Contents/Info.plist")" "${version}"
    said_version "${app}/Contents/MacOS/steamgauge"
    same "What the installed app says after the update" "${said}" "steamgauge ${version}"
    codesign --verify --deep --strict "${app}"
    same "What the update left in Applications" "$(find /Applications -maxdepth 1 -iname '*steamgauge*' | sort)" "${app}"
    if xattr -p com.apple.quarantine "${app}" > /dev/null 2>&1; then
      echo "The updated app is quarantined, so macOS would ask before opening it." >&2
      exit 1
    fi
    ;;
  linux-deb)
    package="$(verified "steamgauge_${previous}-1_amd64.deb")"
    # The runner's package lists are as old as its image, and a mirror drops a version once a
    # newer one replaces it, so the lists are read again before anything is installed.
    sudo apt-get update
    sudo apt-get install -y "${package}"
    # The update installs the package through pkexec, which on a desktop asks for a password;
    # here a polkit rule answers yes for this account alone, so everything else is as it is there.
    sudo apt-get install -y --no-install-recommends pkexec
    rule="polkit.addRule(function (action, subject) {
  if (action.id == \"org.freedesktop.policykit.exec\" && subject.user == \"$(id -un)\") {
    return polkit.Result.YES;
  }
});"
    echo "${rule}" | sudo tee /etc/polkit-1/rules.d/49-update-check.rules > /dev/null
    sudo systemctl restart polkit
    updated /usr/bin/steamgauge
    same "The version dpkg names" "$(dpkg-query --show --showformat '${Version}' steamgauge)" "${version}-1"
    said_version /usr/bin/steamgauge
    same "What the installed copy says after the update" "${said}" "steamgauge ${version}"
    ;;
  linux-archive)
    archive="$(verified "steamgauge-${previous}-x86_64-linux-gnu.tar.gz")"
    # What the .deb names as its dependencies, which a person running the archive has installed.
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends libwebkit2gtk-4.1-0 libgtk-3-0
    apps="$(mktemp -d)"
    tar -xzf "${archive}" -C "${apps}"
    folder="${apps}/steamgauge-${previous}-x86_64-linux-gnu"
    updated "${folder}/steamgauge"
    said_version "${folder}/steamgauge"
    same "What the unpacked copy says after the update" "${said}" "steamgauge ${version}"
    same "What the update left beside it" "$(ls -A "${apps}")" "steamgauge-${previous}-x86_64-linux-gnu"
    ;;
  *)
    echo "No update is checked for '${kind}'." >&2
    exit 1
    ;;
esac
