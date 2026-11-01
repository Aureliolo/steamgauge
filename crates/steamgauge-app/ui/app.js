/* The window: one game at a time, the evidence behind each of its rates, and the way a game is
   added. The cockpit, the library, comparisons and settings live in modules of their own. */

import {
  invoke,
  listen,
  el,
  set,
  make,
  whole,
  share,
  tenth,
  day,
  nothing,
  size,
  duration,
  roughly,
  tiles,
  verdictPill,
  art,
  page,
  go,
  showing,
  openOutside,
  bcp47,
  language,
} from './common.js';
import { startWork, onWork, jobsFor, jobItem, active, queue, drawBrief, settle } from './work.js';
import { setUpCockpit } from './cockpit.js';
import { setUpLibrary } from './library.js';
import { setUpCompare } from './compare.js';
import { setUpGroup } from './group.js';
import { setUpSettings } from './settings.js';
import { setUpAssistants } from './assistants.js';
import { setUpStorage } from './storage.js';
import { setUpNewer } from './newer.js';
import { setUpAbout } from './about.js';
import { setUpFinder, openFinder } from './finder.js';
import { setUpWho, drawWho } from './who.js';
import { setUpUpdates, loadUpdates, markUpdates } from './updates.js';
import { drawSinceLine } from './since.js';

const PER_PAGE = 25;

let chosen = null;
/* The language the shown reading counts, or null for every language. */
let readLanguage = null;

/* The library as the core lists it, for a game's name and capture facts. */
async function shelfGame(appId) {
  const held = await invoke('library');
  return held.games.find((game) => game.app_id === appId) ?? null;
}

function stageTile(stage) {
  if (stage === 'read') return ['Stage', 'Read', 'every claim counted'];
  if (stage === 'embedded') return ['Stage', 'Downloaded', 'ready for search by meaning, not read yet'];
  return ['Stage', 'Downloaded', 'not read yet'];
}

/* A game's page. `named` stands in for a game whose download has only just been asked for and
   is not on disk yet. */
function openGame(appId, named = null) {
  if (appId !== chosen) saidAll = false;
  chosen = appId;
  go('game');
  return showGame(appId, named);
}

/* Draws a game's page where it stands, without moving the page: also how the page follows a job
   on the game finishing. */
async function showGame(appId, named = null) {
  const game = await shelfGame(appId);
  if (chosen !== appId) return;
  const name = game?.name ?? named ?? `App ${appId}`;
  set(el('game-name'), name);
  set(el('back-name'), name);
  art(appId, el('game-art'));
  el('game-verdict').replaceChildren(...[verdictPill(game?.verdict)].filter(Boolean));
  set(el('game-sub'), `App ${appId}`);
  tiles(
    el('game-facts'),
    game === null
      ? []
      : [
          ['Held here', whole.format(game.reviews), 'reviews downloaded'],
          ['Steam reports', whole.format(game.valve_total), 'reviews in total'],
          ['Coverage', share.format(game.coverage), "of Steam's total"],
          stageTile(game.stage),
        ],
  );
  el('game-facts').hidden = game === null;
  if (game === null) el('game-since').hidden = true;
  else seeGame(appId);
  set(
    el('game-note'),
    game === null ? 'This page shows what players talk about once the reviews are downloaded and read.' : '',
  );
  el('game-note').classList.remove('bad');
  el('export-note').hidden = true;
  el('sweep-actions').hidden = game === null;
  el('topics').hidden = true;
  el('game-actions').hidden = true;
  drawGameJobs();
  if (game !== null) loadTopics(appId);
}

/* What changed since the game's page was last seen, then this look recorded, where somebody can
   see the window. */
async function seeGame(appId) {
  const still = () => chosen === appId && showing() === 'game';
  await drawSinceLine(appId, openSubject, still);
  if (still() && document.visibilityState === 'visible') invoke('looked', { appId }).catch(() => {});
}

/* What is running or waiting, and the last of it that finished. */
const current = (jobs) => jobs.filter(active).concat(jobs.filter((job) => !active(job)).slice(0, 1));

/* The work on the shown game. Saving its data is shown under the button that asked for it, the
   rest at the top of the page. */
function drawGameJobs() {
  if (chosen === null) return;
  const mine = jobsFor(chosen);
  const exporting = mine.filter((job) => job.task.kind === 'export_data');
  const shown = current(mine.filter((job) => job.task.kind !== 'export_data'));
  settle(el('game-jobs'), shown.map((job) => jobItem(job, { named: false })));
  const exports = current(exporting);
  settle(el('export-jobs'), exports.map((job) => jobItem(job, { named: false })));
  el('export-jobs').hidden = exports.length === 0;
  el('export-data').disabled = exporting.some(active);
  const reading = mine.some((job) => active(job) && job.task.kind === 'read');
  el('do-read').disabled = reading;
  el('do-sweep').disabled = mine.some((job) => active(job) && job.task.kind === 'update');
}

async function loadTopics(appId) {
  const panel = el('topics');
  try {
    const counted = await invoke('reading', { appId });
    if (counted.app_id !== chosen) return;
    drawTopics(counted);
    drawTimeline(counted.months);
    loadUpdates(appId);
    drawLanguages(counted.languages, counted.corpus_reviews);
    drawWho(counted);
    panel.hidden = false;
    el('game-actions').hidden = true;
    loadInduced(appId);
  } catch (failure) {
    panel.hidden = true;
    el('game-actions').hidden = false;
    set(el('game-note'), String(failure).includes('not been read') ? '' : String(failure));
    drawReadCost(appId);
  }
}

