// The release version. It is written once, under [workspace.package] in the root Cargo.toml,
// every crate inherits it, and Cargo.lock carries a copy for each crate in the workspace. A tag
// is checked against that one number, so the three places it lives are checked against each
// other first: a crate that pinned its own version, or a lock file left behind by a bump, would
// otherwise ship a binary whose `--version` names a different release from its tag.
//
//   node tools/release/version.mjs check                       prints the version once all agree
//   node tools/release/version.mjs bump <patch|minor|major|X.Y.Z>
//   node tools/release/version.mjs of-manifest < Cargo.toml    prints the workspace version
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

import { isMain } from "./github.mjs";

const VERSION = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/;

export function parseVersion(text) {
  const match = VERSION.exec(text);
  if (match === null) {
    return null;
  }
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    pre: match[4] === undefined ? [] : match[4].split("."),
  };
}

// Negative when `left` is the older version, as semver orders them: a pre-release sorts below
// the release it leads to, and numeric identifiers compare as numbers.
export function compareVersions(left, right) {
  const a = parseVersion(left);
  const b = parseVersion(right);
  if (a === null || b === null) {
    throw new Error(`Not a version: ${a === null ? left : right}`);
  }
  for (const part of ["major", "minor", "patch"]) {
    if (a[part] !== b[part]) {
      return a[part] - b[part];
    }
  }
  if (a.pre.length === 0 || b.pre.length === 0) {
    return b.pre.length - a.pre.length;
  }
  for (let i = 0; i < Math.min(a.pre.length, b.pre.length); i += 1) {
    const [x, y] = [a.pre[i], b.pre[i]];
    if (x === y) {
      continue;
    }
    const [xn, yn] = [/^\d+$/.test(x), /^\d+$/.test(y)];
    if (xn && yn) {
      return Number(x) - Number(y);
    }
    if (xn !== yn) {
      return xn ? -1 : 1;
    }
    return x < y ? -1 : 1;
  }
  return a.pre.length - b.pre.length;
}

// The version after `current`. A pre-release has no obvious next patch, so it takes an exact
// version; so does anything that would not move the version forward, because a release that
// goes backwards or stands still is a tag that can never be made.
export function nextVersion(current, how) {
  const parsed = parseVersion(current);
  if (parsed === null) {
    throw new Error(`The current version is not one: ${current}`);
  }
  let next;
  if (how === "major" || how === "minor" || how === "patch") {
    if (parsed.pre.length > 0) {
      throw new Error(`${current} is a pre-release; name the exact version that follows it.`);
    }
    const { major, minor, patch } = parsed;
    next =
      how === "major" ? `${major + 1}.0.0` : how === "minor" ? `${major}.${minor + 1}.0` : `${major}.${minor}.${patch + 1}`;
  } else {
    if (parseVersion(how) === null) {
      throw new Error(`Not a version: ${how}`);
    }
    next = how;
  }
  if (compareVersions(next, current) <= 0) {
    throw new Error(`${next} does not come after ${current}.`);
  }
  return next;
}

// The lines of the [table] named, as a start and end index into `lines`.
function section(lines, table) {
  const start = lines.findIndex((line) => line.trim() === `[${table}]`);
  if (start === -1) {
    return null;
  }
  let end = start + 1;
  while (end < lines.length && !/^\s*\[/.test(lines[end])) {
    end += 1;
  }
  return { start: start + 1, end };
}

const VERSION_LINE = /^(\s*version\s*=\s*")([^"]*)(".*)$/;

export function workspaceVersion(manifest) {
  const lines = manifest.split("\n");
  const table = section(lines, "workspace.package");
  if (table === null) {
    return null;
  }
  for (let i = table.start; i < table.end; i += 1) {
    const match = VERSION_LINE.exec(lines[i]);
    if (match !== null) {
      return match[2];
    }
  }
  return null;
}

export function withWorkspaceVersion(manifest, version) {
  const lines = manifest.split("\n");
  const table = section(lines, "workspace.package");
  for (let i = table?.start ?? 0; table !== null && i < table.end; i += 1) {
    if (VERSION_LINE.test(lines[i])) {
      lines[i] = lines[i].replace(VERSION_LINE, `$1${version}$3`);
      return lines.join("\n");
    }
  }
  throw new Error("The root Cargo.toml has no version under [workspace.package].");
}

export function workspaceMembers(manifest) {
  const match = /^members\s*=\s*\[([^\]]*)\]/m.exec(manifest);
  if (match === null) {
    throw new Error("The root Cargo.toml lists no workspace members.");
  }
  const members = [...match[1].matchAll(/"([^"]+)"/g)].map((found) => found[1]);
  if (members.some((member) => member.includes("*"))) {
    throw new Error("Workspace members are globbed; list them, so each one's version is checked.");
  }
  return members;
}

