// Drives the desktop app's window in headless Chrome against a stand-in core, and fails if it
// does not do what it promises.
//
// The window is plain HTML and modules that talk to the core through `window.__TAURI__`. Served
// here with `stub.js` standing in for that bridge, every page can be opened and every control
// pressed without a library, a model or a webview: the cockpit, the work board, the library's
// sorting, grouping and selection, the comparison, the settings, a game's page, and the notice
// that a newer version is out.
//
//   node tools/app-check/check.mjs [--shots <folder>]
//
// `--shots` saves a picture of every page, light and dark, for a person to look at.
//
// Chrome is found through CHROME_PATH, or in the usual places on each platform.
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, extname, join, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import { connect, debuggerUrl, open, sleep } from "../chrome.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const UI = resolve(HERE, "..", "..", "crates", "steamgauge-app", "ui");
const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
};

const shotsAt = process.argv.indexOf("--shots");
const shots = shotsAt === -1 ? null : resolve(process.argv[shotsAt + 1]);

// The window's own files, with the stand-in loaded ahead of its script. Nothing outside the
// window's folder is served.
const server = createServer(async (request, response) => {
  const path = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
  try {
    if (path === "/__stub.js") {
      response.writeHead(200, { "content-type": TYPES[".js"] });
      response.end(await readFile(join(HERE, "stub.js")));
      return;
    }
    const file = normalize(join(UI, path === "/" ? "index.html" : path));
    if (!file.startsWith(UI + sep)) throw new Error("outside the window's folder");
    let body = await readFile(file);
    if (file.endsWith("index.html")) {
      body = Buffer.from(
        body.toString("utf8").replace('<script type="module"', '<script src="/__stub.js"></script>\n    <script type="module"'),
      );
    }
    response.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end();
  }
});
await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
const page = `http://127.0.0.1:${server.address().port}/index.html`;

