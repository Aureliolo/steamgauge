#!/usr/bin/env bash
# The .rpm's half of install-check.sh, run inside Fedora: Fedora's own linter has to pass it, the
# package has to say what it is, as written below and in install-check.sh, replace the previous
# release's steam-gauge, and run the program it installs.
#
#   VERSION=<version> RPM=<.rpm> PREVIOUS=<version> PREVIOUS_RPM=<.rpm> HOMEPAGE=<url> \
#     MAINTAINER=<name and address> VENDOR=<name> FILES=<the files, sorted> \
#     bash install-check-fedora.sh
#
# /rpmlint.toml holds what rpmlint reports that is so by design.

# A command whose output is compared is checked by the comparison: if it fails, what it printed
# is not what was expected.
# shellcheck disable=SC2312
set -euo pipefail

: "${VERSION:?}" "${RPM:?}" "${PREVIOUS:?}" "${PREVIOUS_RPM:?}" "${HOMEPAGE:?}" "${MAINTAINER:?}"
: "${VENDOR:?}" "${FILES:?}"

same() {
  if [[ "$2" != "$3" ]]; then
    echo "$1 is '$2', not '$3'." >&2
    exit 1
  fi
  echo "$1: $2"
}

# Cisco's codec repository, which rpmlint's dependencies would reach for a codec Fedora's own
# repositories stand in for, is a server of its own that can fail on its own.
dnf install --assumeyes --setopt=install_weak_deps=False --disablerepo=fedora-cisco-openh264 \
  rpmlint > /dev/null
rpmlint --version
rpmlint --strict --config /rpmlint.toml "${RPM}"

rpm --query --info --package "${RPM}"
for pair in "NAME=steamgauge" "VERSION=${VERSION}" "RELEASE=1" "ARCH=x86_64" "LICENSE=Apache-2.0" \
  "URL=${HOMEPAGE}" "VENDOR=${VENDOR}" "PACKAGER=${MAINTAINER}" "GROUP=Applications/Text" \
  "SUMMARY=Finds out what players of a game think, from every Steam review"; do
  same "The .rpm's ${pair%%=*}" "$(rpm --query --package --queryformat "%{${pair%%=*}}" "${RPM}")" "${pair#*=}"
done
same "The .rpm's Obsoletes" "$(rpm --query --package --obsoletes "${RPM}")" "steam-gauge < 0.1.3"
rpm --query --package --provides "${RPM}" | grep -qxF "steam-gauge = 0.1.3" \
  || { echo "The .rpm does not provide steam-gauge = 0.1.3." >&2; exit 1; }
requires="$(rpm --query --package --requires "${RPM}" | grep -v '^rpmlib(' | LC_ALL=C sort | tr '\n' ' ')"
[[ "${requires}" =~ ^libc\.so\.6\(GLIBC_2\.[0-9]+\)\(64bit\)\ libgtk-3\.so\.0\(\)\(64bit\)\ libwebkit2gtk-4\.1\.so\.0\(\)\(64bit\)\ $ ]] \
  || { echo "The .rpm requires '${requires}', not the C library, GTK 3 and WebKitGTK 4.1." >&2; exit 1; }
echo "The .rpm's Requires: ${requires}"
changelog="$(rpm --query --package --changelog "${RPM}")"
[[ "$(head -n 1 <<< "${changelog}")" == "* "*" ${MAINTAINER} - ${VERSION}-1" ]] \
  || { echo "The .rpm's changelog does not open with ${MAINTAINER}'s entry for ${VERSION}-1: ${changelog}" >&2; exit 1; }
echo "The .rpm's changelog: ${changelog}"
same "The .rpm's files" "$(rpm --query --list --package "${RPM}" | LC_ALL=C sort | tr '\n' ' ')" "${FILES}"
same "The .rpm's licence file" "$(rpm --query --licensefiles --package "${RPM}")" /usr/share/licenses/steamgauge/LICENSE

dnf install -y "${PREVIOUS_RPM}" > /dev/null
same "The package ${PREVIOUS} installs" "$(rpm --query --queryformat '%{NAME}' steam-gauge)" steam-gauge
# The image leaves documentation out, which a Fedora system installs.
dnf install -y --setopt=tsflags= "${RPM}" > /dev/null
if rpm --query steam-gauge > /dev/null; then
  echo "Installing steamgauge left ${PREVIOUS}'s steam-gauge installed beside it." >&2
  exit 1
fi
rpm --verify steamgauge
grep -qxF "Name=SteamGauge" /usr/share/applications/steamgauge.desktop \
  || { echo "The .rpm's menu entry does not call the program SteamGauge." >&2; exit 1; }
same "steamgauge --version" "$(steamgauge --version)" "steamgauge ${VERSION}"
