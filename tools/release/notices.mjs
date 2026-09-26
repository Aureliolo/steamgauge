// THIRD-PARTY-NOTICES.txt, which every release archive carries beside LICENSE: the licence of
// each crate compiled into the binary and the notices those licences ask to travel with a copy,
// then ONNX Runtime's, which the binary is linked with, and DirectML's where DirectML.dll ships.
//
//   node tools/release/notices.mjs write <cargo-about> <target> <feature or ""> <output>
//   node tools/release/notices.mjs check <target> <notices> <crates.txt> <Cargo.lock> <archive dir>
//
// `write` resolves the crates the way the build does, for one target and its GPU feature from
// Cargo.lock, through cargo-about and third-party/about.toml. It refuses a crate whose licence
// is not accepted there, and one whose text is SPDX's template for a licence that names a
// copyright holder: cargo-about puts the template in without a word when a crate ships no
// licence file it recognises, and a template with the holder left blank is no notice at all.
// `check` reads an unpacked archive back: the notices must name every crate its build
// resolved that did not come from this repository, and every file in it must be one whose
// licence the notices or LICENSE cover.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { isMain } from "./github.mjs";
import { parseCrates } from "./lock-subset.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const THIRD_PARTY = join(ROOT, "third-party");
const APP_MANIFEST = join(ROOT, "crates", "steamgauge-app", "Cargo.toml");

// What third-party/ holds for the two components no crate carries the licence of. A new ONNX
// Runtime arrives with an ort-sys update, and `write` refuses it until the texts here are
// replaced with that version's and the version below with it.
export const ONNX_RUNTIME = {
  heading: "ONNX Runtime",
  version: "1.28.0",
  source: "https://github.com/microsoft/onnxruntime/tree/v1.28.0",
  files: ["onnxruntime/LICENSE", "onnxruntime/ThirdPartyNotices.txt"],
};

export const DIRECTML = {
  heading: "DirectML",
  version: "1.15.4",
  source: "https://www.nuget.org/packages/Microsoft.AI.DirectML/1.15.4",
  files: ["directml/LICENSE.txt", "directml/LICENSE-CODE.txt", "directml/ThirdPartyNotices.txt"],
  // bin/x64-win/DirectML.dll in that package, byte for byte the file ort-sys puts beside the
  // binary. A different one is a different DirectML, whose terms have not been read.
  dll: new Map([["x86_64-pc-windows-msvc", "9c9e6d822561c6c41b90e6994b3e8857cf1d66dbfb1e0c4c799c7c89b4e92da1"]]),
};

// Licences whose text names no copyright holder, so SPDX's copy of one is the licence itself.
const WITHOUT_A_HOLDER = new Set(["Apache-2.0", "MPL-2.0", "CDLA-Permissive-2.0"]);

// What cargo-about says, on a line of its own, when a clarification in about.toml no longer
// matches the file it names or a crate could not be read. Either leaves a crate to whatever
// the scan finds, which is how a changed licence would pass unread.
const REFUSED_WARNINGS = [/failed to validate all files specified in clarification/, /unable to scan for license files/];

const RULE = "=".repeat(80);
const THIN = "-".repeat(80);
const CRATES_HEADING = "1. Rust crates";
const OWN_FILES = new Set(["steamgauge", "steamgauge.exe", "README.md", "LICENSE", "THIRD-PARTY-NOTICES.txt"]);

// Licence files as publishers ship them carry byte order marks and CRLF endings; one text file
// reads the same everywhere with neither.
export function normaliseText(text) {
  const body = text.replace(/^﻿/, "").replace(/\r\n?/g, "\n").replace(/\s+$/, "");
  return `${body}\n`;
}

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

