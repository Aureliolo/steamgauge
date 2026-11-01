/* What players say across one group of games: each subject's share of reviews and its praise and
   complaint over the group's read games, counted with each game weighing the same or with every
   review weighing the same, and each game's own figures a click away. */

import {
  invoke,
  el,
  set,
  make,
  button,
  roundShare,
  whole,
  language,
  page,
  go,
  ENOUGH_TO_LEAN,
  thin,
} from './common.js';

let name = null;
let found = null;
let weight = 'each_game';
const open = new Set();

const SAYS = {
  each_game:
    'Each game counts once, however many reviews it has.',
  every_review:
    'Every review weighs the same, so the games with the most reviews decide most of each figure.',
};

export function setUpGroup({ openSubject }) {
  page(
    'group',
    el('group'),
    (chosen) => {
      if (chosen && chosen !== name) {
        name = chosen;
        open.clear();
      }
      el('group-thin').checked = thin.shown;
      load();
    },
    'library',
  );

  el('group-thin').addEventListener('change', () => {
    thin.shown = el('group-thin').checked;
    draw();
  });

  el('group-weight').addEventListener('change', (event) => {
    weight = event.target.value;
    draw();
  });
  el('group-compare').addEventListener('click', () => go('compare', found.games.map((game) => game.app_id)));

  async function load() {
    set(el('group-title'), name ?? 'Group');
    try {
      found = await invoke('group_reading', { name });
    } catch (failure) {
      found = null;
      set(el('group-sub'), String(failure));
      for (const id of ['group-table', 'group-legend', 'group-unread', 'group-empty']) el(id).hidden = true;
      return;
    }
    draw();
  }

  function draw() {
    if (!found) return;
    const games = found.games;
    const reviews = games.reduce((sum, game) => sum + game.reviews, 0);
    const languages = new Set(games.map((game) => game.language));
    const tongue = languages.size === 1 && games[0].language ? ` ${language(games[0].language)}` : '';
    set(
      el('group-sub'),
      games.length === 0
        ? 'None of its games is read yet.'
        : `${whole.format(games.length)} read ${games.length === 1 ? 'game' : 'games'}, ` +
            `${whole.format(reviews)}${tongue} reviews` +
            (languages.size > 1 ? ' in several languages' : '') +
            '.',
    );
    set(el('group-weight-says'), SAYS[weight]);
    el('group-compare').disabled = games.length < 2;

    const unread = el('group-unread');
    unread.hidden = found.unread.length === 0;
    const left = found.unread.map((game) => game.name);
    set(
      unread,
      `${left.length > 1 ? `${left.slice(0, -1).join(', ')} and ${left[left.length - 1]} are` : `${left[0] ?? ''} is`} ` +
        'not read yet and left out.',
    );

    const names = new Map(games.map((game) => [game.app_id, game.name]));
    const ranked = found.subjects
      .filter((subject) => subject[weight].rate > 0)
      .sort((a, b) => b[weight].rate - a[weight].rate);
    const widest = Math.max(0.0001, ...ranked.map((subject) => subject[weight].high));
    el('group-rows').replaceChildren(...ranked.flatMap((subject) => rows(subject, names, widest, games.length)));

    el('group-table').hidden = ranked.length === 0;
    el('group-legend').hidden = ranked.length === 0;
    el('group-empty').hidden = games.length > 0;
  }

  function rows(subject, names, widest, count) {
    const figures = subject[weight];
    const shown = open.has(subject.id);
    const detail = `group-games-${subject.id}`;
    const toggle = button(subject.label, 'disclosure', () => {
      if (open.has(subject.id)) open.delete(subject.id);
      else open.add(subject.id);
      draw();
    });
    toggle.setAttribute('aria-expanded', String(shown));
    toggle.setAttribute('aria-controls', detail);

    const most = subject.most === null ? null : names.get(subject.most);
    const split = leaning(subject);
    const row = make(
      'tr',
      null,
      make('th', 'subject', toggle),
      make(
        'td',
        'cell',
        make('span', 'rate', roundShare.format(figures.rate)),
        bar(figures, split, widest),
        make('small', null, `${roundShare.format(figures.low)} to ${roundShare.format(figures.high)}`),
      ),
      make('td', 'cell', lean(split)),
      make(
        'td',
        'cell',
        make('span', 'rate', `${whole.format(subject.raised_in)} of ${whole.format(count)}`),
        most ? make('small', null, `most in ${most}`) : null,
      ),
    );
    if (!shown) return [row];

    const perGame = make(
      'ul',
      'per-game',
      ...subject.games.map((game) =>
        make(
          'li',
          null,
          button(names.get(game.app_id), 'link', () => openSubject(game.app_id, subject.id)),
          make('span', 'rate', roundShare.format(game.rate)),
          make(
            'small',
            null,
            `${whole.format(game.raised)} of ${whole.format(game.reviews)} reviews` +
              (game.raised === 0
                ? ''
                : game.raised >= ENOUGH_TO_LEAN || thin.shown
                  ? `, ${roundShare.format(game.praising)} praise, ${roundShare.format(game.complaining)} complain`
                  : ', too few to split'),
          ),
        ),
      ),
    );
    const cell = make('td', null, perGame);
    cell.colSpan = 4;
    const games = make('tr', 'games-row', cell);
    games.id = detail;
    return [row, games];
  }
}

