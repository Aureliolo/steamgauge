#!/usr/bin/env bash
# A stand-in for gh that winget-submit-test.sh puts first on PATH. It records every call and
# answers from canned data, so the test never reaches GitHub. The scenario comes from FAKE_*
# variables, each of which the test sets for the case it runs.
#
#   FAKE_LOG        file that every call is appended to
#   FAKE_WRITES     file that every call that would change GitHub is appended to
#   FAKE_TEMPLATE   file served as winget-pkgs' pull request template
#   FAKE_FORK       1 when the token's account has a fork of winget-pkgs, 2 when its fork is of
#                   another repository, 0 when it has none
#   FAKE_UPSTREAM   1 when winget-pkgs already holds the version
#   FAKE_PACKAGE    1 when winget-pkgs already holds the package, under any version
#   FAKE_OPEN       none, branch (a pull request on the version's branch), search (one the
#                   search finds under the first-package title) or search-version (one the
#                   search finds under the later-version title)
#   FAKE_DECOY      1 to add an open pull request for a longer version, 1.2.30
#   FAKE_BRANCH     1 when the fork already has the version's branch
#   FAKE_PROBE      error to answer the version probe with a rate limit instead of 200 or 404
#   FAKE_VERSION    the version being submitted
set -euo pipefail

FAKE_LOG="${FAKE_LOG:?}" FAKE_WRITES="${FAKE_WRITES:?}" FAKE_TEMPLATE="${FAKE_TEMPLATE:?}"
FAKE_FORK="${FAKE_FORK:?}" FAKE_UPSTREAM="${FAKE_UPSTREAM:?}" FAKE_OPEN="${FAKE_OPEN:?}"
FAKE_DECOY="${FAKE_DECOY:?}" FAKE_BRANCH="${FAKE_BRANCH:?}" FAKE_VERSION="${FAKE_VERSION:?}"
FAKE_PACKAGE="${FAKE_PACKAGE:?}" FAKE_PROBE="${FAKE_PROBE:?}"

# Ids of forty digits, which the script's own checks accept as commit and tree ids. Upstream's
# newest commit has ids of its own, so a commit built on it shows in the recorded parent.
printf -v head_id '%040d' 1
printf -v base_tree_id '%040d' 2
printf -v tree_id '%040d' 3
printf -v commit_id '%040d' 4
printf -v upstream_head_id '%040d' 9

call="$*"
echo "gh ${call}" >> "${FAKE_LOG}"

not_found() {
  echo "gh: Not Found (HTTP 404)" >&2
  exit 1
}

write() {
  echo "gh ${call}" >> "${FAKE_WRITES}"
  while (($# > 0)); do
    if [[ "$1" == --input ]]; then
      jq -c '{base_tree, paths: [.tree[].path]}' "$2" >> "${FAKE_WRITES}"
    fi
    shift
  done
}

case "${call}" in
  "api user --jq .login") echo releaser ;;
  "api repos/releaser/winget-pkgs --jq "*)
    case "${FAKE_FORK}" in
      1) echo microsoft/winget-pkgs ;;
      2) echo someone/winget-pkgs ;;
      *) not_found ;;
    esac
    ;;
  "api repos/microsoft/winget-pkgs/contents/manifests/a/Aureliolo/SteamGauge/${FAKE_VERSION}")
    if [[ "${FAKE_PROBE}" == error ]]; then
      echo "gh: API rate limit exceeded (HTTP 403)" >&2
      exit 1
    fi
    if [[ "${FAKE_UPSTREAM}" == 1 ]]; then echo '[]'; else not_found; fi
    ;;
  "api repos/microsoft/winget-pkgs/contents/manifests/a/Aureliolo/SteamGauge")
    if [[ "${FAKE_PACKAGE}" == 1 ]]; then echo '[]'; else not_found; fi
    ;;
  "api --method GET repos/microsoft/winget-pkgs/pulls "*)
    if [[ "${FAKE_OPEN}" == branch ]]; then echo 1; else echo 0; fi
    ;;
  "api --method GET search/issues "*)
    items=()
    if [[ "${FAKE_DECOY}" == 1 ]]; then
      items+=('{"title": "New version: Aureliolo.SteamGauge version 1.2.30"}')
    fi
    if [[ "${FAKE_OPEN}" == search ]]; then
      items+=("{\"title\": \"New package: Aureliolo.SteamGauge version ${FAKE_VERSION}\"}")
    fi
    if [[ "${FAKE_OPEN}" == search-version ]]; then
      items+=("{\"title\": \"New version: Aureliolo.SteamGauge version ${FAKE_VERSION}\"}")
    fi
    joined="$(IFS=,; echo "${items[*]:-}")"
    echo "{\"items\": [${joined}]}"
    ;;
  "api repos/microsoft/winget-pkgs --jq .default_branch") echo master ;;
  "api repos/microsoft/winget-pkgs/commits/HEAD --jq "*) echo "${upstream_head_id} ${base_tree_id}" ;;
  "api repos/releaser/winget-pkgs/commits/HEAD --jq "*) echo "${head_id} ${base_tree_id}" ;;
  "api repos/releaser/winget-pkgs/git/trees "*)
    write "$@"
    echo "${tree_id}"
    ;;
  "api repos/releaser/winget-pkgs/git/commits "*)
    write "$@"
    echo "${commit_id}"
    ;;
  "api repos/releaser/winget-pkgs/git/ref/heads/Aureliolo.SteamGauge-${FAKE_VERSION}")
    if [[ "${FAKE_BRANCH}" == 1 ]]; then echo '{}'; else not_found; fi
    ;;
  "api --method PATCH repos/releaser/winget-pkgs/git/refs/heads/"*) write "$@" ;;
  "api repos/releaser/winget-pkgs/git/refs "*) write "$@" ;;
  "api -H Accept: application/vnd.github.raw+json repos/microsoft/winget-pkgs/contents/.github/PULL_REQUEST_TEMPLATE.md")
    cat "${FAKE_TEMPLATE}"
    ;;
  "api repos/microsoft/winget-pkgs/pulls "*)
    write "$@"
    echo 42
    ;;
  *)
    echo "The fake gh has no answer for: gh ${call}" >&2
    exit 99
    ;;
esac