// Runs inside the page. Returns a list of failures, so one run reports everything wrong rather
// than the first thing wrong.
const PROBE = `(async function () {
  var wrong = [];
  var check = function (claim, ok) { if (!ok) { wrong.push(claim); } };
  var pause = function (ms) { return new Promise(function (done) { setTimeout(done, ms || 80); }); };
  var $ = function (selector) { return document.querySelector(selector); };
  var shown = function (id) { return !document.getElementById(id).hidden; };
  var called = function (command) {
    return window.__stub.calls.filter(function (call) { return call.command === command; });
  };
  var last = function (command) { var all = called(command); return all[all.length - 1]; };
  var rows = function () { return Array.prototype.slice.call(document.querySelectorAll('#library-rows tr')); };
  var go = function (name) { document.querySelector('[data-go="' + name + '"]').click(); return pause(250); };
  var fits = function () { var stage = document.getElementById('stage'); return stage.scrollWidth <= stage.clientWidth + 1; };

  await pause(400);
  check('the app does not open on the cockpit', shown('cockpit'));
  check('the cockpit is not the page marked in the rail',
    $('[data-go="cockpit"]').getAttribute('aria-current') === 'page');
  check('the cockpit does not count the games', /3 games/.test($('#cockpit-sub').textContent));
  check('the cockpit does not show what moved', $('#moves').children.length === 2);
  check('a complaint that rose is not marked as bad news', $('#moves .move.complaint.up') !== null);
  check('a share recommending that fell is not marked as bad news', $('#moves .move.recommended.down') !== null);
  check('the cockpit does not list the models', $('#models').children.length === 3);
  check('a newer release goes unmentioned', /v2 is published/.test($('#models').textContent));
  check('a download still to come does not say its size', /1\\.3 GB download/.test($('#models').textContent));
  check('a game new on Steam goes unlisted', /\\+1,520/.test($('#health-lists').textContent));
  check('a game read by an older reader goes unlisted', /Beta/.test($('#health-lists').textContent));
  check('the card in use goes unnamed', /RTX 4090/.test($('#machine-facts').textContent));
  check('the share of the card goes unsaid', /50% of its time/.test($('#machine-reads').textContent));
  check('the card\\'s memory is not in the gigabytes it is sold in', /24 GB/.test($('#machine-facts').textContent));
  check('an idle board does not say nothing is running', shown('cockpit-idle'));
  check('the cockpit does not fit its page', fits());

  var release = 'https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0';
  check('a newer version kept from the last question is not announced at opening', shown('newer-version'));
  check('the notice does not name the newer version', /SteamGauge 0\\.2\\.0 is out/.test($('#newer-version').textContent));
  check('the notice does not name the version this computer has', /has 0\\.1\\.0/.test($('#newer-version').textContent));
  $('#newer-open').click();
  await pause();
  check('the notice does not open the release page through the opener',
    last('plugin:opener|open_url') && last('plugin:opener|open_url').args.url === release);
  window.__stub.hear({ version: '0.3.0', running: '0.1.0',
    url: 'https://github.com/Aureliolo/steamgauge/releases/tag/v0.3.0' });
  await pause();
  check('a fresh answer does not reach the notice', /SteamGauge 0\\.3\\.0 is out/.test($('#newer-version').textContent));
  window.__stub.hear({ version: '0.2.0', running: '0.1.0', url: release });
  await pause();

  window.__stub.board([
    { id: 1, task: { kind: 'read', app_id: 1, language: 'english' }, name: 'Alpha', state: 'running',
      step: 'Fetching the reader', unit: 'bytes', done: 312e6, total: 1127e6, rate: 25e6, left: 32.6,
      note: null, queued: 0, started: 0, ended: null },
    { id: 2, task: { kind: 'update', app_id: 2 }, name: 'Beta', state: 'queued', step: 'Waiting',
      unit: 'pages', done: 0, total: null, rate: null, left: null, note: null, queued: 0,
      started: null, ended: null },
    { id: 3, task: { kind: 'export', app_ids: [1, 2], to: 'C:\\\\r.html' }, name: 'Report on 2 games',
      state: 'done', step: 'Writing the report', unit: 'games', done: 2, total: 2, rate: null,
      left: null, note: 'Saved as C:\\\\r.html.', queued: 0, started: 0, ended: 0 },
  ]);
  await pause();
  var job = $('#cockpit-work .job.running');
  var said = job ? job.textContent : '';
  check('a running job is not drawn', job !== null);
  check('a download does not say how much of how much', /312 MB of 1\\.1 GB/.test(said));
  check('a download does not say how fast', /25 MB\\/s/.test(said));
  check('a download does not say how long is left', /35 s left/.test(said));
  check('the bar is not filled to the share done',
    job && Math.abs(parseFloat(job.querySelector('.fill').style.width) - 27.7) < 0.2);
  check('a waiting job is not drawn as waiting', /Waiting its turn/.test($('#cockpit-work').textContent));
  check('a busy board still says nothing is running', !shown('cockpit-idle'));
  check('the rail does not show the running job',
    shown('rail-work') && /Alpha/.test($('#rail-work').textContent));
  check('the rail does not count what is waiting', /1 waiting/.test($('#rail-work').textContent));
  var open = Array.prototype.find.call(document.querySelectorAll('#cockpit-work .job.done button'),
    function (b) { return b.textContent === 'Open'; });
  check('a saved report cannot be opened', open !== undefined);
  if (open) { open.click(); await pause(); }
  check('opening a report does not name its job to the core', last('open_report') && last('open_report').args.id === 3);
  job.querySelector('button').click();
  await pause();
  check('stopping a job does not ask the core to stop it', last('stop_job') && last('stop_job').args.id === 1);

  await go('library');
  check('the library is not shown', shown('library'));
  check('the library does not list every game', rows().length === 3);
  check('a game being worked on does not say so', /Waiting/.test(rows().filter(function (r) {
    return /Beta/.test(r.textContent); })[0].textContent));
  check('a game read by an older reader is not marked', $('#library-rows .older') !== null);
  check('a game that moved lately does not say what moved', /Complaints: performance/.test($('#library-rows').textContent));
  var sort = $('#library-sort');
  sort.value = 'reviews';
  sort.dispatchEvent(new Event('change'));
  await pause();
  check('sorting by reviews does not put the most first', /Alpha/.test(rows()[0].textContent));
  var filter = $('#library-filter');
  filter.value = 'gam';
  filter.dispatchEvent(new Event('input'));
  await pause();
  check('filtering does not narrow the table', rows().length === 1 && /Gamma/.test(rows()[0].textContent));
  filter.value = '';
  filter.dispatchEvent(new Event('input'));
  await pause();
  var tabs = function () { return Array.prototype.slice.call(document.querySelectorAll('#group-tabs .tab')); };
  var rivals = tabs().filter(function (t) { return /Rivals/.test(t.textContent); })[0];
  check('a group is not a tab', rivals !== undefined);
  rivals.click();
  await pause();
  check('a group tab does not narrow the table to its games', rows().length === 1 && /Beta/.test(rows()[0].textContent));
  check('a group cannot be renamed or deleted from its tab', shown('group-tools'));
  tabs()[0].click();
  await pause();
  rows()[0].querySelector('input').click();
  await pause();
  rows()[1].querySelector('input').click();
  await pause();
  check('selecting games shows no actions for them', shown('selection') && /2 selected/.test($('#selected-count').textContent));
  check('two selected games cannot be compared', !$('#selected-compare').disabled);
  var choose = $('#selected-group');
  choose.value = '\\u0000new';
  choose.dispatchEvent(new Event('change'));
  await pause();
  check('a new group cannot be named', shown('new-group'));
  $('#new-group-name').value = 'Shooters';
  $('#new-group').requestSubmit();
  await pause(250);
  var saved = last('save_groups');
  check('a new group is not saved with the selected games', saved && saved.args.groups.groups.some(function (g) {
    return g.name === 'Shooters' && g.app_ids.length === 2; }));
  check('a new group does not become a tab', tabs().some(function (t) { return /Shooters/.test(t.textContent); }));
  $('#selected-update').click();
  await pause();
  check('updating the selection does not queue it', last('queue_updates') && last('queue_updates').args.appIds.length === 2);
  $('#update-all').click();
  await pause();
  check('updating every game names games instead of all of them', last('queue_updates').args.appIds === null);
  $('#selected-read').click();
  await pause();
  check('reading the selection does not queue it', last('queue_reads') && last('queue_reads').args.appIds.length === 2);
  check('the library does not fit its page', fits());

  $('#selected-compare').click();
  await pause(300);
  check('compare does not open', shown('compare'));
  check('the comparison is not of the selected games',
    document.querySelectorAll('#compare-head th.game-col').length === 2);
  check('a subject nobody raises is in the comparison', !/VR/.test($('#compare-rows').textContent));
  // Story is raised by 20% and 30% of the two games' read reviews, performance by 27% and 12%.
  check('subjects are not ranked by how often the games raise them on average',
    $('#compare-rows tr th').textContent === 'Story');
  check('a share of a subject\\'s reviews goes past the whole', !/\\d{3,}%/.test($('#compare-rows').textContent));
  check('a share in the comparison is not of the game\\'s own reviews', /20%/.test($('#compare-rows').textContent));
  $('#compare-export').click();
  await pause();
  check('saving the comparison does not ask for a report of its games',
    last('export_report') && last('export_report').args.appIds.length === 2);
  check('the comparison does not fit its page', fits());

  await go('settings');
  check('settings do not offer four shares of the card', document.querySelectorAll('#gpu-share input').length === 4);
  var checked = $('#gpu-share input:checked');
  check('the share in use is not the one marked', checked && checked.value === '0.5');
  var whole = Array.prototype.filter.call(document.querySelectorAll('#gpu-share input'), function (i) {
    return i.value === '1'; })[0];
  whole.click();
  await pause(250);
  check('choosing a share does not save it', last('save_settings') && last('save_settings').args.settings.gpu_share === 1);
  $('#search-every-game').click();
  await pause(250);
  check('preparing every game for search is not saved', last('save_settings').args.searchEveryGame === true);
  check('the library\\'s place goes unsaid', /AppData/.test($('#library-place').textContent));
  check('asking for a newer version is not on as it is by default', $('#check-newer-version').checked);
  $('#check-newer-version').click();
  await pause(250);
  check('turning off the question for a newer version is not saved',
    last('save_settings').args.settings.check_newer_version === false);
  check('the notice stays after the question is turned off', !shown('newer-version'));
  $('#check-newer-version').click();
  await pause(250);
  check('turning the question back on does not bring the notice back',
    last('save_settings').args.settings.check_newer_version === true && shown('newer-version'));

  await go('library');
  rows().filter(function (r) { return /Gamma/.test(r.textContent); })[0].querySelector('.game-link').click();
  await pause(300);
  check('a game page does not open', shown('game'));
  check('a first read does not say what it will fetch', /1\\.1 GB download/.test($('#read-cost').textContent));
  $('#do-read').click();
  await pause();
  check('reading does not queue a read of the game', called('queue').some(function (call) {
    return call.args.tasks.some(function (t) { return t.kind === 'read' && t.app_id === 3; }); }));
  check('a game\\'s page does not show its own work', $('#game-jobs .job') !== null);

  await go('library');
  rows().filter(function (r) { return /Alpha/.test(r.textContent); })[0].querySelector('.game-link').click();
  await pause(300);
  check('a read game does not show what people talk about',
    shown('topics') && document.querySelectorAll('#topic-rows tr').length === 2);
  $('#game-back').click();
  await pause(250);
  check('the way back from a game does not lead to the library', shown('library'));

  $('#library-add').click();
  await pause();
  check('adding a game does not open the finder', shown('finder'));
  $('#appid').value = '4';
  $('#lookup-form').requestSubmit();
  await pause(250);
  check('a looked-up game is not offered', shown('found') && /Delta/.test($('#found-name').textContent));
  check('the finder does not say what happens after the download', /every review is read/.test($('#found-then').textContent));
  $('#start').click();
  await pause(250);
  check('downloading does not queue a download', called('queue').some(function (call) {
    return call.args.tasks.some(function (t) { return t.kind === 'download' && t.app_id === 4; }); }));
  check('a game being downloaded does not open on its page', shown('game') && /Delta/.test($('#game-name').textContent));
  return wrong;
})()`;

