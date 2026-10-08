/* What moved since the person last looked: the cockpit's first card, and a line on each game's
   page. Every share is of the reviews read since the look, set against the twelve months before
   it; which changes are wider than chance is decided by the core, never here. */

import { invoke, el, set, make, button, whole, roundShare, day, when, monthName } from './common.js';

/* Games listed before the rest are counted instead. */
const GAMES_SHOWN = 6;

/* What a game's new reviews came to, in a few words, where nothing in them is listed as moving. */
function standing(since) {
  switch (since.standing) {
    case 'unread':
      return 'not read yet';
    case 'first_read':
      return 'read for the first time since';
    case 'reread':
      return 'read again by another reader or in another language, so not compared';
    case 'few':
      return `${whole.format(since.read_new)} read, too few to compare`;
    case 'unanchored':
      return 'too few reviews in the year before to compare with';
    case 'compared':
      /* Where something moved, the line under it says how many were read. */
      return moved(since) ? null : `${whole.format(since.read_new)} read, and nothing in them moved beyond chance`;
    default:
      return 'nothing new';
  }
}

/* A subject's name in the middle of a sentence: lower case, except a word that is an acronym. */
function subjectName(label) {
  if (label === 'Overall verdict') return 'the game as a whole';
  return label
    .split(' ')
    .map((word) => (word.length > 1 && word === word.toUpperCase() ? word : word.toLowerCase()))
    .join(' ');
}

const moved = (since) => since.recommended !== null || since.moves.length > 0;

const newReviews = (since) =>
  since.new === 1 ? '1 new review' : `${whole.format(since.new)} new reviews`;

function figures(shift) {
  return make(
    'span',
    'since-figures',
    make('span', 'move-from', roundShare.format(shift.before)),
    make('span', 'move-arrow', '→'),
    make('span', 'move-to', roundShare.format(shift.recent)),
  );
}

/* One subject, or the share recommending, that moved in a game's new reviews. */
function moveItems(since, appId, openSubject) {
  const items = since.moves.map((one) => {
    const rising = one.shift.recent > one.shift.before;
    const label = subjectName(one.label);
    return make(
      'li',
      `since-move ${one.side} ${rising ? 'up' : 'down'}`,
      make('span', 'side', one.side === 'praise' ? 'Praise for' : 'Complaints about'),
      openSubject ? button(label, 'link', () => openSubject(appId, one.subject, one.side)) : make('span', null, label),
      figures(one.shift),
    );
  });
  if (since.recommended !== null) {
    const rising = since.recommended.recent > since.recommended.before;
    items.push(
      make(
        'li',
        `since-move recommended ${rising ? 'up' : 'down'}`,
        make('span', 'side', 'Recommending the game'),
        figures(since.recommended),
      ),
    );
  }
  return items;
}

function against(since) {
  return make(
    'p',
    'since-against',
    `Of the ${whole.format(since.read_new)} new reviews read, against ${monthName(since.from)} to ${monthName(since.to)}.`,
  );
}

/* The cockpit's first card: when the person last looked, and every game with something to say
   since, the ones that moved first. */
export function drawSinceCard(lately, { openGame, openSubject }) {
  const list = el('since-games');
  const shown = lately.games.slice(0, GAMES_SHOWN);
  list.replaceChildren(
    ...shown.map((game) => {
      const item = make(
        'li',
        `since-game${moved(game) ? ' moved' : ''}`,
        make(
          'div',
          'since-head',
          button(game.name, 'link', () => openGame(game.app_id)),
          game.new > 0 ? make('span', 'since-new', newReviews(game)) : null,
          standing(game) ? make('span', 'since-standing', standing(game)) : null,
        ),
      );
      if (moved(game)) item.append(make('ul', 'since-moves', ...moveItems(game, game.app_id, openSubject)), against(game));
      return item;
    }),
  );
  if (lately.games.length > shown.length) {
    list.append(make('li', 'since-game more', `and ${whole.format(lately.games.length - shown.length)} more games`));
  }
  set(el('since-when'), lately.looked === null ? '' : `You last looked on ${when.format(new Date(lately.looked * 1000))}`);
  el('since-calm').hidden = lately.games.length > 0;
  set(
    el('since-calm-text'),
    lately.looked === null
      ? 'Nothing to compare with yet. From your next visit, this says what changed in between.'
      : 'Nothing has changed since you last looked.',
  );
  el('since-how').hidden = !lately.games.some(moved);
}

/* A game's own line: what changed since its page was last seen. Nothing is shown before a first
   look, when there is nothing to have changed since. */
export async function drawSinceLine(appId, openSubject, still) {
  const line = el('game-since');
  let since = null;
  try {
    since = await invoke('since_last_look', { appId });
  } catch {
    since = null;
  }
  if (!still()) return;
  line.hidden = since === null;
  if (since === null) return;
  const looked = day.format(new Date(since.looked * 1000));
  line.classList.toggle('moved', moved(since));
  if (since.standing === 'nothing') {
    line.replaceChildren(make('p', null, `Nothing new since you last looked, on ${looked}.`));
    return;
  }
  const said = [since.new > 0 ? newReviews(since) : null, standing(since)].filter(Boolean).join(', ');
  line.replaceChildren(
    make('p', null, make('strong', null, `Since you last looked, on ${looked}: `), said ? `${said}.` : 'this moved.'),
  );
  if (moved(since)) line.append(make('ul', 'since-moves', ...moveItems(since, appId, openSubject)), against(since));
}
