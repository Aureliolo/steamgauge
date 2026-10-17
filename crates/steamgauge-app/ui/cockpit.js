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
} from './common.js';
import { onWork, jobs, jobItem, queue, stop, taskName, active, settle } from './work.js';
import { reason } from './settings.js';
import { drawSinceCard } from './since.js';

let overview = null;
let busy = false;
/* The reader the welcome offers, and the models a job is fetching now, by the names jobs use. */
let readerKey = null;
let fetching = new Set();

/* Waiting jobs named before the rest are counted instead. */
const WAITING_SHOWN = 6;
const FINISHED_SHOWN = 2;

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
  fresh: ['M10 3v10', 'M5.5 8.5 10 13l4.5-4.5', 'M4 16.5h12'],
  older: ['M4 10a6 6 0 1 0 2-4.5', 'M4 3.5V6h2.5', 'M10 7v3l2 1.5'],
  unread: ['M4 5h12M4 10h8M4 15h10'],
};

export function setUpCockpit({ openGame, openSubject }) {
  page('cockpit', el('cockpit'), load);

  /* Everything running, the next few waiting, and the last few finished: a library-wide update
     queues a job for every game, and listing seventy of them would bury the one running. */
  onWork((all) => {
    /* Fetching a model is part of the welcome, not a sign the library is under way. */
    busy = all.some((job) => job.task.kind !== 'fetch_model');
    fetching = new Set(all.filter((job) => job.task.kind === 'fetch_model' && active(job)).map((job) => job.task.model));
    /* Drawn again when the cockpit is shown, so a hidden one is not laid out at every tick. */
    if (showing() === 'cockpit') drawWork(all);
  });
  el('ready-fetch').addEventListener('click', () => {
    if (readerKey) queue({ kind: 'fetch_model', model: readerKey });
  });
  el('fetch-every-model').addEventListener('click', () => {
    const missing = (overview?.machine.models ?? []).filter((model) => !model.here && model.bytes_left > 0 && !fetching.has(model.key));
    if (missing.length > 0) queue(...missing.map((model) => ({ kind: 'fetch_model', model: model.key })));
  });
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
    drawWork(jobs());
    try {
      overview = await invoke('overview');
    } catch (failure) {
      set(el('cockpit-sub'), String(failure));
      el('cockpit-sub').hidden = false;
      return;
    }
    draw(overview, { openGame, openSubject });
    /* Only a cockpit somebody can see has been looked at: a window behind others redraws too. */
    if (overview.games > 0 && document.visibilityState === 'visible') {
      invoke('looked', { appId: null }).catch(() => {});
    }
  }
}

/* Everything running and the last few finished as cards, and what waits as one line of names: a
   library-wide update queues a job for every game, and drawing seventy of them would bury the
   one running and push everything else down the page. */
function drawWork(all) {
  const running = all.filter((job) => job.state === 'running');
  const waiting = all.filter((job) => job.state === 'queued');
  const finished = all.filter((job) => !active(job)).reverse().slice(0, FINISHED_SHOWN);
  settle(el('cockpit-work'), [...running, ...finished].map((job) => jobItem(job)));
  const chips = waiting.slice(0, WAITING_SHOWN).map(waitingChip);
  if (waiting.length > WAITING_SHOWN) {
    chips.push(make('li', 'chip more', `and ${whole.format(waiting.length - WAITING_SHOWN)} more`));
  }
  settle(el('cockpit-waiting'), chips);
  set(el('cockpit-waiting-count'), `${whole.format(waiting.length)} waiting`);
  el('cockpit-waiting-row').hidden = waiting.length === 0;
  el('cockpit-running').hidden = all.length === 0;
  el('clear-finished').hidden = finished.length === 0;
  if (overview) {
    welcome(overview);
    drawMachine(overview);
  }
  drawFetch(all.filter((job) => job.task.kind === 'fetch_model' && job.task.model === readerKey).pop());
}

/* A waiting job in a few words, with the way to take it off the list. */
function waitingChip(job) {
  const chip = make('li', 'chip queued', job.name ? make('span', 'chip-name', job.name) : null);
  chip.dataset.job = job.id;
  chip.append(make('span', 'chip-kind', taskName(job.task)));
  const off = button('', 'chip-off', () => stop(job.id));
  off.setAttribute('aria-label', `Take ${job.name || 'this job'} off the list`);
  off.title = 'Take off the list';
  off.append(icon(['M5 5l10 10M15 5 5 15']));
  chip.append(off);
  return chip;
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
  readerKey = chosen?.name ?? null;
  const underway = el('ready-job').childElementCount > 0;
  el('welcome-ready').hidden = !underway && (!chosen || !chosen.published || chosen.bytes_left === 0 || laterChosen());
  if (el('welcome-ready').hidden || underway) return;

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
  /* The band says what the library holds; the line under the title is for when it cannot. */
  el('cockpit-sub').hidden = true;
  welcome(found);
  drawSinceCard(found.since, { openGame, openSubject });
  drawFigures(found);
  drawHealth(found, openGame);
  drawMoves(found, { openGame, openSubject });
  drawMachine(found);
  balance();
}

/* One lane on a window narrower than this, as app.css lays it out. */
const ONE_LANE = window.matchMedia('(max-width: 1080px)');

/* How much nearer level the lanes must come before a panel changes lane, so one that moved
   stays put while what it holds grows and shrinks by a line or two. */
const SETTLED = 96;