/* What reading would take, said before the button is pressed: the reader the first read has to
   fetch, and on a computer without a graphics card how long it would run. */
async function drawReadCost(appId) {
  let offer;
  try {
    offer = await invoke('read_offer', { appId, language: el('read-language').value || null });
  } catch {
    set(el('read-cost'), '');
    return;
  }
  if (appId !== chosen) return;
  const parts = [];
  if (!offer.here) {
    parts.push(
      offer.published
        ? `The first read fetches the ${offer.reader} reader, a ${size(offer.download_bytes)} download, once.`
        : `No ${offer.reader} reader is on this computer and none has been published yet.`,
    );
  }
  const mine = offer.choices.find((choice) => choice.name === offer.reader);
  if (mine?.seconds) parts.push(`About ${duration(mine.seconds)} on this computer.`);
  set(el('read-cost'), parts.join(' '));
}

/* The core opens the system's save dialog, so the page never names a path. What it writes runs
   as a job, drawn under the button. */
async function exportData() {
  if (chosen === null) return;
  const appId = chosen;
  const note = el('export-note');
  note.hidden = true;
  el('export-data').disabled = true;
  try {
    await invoke('export_data', { appId });
  } catch (failure) {
    if (appId === chosen) {
      set(note, String(failure));
      note.hidden = false;
    }
  }
  el('export-data').disabled = false;
  drawGameJobs();
}

function readGame(only = undefined) {
  if (chosen === null) return;
  const wanted = only === undefined ? el('read-language').value || null : only;
  queue({ kind: 'read', app_id: chosen, language: wanted });
}

function cell(text, className) {
  const td = document.createElement('td');
  td.className = className ? `num ${className}` : 'num';
  const span = document.createElement('span');
  span.textContent = text;
  td.append(span);
  return td;
}

const SVG = 'http://www.w3.org/2000/svg';
const CHART = { width: 1000, height: 160 };