// The ONNX Runtime ort-sys downloads for `target` with `features`, as its build script picks
// one from the table it ships: the row whose execution providers are exactly those enabled.
export function onnxRuntimeDistribution(table, target, features) {
  const rows = table
    .split(/\r?\n/)
    .slice(1)
    .filter((line) => line.trim() !== "" && !line.startsWith("#"))
    .map((line) => line.split("\t").map((cell) => cell.trim()));
  const providers = (cell) => (cell === "none" ? [] : cell.split(","));
  const offered = new Set(rows.flatMap((row) => providers(row[1])));
  const wanted = [...new Set(features.filter((feature) => offered.has(feature)))].sort();
  const row = rows.find(
    (candidate) => candidate[0] === target && providers(candidate[1]).sort().join(",") === wanted.join(","),
  );
  if (row === undefined) {
    throw new Error(`ort-sys offers no ONNX Runtime for ${target} with ${wanted.join(", ") || "no execution provider"}.`);
  }
  const version = /\/ms@([^/]+)\//.exec(row[2])?.[1];
  if (version === undefined) {
    throw new Error(`Cannot read the ONNX Runtime version from ${row[2]}.`);
  }
  return { providers: wanted, url: row[2], version };
}

const CRATES_IO = /^(registry|sparse)\+https:\/\/(github\.com\/rust-lang\/crates\.io-index|index\.crates\.io\/)$/;

// Where a crate's source can be had, which MPL-2.0 asks an executable to say of every file under
// it and which is worth saying of all of them.
export function sourceOf(pkg) {
  if (CRATES_IO.test(pkg.source ?? "")) {
    return `https://crates.io/crates/${pkg.name}/${pkg.version}`;
  }
  if ((pkg.source ?? "").startsWith("git+")) {
    return pkg.source.slice("git+".length);
  }
  return pkg.repository ?? "no published source";
}

const crateKey = (pkg) => `${pkg.name} ${pkg.version}`;

function wrapList(prefix, items) {
  const lines = [];
  let line = prefix;
  for (const [index, item] of items.entries()) {
    const piece = index === items.length - 1 ? item : `${item},`;
    if (line.length + 1 + piece.length > 100 && line !== prefix) {
      lines.push(line);
      line = `   ${piece}`;
    } else {
      line = line === prefix ? `${prefix} ${piece}` : `${line} ${piece}`;
    }
  }
  lines.push(line);
  return lines.join("\n");
}

// Every licence text cargo-about found, grouped by the text itself: one crate's LICENSE can be
// what several of its licences point at, and a thousand crates share a handful of texts.
function groupTexts(licences) {
  const groups = new Map();
  for (const licence of licences) {
    const text = normaliseText(licence.text);
    const group = groups.get(text) ?? { ids: new Set(), crates: new Set(), standard: false };
    group.ids.add(licence.id);
    for (const use of licence.used_by) {
      group.crates.add(crateKey(use.crate));
    }
    group.standard ||= licence.source_path === null || licence.source_path === undefined;
    groups.set(text, group);
  }
  return [...groups.entries()]
    .map(([text, group]) => ({ text, ...group, ids: [...group.ids].sort(), crates: [...group.crates].sort() }))
    .sort((a, b) => a.ids.join().localeCompare(b.ids.join()) || a.crates[0].localeCompare(b.crates[0]));
}

// The crates with no licence text of their own: none at all, or SPDX's template for a licence
// that names a holder.
export function cratesWithoutText(about) {
  const problems = [];
  const covered = new Set();
  for (const licence of about.licenses) {
    const templated =
      (licence.source_path === null || licence.source_path === undefined) && !WITHOUT_A_HOLDER.has(licence.id);
    for (const use of licence.used_by) {
      covered.add(crateKey(use.crate));
      if (templated) {
        problems.push(`${crateKey(use.crate)}: ${licence.id} is SPDX's template, with the holder left blank`);
      }
    }
  }
  for (const { package: pkg } of about.crates) {
    if (!covered.has(crateKey(pkg))) {
      problems.push(`${crateKey(pkg)}: no licence text at all`);
    }
  }
  return problems.sort();
}

function section(title) {
  return [RULE, title, RULE, ""].join("\n");
}

// ONNX_RUNTIME or DIRECTML with the texts third-party/ holds for it, as renderNotices takes them.
export function withTexts(component) {
  const texts = component.files.map((file) => {
    const text = normaliseText(readFileSync(join(THIRD_PARTY, file), "utf8"));
    return [THIN, file.slice(file.indexOf("/") + 1), THIN, "", text].join("\n");
  });
  return { ...component, texts };
}

