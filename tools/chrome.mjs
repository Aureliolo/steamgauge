// Driving headless Chrome over the DevTools protocol, which every browser check here needs.
//
// Chrome is found through CHROME_PATH, or in the usual places on each platform.
//
// The debugging port is Chrome's to choose. A fixed one is a shared name between processes
// that have nothing else in common: five of these run one after another in CI, and a Chrome
// that outlives the script that spawned it holds the number the next one asks for, which
// comes back as a browser that started and never answered. Asking for port zero and reading
// back what it bound leaves nothing to collide over.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CANDIDATES = [
  process.env.CHROME_PATH,
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
  "/snap/bin/chromium",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
];

export function browser() {
  const found = CANDIDATES.filter(Boolean).find((path) => existsSync(path));
  if (!found) {
    throw new Error(
      `no Chrome found. Set CHROME_PATH, or install one of:\n  ${CANDIDATES.filter(Boolean).join("\n  ")}`,
    );
  }
  return found;
}

export const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

const START_LIMIT_MS = 90_000;

/// Starts Chrome on the page, in a profile of its own, and waits for it to say which port it
/// listens on. Throws with whatever Chrome said if it dies first.
export async function open(url, { prefix = "steamgauge-check-" } = {}) {
  const profile = await mkdtemp(join(tmpdir(), prefix));
  const chrome = spawn(
    browser(),
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-gpu",
      // A runner's /dev/shm is small enough that Chrome's renderer falls over on it, and the
      // fall is silent.
      "--disable-dev-shm-usage",
      url,
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  // Kept so that a failure to start says why, instead of the twenty-second silence that a
  // discarded stderr leaves behind.
  let said = "";
  chrome.stderr.on("data", (chunk) => {
    said = `${said}${chunk}`.slice(-4000);
  });
  let stopped = null;
  chrome.once("exit", (code, signal) => {
    stopped = signal ? `killed by ${signal}` : `exited with ${code}`;
  });

  // A start usually takes a second or two, but on a CI runner two in twenty-three needed more
  // than twenty, still starting with nothing wrong. The wait is for a browser that will never
  // answer, so it is set well past a slow one, and a slow one is said out loud: a start that
  // keeps getting slower is a finding, and a limit that is simply raised would hide it.
  const active = join(profile, "DevToolsActivePort");
  const started = Date.now();
  while (Date.now() - started < START_LIMIT_MS) {
    const [port] = (await readFile(active, "utf8").catch(() => "")).split("\n");
    if (port) {
      const took = (Date.now() - started) / 1000;
      if (took > 5) {
        console.error(`headless Chrome took ${took.toFixed(1)}s to open its debugging port`);
      }
      return { chrome, port: Number(port), close: () => close(chrome, profile) };
    }
    if (stopped) {
      await rm(profile, { recursive: true, force: true }).catch(() => {});
      throw new Error(`headless Chrome ${stopped} before it opened a debugging port\n${said}`);
    }
    await sleep(100);
  }
  await close(chrome, profile);
  throw new Error(
    `headless Chrome never opened a debugging port in ${START_LIMIT_MS / 1000}s\n${said}`,
  );
}

async function close(chrome, profile) {
  chrome.kill();
  // Chrome holds its profile open for a moment after the signal, and on Windows unlinking a
  // file it still has is an error rather than a wait. A Chrome that ignores the signal is
  // taken down for good, because the next check would find its port and its profile.
  const gone = new Promise((done) => chrome.once("exit", done));
  const forced = setTimeout(() => chrome.kill("SIGKILL"), 5_000);
  await gone;
  clearTimeout(forced);
  await rm(profile, { recursive: true, force: true }).catch(() => {});
}

export async function debuggerUrl(port) {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const list = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
      const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // The port is open before the page target is listed, which is the usual first second.
    }
    await sleep(250);
  }
  throw new Error(`headless Chrome opened port ${port} but listed no page`);
}

export async function connect(url) {
  const socket = new WebSocket(url);
  await new Promise((ok, bad) => {
    socket.addEventListener("open", ok, { once: true });
    socket.addEventListener("error", bad, { once: true });
  });
  let id = 0;
  const waiting = new Map();
  // Every URL the page asks for, which is the only way to hold it to fetching nothing: the
  // markup can be free of every http:// this tool would have written and still pull a font
  // in from a stylesheet nobody read closely.
  const asked = [];
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.method === "Network.requestWillBeSent") {
      asked.push(message.params.request.url);
    }
    const settle = waiting.get(message.id);
    if (settle) {
      waiting.delete(message.id);
      settle(message);
    }
  });
  const send = (method, params) =>
    new Promise((ok) => {
      id += 1;
      waiting.set(id, ok);
      socket.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = (expression) =>
    send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return { socket, send, asked, evaluate };
}
