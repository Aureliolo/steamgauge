// What a change between two commits touches, so a pull request that changes no code is not made
// to wait for builds main has already passed on the same code. Two kinds change none: one that
// only rewrites the package managers' manifests, which the release opens, and one that only
// moves SteamGauge's version, which prepare release opens. Anything else is code.
//
// A version change is proved, not guessed from the files it touches: Cargo.toml and Cargo.lock
// are also where a dependency moves, so the head's two files have to be exactly the base's with
// the workspace version raised and nothing else, byte for byte.
//
//   node tools/release/scope.mjs <base> <head>   prints code, version or packaging
import { execFileSync } from "node:child_process";
import process from "node:process";

import { isMain } from "./github.mjs";
import {
  compareVersions,
  memberPackage,
  withLockVersions,
  withWorkspaceVersion,
  workspaceMembers,
  workspaceVersion,
} from "./version.mjs";

const PACKAGING = /^(Casks|bucket)\//;
const VERSIONED = new Set(["Cargo.toml", "Cargo.lock"]);

// `before` and `after` read a file's text at each commit, or throw where it is absent.
export function scopeOf(changed, before, after) {
  if (changed.length === 0) {
    return "code";
  }
  if (changed.every((path) => PACKAGING.test(path))) {
    return "packaging";
  }
  if (changed.every((path) => VERSIONED.has(path)) && onlyTheVersion(before, after)) {
    return "version";
  }
  return "code";
}

function onlyTheVersion(before, after) {
  try {
    const manifest = after("Cargo.toml");
    const from = workspaceVersion(before("Cargo.toml"));
    const to = workspaceVersion(manifest);
    if (from === null || to === null || compareVersions(to, from) <= 0) {
      return false;
    }
    const names = workspaceMembers(manifest).map((member) => memberPackage(after(`${member}/Cargo.toml`)).name);
    return (
      withWorkspaceVersion(before("Cargo.toml"), to) === manifest &&
      withLockVersions(before("Cargo.lock"), names, to) === after("Cargo.lock")
    );
  } catch {
    return false;
  }
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
}

if (isMain(import.meta.url)) {
  const [base, head] = process.argv.slice(2);
  if (base === undefined || head === undefined) {
    console.error("usage: scope.mjs <base> <head>");
    process.exit(2);
  }
  const changed = git("diff", "--name-only", "--no-renames", base, head).split("\n").filter(Boolean);
  const at = (commit) => (path) => git("show", `${commit}:${path}`);
  console.log(scopeOf(changed, at(base), at(head)));
}
