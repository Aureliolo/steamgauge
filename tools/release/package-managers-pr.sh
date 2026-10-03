#!/usr/bin/env bash
# Lands a release's Homebrew cask and Scoop manifest on main through a pull request that merges
# once every check passes, for release.yml's package managers (main) job.
#
#   tools/release/package-managers-pr.sh open <version> <folder>  opens the pull request, or finds
#                                                                  the one open; prints pr= and
#                                                                  head=, or done=true when main
#                                                                  already holds these files
#   tools/release/package-managers-pr.sh wait <pr>                waits for its checks, printing
#                                                                  state=clean, behind or merged;
#                                                                  anything else fails
#   tools/release/package-managers-pr.sh act <pr> <state>         merges a clean one, brings a
#                                                                  behind one up to date with main
#
# Every refusal ends the job with the reason: a cask that never reaches main has to be seen.
set -euo pipefail
: "${GH_REPO:?GH_REPO names the repository}"

tools="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
files=(Casks/steamgauge.rb bucket/steamgauge.json)

fail() {
  echo "::error::$*" >&2
  exit 1
}

open_pr() {
  local version="$1" folder="$2"
  local branch="package-managers/v${version}"
  local main path have want changed=0
  main="$(gh api "repos/${GH_REPO}/git/ref/heads/main" --jq .object.sha)"
  for path in "${files[@]}"; do
    if ! have="$(gh api "repos/${GH_REPO}/contents/${path}?ref=${main}" --jq .content 2> /dev/null | base64 -d)"; then
      have=""
    fi
    want="$(cat "${folder}/${path}")"
    if [[ "${have}" != "${want}" ]]; then
      changed=1
    fi
  done
  if [[ "${changed}" == 0 ]]; then
    echo "done=true"
    return
  fi

  # Left by an earlier run: started again from main, so the pull request carries one commit.
  # The commit goes through GitHub's API, which signs it; every branch takes only signed commits.
  # Its printed line is kept off stdout, which the workflow reads as the step's outputs.
  local head
  node "${tools}/github-ref.mjs" reset "heads/${branch}" "${main}" >&2
  head="$(cd "${folder}" && EXPECTED_HEAD_OID="${main}" node "${tools}/commit-signed.mjs" \
    "${branch}" "Package v${version} for Homebrew and Scoop" "${files[@]}")"
  head="${head%% *}"

  # The label keeps it out of the next release's changelog, as it does the version bump's.
  local pr
  pr="$(gh pr list --head "${branch}" --state open --json number --jq '.[0].number // empty')"
  if [[ -z "${pr}" ]]; then
    pr="$(gh pr create --base main --head "${branch}" --label release \
      --title "Package v${version} for Homebrew and Scoop" \
      --body "The Homebrew cask and the Scoop manifest for v${version}, written by the release from its signed checksums. It merges once every check passes." \
      | sed -E 's|.*/pull/([0-9]+)$|\1|')"
  fi
  echo "pr=${pr}"
  echo "head=${head}"
}

wait_for() {
  local pr="$1"
  local deadline=$((SECONDS + 100 * 60))
  local view state head held failed decision merge
  while ((SECONDS < deadline)); do
    view="$(gh pr view "${pr}" --json state,mergeStateStatus,reviewDecision,headRefOid,statusCheckRollup)"
    state="$(jq -r .state <<< "${view}")"
    case "${state}" in
      MERGED)
        echo "state=merged"
        return
        ;;
      CLOSED) fail "Pull request #${pr} was closed without merging." ;;
      OPEN) ;;
      *) fail "Pull request #${pr} is in a state GitHub does not document: ${state}." ;;
    esac
    head="$(jq -r .headRefOid <<< "${view}")"
    held="$(gh api "repos/${GH_REPO}/actions/runs?head_sha=${head}" \
      --jq '[.workflow_runs[] | select(.status == "action_required")] | length')"
    if [[ "${held}" != 0 ]]; then
      fail "Pull request #${pr}'s checks are waiting for someone to approve them, which happens when it is not opened as the packaging App."
    fi
    failed="$(jq -r '.statusCheckRollup[]
      | select(((.conclusion // "") | IN("FAILURE", "CANCELLED", "TIMED_OUT", "ACTION_REQUIRED", "STARTUP_FAILURE"))
          or ((.state // "") | IN("FAILURE", "ERROR")))
      | .name // .context' <<< "${view}")"
    if [[ -n "${failed}" ]]; then
      failed="$(paste -sd, - <<< "${failed}")"
      fail "Pull request #${pr} failed: ${failed}."
    fi
    decision="$(jq -r .reviewDecision <<< "${view}")"
    if [[ "${decision}" == "REVIEW_REQUIRED" ]]; then
      fail "Pull request #${pr} needs an approving review before it can merge; see .github/release-process.md."
    fi
    merge="$(jq -r .mergeStateStatus <<< "${view}")"
    case "${merge}" in
      CLEAN)
        echo "state=clean"
        return
        ;;
      BEHIND)
        echo "state=behind"
        return
        ;;
      DIRTY) fail "Pull request #${pr} conflicts with main." ;;
      # Checks still running, or GitHub still working out whether it can merge.
      *) ;;
    esac
    sleep 30
  done
  fail "Pull request #${pr}'s checks did not finish within 100 minutes."
}

act() {
  local pr="$1" state="$2"
  local head
  head="$(gh pr view "${pr}" --json headRefOid --jq .headRefOid)"
  case "${state}" in
    merged) ;;
    clean) gh pr merge "${pr}" --squash --match-head-commit "${head}" ;;
    behind)
      gh api -X PUT "repos/${GH_REPO}/pulls/${pr}/update-branch" -f "expected_head_sha=${head}" > /dev/null
      ;;
    *) fail "No action for state ${state}." ;;
  esac
}

case "${1:-}" in
  open) open_pr "$2" "$3" ;;
  wait) wait_for "$2" ;;
  act) act "$2" "$3" ;;
  *)
    echo "usage: $0 open <version> <folder> | wait <pr> | act <pr> <state>" >&2
    exit 2
    ;;
esac
