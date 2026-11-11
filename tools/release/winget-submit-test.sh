#!/usr/bin/env bash
# Runs winget-submit.sh against a fake gh and checks the calls it makes and what it prints, for
# the cases a release's re-run meets. No call reaches GitHub.
#
#   tools/release/winget-submit-test.sh
set -euo pipefail
export LC_ALL=C

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

mkdir "${work}/bin"
cp "${here}/winget-submit-fake-gh.sh" "${work}/bin/gh"
chmod +x "${work}/bin/gh"
export PATH="${work}/bin:${PATH}"

version=1.2.3
names=(Aureliolo.SteamGauge.yaml Aureliolo.SteamGauge.installer.yaml Aureliolo.SteamGauge.locale.en-GB.yaml)

# A manifest folder named ${1}, holding the three files for ${2} unless a case changes it.
make_folder() {
  local folder="${work}/$1" name
  mkdir "${folder}"
  for name in "${names[@]}"; do
    printf 'PackageIdentifier: Aureliolo.SteamGauge\nPackageVersion: %s\n' "$2" > "${folder}/${name}"
  done
}
make_folder manifests "${version}"
make_folder stale 9.9.9
make_folder two "${version}"
rm "${work}/two/Aureliolo.SteamGauge.locale.en-GB.yaml"
make_folder foreign "${version}"
printf 'PackageIdentifier: Other.Package\nPackageVersion: %s\n' "${version}" > "${work}/foreign/Other.Package.yaml"
rm "${work}/foreign/Aureliolo.SteamGauge.locale.en-GB.yaml"

cat > "${work}/template.md" << 'TEMPLATE'
<!-- PR Title Format: "New package: Publisher.Name version X.Y.Z" or "Update: Publisher.Name to X.Y.Z" -->

## Checklist

