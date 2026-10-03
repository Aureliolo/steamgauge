/* The notice that a newer SteamGauge is out. The core asks GitHub at most once a day and keeps
   the answer; the window shows what it kept, and looks again when a fresh answer arrives. */

import { invoke, listen, el, set, openOutside } from './common.js';

let release = null;

function draw(found) {
  release = found ?? null;
  el('newer-version').hidden = release === null;
  if (release === null) return;
  set(el('newer-name'), `SteamGauge ${release.version}`);
  set(el('newer-running'), `This computer has ${release.running}.`);
}

/* Asks the core for what it kept, which is also how turning the setting off takes the notice
   away. */
export async function showNewer() {
  try {
    draw(await invoke('newer_version'));
  } catch {
    draw(null);
  }
}

export function setUpNewer() {
  el('newer-open').addEventListener('click', () => {
    if (release) openOutside(release.url);
  });
  /* Asked again rather than drawn from the event, in case the setting was turned off while the
     question was out. */
  listen('newer-version', showNewer);
  showNewer();
}
