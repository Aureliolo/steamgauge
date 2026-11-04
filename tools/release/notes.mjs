// The changelog half of a release's notes, split by whether an entry reaches the binary
// somebody downloads.
//
// A flat list of pull request titles, which is what `gh release create --generate-notes`
// writes, describes each change and not its reach. Most of what lands here is training runs,
// reference data, the browser checks and CI. A reader deciding whether a release is worth
// taking has to guess which titles changed the program they run, and a wrong guess costs both
// ways: an upgrade for a change that touches nothing they install, and a skipped release that
// fixed their problem. So the split is computed from the files each pull request touched, not
// from its title or a label somebody remembers to add.
//
//   GH_TOKEN="$(gh auth token)" GITHUB_REPOSITORY=Aureliolo/steamgauge \
//     node tools/release/notes.mjs v0.1.0 [<commit>]
//
// The commit defaults to the tag. A dry run passes the commit it built, whose tag does not exist
// yet. Needs the history down to the previous release, so a checkout with fetch-depth 0.
import { execFileSync } from 'node:child_process';
import process from 'node:process';

import { answered, askGitHub, fromEnvironment, isMain, pause } from './github.mjs';
import { compareVersions, parseVersion } from './version.mjs';

// What decides the bytes of a release archive. Everything under crates/ is compiled in or
// shipped beside the binary, except the examples and tests, which are neither. The toolchain
// file picks the compiler, release-build.yml and package.sh assemble the archives and the
// installers and say what goes in, runtime-dlls.mjs picks the C++ runtime that ships beside the
// Windows binary, third-party/ with notices.mjs writes the THIRD-PARTY-NOTICES.txt each archive
// carries, and package-managers.sh writes the Scoop manifest the release carries.
const SHIPPED = [
  'crates/',
  'Cargo.toml',
  'Cargo.lock',
  'rust-toolchain.toml',
  'README.md',
  'LICENSE',
  'third-party/',
  'tools/release/notices.mjs',
  'tools/release/package.sh',
  'tools/release/package-managers.sh',
  'tools/release/runtime-dlls.mjs',
  '.github/workflows/release-build.yml',
];
const NEVER_SHIPPED = /^crates\/[^/]+\/(?:examples|tests|benches)\//u;
const PULL_REQUEST_SUFFIX = /\(#(?<number>\d+)\)\s*$/u;

// The label on the pull requests a release makes for itself: release-prepare.yml's, which only
// raises the version, and the one that puts the Homebrew cask and the Scoop manifest on main.
const THE_RELEASE_ITSELF = 'release';

// One shipped file is enough. A pull request that fixes the reader and adds the fixture for it
// is a change you download, and listing it anywhere else would be the same misreading reversed.
function shipsInDownload(files) {
  return files.some(
    (file) =>
      !NEVER_SHIPPED.test(file) &&
      SHIPPED.some((shipped) =>
        shipped.endsWith('/') ? file.startsWith(shipped) : file === shipped,
      ),
  );
}

// The number GitHub appends to a squashed pull request's subject.
function pullRequestNumber(subject) {
  const match = PULL_REQUEST_SUFFIX.exec(subject);
  return match === null ? null : Number(match.groups.number);
}

// The newest release strictly below `version`, rather than simply the newest one. At release
// time those agree, since the tag has no release yet. Pointed at a tag already published, the
// second compares against the release itself, finds nothing between, and says a release changed
// nothing. A changelog that is only right at one moment cannot be checked afterwards.
function previousRelease(tags, version) {
  return (
    tags
      .filter((tag) => tag.startsWith('v') && parseVersion(tag.slice(1)) !== null)
      .filter((tag) => compareVersions(tag.slice(1), version) < 0)
      .sort((left, right) => compareVersions(right.slice(1), left.slice(1)))[0] ?? null
  );
}

function renderNotes(entries, repository, from, to) {
  // Below the first release lies every pull request the repository ever merged, which is a
  // history rather than release notes.
  if (from === null) {
    return `## What's Changed\n\nThe first release.\n\n**Full Changelog**: https://github.com/${repository}/commits/${to}\n`;
  }
  const lines = ["## What's Changed", ''];
  const sections = [
    ['In what you download', true],
    ['Repository only: training, reference data, tools, CI and docs', false],
  ];
  for (const [heading, ships] of sections) {
    const listed = entries.filter((entry) => entry.ships === ships);
    if (listed.length === 0) {
      continue;
    }
    lines.push(`### ${heading}`, '');
    for (const entry of listed) {
      lines.push(
        entry.author === null
          ? `* ${entry.title} in ${entry.url}`
          : `* ${entry.title} by @${entry.author} in ${entry.url}`,
      );
    }
    lines.push('');
  }
  if (entries.length === 0) {
    lines.push('Nothing has changed since the previous release.', '');
  }
  lines.push(`**Full Changelog**: https://github.com/${repository}/compare/${from}...${to}`);
  return `${lines.join('\n')}\n`;
}

function git(...args) {
  return execFileSync('git', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
}

async function get(ask, path) {
  const answer = await answered(() => ask('GET', path), pause, `GET ${path}`);
  if (answer.status !== 200) {
    throw new Error(`GET ${path} answered ${answer.status}: ${JSON.stringify(answer.body)}`);
  }
  return answer.body;
}

async function releaseTags(ask, repository) {
  const tags = [];
  for (let page = 1; ; page += 1) {
    // biome-ignore lint/performance/noAwaitInLoops: whether another page exists is known only from the one before
    const releases = await get(ask, `/repos/${repository}/releases?per_page=100&page=${page}`);
    tags.push(...releases.filter((release) => !release.draft).map((release) => release.tag_name));
    if (releases.length < 100) {
      return tags;
    }
  }
}

async function main() {
  const [tag, commit = tag] = process.argv.slice(2);
  if (tag === undefined || !tag.startsWith('v') || parseVersion(tag.slice(1)) === null) {
    throw new Error('Usage: node tools/release/notes.mjs v<version> [<commit>]');
  }
  const { repository, token } = fromEnvironment();
  const ask = askGitHub(token);

  const previous = previousRelease(await releaseTags(ask, repository), tag.slice(1));
  if (previous === null) {
    process.stdout.write(renderNotes([], repository, null, tag));
    return;
  }
  const range = `${previous}..${commit}`;

  const changes = await changesIn(ask, repository, range);
  const entries = await entriesOf(ask, repository, changes);
  process.stdout.write(renderNotes(entries, repository, previous, tag));
}

// Oldest first, and grouped by pull request: a pull request merged by rebase lands as several
// commits, and it is one change.
async function changesIn(ask, repository, range) {
  const changes = new Map();
  for (const line of git('log', '--reverse', '--format=%H %s', range).split('\n')) {
    if (line === '') {
      continue;
    }
    const sha = line.slice(0, line.indexOf(' '));
    const subject = line.slice(line.indexOf(' ') + 1);
    let number = pullRequestNumber(subject);
    if (number === null) {
      // biome-ignore lint/performance/noAwaitInLoops: one request at a time keeps to GitHub's secondary rate limit
      const pulls = await get(ask, `/repos/${repository}/commits/${sha}/pulls`);
      number = pulls.find((pull) => pull.merged_at !== null)?.number ?? null;
    }
    const key = number === null ? sha : `#${number}`;
    const files = git('diff-tree', '--root', '--no-commit-id', '--name-only', '-r', '-z', sha)
      .split('\0')
      .filter(Boolean);
    const change = changes.get(key) ?? { files: [], number, sha, subject };
    change.files.push(...files);
    changes.set(key, change);
  }
  return changes;
}

async function entriesOf(ask, repository, changes) {
  const entries = [];
  for (const change of changes.values()) {
    if (change.number === null) {
      entries.push({
        author: null,
        ships: shipsInDownload(change.files),
        title: change.subject,
        url: `https://github.com/${repository}/commit/${change.sha}`,
      });
      continue;
    }
    // biome-ignore lint/performance/noAwaitInLoops: one request at a time keeps to GitHub's secondary rate limit
    const pull = await get(ask, `/repos/${repository}/pulls/${change.number}`);
    if (pull.labels.some((label) => label.name === THE_RELEASE_ITSELF)) {
      continue;
    }
    entries.push({
      author: pull.user.login,
      ships: shipsInDownload(change.files),
      title: pull.title,
      url: pull.html_url,
    });
  }
  return entries;
}

if (isMain(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

export { previousRelease, pullRequestNumber, renderNotes, shipsInDownload };