/* Puts each panel in whichever lane leaves the two closest to level, in the order a person
   reads them, what is new always first on the left. Placed only when the figures are drawn or
   the window is resized, never as work moves, so no panel shifts under the pointer. */
function balance() {
  const left = el('lane-left');
  const right = el('lane-right');
  const panels = ['since-title', 'health-title', 'moves-title'].map((id) => el(id).closest('.panel'));
  if (ONE_LANE.matches) {
    left.append(...panels);
    return;
  }
  /* A page out of sight has nothing laid out to measure. */
  if (panels[0].offsetParent === null) return;
  const gap = parseFloat(getComputedStyle(left).rowGap);
  const heights = panels.map((panel) => panel.offsetHeight);
  /* Bit i of a placement set is panel i in the right lane. */
  const tallest = (placing) => {
    const lanes = [[], []];
    heights.forEach((height, at) => lanes[(placing >> at) & 1].push(height));
    return Math.max(...lanes.map((lane) => lane.reduce((sum, height) => sum + height, 0) + gap * Math.max(0, lane.length - 1)));
  };
  const now = panels.reduce((placing, panel, at) => (panel.parentElement === right ? placing | (1 << at) : placing), 0);
  /* Even placings from two: what is new stays on the left, and the right lane is never empty. */
  let best = 2;
  for (let placing = 4; placing < 1 << panels.length; placing += 2) {
    if (tallest(placing) < tallest(best)) best = placing;
  }
  if (now !== 0 && tallest(now) - tallest(best) < SETTLED) best = now;
  panels.forEach((panel, at) => ((best >> at) & 1 ? right : left).append(panel));
}

let resizing = null;
window.addEventListener('resize', () => {
  clearTimeout(resizing);
  resizing = setTimeout(() => {
    if (showing() === 'cockpit') balance();
  }, 150);
});

/* A figure is read value first; the term still comes first for a screen reader. */
function figure(value, term) {
  return make('div', null, make('dt', null, term), make('dd', null, value));
}

function drawFigures(found) {
  el('health-stats').replaceChildren(
    figure(
      whole.format(found.games),
      found.games === 0
        ? 'games'
        : found.read === found.games
          ? 'games, all read'
          : `games, ${whole.format(found.read)} read`,
    ),
    figure(whole.format(found.reviews), 'reviews held'),
    figure(whole.format(found.claims), 'points read'),
    figure(size(found.disk_bytes), 'on disk'),
  );
}

function fact(term, value, under) {
  return make('div', null, make('dt', null, term), make('dd', null, value, under ? make('small', null, under) : null));
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

/* The figures, then the months they cover on a line of their own under the row; the panel says
   once what they are set against. */
function figures(shift, from, to) {
  return [
    make(
      'div',
      'move-figures',
      make('span', 'move-from', roundShare.format(shift.before)),
      make('span', 'move-arrow', '→'),
      make('span', 'move-to', roundShare.format(shift.recent)),
    ),
    make('span', 'move-when', `${monthName(from)} to ${monthName(to)}`),
  ];
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
        ...figures(moved.shift, moved.from, moved.to),
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
        ...figures(shifted.shift, shifted.from, shifted.to),
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
  /* A model that is not published yet is nobody's to download, so it is not counted. */
  const published = machine.models.filter((model) => model.here || model.bytes_left > 0);
  const here = published.filter((model) => model.here).length;
  const coming = published.filter((model) => !model.here && fetching.has(model.key)).length;
  el('machine-facts').replaceChildren(
    fact('Graphics card', machine.card ?? (memory ? 'Unnamed' : 'None reported'), memory),
    fact('Reader', machine.reader === 'standard' ? 'Standard' : 'Small'),
    fact('Card time', machine.on_processor ? 'Processor only' : roundShare.format(machine.gpu_share)),
    fact(
      'Models',
      here === published.length
        ? `all ${whole.format(here)} on this computer`
        : `${whole.format(here)} of ${whole.format(published.length)} on this computer`,
      coming > 0 ? `${whole.format(coming)} downloading` : null,
    ),
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
  /* A model not here yet comes the first time it is needed, or now, so the work that needs it
     never waits on the download and the computer can go offline holding every model. */
  const missing = machine.models.filter((model) => !model.here && model.bytes_left > 0);
  /* A model here and current needs nothing, so only one still to come or with a newer release
     takes room in the cockpit; Storage lists every one. */
  const due = machine.models.filter((model) => (!model.here && model.bytes_left > 0) || model.newer);
  el('models-due').hidden = due.length === 0;
  for (const model of due) {
    const where = model.here
      ? make('span', 'pill good', 'On this computer')
      : fetching.has(model.key)
        ? make('span', 'quiet small', `Downloading, ${size(model.bytes_left)}.`)
        : make(
            'span',
            'model-fetch',
            make('span', 'quiet small', `A ${size(model.bytes_left)} download, the first time it is needed.`),
            button('Download now', 'quiet-button small', () => queue({ kind: 'fetch_model', model: model.key })),
          );
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
  const waiting = missing.filter((model) => !fetching.has(model.key));
  const every = el('fetch-every-model');
  every.hidden = waiting.length < 2;
  set(every, `Download every model now, ${size(waiting.reduce((total, model) => total + model.bytes_left, 0))}`);
  set(
    el('models-checked'),
    machine.releases_checked > 0
      ? `Asked for newer models on ${day.format(new Date(machine.releases_checked * 1000))}.`
      : '',
  );
}