function shape(name, attributes) {
  const node = document.createElementNS(SVG, name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
  return node;
}

/* Reviews per month as bars against the busiest month, and the share recommending the game
   as a line from none at the bottom to all at the top. The same two shapes the report page
   draws, in the same coordinate space, so a reader moving between them sees one chart. A
   rate is drawn only for a month big enough to carry one. The core decides which those are. */
function drawTimeline(months) {
  const figure = el('timeline');
  figure.hidden = months.length < 2;
  if (figure.hidden) {
    markUpdates(months);
    return;
  }

  const peak = months.reduce((best, month) => (month.reviews > best.reviews ? month : best));
  const tallest = Math.max(peak.reviews, 1);
  const step = CHART.width / months.length;
  /* A bar no wider than a column of a busy year, so two months read as two months and not as
     two walls. Each sits centred on its month, where the line's point for it is. */
  const bar = Math.max(Math.min(step * 0.82, 56), 0.5);
  const svg = el('timeline-svg');
  svg.replaceChildren();
  svg.setAttribute('aria-label', `Reviews per month from ${months[0].name} to ${months[months.length - 1].name}`);

  months.forEach((month, index) => {
    const tall = (month.reviews / tallest) * CHART.height;
    svg.append(
      shape('rect', {
        class: 'bar',
        x: (index * step + (step - bar) / 2).toFixed(2),
        y: (CHART.height - tall).toFixed(2),
        width: bar.toFixed(2),
        height: tall.toFixed(2),
      }),
    );
  });
  svg.append(
    shape('line', {
      class: 'midline',
      x1: 0,
      y1: (CHART.height / 2).toFixed(2),
      x2: CHART.width,
      y2: (CHART.height / 2).toFixed(2),
    }),
  );
  const points = months
    .map((month, index) =>
      month.positive === null
        ? null
        : `${(index * step + step / 2).toFixed(2)},${((1 - month.positive) * CHART.height).toFixed(2)}`,
    )
    .filter((point) => point !== null);
  if (points.length > 1) svg.append(shape('polyline', { class: 'share', points: points.join(' ') }));

  /* Full-height hit areas last, because a quiet month is a bar one pixel tall and nothing
     to point at. The title is the only way a month is read. */
  months.forEach((month, index) => {
    const hit = shape('rect', {
      class: 'hit',
      x: (index * step).toFixed(2),
      y: 0,
      width: Math.max(step, 0.5).toFixed(2),
      height: CHART.height,
    });
    const title = document.createElementNS(SVG, 'title');
    title.textContent = `${month.name}: ${whole.format(month.reviews)} reviews, ${
      month.positive === null ? 'too few to carry a rate' : `${share.format(month.positive)} recommended`
    }`;
    hit.append(title);
    svg.append(hit);
  });

  set(
    el('timeline-note'),
    `Reviews per month, against a busiest month of ${whole.format(tallest)} in ${peak.name}. ` +
      `The line is the share recommending the game, and the horizontal dashed line is 50%. ` +
      `Point at a month to read it.`,
  );
  /* One column per month, so the first and last names sit under their own bars. */
  const caption = figure.querySelector('figcaption');
  caption.style.gridTemplateColumns = `repeat(${months.length}, minmax(0, 1fr))`;
  caption.classList.toggle('dense', months.length > 12);
  set(el('timeline-first'), months[0].name);
  set(el('timeline-last'), months[months.length - 1].name);
  el('timeline-last').style.gridColumn = String(months.length);
  markUpdates(months);
}

/* The languages of the whole capture, commonest first, so a reader knows what "reviews"
   means before reading a rate over them. */
function drawLanguages(languages, corpus) {
  const line = el('languages');
  line.hidden = languages.length === 0;
  if (line.hidden) return;
  const shown = languages.slice(0, 8);
  const named = shown.map((counted) =>
    counted.share === null ? language(counted.name) : `${language(counted.name)} ${share.format(counted.share)}`,
  );
  const rest = languages.length - shown.length;
  set(
    line,
    `Languages in the ${whole.format(corpus)} reviews held: ${named.join(', ')}` +
      (rest > 0 ? `, and ${whole.format(rest)} more.` : '.'),
  );
}

/* Loaded after the table rather than with it: it walks the capture for the quoted reviews,
   and most games have not had their own subjects induced yet, so most of the time there is
   nothing to show and nothing to wait for. */
async function loadInduced(appId) {
  const section = el('induced');
  section.hidden = true;
  let found;
  try {
    found = await invoke('induced', { appId });
  } catch {
    return;
  }
  if (appId !== chosen || found.length === 0) return;

  const list = el('induced-list');
  list.replaceChildren();
  for (const subject of found) {
    const dt = make('dt', null, subject.label);
    if (subject.refines !== null) dt.append(make('span', 'quiet', ` a form of ${subject.refines}`));
    const details = make(
      'details',
      null,
      make(
        'summary',
        null,
        `${whole.format(subject.reviews.length)} of the ${whole.format(subject.found_in)} reviews it was found in`,
      ),
    );
    const quotes = make('ol', 'quotes');
    for (const review of subject.reviews) quotes.append(quoteItem(review));
    details.append(quotes);
    list.append(make('div', 'found', dt, make('dd', null, make('p', null, subject.description), details)));
  }
  section.hidden = false;
}

function steamLink(url) {
  const link = make('a', null, 'On Steam');
  link.href = '#';
  link.addEventListener('click', (event) => {
    event.preventDefault();
    openOutside(url);
  });
  return link;
}

/* A review shown whole, with the facts about it and the way back to Steam, and no reading:
   the model that counts the table never read it, so a confidence here would be invented. */
function quoteItem(review) {
  const body = make('p', 'said', review.review);
  body.lang = bcp47(review.language);
  const byline = make(
    'div',
    'byline',
    make(
      'span',
      review.voted_up ? 'verdict-up' : 'verdict-down',
      review.voted_up ? 'Recommended the game' : 'Did not recommend it',
    ),
    review.votes_up > 0 ? make('span', null, `${whole.format(review.votes_up)} found it helpful`) : null,
    make('span', null, day.format(new Date(review.created * 1000))),
    review.url ? steamLink(review.url) : null,
  );
  return make('li', null, body, byline);
}

function drawTopics(found) {
  const ranked = found.subjects
    .filter((subject) => subject.reviews > 0)
    .sort((left, right) => right.reviews - left.reviews);
  const widest = ranked.length > 0 ? (ranked[0].rate ?? 0) : 0;

  const parts = [`${whole.format(found.reviews)} reviews`];
  if (found.language) {
    parts[0] = `${whole.format(found.reviews)} ${language(found.language)} reviews of the ${whole.format(
      found.corpus_reviews,
    )} held`;
  }
  parts.push(`${whole.format(found.claims)} claims`);
  if (found.positive_baseline !== null) {
    parts.push(`${tenth.format(found.positive_baseline)} recommending the game`);
  }
  set(el('topics-counted'), `${parts.join(', ')}. `);

  /* The other reading is one click away, and it is a re-read rather than a filter: the
     model has to read the claims it skipped. */
  readLanguage = found.language;
  set(el('switch-language'), found.language ? 'Count every language instead' : 'Count only English reviews instead');

  drawParagraph(found);
  drawSaidMost(found);

  const swept = el('swept-caveat');
  swept.hidden = found.swept_since === null;
  if (found.swept_since !== null) {
    set(
      swept,
      `The download was brought up to date on ${day.format(new Date(found.swept_since * 1000))}. ` +
        `These counts were made before that. Read it again to count what arrived.`,
    );
  }

  const unread = found.claims > 0 ? found.unclassified_claims / found.claims : 0;
  const silent = found.reviews > 0 ? found.silent_reviews / found.reviews : 0;

  /* Above the table, not below it, once the model has declined more than it answered. Every
     rate under it is then a floor, and a reader who finds that out in a footnote has already
     drawn the wrong conclusion. */
  const caveat = el('topics-caveat');
  caveat.hidden = unread < 0.5;
  set(
    caveat,
    `The reader left ${share.format(unread)} of the claims here unfiled, and filed nothing from ` +
      `${share.format(silent)} of the reviews. Every rate below is a floor.`,
  );

  /* How often it is wrong, where that has been measured. A rate without this beside it is an
     opinion with decimal places, and a game nobody has labelled is told so rather than
     shown the same table with nothing missing from it. */
  const measured = found.measured;
  const heldOut = found.held_out;
  /* Why this game carries no figure of its own: nobody has labelled it, or the reader learned
     from its labels, in which case agreeing with them would measure memory. */
  const why = found.learned
    ? `The reader trained on this game's labels, so nothing here is scored against them.`
    : `No one has labelled this game.`;
  const trust =
    measured === null
      ? heldOut
        ? /* Most games anyone opens are in exactly this position. The reader does have a
             measurement, about other games, and the wording has to say so. */
          `${why} On ${heldOut.games} games it never trained on, the reader answers ` +
          `${share.format(heldOut.coverage)} of claims and agrees with the labels on ` +
          `${share.format(heldOut.accuracy)}.`
        : why
      : measured.agreement === null
        ? `The reader declined every labelled claim on this game, so nothing here is measured.`
        : `Where this game is labelled, the reader agrees with the labels on ` +
          `${share.format(measured.agreement)} of the ${whole.format(measured.answered)} claims it ` +
          `answered (${share.format(measured.low)} to ${share.format(measured.high)}).` +
          /* A label names a span of a review, and a splitter that has since learned to cut
             that review differently leaves the label naming nothing. */
          (measured.unjoined > 0
            ? ` ${whole.format(measured.unjoined)} more labelled claims are left out, because ` +
              `this build splits their reviews differently.`
            : ``);

  const notes = [
    'A review counts once for each subject it raises, so rates add up to more than 100%.',
    `In the most helpful compares the ${whole.format(found.top_of_the_pile)} reviews Steam shows ` +
      `first with all of them.`,
    caveat.hidden
      ? `The reader left ${share.format(unread)} of claims unfiled, and filed nothing from ` +
        `${whole.format(found.silent_reviews)} reviews.`
      : 'Every row opens onto the claims behind it.',
    trust,
  ];
  set(el('topics-footnote'), notes.join(' '));

  const rows = el('topic-rows');
  rows.replaceChildren();
  for (const subject of ranked) {
    const name = make('td');
    const open = make('button', 'subject', subject.label);
    open.type = 'button';
    open.addEventListener('click', () => openClaims(subject, 0));
    name.append(open);
    /* Marked where the model is measured to miss most of a subject's labelled claims: that
       row's rate is a floor, and a reader scanning the table cannot tell it from a count
       unless the row says so. */
    if (subject.found !== null && subject.found < 0.25) {
      const thin = make('span', 'thin', '!');
      thin.title = `Found in only ${share.format(subject.found)} of the labelled claims about it, so this rate is a floor rather than a count`;
      name.append(thin);
    }
    /* The corrected share, where the measured errors allow one. Shown as a hint on the name
       rather than a column of its own, because most games have no labels and a column that
       is empty for most of them teaches a reader to skip it. */
    if (subject.corrected !== null) {
      const fixed = make('span', 'corrected', `≈ ${share.format(subject.corrected)} of claims`);
      fixed.title = 'The corrected share: the claims about this with the reader’s measured errors taken out';
      name.append(fixed);
    }

    const bar = make('i', 'bar');
    bar.style.width = `${widest > 0 ? (100 * (subject.rate ?? 0)) / widest : 0}%`;
    const rate = make(
      'td',
      'rate',
      make('span', 'rate-value', subject.rate === null ? nothing : share.format(subject.rate)),
      make('span', 'rate-track', bar),
    );

    const gauge = make('td', 'num');
    const factor = make('span');
    if (subject.bias === null) {
      factor.textContent = nothing;
      factor.className = 'faint';
    } else {
      factor.textContent = `${subject.bias.toFixed(1)}×`;
      factor.className = subject.bias >= 1.15 ? 'over' : subject.bias <= 0.87 ? 'under' : 'faint';
    }
    gauge.append(factor);

    const row = make(
      'tr',
      null,
      name,
      rate,
      cell(whole.format(subject.praised), 'under'),
      cell(whole.format(subject.criticised), 'over'),
      cell(whole.format(subject.mixed), 'faint'),
      gauge,
    );
    /* The whole row opens the points behind it. The name stays the button a keyboard reaches. */
    row.addEventListener('click', (event) => {
      if (event.target !== open) openClaims(subject, 0);
    });
    rows.append(row);
  }
}

/* What stands out, as the writer put it from the game's themes, or the way to have it written
   where the themes are found and it is not. An empty paragraph is one the check left nothing
   of, which is not offered again for the same themes. */
function drawParagraph(found) {
  const written = found.paragraph ?? '';
  set(el('in-short'), written);
  el('in-short').hidden = written === '';
  el('in-short-note').hidden = written === '';
  el('write-offer').hidden = true;
  if (found.themes !== null && found.paragraph === null) drawWriteOffer(found.app_id);
}

async function drawWriteOffer(appId) {
  let offer;
  try {
    offer = await invoke('writer_offer');
  } catch {
    return;
  }
  if (appId !== chosen) return;
  if (jobsFor(appId).some((job) => active(job) && job.task.kind === 'prepare')) return;
  const where = "on this computer's processor";
  const download = offer.download_bytes > 0 ? `, after a ${size(offer.download_bytes)} download, once` : '';
  set(
    el('write-offer-note'),
    offer.write
      ? `No paragraph has been written for these themes yet: ${roughly(offer.seconds)} ${where}${download}.`
      : `A language model on this computer can write a paragraph of what stands out in these themes, ` +
          `${roughly(offer.seconds)} a game ${where}${download}. Every number and quotation ` +
          'in it is checked against the themes, and a sentence with any other is left out.',
  );
  const write = el('write-it');
  set(write, offer.write ? 'Write it' : 'Write it, here and for every game I prepare');
  write.onclick = async () => {
    el('write-offer').hidden = true;
    if (!offer.write) await invoke('choose_writer', { write: true });
    queue({ kind: 'prepare', app_id: appId });
  };
  el('write-offer').hidden = false;
}

/* Enough of each side to see what stands out. The rest is one click further. */
const SAID_FIRST = 6;
let saidAll = false;

/* The particular things praised and complained about, or the way to have them found: they come
   from the vectors that prepare a game for search by meaning. */
function drawSaidMost(found) {
  const section = el('said-most');
  section.hidden = false;
  const themes = found.themes;
  el('said-sides').hidden = themes === null;
  el('said-most-note').hidden = themes === null;
  el('said-offer').hidden = true;
  el('said-more').hidden = true;
  if (themes === null) {
    drawSaidOffer(found.app_id);
    return;
  }
  const lists = [
    ['said-praise', 'praise', themes.praise],
    ['said-complaint', 'complaint', themes.complaint],
  ];
  for (const [id, side, said] of lists) {
    const shown = saidAll ? said : said.slice(0, SAID_FIRST);
    el(id).replaceChildren(
      ...(said.length === 0
        ? [make('li', 'said-none', 'Nothing is said in enough reviews to name.')]
        : shown.map((theme) => saidItem(found, side, theme))),
    );
  }
  const most = Math.max(themes.praise.length, themes.complaint.length);
  const more = el('said-more');
  more.hidden = most <= SAID_FIRST;
  set(more, saidAll ? 'Show fewer' : `Show all ${whole.format(themes.praise.length + themes.complaint.length)}`);
  more.onclick = () => {
    saidAll = !saidAll;
    drawSaidMost(found);
  };
}

function saidItem(found, side, theme) {
  const parts = [
    make('span', 'said-label', theme.label),
    make(
      'span',
      'said-count',
      `${whole.format(theme.reviews)} ${theme.reviews === 1 ? 'review' : 'reviews'}` +
        (theme.share === null ? '' : `, ${share.format(theme.share)}`) +
        ` · ${theme.subject_label}`,
    ),
  ];
  if (theme.also_in.length > 0) parts.push(make('span', 'said-also', `Also said about ${theme.also_in.join(', ')}`));
  if (theme.quote !== null) parts.push(make('span', 'said-quote', `“${theme.quote}”`));
  const open = make('button', 'said-theme', ...parts);
  open.type = 'button';
  open.addEventListener('click', () => {
    const subject = found.subjects.find((one) => one.id === theme.subject);
    if (subject) openClaims(subject, 0, { side, term: null, theme: theme.theme, label: theme.label });
  });
  return make('li', null, open);
}

async function drawSaidOffer(appId) {
  let offer;
  try {
    offer = await invoke('meaning_offer', { appId });
  } catch {
    el('said-most').hidden = true;
    return;
  }
  if (appId !== chosen) return;
  const note = el('said-offer-note');
  const find = el('said-find');
  find.hidden = offer.job !== null;
  if (offer.job !== null) {
    set(note, 'Being found as this game is prepared for search by meaning; they appear here when it finishes.');
  } else {
    const where = offer.on_card ? "on this computer's graphics card" : 'on this computer';
    const download = offer.download_bytes > 0 ? `, and a ${size(offer.download_bytes)} download, once` : '';
    set(
      note,
      offer.status === 'ready'
        ? `This game is prepared for search by meaning, and finding them from that takes seconds${download}.`
        : `Prepare this game to group what players say and to search it by meaning. ` +
            `${language(roughly(offer.seconds))} ${where}, up to ${size(offer.disk_bytes)}${download}.`,
    );
    set(find, offer.status === 'ready' ? 'Find them' : 'Prepare this game');
    find.onclick = () => {
      find.hidden = true;
      queue({ kind: 'prepare', app_id: appId });
    };
  }
  el('said-offer').hidden = false;
}

let reading = null;

/* `narrowed` is null for every point under the subject, or `{side, term}` for the points on
   one side, using one word from the strip where `term` is given, or `{side, theme, label}` for
   the points of one theme. The same page function serves them all, so the strip is a filter on
   the evidence and not a second view of it. `who` is the kind of reviewer whose reviews alone
   the points are drawn from, or null for everyone. */
async function openClaims(subject, from, narrowed = null, who = null) {
  reading = { subject, from, narrowed, who };
  go('evidence');
  set(el('evidence-name'), subject.label);
  set(el('evidence-lede'), 'Finding them...');
  el('said-strip').hidden = true;
  el('meaning').hidden = true;
  meaningFor = null;
  /* The words that stand out are counted over every point of the subject, so beside one kind's
     points or one theme's they would be other points' figures. */
  const termsFit = who === null && narrowed?.theme === undefined;
  drawTerms(termsFit ? subject : { ...subject, praised_terms: [], criticised_terms: [] }, narrowed);
  el('quotes').replaceChildren();
  el('earlier').disabled = true;
  el('later').disabled = true;

  let found;
  try {
    found = await invoke('claims_behind', {
      appId: chosen,
      subject: subject.id,
      side: narrowed?.side ?? null,
      term: narrowed?.term ?? null,
      who: who?.id ?? null,
      theme: narrowed?.theme ?? null,
      from,
      count: PER_PAGE,
    });
  } catch (failure) {
    set(el('evidence-lede'), String(failure));
    return;
  }
  if (
    reading?.subject?.id !== subject.id ||
    reading.from !== from ||
    reading.narrowed !== narrowed ||
    reading.who !== who
  ) {
    return;
  }

  const sided = narrowed?.side === 'praise' ? 'praising' : 'complaining';
  const claims = `${whole.format(found.total)} ${found.total === 1 ? 'claim' : 'claims'}`;
  const reviews = `${whole.format(subject.reviews)} ${subject.reviews === 1 ? 'review' : 'reviews'}`;
  set(
    el('evidence-lede'),
    who !== null
      ? `${claims} about this from ${who.phrase}, raised in ${reviews} of theirs.`
      : narrowed === null
        ? `${claims} about this, raised in ${reviews}.`
        : narrowed.theme !== undefined
          ? `${whole.format(found.total)} ${sided} ${found.total === 1 ? 'claim' : 'claims'} that say ` +
            `what “${narrowed.label}” says, in those words or others.`
          : narrowed.term
          ? `${whole.format(found.total)} ${sided} ${found.total === 1 ? 'claim' : 'claims'} about this that ` +
            `say “${narrowed.term}”.`
          : `${whole.format(found.total)} ${sided} ${found.total === 1 ? 'claim' : 'claims'} about this.`,
  );
  drawClaims(found.claims);

  const upTo = from + found.claims.length;
  el('paging').hidden = found.total <= PER_PAGE;
  set(el('paging-note'), `${whole.format(from + 1)} to ${whole.format(upTo)} of ${whole.format(found.total)}`);
  el('earlier').disabled = from === 0;
  el('later').disabled = upTo >= found.total;
}

/* From the cockpit: one subject of one game, on the side that moved. */
async function openSubject(appId, subjectId, side) {
  chosen = appId;
  let found;
  try {
    found = await invoke('reading', { appId });
  } catch {
    openGame(appId);
    return;
  }
  const subject = found.subjects.find((one) => one.id === subjectId);
  if (!subject) {
    openGame(appId);
    return;
  }
  openClaims(subject, 0, { side, term: null });
}

const NARROW_NONE = { side: null, subject: null };

/* Search by meaning. The window asks before preparing a game for it, shows what each choice
   costs on this machine, and recommends from what it can see: minutes on a graphics card, hours
   on a processor. */
let meaningFor = null;

async function drawMeaning(query) {
  const appId = chosen;
  meaningFor = { appId, query };
  el('meaning').hidden = false;
  el('meaning-offer').hidden = true;
  el('meaning-quotes').replaceChildren();
  set(el('meaning-note'), '');
  drawMeaningJobs();

  let offer;
  try {
    offer = await invoke('meaning_offer', { appId });
  } catch (failure) {
    set(el('meaning-note'), String(failure));
    return;
  }
  if (meaningFor?.appId !== appId || meaningFor.query !== query) return;
  if (offer.job !== null) return;
  // Something still to fetch means the search cannot run yet, even over a game whose points
  // are all prepared.
  const fetching = offer.download_bytes > 0;
  if (offer.status !== 'ready' || fetching) drawOffer(offer, appId);
  if (offer.status !== 'none' && !fetching) await showNear(appId, query);
}

function drawMeaningJobs() {
  if (!meaningFor) return;
  const preparing = jobsFor(meaningFor.appId).filter((job) => job.task.kind === 'prepare');
  const shown = preparing.filter(active).concat(preparing.filter((job) => !active(job)).slice(0, 1));
  settle(el('meaning-jobs'), shown.map((job) => jobItem(job, { named: false })));
}

function choiceButton(label, detail, recommended, onClick) {
  const choice = make('button', recommended ? 'primary' : 'quiet-button', recommended ? `${label} (recommended)` : label);
  choice.type = 'button';
  if (detail) choice.append(make('span', 'detail', detail));
  choice.addEventListener('click', onClick);
  return choice;
}

function drawOffer(offer, appId) {
  const where = offer.on_card ? "on this computer's graphics card" : 'on this computer';
  const download = offer.download_bytes > 0 ? `, and a ${size(offer.download_bytes)} download, once` : '';
  const cost =
    offer.status === 'ready'
      ? `a ${size(offer.download_bytes)} download, once`
      : `${roughly(offer.seconds)} ${where}, up to ${size(offer.disk_bytes)}${download}`;
  const prepareNow = offer.recommended === 'prepare' || offer.status === 'ready';
  const label = {
    none: 'Prepare this game',
    partial: 'Carry on preparing this game',
    stale: 'Bring this game up to date',
    ready: 'Fetch the two search models',
  }[offer.status];

  const choices = [
    choiceButton(label, cost, prepareNow, () => startPreparing(appId)),
    offer.every_game
      ? choiceButton('Stop preparing every game I read', null, false, async () => {
          await invoke('choose_meaning', { everyGame: false });
          drawMeaning(meaningFor.query);
        })
      : choiceButton(
          'This game, and every game I read from now on',
          'each right after it is read, at about the same cost',
          false,
          async () => {
            await invoke('choose_meaning', { everyGame: true });
            startPreparing(appId);
          },
        ),
    choiceButton('Not now', null, !prepareNow, () => {
      el('meaning-offer').hidden = true;
      set(el('meaning-note'), 'Not prepared. Searching again will ask again.');
    }),
  ];
  el('meaning-choices').replaceChildren(...choices);
  set(
    el('meaning-advice'),
    offer.status === 'ready'
      ? 'This game is prepared, but the two models that search it by meaning are not on this ' +
          'computer: one finds the claims nearest what you typed, the other reads each of them ' +
          'beside it so that opposites like "boring" and "fun" are not shown as the same thing.'
      : prepareNow
        ? 'Recommended here: this computer has a graphics card the app can use, so it takes minutes.'
        : `Not recommended here: this computer has no graphics card the app can use, so it would ` +
          `take ${duration(offer.seconds)}. It runs while you do other things, and stopping ` +
          'keeps what is done.',
  );
  el('meaning-offer').hidden = false;
}

function startPreparing(appId) {
  el('meaning-offer').hidden = true;
  set(el('meaning-note'), '');
  queue({ kind: 'prepare', app_id: appId });
}

async function showNear(appId, query) {
  set(el('meaning-note'), 'Looking for the same thing in other words...');
  let near;
  try {
    near = await invoke('search_by_meaning', { appId, query });
  } catch (failure) {
    set(el('meaning-note'), String(failure));
    return;
  }
  if (meaningFor?.appId !== appId || meaningFor.query !== query) return;
  if (near.length === 0) {
    set(el('meaning-note'), 'Nothing the words above missed comes close enough in meaning to show.');
    return;
  }
  set(
    el('meaning-note'),
    `The ${whole.format(near.length)} claims closest in meaning that the words above did not ` +
      'find, closest first. Not counted: closeness in meaning has no line at which saying it ' +
      'stops, so a number here would only say where the line was drawn.',
  );
  drawClaims(near, el('meaning-quotes'));
}

/* What reviewers said in the words somebody typed. The counts are always of every point that
   says it. A side or a subject narrows only the points listed, so the figures at the top never
   change under the reader's hand. */
async function openSearch(query, from, narrow = NARROW_NONE) {
  const appId = chosen;
  // The same words asked of another game are a new search, meaning and all.
  const fresh = reading?.query !== query || reading.appId !== appId;
  reading = { appId, query, from, narrow };
  go('evidence');
  if (fresh) drawMeaning(query);
  set(el('evidence-name'), `“${query}”`);
  set(el('evidence-lede'), 'Looking through every review...');
  el('stands-out').hidden = true;
  el('said-strip').hidden = true;
  el('quotes').replaceChildren();
  set(el('paging-note'), '');
  el('earlier').disabled = true;
  el('later').disabled = true;

  let found;
  try {
    found = await invoke('search_game', {
      appId,
      query,
      side: narrow.side,
      subject: narrow.subject,
      from,
      count: PER_PAGE,
    });
  } catch (failure) {
    set(el('evidence-lede'), String(failure));
    return;
  }
  if (reading?.appId !== appId || reading.query !== query || reading.from !== from || reading.narrow !== narrow)
    return;

  if (found.claims === 0) {
    el('paging').hidden = true;
    set(
      el('evidence-lede'),
      `No review counted here says “${query}”. Only the words typed are looked for, so a ` +
        'different wording or another language may still say it.',
    );
    return;
  }
  const of = found.share === null ? '' : `, ${share.format(found.share)} of the reviews counted,`;
  set(
    el('evidence-lede'),
    `${whole.format(found.reviews)} ${found.reviews === 1 ? 'review' : 'reviews'}${of} ` +
      `${found.reviews === 1 ? 'says' : 'say'} it, in ${whole.format(found.claims)} ` +
      `${found.claims === 1 ? 'claim' : 'claims'}, most helpful review first.`,
  );
  drawSaid(found, query, narrow);
  drawClaims(found.page);

  const upTo = from + found.page.length;
  el('paging').hidden = found.narrowed <= PER_PAGE;
  set(el('paging-note'), `${whole.format(from + 1)} to ${whole.format(upTo)} of ${whole.format(found.narrowed)}`);
  el('earlier').disabled = from === 0;
  el('later').disabled = upTo >= found.narrowed;
}

function chip(text, count, chosenHere, onClick) {
  const node = make(onClick ? 'button' : 'span', `term${chosenHere ? ' chosen' : ''}${onClick ? '' : ' static'}`, text);
  if (onClick) {
    node.type = 'button';
    node.addEventListener('click', onClick);
  }
  node.append(make('span', 'n', whole.format(count)));
  return node;
}

function drawSaid(found, query, narrow) {
  const refill = (id, chips) => {
    const strip = el(id);
    for (const stale of strip.querySelectorAll('.term')) stale.remove();
    strip.append(...chips);
    strip.hidden = chips.length === 0;
  };
  const narrowTo = (change) => () => openSearch(query, 0, { ...narrow, ...change });

  const sides = [
    ['praise', 'Praise', found.praise],
    ['complaint', 'Complaints', found.complaint],
    ['neutral', 'Neither', found.neutral],
  ].filter(([, , count]) => count > 0);
  refill(
    'said-sides',
    sides.map(([side, label, count]) => {
      const here = narrow.side === side;
      return chip(label, count, here, narrowTo({ side: here ? null : side }));
    }),
  );
  refill(
    'said-subjects',
    found.subjects.map((subject) => {
      const here = narrow.subject === subject.id;
      return chip(subject.label, subject.claims, here, narrowTo({ subject: here ? null : subject.id }));
    }),
  );
  refill(
    'said-forms',
    found.forms.map(([form, count]) => chip(form, count, false, null)),
  );
  el('said-strip').hidden = false;
}

/* The words each side of a subject uses and the other does not, each a button that narrows
   the page to the points using it. Nothing is drawn for a side with nothing to say, and the
   whole strip goes when neither side has anything, which on a game the model barely reads is
   the honest state rather than an empty box. */
function drawTerms(subject, narrowed) {
  const sides = [
    ['praised-terms', 'praise', subject.praised_terms],
    ['criticised-terms', 'complaint', subject.criticised_terms],
  ];
  let any = false;
  for (const [id, side, terms] of sides) {
    const strip = el(id);
    strip.hidden = terms.length === 0;
    for (const stale of strip.querySelectorAll('button')) stale.remove();
    for (const term of terms) {
      any = true;
      const chosenHere = narrowed !== null && narrowed.side === side && narrowed.term === term.text;
      const node = make('button', chosenHere ? 'term chosen' : 'term', term.text, make('span', 'n', whole.format(term.reviews)));
      node.type = 'button';
      node.title = chosenHere ? 'Back to every claim about this' : `${whole.format(term.reviews)} reviews used this word on this side`;
      node.addEventListener('click', () => openClaims(subject, 0, chosenHere ? null : { side, term: term.text }));
      strip.append(node);
    }
  }
  el('stands-out').hidden = !any;
}

function drawClaims(claims, list = el('quotes')) {
  list.replaceChildren();
  for (const found of claims) {
    const body = make('p', 'said');
    body.lang = bcp47(found.language);
    /* The claim is shown inside the review it came from, so a reader can see whether it was
       cut in the right place rather than taking the split on trust. */
    const at = found.review.indexOf(found.claim);
    if (at === -1) {
      body.textContent = found.claim;
    } else {
      body.append(
        make('span', 'quiet', found.review.slice(Math.max(0, at - 160), at)),
        make('b', null, found.claim),
        make('span', 'quiet', found.review.slice(at + found.claim.length, at + found.claim.length + 160)),
      );
    }

    const byline = make(
      'div',
      'byline',
      make(
        'span',
        found.polarity === 'praise' ? 'verdict-up' : found.polarity === 'complaint' ? 'verdict-down' : '',
        found.polarity === 'praise' ? 'Praise' : found.polarity === 'complaint' ? 'Complaint' : 'Neutral',
      ),
      make('span', null, `${share.format(found.confidence)} sure`),
      make('span', null, found.voted_up ? 'Recommended the game' : 'Did not recommend it'),
      found.votes_up > 0 ? make('span', null, `${whole.format(found.votes_up)} found it helpful`) : null,
      make('span', null, day.format(new Date(found.created * 1000))),
      found.url ? steamLink(found.url) : null,
    );
    list.append(make('li', null, body, byline));
  }
}

function turnPage(from) {
  if (!reading) return;
  if (reading.query !== undefined) openSearch(reading.query, from, reading.narrow);
  else openClaims(reading.subject, from, reading.narrowed, reading.who);
}

page('game', el('game'), null, 'library');
page('evidence', el('evidence'), null, 'library');
setUpFinder({ openGame });
setUpWho({ openClaims, drawTimeline });
setUpCockpit({ openGame, openSubject });
setUpLibrary({ openGame, openFinder, compare: (appIds) => go('compare', appIds) });
setUpCompare({ openGame });
setUpGroup({ openGame, openSubject });
setUpSettings();
setUpAssistants();
setUpStorage({ openGame });
setUpNewer();
setUpAbout();
setUpUpdates();

for (const link of document.querySelectorAll('[data-go]')) {
  link.addEventListener('click', () => go(link.dataset.go));
}
el('rail-add').addEventListener('click', openFinder);
el('game-back').addEventListener('click', () => go('library'));
el('back').addEventListener('click', () => openGame(chosen));
el('do-read').addEventListener('click', () => readGame());
el('export-data').addEventListener('click', exportData);
el('read-language').addEventListener('change', () => {
  if (chosen !== null) drawReadCost(chosen);
});
el('do-sweep').addEventListener('click', () => {
  if (chosen !== null) queue({ kind: 'update', app_id: chosen });
});
el('switch-language').addEventListener('click', () => readGame(readLanguage ? null : 'english'));
el('game-store').addEventListener('click', () => {
  if (chosen !== null) openOutside(`https://store.steampowered.com/app/${chosen}/`);
});
el('earlier').addEventListener('click', () => {
  if (reading) turnPage(Math.max(0, reading.from - PER_PAGE));
});
el('later').addEventListener('click', () => {
  if (reading) turnPage(reading.from + PER_PAGE);
});
el('search-form').addEventListener('submit', (event) => {
  event.preventDefault();
  const query = el('search-query').value.trim();
  if (query && chosen !== null) openSearch(query, 0);
});

onWork(() => {
  if (showing() === 'game') drawGameJobs();
  if (showing() === 'evidence') drawMeaningJobs();
});
/* A job that finished changed what is on disk: the game on screen is shown again where it was
   that game, and a search that was waiting on its preparation is asked again. */
listen('library', ({ payload }) => {
  if (payload !== chosen) return;
  if (showing() === 'game') showGame(chosen);
  if (showing() === 'evidence' && meaningFor && !el('meaning').hidden) drawMeaning(meaningFor.query);
});

/* A desktop notification clicked: the game it named. */
listen('open-game', ({ payload }) => openGame(payload));

/* A program steering the app through its MCP server asked to show the person a page. */
listen('show', async ({ payload }) => {
  const { page: name, app_id: appId, app_ids: appIds, group, subject, side, query } = payload;
  if (name === 'game') openGame(appId);
  else if (name === 'subject') openSubject(appId, subject, side ?? null);
  else if (name === 'search') {
    await openGame(appId);
    openSearch(query, 0);
  } else if (name === 'compare') go('compare', appIds);
  else if (name === 'group') go('group', group);
  else if (name === 'finder') openFinder();
  else go(name);
});

/* It changed something: the page on screen is drawn again from what is there now. */
listen('steered', () => {
  const now = showing();
  if (now === 'game' && chosen !== null) showGame(chosen);
  else if (['cockpit', 'library', 'settings', 'assistants', 'storage', 'about'].includes(now)) go(now);
});

drawBrief(el('rail-work'));
el('rail-work').addEventListener('click', () => go('cockpit'));
startWork();
go('cockpit');