// `about` is cargo-about's JSON and `notices` the NOTICE files found in the crates, as
// { text, crates }. `onnxRuntime` and `directml` are the entries above with their texts read
// into `texts`, and `directml` is null for an archive DirectML.dll is not in.
export function renderNotices({ target, about, notices, onnxRuntime, directml }) {
  const used = new Map();
  for (const licence of about.licenses) {
    for (const use of licence.used_by) {
      const key = crateKey(use.crate);
      used.set(key, (used.get(key) ?? new Set()).add(licence.id));
    }
  }
  const crates = [...about.crates].sort((a, b) => crateKey(a.package).localeCompare(crateKey(b.package)));
  const parts = [
    "THIRD-PARTY SOFTWARE NOTICES",
    `steamgauge for ${target}`,
    "",
    "The steamgauge binary in this archive is built from the Rust crates in part 1 and linked with",
    `ONNX Runtime, part 3.${directml ? " DirectML.dll beside it is Microsoft DirectML, part 4." : ""}`,
    "This file carries the licence each of them is used under here and the notices those licences",
    "ask to accompany a copy. The program's own licence is LICENSE, beside this file.",
    "",
    `  ${CRATES_HEADING}`,
    "  2. NOTICE files the crates carry",
    `  3. ${onnxRuntime.heading} ${onnxRuntime.version}`,
    ...(directml ? [`  4. ${directml.heading} ${directml.version}`] : []),
    "",
    section(CRATES_HEADING),
    `${crates.length} crates. Each line gives the crate and version, the licence it is used under here,`,
    "the licence it declares, and where its source is.",
    "",
    ...crates.map((entry) => {
      const key = crateKey(entry.package);
      const under = [...(used.get(key) ?? [])].sort().join(" AND ") || "none";
      return `${key} | ${under} | ${entry.license} | ${sourceOf(entry.package)}`;
    }),
    "",
    "The licence texts follow, each once, with the crates it is the text for. A crate that ships",
    "its licence in a form not recognised as one, or not at all, is given the licence's standard",
    "text, and only for a licence whose text names no copyright holder.",
    "",
  ];
  for (const group of groupTexts(about.licenses)) {
    parts.push(
      THIN,
      `${group.ids.join(", ")}${group.standard ? ", the standard text" : ""}`,
      wrapList("Used by:", group.crates),
      THIN,
      "",
      group.text,
    );
  }
  parts.push(
    section("2. NOTICE files the crates carry"),
    "The Apache License asks that the attribution notices in a work's NOTICE file travel with it.",
    "These are those files, word for word.",
    "",
  );
  if (notices.length === 0) {
    parts.push("None of the crates above carries one.", "");
  }
  for (const notice of notices) {
    parts.push(THIN, wrapList("In:", notice.crates), THIN, "", notice.text);
  }
  parts.push(
    section(`3. ${onnxRuntime.heading} ${onnxRuntime.version}`),
    "The prebuilt library the ort-sys crate downloads for this target, which the binary is linked",
    "with. Its licence and third-party notices, as published with its source at",
    `${onnxRuntime.source}:`,
    "",
    ...onnxRuntime.texts,
  );
  if (directml) {
    parts.push(
      section(`4. ${directml.heading} ${directml.version}`),
      "DirectML.dll is bin/x64-win/DirectML.dll from the Microsoft.AI.DirectML package at",
      `${directml.source}.`,
      "LICENSE.txt is the licence of DirectML.dll. LICENSE-CODE.txt is the licence of the package's",
      "headers, which ONNX Runtime is compiled against. ThirdPartyNotices.txt covers what DirectML",
      "contains.",
      "",
      ...directml.texts,
    );
  }
  return `${parts.join("\n").replace(/\n+$/, "")}\n`;
}

// The `name version` lines of part 1.
export function listedCrates(notices) {
  const lines = notices.split("\n");
  const start = lines.findIndex((line, index) => line === CRATES_HEADING && lines[index - 1] === RULE);
  if (start === -1) {
    return null;
  }
  const listed = new Set();
  for (const line of lines.slice(start + 2)) {
    if (line === RULE) {
      break;
    }
    const match = /^(\S+) (\S+) \| /.exec(line);
    if (match !== null) {
      listed.add(`${match[1]} ${match[2]}`);
    }
  }
  return listed;
}