- [ ] Signed the [Contributor License Agreement](https://cla.opensource.microsoft.com)

## Manifest Checklist

- [ ] Checked that there aren't other open pull requests for the same manifest update/change
- [ ] This PR only modifies one (1) manifest
- [ ] Validated manifest locally with `winget validate --manifest <path>` (validation guide)
- [ ] Tested manifest locally with `winget install --manifest <path>`
TEMPLATE

failures=0
fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

# Each case sets its scenario in the environment, runs the script and leaves its exit status in
# status, and its output and writing calls in ${work}/<name>.* The folder and version it submits
# are ${folder} and ${submit}, which a case may change.
folder=manifests
submit="${version}"
run_case() {
  local name="$1"
  export FAKE_LOG="${work}/${name}.log" FAKE_WRITES="${work}/${name}.writes"
  export FAKE_TEMPLATE="${work}/template.md" FAKE_VERSION="${submit}"
  : > "${FAKE_LOG}"
  : > "${FAKE_WRITES}"
  status=0
  GH_TOKEN=fake bash "${here}/winget-submit.sh" "${submit}" "${work}/${folder}" \
    > "${work}/${name}.out" 2> "${work}/${name}.err" || status=$?
  sed -i -E 's# --input .*# --input FILE#' "${FAKE_WRITES}"
}

expect_status() {
  [[ "${status}" == "$2" ]] || fail "$1: exit status ${status}, not $2"
}

expect_file() {
  local name="$1" extension="$2" want="$3" have
  have="$(< "${work}/${name}.${extension}")"
  [[ "${have}" == "${want}" ]] || fail "${name}: ${extension} was
${have}
but should have been
${want}"
}

expect_no_writes() {
  local written
  written="$(< "${work}/$1.writes")"
  [[ -z "${written}" ]] || fail "$1: wrote to GitHub: ${written}"
}

# The number of lines of a case's writing calls that hold a fixed string.
expect_writes() {
  local name="$1" needle="$2" want="$3" what="$4" have
  have="$(grep -cF -- "${needle}" "${work}/${name}.writes" || true)"
  [[ "${have}" == "${want}" ]] || fail "${name}: ${have} writing calls hold '${needle}', so not ${what}"
}

# The number of lines of a case's recorded calls that hold a fixed string.
expect_calls() {
  local name="$1" needle="$2" want="$3" what="$4" have
  have="$(grep -cF -- "${needle}" "${work}/${name}.log" || true)"
  [[ "${have}" == "${want}" ]] || fail "${name}: ${have} calls hold '${needle}', so not ${what}"
}

printf -v head_id '%040d' 1
printf -v base_tree_id '%040d' 2
printf -v tree_id '%040d' 3
printf -v commit_id '%040d' 4
printf -v upstream_head_id '%040d' 9

new_package="New package: Aureliolo.SteamGauge version ${version}"
new_version="New version: Aureliolo.SteamGauge version ${version}"
branch="Aureliolo.SteamGauge-${version}"
paths="manifests/a/Aureliolo/SteamGauge/${version}"
tree_call="gh api repos/releaser/winget-pkgs/git/trees --input FILE"
tree_body="{\"base_tree\":\"${base_tree_id}\",\"paths\":[\"${paths}/Aureliolo.SteamGauge.installer.yaml\",\"${paths}/Aureliolo.SteamGauge.locale.en-GB.yaml\",\"${paths}/Aureliolo.SteamGauge.yaml\"]}"
commit_prefix="gh api repos/releaser/winget-pkgs/git/commits -f message="
commit_suffix=" -f tree=${tree_id} -f parents[]=${head_id} --jq .sha"
pull_prefix="gh api repos/microsoft/winget-pkgs/pulls -f title="
pull_suffix=" -f head=releaser:${branch} -f base=master -f body="
commit_package="${commit_prefix}${new_package}${commit_suffix}"
commit_version="${commit_prefix}${new_version}${commit_suffix}"
pull_package="${pull_prefix}${new_package}${pull_suffix}"
pull_version="${pull_prefix}${new_version}${pull_suffix}"
ref_call="gh api repos/releaser/winget-pkgs/git/refs -f ref=refs/heads/${branch} -f sha=${commit_id}"
patch_call="gh api --method PATCH repos/releaser/winget-pkgs/git/refs/heads/${branch} -f sha=${commit_id} -F force=true"
opened="Opened pull request #42 on microsoft/winget-pkgs for Aureliolo.SteamGauge ${version}."
already_open="A pull request for Aureliolo.SteamGauge ${version} is already open on microsoft/winget-pkgs: nothing to submit."

# The template with the two boxes the job proves ticked, as the pull request body carries it.
ticked="$(sed -e 's/^- \[ \] \(This PR only\)/- [x] \1/' -e 's/^- \[ \] \(Validated\)/- [x] \1/' "${work}/template.md")"

export FAKE_FORK=1 FAKE_UPSTREAM=0 FAKE_PACKAGE=0 FAKE_OPEN=none FAKE_DECOY=1 FAKE_BRANCH=0 FAKE_PROBE=ok

# A package upstream does not hold yet gets the "New package" title. One branch, one commit of
# exactly the three files and one pull request, with only the two boxes the job proves ticked.
# The decoy is an open pull request for version 1.2.30, which must not be taken for this one.
run_case new-package
expect_status new-package 0
expect_file new-package out "${opened}"
expect_file new-package err ""
expect_file new-package writes "${tree_call}
${tree_body}
${commit_package}
${ref_call}
${pull_package}${ticked} --jq .number"

# A package upstream already holds gets the "New version" title.
export FAKE_PACKAGE=1
run_case new-version
expect_status new-version 0
expect_file new-version out "${opened}"
expect_file new-version err ""
expect_file new-version writes "${tree_call}
${tree_body}
${commit_version}
${ref_call}
${pull_version}${ticked} --jq .number"
# The commit's parent is the fork's newest commit, never upstream's.
expect_calls new-version "repos/releaser/winget-pkgs/commits/HEAD" 1 "one read of the fork's newest commit"
expect_calls new-version "repos/microsoft/winget-pkgs/commits/HEAD" 0 "no read of upstream's newest commit"
expect_writes new-version "parents[]=${upstream_head_id}" 0 "no commit on upstream's newest commit"
expect_writes new-version "parents[]=${head_id}" 1 "one commit on the fork's newest commit"

# The version is already in winget-pkgs.
export FAKE_UPSTREAM=1
run_case upstream
expect_status upstream 0
expect_file upstream out "microsoft/winget-pkgs already holds Aureliolo.SteamGauge ${version}: nothing to submit."
expect_no_writes upstream
export FAKE_UPSTREAM=0

# A pull request is open on the version's branch, or open under another branch with either title.
for open in branch search search-version; do
  export FAKE_OPEN="${open}"
  run_case "open-${open}"
  expect_status "open-${open}" 0
  expect_file "open-${open}" out "${already_open}"
  expect_no_writes "open-${open}"
done
export FAKE_OPEN=none

# An earlier run failed after making the branch: it is moved to the new commit, not made again,
# and one pull request is opened.
export FAKE_BRANCH=1
run_case branch
expect_status branch 0
expect_file branch out "${opened}"
expect_file branch err ""
expect_writes branch "${tree_call}" 1 "one tree"
expect_writes branch "${commit_version}" 1 "one commit"
expect_writes branch "${patch_call}" 1 "the existing branch moved to the new commit"
expect_writes branch 'git/refs ' 0 "no second branch"
expect_writes branch "${pull_version}" 1 "one pull request"
export FAKE_BRANCH=0

# GitHub answers the upstream probe with neither 200 nor 404.
export FAKE_PROBE=error
run_case probe
expect_status probe 1
expect_file probe err "::error::GitHub did not answer whether Aureliolo.SteamGauge ${version} upstream exists."
expect_file probe out ""
expect_no_writes probe
export FAKE_PROBE=ok

# The manifest folder is wrong.
folder=two
run_case two-files
expect_status two-files 1
expect_file two-files err "::error::${work}/two holds 2 manifest files, not 3."
expect_no_writes two-files

folder=foreign
run_case foreign-file
expect_status foreign-file 1
expect_file foreign-file err "::error::${work}/foreign/Other.Package.yaml is not a manifest of Aureliolo.SteamGauge."
expect_no_writes foreign-file

folder=stale
run_case version-mismatch
expect_status version-mismatch 1
expect_file version-mismatch err "::error::${work}/stale/Aureliolo.SteamGauge.yaml is not for version ${version}."
expect_no_writes version-mismatch
folder=manifests

# A version that is not three numbers never reaches GitHub.
submit=1.2
run_case malformed
expect_status malformed 1
expect_file malformed err "::error::1.2 is not a release version."
expect_file malformed log ""
submit="${version}"

# The token's account has no fork, or a fork of something else.
for fork in 0 2; do
  export FAKE_FORK="${fork}"
  run_case "fork-${fork}"
  expect_status "fork-${fork}" 1
  expect_file "fork-${fork}" err "::error::The token's account has no fork of microsoft/winget-pkgs. Fork it to that account, as .github/release-process.md says."
  expect_file "fork-${fork}" out ""
  expect_no_writes "fork-${fork}"
done
export FAKE_FORK=1

if ((failures > 0)); then
  echo "${failures} check(s) failed." >&2
  exit 1
fi
echo "winget-submit.sh: every case passed."
