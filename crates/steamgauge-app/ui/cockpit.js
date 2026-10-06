/* The cockpit: what is running, what state the library is in, what moved lately across every
   game, and what this machine reads with. The first page the app opens on, and for an empty
   library a welcome that says what the app does and finds the first game. */

import {
  invoke,
  listen,
  el,
  set,
  make,
  button,
  whole,
  roundShare,
  day,
  size,
  monthName,
  page,
  go,
  showing,
  setPath,
} from './common.js';
import { onWork, jobItem, queue, active } from './work.js';
import { reason } from './settings.js';

let overview = null;
let busy = false;

/* Waiting jobs listed before the rest are counted instead. */
const WAITING_SHOWN = 4;

const SVG = 'http://www.w3.org/2000/svg';

function icon(paths) {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('viewBox', '0 0 20 20');
  svg.setAttribute('aria-hidden', 'true');
  for (const d of paths) {
    const path = document.createElementNS(SVG, 'path');
    path.setAttribute('d', d);
    svg.append(path);
  }
  return svg;
}

const ICONS = {
  games: ['M3 6h14v9H3z', 'M7 10.5h2M8 9.5v2', 'M12.5 10h.01M14 11.5h.01'],
  reviews: ['M4 4h12v9H9l-4 3v-3H4z'],
  points: ['M5 6h10M5 10h7M5 14h9'],
  disk: ['M3 12h14l-2-7H5z', 'M3 12v3h14v-3', 'M13.5 13.5h.01'],
  fresh: ['M10 3v10', 'M5.5 8.5 10 13l4.5-4.5', 'M4 16.5h12'],
  older: ['M4 10a6 6 0 1 0 2-4.5', 'M4 3.5V6h2.5', 'M10 7v3l2 1.5'],
  unread: ['M4 5h12M4 10h8M4 15h10'],
};

export function setUpCockpit({ openGame, openSubject }) {
  page('cockpit', el('cockpit'), load);

  /* Everything running, the next few waiting, and the last few finished: a library-wide update
     queues a job for every game, and listing seventy of them would bury the one running. */
  onWork((all) => {
    const running = all.filter((job) => job.state === 'running');
    const waiting = all.filter((job) => job.state === 'queued');
    const finished = all.filter((job) => !active(job)).reverse().slice(0, 4);
    const items = [...running, ...waiting.slice(0, WAITING_SHOWN)].map((job) => jobItem(job));
    if (waiting.length > WAITING_SHOWN) {
      items.push(make('li', 'job more', `and ${whole.format(waiting.length - WAITING_SHOWN)} more waiting`));
    }
    items.push(...finished.map((job) => jobItem(job)));
    el('cockpit-work').replaceChildren(...items);
    el('cockpit-idle').hidden = items.length > 0;
    el('clear-finished').hidden = finished.length === 0;
    /* Fetching the reader is part of the welcome, not a sign the library is under way. */
    busy = all.some((job) => job.task.kind !== 'fetch_reader');
    if (overview) welcome(overview);
    drawFetch(all.filter((job) => job.task.kind === 'fetch_reader').pop());
  });
  el('ready-fetch').addEventListener('click', () => queue({ kind: 'fetch_reader' }));
  el('ready-later').addEventListener('click', () => {
    try {
      localStorage.setItem('reader-later', '1');
    } catch {
      /* Without storage the card only stays away until the app is opened again. */
    }
    el('welcome-ready').hidden = true;
  });
  el('clear-finished').addEventListener('click', () => invoke('clear_finished'));
  el('check-now').addEventListener('click', () => queue({ kind: 'check' }));
  el('to-settings').addEventListener('click', () => go('settings'));

  /* A finished job changes what the cockpit counts, so it counts again, but only while it is
     what is on screen: the walk over the library is not free. */
  listen('library', () => {
    if (showing() === 'cockpit') load();
  });

  async function load() {
    try {
      overview = await invoke('overview');
    } catch (failure) {
      set(el('cockpit-sub'), String(failure));
      return;
    }
    draw(overview, { openGame, openSubject });
  }
}

/* An empty library with nothing on its way is a first visit: the welcome stands in for the
   cockpit until there is something to steer. */
function welcome(found) {
  const first = found.games === 0 && !busy;
  el('cockpit-empty').hidden = !first;
  el('cockpit-full').hidden = first;
  if (first) drawReady();
}

function laterChosen() {
  try {
    return localStorage.getItem('reader-later') === '1';
  } catch {
    return false;
  }
}

/* The reader before a first game: what this computer has, the reader it reads with and why, and
   the download against the room left, offered now so it runs while a first game is chosen. */
