// Commits the given files to a branch through GitHub's GraphQL API.
//
// Every branch requires signed commits, and a commit made with git on a runner is not signed:
// there is no key there to sign with, and putting one there would mean a signing key living in
// CI. createCommitOnBranch is the way out, because GitHub signs what it commits on the
// workflow's behalf, so the ruleset stays strict and nothing has to be excepted from it.
//
// Not retried. The commit names the head it expects, so a second attempt after a first that
// landed would be refused anyway, and a refusal is clearer than a guess about which one won.
//
//   EXPECTED_HEAD_OID=<sha> node tools/release/commit-signed.mjs <branch> <message> <file>...
import { readFileSync } from "node:fs";
import process from "node:process";

import { fromEnvironment } from "./github.mjs";

const QUERY = `
  mutation ($input: CreateCommitOnBranchInput!) {
    createCommitOnBranch(input: $input) {
      commit { oid url }
    }
  }
`;

async function main() {
  const [branch, message, ...files] = process.argv.slice(2);
  const expectedHeadOid = process.env.EXPECTED_HEAD_OID;
  if (!branch || !message || files.length === 0 || !expectedHeadOid) {
    throw new Error("Usage: EXPECTED_HEAD_OID=<sha> node tools/release/commit-signed.mjs <branch> <message> <file>...");
  }
  const { repository, token } = fromEnvironment();
  const [headline, ...rest] = message.split("\n\n");
  const body = rest.join("\n\n");

  const input = {
    branch: { repositoryNameWithOwner: repository, branchName: branch },
    expectedHeadOid,
    message: body ? { headline, body } : { headline },
    fileChanges: {
      additions: files.map((path) => ({ path, contents: readFileSync(path).toString("base64") })),
    },
  };

  const response = await fetch("https://api.github.com/graphql", {
    method: "POST",
    headers: { authorization: `bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify({ query: QUERY, variables: { input } }),
  });
  const payload = await response.json();
  if (!response.ok || payload.errors) {
    throw new Error(JSON.stringify(payload.errors ?? payload, null, 2));
  }
  const commit = payload.data?.createCommitOnBranch?.commit;
  if (!commit) {
    throw new Error(`GitHub answered without a commit: ${JSON.stringify(payload)}`);
  }
  console.log(`${commit.oid} ${commit.url}`);
}

try {
  await main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
