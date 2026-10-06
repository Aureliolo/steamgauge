/* The notice that a newer SteamGauge is out, and the update it offers. The core asks GitHub at
   most once a day and keeps the answer; the window shows what it kept, and looks again when a
   fresh answer arrives. Update now downloads the release's file for this system and installs it
   only once its build provenance verifies; the core says how far it has got. */

import { invoke, listen, el, set, size, left, openOutside } from './common.js';

let release = null;
let update = { state: 'idle', why: null };

const underWay = (state) => state === 'downloading' || state === 'verifying' || state === 'installing';

function fraction(done, total) {
  return total === null || total <= 0 ? null : Math.min(1, done / total);
}

/* The bar and the line under it, the way the work board draws a running job. */
function drawProgress(done, total, step) {
  const fill = el('newer-fill');
  const track = el('newer-track');
  const share = fraction(done, total);
  fill.classList.toggle('working', share === null);
  /* No total to divide by: a bar that invents a denominator is a bar that lies. */
  fill.style.width = share === null ? '100%' : `${(100 * share).toFixed(1)}%`;
  if (share === null) track.removeAttribute('aria-valuenow');
  else track.setAttribute('aria-valuenow', String(Math.round(100 * share)));
  set(el('newer-step'), step);
}

function downloaded(progress) {
  const amount = progress.total === null ? size(progress.done) : `${size(progress.done)} of ${size(progress.total)}`;
  const speed = progress.rate === null || progress.rate <= 0 ? '' : `${size(progress.rate)}/s`;
  const time = progress.left === null ? '' : left(progress.left);
  return [amount, speed, time].filter(Boolean).join(' · ');
}

function draw() {
  const notice = el('newer-version');
  notice.hidden = release === null;
  if (release === null) return;
  const name = `SteamGauge ${release.version}`;
  const state = update.state;
  notice.classList.toggle('failed', state === 'failed');

  set(
    el('newer-says'),
    {
      downloading: `Downloading ${release.version}`,
      verifying: `Checking ${release.version}`,
      installing: `Installing ${release.version}`,
      failed: `The update to ${release.version} stopped`,
    }[state] ?? `${name} is out.`,
  );
  set(el('newer-running'), `This computer has ${release.running}.`);

  el('newer-progress').hidden = !underWay(state);
  if (state === 'downloading') drawProgress(update.done, update.total, downloaded(update));
  if (state === 'verifying') drawProgress(0, null, "Proving SteamGauge's release workflow built it");
  if (state === 'installing') drawProgress(0, null, `SteamGauge closes and opens again as ${release.version}`);

  /* Idle with a reason is a copy the window does not update: a Scoop install, a portable folder. */
  const why =
    state === 'failed'
      ? `Nothing was installed, because ${update.why}.`
      : state === 'idle' && update.why
        ? `${update.why.charAt(0).toUpperCase()}${update.why.slice(1)}.`
        : null;
  el('newer-why').hidden = !why;
  set(el('newer-why'), why ?? '');

  const offered = (state === 'idle' && !update.why) || state === 'failed';
  el('newer-update').hidden = !offered;
  set(el('newer-update'), state === 'failed' ? 'Try again' : 'Update now');
  el('newer-show').hidden = !(state === 'failed' && update.file);
  el('newer-open').hidden = underWay(state);
  set(el('newer-open'), state === 'idle' && update.why ? 'Download it from its release page' : 'Open the release page');
}

/* Asks the core for what it kept, which is also how turning the setting off takes the notice
   away. */
export async function showNewer() {
  try {
    release = (await invoke('newer_version')) ?? null;
  } catch {
    release = null;
  }
  try {
    update = await invoke('update_state');
  } catch {
    update = { state: 'idle', why: null };
  }
  draw();
}

export function setUpNewer() {
  el('newer-open').addEventListener('click', () => {
    if (release) openOutside(release.url);
  });
  el('newer-update').addEventListener('click', () => {
    update = { state: 'downloading', done: 0, total: null, rate: null, left: null };
    draw();
    invoke('update_now').catch(() => showNewer());
  });
  el('newer-show').addEventListener('click', () => invoke('show_update_file').catch(() => {}));
  /* Asked again rather than drawn from the event, in case the setting was turned off while the
     question was out. */
  listen('newer-version', showNewer);
  listen('update', (event) => {
    update = event.payload;
    draw();
  });
  showNewer();
}
