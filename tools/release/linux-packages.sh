#!/usr/bin/env bash
# Builds the .deb and the .rpm for one release from its Linux archive, with nFPM and
# tools/release/linux/nfpm.yaml, so both hold the very program the archive holds.
#
#   tools/release/linux-packages.sh <version> <Linux archive> <folder to write into>
#
# SOURCE_DATE_EPOCH, when set, dates the changelog, the man page and every file in both
# packages; package.sh sets it to the packaged commit's time, so one commit builds the same
# packages. The program is run once, for the man page, so the libraries it links have to be
# installed, as they are wherever it was built.
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <version> <Linux archive> <folder to write into>" >&2
  exit 2
fi
version="$1"
archive="$2"
out="$3"

if [[ ! "${version}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "${version} is not a version of three numbers." >&2
  exit 2
fi

here="$(realpath "$(dirname "${BASH_SOURCE[0]}")")"
icons="$(realpath "${here}/../../crates/steamgauge-app/icons")"
mkdir -p "${out}"
out="$(realpath "${out}")"
repository="https://github.com/Aureliolo/steamgauge"
maintainer="$(sed -n 's/^maintainer: //p' "${here}/linux/nfpm.yaml")"
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(date +%s)}"

stage="$(mktemp -d)"
trap 'rm -rf "${stage}"' EXIT
tar -xzf "${archive}" -C "${stage}"
folder="${stage}/$(basename "${archive}" .tar.gz)"
if [[ ! -x "${folder}/steamgauge" ]]; then
  echo "${archive} holds no steamgauge program in $(basename "${folder}")." >&2
  exit 1
fi
# Every file the archive carries has a place in the packages below; one that does not, a runtime
# library a backend added, would otherwise be left out without a word.
carried="$(cd "${folder}" && find . -type f | LC_ALL=C sort | tr '\n' ' ')"
if [[ "${carried}" != "./LICENSE ./README.md ./THIRD-PARTY-NOTICES.txt ./steamgauge " ]]; then
  echo "${archive} holds files the packages give no place: ${carried}" >&2
  exit 1
fi

cp "${here}/linux/nfpm.yaml" "${here}/linux/steamgauge.desktop" "${here}/linux/lintian-overrides" "${folder}/"
cp -r "${icons}" "${folder}/icons"

# The C library version the packages ask for is the newest symbol version the program links.
glibc="$(objdump -T "${folder}/steamgauge" | grep -o 'GLIBC_[0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -n 1)"
if [[ -z "${glibc}" ]]; then
  echo "objdump found no C library version in the program." >&2
  exit 1
fi

# The man page is the program's own --help and --version, so it says what they say. It is
# installed compressed, and without a name or time inside that would make two builds differ.
mkdir -p "${folder}/man"
help2man --no-info --section 1 --name "finds out what players of a game think, from every Steam review" \
  --output "${folder}/man/steamgauge.1" "${folder}/steamgauge"
gzip -9n "${folder}/man/steamgauge.1"

# One entry, pointing at the release's notes, which nFPM writes as Debian's changelog and as the
# RPM's, short enough for Debian's 80 columns.
changed="$(date -u -d "@${SOURCE_DATE_EPOCH}" +%Y-%m-%dT%H:%M:%SZ)"
cat > "${folder}/changelog.yml" << CHANGELOG
- semver: ${version}-1
  date: ${changed}
  packager: ${maintainer}
  deb:
    urgency: medium
    distributions:
      - unstable
  changes:
    - note: "Release notes: ${repository}/releases/tag/v${version}"
CHANGELOG

# Debian's machine-readable form. Apache-2.0 is among the licences every Debian system carries,
# so the file points at it; the holder is the one LICENSE names.
holder="$(grep -m1 -E '^ *Copyright [0-9]{4} ' "${folder}/LICENSE" | sed -E 's/^ *Copyright //')"
if [[ -z "${holder}" ]]; then
  echo "LICENSE names no copyright holder." >&2
  exit 1
fi
cat > "${folder}/copyright" << COPYRIGHT
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: SteamGauge
Upstream-Contact: ${repository}/issues
Source: ${repository}
Comment: The program is built with third-party components under their own licences,
 whose notices and texts are in /usr/share/doc/steamgauge/THIRD-PARTY-NOTICES.txt.

Files: *
Copyright: ${holder}
License: Apache-2.0
 On Debian systems, the full text of the Apache License, Version 2.0, is in
 /usr/share/common-licenses/Apache-2.0.
COPYRIGHT

for packager in deb rpm; do
  (cd "${folder}" && VERSION="${version}" GLIBC="${glibc}" \
    nfpm package --config nfpm.yaml --packager "${packager}" --target "${out}/")
done

for package in "${out}/steamgauge_${version}-1_amd64.deb" "${out}/steamgauge-${version}-1.x86_64.rpm"; do
  if [[ ! -f "${package}" ]]; then
    echo "nFPM wrote no $(basename "${package}")." >&2
    exit 1
  fi
done
