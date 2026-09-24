#!/usr/bin/env bash
# Builds the core crate while a training run holds the card, without taking the machine's
# memory from under it.
#
#   tools/cargo-beside-training.sh test -p steamgauge-core
#   tools/cargo-beside-training.sh run -p steamgauge-core --example sample-report -- out.html
#
# On Windows every byte a training run reserves on the card is charged to the machine's commit
# as well, so a 560M run holds about 21.5 GB of it, and a workspace build beside it has twice
# taken the machine to its ceiling and killed the run. The core crate alone, two jobs at a time,
# is small enough to fit beside one, and this makes sure it only ever does: it refuses to start
# with less than START_GB of commit free, and stops the build if free commit falls under
# FLOOR_GB while it runs. A figure that cannot be read counts as none: when the machine cannot
# start PowerShell it has none to give.
#
# Only check, clippy, test and run, only the core crate, never --release: the app crate pulls in
# the whole of Tauri, and a release build is a different order of memory.
set -u
START_GB=${START_GB:-25}
FLOOR_GB=${FLOOR_GB:-12}

free_commit_gb() {
  powershell -NoProfile -Command "[int]((Get-CimInstance Win32_OperatingSystem).FreeVirtualMemory/1MB)" 2>/dev/null | tr -d '\r'
}

command=${1:-}
case "$command" in
  check | clippy | test | run) ;;
  *)
    echo "only check, clippy, test and run may build beside training, not '${command}'" >&2
    exit 64
    ;;
esac
shift
case " $* " in
  *" -p steamgauge-core "*) ;;
  *)
    echo "only the core crate may build beside training: pass -p steamgauge-core" >&2
    exit 64
    ;;
esac
case " $* " in
  *" --release "* | *" -r "* | *" --workspace "*)
    echo "no release or workspace builds beside training" >&2
    exit 64
    ;;
esac

have=$(free_commit_gb)
if ! [[ "$have" =~ ^[0-9]+$ ]] || [ "$have" -lt "$START_GB" ]; then
  echo "${have:-an unreadable figure of} GB of commit free, ${START_GB} needed; not building" >&2
  exit 75
fi

cargo "$command" -j 2 "$@" &
pid=$!
while kill -0 "$pid" 2> /dev/null; do
  have=$(free_commit_gb)
  if ! [[ "$have" =~ ^[0-9]+$ ]] || [ "$have" -lt "$FLOOR_GB" ]; then
    echo "free commit fell to ${have:-an unreadable figure} GB; stopping the build" >&2
    taskkill //F //T //PID "$(cat "/proc/$pid/winpid")" > /dev/null 2>&1
    kill "$pid" 2> /dev/null
    wait "$pid" 2> /dev/null
    exit 75
  fi
  sleep 5
done
wait "$pid"
