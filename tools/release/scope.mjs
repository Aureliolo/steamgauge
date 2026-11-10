// What a change between two commits touches, so a pull request that changes no code is not made
// to wait for builds main has already passed on the same code. Two kinds change none: one that
// only rewrites the package managers' manifests, which the release opens, and one that only
// moves SteamGauge's version, which prepare release opens. Anything else is code.
//
// A version change is proved, not guessed from the files it touches: Cargo.toml and Cargo.lock
// are also where a dependency moves, so the head's two files have to be exactly the base's with
// the workspace version raised and nothing else, byte for byte.
//
// Whether a change reaches what the fuzz targets are built from is a second question, asked with
// a third argument. A classifier that predates it ignores the argument and answers the first
// question, which is how the scope action tells that the base cannot answer it.
//
//   node tools/release/scope.mjs <base> <head>          prints code, version or packaging
//   node tools/release/scope.mjs <base> <head> fuzzed   prints true or false
import { execFileSync } from 'node:child_process';
import process from 'node:process';

import { isMain } from './github.mjs';
import {
  compareVersions,
  memberPackage,
  withLockVersions,
  withWorkspaceVersion,
  workspaceMembers,
  workspaceVersion,
} from './version.mjs';

const PACKAGING = /^(?:Casks|bucket)\//u;
const VERSIONED = new Set(['Cargo.toml', 'Cargo.lock']);

// Everything fuzz/Cargo.toml and .clusterfuzzlite/build.sh build the targets from: the harness
// and its image, the core crate they depend on by path, the workspace manifest that crate
// inherits its version and lints from, the lock file build.sh copies, the toolchain build.sh
// reads, cargo's own configuration, and the workflow that runs them.
const FUZZ_FOLDERS = /^(?:\.cargo|\.clusterfuzzlite|crates\/steamgauge-core|fuzz)\//u;
const FUZZ_FILES = new Set([
  '.github/workflows/fuzz.yml',
  'Cargo.lock',
  'Cargo.toml',
  'rust-toolchain.toml',
]);

// `before` and `after` read a file's text at each commit, or throw where it is absent.
function scopeOf(changed, before, after) {
  if (changed.length === 0) {
    return 'code';
  }
  if (changed.every((path) => PACKAGING.test(path))) {
    return 'packaging';
  }
  if (changed.every((path) => VERSIONED.has(path)) && onlyTheVersion(before, after)) {
    return 'version';
  }
  return 'code';
}

// A change that lists no path is not known to leave the targets alone.
function fuzzedBy(changed) {
  return (
    changed.length === 0 || changed.some((path) => FUZZ_FOLDERS.test(path) || FUZZ_FILES.has(path))
  );
}

function onlyTheVersion(before, after) {
  try {
    const manifest = after('Cargo.toml');
    const from = workspaceVersion(before('Cargo.toml'));
    const to = workspaceVersion(manifest);
    if (from === null || to === null || compareVersions(to, from) <= 0) {
      return false;
    }
    const names = workspaceMembers(manifest).map(
      (member) => memberPackage(after(`${member}/Cargo.toml`)).name,
    );
    return (
      withWorkspaceVersion(before('Cargo.toml'), to) === manifest &&
      withLockVersions(before('Cargo.lock'), names, to) === after('Cargo.lock')
    );
  } catch {
    return false;
  }
}

function git(...args) {
  return execFileSync('git', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
}

if (isMain(import.meta.url)) {
  const [base, head, question] = process.argv.slice(2);
  if (base === undefined || head === undefined) {
    console.error('usage: scope.mjs <base> <head>');
    process.exit(2);
  }
  const changed = git('diff', '--name-only', '--no-renames', base, head)
    .split('\n')
    .filter(Boolean);
  const at = (commit) => (path) => git('show', `${commit}:${path}`);
  console.log(question === 'fuzzed' ? fuzzedBy(changed) : scopeOf(changed, at(base), at(head)));
}

export { fuzzedBy, scopeOf };
