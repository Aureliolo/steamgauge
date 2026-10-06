/* What SteamGauge keeps on this computer: each drive it uses, the library by part, every game and
   every model by size, and the way to give room back. Every part says what removing it costs,
   and anything that costs hours to redo asks once more, on the page, before it goes. */

import { invoke, el, set, make, button, size, page, setPath } from './common.js';

/* The parts in the order a person weighs them: what they came for first, what rebuilds itself
   last. `everywhere` is the action offered across every game at once, where one is safe. */
const PARTS = [
  { key: 'reviews', name: 'Downloaded reviews', cost: 'Fetched again from Steam in minutes.' },
  { key: 'reads', name: 'Reads', cost: 'Hours of reading on this computer to redo.' },
  { key: 'search', name: 'Search preparations', cost: 'Made again when a search needs them.', everywhere: 'search' },
  { key: 'earlier', name: 'Earlier downloads', cost: 'Replaced by a newer download of the same game.', everywhere: 'earlier' },
  { key: 'partial', name: 'Unfinished work', cost: 'Downloads and reads that stopped; they start over when run again.', everywhere: 'partial' },
  { key: 'other', name: 'Library records', cost: 'Download progress, groups and choices.' },
];

/* What removing each thing from one game means, said before it happens. `heavy` asks again. */
const REMOVALS = [
  { what: 'search', label: 'Search preparations', parts: ['search'] },
  { what: 'earlier', label: 'Earlier downloads', parts: ['earlier'] },
  { what: 'partial', label: 'Unfinished work', parts: ['partial'] },
  { what: 'reads', label: 'Reads', parts: ['reads'], heavy: 'Reading this game again takes hours.' },
  {
    what: 'game',
    label: 'The whole game',
    parts: ['reviews', 'reads', 'search', 'earlier', 'partial', 'other'],
    heavy: 'Every review, every read and its place in groups go; adding it again downloads it from the start.',
  },
];

let found = null;

const sum = (parts, keys) => keys.reduce((total, key) => total + (parts[key] ?? 0), 0);

function legendSwatch(key) {
  return make('span', `swatch part-${key}`);
}

/* A bar of parts, each as wide as its share of `whole`. */
function bar(segments, whole) {
  const track = make('div', 'room-bar');
  for (const [key, bytes] of segments) {
    if (bytes <= 0 || whole <= 0) continue;
    const piece = make('span', `part-${key}`);
    piece.style.width = `${Math.max((100 * bytes) / whole, 0.6).toFixed(2)}%`;
    piece.title = `${PARTS.find((part) => part.key === key)?.name ?? key}: ${size(bytes)}`;
    track.append(piece);
  }
  return track;
}

/* A button that, where the loss is heavy, turns into a question before it acts. */
function removeButton(label, heavy, frees, act) {
  const holder = make('span', 'remove');
  const ask = () => {
    holder.replaceChildren(
      make('span', 'remove-question', `${heavy} Frees ${size(frees)}.`),
      make('span', 'remove-actions', button('Remove', 'danger small', act), button('Keep', 'ghost small', reset)),
    );
  };
  const reset = () => holder.replaceChildren(button(label, 'quiet-button small', heavy ? ask : act));
  reset();
  return holder;
}

