#!/usr/bin/env bash
# The Windows setup program is built from crates/steamgauge-app/installer.nsi, which is Tauri's
# own installer script at the bundler version the pinned tauri-cli builds with, plus the change
# installer.patch holds. Tauri fills that script in from its own data, so a copy left behind by a
# tauri-cli bump could build a setup program that silently misses something new. This rebuilds the
# copy from upstream and the patch, and fails when it differs: when Renovate moves tauri-cli, or
# when someone edits the copy without the patch.
set -euo pipefail

app=crates/steamgauge-app
cli="$(sed -n 's/^ *TAURI_CLI_TAG: tauri-cli-v\(.*\)$/\1/p' .github/workflows/packages.yml | sort -u)"
if [[ ! "${cli}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "packages.yml does not name one tauri-cli version: '${cli}'." >&2
  exit 1
fi
for workflow in .github/workflows/release-build.yml; do
  if ! grep -q "TAURI_CLI_TAG: tauri-cli-v${cli}$" "${workflow}"; then
    echo "${workflow} does not build with tauri-cli ${cli}, as packages.yml does." >&2
    exit 1
  fi
done

# The bundler a tauri-cli release builds with is the exact version it requires.
bundler="$(curl -fsSL --proto '=https' --tlsv1.2 --retry 6 --retry-all-errors -A "steamgauge-ci (https://github.com/Aureliolo/steamgauge)" \
  "https://crates.io/api/v1/crates/tauri-cli/${cli}/dependencies" |
  jq -r '.dependencies[] | select(.crate_id == "tauri-bundler") | .req')"
bundler="${bundler#=}"
if [[ ! "${bundler}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "tauri-cli ${cli} requires tauri-bundler '${bundler}', not one exact version." >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
curl -fsSL --proto '=https' --tlsv1.2 --retry 6 --retry-all-errors -o "${work}/installer.nsi" \
  "https://raw.githubusercontent.com/tauri-apps/tauri/tauri-bundler-v${bundler}/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi"
if ! patch --quiet --no-backup-if-mismatch "${work}/installer.nsi" "${app}/installer.patch"; then
  echo "installer.patch no longer applies to Tauri's installer.nsi at tauri-bundler ${bundler}." >&2
  echo "Copy upstream's file to ${app}/installer.nsi again and redo the change in installer.patch." >&2
  exit 1
fi
if ! diff -u "${work}/installer.nsi" "${app}/installer.nsi"; then
  echo "${app}/installer.nsi is not Tauri's installer.nsi at tauri-bundler ${bundler} with installer.patch applied." >&2
  exit 1
fi
echo "installer.nsi is Tauri's at tauri-bundler ${bundler} (tauri-cli ${cli}) with installer.patch."
