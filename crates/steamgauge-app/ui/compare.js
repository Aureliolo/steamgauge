/* Several games side by side: for each subject, how many of each game's reviews raise it, and of
   those how many praise it and how many complain. Every share is of the game's own reviews, so a
   game with a million reviews and one with a thousand compare as rates, not as counts. */

import {
  button,
  ENOUGH_TO_LEAN,
  el,
  invoke,
  language,
  make,
  nothing,
  page,
  set,
  share,
  thin,
  whole,
} from './common.js';

let chosen = [];
let found = [];

function mentionRate(game, id) {
  const subject = game.subjects.find((one) => one.id === id);
  return subject && game.reviews > 0 ? subject.mention_reviews / game.reviews : 0;
}

function cellNote(subject, raised, enough) {
  if (!(enough || thin.shown)) {
    return `Too few reviews to split (${whole.format(raised)})`;
  }
  const praising = (subject.praised + subject.mixed) / Math.max(raised, 1);
  const complaining = (subject.criticised + subject.mixed) / Math.max(raised, 1);
  const split = `${share.format(praising)} praise, ${share.format(complaining)} complain`;
  return enough
    ? split
    : `${split}, from ${whole.format(raised)} ${raised === 1 ? 'review' : 'reviews'}`;
}

function compareCell(game, id, widest) {
  const subject = game.subjects.find((one) => one.id === id);
  if (!subject || game.reviews === 0) {
    return make('td', 'faint', nothing);
  }
  const raised = subject.mention_reviews;
  const enough = raised >= ENOUGH_TO_LEAN;
  const bar = make('span', enough || thin.shown ? 'split' : 'split plain');
  bar.style.width = `${(100 * mentionRate(game, id)) / widest}%`;
  if (enough || thin.shown) {
    const praise = make('i', 'praise');
    praise.style.width = `${100 * (subject.praised / Math.max(raised, 1))}%`;
    const mixed = make('i', 'mixed');
    mixed.style.width = `${100 * (subject.mixed / Math.max(raised, 1))}%`;
    const complaint = make('i', 'complaint');
    complaint.style.width = `${100 * (subject.criticised / Math.max(raised, 1))}%`;
    bar.append(praise, mixed, complaint);
  }
  const td = make(
    'td',
    'cell',
    make('span', 'rate', share.format(mentionRate(game, id))),
    bar,
    make('small', null, cellNote(subject, raised, enough)),
  );
  td.title = `${whole.format(raised)} of ${whole.format(game.reviews)} reviews raise it`;
  return td;
}

function setUpCompare({ openGame }) {
  page('compare', el('compare'), (appIds) => {
    if (appIds) {
      chosen = [...appIds];
    }
    el('compare-thin').checked = thin.shown;
    load();
  });

  el('compare-thin').addEventListener('change', () => {
    thin.shown = el('compare-thin').checked;
    draw(found);
  });

  el('compare-add').addEventListener('change', () => {
    const appId = Number(el('compare-add').value);
    el('compare-add').value = '';
    if (appId && !chosen.includes(appId)) {
      chosen.push(appId);
      load();
    }
  });
  el('compare-export').addEventListener('click', () => invoke('export_report', { appIds: chosen }));

  async function load() {
    let games = [];
    found = [];
    try {
      [games, found] = await Promise.all([
        invoke('games'),

        chosen.length > 0 ? invoke('compare', { appIds: chosen }) : [],
      ]);
    } catch (failure) {
      set(el('compare-sub'), String(failure));
      return;
    }
    const readable = games.filter((game) => game.read && !chosen.includes(game.app_id));
    el('compare-add').replaceChildren(
      make('option', null, chosen.length > 0 ? 'Add another game…' : 'Choose a game…'),
      ...readable.map((game) => {
        const option = make('option', null, game.name);
        option.value = game.app_id;
        return option;
      }),
    );
    el('compare-add').firstChild.value = '';
    el('compare-export').disabled = found.length === 0;
    draw(found);
  }

  function draw(columns) {
    set(
      el('compare-sub'),
      columns.length < 2
        ? 'Each share is of that game’s own reviews.'
        : `${columns.length} games. Each share is of that game’s own reviews.`,
    );
    const head = el('compare-head');
    head.replaceChildren(
      make('th', null, 'Subject'),
      ...columns.map((game) => {
        const th = make(
          'th',
          'game-col',
          button(game.name, 'link', () => openGame(game.app_id)),
          make(
            'small',
            null,
            `${whole.format(game.reviews)} ${game.language ? `${language(game.language)} ` : ''}reviews` +
              (game.recommended === null ? '' : `, ${share.format(game.recommended)} recommending`),
          ),
          button('Remove', 'link quiet small', () => {
            chosen = chosen.filter((id) => id !== game.app_id);
            load();
          }),
        );
        th.scope = 'col';
        return th;
      }),
    );

    /* Subjects in the order the games raise them on average, so the top of the table is what
       all of them are about and the bottom is what none of them are. */
    const ids = columns[0]?.subjects.map((subject) => subject.id) ?? [];
    const ranked = ids
      .map((id) => [
        id,
        columns.reduce((sum, game) => sum + mentionRate(game, id), 0) / Math.max(columns.length, 1),
      ])
      .filter(([, mean]) => mean > 0)
      .sort((a, b) => b[1] - a[1]);
    const widest = Math.max(
      0.0001,
      ...columns.flatMap((game) => ids.map((id) => mentionRate(game, id))),
    );

    const body = el('compare-rows');
    body.replaceChildren(
      ...ranked.map(([id]) => {
        const { label } = columns[0].subjects.find((one) => one.id === id);
        return make(
          'tr',
          null,
          make('th', 'subject', label),
          ...columns.map((game) => compareCell(game, id, widest)),
        );
      }),
    );
    el('compare-table').hidden = columns.length === 0;
    el('compare-wrap').hidden = columns.length === 0;
    el('compare-empty').hidden = columns.length > 0;
    el('compare-legend').hidden = columns.length === 0;
  }
}

export { setUpCompare };
