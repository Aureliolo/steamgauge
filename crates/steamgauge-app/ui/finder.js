/* Finding a game to add: by its name, its app ID or its store link, from the welcome's box or the
   page for adding one. The store's own search answers names; an ID or a link goes straight to the
   game. */

import { invoke, el, set, make, whole, roundShare, facts, verdictPill, art, page, go } from './common.js';
import { queue } from './work.js';

/* The app ID in a store link or a bare number, or null for words to search. */
export function appIdIn(typed) {
  const text = typed.trim();
  if (/^\d+$/.test(text)) return Number(text);
  const linked = text.match(/store\.steampowered\.com\/app\/(\d+)/i) ?? text.match(/steamcommunity\.com\/app\/(\d+)/i);
  return linked ? Number(linked[1]) : null;
}

/* Waits this long after the last key before asking the store, so a name typed quickly is one
   question rather than one per letter. */
const PAUSE = 300;

/* A box that lists what the store finds as somebody types, and hands the chosen game on. */
function searchAs(input, list, pick) {
  let timer = null;
  let asked = '';

  const show = (listings) => {
    list.replaceChildren(
      ...listings.slice(0, 8).map((listing) => {
        const choice = make(
          'button',
          'result',
          art(listing.app_id),
          make('span', 'result-name', listing.name),
          make('span', 'result-id', `App ${listing.app_id}`),
        );
        choice.type = 'button';
        choice.addEventListener('click', () => {
          list.replaceChildren();
          pick(listing.app_id);
        });
        return make('li', null, choice);
      }),
    );
  };

  const ask = async (words) => {
    asked = words;
    let listings;
    try {
      listings = await invoke('find_games', { words });
    } catch (failure) {
      if (asked === words) list.replaceChildren(make('li', 'quiet', String(failure)));
      return;
    }
    if (asked !== words) return;
    if (listings.length === 0) list.replaceChildren(make('li', 'quiet', `Steam lists no game called “${words}”.`));
    else show(listings);
  };

  input.addEventListener('input', () => {
    clearTimeout(timer);
    const typed = input.value.trim();
    if (typed.length < 2 || appIdIn(typed) !== null) {
      asked = '';
      list.replaceChildren();
      return;
    }
    timer = setTimeout(() => ask(typed), PAUSE);
  });

  /* Enter takes an ID or a link straight to the game, and words to the first game listed. */
  return async () => {
    clearTimeout(timer);
    const typed = input.value.trim();
    const appId = appIdIn(typed);
    if (appId !== null) {
      list.replaceChildren();
      pick(appId);
      return;
    }
    if (typed.length < 2) return;
    const first = list.querySelector('.result');
    if (first && asked === typed) {
      first.click();
      return;
    }
    await ask(typed);
  };
}

let found = null;

export function setUpFinder({ openGame }) {
  page('finder', el('finder'));

  const submitFinder = searchAs(el('appid'), el('finder-results'), lookUp);
  el('lookup-form').addEventListener('submit', (event) => {
    event.preventDefault();
    submitFinder();
  });

  const submitWelcome = searchAs(el('welcome-query'), el('welcome-results'), (appId) => {
    openFinder();
    lookUp(appId);
  });
  el('welcome-form').addEventListener('submit', (event) => {
    event.preventDefault();
    submitWelcome();
  });
  for (const example of el('welcome-examples').querySelectorAll('[data-example]')) {
    example.addEventListener('click', () => {
      el('welcome-query').value = example.dataset.example;
      el('welcome-query').dispatchEvent(new Event('input'));
      el('welcome-query').focus();
    });
  }

  el('start').addEventListener('click', () => {
    if (!found) return;
    const appId = found.app_id;
    queue({ kind: 'download', app_id: appId });
    openGame(appId, found.name || null);
  });
  el('cancel').addEventListener('click', () => {
    el('found').hidden = true;
    el('appid').focus();
  });
}

export function openFinder() {
  go('finder');
  el('found').hidden = true;
  el('finder-results').replaceChildren();
  set(el('lookup-note'), '');
  el('lookup-note').classList.remove('bad');
  el('appid').value = '';
  el('appid').focus();
}

async function lookUp(appId) {
  const note = el('lookup-note');
  note.classList.remove('bad');
  el('finder-results').replaceChildren();
  el('found').hidden = true;
  set(note, 'Asking Steam...');
  try {
    found = await invoke('look_up', { appId });
  } catch (failure) {
    note.classList.add('bad');
    set(note, String(failure));
    return;
  }
  set(note, '');
  set(el('found-name'), found.name || `App ${found.app_id}`);
  art(found.app_id, el('found-art'));
  el('found-verdict').replaceChildren(
    ...[verdictPill(found.verdict), make('span', 'quiet small', `App ${found.app_id}`)].filter(Boolean),
  );
  const total = found.positive + found.negative;
  facts(el('found-facts'), [
    ['Reviews', whole.format(found.reviews), 'Steam will serve'],
    ['Recommending', total > 0 ? roundShare.format(found.positive / total) : '–', `${whole.format(found.positive)} of ${whole.format(total)}`],
  ]);
  set(
    el('found-note'),
    found.held
      ? 'Already in your library. Downloading again picks up where the last download stopped.'
      : `About ${whole.format(Math.ceil(found.reviews / 100))} requests, paced so Steam is not leaned on.`,
  );
  let settings = null;
  try {
    settings = await invoke('settings');
  } catch {
    /* What happens after the download is a courtesy line; the download does not wait on it. */
  }
  set(
    el('found-then'),
    settings?.read_after_download
      ? 'Once downloaded, every review is read. That can be changed in Settings.'
      : 'Once downloaded, the game is ready to read from its page.',
  );
  el('found').hidden = false;
}
