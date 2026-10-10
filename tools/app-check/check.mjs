// Drives the desktop app's window in headless Chrome against a stand-in core, and fails if it
// does not do what it promises.
//
// The window is plain HTML and modules that talk to the core through `window.__TAURI__`. Served
// here with `stub.js` standing in for that bridge, every page can be opened and every control
// pressed without a library, a model or a webview: the cockpit and what changed since the last
// look, the work board, the library's sorting, grouping and selection, the comparison, the
// settings, a game's page, and the notice that a newer version is out with every state of its
// update.
//
//   node tools/app-check/check.mjs [--shots <folder>]
//
// Every page and state is then walked light and dark and at the narrowest window, and each stop
// is held to WCAG 2.2 A and AA by axe-core (`npm ci` in tools/ first). `--shots` saves a picture
// of every stop for a person to look at.
//
// Chrome is found through CHROME_PATH, or in the usual places on each platform.
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, extname, join, normalize, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import { audit } from "../accessibility.mjs";
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
// `--pages` keeps each stop's page as rendered, so the words the window shows, most of them
// written by its scripts, can be checked as prose.
const pagesAt = process.argv.indexOf("--pages");
const pages = pagesAt === -1 ? null : resolve(process.argv[pagesAt + 1]);

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
  check('the share of the card goes unsaid', /50%/.test($('#machine-facts').textContent));
  check('reading on the card is explained as if it were the processor', !shown('machine-reads'));
  check('the library\\'s folder is shown without saying what it is',
    $('#library-at').previousElementSibling.textContent === 'Library folder' && $('#library-at').textContent !== '');
  check('the card\\'s memory is not in the gigabytes it is sold in', /24 GB/.test($('#machine-facts').textContent));
  check('an idle board does not say nothing is running', shown('cockpit-idle'));
  check('the cockpit does not fit its page', fits());

  check('the cockpit does not say what changed since the last look',
    document.querySelectorAll('#since-games .since-game').length === 3 && !shown('since-calm'));
  check('the games that moved since do not come first', /Alpha/.test($('#since-games .since-game').textContent) &&
    $('#since-games .since-game').classList.contains('moved'));
  check('a complaint that rose since is not marked as bad news', $('#since-games .since-move.complaint.up') !== null);
  check('praise that fell since is not marked as bad news', $('#since-games .since-move.praise.down') !== null);
  check('a share recommending that fell since is not shown', $('#since-games .since-move.recommended.down') !== null);
  check('the new reviews are not counted', /1,520 new reviews/.test($('#since-games').textContent));
  check('what the new reviews are set against goes unsaid',
    /Of the 1,140 new reviews read, against \\S+ 2024 to \\S+ 2025\\./.test($('#since-games').textContent) && shown('since-how'));
  check('a game whose new reviews are not read yet does not say so', /260 new reviews/.test($('#since-games').textContent) &&
    /not read yet/.test($('#since-games').textContent));
  check('when the cockpit was last looked at goes unsaid', /^You last looked on /.test($('#since-when').textContent));
  check('looking at the cockpit is not recorded', called('looked').some(function (call) { return call.args.appId === null; }));
  check('a subject that moved since is not named in the middle of a sentence', /Praise for\\s*story and writing/.test($('#since-games').textContent));
  window.__stub.lately('calm');
  await pause(250);
  check('a cockpit with nothing changed since does not say so plainly',
    $('#since-games').children.length === 0 && shown('since-calm') &&
    /^Nothing has changed since you last looked\\.$/.test($('#since-calm-text').textContent) && !shown('since-how'));
  window.__stub.lately('first');
  await pause(250);
  check('a first look claims to know when the last one was', $('#since-when').textContent === '' &&
    /Nothing to compare with yet/.test($('#since-calm-text').textContent));
  window.__stub.lately('moved');
  await pause(250);
  $('#since-games .since-game .link').click();
  await pause(300);
  check('a game that changed since does not open from the card', shown('game') && /Alpha/.test($('#game-name').textContent));
  check('a game\\'s page does not mark the library it belongs to in the rail',
    $('[data-go="library"]').getAttribute('aria-current') === 'true' &&
    !$('[data-go="cockpit"]').hasAttribute('aria-current'));
  await go('cockpit');
  window.__stub.send('open-game', 2);
  await pause(300);
  check('a notification clicked does not open the game it named', shown('game') && /Beta/.test($('#game-name').textContent));
  await go('cockpit');

  window.__stub.send('show', { page: 'library' });
  await pause(300);
  check('a client asking to show the library does not get it', shown('library') &&
    $('[data-go="library"]').getAttribute('aria-current') === 'page');
  window.__stub.send('show', { page: 'game', app_id: 2 });
  await pause(300);
  check('a client asking to show a game does not get its page', shown('game') && /Beta/.test($('#game-name').textContent));
  window.__stub.send('show', { page: 'subject', app_id: 1, subject: 'story', side: 'praise' });
  await pause(400);
  check('a client asking to show a subject does not get its reviews', shown('evidence') &&
    $('#evidence-name').textContent === 'Story');
  window.__stub.send('show', { page: 'settings' });
  await pause(300);
  check('a client asking to show the settings does not get them', shown('settings'));
  var asked = called('settings').length;
  window.__stub.send('steered', null);
  await pause(300);
  check('a change a client made is not drawn on the page showing', called('settings').length === asked + 1);
  await go('cockpit');

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

  var notice = function () { return $('#newer-version').textContent; };
  var visible = function (id) { return !document.getElementById(id).hidden; };
  check('a copy the window can update is not offered Update now', visible('newer-update') &&
    /Update now/.test($('#newer-update').textContent));
  $('#newer-update').click();
  await pause();
  check('Update now does not ask the core to update', called('update_now').length === 1);
  check('an update under way still offers Update now', !visible('newer-update'));
  check('a download that has not started says nothing about it', visible('newer-progress') &&
    /Downloading 0\\.2\\.0/.test(notice()));
  window.__stub.update({ state: 'downloading', done: 12e6, total: 48e6, rate: 3e6, left: 12 });
  await pause();
  check('the update\\'s download does not say how much of how much', /12 MB of 48 MB/.test($('#newer-step').textContent));
  check('the update\\'s download does not say how fast', /3 MB\\/s/.test($('#newer-step').textContent));
  check('the update\\'s download does not say how long is left', /10 s left/.test($('#newer-step').textContent));
  check('the update\\'s bar is not filled to the share downloaded', parseFloat($('#newer-fill').style.width) === 25 &&
    !$('#newer-fill').classList.contains('working') && $('#newer-track').getAttribute('aria-valuenow') === '25');
  check('a download under way can be sent elsewhere', !visible('newer-open'));
  window.__stub.update({ state: 'verifying' });
  await pause();
  check('checking the file is not said', /Checking 0\\.2\\.0/.test(notice()) &&
    /release workflow/.test($('#newer-step').textContent));
  check('a step with no total has a bar that invents one', $('#newer-fill').classList.contains('working') &&
    !$('#newer-track').hasAttribute('aria-valuenow'));
  window.__stub.update({ state: 'installing' });
  await pause();
  check('installing does not say the app closes and opens again',
    /Installing 0\\.2\\.0/.test(notice()) && /opens again as 0\\.2\\.0/.test($('#newer-step').textContent));
  window.__stub.update({ state: 'failed', why: 'its signature does not verify: the log has no entry', file: false });
  await pause();
  check('a failed update does not say nothing was installed and why',
    /The update to 0\\.2\\.0 stopped/.test(notice()) &&
    /Nothing was installed, because its signature does not verify: the log has no entry\\./.test($('#newer-why').textContent));
  check('a failed update does not offer the release page', visible('newer-open') &&
    /Open the release page/.test($('#newer-open').textContent));
  check('a failed update cannot be tried again', visible('newer-update') && /Try again/.test($('#newer-update').textContent));
  check('a failed update offers a file it never kept', !visible('newer-show'));
  check('a failed update still shows its bar', !visible('newer-progress'));
  $('#newer-open').click();
  await pause();
  check('the release page is not opened after a failed update',
    last('plugin:opener|open_url') && last('plugin:opener|open_url').args.url === release);
  $('#newer-update').click();
  await pause();
  check('trying again does not ask the core again', called('update_now').length === 2);
  window.__stub.update({ state: 'failed', why: 'the password prompt was closed', file: true });
  await pause();
  check('a verified package left on disk is not offered', visible('newer-show'));
  $('#newer-show').click();
  await pause();
  check('showing the downloaded file does not ask the core', called('show_update_file').length === 1);
  window.__stub.update({ state: 'idle', why: 'scoop installed this copy, so Scoop updates it: scoop update steamgauge' });
  await pause();
  check('a copy the window does not update is offered Update now', !visible('newer-update'));
  check('a copy the window does not update is not told why, as a sentence',
    /^Scoop installed this copy.*scoop update steamgauge\\.$/.test($('#newer-why').textContent));
  check('a copy the window does not update is not sent to its release page',
    visible('newer-open') && /Download it from its release page/.test($('#newer-open').textContent));
  window.__stub.update({ state: 'idle', why: null });
  await pause();
  check('the notice keeps an old reason once the copy can update', !visible('newer-why') && visible('newer-update'));

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
  check('a waiting job is not drawn as waiting', /waiting its turn/.test($('#cockpit-work .job.queued').textContent));
  check('a job runs off the side of the cockpit\\'s column', Array.prototype.every.call(
    document.querySelectorAll('#cockpit-work .job'), function (j) { return j.scrollWidth <= j.clientWidth + 1; }));
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
  check('the comparison\\'s bars go unexplained', shown('compare-legend'));
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
  var sizes = document.querySelectorAll('#reader-sizes input');
  check('settings do not offer both readers', sizes.length === 2);
  check('the recommended reader is not marked', /Recommended/.test($('#reader-sizes').textContent));
  check('the reader in use is not the one chosen', $('#reader-sizes input:checked').value === 'standard');
  check('the card behind the recommendation goes unnamed', /RTX 4090/.test($('#reader-machine').textContent));
  check('a reader still to fetch does not say its size', /244 MB download/.test($('#reader-sizes').textContent));
  sizes[0].click();
  await pause(250);
  check('choosing the small reader is not saved', last('save_settings').args.settings.reader === 'small');
  $('#reader-sizes input[value="standard"]').click();
  await pause(250);
  check('the recommended reader is kept as a choice rather than as the default',
    last('save_settings').args.settings.reader === null);
  check('keeping games up to date is not on as it is by default', $('#keep-up-to-date').checked && !$('#keep-up-to-date').disabled);
  check('a notification is on before anyone asked for one', !$('#notify-moves').checked);
  $('#notify-moves').click();
  await pause(250);
  check('asking for notifications is not saved', last('save_settings').args.settings.notify_moves === true &&
    last('save_settings').args.settings.keep_up_to_date === true);
  $('#keep-up-to-date').click();
  await pause(250);
  check('turning off keeping games up to date is not saved', last('save_settings').args.settings.keep_up_to_date === false);
  $('#keep-up-to-date').click();
  await pause(250);
  $('#check-steam').click();
  await pause(250);
  check('keeping games up to date can be chosen without Steam\\'s counts to go by',
    last('save_settings').args.settings.check_steam === false && $('#keep-up-to-date').disabled);
  $('#check-steam').click();
  await pause(250);
  check('asking Steam again does not let games be kept up to date', !$('#keep-up-to-date').disabled);
  check('settings do not fit their page', fits());

  await go('storage');
  await pause(250);
  check('storage does not say what the app takes', /SteamGauge takes/.test($('#storage-sub').textContent));
  check('storage does not draw the drive', document.querySelectorAll('#drives .drive').length === 1);
  check('storage does not list every part of the library', document.querySelectorAll('#parts .part-row').length === 6);
  check('storage does not list the games largest first',
    document.querySelectorAll('#game-rooms .room').length === 3 &&
    /Alpha/.test(document.querySelector('#game-rooms .room').textContent));
  check('storage does not list the models', document.querySelectorAll('#model-rooms .room').length === 4);
  check('the library\\'s place goes unsaid', /AppData/.test($('#storage-library').textContent));
  var earlier = Array.prototype.find.call(document.querySelectorAll('#parts .part-row'), function (row) {
    return /Earlier downloads/.test(row.textContent); });
  earlier.querySelector('button').click();
  await pause(250);
  check('removing earlier downloads everywhere is not asked of the core',
    last('free_room') && last('free_room').args.appId === null && last('free_room').args.what === 'earlier');
  var reads = document.querySelector('#game-rooms .room select');
  reads.value = 'reads';
  reads.dispatchEvent(new Event('change'));
  await pause(150);
  check('removing a game\\'s reads does not ask first', /Hours|hours/.test($('#game-rooms').textContent) &&
    document.querySelector('#game-rooms .room button.danger') !== null);
  var before = called('free_room').length;
  document.querySelector('#game-rooms .room button.ghost').click();
  await pause(150);
  check('keeping a game\\'s reads still removes them', called('free_room').length === before);
  check('storage does not fit its page', fits());
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

  await go('settings');
  check('Settings does not say how to add SteamGauge to Claude Code',
    /^claude mcp add steamgauge -- ".*steamgauge\\.exe" mcp$/.test($('#claude-command').textContent));
  check('answering over HTTP is on before anyone asked for it', !$('#answer-over-http').checked && !shown('http-reach'));
  $('#answer-over-http').click();
  await pause(250);
  check('switching on HTTP is not saved', last('save_settings').args.settings.answer_over_http === true);
  check('switched on, the address and token go unshown', shown('http-reach') &&
    $('#http-address').textContent === 'http://127.0.0.1:47800/mcp' && $('#http-token').textContent === '0f'.repeat(32));
  $('#new-http-token').click();
  await pause(250);
  check('a new token is not shown once drawn', called('new_http_token').length === 1 && $('#http-token').textContent === 'a1'.repeat(32));
  $('#http-port').value = '47801';
  $('#http-port').dispatchEvent(new Event('change', { bubbles: true }));
  await pause(250);
  check('a port another program holds goes unsaid', last('save_settings').args.settings.http_port === 47801 &&
    shown('http-problem') && /another program has it/.test($('#http-problem').textContent) && !shown('http-reach'));
  $('#http-port').value = '47800';
  $('#http-port').dispatchEvent(new Event('change', { bubbles: true }));
  $('#answer-over-http').click();
  await pause(250);
  check('switched off, the token is still shown', !shown('http-reach') && !shown('http-problem') &&
    last('save_settings').args.settings.answer_over_http === false);
  check('Settings does not fit its page', fits());

  await go('library');
  rows().filter(function (r) { return /Gamma/.test(r.textContent); })[0].querySelector('.game-link').click();
  await pause(300);
  check('a game page does not open', shown('game'));
  check('a game whose page was never seen claims a last look', !shown('game-since'));
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
  check('a game\\'s page does not say what changed since it was last seen', shown('game-since') &&
    /^Since you last looked, on .*: 1,520 new reviews\\.$/.test($('#game-since p').textContent) &&
    $('#game-since .since-move.complaint.up') !== null && $('#game-since').classList.contains('moved'));
  check('looking at a game\\'s page is not recorded', called('looked').some(function (call) { return call.args.appId === 1; }));
  $('#game-since .since-move .link').click();
  await pause(300);
  check('a subject that moved since does not open onto its points', shown('evidence') &&
    last('claims_behind') && last('claims_behind').args.subject === 'performance');
  check('a subject\\'s page does not name the game it goes back to', $('#back').textContent.trim() === 'Alpha');
  check('a subject\\'s page leaves the rail with no place marked',
    $('[data-go="library"]').getAttribute('aria-current') === 'true');
  $('#back').click();
  await pause(300);
  check('a language is not named as a person writes it', /English 75%/.test($('#languages').textContent));
  check('a month\\'s bar is not held to a column\\'s width',
    Number($('#timeline-svg .bar').getAttribute('width')) <= 56);

  // Alpha's seven updates: one posted before its first month, one alone, three within four
  // days of each other, one with too few reviews either side, one held only nine days after.
  check('the timeline does not draw each update it holds a month for',
    document.querySelectorAll('#timeline-svg .update-line').length === 6);
  var lines = Array.prototype.slice.call(document.querySelectorAll('#timeline-svg .update-line'));
  var hits = Array.prototype.slice.call(document.querySelectorAll('#timeline-svg .hit'));
  check('an update is drawn over the months it should sit under',
    lines.every(function (line) { return hits.every(function (hit) {
      return line.compareDocumentPosition(hit) & Node.DOCUMENT_POSITION_FOLLOWING; }); }));
  var marks = function () { return Array.prototype.slice.call(document.querySelectorAll('#update-strip .update-mark')); };
  check('updates days apart are not one mark that says how many it holds',
    marks().length === 4 && marks().some(function (m) { return m.textContent === '3'; }));
  check('a mark is not named for the update it chooses', marks().every(function (m) {
    return (m.getAttribute('aria-label') || '').length > 0; }));
  check('a mark is not where its update falls under the chart', (function () {
    var strip = $('#update-strip').getBoundingClientRect();
    return marks().every(function (m) {
      var box = m.getBoundingClientRect();
      return box.left >= strip.left - 1 && box.right <= strip.right + 1;
    });
  })());
  check('the updates cannot be chosen from a list, newest first',
    document.querySelectorAll('#update-choice option').length === 8 &&
    /Patch 1\\.4/.test($('#update-choice option:nth-child(2)').textContent));
  check('the updates are not introduced', /7 updates its developer posted on Steam/.test($('#around-lede').textContent));
  marks().filter(function (m) { return m.textContent === '3'; })[0].click();
  await pause(250);
  check('a mark for three updates does not choose the newest of them',
    last('before_after') && last('before_after').args.gid === '1840000000000004' &&
    $('#update-choice').value === '1840000000000004');
  check('the chosen update is not marked on the chart',
    $('#timeline-svg .update-line.chosen') && $('#timeline-svg .update-line.chosen').dataset.gid === '1840000000000004' &&
    $('#update-strip .update-mark.chosen') !== null);
  var pick = function (gid) {
    var select = $('#update-choice');
    select.value = gid;
    select.dispatchEvent(new Event('change'));
    return pause(250);
  };
  await pick('1840000000000002');
  check('choosing an update does not show what changed across it', shown('around-body'));
  check('an update does not say how many reviews either side it rests on',
    /1,840 reviews in the 28 days before, 2,960 in the 28 days after/.test($('#around-counts').textContent));
  check('the updates sharing its weeks go unmentioned', /2 other updates were posted within these weeks/.test($('#around-counts').textContent));
  check('a change across an update is not said with both shares',
    /complaints about bugs and crashes rose from 8% to 19%/.test($('#around-summary').textContent) &&
    /the share recommending the game fell from 84% to 71%/.test($('#around-summary').textContent));
  check('a complaint that rose is not marked as bad news',
    Array.prototype.some.call(document.querySelectorAll('#around-summary .worse'), function (w) {
      return /complaints about bugs/.test(w.textContent); }));
  check('praise that rose is not marked as good news', $('#around-summary .better') !== null &&
    /praise of performance/.test($('#around-summary .better').textContent));
  check('every subject is not shown before and after', document.querySelectorAll('#around-rows tr').length === 3);
  check('the changes are not marked in the table', document.querySelectorAll('#around-rows .changed').length === 2);
  check('a share within chance is marked as a change',
    !/changed/.test(document.querySelectorAll('#around-rows tr')[2].innerHTML));
  check('the share recommending the game is not shown either side',
    shown('around-recommended') && /84%\\s*→\\s*71%/.test($('#around-recommended').textContent) &&
    $('#around-recommended .changed.worse') !== null);
  check('the rule a change is held to goes unsaid', /three standard errors/.test($('#around-footnote').textContent));
  $('#around-steam').click();
  await pause();
  check('an update\\'s post does not open on Steam through the opener',
    last('plugin:opener|open_url').args.url ===
      'https://store.steampowered.com/news/externalpost/steam_community_announcements/1840000000000002');
  await pick('1840000000000005');
  check('an update with too few reviews either side does not say so', shown('around-thin') &&
    /Too few reviews/.test($('#around-thin').textContent));
  check('an update with too few reviews either side still shows shares',
    document.getElementById('around-table-wrap').hidden && document.getElementById('around-summary').hidden);
  check('an update with too few reviews does not say how few', /64 reviews in the 28 days before, 41 in/.test($('#around-counts').textContent));
  await pick('1840000000000006');
  check('an update held only days after does not say how many', /the 9 days since/.test($('#around-counts').textContent));
  check('an update that changed nothing does not say so', /Nothing changed beyond chance/.test($('#around-summary').textContent));
  await pick('');
  check('choosing no update leaves one shown', !shown('around-body') && $('#update-strip .update-mark.chosen') === null);
  check('the game page with an update chosen does not fit its page', fits());
  $('#game-store').click();
  await pause();
  check('a game\\'s store page does not open through the opener',
    last('plugin:opener|open_url').args.url === 'https://store.steampowered.com/app/1/');

  check('a read game cannot export its data', shown('export-jobs') === false && !$('#export-data').disabled);
  $('#export-data').click();
  await pause();
  check('exporting the data does not ask the core for this game\\'s',
    last('export_data') && last('export_data').args.appId === 1);
  check('the data being written is not shown under the button',
    shown('export-jobs') && /Export the data/.test($('#export-jobs').textContent));
  check('the data being written is shown at the top of the page as well',
    !/Export the data/.test($('#game-jobs').textContent));
  check('the data can be asked for again while it is being written', $('#export-data').disabled);
  window.__stub.board([{ id: 77, task: { kind: 'export_data', app_id: 1, to: 'C:\\\\Alpha data' }, name: 'Alpha',
    state: 'done', step: 'Writing the points', unit: 'reviews', done: 25900, total: 25900, rate: null, left: null,
    note: 'Saved 78,000 points and the figures in C:\\\\Alpha data.', queued: 0, started: 0, ended: 0 }]);
  await pause();
  var folder = Array.prototype.find.call(document.querySelectorAll('#export-jobs button'), function (b) {
    return b.textContent === 'Open the folder'; });
  check('saved data cannot be opened', folder !== undefined);
  check('the data cannot be exported again once it is saved', !$('#export-data').disabled);
  if (folder) { folder.click(); await pause(); }
  check('opening the saved data does not name its job to the core',
    last('open_report') && last('open_report').args.id === 77);
  window.__stub.refuse('C:\\\\Alpha - SteamGauge data is there already; choose a name nothing has yet');
  $('#export-data').click();
  await pause();
  check('a refused export goes unsaid', shown('export-note') && /is there already/.test($('#export-note').textContent));
  check('a refused export leaves the button off', !$('#export-data').disabled);
  window.__stub.refuse(null);
  window.__stub.board([]);
  await pause();
  check('the game page with its data saved does not fit its page', fits());
  $('#topic-rows .subject').click();
  await pause(300);
  check('a subject does not show the points behind it', document.querySelectorAll('#quotes li').length === 2);
  check('a count of points is not said in words', /^2 points about this/.test($('#evidence-lede').textContent));
  check('a single page of points offers pages', !shown('paging'));
  check('the points behind the whole game are narrowed to one kind of reviewer', last('claims_behind').args.who === null);
  $('#back').click();
  await pause(300);

  check('a read game does not say who wrote its reviews', shown('who') && shown('who-pick') && !shown('who-recount'));
  check('the kinds of reviewer are not offered under the questions that make them',
    document.querySelectorAll('#who-first optgroup').length === 4);
  check('a kind with too few reviews to count can be chosen', $('#who-first option[value="steam-deck"]').disabled);
  check('where the kinds of reviewer differ is not listed',
    shown('who-differ') && document.querySelectorAll('#who-findings li').length === 6 && shown('who-more'));
  $('#who-more').click();
  await pause();
  check('the rest of where they differ does not come when asked for',
    document.querySelectorAll('#who-findings li').length === 7 && !shown('who-more'));
  $('#who-findings li .link').click();
  await pause(300);
  check('showing a kind does not ask for it beside everyone else',
    last('who_wrote') && last('who_wrote').args.these === '100-hours-or-more' && last('who_wrote').args.others === null);
  check('a kind chosen still shows the table of everyone', !shown('counts-wrap') && shown('who-wrap') && shown('who-legend'));
  check('where they differ stays listed beside one kind', !shown('who-differ'));
  check('a subject neither side raised is listed', document.querySelectorAll('#who-rows tr').length === 2);
  check('the subjects are not in the order the first kind raises them',
    /^Performance/.test($('#who-rows tr').textContent));
  check('a gap wider than chance is not marked', /More complaints/.test($('#who-rows tr').textContent) &&
    /Raised more/.test($('#who-rows tr').textContent));
  check('a gap chance could make is marked', !/More praise/.test($('#who-rows tr').textContent) &&
    document.querySelectorAll('#who-rows tr')[1].querySelector('.who-marks') === null);
  check('a share is drawn without where it would likely fall', document.querySelectorAll('#who-rows .range').length === 4);
  check('the timeline does not follow the kind chosen', /Aug 2025: 60 reviews/.test($('#timeline-svg').textContent));
  // That kind's timeline runs from July to September 2025, which holds one of Alpha's updates.
  check('the updates are not marked on one kind\\'s timeline',
    document.querySelectorAll('#timeline-svg .update-line').length === 1 &&
    document.querySelectorAll('#update-strip .update-mark').length === 1);
  await pick('1840000000000006');
  check('an update is not counted over the kind of reviewer chosen, and does not say so',
    last('before_after').args.kind === '100-hours-or-more' &&
    /Counted over one kind of reviewer alone, 100 hours or more/.test($('#around-counts').textContent));
  check('the page does not say whose reviews it shows',
    /Reviewers with 100 hours or more played wrote 2,700 of these reviews/.test($('#who-note').textContent) &&
    /beside everyone else/.test($('#who-note').textContent));
  $('#who-others').value = 'under-2-hours';
  $('#who-others').dispatchEvent(new Event('change'));
  await pause(300);
  check('a second kind is not asked for beside the first', last('who_wrote').args.others === 'under-2-hours');
  check('the second kind is not named over its column', /Under 2 hours/.test($('#who-head').textContent));
  $('#who-first').value = 'got-it-free';
  $('#who-first').dispatchEvent(new Event('change'));
  await pause(300);
  check('a question with two answers offers a choice of one',
    !shown('who-others') && shown('who-others-fixed') && /paid for it/.test($('#who-others-fixed').textContent));
  $('#who-first').value = '100-hours-or-more';
  $('#who-first').dispatchEvent(new Event('change'));
  await pause(300);
  $('#who-rows .subject').click();
  await pause(300);
  check('the points behind a kind of reviewer are not narrowed to their reviews',
    last('claims_behind').args.who === '100-hours-or-more');
  check('the points behind a kind of reviewer do not say whose they are',
    /from reviewers with 100 hours or more played/.test($('#evidence-lede').textContent));
  check('words counted over every review are shown beside one kind', !shown('stands-out'));
  $('#back').click();
  await pause(400);
  check('coming back from the points forgets the kind chosen',
    shown('who-wrap') && $('#who-first').value === '100-hours-or-more');
  $('#who-everyone').click();
  await pause(300);
  check('back to everyone does not bring back the whole game',
    shown('counts-wrap') && !shown('who-wrap') && shown('who-differ') && /Aug 2025: 307 reviews/.test($('#timeline-svg').textContent));
  check('back to everyone does not count the update on show over every reviewer again',
    last('before_after').args.kind === null && /^Counted over every reviewer/.test($('#around-counts').textContent) &&
    document.querySelectorAll('#timeline-svg .update-line').length === 6);
  await pick('');
  $('#game-back').click();
  await pause(250);
  rows().filter(function (r) { return /Beta/.test(r.textContent); })[0].querySelector('.game-link').click();
  await pause(300);
  check('a game counted before reviewers were told apart does not offer to count it again',
    shown('who') && shown('who-recount') && !shown('who-pick') && !shown('who-differ'));
  check('a game with nothing new since it was last seen does not say so plainly', shown('game-since') &&
    /^Nothing new since you last looked, on .*\\.$/.test($('#game-since').textContent) &&
    !$('#game-since').classList.contains('moved'));
  $('#do-recount').click();
  await pause();
  check('counting again does not queue a recount of the game', called('queue').some(function (call) {
    return call.args.tasks.some(function (t) { return t.kind === 'recount' && t.app_id === 2; }); }));
  $('#game-back').click();
  await pause(250);
  check('the way back from a game does not lead to the library', shown('library'));
  rows().filter(function (r) { return /Beta/.test(r.textContent); })[0].querySelector('.game-link').click();
  await pause(300);
  check('a game Steam was never asked about does not say how to ask',
    shown('around') && /has not been asked/.test($('#around-lede').textContent) &&
    $('#around-choose').hidden && document.getElementById('update-strip').hidden);
  await go('library');

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
  check('a game being downloaded does not say what its page will show', /once the reviews are downloaded/.test($('#game-note').textContent));
  check('a game being downloaded is called not downloaded', !/Not downloaded/.test($('#game').textContent));
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
  // Opens `url` afresh. The page about to be left is marked first, because right after a
  // navigation it can still answer as ready and be checked in place of the new one.
  let loads = 0;
  const load = async (url) => {
    await evaluate("window.__left = true");
    // A query of its own every time, so each load is a navigation and never a jump within the
    // page that a reload could overtake.
    loads += 1;
    await send("Page.navigate", { url: `${url}${url.includes("?") ? "&" : "?"}load=${loads}` });
    for (let attempt = 0; attempt < 80; attempt += 1) {
      const ready = await evaluate("document.readyState === 'complete' && !!window.__stub && !window.__left");
      if (ready.result?.result?.value === true) return;
      await sleep(250);
    }
    throw new Error(`${url} did not load`);
  };
  await load(page);

  const answer = await evaluate(PROBE);
  // The narrowest window the app allows, where the library's table is the first thing to
  // run off the side.
  await send("Emulation.setDeviceMetricsOverride", { width: 760, height: 560, deviceScaleFactor: 1, mobile: false });
  const narrow = await evaluate(`(async function () {
    var wrong = [];
    for (var name of ['cockpit', 'library', 'compare', 'storage', 'settings']) {
      document.querySelector('[data-go="' + name + '"]').click();
      await new Promise(function (done) { setTimeout(done, 250); });
      var stage = document.getElementById('stage');
      if (stage.scrollWidth > stage.clientWidth + 1) wrong.push('the ' + name + ' runs off the side of the narrowest window');
    }
    document.querySelector('[data-go="library"]').click();
    await new Promise(function (done) { setTimeout(done, 250); });
    Array.prototype.filter.call(document.querySelectorAll('#library-rows tr'), function (r) {
      return /Alpha/.test(r.textContent); })[0].querySelector('.game-link').click();
    await new Promise(function (done) { setTimeout(done, 400); });
    var exportBox = document.getElementById('export-data').getBoundingClientRect();
    if (exportBox.width === 0 || exportBox.right > document.getElementById('stage').getBoundingClientRect().right + 1) {
      wrong.push('the way to export a game\\'s data is out of sight in the narrowest window');
    }
    var first = document.getElementById('who-first');
    first.value = '100-hours-or-more';
    first.dispatchEvent(new Event('change'));
    await new Promise(function (done) { setTimeout(done, 300); });
    var stage = document.getElementById('stage');
    if (stage.scrollWidth > stage.clientWidth + 1) wrong.push('one kind of reviewer runs off the side of the narrowest window');
    var frame = document.getElementById('who-wrap');
    if (frame.hidden || frame.scrollWidth > frame.clientWidth + 1) {
      wrong.push('one kind of reviewer beside everyone else needs scrolling sideways in the narrowest window');
    }
    var choice = document.getElementById('update-choice');
    choice.value = '1840000000000002';
    choice.dispatchEvent(new Event('change'));
    await new Promise(function (done) { setTimeout(done, 250); });
    var stage = document.getElementById('stage');
    if (stage.scrollWidth > stage.clientWidth + 1) wrong.push('a game\\'s update runs off the side of the narrowest window');
    var marks = Array.prototype.slice.call(document.querySelectorAll('#update-strip .update-mark'));
    var overlap = marks.some(function (mark, index) {
      var next = marks[index + 1];
      return next && mark.getBoundingClientRect().right > next.getBoundingClientRect().left;
    });
    if (overlap) wrong.push('two marks under the chart overlap in the narrowest window');
    return wrong;
  })()`);
  // The library's table scrolls inside its own frame, so the page never shows it running off;
  // every width from the narrowest to a common laptop's is asked whether the table fits.
  const sideways = [];
  for (const width of [760, 900, 1080, 1180, 1220, 1280, 1366]) {
    await send("Emulation.setDeviceMetricsOverride", { width, height: 700, deviceScaleFactor: 1, mobile: false });
    const fits = await evaluate(`(async function () {
      document.querySelector('[data-go="library"]').click();
      await new Promise(function (done) { setTimeout(done, 250); });
      var frame = document.querySelector('#library .table-wrap');
      var fits = frame.scrollWidth <= frame.clientWidth + 1;
      // Another system's fonts set the same columns wider; text spaced out stands in for them.
      document.querySelector('table.games').style.letterSpacing = '0.12em';
      document.querySelector('[data-go="library"]').click();
      await new Promise(function (done) { setTimeout(done, 250); });
      fits = fits && frame.scrollWidth <= frame.clientWidth + 1;
      document.querySelector('table.games').style.letterSpacing = '';
      return fits;
    })()`);
    if (fits.result?.result?.value !== true) sideways.push(`the library's table needs scrolling sideways at ${width} pixels`);
  }

  // The first visit: an empty library, where the cockpit is a welcome that finds the first game.
  await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 860, deviceScaleFactor: 1, mobile: false });
  await load(`${page}?first`);
  const first = await evaluate(`(async function () {
    var wrong = [];
    var check = function (claim, ok) { if (!ok) { wrong.push(claim); } };
    var pause = function (ms) { return new Promise(function (done) { setTimeout(done, ms || 80); }); };
    var shown = function (id) { return !document.getElementById(id).hidden; };
    await pause(400);
    check('an empty library is not welcomed', shown('cockpit-empty') && !shown('cockpit-full'));
    check('a first visit does not offer the reader', shown('welcome-ready'));
    check('the reader on offer does not say its size and the room for it',
      /1\.1 GB download/.test(document.getElementById('ready-room').textContent) &&
      /182\.0 GB free/.test(document.getElementById('ready-room').textContent));
    check('the reader on offer does not say why', /RTX 4090/.test(document.getElementById('ready-why').textContent));
    document.getElementById('ready-fetch').click();
    await pause(300);
    var fetched = window.__stub.calls.filter(function (call) { return call.command === 'queue'; }).pop();
    check('downloading the reader does not queue it',
      fetched && fetched.args.tasks.some(function (t) { return t.kind === 'fetch_reader'; }));
    check('the reader downloading takes the welcome away', shown('cockpit-empty'));
    check('the reader downloading is not shown where it was offered',
      document.querySelector('#ready-job .job') !== null && !shown('ready-actions'));
    var box = document.getElementById('welcome-query');
    var examplesAt = document.getElementById('welcome-examples').getBoundingClientRect().top;
    box.value = 'delta';
    box.dispatchEvent(new Event('input'));
    await pause(600);
    var found = document.querySelectorAll('#welcome-results .result');
    check('the welcome does not find a game by its name', found.length === 2 && /Delta/.test(found[0].textContent));
    var under = document.getElementById('welcome-results').getBoundingClientRect().top -
      box.getBoundingClientRect().bottom;
    check('the welcome\\'s matches do not drop straight under the box, over what follows',
      under >= 0 && under < 24 && document.getElementById('welcome-examples').getBoundingClientRect().top === examplesAt);
    if (found.length > 0) { found[0].click(); await pause(300); }
    check('choosing a game the welcome found does not offer it', shown('finder') && shown('found') &&
      /Delta/.test(document.getElementById('found-name').textContent));
    document.querySelector('[data-go="library"]').click();
    await pause(250);
    check('an empty library does not say so', shown('library-empty') && !shown('library-full'));
    document.querySelector('[data-go="compare"]').click();
    await pause(250);
    check('an empty comparison explains bars it does not show', shown('compare-empty') && !shown('compare-legend'));
    var finder = document.getElementById('appid');
    document.getElementById('rail-add').click();
    await pause(200);
    finder.value = 'https://store.steampowered.com/app/4/Delta/';
    document.getElementById('lookup-form').requestSubmit();
    await pause(250);
    var asked = window.__stub.calls.filter(function (call) { return call.command === 'look_up'; }).pop();
    check('a store link is not read for its app ID', asked && asked.args.appId === 4);
    return wrong;
  })()`);

  await load(`${page}?first`);
  const later = await evaluate(`(async function () {
    var pause = function (ms) { return new Promise(function (done) { setTimeout(done, ms || 80); }); };
    await pause(400);
    document.getElementById('ready-later').click();
    await pause(100);
    var put = document.getElementById('welcome-ready').hidden;
    var queued = window.__stub.calls.some(function (call) { return call.command === 'queue'; });
    try { localStorage.removeItem('reader-later'); } catch (e) {}
    return [].concat(put ? [] : ['the reader put off for later stays on offer'])
      .concat(queued ? ['the reader put off for later is downloaded anyway'] : []);
  })()`);

  // A walk through every page and state, light and dark, wide and at the narrowest window, each
  // one checked for what a screen reader, a keyboard or weaker eyes would find wrong with it,
  // and pictured when --shots asks.
  const unreachable = [];
  {
    if (shots) await mkdir(shots, { recursive: true });
    if (pages) await mkdir(pages, { recursive: true });
    // Alpha's data being written, or written, as the board would hold it.
    const exporting = (state) =>
      "var at=Math.floor(Date.now()/1000);window.__stub.board([{id:96,task:{kind:'export_data',app_id:1," +
      "to:'C:\\\\Users\\\\someone\\\\Documents\\\\Alpha - SteamGauge data'},name:'Alpha',state:'" + state + "'," +
      "step:'Writing the points',unit:'reviews',done:" + (state === "done" ? 25900 : 9400) + ",total:25900," +
      "rate:" + (state === "done" ? "null" : 4100) + ",left:" + (state === "done" ? "null" : 4) + ",note:" +
      (state === "done"
        ? "'Saved 78,000 points and the figures in C:\\\\Users\\\\someone\\\\Documents\\\\Alpha - SteamGauge data.'"
        : "null") +
      ",queued:at-10,started:at-5,ended:" + (state === "done" ? "at" : "null") + "}])";
    // Every page whole, rather than the window's height of it.
    const unroll =
      "document.body.style.overflow='visible';document.querySelector('.frame').style.height='auto';" +
      "document.querySelector('.frame').style.minHeight='100vh';" +
      "document.getElementById('stage').style.overflow='visible';";
    const shoot = async (name) => {
      await sleep(400);
      unreachable.push(...(await audit(evaluate, name)));
      // The words are the same in either scheme, so one copy is kept.
      if (pages && !name.endsWith("-dark")) {
        const html = await evaluate("document.documentElement.outerHTML");
        await writeFile(join(pages, `${name}.html`), `<!doctype html>\n${html.result.result.value}`);
      }
      if (!shots) return;
      const picture = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
      await writeFile(join(shots, `${name}.png`), Buffer.from(picture.result.data, "base64"));
    };
    for (const scheme of ["light", "dark"]) {
      await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
      await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 860, deviceScaleFactor: 1, mobile: false });

      await load(`${page}?first`);
      await sleep(400);
      await evaluate(unroll);
      await shoot(`welcome-${scheme}`);
      await evaluate(
        "var box=document.getElementById('welcome-query');box.value='delta';box.dispatchEvent(new Event('input'))",
      );
      await shoot(`welcome-search-${scheme}`);

      await load(page);
      await sleep(400);
      await evaluate(unroll);
      for (const name of ["cockpit", "library", "compare", "storage", "settings"]) {
        await evaluate(`document.querySelector('[data-go="${name}"]').click()`);
        await shoot(`${name}-${scheme}`);
      }
      // A comparison with games in it, as choosing two in the library opens it.
      await evaluate("document.querySelector('[data-go=\"library\"]').click()");
      await sleep(300);
      // The table is drawn again after each tick, so each row is found afresh.
      for (const row of [0, 1]) {
        await evaluate(`document.querySelectorAll('#library-rows tr input')[${row}].click()`);
        await sleep(150);
      }
      await evaluate("document.getElementById('selected-compare').click()");
      await shoot(`compare-chosen-${scheme}`);
      // The cockpit's first card when nothing changed since the last look, and on a first look.
      await evaluate("document.querySelector('[data-go=\"cockpit\"]').click();window.__stub.lately('calm')");
      await shoot(`cockpit-since-calm-${scheme}`);
      await evaluate("window.__stub.lately('first')");
      await shoot(`cockpit-since-first-${scheme}`);
      await evaluate("window.__stub.lately('moved')");
      // The cockpit while work runs: a download with its pace, a read, one waiting and one done.
      await evaluate(
        "var at=Math.floor(Date.now()/1000);var job=function(id,kind,app,name,state,step,unit,done,total,rate,left,note){" +
          "return {id:id,task:{kind:kind,app_id:app},name:name,state:state,step:step,unit:unit,done:done,total:total," +
          "rate:rate,left:left,note:note,queued:at-600,started:state==='queued'?null:at-300,ended:state==='done'?at-60:null};};" +
          "window.__stub.board([" +
          "job(91,'update',1,'Alpha','running','Downloading','reviews',6200,21000,48,310,null)," +
          "job(92,'read',2,'Beta','running','Reading','points',12000,18000,900,7,null)," +
          "job(93,'read',3,'Gamma','queued','Waiting','points',0,null,null,null,null)," +
          "job(94,'update',3,'Gamma','done','Done','reviews',900,900,null,null,'900 reviews downloaded.')]);" +
          "document.querySelector('[data-go=\"cockpit\"]').click()",
      );
      await shoot(`working-${scheme}`);
      await evaluate("window.__stub.board([])");
      await evaluate("document.querySelector('[data-go=\"library\"]').click()");
      await sleep(300);
      await evaluate(
        "Array.prototype.filter.call(document.querySelectorAll('#library-rows tr'), function (r) { return /Alpha/.test(r.textContent); })[0].querySelector('.game-link').click()",
      );
      await shoot(`game-${scheme}`);
      // A game's data being written, and then saved, under the button that asked for it.
      await evaluate(exporting("running"));
      await shoot(`game-export-${scheme}`);
      await evaluate(exporting("done"));
      await shoot(`game-export-done-${scheme}`);
      await evaluate("window.__stub.board([])");
      await evaluate(
        "var choice=document.getElementById('update-choice');choice.value='1840000000000005';choice.dispatchEvent(new Event('change'))",
      );
      await shoot(`game-update-thin-${scheme}`);
      await evaluate(
        "var choice=document.getElementById('update-choice');choice.value='1840000000000002';choice.dispatchEvent(new Event('change'))",
      );
      await shoot(`game-update-${scheme}`);
      await evaluate(
        "var first=document.getElementById('who-first');first.value='100-hours-or-more';first.dispatchEvent(new Event('change'))",
      );
      await shoot(`game-who-${scheme}`);
      await evaluate(
        "var others=document.getElementById('who-others');others.value='under-2-hours';others.dispatchEvent(new Event('change'))",
      );
      await shoot(`game-who-beside-${scheme}`);
      await evaluate("document.getElementById('who-everyone').click()");
      await evaluate("document.querySelector('#topic-rows .subject').click()");
      await shoot(`evidence-${scheme}`);
      await evaluate("document.getElementById('rail-add').click()");
      await evaluate(
        "var box=document.getElementById('appid');box.value='4';document.getElementById('lookup-form').requestSubmit()",
      );
      await shoot(`finder-${scheme}`);
      // The first thing after choosing a game: its page, with the download under way.
      await evaluate(
        "document.getElementById('start').click();var at=Math.floor(Date.now()/1000);" +
          "window.__stub.board([{id:95,task:{kind:'download',app_id:4},name:'Delta',state:'running',step:'Downloading'," +
          "unit:'reviews',done:2300,total:12000,rate:52,left:186,note:null,queued:at-60,started:at-45,ended:null}])",
      );
      await shoot(`downloading-${scheme}`);
      await evaluate("window.__stub.board([])");
      await evaluate("document.querySelector('[data-go=\"library\"]').click()");
      await sleep(300);
      await evaluate(
        "Array.prototype.filter.call(document.querySelectorAll('#library-rows tr'), function (r) { return /Gamma/.test(r.textContent); })[0].querySelector('.game-link').click()",
      );
      await shoot(`game-unread-${scheme}`);
      await evaluate("document.querySelector('[data-go=\"library\"]').click()");
      await sleep(300);
      await evaluate(
        "Array.prototype.filter.call(document.querySelectorAll('#library-rows tr'), function (r) { return /Beta/.test(r.textContent); })[0].querySelector('.game-link').click()",
      );
      await shoot(`game-recount-${scheme}`);

      // The rail's notice through every state of Update now, close up.
      await load(page);
      const notice = async (name, progress) => {
        await evaluate(`window.__stub.update(${JSON.stringify(progress)})`);
        await sleep(400);
        unreachable.push(...(await audit(evaluate, `update-${name}-${scheme}`)));
        if (!shots) return;
        const box = await evaluate(
          "JSON.stringify(document.getElementById('newer-version').closest('.rail-foot').getBoundingClientRect())",
        );
        const { x, y, width, height } = JSON.parse(box.result.result.value);
        const clip = { x: Math.max(0, x - 12), y: Math.max(0, y - 12), width: width + 24, height: height + 24, scale: 2 };
        const picture = await send("Page.captureScreenshot", { format: "png", clip });
        await writeFile(join(shots, `update-${name}-${scheme}.png`), Buffer.from(picture.result.data, "base64"));
      };
      await notice("offered", { state: "idle", why: null });
      await notice("downloading", { state: "downloading", done: 31e6, total: 52e6, rate: 4.2e6, left: 5 });
      await notice("verifying", { state: "verifying" });
      await notice("installing", { state: "installing" });
      await notice("failed", {
        state: "failed",
        why: "its signature does not verify: the transparency log has no entry for it",
        file: false,
      });
      await notice("declined", {
        state: "failed",
        why: "the password prompt was closed",
        file: true,
      });
      await notice("elsewhere", {
        state: "idle",
        why: "Scoop installed this copy, so Scoop updates it: scoop update steamgauge",
      });
    }
    // The narrowest window the app allows, where a layout is first to give.
    await send("Emulation.setDeviceMetricsOverride", { width: 760, height: 560, deviceScaleFactor: 1, mobile: false });
    await load(page);
    await sleep(400);
    await evaluate(unroll);
    for (const name of ["cockpit", "library", "storage", "settings"]) {
      await evaluate(`document.querySelector('[data-go="${name}"]').click()`);
      await shoot(`narrow-${name}`);
    }
    await evaluate("document.querySelector('[data-go=\"library\"]').click()");
    await sleep(300);
    await evaluate(
      "Array.prototype.filter.call(document.querySelectorAll('#library-rows tr'), function (r) { return /Alpha/.test(r.textContent); })[0].querySelector('.game-link').click()",
    );
    await shoot("narrow-game");
    await evaluate(exporting("done"));
    await shoot("narrow-game-export");
    await evaluate("window.__stub.board([])");
    await evaluate(
      "var choice=document.getElementById('update-choice');choice.value='1840000000000002';choice.dispatchEvent(new Event('change'))",
    );
    await shoot("narrow-game-update");
    await evaluate(
      "var first=document.getElementById('who-first');first.value='100-hours-or-more';first.dispatchEvent(new Event('change'))",
    );
    await shoot("narrow-game-who");
    await load(`${page}?first`);
    await sleep(400);
    await evaluate(unroll);
    await shoot("narrow-welcome");
  }

  socket.close();
  // A data: or blob: address is the page's own memory, not a request that leaves the machine.
  const fetched = asked.filter(
    (url) =>
      !url.startsWith(`http://127.0.0.1:${server.address().port}/`) &&
      !url.startsWith("data:") &&
      !url.startsWith("blob:"),
  );
  const broke = [answer, narrow, first].find((r) => r.result?.exceptionDetails);
  if (broke) {
    console.error("the window threw while being checked:");
    console.error(broke.result.exceptionDetails.exception?.description ?? broke.result.exceptionDetails.text);
  } else {
    const wrong = []
      .concat(threw.map((said) => `the window reported an error: ${said}`))
      .concat(fetched.length === 0 ? [] : [`the window fetched ${fetched.length} thing(s): ${fetched.slice(0, 5).join(", ")}`])
      .concat(answer.result.result.value)
      .concat(narrow.result.result.value)
      .concat(sideways)
      .concat(first.result.result.value)
      .concat(later.result.result.value)
      .concat(unreachable);
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
