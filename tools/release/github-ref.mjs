// Reads and makes the git refs a release is built on: whether a tag or a branch exists, and
// creating one.
//
// One `gh api` call whose failure is read as "absent" is exactly wrong here: a 503 would call a
// taken tag free and send a release after a version that can never be tagged. Only a 404 is
// absent, and anything else is asked again (github.mjs).
//
// Creating can be repeated: a ref that already exists at the commit asked for is the ref wanted,
// so a run retried after its create reached GitHub and the answer did not come back still
// succeeds. A ref at another commit is refused, because moving it is not what was asked.
//
//   node tools/release/github-ref.mjs state tags/v0.1.0             prints present or absent
//   node tools/release/github-ref.mjs create heads/release/v0.1.0 <sha>
//   node tools/release/github-ref.mjs reset heads/release/v0.1.0 <sha>   moves it if it exists
import process from "node:process";

import { answered, askGitHub, fromEnvironment, isMain, pause } from "./github.mjs";

function shaOf(body) {
  const sha = body?.object?.sha;
  return typeof sha === "string" ? sha : null;
}

export async function refState(ask, repository, ref, wait) {
  const answer = await answered(() => ask("GET", `/repos/${repository}/git/ref/${ref}`), wait, `Reading ${ref}`);
  if (answer.status === 200) {
    return "present";
  }
  if (answer.status === 404) {
    return "absent";
  }
  throw new Error(`Reading ${ref} got HTTP ${answer.status}: ${JSON.stringify(answer.body)}`);
}

export async function createRef(ask, repository, ref, sha, wait) {
  const answer = await answered(
    () => ask("POST", `/repos/${repository}/git/refs`, { ref: `refs/${ref}`, sha }),
    wait,
    `Creating ${ref}`,
  );
  if (answer.status === 201) {
    return "created";
  }
  if (answer.status === 422) {
    const existing = await answered(() => ask("GET", `/repos/${repository}/git/ref/${ref}`), wait, `Reading ${ref}`);
    const at = existing.status === 200 ? shaOf(existing.body) : null;
    if (at === sha) {
      return "existed";
    }
    throw new Error(`${ref} already exists at ${at ?? "a commit that could not be read"}, not ${sha}.`);
  }
  throw new Error(`Creating ${ref} got HTTP ${answer.status}: ${JSON.stringify(answer.body)}`);
}

// A branch set to `sha` whether or not it existed, for a branch this tooling owns: a release
// branch left by a run that stopped half way is started again rather than refused.
export async function resetRef(ask, repository, ref, sha, wait) {
  if ((await refState(ask, repository, ref, wait)) === "absent") {
    return createRef(ask, repository, ref, sha, wait);
  }
  const answer = await answered(
    () => ask("PATCH", `/repos/${repository}/git/refs/${ref}`, { sha, force: true }),
    wait,
    `Resetting ${ref}`,
  );
  if (answer.status === 200 && shaOf(answer.body) === sha) {
    return "reset";
  }
  throw new Error(`Resetting ${ref} got HTTP ${answer.status}: ${JSON.stringify(answer.body)}`);
}

if (isMain(import.meta.url)) {
  const [command, ref, sha] = process.argv.slice(2);
  try {
    const writes = command === "create" || command === "reset";
    if (ref === undefined || !(command === "state" || (writes && sha !== undefined))) {
      throw new Error("Usage: node tools/release/github-ref.mjs state <ref> | create <ref> <sha> | reset <ref> <sha>");
    }
    const { repository, token } = fromEnvironment();
    const ask = askGitHub(token);
    const act = {
      state: () => refState(ask, repository, ref, pause),
      create: () => createRef(ask, repository, ref, sha, pause),
      reset: () => resetRef(ask, repository, ref, sha, pause),
    };
    console.log(await act[command]());
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