// The crates Cargo.lock says come from somewhere other than this repository.
export function publishedCrates(lock) {
  const published = new Set();
  for (const block of lock.split(/\n(?=\[\[package\]\]\r?\n)/).slice(1)) {
    const name = /^name = "([^"]*)"/m.exec(block)?.[1];
    const version = /^version = "([^"]*)"/m.exec(block)?.[1];
    if (/^source = "/m.test(block)) {
      published.add(`${name} ${version}`);
    }
  }
  return published;
}

// `files` maps each file name in the unpacked archive to its SHA-256.
export function checkNotices({ target, notices, crates, lock, files }) {
  const problems = [];
  if (notices.trim() === "") {
    return ["THIRD-PARTY-NOTICES.txt is empty."];
  }
  if (!notices.split("\n", 2).includes(`steamgauge for ${target}`)) {
    problems.push(`THIRD-PARTY-NOTICES.txt was not written for ${target}.`);
  }
  const listed = listedCrates(notices);
  if (listed === null || listed.size === 0) {
    problems.push("THIRD-PARTY-NOTICES.txt lists no crates.");
  } else {
    const published = publishedCrates(lock);
    const missing = [...crates].filter((key) => published.has(key) && !listed.has(key));
    if (missing.length > 0) {
      problems.push(`THIRD-PARTY-NOTICES.txt leaves out crates the build resolved: ${missing.join(", ")}`);
    }
  }
  const [licence] = ONNX_RUNTIME.files;
  if (
    !notices.includes(`${RULE}\n3. ${ONNX_RUNTIME.heading} ${ONNX_RUNTIME.version}\n${RULE}`) ||
    !notices.includes(normaliseText(readFileSync(join(THIRD_PARTY, licence), "utf8")))
  ) {
    problems.push(`THIRD-PARTY-NOTICES.txt does not carry ONNX Runtime ${ONNX_RUNTIME.version}'s licence.`);
  }
  const hasDirectmlSection = notices.includes(`${RULE}\n4. ${DIRECTML.heading} ${DIRECTML.version}\n${RULE}`);
  for (const [file, digest] of files) {
    if (file === "DirectML.dll") {
      if (!hasDirectmlSection) {
        problems.push("DirectML.dll ships without DirectML's licence in THIRD-PARTY-NOTICES.txt.");
      }
      if (DIRECTML.dll.get(target) !== digest) {
        problems.push(`DirectML.dll is not the DirectML ${DIRECTML.version} whose licence third-party/directml holds.`);
      }
    } else if (!OWN_FILES.has(file)) {
      problems.push(`${file} ships, and nothing says whose it is or what licence it is under.`);
    }
  }
  if (hasDirectmlSection && !files.has("DirectML.dll")) {
    problems.push("THIRD-PARTY-NOTICES.txt carries DirectML's licence for an archive without DirectML.dll.");
  }
  return problems;
}

