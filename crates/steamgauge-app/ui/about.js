/* About SteamGauge, opened from the sidebar or from its name at the top of it: which version this
   computer has and the newest release GitHub last named, what each release in between changes,
   this copy's details for a report of a problem and whether its build was checked, what
   SteamGauge is built on, and SteamGauge's own pages. */

import { invoke, listen, el, set, page, go, openOutside, LOCALE } from './common.js';
import { startUpdate } from './newer.js';

let about = null;
let update = { state: 'idle', why: null };
let asking = false;
let failure = null;
/* What GitHub said changed, for the span it was asked about, so reopening the page asks nothing. */
let changes = { span: null, list: null, asking: false, why: null };
let noticesFailure = null;

const underWay = (state) => state === 'downloading' || state === 'verifying' || state === 'installing';

const when = new Intl.DateTimeFormat(LOCALE, { dateStyle: 'medium', timeStyle: 'short' });
const at = (seconds) => when.format(new Date(seconds * 1000));

/* The versions the list covers: up to the newer release where there is one, and otherwise since
   the version this copy was updated from. */
function span() {
  if (about?.newer) return { from: about.running, to: about.newer.version, coming: true };
  if (about?.updated) return { from: about.updated.from, to: about.running, coming: false };
  return null;
}

const key = (covered) => (covered ? `${covered.from}..${covered.to}` : null);

function latestLine() {
  if (asking) return 'Asking GitHub for the newest release.';
  if (failure) return `GitHub was not asked, because ${failure}.`;
  if (about.newer) return `SteamGauge ${about.newer.version} is out.`;
  if (about.latest === about.running) return 'This is the newest release.';
  if (about.latest) return `The newest release GitHub named is ${about.latest}.`;
  return about.asked ? 'GitHub did not name a release.' : 'GitHub has not been asked yet.';
}

function askedLine() {
  if (!about.asked) return '';
  if (about.answered === about.asked) return `GitHub last answered on ${at(about.answered)}.`;
  /* GitHub has never answered: every question so far failed. */
  if (about.answered === null) return `GitHub was last asked on ${at(about.asked)}.`;
  return `GitHub could not be reached on ${at(about.asked)}; it last answered on ${at(about.answered)}.`;
}

function updateLine() {
  const version = about.newer?.version;
  if (underWay(update.state)) {
    return { downloading: `Downloading ${version}.`, verifying: `Checking ${version}.`, installing: `Installing ${version}; SteamGauge closes and opens again.` }[update.state];
  }
  if (update.state === 'failed') return `The update to ${version} stopped, and nothing was installed, because ${update.why}.`;
  if (update.why) return `${update.why.charAt(0).toUpperCase()}${update.why.slice(1)}.`;
  return '';
}

function drawRelease(release) {
  const node = document.createElement('section');
  node.className = 'about-release';
  const title = document.createElement('h5');
  set(title, release.version);
  node.append(title);
  if (release.changes.length === 0) {
    const none = document.createElement('p');
    none.className = 'quiet small';
    set(none, 'Nothing in what you download.');
    node.append(none);
    return node;
  }
  const list = document.createElement('ul');
  for (const change of release.changes) {
    const item = document.createElement('li');
    set(item, change);
    list.append(item);
  }
  node.append(list);
  return node;
}

function drawChanges() {
  const covered = span();
  el('about-changes').hidden = covered === null;
  if (covered === null) return;
  set(
    el('about-changes-title'),
    covered.coming ? `What changes when you update to ${covered.to}` : `What changed since ${covered.from}`,
  );
  const kept = changes.span === key(covered) ? changes : { list: null, asking: false, why: null };
  const state = kept.asking
    ? 'Asking GitHub what changed.'
    : kept.why
      ? `GitHub did not say what changed, because ${kept.why}.`
      : kept.list && kept.list.length === 0
        ? 'GitHub lists no release in between.'
        : '';
  el('about-changes-state').hidden = !state;
  set(el('about-changes-state'), state);
  el('about-changes-ask').hidden = kept.asking || kept.list !== null || (about.by_itself && !kept.why);
  set(el('about-changes-ask'), kept.why ? 'Ask again' : 'Ask GitHub what changed');
  el('about-releases').replaceChildren(...(kept.list ?? []).map(drawRelease));
}

async function askChanges() {
  const covered = span();
  if (covered === null) return;
  changes = { span: key(covered), list: null, asking: true, why: null };
  drawChanges();
  try {
    const listed = await invoke('whats_new');
    changes = { span: key(covered), list: listed?.releases ?? [], asking: false, why: null };
  } catch (reason) {
    changes = { span: key(covered), list: null, asking: false, why: String(reason) };
  }
  drawChanges();
}

function drawDetails() {
  el('about-details').replaceChildren(
    ...about.details.flatMap((detail) => {
      const name = document.createElement('dt');
      set(name, detail.name);
      const value = document.createElement('dd');
      set(value, detail.value);
      return [name, value];
    }),
  );
}

