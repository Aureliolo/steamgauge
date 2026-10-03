#!/usr/bin/env bash
# Turns one target's release build into what a person downloads: a portable archive and the
# installers for that system, every one holding the same program, beside a SHA-256 for each.
#
#   tools/release/package.sh <target> <features> <version> <notices> <out-dir>
#
# The binary must already be built for <target> with <features> (custom-protocol among them);
# Tauri's bundler packages that build rather than making its own, so what is packaged is exactly
# what the build step compiled from the locked sources. $CARGO_TAURI names the tauri-cli to use.
set -euo pipefail

target="$1"
features="$2"
version="$3"
notices="$4"
out="$5"

release="target/${target}/release"
name="steamgauge-${version}-${target}"
case "${target}" in
  *-windows-*) binary=steamgauge.exe bundles=nsis ;;
  *-apple-darwin) binary=steamgauge bundles=app,dmg ;;
  *-linux-*) binary=steamgauge bundles=deb,rpm ;;
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
# whatever the build put there travels with it, in the archive and in every installer.
libraries=()
while IFS= read -r library; do
  cp "${library}" "${payload}/"
  libraries+=("$(basename "${library}")")
done < <(find "${release}" -maxdepth 1 -type f \
  \( -name '*.dll' -o -name '*.dylib' -o -name '*.so' -o -name '*.so.*' \) | LC_ALL=C sort)

# Windows Explorer opens a zip and nothing else without help; every other system opens a tarball.
# macOS tar otherwise adds an AppleDouble `._` file for every file carrying extended attributes.
case "${target}" in
  *-windows-*) (cd "${out}" && 7z a -tzip -bso0 -bsp0 "${name}.zip" "${name}") ;;
  *) (cd "${out}" && COPYFILE_DISABLE=1 tar -czf "${name}.tar.gz" "${name}") ;;
esac

# Resources are named relative to the directory tauri.conf.json is in, and land beside the
# program on Windows and Linux and in the bundle's Resources on macOS.
resources="\"../../${payload}/THIRD-PARTY-NOTICES.txt\": \"THIRD-PARTY-NOTICES.txt\""
for library in ${libraries[@]+"${libraries[@]}"}; do
  resources="${resources}, \"../../${payload}/${library}\": \"${library}\""
done
config="{\"version\": \"${version}\", \"bundle\": {\"resources\": {${resources}}}}"

feature_args=()
if [[ -n "${features}" ]]; then
  feature_args=(--features "${features}")
fi
"${CARGO_TAURI}" bundle --ci --target "${target}" --bundles "${bundles}" --config "${config}" \
  ${feature_args[@]+"${feature_args[@]}"}

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
  x86_64-pc-windows-msvc)
    cp "$(only "${bundle}/nsis/*-setup.exe")" "${out}/steamgauge-${version}-windows-x64-setup.exe"
    ;;
  aarch64-apple-darwin)
    cp "$(only "${bundle}/dmg/*.dmg")" "${out}/steamgauge-${version}-macos-arm64.dmg"
    ;;
  x86_64-unknown-linux-gnu)
    cp "$(only "${bundle}/deb/*.deb")" "${out}/steamgauge_${version}_amd64.deb"
    cp "$(only "${bundle}/rpm/*.rpm")" "${out}/steamgauge-${version}-1.x86_64.rpm"
    ;;
  *)
    echo "No installer names are defined for ${target}." >&2
    exit 1
    ;;
esac
rm -rf "${payload}"

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
