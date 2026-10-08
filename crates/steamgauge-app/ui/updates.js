/* A game's own updates on its timeline, and each subject in the weeks before one against the
   weeks after it. The core decides what an update is and what changed across it; this draws
   what it is told. Titles come from Steam and are always set as text. */

import { invoke, el, set, make, whole, day, openOutside } from './common.js';

const SVG = 'http://www.w3.org/2000/svg';
const CHART_WIDTH = 1000;
/* Markers closer than this share of the chart's width are one marker: at the narrowest window
   two of them would otherwise overlap. */
const TOO_CLOSE = 2.4;

const tenth = new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 1 });

let game = null;
let listed = { asked: null, updates: [], window_days: 28, enough: 100 };
let chosen = null;

const dated = (unix) => day.format(new Date(unix * 1000));

/* The updates kept for a game, marked on the timeline drawn from `months` columns. */
export async function loadUpdates(appId, months) {
  game = appId;
  chosen = null;
  const section = el('around');
  const strip = el('update-strip');
  section.hidden = true;
  strip.hidden = true;
  try {
    listed = await invoke('game_updates', { appId });
  } catch {
    return;
  }
  if (game !== appId) return;
  section.hidden = false;
  el('around-body').hidden = true;
  set(el('around-state'), '');
  drawLede();
  drawChoices();
  drawMarks(months);
}

function drawLede() {
  const { asked, updates, window_days: days } = listed;
  const choose = el('around-choose');
  choose.hidden = updates.length === 0;
  if (asked === null) {
    set(el('around-lede'), 'Steam has not been asked for this game’s updates yet. Bringing it up to date asks.');
  } else if (updates.length === 0) {
    set(el('around-lede'), `Nothing its developer had posted on Steam by ${dated(asked)} reads as an update.`);
  } else {
    set(
      el('around-lede'),
      `${whole.format(updates.length)} ${updates.length === 1 ? 'update' : 'updates'} its developer posted on ` +
        `Steam, as listed on ${dated(asked)}. Choose one to see each subject in the ${days} days ` +
        `before it against the ${days} days after.`,
    );
  }
}

function drawChoices() {
  const select = el('update-choice');
  select.replaceChildren(make('option', null, 'Choose an update'));
  select.firstChild.value = '';
  for (const update of [...listed.updates].reverse()) {
    const option = make('option', null, `${dated(update.posted)}: ${update.title}`);
    option.value = update.gid;
    select.append(option);
  }
  select.value = '';
}

/* Lines on the chart where each update falls, and under it the markers that choose one. */
function drawMarks(months) {
  const svg = el('timeline-svg');
  for (const old of svg.querySelectorAll('.update-line')) old.remove();
  const strip = el('update-strip');
  strip.replaceChildren();
  const placed = listed.updates.filter((update) => update.at !== null);
  if (months < 2 || placed.length === 0) return;

  const step = CHART_WIDTH / months;
  const firstTarget = svg.querySelector('.hit');
  for (const update of placed) {
    const x = (update.at * step).toFixed(2);
    const line = document.createElementNS(SVG, 'line');
    for (const [key, value] of Object.entries({ class: 'update-line', x1: x, x2: x, y1: 0, y2: 160 })) {
      line.setAttribute(key, value);
    }
    line.dataset.gid = update.gid;
    /* Under the months' hit areas, so a month stays readable where an update falls in it. */
    svg.insertBefore(line, firstTarget);
  }

  const groups = [];
  for (const update of placed) {
    const at = (100 * update.at) / months;
    const last = groups[groups.length - 1];
    if (last && at - last.from < TOO_CLOSE) last.updates.push({ ...update, pct: at });
    else groups.push({ from: at, updates: [{ ...update, pct: at }] });
  }
  for (const group of groups) {
    const { updates } = group;
    const middle = updates.reduce((sum, update) => sum + update.pct, 0) / updates.length;
    const several = updates.length > 1;
    const mark = make('button', several ? 'update-mark' : 'update-mark one', several ? String(updates.length) : '');
    mark.type = 'button';
    mark.style.left = `${Math.min(Math.max(middle, 0.6), 99.4)}%`;
    const named = updates.map((update) => `${update.title}, ${dated(update.posted)}`);
    const label = several
      ? `${updates.length} updates, ${dated(updates[0].posted)} to ${dated(updates[updates.length - 1].posted)}`
      : named[0];
    mark.setAttribute('aria-label', label);
    mark.title = named.join('\n');
    mark.dataset.gids = updates.map((update) => update.gid).join(' ');
    /* The newest of a group, since an update is most often followed by its fixes and the
       list beside it offers the rest. */
    mark.addEventListener('click', () => choose(updates[updates.length - 1].gid));
    strip.append(mark);
  }
  strip.hidden = false;
  el('timeline-note').append(' The dashed lines are its updates; a mark under the chart chooses one.');
}