// A member crate's name, and whether its version is the workspace's rather than its own.
export function memberPackage(manifest) {
  const lines = manifest.split("\n");
  const table = section(lines, "package");
  if (table === null) {
    throw new Error("A workspace member has no [package] table.");
  }
  const body = lines.slice(table.start, table.end);
  const name = body.map((line) => /^\s*name\s*=\s*"([^"]+)"/.exec(line)).find(Boolean)?.[1];
  const inherits = body.some(
    (line) => /^\s*version\.workspace\s*=\s*true\b/.test(line) || /^\s*version\s*=\s*\{\s*workspace\s*=\s*true\s*\}/.test(line),
  );
  return { name, inherits };
}

// Cargo.lock's [[package]] entries, each as its own text, the file's header first.
function lockBlocks(lock) {
  return lock.split(/\n(?=\[\[package\]\]\r?\n)/);
}

function blockField(block, field) {
  return new RegExp(`^${field} = "([^"]*)"`, "m").exec(block)?.[1];
}

// The version Cargo.lock records for each named crate of the workspace. Only entries without a
// source are the workspace's own: a crate of the same name from a registry is somebody else's.
export function lockVersions(lock, names) {
  const found = new Map();
  for (const block of lockBlocks(lock).slice(1)) {
    const name = blockField(block, "name");
    if (names.includes(name) && blockField(block, "source") === undefined) {
      found.set(name, blockField(block, "version"));
    }
  }
  return found;
}

export function withLockVersions(lock, names, version) {
  const blocks = lockBlocks(lock);
  let changed = 0;
  for (let i = 1; i < blocks.length; i += 1) {
    const name = blockField(blocks[i], "name");
    if (names.includes(name) && blockField(blocks[i], "source") === undefined) {
      blocks[i] = blocks[i].replace(/^version = "[^"]*"/m, `version = "${version}"`);
      changed += 1;
    }
  }
  if (changed !== names.length) {
    throw new Error(`Cargo.lock holds ${changed} of the workspace's ${names.length} crates.`);
  }
  return blocks.join("\n");
}

function read(root, path) {
  return readFileSync(join(root, path), "utf8");
}

// The workspace version, and every way the three places it lives disagree with it.
export function inspect(root) {
  const manifest = read(root, "Cargo.toml");
  const version = workspaceVersion(manifest);
  const problems = [];
  if (version === null) {
    problems.push("The root Cargo.toml has no version under [workspace.package].");
  } else if (parseVersion(version) === null) {
    problems.push(`The workspace version ${version} is not a version.`);
  }
  const names = [];
  for (const member of workspaceMembers(manifest)) {
    const { name, inherits } = memberPackage(read(root, `${member}/Cargo.toml`));
    if (name === undefined) {
      problems.push(`${member}/Cargo.toml names no package.`);
      continue;
    }
    names.push(name);
    if (!inherits) {
      problems.push(`${name} sets its own version rather than taking the workspace's.`);
    }
  }
  const locked = lockVersions(read(root, "Cargo.lock"), names);
  for (const name of names) {
    if (!locked.has(name)) {
      problems.push(`Cargo.lock has no entry for ${name}.`);
    } else if (locked.get(name) !== version) {
      problems.push(`Cargo.lock records ${name} ${locked.get(name)}, not ${version}.`);
    }
  }
  return { version, names, problems };
}

// Raises the version in both files and names them, for the commit that carries the change.
export function bump(root, how) {
  const { version, names, problems } = inspect(root);
  if (problems.length > 0) {
    throw new Error(problems.join("\n"));
  }
  const next = nextVersion(version, how);
  writeFileSync(join(root, "Cargo.toml"), withWorkspaceVersion(read(root, "Cargo.toml"), next));
  writeFileSync(join(root, "Cargo.lock"), withLockVersions(read(root, "Cargo.lock"), names, next));
  const after = inspect(root);
  if (after.problems.length > 0 || after.version !== next) {
    throw new Error(`The bump to ${next} left: ${after.problems.join(" ")}`);
  }
  return { from: version, to: next, files: ["Cargo.toml", "Cargo.lock"] };
}

function main() {
  const [command, argument] = process.argv.slice(2);
  const root = process.cwd();
  if (command === "check") {
    const { version, problems } = inspect(root);
    if (problems.length > 0) {
      throw new Error(problems.join("\n"));
    }
    console.log(version);
  } else if (command === "bump" && argument !== undefined) {
    const { from, to, files } = bump(root, argument);
    console.error(`Version ${from} -> ${to}`);
    console.log(`Updated: ${files.join(", ")}`);
  } else if (command === "of-manifest") {
    console.log(workspaceVersion(readFileSync(0, "utf8")) ?? "");
  } else {
    throw new Error("Usage: node tools/release/version.mjs check | bump <patch|minor|major|X.Y.Z> | of-manifest");
  }
}

if (isMain(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
