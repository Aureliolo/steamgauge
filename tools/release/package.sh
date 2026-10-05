#!/usr/bin/env bash
# Turns one target's release build into what a person downloads: a portable archive and the
# installers for that system, every one holding the same program, beside a SHA-256 for each.
#
#   tools/release/package.sh <target> <features> <version> <notices> <out-dir>
#
# The binary must already be built for <target> with <features> (custom-protocol among them);
# what is packaged is that build rather than one of its own, so it is exactly what the build step
# compiled from the locked sources. Tauri's bundler makes the Windows setup program and the macOS
# disk image ($CARGO_TAURI names the tauri-cli to use); nFPM makes the .deb and the .rpm from the
# Linux archive itself (tools/release/linux-packages.sh).

# pipefail makes a failing stage fail the pipeline it is in.
# shellcheck disable=SC2312
set -euo pipefail

target="$1"
features="$2"
version="$3"
notices="$4"
out="$5"

VERSION="${version}"
# shellcheck source=tools/release/names.sh
source "$(dirname "${BASH_SOURCE[0]}")/names.sh"

release="target/${target}/release"
name="$(stem "${target}")"
listed="$(installers "${target}")"
read -r -a installer_names <<< "${listed}"
case "${target}" in
  *-windows-*) binary=steamgauge.exe bundles=nsis ;;
  *-apple-darwin) binary=steamgauge bundles=app,dmg ;;
  *-linux-*) binary=steamgauge bundles="" ;;
  *)
    echo "No packaging is defined for ${target}." >&2
    exit 1
    ;;
esac

mkdir -p "${out}"
payload="${out}/${name}"
rm -rf "${payload}"
mkdir -p "${payload}"
cp "${release}/${binary}" README.md LICENSE "${payload}/"
cp "${notices}" "${payload}/THIRD-PARTY-NOTICES.txt"

# The GPU backends load runtime libraries from beside the program (DirectML.dll on Windows), so
# whatever the build put there travels with it, in the archive and in every installer. ort places
# them as symbolic links where the system allows one, as GitHub's Windows runners do, so links
# are found too and cp copies what they point at.
libraries=()
while IFS= read -r library; do
  cp -L "${library}" "${payload}/"
  libraries+=("$(basename "${library}")")
done < <(find "${release}" -maxdepth 1 \( -type f -o -type l \) \
  \( -name '*.dll' -o -name '*.dylib' -o -name '*.so' -o -name '*.so.*' \) | LC_ALL=C sort)

# Windows Explorer opens a zip and nothing else without help; every other system opens a tarball.
# macOS tar otherwise adds an AppleDouble `._` file for every file carrying extended attributes.
case "${target}" in
  *-windows-*) (cd "${out}" && 7z a -tzip -bso0 -bsp0 "${name}.zip" "${name}") ;;
  *) (cd "${out}" && COPYFILE_DISABLE=1 tar -czf "${name}.tar.gz" "${name}") ;;
esac

if [[ -z "${bundles}" ]]; then
  # Dated by the commit packaged, so the same commit builds the same packages.
  SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}" \
    bash "$(dirname "${BASH_SOURCE[0]}")/linux-packages.sh" "${version}" "${out}/${name}.tar.gz" "${out}"
else
  # Resources are named relative to the directory tauri.conf.json is in, and land beside the
  # program on Windows and in the bundle's Resources on macOS.
  # The licence travels as a file rather than as the bundler's licence page, which would make the
  # disk image and the setup program ask a person to accept an Apache licence before installing.
  resources="\"../../${payload}/LICENSE\": \"LICENSE\", \"../../${payload}/THIRD-PARTY-NOTICES.txt\": \"THIRD-PARTY-NOTICES.txt\""
  for library in ${libraries[@]+"${libraries[@]}"}; do
    resources="${resources}, \"../../${payload}/${library}\": \"${library}\""
  done
  config="{\"version\": \"${version}\", \"bundle\": {\"resources\": {${resources}}}}"

  feature_args=()
  if [[ -n "${features}" ]]; then
    feature_args=(--features "${features}")
  fi
  "${CARGO_TAURI:?package.sh needs CARGO_TAURI}" bundle --ci --target "${target}" --bundles "${bundles}" \
    --config "${config}" ${feature_args[@]+"${feature_args[@]}"}

  # One file per installer, or the names below would silently pick one of several.
  only() {
    local found
    found="$(compgen -G "$1" || true)"
    if [[ -z "${found}" || "$(wc -l <<<"${found}")" -ne 1 ]]; then
      echo "Expected exactly one file matching $1, found: ${found:-none}" >&2
      exit 1
    fi
    printf '%s\n' "${found}"
  }
  bundle="${release}/bundle"
  case "${target}" in
    *-windows-*) cp "$(only "${bundle}/nsis/*-setup.exe")" "${out}/${installer_names[0]}" ;;
    *) cp "$(only "${bundle}/dmg/*.dmg")" "${out}/${installer_names[0]}" ;;
  esac
fi
rm -rf "${payload}"

for installer in "${installer_names[@]}"; do
  if [[ ! -f "${out}/${installer}" ]]; then
    echo "Packaging wrote no ${installer}." >&2
    exit 1
  fi
done

# BSD's sha256sum on macOS takes none of the GNU options, and shasum writes the same two-space
# format GNU's --check reads.
(
  cd "${out}"
  for file in *; do
    [[ "${file}" == *.sha256 || "${file}" == *.crates ]] && continue
    if [[ "$(uname -s)" == Darwin ]]; then
      shasum -a 256 "${file}" > "${file}.sha256"
    else
      sha256sum "${file}" > "${file}.sha256"
    fi
  done
)
ls -la "${out}"
