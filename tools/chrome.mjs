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
  // Where Chrome installs itself for one user, without administrator rights.
  process.env.LOCALAPPDATA && join(process.env.LOCALAPPDATA, "Google", "Chrome", "Application", "chrome.exe"),
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

// What the page calls when it goes hidden, on every document it loads.
const HIDDEN = "steamgaugeCheckWentHidden";

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
      // Every page checked is a file or is served from 127.0.0.1, and holds to fetching nothing
      // else, so no name needs resolving. Closing that off also keeps out an extension a
      // machine's policy force-installs into every profile, a fresh headless one included: it
      // downloads from the Web Store and opens a sign-in page in front of the one under test,
      // which hides it.
      "--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1",
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
  // A hidden page has its timers fired once a minute and the window records no look nobody
  // could see, so a check run on one crawls and then fails on something else. Nothing in a
  // check hides the page; something that does has been put in front of it, and is named.
  let hidden = false;
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.method === "Network.requestWillBeSent") {
      asked.push(message.params.request.url);
    }
    if (message.method === "Runtime.bindingCalled" && message.params.name === HIDDEN) {
      hidden = true;
    }
    // Only a reply to a command this side sent: an id it numbered, answered once. Anything
    // else on the socket, events included, settles nothing.
    if (!Number.isInteger(message.id)) {
      return;
    }
    const settle = waiting.get(message.id);
    if (typeof settle === "function") {
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
  const evaluate = async (expression) => {
    if (hidden) {
      const pages = await fetch(`${new URL(url).origin.replace("ws:", "http:")}/json/list`)
        .then((r) => r.json())
        .then((list) => list.filter((t) => t.type === "page").map((t) => t.url))
        .catch(() => []);
      throw new Error(`the page under test went hidden; the browser's pages:\n  ${pages.join("\n  ")}`);
    }
    return send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  };
  // A page being left goes hidden on its way out, and by then its binding may be gone too.
  const watch = `{
    let leaving = false;
    addEventListener("pagehide", () => { leaving = true; });
    const tell = () => {
      if (!leaving && document.visibilityState === "hidden" && typeof ${HIDDEN} === "function") {
        ${HIDDEN}("");
      }
    };
    document.addEventListener("visibilitychange", tell);
    tell();
  }`;
  await send("Runtime.addBinding", { name: HIDDEN });
  await send("Runtime.enable", {});
  await send("Page.addScriptToEvaluateOnNewDocument", { source: watch });
  await send("Runtime.evaluate", { expression: watch });
  return { socket, send, asked, evaluate };
}
