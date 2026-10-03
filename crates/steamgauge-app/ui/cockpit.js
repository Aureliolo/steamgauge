/* The cockpit: what is running, what state the library is in, what moved lately across every
   game, and what this machine reads with. The first page the app opens on. */

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
import { onWork, jobItem, queue, active } from './work.js';

let overview = null;

/* Waiting jobs listed before the rest are counted instead. */
const WAITING_SHOWN = 4;

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
  });
  el('clear-finished').addEventListener('click', () => invoke('clear_finished'));
  el('check-now').addEventListener('click', () => queue({ kind: 'check' }));

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

function draw(found, { openGame, openSubject }) {
  set(
    el('cockpit-sub'),
    found.games === 0
      ? 'Nothing in the library yet.'
      : `${whole.format(found.games)} games, ${whole.format(found.reviews)} reviews held, ` +
          `${whole.format(found.read)} read.`,
  );
  el('cockpit-empty').hidden = found.games > 0;

  drawHealth(found, openGame);
  drawMoves(found, { openGame, openSubject });
  drawMachine(found.machine);
}

function stat(term, value, under) {
  return make('div', 'stat', make('dt', null, term), make('dd', null, value, under ? make('small', null, under) : null));
}

/* A short list of games with one action over all of them. */
function gameList(title, games, describe, action, openGame) {
  if (games.length === 0) return null;
  const shown = games.slice(0, 5);
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
  const block = make('div', 'health-list', make('h4', null, title), list);
  if (action) block.append(button(action.label, 'quiet-button small', action.run));
  return block;
}

function drawHealth(found, openGame) {
  const stats = el('health-stats');
  stats.replaceChildren(
    stat('Games', whole.format(found.games), `${whole.format(found.read)} read`),
    stat('Reviews held', whole.format(found.reviews)),
    stat('Points read', whole.format(found.claims)),
    stat('On disk', size(found.disk_bytes)),
  );
  set(el('library-at'), found.library);

  const lists = el('health-lists');
  lists.replaceChildren(
    ...[
      gameList(
        'New on Steam since your last download',
        found.new_on_steam,
        (game) => ` +${whole.format(game.new)}`,
        {
          label: 'Bring these up to date',
          run: () => invoke('queue_updates', { appIds: found.new_on_steam.map((game) => game.app_id) }),
        },
        openGame,
      ),
      gameList(
        'Read by an older reader',
        found.older_reader,
        null,
        {
          label: 'Read them again',
          run: () => invoke('queue_reads', { appIds: found.older_reader.map((game) => game.app_id) }),
        },
        openGame,
      ),
      gameList(
        'Downloaded, not read yet',
        found.not_read,
        null,
        {
          label: 'Read them',
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
  return count > 0 ? make('li', 'move more quiet', `and ${whole.format(count)} more ${what}`) : null;
}

function drawMoves(found, { openGame, openSubject }) {
  const list = el('moves');
  list.replaceChildren();
  for (const moved of found.moves.slice(0, MOVES_SHOWN)) {
    const rising = moved.shift.recent > moved.shift.before;
    const side = moved.side === 'praise' ? 'Praise' : 'Complaints';
    const item = make(
      'li',
      `move ${moved.side} ${rising ? 'up' : 'down'}`,
      make(
        'div',
        'move-what',
        button(moved.name, 'link', () => openGame(moved.app_id)),
        make('span', null, ` · ${side} about `),
        button(moved.label.toLowerCase(), 'link', () => openSubject(moved.app_id, moved.subject, moved.side)),
      ),
      make(
        'div',
        'move-figures',
        make('span', 'move-from', roundShare.format(moved.shift.before)),
        make('span', 'move-arrow', '→'),
        make('span', 'move-to', roundShare.format(moved.shift.recent)),
        make(
          'span',
          'quiet',
          ` of reviews, ${monthName(moved.from)} to ${monthName(moved.to)} against the 12 months before`,
        ),
      ),
    );
    list.append(item);
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
          make('span', null, ' · Recommending the game'),
        ),
        make(
          'div',
          'move-figures',
          make('span', 'move-from', roundShare.format(shifted.shift.before)),
          make('span', 'move-arrow', '→'),
          make('span', 'move-to', roundShare.format(shifted.shift.recent)),
          make('span', 'quiet', ` of reviews, ${monthName(shifted.from)} to ${monthName(shifted.to)}`),
        ),
      ),
    );
  }
  const gamesLeft = more(found.recommended.length - MOVES_SHOWN, 'games whose share recommending moved');
  if (gamesLeft) list.append(gamesLeft);
  el('moves-calm').hidden = list.childElementCount > 0;
}

function drawMachine(machine) {
  /* A card's memory is sold in binary gigabytes: a 24 GB card reports 25.8 decimal ones. */
  const memory = machine.card_bytes === null ? null : `${Math.round(machine.card_bytes / 2 ** 30)} GB`;
  el('machine-facts').replaceChildren(
    stat('Graphics card', machine.card ?? (memory ? 'Unnamed' : 'None reported'), memory),
    stat('Reader', machine.reader === 'standard' ? 'Standard' : 'Small'),
    stat('Card time', machine.on_processor ? 'Processor only' : roundShare.format(machine.gpu_share)),
  );
  set(
    el('machine-reads'),
    machine.on_processor
      ? machine.reaches_card
        ? `Reads on the processor with the ${machine.reader} reader: the card has too little memory for one.`
        : `Reads on the processor with the ${machine.reader} reader.`
      : `Reads on the graphics card with the ${machine.reader} reader, taking ` +
          `${machine.gpu_share === 1 ? 'all of its time' : `${roundShare.format(machine.gpu_share)} of its time`}.`,
  );

  const list = el('models');
  list.replaceChildren();
  for (const model of machine.models) {
    const where = model.here
      ? 'On this computer'
      : model.bytes_left > 0
        ? `A ${size(model.bytes_left)} download, the first time it is needed`
        : 'Not published yet';
    list.append(
      make(
        'li',
        model.used ? 'model' : 'model unused',
        make('span', 'model-name', model.name, model.release ? make('span', 'tag', model.release) : null),
        make('span', 'quiet', `${model.role}. ${where}.`),
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
  el('to-settings').onclick = () => go('settings');
}
