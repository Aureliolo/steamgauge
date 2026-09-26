// GitHub's REST API as the release workflows ask it: a request that failed on GitHub's side is
// asked again rather than read as an answer.
//
// A 503 read as "absent" is the dangerous case. The check for a taken tag would call the tag
// free, and the check that a tag exists would try to make it again. Only a status below 500 is
// an answer; anything else is retried, and named if the last attempt fails too.
import { realpathSync } from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

const ATTEMPTS = 5;
const FIRST_PAUSE_MS = 5_000;

// One request, by path under the API host. Resolves with whatever came back, errors included.
export function askGitHub(token) {
  return async (method, path, body) => {
    const response = await fetch(`https://api.github.com${path}`, {
      method,
      headers: {
        accept: "application/vnd.github+json",
        authorization: `Bearer ${token}`,
        "x-github-api-version": "2022-11-28",
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const text = await response.text();
    let parsed = text;
    try {
      parsed = JSON.parse(text);
    } catch {
      // Kept as text: an error page from a proxy is still worth printing.
    }
    return { status: response.status, body: parsed };
  };
}

export const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// `ask` until it answers with something other than a server error or nothing, waiting a little
// longer each time.
export async function answered(ask, wait, doing) {
  let last = "";
  for (let attempt = 1; attempt <= ATTEMPTS; attempt += 1) {
    try {
      const answer = await ask();
      if (answer.status < 500) {
        return answer;
      }
      last = `HTTP ${answer.status}: ${JSON.stringify(answer.body)}`;
    } catch (error) {
      last = error instanceof Error ? error.message : String(error);
    }
    if (attempt < ATTEMPTS) {
      console.error(`${doing}: attempt ${attempt} got ${last}; asking again.`);
      await wait(FIRST_PAUSE_MS * attempt);
    }
  }
  throw new Error(`${doing} failed on all ${ATTEMPTS} attempts; the last got ${last}`);
}

// The repository and token every script here reads.
export function fromEnvironment() {
  const repository = process.env.GH_REPO ?? process.env.GITHUB_REPOSITORY;
  const token = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
  if (!repository || !token) {
    throw new Error("GH_REPO (or GITHUB_REPOSITORY) and GH_TOKEN must both be set.");
  }
  return { repository, token };
}

// Whether the module at `url` is the script node was started with, rather than one a test
// imported.
export function isMain(url) {
  return process.argv[1] !== undefined && url === pathToFileURL(realpathSync(process.argv[1])).href;
}