function cargoMetadata(target, feature) {
  const result = spawnSync(
    "cargo",
    [
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--filter-platform",
      target,
      ...(feature ? ["--features", feature] : []),
      "--manifest-path",
      APP_MANIFEST,
    ],
    { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  if (result.status !== 0) {
    throw new Error(`cargo metadata failed:\n${result.stderr}`);
  }
  return JSON.parse(result.stdout);
}

function onnxRuntimeFor(target, feature) {
  const metadata = cargoMetadata(target, feature);
  const ortSys = metadata.packages.filter((pkg) => pkg.name === "ort-sys");
  if (ortSys.length !== 1) {
    throw new Error(`Expected one ort-sys in the resolution, found ${ortSys.length}.`);
  }
  const node = metadata.resolve.nodes.find((candidate) => candidate.id === ortSys[0].id);
  const table = readFileSync(join(dirname(ortSys[0].manifest_path), "build", "download", "dist.tsv"), "utf8");
  const distribution = onnxRuntimeDistribution(table, target, node.features);
  if (distribution.version !== ONNX_RUNTIME.version) {
    throw new Error(
      `ort-sys ${ortSys[0].version} links ONNX Runtime ${distribution.version}, and third-party/onnxruntime holds ` +
        `${ONNX_RUNTIME.version}'s licence and notices. Replace them with ${distribution.version}'s and update ` +
        "ONNX_RUNTIME in tools/release/notices.mjs and third-party/README.md.",
    );
  }
  return distribution;
}

function runCargoAbout(cargoAbout, target, feature) {
  const scratch = mkdtempSync(join(tmpdir(), "notices-"));
  try {
    const output = join(scratch, "about.json");
    const result = spawnSync(
      cargoAbout,
      [
        "--color",
        "never",
        "generate",
        "--locked",
        "--fail",
        "--format",
        "json",
        "--target",
        target,
        ...(feature ? ["--features", feature] : []),
        "--manifest-path",
        APP_MANIFEST,
        "--config",
        join(THIRD_PARTY, "about.toml"),
        "--output-file",
        output,
      ],
      { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    );
    process.stderr.write(result.stderr ?? "");
    if (result.error) {
      throw result.error;
    }
    if (result.status !== 0) {
      throw new Error(`cargo-about refused the crates for ${target}; its reasons are above.`);
    }
    const refused = (result.stderr ?? "").split("\n").filter((line) => REFUSED_WARNINGS.some((warning) => warning.test(line)));
    if (refused.length > 0) {
      throw new Error(`cargo-about could not read every crate's licence for ${target}:\n${refused.join("\n")}`);
    }
    return JSON.parse(readFileSync(output, "utf8"));
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

// NOTICE, NOTICE.txt and the like at each crate's root, one entry per distinct text.
function noticeFiles(about) {
  const byText = new Map();
  for (const { package: pkg } of about.crates) {
    const root = dirname(pkg.manifest_path);
    for (const file of readdirSync(root).filter((name) => /^NOTICE(\.[A-Za-z]+)?$/i.test(name)).sort()) {
      const text = normaliseText(readFileSync(join(root, file), "utf8"));
      byText.set(text, [...(byText.get(text) ?? []), crateKey(pkg)]);
    }
  }
  return [...byText.entries()]
    .map(([text, crates]) => ({ text, crates: crates.sort() }))
    .sort((a, b) => a.crates[0].localeCompare(b.crates[0]));
}

function write(cargoAbout, target, feature, outputPath) {
  const distribution = onnxRuntimeFor(target, feature);
  const about = runCargoAbout(cargoAbout, target, feature);
  if (about.crates.length === 0) {
    throw new Error(`cargo-about found no crates for ${target}.`);
  }
  const withoutText = cratesWithoutText(about);
  if (withoutText.length > 0) {
    throw new Error(
      "These crates ship no licence text cargo-about recognises. Clarify each in " +
        `third-party/about.toml with the file that is its licence:\n  ${withoutText.join("\n  ")}`,
    );
  }
  const notices = renderNotices({
    target,
    about,
    notices: noticeFiles(about),
    onnxRuntime: withTexts(ONNX_RUNTIME),
    directml: distribution.providers.includes("directml") ? withTexts(DIRECTML) : null,
  });
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, notices);
  console.error(`${outputPath}: ${about.crates.length} crates, ONNX Runtime ${distribution.version}.`);
}

function check(target, noticesPath, cratesPath, lockPath, archiveDir) {
  if (!existsSync(noticesPath)) {
    throw new Error(`${noticesPath} is missing.`);
  }
  const files = new Map(
    readdirSync(archiveDir, { withFileTypes: true }).map((entry) => {
      if (!entry.isFile()) {
        throw new Error(`${join(archiveDir, entry.name)} is not a file; the archive is expected to be flat.`);
      }
      return [entry.name, sha256(readFileSync(join(archiveDir, entry.name)))];
    }),
  );
  const problems = checkNotices({
    target,
    notices: readFileSync(noticesPath, "utf8"),
    crates: parseCrates(readFileSync(cratesPath, "utf8")),
    lock: readFileSync(lockPath, "utf8"),
    files,
  });
  if (problems.length > 0) {
    throw new Error(problems.join("\n"));
  }
  console.error(`${noticesPath}: covers the ${files.size} files and every published crate the build resolved.`);
}

if (isMain(import.meta.url)) {
  const [command, ...args] = process.argv.slice(2);
  try {
    if (command === "write" && args.length === 4) {
      write(...args);
    } else if (command === "check" && args.length === 5) {
      check(...args);
    } else {
      throw new Error(
        "Usage: node tools/release/notices.mjs write <cargo-about> <target> <feature or \"\"> <output>\n" +
          "       node tools/release/notices.mjs check <target> <notices> <crates.txt> <Cargo.lock> <archive dir>",
      );
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
