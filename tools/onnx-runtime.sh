#!/usr/bin/env bash
# Runs a cargo command that compiles ort-sys, and runs it again while what failed is ort-sys's
# download of ONNX Runtime. ort-sys fetches the prebuilt library from its maker's CDN inside its
# build script, in one attempt with no retry, so a connection dropped mid-download fails the whole
# build. A failure with the library already in place is a real one and is not tried again.
#
#   tools/onnx-runtime.sh cargo build --release --locked ...
set -uo pipefail

case "${RUNNER_OS:-$(uname -s)}" in
  Windows | MINGW* | MSYS*) fetched="${LOCALAPPDATA:?}\\ort.pyke.io\\dfbin" ;;
  macOS | Darwin) fetched="${HOME}/Library/Caches/ort.pyke.io/dfbin" ;;
  *) fetched="${XDG_CACHE_HOME:-${HOME}/.cache}/ort.pyke.io/dfbin" ;;
esac

attempts=5
for ((attempt = 1; attempt <= attempts; attempt++)); do
  "$@" && exit 0
  status=$?
  # ort-sys keeps a finished download under a folder named by its hash, and leaves a tmp. folder
  # where one was cut short.
  finished="$(find "${fetched}" -mindepth 2 -maxdepth 2 -type d ! -name 'tmp.*' 2>/dev/null || true)"
  if [[ -n "${finished}" ]]; then
    exit "${status}"
  fi
  find "${fetched}" -mindepth 2 -maxdepth 2 -type d -name 'tmp.*' -exec rm -rf {} + 2>/dev/null
  if ((attempt < attempts)); then
    echo "::warning::Fetching ONNX Runtime failed on attempt ${attempt} of ${attempts}; trying again."
    sleep $((attempt * 15))
  fi
done
echo "::error::ONNX Runtime could not be fetched in ${attempts} attempts."
exit 1
