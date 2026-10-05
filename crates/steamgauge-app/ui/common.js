/* What every page of the window shares: the bridge to the core, the formats numbers are shown
   in, and the way from one page to another. */

export const { invoke } = window.__TAURI__.core;
export const { listen } = window.__TAURI__.event;

export const el = (id) => document.getElementById(id);

export const whole = new Intl.NumberFormat();
export const share = new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 2 });
export const roundShare = new Intl.NumberFormat(undefined, { style: 'percent', maximumFractionDigits: 0 });
export const day = new Intl.DateTimeFormat(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
export const when = new Intl.DateTimeFormat(undefined, {
  month: 'short',
  day: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
});

// A cell with no number behind it, drawn as the report draws one.
export const nothing = '–';

export function set(node, text) {
  node.textContent = text;
}

/* Builds an element from a tag, a class and children. Text is always set as text, never parsed:
   game names and reviews come from Steam. */
export function make(tag, className, ...children) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    node.append(child instanceof Node ? child : String(child));
  }
  return node;
}

/* A folder shown so a line breaks after a separator, where a reader expects it, and nowhere
   inside a name unless one name is wider than the line. */
export function setPath(node, path) {
  node.replaceChildren(
    ...path.split(/(?<=[\\/])/).flatMap((part, index) => (index === 0 ? [part] : [document.createElement('wbr'), part])),
  );
}

export function button(label, className, onClick) {
  const node = make('button', className, label);
  node.type = 'button';
  node.addEventListener('click', onClick);
  return node;
}

export const size = (bytes) =>
  bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(bytes / 1e6))} MB`;

export function duration(seconds) {
  if (seconds < 60) return 'under a minute';
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return minutes === 1 ? 'a minute' : `${minutes} minutes`;
  return `${Math.round(seconds / 360) / 10} hours`;
}

/* Time left, said the way a person reads a clock rather than a stopwatch. */
export function left(seconds) {
  if (seconds < 10) return 'a few seconds left';
  if (seconds < 60) return `${Math.round(seconds / 5) * 5} s left`;
  if (seconds < 3600) {
    const minutes = Math.round(seconds / 60);
    return minutes === 1 ? 'a minute left' : `${minutes} min left`;
  }
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.round((seconds % 3600) / 60);
  return minutes === 0 ? `${hours} h left` : `${hours} h ${minutes} min left`;
}

/* A Steam language name as a person writes it: `english` is English. */
export const language = (name) => (name ? name.charAt(0).toUpperCase() + name.slice(1) : name);

export function monthName(label) {
  const [year, month] = label.split('-').map(Number);
  if (!year || !month) return label;
  return new Date(Date.UTC(year, month - 1, 15)).toLocaleDateString(undefined, {
    month: 'short',
    year: 'numeric',
    timeZone: 'UTC',
  });
}

export function facts(list, entries) {
  list.replaceChildren();
  for (const [term, value, under] of entries) {
    const dd = make('dd', null, value);
    if (under) dd.append(make('small', null, under));
    list.append(make('div', null, make('dt', null, term), dd));
  }
}

/* Large figures in cards: the term, the figure, and a line under it. */
export function tiles(list, entries) {
  list.replaceChildren(
    ...entries.map(([term, value, under]) =>
      make(
        'div',
        'tile',
        make('dt', null, term),
        make('dd', 'tile-value', value),
        under ? make('dd', 'tile-under', under) : null,
      ),
    ),
  );
}

/* Steam's verdict as a pill coloured by which way it leans. */
export function verdictPill(verdict) {
  if (!verdict) return null;
  const leaning = /positive/i.test(verdict) ? 'good' : /negative/i.test(verdict) ? 'bad' : 'warn';
  return make('span', `pill ${leaning}`, make('span', 'dot'), verdict);
}

/* A game's store picture. The core fetches each once and keeps it, and the window loads only a
   few at a time, so a library of seventy games does not ask for seventy at once. */
const pictures = new Map();
const waitingForArt = [];
let artInFlight = 0;
const ART_AT_ONCE = 4;

function nextArt() {
  while (artInFlight < ART_AT_ONCE && waitingForArt.length > 0) {
    const { appId, settle } = waitingForArt.shift();
    artInFlight += 1;
    invoke('art', { appId })
      .then((bytes) => settle(URL.createObjectURL(new Blob([bytes]))))
      .catch(() => settle(null))
      .finally(() => {
        artInFlight -= 1;
        nextArt();
      });
  }
}

function artUrl(appId) {
  if (!pictures.has(appId)) {
    pictures.set(
      appId,
      new Promise((settle) => {
        waitingForArt.push({ appId, settle });
        nextArt();
      }),
    );
  }
  return pictures.get(appId);
}

/* The picture's frame, filled in when the picture arrives; a game with none keeps the frame. */
export function art(appId, holder = make('span', 'art')) {
  holder.replaceChildren();
  holder.dataset.app = String(appId);
  artUrl(appId).then((url) => {
    if (!url || holder.dataset.app !== String(appId)) return;
    const picture = make('img');
    picture.alt = '';
    picture.decoding = 'async';
    picture.addEventListener('load', () => picture.classList.add('loaded'));
    picture.src = url;
    holder.replaceChildren(picture);
  });
  return holder;
}

/* Pages register what to do when they are shown; anything can ask to go to one. */
const pages = new Map();
let current = null;

export function page(name, node, onShow) {
  pages.set(name, { node, onShow });
}

export function go(name, ...args) {
  for (const [other, { node }] of pages) node.hidden = other !== name;
  current = name;
  for (const link of document.querySelectorAll('[data-go]')) {
    if (link.dataset.go === name) link.setAttribute('aria-current', 'page');
    else link.removeAttribute('aria-current');
  }
  el('stage').scrollTop = 0;
  pages.get(name)?.onShow?.(...args);
}

export const showing = () => current;

export function openOutside(url) {
  const opener = window.__TAURI__.opener;
  if (opener?.openUrl) opener.openUrl(url);
  else invoke('plugin:opener|open_url', { url });
}

/* Steam's language codes are its own; the ones a browser needs for hyphenation and font
   selection are not, and getting this wrong renders Chinese in a Japanese face. */
export function bcp47(steam) {
  const known = {
    schinese: 'zh-Hans',
    tchinese: 'zh-Hant',
    japanese: 'ja',
    koreana: 'ko',
    russian: 'ru',
    thai: 'th',
    brazilian: 'pt-BR',
    latam: 'es-419',
    english: 'en',
    french: 'fr',
    german: 'de',
    spanish: 'es',
    italian: 'it',
    polish: 'pl',
    turkish: 'tr',
    ukrainian: 'uk',
    czech: 'cs',
    dutch: 'nl',
    hungarian: 'hu',
    portuguese: 'pt',
    swedish: 'sv',
    danish: 'da',
    finnish: 'fi',
    norwegian: 'no',
    romanian: 'ro',
    bulgarian: 'bg',
    greek: 'el',
    vietnamese: 'vi',
    indonesian: 'id',
  };
  return known[steam] ?? '';
}
