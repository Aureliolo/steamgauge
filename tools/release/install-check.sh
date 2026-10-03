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

expect_version() {
  local said
  said="$("$@" --version)"
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
  "$@" &
  local pid=$!
  sleep 20
  if ! kill -0 "${pid}" 2>/dev/null; then
    wait "${pid}" || true
    echo "The window exited within 20 seconds of starting." >&2
    exit 1
  fi
  kill "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
  echo "The window stayed up."
}

case "${target}" in
  x86_64-pc-windows-msvc)
    "${dir}/steamgauge-${version}-windows-x64-setup.exe" /S
    home="$(cygpath -u "${LOCALAPPDATA:?}")/SteamGauge"
    for file in steamgauge.exe DirectML.dll LICENSE THIRD-PARTY-NOTICES.txt; do
      test -f "${home}/${file}" || { echo "The installer put no ${file} in ${home}." >&2; exit 1; }
    done
    # A windowed program writes to a pipe it is given, which is what the command substitution
    # gives it.
    expect_version "${home}/steamgauge.exe"
    stays_up "${home}/steamgauge.exe"
    taskkill //F //IM steamgauge.exe > /dev/null 2>&1 || true
    "${home}/uninstall.exe" /S
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
