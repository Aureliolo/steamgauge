/* The library: every game in one table, sorted and grouped however the person likes, with the
   work that can be done to several games at once. */

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
  nothing,
  language,
  page,
  go,
  showing,
} from './common.js';
import { jobs, active, onWork } from './work.js';

let rows = [];
let groups = [];
let group = null;
let sortBy = 'name';
let descending = false;
const selected = new Set();

const SORTS = {
  name: (row) => row.name.toLowerCase(),
  reviews: (row) => row.reviews,
  recommended: (row) => row.recommended ?? -1,
  new: (row) => row.new_on_steam ?? -1,
  updated: (row) => row.updated ?? row.downloaded,
  lately: (row) => Math.abs(row.recent?.moves?.[0]?.shift?.z ?? 0),
};

/* What a sort starts as: names from A, everything else from the most. */
const STARTS_DESCENDING = new Set(['reviews', 'recommended', 'new', 'updated', 'lately']);

export function setUpLibrary({ openGame, openFinder, compare }) {
  page('library', el('library'), load);

  el('library-add').addEventListener('click', openFinder);
  el('update-all').addEventListener('click', () => invoke('queue_updates', { appIds: null }));
  el('library-filter').addEventListener('input', draw);
  el('library-sort').addEventListener('change', () => {
    sortBy = el('library-sort').value;
    descending = STARTS_DESCENDING.has(sortBy);
    draw();
  });
  el('select-all').addEventListener('change', () => {
    for (const row of visible()) {
      if (el('select-all').checked) selected.add(row.app_id);
      else selected.delete(row.app_id);
    }
    draw();
  });

  el('selected-update').addEventListener('click', () => invoke('queue_updates', { appIds: [...selected] }));
  el('selected-read').addEventListener('click', () => invoke('queue_reads', { appIds: [...selected] }));
  el('selected-compare').addEventListener('click', () => compare([...selected]));
  el('selected-export').addEventListener('click', () => invoke('export_report', { appIds: [...selected] }));
  el('selected-group').addEventListener('change', async () => {
    const name = el('selected-group').value;
    el('selected-group').value = '';
    if (!name) return;
    if (name === '\u0000new') {
      el('new-group').hidden = false;
      el('new-group-name').focus();
      return;
    }
    await addToGroup(name);
  });
  el('new-group').addEventListener('submit', async (event) => {
    event.preventDefault();
    const name = el('new-group-name').value.trim();
    if (!name) return;
    el('new-group-name').value = '';
    el('new-group').hidden = true;
    await addToGroup(name);
  });
  el('selected-ungroup').addEventListener('click', async () => {
    const changed = groups.map((one) =>
      one.name === group ? { ...one, app_ids: one.app_ids.filter((id) => !selected.has(id)) } : one,
    );
    await saveGroups(changed);
  });
  el('delete-group').addEventListener('click', async () => {
    const gone = group;
    group = null;
    await saveGroups(groups.filter((one) => one.name !== gone));
  });
  el('rename-group').addEventListener('submit', async (event) => {
    event.preventDefault();
    const name = el('rename-group-name').value.trim();
    if (!name || name === group) return;
    const before = group;
    group = name;
    await saveGroups(groups.map((one) => (one.name === before ? { ...one, name } : one)));
  });

  listen('library', () => {
    if (showing() === 'library') load();
  });
  /* A row says what is being done to its game, so the table follows the board, redrawn only when
     what it says changes rather than on every tick of a download. */
  let said = '';
  onWork((all) => {
    const saying = all
      .filter(active)
      .map((job) => `${job.task.app_id}:${job.state}:${job.step}`)
      .join('|');
    if (saying === said) return;
    said = saying;
    if (showing() === 'library' && rows.length > 0) draw();
  });

  async function load() {
    try {
      [rows, { groups }] = await Promise.all([invoke('games'), invoke('groups')]);
    } catch (failure) {
      set(el('library-sub'), String(failure));
      return;
    }
    if (group !== null && !groups.some((one) => one.name === group)) group = null;
    for (const id of [...selected]) if (!rows.some((row) => row.app_id === id)) selected.delete(id);
    draw();
  }

  async function addToGroup(name) {
    const exists = groups.some((one) => one.name.toLowerCase() === name.toLowerCase());
    const changed = exists
      ? groups.map((one) =>
          one.name.toLowerCase() === name.toLowerCase()
            ? { ...one, app_ids: [...new Set([...one.app_ids, ...selected])] }
            : one,
        )
      : [...groups, { name, app_ids: [...selected] }];
    await saveGroups(changed);
  }

  async function saveGroups(changed) {
    try {
      ({ groups } = await invoke('save_groups', { groups: { groups: changed } }));
    } catch (failure) {
      set(el('library-sub'), String(failure));
      return;
    }
    await load();
  }

  function visible() {
    const needle = el('library-filter').value.trim().toLowerCase();
    const inGroup = group === null ? null : groups.find((one) => one.name === group)?.app_ids ?? [];
    const key = SORTS[sortBy];
    return rows
      .filter((row) => inGroup === null || inGroup.includes(row.app_id))
      .filter((row) => !needle || row.name.toLowerCase().includes(needle) || String(row.app_id).includes(needle))
      .sort((a, b) => {
        const [left, right] = [key(a), key(b)];
        const order = left < right ? -1 : left > right ? 1 : 0;
        return descending ? -order : order;
      });
  }

  function draw() {
    set(
      el('library-sub'),
      rows.length === 0
        ? 'Nothing here yet. Add a game to download every review it has.'
        : `${whole.format(rows.length)} games, ${whole.format(rows.reduce((sum, row) => sum + row.reviews, 0))} reviews.`,
    );
    drawTabs();
    const shown = visible();
    const body = el('library-rows');
    body.replaceChildren(...shown.map((row) => rowOf(row)));
    el('library-none').hidden = shown.length > 0 || rows.length === 0;
    el('select-all').checked = shown.length > 0 && shown.every((row) => selected.has(row.app_id));
    drawSelection();
  }

  function drawTabs() {
    const tabs = el('group-tabs');
    const tab = (label, name, count) => {
      const node = button(label, 'tab', () => {
        group = name;
        draw();
      });
      if (group === name) node.setAttribute('aria-current', 'true');
      node.append(make('span', 'n', whole.format(count)));
      return node;
    };
    tabs.replaceChildren(
      tab('Every game', null, rows.length),
      ...groups.map((one) => tab(one.name, one.name, one.app_ids.length)),
    );
    el('group-tools').hidden = group === null;
    if (group !== null) el('rename-group-name').value = group;
  }

  function drawSelection() {
    const count = selected.size;
    el('selection').hidden = count === 0;
    set(el('selected-count'), `${whole.format(count)} selected`);
    el('selected-compare').disabled = count < 2;
    el('selected-ungroup').hidden = group === null;
    const choose = el('selected-group');
    choose.replaceChildren(
      make('option', null, 'Add to a group…'),
      ...groups.map((one) => {
        const option = make('option', null, one.name);
        option.value = one.name;
        return option;
      }),
    );
    choose.firstChild.value = '';
    const fresh = make('option', null, 'A new group…');
    fresh.value = '\u0000new';
    choose.append(fresh);
  }

  function rowOf(row) {
    const tr = make('tr');
    if (selected.has(row.app_id)) tr.className = 'chosen';

    const tick = make('input');
    tick.type = 'checkbox';
    tick.checked = selected.has(row.app_id);
    tick.setAttribute('aria-label', `Select ${row.name}`);
    tick.addEventListener('change', () => {
      if (tick.checked) selected.add(row.app_id);
      else selected.delete(row.app_id);
      draw();
    });

    const working = jobs().find((job) => active(job) && job.task.app_id === row.app_id);
    const name = make(
      'td',
      null,
      make(
        'div',
        'game-cell',
        button(row.name, 'link game-link', () => openGame(row.app_id)),
        make('span', 'quiet', row.verdict || `App ${row.app_id}`),
        working ? make('span', 'busy', working.state === 'running' ? working.step : 'Waiting') : null,
        ...row.groups.map((group) => make('span', 'tag', group)),
      ),
    );

    const reading = row.read
      ? make(
          'span',
          row.read.current ? null : 'older',
          row.read.current ? 'Read' : 'Read by an older reader',
          make('small', null, row.read.language ? `${language(row.read.language)} only` : 'Every language'),
        )
      : make('span', 'faint', 'Not read');

    const moved = row.recent?.moves?.[0];
    const lately = moved
      ? make(
          'span',
          `lately ${moved.side} ${moved.shift.recent > moved.shift.before ? 'up' : 'down'}`,
          `${moved.side === 'praise' ? 'Praise' : 'Complaints'}: ${moved.label.toLowerCase()}`,
          make(
            'small',
            null,
            `${roundShare.format(moved.shift.before)} → ${roundShare.format(moved.shift.recent)}`,
          ),
        )
      : make('span', 'faint', nothing);

    tr.append(
      make('td', 'tick', tick),
      name,
      make('td', 'num', whole.format(row.reviews)),
      make('td', 'num', row.recommended === null ? nothing : roundShare.format(row.recommended)),
      make('td', 'num', row.new_on_steam ? `+${whole.format(row.new_on_steam)}` : nothing),
      make('td', 'num', day.format(new Date((row.updated ?? row.downloaded) * 1000))),
      make('td', null, reading),
      make('td', null, lately),
    );
    return tr;
  }
}

export function openLibrary() {
  go('library');
}
