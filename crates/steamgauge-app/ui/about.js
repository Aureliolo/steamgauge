/* About SteamGauge, opened from the sidebar or from its name at the top of it: which version this
   computer has, the newest release GitHub last named, a button that asks GitHub now, and where
   SteamGauge is published. */

import { invoke, listen, el, set, page, copiers, openOutside } from './common.js';
import { startUpdate } from './newer.js';

let about = null;
let update = { state: 'idle', why: null };
let asking = false;
let failure = null;

const underWay = (state) => state === 'downloading' || state === 'verifying' || state === 'installing';

const when = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });
const at = (seconds) => when.format(new Date(seconds * 1000));

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
  /* Not known when, or whether, GitHub answered: a first question that failed, or an answer
     kept by a build that did not time it. Only when it was asked is certain. */
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

function draw() {
  set(el('brand-version'), about ? about.running : '');
  if (!about) return;
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
    el('about-daily'),
    about.daily
      ? 'SteamGauge also asks by itself, at most once a day. Settings turns that off.'
      : 'SteamGauge does not ask by itself; Settings turns the daily question on.',
  );
  set(el('about-site'), about.site);
  set(el('about-repository'), about.repository);
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
}

export function setUpAbout() {
  page('about', el('about'), load);
  copiers(el('about'));
  el('about-check').addEventListener('click', check);
  el('about-update').addEventListener('click', () => {
    update = { state: 'downloading', done: 0, total: null, rate: null, left: null };
    draw();
    startUpdate();
  });
  el('about-release').addEventListener('click', () => {
    if (about?.newer) openOutside(about.newer.url);
  });
  el('about-notes').addEventListener('click', () => {
    if (about) openOutside(about.notes);
  });
  listen('update', (event) => {
    update = event.payload;
    if (about) draw();
  });
  listen('newer-version', load);
  load();
}