export function setUpStorage({ openGame }) {
  page('storage', el('storage'), load);

  async function load() {
    try {
      found = await invoke('storage');
    } catch (failure) {
      set(el('storage-note'), String(failure));
      return;
    }
    draw();
  }

  async function act(command, args, done) {
    set(el('storage-note'), 'Removing…');
    el('storage-note').classList.remove('bad');
    try {
      found = await invoke(command, args);
      draw();
      set(el('storage-note'), done);
    } catch (failure) {
      el('storage-note').classList.add('bad');
      set(el('storage-note'), String(failure));
    }
  }

  function draw() {
    const library = PARTS.reduce((total, part) => total + found.parts[part.key], 0);
    const models = found.model_rooms.reduce((total, model) => total + model.bytes, 0);
    set(el('storage-sub'), `SteamGauge takes ${size(library + models)} on this computer: ${size(library)} of library and ${size(models)} of models.`);

    el('drives').replaceChildren(
      ...found.drives.map((drive) => {
        const ours = (drive.holds.includes('library') ? library : 0) + (drive.holds.includes('models') ? models : 0);
        const used = Math.max(drive.total - drive.free, ours);
        const segments = [];
        if (drive.holds.includes('library')) segments.push(...PARTS.map((part) => [part.key, found.parts[part.key]]));
        if (drive.holds.includes('models')) segments.push(['models', models]);
        segments.push(['elsewhere', used - ours]);
        return make(
          'div',
          'drive',
          make(
            'div',
            'drive-head',
            make('strong', null, drive.name),
            make('span', 'quiet small', drive.holds.length === 2 ? 'Library and models' : drive.holds[0] === 'library' ? 'Library' : 'Models'),
            make('span', 'drive-free', `${size(drive.free)} free of ${size(drive.total)}`),
          ),
          bar(segments, drive.total),
          make(
            'p',
            'drive-key',
            make('span', null, legendSwatch('reviews'), `SteamGauge ${size(ours)}`),
            make('span', null, legendSwatch('elsewhere'), `Other files ${size(Math.max(used - ours, 0))}`),
            make('span', null, make('span', 'swatch free'), `Free ${size(drive.free)}`),
          ),
        );
      }),
    );

    el('parts').replaceChildren(
      ...PARTS.map((part) => {
        const bytes = found.parts[part.key];
        return make(
          'li',
          'part-row',
          legendSwatch(part.key),
          make('span', 'part-text', make('strong', null, part.name), make('span', 'quiet small', part.cost)),
          make('span', 'part-size', size(bytes)),
          part.everywhere && bytes > 0
            ? removeButton('Remove', null, bytes, () =>
                act('free_room', { appId: null, what: part.everywhere }, `${part.name} removed from every game.`),
              )
            : make('span'),
        );
      }),
    );

    const games = el('game-rooms');
    el('game-rooms-none').hidden = found.games.length > 0;
    const widest = Math.max(...found.games.map((game) => game.total), 1);
    games.replaceChildren(
      ...found.games.map((game) => {
        const choose = make('select', 'remove-choice');
        choose.setAttribute('aria-label', `What to remove from ${game.name}`);
        choose.append(make('option', null, 'Remove…'));
        for (const removal of REMOVALS) {
          const frees = sum(game.parts, removal.parts);
          if (frees === 0) continue;
          const option = make('option', null, `${removal.label}, ${size(frees)}`);
          option.value = removal.what;
          choose.append(option);
        }
        const tail = make('span', 'room-act', choose);
        choose.addEventListener('change', () => {
          const removal = REMOVALS.find((one) => one.what === choose.value);
          if (!removal) return;
          const frees = sum(game.parts, removal.parts);
          const go = () =>
            act('free_room', { appId: game.app_id, what: removal.what }, `${removal.label} removed from ${game.name}.`);
          if (!removal.heavy) {
            go();
            return;
          }
          tail.replaceChildren(removeButton(removal.label, removal.heavy, frees, go));
          tail.querySelector('button').click();
        });
        const name = button(game.name, 'link room-name', () => openGame(game.app_id));
        return make(
          'li',
          'room',
          make('span', 'room-text', name, bar(PARTS.map((part) => [part.key, game.parts[part.key]]), widest)),
          make('span', 'room-size', size(game.total)),
          tail,
        );
      }),
    );

    el('model-rooms').replaceChildren(
      ...found.model_rooms.map((model) => {
        const held = model.bytes > 0;
        return make(
          'li',
          'room',
          make(
            'span',
            'room-text',
            make('strong', null, model.name),
            make('span', 'quiet small', `${model.role.charAt(0).toUpperCase()}${model.role.slice(1)}.${held ? '' : ' Not downloaded.'}`),
          ),
          make('span', 'room-size', held ? size(model.bytes) : '–'),
          held
            ? removeButton(
                'Remove',
                model.used ? 'This computer uses it; it is downloaded again the next time it is needed.' : null,
                model.bytes,
                () => act('remove_model', { key: model.key }, `${model.name} removed.`),
              )
            : make('span'),
        );
      }),
    );

    setPath(el('storage-library'), found.library);
    setPath(el('storage-models'), found.models);
  }
}
