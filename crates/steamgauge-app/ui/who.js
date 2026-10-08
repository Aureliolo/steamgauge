/* Who wrote a game's reviews. Steam records beside every review how long its writer had played,
   whether mostly on a Steam Deck, whether during early access and whether they got the game free.
   One kind of reviewer can be shown beside everyone else or beside another kind, and the timeline,
   the table and the points behind every figure follow the choice. Which gaps are marked is decided
   by the core, by the rule what moved is held to: too wide for chance and wide enough to act on.
   A kind with too few reviews is offered no figure at all. */

import { invoke, el, set, make, button, whole } from './common.js';
import { queue, jobsFor, active } from './work.js';

/* Differences listed before the rest wait behind a button. */
const FINDINGS_SHOWN = 6;

const percent = new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 1 });

let hooks = null;
/* The reading the page last drew, and what was chosen on it. */
let reading = null;
let choice = { appId: null, these: '', others: '' };
let showingAll = false;

const lowerFirst = (text) => text.charAt(0).toLowerCase() + text.slice(1);

export function setUpWho(given) {
  hooks = given;
  el('who-first').addEventListener('change', () => choose(el('who-first').value, ''));
  el('who-others').addEventListener('change', () => choose(choice.these, el('who-others').value));
  el('who-everyone').addEventListener('click', () => choose('', ''));
  el('who-more').addEventListener('click', () => {
    showingAll = true;
    drawFindings();
  });
  el('do-recount').addEventListener('click', () => {
    if (reading) queue({ kind: 'recount', app_id: reading.app_id });
  });
}

/* Drawn with every reading the page shows; a choice made on the same game is kept, so coming back
   from the points behind a figure comes back to the same view. */
export function drawWho(found) {
  if (choice.appId !== found.app_id) {
    choice = { appId: found.app_id, these: '', others: '' };
    showingAll = false;
  }
  reading = found;
  const counted = found.who.kinds.length > 0;
  el('who').hidden = false;
  el('who-pick').hidden = !counted;
  el('who-recount').hidden = counted;
  drawRecount();
  if (!counted) {
    choice.these = '';
    showEveryone();
    return;
  }
  if (choice.these && !kindOf(choice.these)?.kind.enough) choice = { ...choice, these: '', others: '' };
  fillFirst();
  choose(choice.these, choice.others);
}

/* Whether counting again is under way for the game, which the button says by being unavailable. */
export function drawRecount() {
  if (!reading) return;
  el('do-recount').disabled = jobsFor(reading.app_id).some((job) => active(job) && job.task.kind === 'recount');
}

/* The kind of reviewer the page shows, as its id and the label it is listed under, or null for
   everyone. */
export function chosenKind() {
  if (!reading || !choice.these) return null;
  const found = kindOf(choice.these);
  return found ? { id: found.kind.id, label: found.kind.label } : null;
}

function kindOf(id) {
  for (const split of reading.who.kinds) {
    const kind = split.kinds.find((one) => one.id === id);
    if (kind) return { split, kind };
  }
  return null;
}

function option(label, value, disabled = false) {
  const node = make('option', null, label);
  node.value = value;
  node.disabled = disabled;
  return node;
}

function fillFirst() {
  el('who-first').replaceChildren(
    option('everyone', ''),
    ...reading.who.kinds.map((split) => {
      const group = make('optgroup');
      group.label = split.label;
      group.append(
        ...split.kinds.map((kind) =>
          option(
            kind.enough
              ? `${kind.label} (${whole.format(kind.reviews)})`
              : `${kind.label} (${whole.format(kind.reviews)}, too few)`,
            kind.id,
            !kind.enough,
          ),
        ),
      );
      return group;
    }),
  );
  el('who-first').value = choice.these;
}

/* What the chosen kind can be set beside: everyone else, or another answer to the same question.
   A question with two answers has one other, and it is everyone else. */
function fillOthers() {
  const { split } = kindOf(choice.these);
  const others = split.kinds.filter((kind) => kind.id !== choice.these);
  const select = el('who-others');
  const fixed = el('who-others-fixed');
  if (others.length === 1) {
    select.hidden = true;
    fixed.hidden = false;
    set(fixed, others[0].label.toLowerCase());
    return;
  }
  select.hidden = false;
  fixed.hidden = true;
  select.replaceChildren(
    option('everyone else', ''),
    ...others.filter((kind) => kind.enough).map((kind) => option(kind.label.toLowerCase(), kind.id)),
  );
  select.value = choice.others;
}

async function choose(these, others) {
  choice = { appId: reading.app_id, these, others };
  el('who-first').value = these;
  const everyone = these === '';
  el('who-beside').hidden = everyone;
  el('who-everyone').hidden = everyone;
  if (everyone) {
    showEveryone();
    return;
  }
  fillOthers();
  let view;
  try {
    view = await invoke('who_wrote', { appId: reading.app_id, these, others: others || null });
  } catch (failure) {
    el('who-note').hidden = false;
    set(el('who-note'), String(failure));
    return;
  }
  if (choice.appId !== reading.app_id || choice.these !== these || choice.others !== others) return;
  drawBeside(view);
}

function showEveryone() {
  el('counts-wrap').hidden = false;
  el('topics-footnote').hidden = false;
  el('who-wrap').hidden = true;
  el('who-legend').hidden = true;
  hooks.drawTimeline(reading.months);
  const counted = reading.who.kinds.length > 0;
  el('who-note').hidden = !counted;
  set(
    el('who-note'),
    counted
      ? 'Steam records beside every review how long its writer had played, whether mostly on a Steam Deck, ' +
          'whether the game was in early access and whether they got it free. Choose one kind of reviewer ' +
          'to see their reviews alone.'
      : '',
  );
  drawFindings();
}