const mean = (values) => values.reduce((sum, value) => sum + value, 0) / values.length;

/* How the reviews raising a subject split, by the weighting shown, or null below the floor. Asked
   to show thin splits, each game's mean takes every game raising the subject at all, where the
   core's takes only those with enough. */
function leaning(subject) {
  const figures = subject[weight];
  const raised = subject.games.reduce((sum, game) => sum + game.raised, 0);
  if (weight === 'every_review') {
    if (raised === 0 || (raised < ENOUGH_TO_LEAN && !thin.shown)) return { withheld: raised };
    return { ...figures, note: raised < ENOUGH_TO_LEAN ? `from ${whole.format(raised)} reviews` : null };
  }
  if (!thin.shown) return figures.praising === null ? { withheld: raised } : figures;
  const some = subject.games.filter((game) => game.raised > 0);
  if (some.length === 0) return { withheld: 0 };
  return {
    praising: mean(some.map((game) => game.praising)),
    complaining: mean(some.map((game) => game.complaining)),
    mixed: mean(some.map((game) => game.mixed)),
    note: some.some((game) => game.raised < ENOUGH_TO_LEAN)
      ? `counting games with fewer than ${ENOUGH_TO_LEAN} reviews raising it`
      : null,
  };
}

/* The share as a bar against the widest interval on the page, coloured by how the reviews raising
   it lean, with the interval drawn across it. */
function bar(figures, split, widest) {
  const track = make('span', 'interval-track');
  const share = make('span', split.withheld === undefined ? 'split' : 'split plain');
  share.style.width = `${(100 * figures.rate) / widest}%`;
  if (split.withheld === undefined) {
    const mixed = split.mixed ?? 0;
    for (const [side, part] of [
      ['praise', split.praising - mixed],
      ['mixed', mixed],
      ['complaint', split.complaining - mixed],
    ]) {
      const piece = make('i', side);
      piece.style.width = `${100 * Math.max(part, 0)}%`;
      share.append(piece);
    }
  }
  const interval = make('span', 'interval');
  interval.style.left = `${(100 * figures.low) / widest}%`;
  interval.style.width = `${(100 * (figures.high - figures.low)) / widest}%`;
  track.append(share, interval);
  return track;
}

function lean(split) {
  if (split.withheld !== undefined) return make('small', null, 'Too few reviews to split');
  return make(
    'span',
    null,
    make('span', 'praise-text', `${roundShare.format(split.praising)} praise`),
    ', ',
    make('span', 'complaint-text', `${roundShare.format(split.complaining)} complain`),
    split.note ? make('small', null, split.note) : null,
  );
}
