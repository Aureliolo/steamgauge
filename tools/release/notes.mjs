// The changelog half of a release's notes, split by whether an entry reaches the binary
// somebody downloads.
//
// A flat list of pull request titles, which is what `gh release create --generate-notes`
// writes, describes each change and not its reach. Most of what lands here is training runs,
// reference data, the browser checks and CI; a reader deciding whether a release is worth
// taking has to guess which titles changed the program they run, and a wrong guess costs both
// ways: an upgrade for a change that touches nothing they install, and a skipped release that
// fixed their problem. So the split is computed from the files each pull request touched, not
// from its title or a label somebody remembers to add.
//
//   GH_TOKEN="$(gh auth token)" GITHUB_REPOSITORY=Aureliolo/steamgauge \
//     node tools/release/notes.mjs v0.1.0 [<commit>]
//
// The commit defaults to the tag; a dry run passes the commit it built, whose tag does not exist
// yet. Needs the history down to the previous release, so a checkout with fetch-depth 0.
import { execFileSync } from "node:child_process";
import process from "node:process";

import { answered, askGitHub, fromEnvironment, isMain, pause } from "./github.mjs";
import { compareVersions, parseVersion } from "./version.mjs";

// What decides the bytes of a release archive. Everything under crates/ is compiled in or
// shipped beside the binary, except the examples and tests, which are neither. The toolchain
// file picks the compiler, release-build.yml assembles the archive and says what goes in, and
// third-party/ with notices.mjs writes the THIRD-PARTY-NOTICES.txt each archive carries.
const SHIPPED = [
  "crates/",
  "Cargo.toml",
  "Cargo.lock",
  "rust-toolchain.toml",
  "README.md",
  "LICENSE",
  "third-party/",
  "tools/release/notices.mjs",
  ".github/workflows/release-build.yml",
];
const NEVER_SHIPPED = /^crates\/[^/]+\/(examples|tests|benches)\//;

// The label release-prepare.yml puts on the pull request that only raises the version.
const THE_RELEASE_ITSELF = "release";

// One shipped file is enough. A pull request that fixes the reader and adds the fixture for it
// is a change you download, and listing it anywhere else would be the same misreading reversed.
export function shipsInDownload(files) {
  return files.some(
    (file) =>
      !NEVER_SHIPPED.test(file) &&
      SHIPPED.some((shipped) => (shipped.endsWith("/") ? file.startsWith(shipped) : file === shipped)),
  );
}

// The number GitHub appends to a squashed pull request's subject.
export function pullRequestNumber(subject) {
  const match = /\(#(\d+)\)\s*$/.exec(subject);
  return match === null ? null : Number(match[1]);
}

// The newest release strictly below `version`, rather than simply the newest one. At release
// time those agree, since the tag has no release yet; pointed at a tag already published, the
// second compares against the release itself, finds nothing between, and says a release changed
// nothing. A changelog that is only right at one moment cannot be checked afterwards.
export function previousRelease(tags, version) {
  return (
    tags
      .filter((tag) => tag.startsWith("v") && parseVersion(tag.slice(1)) !== null)
      .filter((tag) => compareVersions(tag.slice(1), version) < 0)
      .sort((left, right) => compareVersions(right.slice(1), left.slice(1)))[0] ?? null
  );
}

export function renderNotes(entries, repository, from, to) {
  const lines = ["## What's Changed", ""];
  const sections = [
    ["In what you download", true],
    ["Repository only: training, reference data, tools, CI and docs", false],
  ];
  for (const [heading, ships] of sections) {
    const listed = entries.filter((entry) => entry.ships === ships);
    if (listed.length === 0) {
      continue;
    }
    lines.push(`### ${heading}`, "");
    for (const entry of listed) {
      lines.push(entry.author === null ? `* ${entry.title} in ${entry.url}` : `* ${entry.title} by @${entry.author} in ${entry.url}`);
    }
    lines.push("");
  }
  if (entries.length === 0) {
    lines.push("Nothing has changed since the previous release.", "");
  }
  lines.push(
    from === null
      ? `**Full Changelog**: https://github.com/${repository}/commits/${to}`
      : `**Full Changelog**: https://github.com/${repository}/compare/${from}...${to}`,
  );
  return `${lines.join("\n")}\n`;
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
}

async function get(ask, path) {
  const answer = await answered(() => ask("GET", path), pause, `GET ${path}`);
  if (answer.status !== 200) {
    throw new Error(`GET ${path} answered ${answer.status}: ${JSON.stringify(answer.body)}`);
  }
  return answer.body;
}

async function releaseTags(ask, repository) {
  const tags = [];
  for (let page = 1; ; page += 1) {
    const releases = await get(ask, `/repos/${repository}/releases?per_page=100&page=${page}`);
    tags.push(...releases.filter((release) => !release.draft).map((release) => release.tag_name));
    if (releases.length < 100) {
      return tags;
    }
  }
}

async function main() {
  const [tag, commit = tag] = process.argv.slice(2);
  if (tag === undefined || !tag.startsWith("v") || parseVersion(tag.slice(1)) === null) {
    throw new Error("Usage: node tools/release/notes.mjs v<version> [<commit>]");
  }
  const { repository, token } = fromEnvironment();
  const ask = askGitHub(token);

  const previous = previousRelease(await releaseTags(ask, repository), tag.slice(1));
  const range = previous === null ? commit : `${previous}..${commit}`;

  // Oldest first, and grouped by pull request: a pull request merged by rebase lands as several
  // commits, and it is one change.
  const changes = new Map();
  for (const line of git("log", "--reverse", "--format=%H %s", range).split("\n")) {
    if (line === "") {
      continue;
    }
    const sha = line.slice(0, line.indexOf(" "));
    const subject = line.slice(line.indexOf(" ") + 1);
    let number = pullRequestNumber(subject);
    if (number === null) {
      const pulls = await get(ask, `/repos/${repository}/commits/${sha}/pulls`);
      number = pulls.find((pull) => pull.merged_at !== null)?.number ?? null;
    }
    const key = number === null ? sha : `#${number}`;
    const files = git("diff-tree", "--root", "--no-commit-id", "--name-only", "-r", "-z", sha).split("\0").filter(Boolean);
    const change = changes.get(key) ?? { number, sha, subject, files: [] };
    change.files.push(...files);
    changes.set(key, change);
  }

  const entries = [];
  for (const change of changes.values()) {
    if (change.number === null) {
      entries.push({
        title: change.subject,
        author: null,
        url: `https://github.com/${repository}/commit/${change.sha}`,
        ships: shipsInDownload(change.files),
      });
      continue;
    }
    const pull = await get(ask, `/repos/${repository}/pulls/${change.number}`);
    if (pull.labels.some((label) => label.name === THE_RELEASE_ITSELF)) {
      continue;
    }
    entries.push({
      title: pull.title,
      author: pull.user.login,
      url: pull.html_url,
      ships: shipsInDownload(change.files),
    });
  }

  process.stdout.write(renderNotes(entries, repository, previous, tag));
}

if (isMain(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
