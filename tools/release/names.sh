# What each system ships, by name, for the release jobs that check and sign the files: sourced,
# with VERSION set, so a format or a name changes in one place.
# shellcheck shell=bash
# Every caller runs under `set -o pipefail`, so a failing stage fails the pipeline it is in.
# shellcheck disable=SC2312

: "${VERSION:?names.sh needs VERSION}"

# The portable archive for a target: a zip for Windows, which opens nothing else unaided.
archive() {
  case "$1" in
    *-windows-*) echo "steamgauge-${VERSION}-$1.zip" ;;
    *) echo "steamgauge-${VERSION}-$1.tar.gz" ;;
  esac
}

installers() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo "steamgauge_${VERSION}_amd64.deb steamgauge-${VERSION}-1.x86_64.rpm" ;;
    x86_64-pc-windows-msvc) echo "steamgauge-${VERSION}-windows-x64-setup.exe" ;;
    aarch64-apple-darwin) echo "steamgauge-${VERSION}-macos-arm64.dmg" ;;
    *)
      echo "No installers are named for $1." >&2
      return 1
      ;;
  esac
}

# Every file an archive holds, directories left out, in one order.
members() {
  case "$1" in
    *.zip) unzip -Z1 "$1" ;;
    *) tar -tzf "$1" ;;
  esac | sed '/\/$/d' | LC_ALL=C sort
}

unpack() {
  case "$1" in
    *.zip) unzip -q "$1" -d "$2" ;;
    *) tar -xzf "$1" -C "$2" ;;
  esac
}
