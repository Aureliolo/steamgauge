#!/usr/bin/env bash
# Installs what package.sh made the way a person would, then runs the installed program: it has
# to name the version it was built as, and its window has to stay up. A package that installs and
# then fails to start passes every other check in the release.
#
#   tools/release/install-check.sh <target> <version> <dir>
#
# The Linux run checks the .deb on this machine and the .rpm in a Fedora container
# ($FEDORA_IMAGE), so both package formats are installed by their own package manager.
set -euo pipefail

target="$1"
version="$2"
dir="$3"

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
  local said
  echo "Asking $1 for its version."
  said="$(timeout 120 "$@" --version)"
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
  # Git Bash's signals do not reach a native Windows program, so there it is ended by its Windows
  # process id; otherwise the wait below would wait on a window nothing closes.
  if [[ -r "/proc/${pid}/winpid" ]]; then
    taskkill //F //T //PID "$(cat "/proc/${pid}/winpid")" > /dev/null 2>&1 || true
  fi
  kill "${pid}" 2>/dev/null || true
  sleep 2
  kill -9 "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
  echo "The window stayed up."
}

case "${target}" in
  x86_64-pc-windows-msvc)
    step "Installing silently" "${dir}/steamgauge-${version}-windows-x64-setup.exe" /S
    home="$(cygpath -u "${LOCALAPPDATA:?}")/SteamGauge"
    ls -la "${home}"
    for file in steamgauge.exe DirectML.dll LICENSE THIRD-PARTY-NOTICES.txt; do
      test -f "${home}/${file}" || { echo "The installer put no ${file} in ${home}." >&2; exit 1; }
    done
    # A windowed program writes to a pipe it is given, which is what the command substitution
    # gives it.
    expect_version "${home}/steamgauge.exe"
    stays_up "${home}/steamgauge.exe"
    taskkill //F //IM steamgauge.exe > /dev/null 2>&1 || true
    step "Uninstalling silently" "${home}/uninstall.exe" /S
    sleep 5
    if [[ -e "${home}/steamgauge.exe" ]]; then
      echo "Uninstalling left ${home}/steamgauge.exe behind." >&2
      exit 1
    fi
    ;;
  aarch64-apple-darwin)
    mount="$(mktemp -d)"
    hdiutil attach -nobrowse -readonly -mountpoint "${mount}" "${dir}/steamgauge-${version}-macos-arm64.dmg"
    mkdir -p "${HOME}/Applications"
    cp -R "${mount}/SteamGauge.app" "${HOME}/Applications/"
    hdiutil detach "${mount}"
    app="${HOME}/Applications/SteamGauge.app"
    for file in LICENSE THIRD-PARTY-NOTICES.txt; do
      test -f "${app}/Contents/Resources/${file}" || { echo "The app carries no ${file}." >&2; exit 1; }
    done
    expect_version "${app}/Contents/MacOS/steamgauge"
    stays_up "${app}/Contents/MacOS/steamgauge"
    ;;
  x86_64-unknown-linux-gnu)
    deb="${dir}/steamgauge_${version}_amd64.deb"
    sudo apt-get install -y "./${deb}"
    expect_version /usr/bin/steamgauge
    stays_up xvfb-run --auto-servernum /usr/bin/steamgauge
    # Stopping xvfb-run leaves the program it started running.
    pkill -x steamgauge || true
    # Removed by the name the package declares, which the bundler takes from the product name.
    package="$(dpkg-deb --field "${deb}" Package)"
    sudo apt-get remove -y "${package}"
    if [[ -e /usr/bin/steamgauge ]]; then
      echo "Removing ${package} left /usr/bin/steamgauge behind." >&2
      exit 1
    fi
    docker run --rm -v "${PWD}/${dir}:/packages:ro" "${FEDORA_IMAGE:?}" bash -euo pipefail -c "
      dnf install -y /packages/steamgauge-${version}-1.x86_64.rpm > /dev/null
      said=\"\$(steamgauge --version)\"
      echo \"\${said}\"
      test \"\${said}\" = 'steamgauge ${version}'
    "
    ;;
  *)
    echo "No install check is defined for ${target}." >&2
    exit 1
    ;;
esac