function drawFindings() {
  const counted = reading.who.kinds.length > 0;
  const everyone = choice.these === '';
  const findings = reading.who.findings;
  const shown = showingAll ? findings : findings.slice(0, FINDINGS_SHOWN);
  el('who-findings').replaceChildren(
    ...shown.map((finding) =>
      make(
        'li',
        null,
        make('span', null, finding.sentence),
        button('Show them', 'link small', () => choose(finding.segment, '')),
      ),
    ),
  );
  el('who-differ').hidden = !counted || !everyone || findings.length === 0;
  el('who-none').hidden = !counted || !everyone || findings.length > 0;
  el('who-more').hidden = !everyone || showingAll || findings.length <= FINDINGS_SHOWN;
  set(el('who-more'), `Show ${whole.format(findings.length - FINDINGS_SHOWN)} more`);
}

/* A gap the core marked, said as what the first kind does more or less of. */
function mark(gap, more, less, tone) {
  if (!gap.clear) return null;
  const pill = make('span', gap.z > 0 ? `pill ${tone}` : 'pill', gap.z > 0 ? more : less);
  pill.title = `${percent.format(gap.share)} against ${percent.format(gap.against)}`;
  return pill;
}

function figuresCell(figures, widest, corrected, marks) {
  const raised = Math.max(figures.raised, 1);
  const across = (part) => `${Math.min(100, (100 * part) / widest)}%`;
  const bar = make('span', 'split');
  bar.style.width = across(figures.rate);
  for (const [part, count] of [
    ['praise', figures.praised],
    ['mixed', figures.mixed],
    ['complaint', figures.criticised],
  ]) {
    const piece = make('i', part);
    piece.style.width = `${(100 * count) / raised}%`;
    bar.append(piece);
  }
  /* Where the share would likely fall with more reviews like these: wide on a few hundred, a
     hairline on a hundred thousand. */
  const range = make('span', 'range');
  range.style.left = across(figures.low);
  range.style.width = across(figures.high - figures.low);
  const shown = marks.filter(Boolean);
  const td = make(
    'td',
    'cell',
    make('span', 'rate', percent.format(figures.rate)),
    corrected === null ? null : make('span', 'corrected', `≈ ${percent.format(corrected)} of points`),
    make('span', 'who-track', bar, range),
    make(
      'small',
      null,
      `${percent.format(figures.praising)} praise it, ${percent.format(figures.complaining)} complain`,
    ),
    shown.length > 0 ? make('span', 'who-marks', ...shown) : null,
  );
  td.title =
    `${whole.format(figures.raised)} reviews raise it, ` +
    `likely between ${percent.format(figures.low)} and ${percent.format(figures.high)} of reviews like these`;
  return td;
}

function head(side) {
  const th = make(
    'th',
    'game-col',
    make('span', 'who-name', side.label),
    make('small', null, `${whole.format(side.reviews)} reviews, ${percent.format(side.recommending)} recommend it`),
  );
  th.scope = 'col';
  return th;
}

function drawBeside(view) {
  const { these, others, recommended } = view;
  el('counts-wrap').hidden = true;
  el('topics-footnote').hidden = true;
  el('who-wrap').hidden = false;
  el('who-legend').hidden = false;
  el('who-differ').hidden = true;
  el('who-none').hidden = true;
  el('who-more').hidden = true;
  hooks.drawTimeline(view.months);

  const against = lowerFirst(others.who);
  el('who-note').hidden = false;
  set(
    el('who-note'),
    `${these.who} wrote ${whole.format(these.reviews)} of these reviews, and ` +
      `${percent.format(these.recommending)} of them recommend the game, against ` +
      `${percent.format(others.recommending)} of ${against}` +
      (recommended.clear ? ', a gap too wide for chance. ' : '. ') +
      `The timeline and the table count their reviews alone, beside ${against}.`,
  );

  el('who-head').replaceChildren(make('th', null, 'Subject'), head(these), head(others));
  el('who-head').firstChild.scope = 'col';
  const rows = view.subjects
    .filter((subject) => subject.these.raised + subject.others.raised > 0)
    .sort((left, right) => right.these.rate - left.these.rate);
  const widest = Math.max(0.0001, ...rows.flatMap((subject) => [subject.these.high, subject.others.high]));
  const who = { id: these.id, label: these.label, phrase: lowerFirst(these.who) };
  el('who-rows').replaceChildren(
    ...rows.map((subject) => {
      const open = button(subject.label, 'subject', () =>
        hooks.openClaims(
          { id: subject.id, label: subject.label, reviews: subject.these.raised, praised_terms: [], criticised_terms: [] },
          0,
          null,
          who,
        ),
      );
      const row = make(
        'tr',
        null,
        make('th', null, open),
        figuresCell(subject.these, widest, subject.these_corrected, [
          mark(subject.raised, 'Raised more', 'Raised less', 'accent'),
          mark(subject.praise, 'More praise', 'Less praise', 'good'),
          mark(subject.complaint, 'More complaints', 'Fewer complaints', 'bad'),
        ]),
        figuresCell(subject.others, widest, subject.others_corrected, []),
      );
      /* The whole row opens the points behind the first kind's figures; the name stays the button
         a keyboard reaches. */
      row.addEventListener('click', (event) => {
        if (event.target !== open) open.click();
      });
      return row;
    }),
  );
}
