#!/usr/bin/env bash
# The .rpm's half of install-check.sh, run inside Fedora: the package has to say what it is, as
# written below, replace the previous release's steam-gauge, and run the program it installs.
#
#   VERSION=<version> RPM=<.rpm> PREVIOUS=<version> PREVIOUS_RPM=<.rpm> HOMEPAGE=<url> \
#     bash install-check-fedora.sh

# A command whose output is compared is checked by the comparison: if it fails, what it printed
# is not what was expected.
# shellcheck disable=SC2312
set -euo pipefail

: "${VERSION:?}" "${RPM:?}" "${PREVIOUS:?}" "${PREVIOUS_RPM:?}" "${HOMEPAGE:?}"

same() {
  if [[ "$2" != "$3" ]]; then
    echo "$1 is '$2', not '$3'." >&2
    exit 1
  fi
  echo "$1: $2"
}

rpm --query --info --package "${RPM}"
for pair in "NAME=steamgauge" "VERSION=${VERSION}" "RELEASE=1" "LICENSE=Apache-2.0" "URL=${HOMEPAGE}"; do
  same "The .rpm's ${pair%%=*}" "$(rpm --query --package --queryformat "%{${pair%%=*}}" "${RPM}")" "${pair#*=}"
done
for relation in obsoletes provides; do
  rpm --query --package "--${relation}" "${RPM}" | grep -qxF steam-gauge \
    || { echo "The .rpm does not declare that it ${relation} steam-gauge." >&2; exit 1; }
done
files="$(rpm --query --list --package "${RPM}")"
for file in /usr/bin/steamgauge /usr/lib/steamgauge/LICENSE /usr/lib/steamgauge/THIRD-PARTY-NOTICES.txt \
  /usr/share/applications/steamgauge.desktop /usr/share/doc/steamgauge/README.md \
  /usr/share/licenses/steamgauge/LICENSE; do
  grep -qxF "${file}" <<< "${files}" || { echo "The .rpm holds no ${file}." >&2; exit 1; }
done

dnf install -y "${PREVIOUS_RPM}" > /dev/null
same "The package ${PREVIOUS} installs" "$(rpm --query --queryformat '%{NAME}' steam-gauge)" steam-gauge
dnf install -y "${RPM}" > /dev/null
if rpm --query steam-gauge > /dev/null; then
  echo "Installing steamgauge left ${PREVIOUS}'s steam-gauge installed beside it." >&2
  exit 1
fi
grep -qxF "Name=SteamGauge" /usr/share/applications/steamgauge.desktop \
  || { echo "The .rpm's menu entry does not call the program SteamGauge." >&2; exit 1; }
same "steamgauge --version" "$(steamgauge --version)" "steamgauge ${VERSION}"