/* A list read as a sentence: "a, b and c". */
function listed(parts) {
  return parts.length < 2 ? parts.join('') : `${parts.slice(0, -1).join(', ')} and ${parts.at(-1)}`;
}

function drawCredits() {
  set(
    el('about-built-on'),
    about.built_on.length > 0
      ? `SteamGauge is built on open-source software: ${listed(about.built_on)}, each used under its own licence.`
      : 'SteamGauge is built on open-source software, each used under its own licence.',
  );
  el('about-open-notices').hidden = !about.notices;
  const missing = about.notices ? '' : 'Every release carries their licences and notices; this copy was not built by one.';
  el('about-notices-state').hidden = !missing && !noticesFailure;
  set(el('about-notices-state'), noticesFailure ? `The licences did not open, because ${noticesFailure}.` : missing);
}

async function openNotices() {
  noticesFailure = null;
  try {
    await invoke('open_notices');
  } catch (reason) {
    noticesFailure = String(reason);
  }
  drawCredits();
}

/* The rail points at what changed after an update until the person has seen it. */
function drawRail() {
  const told = about?.updated && !about.updated.seen && !about.newer;
  el('rail-updated').hidden = !told;
  if (told) set(el('rail-updated-says'), `SteamGauge was updated to ${about.running}.`);
}

function draw() {
  set(el('brand-version'), about ? about.running : '');
  drawRail();
  if (!about) return;
  set(el('about-made-by'), `Made by ${about.made_by}.`);
  set(el('about-running'), `This computer has SteamGauge ${about.running}.`);
  set(el('about-latest'), latestLine());
  set(el('about-asked'), askedLine());
  const line = about.newer ? updateLine() : '';
  el('about-update-state').hidden = !line;
  set(el('about-update-state'), line);

  const offered = about.newer && ((update.state === 'idle' && !update.why) || update.state === 'failed');
  el('about-update').hidden = !offered;
  set(el('about-update'), update.state === 'failed' ? 'Try again' : `Update to ${about.newer?.version ?? ''}`);
  el('about-release').hidden = !(about.newer && update.state === 'idle' && update.why);
  el('about-check').disabled = asking || underWay(update.state);
  el('about-check').classList.toggle('primary', !offered);
  el('about-check').classList.toggle('quiet-button', Boolean(offered));

  set(
    el('about-by-itself'),
    about.by_itself
      ? 'SteamGauge also asks by itself, each time it opens and every six hours while it stays open. Settings turns that off.'
      : 'SteamGauge does not ask by itself; Settings turns that on.',
  );
  drawChanges();
  drawDetails();
  drawCredits();
  set(el('about-licence'), `SteamGauge is free software under the ${about.licence} licence.`);
}

async function load() {
  try {
    [about, update] = await Promise.all([invoke('about'), invoke('update_state')]);
  } catch {
    about = null;
  }
  draw();
}

/* Shown: the list is asked for where the person lets SteamGauge ask by itself, and an update's
   changes count as seen. */
async function shown() {
  await load();
  const covered = span();
  if (covered && about.by_itself && changes.span !== key(covered)) askChanges();
  if (about?.updated && !about.updated.seen) {
    invoke('saw_what_changed').catch(() => {});
    about.updated.seen = true;
    drawRail();
  }
}

async function check() {
  asking = true;
  failure = null;
  draw();
  try {
    about = await invoke('check_for_updates');
  } catch (reason) {
    failure = String(reason);
  }
  asking = false;
  draw();
  const covered = span();
  if (covered && changes.span !== key(covered)) askChanges();
}

async function copyDetails() {
  const button = el('about-details-copy');
  const text = about.details.map((detail) => `${detail.name}: ${detail.value}`).join('\n');
  try {
    await navigator.clipboard.writeText(text);
    set(button, 'Copied');
  } catch {
    set(button, 'Select them to copy');
  }
  setTimeout(() => set(button, 'Copy the details'), 2000);
}

export function setUpAbout() {
  page('about', el('about'), shown);
  el('about-check').addEventListener('click', check);
  el('about-update').addEventListener('click', () => {
    update = { state: 'downloading', done: 0, total: null, rate: null, left: null };
    draw();
    startUpdate();
  });
  el('about-release').addEventListener('click', () => {
    if (about?.newer) openOutside(about.newer.url);
  });
  el('about-changes-ask').addEventListener('click', askChanges);
  el('about-details-copy').addEventListener('click', () => {
    if (about) copyDetails();
  });
  for (const [id, address] of [
    ['about-site', 'site'],
    ['about-repository', 'repository'],
    ['about-issues', 'issues'],
    ['about-notes', 'notes'],
    ['about-checking', 'checking'],
  ]) {
    el(id).addEventListener('click', () => {
      if (about) openOutside(about[address]);
    });
  }
  el('rail-updated-show').addEventListener('click', () => go('about'));
  el('about-open-notices').addEventListener('click', openNotices);
  listen('update', (event) => {
    update = event.payload;
    if (about) draw();
  });
  listen('newer-version', load);
  load();
}
