// Whether the code a release ships passed every check the branch ruleset requires, read from the
// pull request that brought it to main rather than run again.
//
// The release commit only raises the version (scope.mjs), so the code it ships is its parent's.
// A parent that itself holds no code, the package managers' manifests merged after the last
// release or another version raise, ships its own parent's code, and its pull request had its
// builds skipped for that reason; so the walk goes back over those to the first commit that
// changed code. That commit's pull request has to have run every required check to success on a
// head whose tree is the commit's own: main requires a branch to be up to date before it merges,
// so the squash of a passing head is exactly what was checked.
//
//   GH_REPO=<owner/repo> GH_TOKEN=<token> node tools/release/checked.mjs <release commit>
import { execFileSync } from "node:child_process";
import process from "node:process";

import { answered, askGitHub, fromEnvironment, isMain, pause } from "./github.mjs";
import { scopeOf } from "./scope.mjs";

// More commits in a row with no code than any run of releases and manifest updates leaves.
const MOST_WITHOUT_CODE = 50;

// `look` answers each question about the history:
//   parent(commit)        the commit before it on main
//   kind(commit)          scope.mjs's verdict on what the commit changed against its parent
//   merged(commit)        { number, head } of the pull request squashed into it, or null
//   tree(commit)          its tree
//   checks(head)          Map of check name to its latest conclusion on that head
//   required              the names the ruleset requires
export async function checkedCode(look, release) {
  if ((await look.kind(release)) !== "version") {
    throw new Error(`The release commit ${release} changes more than the version.`);
  }
  let commit = await look.parent(release);
  for (let step = 0; step < MOST_WITHOUT_CODE; step += 1) {
    const pull = await look.merged(commit);
    if (pull === null) {
      throw new Error(`${commit} came to main without a pull request, so no check ran on it.`);
    }
    if ((await look.kind(commit)) !== "code") {
      commit = await look.parent(commit);
      continue;
    }
    if ((await look.tree(pull.head)) !== (await look.tree(commit))) {
      throw new Error(`#${pull.number}'s checked head ${pull.head} is not the code ${commit} holds.`);
    }
    const checks = await look.checks(pull.head);
    const failed = look.required.filter((name) => checks.get(name) !== "success");
    if (failed.length > 0) {
      const states = failed.map((name) => `${name}: ${checks.get(name) ?? "never ran"}`);
      throw new Error(`#${pull.number} did not pass every required check on ${pull.head}: ${states.join(", ")}.`);
    }
    return { commit, pull: pull.number };
  }
  throw new Error(`No commit changing code in the ${MOST_WITHOUT_CODE} before ${release}.`);
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }).trim();
}

async function main() {
  const [release] = process.argv.slice(2);
  if (release === undefined) {
    throw new Error("usage: checked.mjs <release commit>");
  }
  const { repository, token } = fromEnvironment();
  const ask = askGitHub(token);
  const get = async (path, doing) => {
    const answer = await answered(() => ask("GET", path), pause, doing);
    if (answer.status !== 200) {
      throw new Error(`${doing}: HTTP ${answer.status}: ${JSON.stringify(answer.body)}`);
    }
    return answer.body;
  };

  const rules = await get(`/repos/${repository}/rules/branches/main`, "Reading main's rules");
  const required = rules
    .filter((rule) => rule.type === "required_status_checks")
    .flatMap((rule) => rule.parameters.required_status_checks.map((check) => check.context));
  if (required.length === 0) {
    throw new Error("main requires no checks, so there is nothing a release could be held to.");
  }

  const look = {
    required,
    parent: async (commit) => git("rev-parse", `${commit}^`),
    kind: async (commit) => {
      const parent = git("rev-parse", `${commit}^`);
      const changed = git("diff", "--name-only", "--no-renames", parent, commit).split("\n").filter(Boolean);
      const at = (rev) => (path) => execFileSync("git", ["show", `${rev}:${path}`], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
      return scopeOf(changed, at(parent), at(commit));
    },
    merged: async (commit) => {
      const pulls = await get(`/repos/${repository}/commits/${commit}/pulls`, `Finding the pull request of ${commit}`);
      const pull = pulls.find((found) => found.merge_commit_sha === commit);
      return pull === undefined ? null : { number: pull.number, head: pull.head.sha };
    },
    tree: async (commit) => (await get(`/repos/${repository}/git/commits/${commit}`, `Reading ${commit}`)).tree.sha,
    checks: async (head) => {
      const latest = new Map();
      for (let page = 1; ; page += 1) {
        const body = await get(`/repos/${repository}/commits/${head}/check-runs?per_page=100&page=${page}`, `Reading the checks on ${head}`);
        for (const run of body.check_runs) {
          const seen = latest.get(run.name);
          if (seen === undefined || (run.completed_at ?? "") > seen.at) {
            latest.set(run.name, { at: run.completed_at ?? "", conclusion: run.conclusion ?? run.status });
          }
        }
        if (body.check_runs.length < 100) {
          break;
        }
      }
      return new Map([...latest].map(([name, { conclusion }]) => [name, conclusion]));
    },
  };
  const { commit, pull } = await checkedCode(look, release);
  console.log(`The code ${release} ships is ${commit}'s, which passed every required check on #${pull}.`);
}

if (isMain(import.meta.url)) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