async function drawReady() {
  let found;
  try {
    found = await invoke('reader_options');
  } catch {
    el('welcome-ready').hidden = true;
    return;
  }
  const chosen = found.sizes.find((one) => one.name === found.reads_with);
  const fetching = el('ready-job').childElementCount > 0;
  el('welcome-ready').hidden = !fetching && (!chosen || !chosen.published || chosen.bytes_left === 0 || laterChosen());
  if (el('welcome-ready').hidden || fetching) return;

  set(el('ready-title'), `The ${chosen.name} reader reads every review here`);
  set(el('ready-why'), reason(found));
  const short = found.free_bytes !== null && found.free_bytes < chosen.bytes_left * 1.1;
  const room = el('ready-room');
  room.classList.toggle('bad', short);
  set(
    room,
    short
      ? `It is a ${size(chosen.bytes_left)} download, and this drive has ${size(found.free_bytes)} free: make room first.`
      : `A ${size(chosen.bytes_left)} download, once` +
          (found.free_bytes === null ? '.' : `, with ${size(found.free_bytes)} free on this drive.`) +
          ' It runs while you choose a first game.',
  );
  el('ready-fetch').disabled = short;
  el('ready-actions').hidden = false;
}

/* The reader's download, drawn in the card that offered it. */
function drawFetch(job) {
  const holder = el('ready-job');
  if (!job) {
    holder.replaceChildren();
    return;
  }
  el('welcome-ready').hidden = false;
  el('ready-actions').hidden = true;
  holder.replaceChildren(make('ul', 'jobs', jobItem(job, { named: false })));
}

function draw(found, { openGame, openSubject }) {
  set(
    el('cockpit-sub'),
    found.games === 0
      ? 'Nothing in the library yet.'
      : `${whole.format(found.games)} games, ${whole.format(found.reviews)} reviews held, ` +
          `${whole.format(found.read)} read.`,
  );
  welcome(found);
  drawTiles(found);
  drawHealth(found, openGame);
  drawMoves(found, { openGame, openSubject });
  drawMachine(found);
}

function tile(iconName, term, value, under) {
  return make(
    'div',
    'tile',
    make('dt', null, icon(ICONS[iconName]), term),
    make('dd', 'tile-value', value),
    under ? make('dd', 'tile-under', under) : null,
  );
}

function drawTiles(found) {
  el('health-stats').replaceChildren(
    tile('games', 'Games', whole.format(found.games), `${whole.format(found.read)} read`),
    tile('reviews', 'Reviews held', whole.format(found.reviews), 'downloaded from Steam'),
    tile('points', 'Points read', whole.format(found.claims), 'each filed under a subject'),
    tile('disk', 'On disk', size(found.disk_bytes), 'reviews, readings and search'),
  );
}

function stat(term, value, under) {
  return make('div', 'stat', make('dt', null, term), make('dd', null, value, under ? make('small', null, under) : null));
}

/* One thing the library would be better for, the games it concerns, and the one action that
   deals with all of them. */
function attention(iconName, tone, title, games, describe, action, openGame) {
  if (games.length === 0) return null;
  const shown = games.slice(0, 4);
  const list = make('ul', 'game-list');
  for (const game of shown) {
    list.append(
      make(
        'li',
        null,
        button(game.name, 'link', () => openGame(game.app_id)),
        describe ? make('span', 'quiet', describe(game)) : null,
      ),
    );
  }
  if (games.length > shown.length) {
    list.append(make('li', 'quiet', `and ${whole.format(games.length - shown.length)} more`));
  }
  return make(
    'li',
    'attention-item',
    make('span', `attention-icon ${tone}`, icon(ICONS[iconName])),
    make('div', 'attention-text', make('h4', null, title, ' ', make('span', 'count', whole.format(games.length))), list),
    button(action.label, 'quiet-button small', action.run),
  );
}

function drawHealth(found, openGame) {
  setPath(el('library-at'), found.library);

  const lists = el('health-lists');
  lists.replaceChildren(
    ...[
      attention(
        'fresh',
        '',
        'New reviews on Steam',
        found.new_on_steam,
        (game) => `+${whole.format(game.new)}`,
        {
          label: 'Bring up to date',
          run: () => invoke('queue_updates', { appIds: found.new_on_steam.map((game) => game.app_id) }),
        },
        openGame,
      ),
      attention(
        'older',
        'warn',
        'Read by an older reader',
        found.older_reader,
        null,
        {
          label: 'Read again',
          run: () => invoke('queue_reads', { appIds: found.older_reader.map((game) => game.app_id) }),
        },
        openGame,
      ),
      attention(
        'unread',
        '',
        'Downloaded, not read yet',
        found.not_read,
        null,
        {
          label: 'Read',
          run: () => invoke('queue_reads', { appIds: found.not_read.map((game) => game.app_id) }),
        },
        openGame,
      ),
    ].filter(Boolean),
  );
  el('health-calm').hidden = lists.childElementCount > 0 || found.games === 0;
  set(
    el('checked'),
    found.checked > 0
      ? `Checked against Steam on ${day.format(new Date(found.checked * 1000))}.`
      : 'Not checked against Steam yet.',
  );
}