const chrome = await open(page, { prefix: "steamgauge-app-check-" });
let failed = true;
try {
  const { socket, send, asked, evaluate } = await connect(await debuggerUrl(chrome.port));
  const threw = [];
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.method === "Runtime.exceptionThrown") {
      const details = message.params.exceptionDetails;
      threw.push(details.exception?.description ?? details.text);
    }
    if (message.method === "Runtime.consoleAPICalled" && message.params.type === "error") {
      threw.push(message.params.args.map((arg) => arg.value ?? arg.description).join(" "));
    }
  });
  await send("Runtime.enable", {});
  await send("Network.enable", {});
  await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 860, deviceScaleFactor: 1, mobile: false });
  await send("Page.reload", {});
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const ready = await evaluate("document.readyState === 'complete' && !!window.__stub");
    if (ready.result?.result?.value === true) break;
    await sleep(250);
  }

  const answer = await evaluate(PROBE);
  // The narrowest window the app allows, where the library's table is the first thing to
  // run off the side.
  await send("Emulation.setDeviceMetricsOverride", { width: 760, height: 560, deviceScaleFactor: 1, mobile: false });
  const narrow = await evaluate(`(async function () {
    var wrong = [];
    for (var name of ['cockpit', 'library', 'compare', 'settings']) {
      document.querySelector('[data-go="' + name + '"]').click();
      await new Promise(function (done) { setTimeout(done, 250); });
      var stage = document.getElementById('stage');
      if (stage.scrollWidth > stage.clientWidth + 1) wrong.push('the ' + name + ' runs off the side of the narrowest window');
    }
    return wrong;
  })()`);

  if (shots) {
    await mkdir(shots, { recursive: true });
    for (const scheme of ["light", "dark"]) {
      await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
      await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 860, deviceScaleFactor: 1, mobile: false });
      for (const name of ["cockpit", "library", "compare", "settings"]) {
        await evaluate(`document.querySelector('[data-go="${name}"]').click()`);
        await sleep(350);
        const picture = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
        await writeFile(join(shots, `${name}-${scheme}.png`), Buffer.from(picture.result.data, "base64"));
      }
    }
  }

  socket.close();
  const fetched = asked.filter((url) => !url.startsWith(`http://127.0.0.1:${server.address().port}/`));
  const broke = [answer, narrow].find((r) => r.result?.exceptionDetails);
  if (broke) {
    console.error("the window threw while being checked:");
    console.error(broke.result.exceptionDetails.exception?.description ?? broke.result.exceptionDetails.text);
  } else {
    const wrong = []
      .concat(threw.map((said) => `the window reported an error: ${said}`))
      .concat(fetched.length === 0 ? [] : [`the window fetched ${fetched.length} thing(s): ${fetched.slice(0, 5).join(", ")}`])
      .concat(answer.result.result.value)
      .concat(narrow.result.result.value);
    if (wrong.length === 0) {
      console.log("the window behaves as it says it does");
      failed = false;
    } else {
      console.error(`${wrong.length} promise(s) the window does not keep:`);
      for (const claim of wrong) console.error(`  - ${claim}`);
    }
  }
} finally {
  await chrome.close();
  server.close();
}
process.exit(failed ? 1 : 0);
