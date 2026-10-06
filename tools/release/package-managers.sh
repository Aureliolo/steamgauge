#!/usr/bin/env bash
# Writes the Homebrew cask, the Scoop manifest and the winget manifests for one release, from the
# release's SHA256SUMS. Whoever calls this has already checked that file against the release's
# attestation, so every hash below is one the release is signed over.
#
#   tools/release/package-managers.sh <version> <SHA256SUMS> <folder to write into>
#
# It writes <folder>/Casks/steamgauge.rb, <folder>/bucket/steamgauge.json, and the three
# manifests winget takes in <folder>/winget.
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <version> <SHA256SUMS> <folder to write into>" >&2
  exit 2
fi
version="$1"
sums="$2"
out="$3"

# The release gate refuses anything else, and the manifests below put the version into URLs.
if [[ ! "${version}" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "${version} is not a version of three numbers." >&2
  exit 2
fi

repository="https://github.com/Aureliolo/steamgauge"
download="${repository}/releases/download/v${version}"

# The SHA-256 SHA256SUMS gives a file, refused unless exactly one line names it: two would leave
# the choice to whichever came first.
hash_of() {
  local hash name found=()
  while read -r hash name; do
    if [[ "${name#\*}" == "$1" ]]; then
      found+=("${hash}")
    fi
  done < "${sums}"
  if [[ ${#found[@]} -ne 1 || ! "${found[0]}" =~ ^[0-9a-f]{64}$ ]]; then
    echo "${sums} does not hold exactly one SHA-256 for $1." >&2
    exit 1
  fi
  printf '%s' "${found[0]}"
}

dmg="steamgauge-${version}-macos-arm64.dmg"
zip="steamgauge-${version}-x86_64-pc-windows-msvc.zip"
setup="steamgauge-${version}-windows-x64-setup.exe"
dmg_hash="$(hash_of "${dmg}")"
zip_hash="$(hash_of "${zip}")"
setup_hash="$(hash_of "${setup}")"

short="Finds out what players of a game think, from every Steam review."
long="SteamGauge downloads a game's Steam reviews, reads every claim in them with a model on your own machine, and shows what players praise and criticise, with the reviews behind every figure."

mkdir -p "${out}/Casks" "${out}/bucket" "${out}/winget"

# A cask rather than a formula: what ships for macOS is an app in a disk image, which a formula
# cannot install. The command reaches PATH through a script that runs the program inside the app
# by its real path. A symlink would not do: Tauri on macOS refuses its own path when that passes
# through one, and it finds its own files from that path. The app updates itself in place, which
# `auto_updates` tells Homebrew, so `brew upgrade` leaves it to the app unless asked with --greedy.
cat > "${out}/Casks/steamgauge.rb" << CASK
# Written by tools/release/package-managers.sh for each release, from the release's own signed
# checksums.
cask "steamgauge" do
  version "${version}"
  sha256 "${dmg_hash}"

  url "${repository}/releases/download/v#{version}/steamgauge-#{version}-macos-arm64.dmg"
  name "SteamGauge"
  desc "${short%.}"
  homepage "${repository}"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on arch: :arm64
  depends_on macos: ">= :ventura"

  app "SteamGauge.app"
  shimscript = "#{staged_path}/steamgauge.wrapper.sh"
  binary shimscript, target: "steamgauge"

  preflight do
    File.write shimscript, <<~EOS
      #!/bin/sh
      exec '#{appdir}/SteamGauge.app/Contents/MacOS/steamgauge' "\$@"
    EOS
  end

  zap trash: [
    "~/Library/Application Support/com.aureliolo.steamgauge",
    "~/Library/Caches/com.aureliolo.steamgauge",
    "~/Library/Caches/steamgauge",
    "~/Library/Preferences/com.aureliolo.steamgauge.plist",
    "~/Library/Saved Application State/com.aureliolo.steamgauge.savedState",
    "~/Library/WebKit/com.aureliolo.steamgauge",
  ]

  caveats <<~EOS
    SteamGauge is not notarised by Apple, so macOS warns the first time it opens.
    See ${repository}#the-first-launch for what to do and why.
  EOS
end
CASK

# The autoupdate block is what `checkver.ps1 -Update` in a Scoop bucket reads to raise the
# manifest by itself; the hash comes from the release's SHA256SUMS rather than a download. The
# program's own folder goes on the PATH rather than a `bin` shim: Scoop makes the shim of a
# windowed program windowed too, and that shim returns at once and passes on none of its output.
jq -n \
  --arg version "${version}" \
  --arg homepage "${repository}" \
  --arg download "${download}" \
  --arg next_download "${repository}/releases/download/v\$version" \
  --arg folder "${zip%.zip}" \
  --arg hash "${zip_hash}" \
  --arg description "${short}" \
  '
  def later: split($version) | join("$version");
  {
    version: $version,
    description: $description,
    homepage: $homepage,
    license: "Apache-2.0",
    architecture: {"64bit": {url: "\($download)/\($folder).zip", hash: $hash, extract_dir: $folder}},
    env_add_path: ".",
    shortcuts: [["steamgauge.exe", "SteamGauge"]],
    checkver: "github",
    autoupdate: {architecture: {"64bit": {
      url: "\($next_download)/\($folder | later).zip",
      hash: {url: "$baseurl/SHA256SUMS", regex: "$sha256\\s+\\*?$basename"},
      extract_dir: ($folder | later)
    }}}
  }' > "${out}/bucket/steamgauge.json"

# Written whole rather than raised with `wingetcreate update`, so every field is this script's
# and none is carried over from whatever the last version in winget-pkgs said. The product code
# is the key Tauri's setup program writes its uninstall entry under, its product name, and the
# entry's publisher is the one that setup program writes there, tauri.conf.json's, so winget
# knows an installed SteamGauge as this package. The package itself is published under the
# GitHub account that releases it.
manifest_version=1.12.0
schema="https://aka.ms/winget-manifest"
identifier=Aureliolo.SteamGauge

cat > "${out}/winget/${identifier}.yaml" << VERSION
# yaml-language-server: \$schema=${schema}.version.${manifest_version}.schema.json

PackageIdentifier: ${identifier}
PackageVersion: ${version}
DefaultLocale: en-GB
ManifestType: version
ManifestVersion: ${manifest_version}
VERSION

cat > "${out}/winget/${identifier}.installer.yaml" << INSTALLER
# yaml-language-server: \$schema=${schema}.installer.${manifest_version}.schema.json

PackageIdentifier: ${identifier}
PackageVersion: ${version}
InstallerType: nullsoft
Scope: user
UpgradeBehavior: install
ProductCode: SteamGauge
AppsAndFeaturesEntries:
- DisplayName: SteamGauge
  Publisher: Aurelio Amoroso
  ProductCode: SteamGauge
InstallationMetadata:
  DefaultInstallLocation: '%LocalAppData%\\SteamGauge'
Installers:
- Architecture: x64
  InstallerUrl: ${download}/${setup}
  InstallerSha256: ${setup_hash^^}
ManifestType: installer
ManifestVersion: ${manifest_version}
INSTALLER

cat > "${out}/winget/${identifier}.locale.en-GB.yaml" << LOCALE
# yaml-language-server: \$schema=${schema}.defaultLocale.${manifest_version}.schema.json

PackageIdentifier: ${identifier}
PackageVersion: ${version}
PackageLocale: en-GB
Publisher: Aureliolo
PublisherUrl: https://github.com/Aureliolo
PublisherSupportUrl: ${repository}/issues
PackageName: SteamGauge
PackageUrl: ${repository}
License: Apache-2.0
LicenseUrl: ${repository}/blob/v${version}/LICENSE
ShortDescription: ${short}
Description: ${long}
Moniker: steamgauge
Tags:
- game-reviews
- games
- steam
- reviews
ReleaseNotesUrl: ${repository}/releases/tag/v${version}
ManifestType: defaultLocale
ManifestVersion: ${manifest_version}
LOCALE