/* Moves shown of each kind before the rest are counted instead. */
const MOVES_SHOWN = 8;

function more(count, what) {
  return count > 0 ? make('li', 'move more', `and ${whole.format(count)} more ${what}`) : null;
}

function figures(shift, from, to, against) {
  return make(
    'div',
    'move-figures',
    make('span', 'move-from', roundShare.format(shift.before)),
    make('span', 'move-arrow', '→'),
    make('span', 'move-to', roundShare.format(shift.recent)),
    make('span', 'move-when', `of reviews, ${monthName(from)} to ${monthName(to)}${against}`),
  );
}

function drawMoves(found, { openGame, openSubject }) {
  const list = el('moves');
  list.replaceChildren();
  for (const moved of found.moves.slice(0, MOVES_SHOWN)) {
    const rising = moved.shift.recent > moved.shift.before;
    const side = moved.side === 'praise' ? 'Praise' : 'Complaints';
    list.append(
      make(
        'li',
        `move ${moved.side} ${rising ? 'up' : 'down'}`,
        make(
          'div',
          'move-what',
          button(moved.name, 'link', () => openGame(moved.app_id)),
          make('span', 'side', `${side} about`),
          button(moved.label.toLowerCase(), 'link', () => openSubject(moved.app_id, moved.subject, moved.side)),
        ),
        figures(moved.shift, moved.from, moved.to, ' against the 12 months before'),
      ),
    );
  }
  const subjectsLeft = more(found.moves.length - MOVES_SHOWN, 'subjects that moved');
  if (subjectsLeft) list.append(subjectsLeft);
  for (const shifted of found.recommended.slice(0, MOVES_SHOWN)) {
    const rising = shifted.shift.recent > shifted.shift.before;
    list.append(
      make(
        'li',
        `move recommended ${rising ? 'up' : 'down'}`,
        make(
          'div',
          'move-what',
          button(shifted.name, 'link', () => openGame(shifted.app_id)),
          make('span', 'side', 'Recommending the game'),
        ),
        figures(shifted.shift, shifted.from, shifted.to, ''),
      ),
    );
  }
  const gamesLeft = more(found.recommended.length - MOVES_SHOWN, 'games whose share recommending moved');
  if (gamesLeft) list.append(gamesLeft);
  el('moves-calm').hidden = list.childElementCount > 0;
}

function drawMachine(found) {
  const machine = found.machine;
  /* A card's memory is sold in binary gigabytes: a 24 GB card reports 25.8 decimal ones. */
  const memory = machine.card_bytes === null ? null : `${Math.round(machine.card_bytes / 2 ** 30)} GB`;
  el('machine-facts').replaceChildren(
    stat('Graphics card', machine.card ?? (memory ? 'Unnamed' : 'None reported'), memory),
    stat('Reader', machine.reader === 'standard' ? 'Standard' : 'Small'),
    stat('Card time', machine.on_processor ? 'Processor only' : roundShare.format(machine.gpu_share)),
  );
  /* On the card the figures above say it all; on the processor a person wants to know why. */
  const reads = el('machine-reads');
  reads.hidden = !machine.on_processor;
  set(
    reads,
    machine.reaches_card
      ? 'Reads on the processor: no graphics card was found.'
      : 'Reads on the processor: this build of SteamGauge does not use a graphics card.',
  );

  const list = el('models');
  list.replaceChildren();
  for (const model of machine.models) {
    const where = model.here
      ? make('span', 'pill good', 'On this computer')
      : model.bytes_left > 0
        ? make('span', 'quiet small', `A ${size(model.bytes_left)} download, the first time it is needed.`)
        : make('span', 'quiet small', 'Not published yet.');
    list.append(
      make(
        'li',
        model.used ? 'model' : 'model unused',
        make('span', 'model-name', model.name, model.release ? make('span', 'tag', model.release) : null),
        make('span', 'quiet', `${model.role.charAt(0).toUpperCase()}${model.role.slice(1)}.`),
        where,
        model.newer
          ? make('span', 'newer', `${model.newer} is published; it comes with the next SteamGauge update.`)
          : null,
      ),
    );
  }
  set(
    el('models-checked'),
    machine.releases_checked > 0
      ? `Asked for newer models on ${day.format(new Date(machine.releases_checked * 1000))}.`
      : '',
  );
}