function markChosen() {
  for (const line of el('timeline-svg').querySelectorAll('.update-line')) {
    line.classList.toggle('chosen', line.dataset.gid === chosen);
  }
  for (const mark of el('update-strip').querySelectorAll('.update-mark')) {
    const holds = mark.dataset.gids.split(' ').includes(chosen);
    mark.classList.toggle('chosen', holds);
    mark.setAttribute('aria-pressed', String(holds));
  }
}

async function choose(gid) {
  chosen = gid || null;
  el('update-choice').value = gid ?? '';
  markChosen();
  const body = el('around-body');
  if (chosen === null) {
    body.hidden = true;
    set(el('around-state'), '');
    return;
  }
  const appId = game;
  set(el('around-state'), 'Counting the reviews either side…');
  let found;
  try {
    found = await invoke('before_after', { appId, gid });
  } catch (failure) {
    if (appId === game && chosen === gid) {
      body.hidden = true;
      set(el('around-state'), String(failure));
    }
    return;
  }
  if (appId !== game || chosen !== gid) return;
  set(el('around-state'), '');
  drawAround(found);
  body.hidden = false;
}

/* A share either side, the later one marked where the difference is a change. `worse` says
   which way is bad news for this share. */
function either(compared, worse) {
  const cell = make(
    'span',
    null,
    make('span', 'was', tenth.format(compared.before)),
    make('span', 'arrow', '→'),
    make('span', 'now', tenth.format(compared.after)),
  );
  if (compared.change) {
    cell.className = `changed ${(compared.after > compared.before) === (worse === 'up') ? 'worse' : 'better'}`;
    cell.title = 'A change beyond chance';
  }
  return cell;
}

function sentence(name, compared, worse) {
  const rose = compared.after > compared.before;
  return make(
    'span',
    rose === (worse === 'up') ? 'worse' : 'better',
    `${name} ${rose ? 'rose' : 'fell'} from ${tenth.format(compared.before)} to ${tenth.format(compared.after)}`,
  );
}

function drawAround(found) {
  const { window_days: days, enough } = listed;
  const { update, before, after } = found;
  set(el('around-name'), update.title);
  set(el('around-when'), dated(update.posted));
  const steam = el('around-steam');
  steam.onclick = () => openOutside(update.link);

  const afterDays = Math.floor((after.to - after.from) / 86_400);
  let counts =
    after.to === after.from
      ? `${whole.format(before.reviews)} reviews in the ${days} days before. It was posted after these reviews were ` +
        `downloaded; bringing the game up to date fetches what followed.`
      : `${whole.format(before.reviews)} reviews in the ${days} days before, ${whole.format(after.reviews)} in ` +
        (found.after_whole ? `the ${days} days after.` : `the ${afterDays} days since, all this download holds.`);
  if (found.nearby === 1) counts += ' One other update was posted within these weeks, and its effect is in them too.';
  if (found.nearby > 1) {
    counts += ` ${whole.format(found.nearby)} other updates were posted within these weeks, and their effects are in them too.`;
  }
  set(el('around-counts'), counts);

  const thin = el('around-thin');
  thin.hidden = found.enough;
  set(
    thin,
    found.enough
      ? ''
      : `Too few reviews to call anything a change: each side needs ${whole.format(enough)}, so no share is ` +
          `compared here.`,
  );
  el('around-table-wrap').hidden = !found.enough;
  el('around-summary').hidden = !found.enough;

  const changes = [];
  if (found.recommended?.change) changes.push(sentence('the share recommending the game', found.recommended, 'down'));
  for (const subject of found.subjects) {
    const name = subject.label.toLowerCase();
    if (subject.praise.change) changes.push(sentence(`praise of ${name}`, subject.praise, 'down'));
    if (subject.complaint.change) changes.push(sentence(`complaints about ${name}`, subject.complaint, 'up'));
  }
  const summary = el('around-summary');
  if (changes.length === 0) {
    summary.replaceChildren('Nothing changed beyond chance: not the share recommending the game, and not any subject’s praise or complaints.');
  } else {
    summary.replaceChildren(
      'Changed beyond chance: ',
      ...changes.flatMap((part, index) => (index === 0 ? [part] : ['; ', part])),
      '.',
    );
  }

  const recommended = el('around-recommended');
  recommended.hidden = !found.recommended;
  recommended.replaceChildren(
    ...(found.recommended ? ['Recommending the game: ', either(found.recommended, 'down')] : []),
  );

  const rows = el('around-rows');
  rows.replaceChildren();
  for (const subject of found.subjects) {
    rows.append(
      make(
        'tr',
        null,
        make('td', null, subject.label),
        make('td', 'num', either(subject.praise, 'down')),
        make('td', 'num', either(subject.complaint, 'up')),
      ),
    );
  }

  set(
    el('around-footnote'),
    `Each share is of the reviews written in its window. A change is a share that moved further than ` +
      `chance would move it, three standard errors and at least two points, and a share is compared only ` +
      `where each side holds at least ${whole.format(enough)} reviews, as on the cockpit’s What moved lately. ` +
      `A change happened across the update; that alone does not make the update the reason for it.`,
  );
}

export function setUpUpdates() {
  el('update-choice').addEventListener('change', (event) => choose(event.target.value));
}
