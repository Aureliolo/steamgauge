// Cargo.lock cut down to the crates one release archive was built from, for syft to read.
//
// The lock file holds every crate any platform, feature or test could want: the Windows
// bindings on Linux, the CoreML ones on Windows, and every dev-dependency everywhere. An SBOM
// read from all of it would claim each archive carries crates it never compiled. What a build
// actually used is what `cargo tree` resolves for its target and features, so the entries kept
// are exactly those, and a crate that resolution names but the lock does not hold is an error
// rather than a quiet omission.
//
//   cargo tree --locked -e normal,build --target <triple> [--features f] --prefix none --format '{p}' \
//     | awk '{ print $1, substr($2, 2) }' | sort -u > crates.txt
//   node tools/release/lock-subset.mjs Cargo.lock crates.txt > subset/Cargo.lock
import { readFileSync } from 'node:fs';
import process from 'node:process';

import { isMain } from './github.mjs';

const LINE_BREAK = /\r?\n/u;
const WHITESPACE_RUN = /\s+/u;
const LOCK_PACKAGE_BOUNDARY = /\n(?=\[\[package\]\]\r?\n)/u;
const LOCK_NAME_FIELD = /^name = "(?<name>[^"]*)"/mu;
const LOCK_VERSION_FIELD = /^version = "(?<version>[^"]*)"/mu;

// `name version` lines, as the awk above writes them.
function parseCrates(text) {
  const crates = new Set();
  for (const line of text.split(LINE_BREAK)) {
    const trimmed = line.trim();
    if (trimmed === '') {
      continue;
    }
    const [name, version, extra] = trimmed.split(WHITESPACE_RUN);
    if (version === undefined || extra !== undefined) {
      throw new Error(`Not a "name version" line: ${line}`);
    }
    crates.add(`${name} ${version}`);
  }
  return crates;
}

function lockSubset(lock, crates) {
  const blocks = lock.split(LOCK_PACKAGE_BOUNDARY);
  const kept = [blocks[0]];
  const found = new Set();
  for (const block of blocks.slice(1)) {
    const name = LOCK_NAME_FIELD.exec(block)?.groups.name;
    const version = LOCK_VERSION_FIELD.exec(block)?.groups.version;
    const key = `${name} ${version}`;
    if (crates.has(key)) {
      if (found.has(key)) {
        throw new Error(`Cargo.lock holds ${key} twice, so which one was built cannot be told.`);
      }
      found.add(key);
      kept.push(block);
    }
  }
  const missing = [...crates].filter((key) => !found.has(key));
  if (missing.length > 0) {
    throw new Error(`Cargo.lock does not hold: ${missing.join(', ')}`);
  }
  const text = kept.join('\n');
  return {
    kept: found.size,
    of: blocks.length - 1,
    text: text.endsWith('\n') ? text : `${text}\n`,
  };
}

if (isMain(import.meta.url)) {
  const [lockPath, cratesPath] = process.argv.slice(2);
  try {
    if (lockPath === undefined || cratesPath === undefined) {
      throw new Error('Usage: node tools/release/lock-subset.mjs <Cargo.lock> <crates.txt>');
    }
    const crates = parseCrates(readFileSync(cratesPath === '-' ? 0 : cratesPath, 'utf8'));
    if (crates.size === 0) {
      throw new Error(`${cratesPath} names no crates.`);
    }
    const { text, kept, of } = lockSubset(readFileSync(lockPath, 'utf8'), crates);
    process.stdout.write(text);
    console.error(`Kept ${kept} of the ${of} crates in ${lockPath}.`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

export { lockSubset, parseCrates };
