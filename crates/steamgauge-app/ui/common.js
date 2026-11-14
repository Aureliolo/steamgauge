/* What every page of the window shares: the bridge to the core, the formats numbers are shown
   in, and the way from one page to another. */

const AFTER_PATH_SEPARATOR = /(?<=[\\/])/u;
const HAS_DIGIT = /\d/u;
const POSITIVE_VERDICT = /positive/iu;
const NEGATIVE_VERDICT = /negative/iu;

/* British English shortens September to "Sept" alone. Every month here has three letters, as
   the core writes them. */
const threeLetters = (text) => text.replace('Sept', 'Sep');

function shortMonths(format) {
  return { format: (date) => threeLetters(format.format(date)) };
}

/* A share as the core writes it (`figure_text::percent`). `Intl` does the rounding, a half up on
   the value's shortest decimal text, which is what the core rounds. Whole points sit between 10%
   and 90%, a tenth elsewhere, and a share short of 0 or 1 never reads as either. */
const percentFormats = new Map();
function percentFormat(digits) {
  if (!percentFormats.has(digits)) {
    percentFormats.set(
      digits,
      new Intl.NumberFormat(LOCALE, { maximumFractionDigits: digits, style: 'percent' }),
    );
  }
  return percentFormats.get(digits);
}

/* The text of a share and whether it reads with a tenth. The locale's decimal mark is not looked
   for, so the answer holds if the window's locale ever writes a comma. */
function written(value, tenths) {
  if (value <= 0) {
    return { text: '0%', tenth: false };
  }
  if (value >= 1) {
    return { text: '100%', tenth: false };
  }
  if (value < 0.001) {
    return { text: '<0.1%', tenth: true };
  }
  if (value > 0.999) {
    return { text: '>99.9%', tenth: true };
  }
  const format = percentFormat(!tenths && value >= 0.1 && value <= 0.9 ? 0 : 1);
  return {
    text: format.format(value),
    tenth: format.formatToParts(value).some((part) => part.type === 'fraction'),
  };
}

const missing = (value) => value === null || value === undefined || Number.isNaN(value);

let thinShown = false;

const pages = new Map();
let current = null;

/* A game's store picture. The core fetches each once and keeps it, and the window loads only a
   few at a time, so a library of seventy games does not ask for seventy at once. */
const pictures = new Map();
const waitingForArt = [];
let artInFlight = 0;
const ART_AT_ONCE = 4;

const { invoke } = globalThis.__TAURI__.core;
const { listen } = globalThis.__TAURI__.event;

function loadArt({ appId, settle }) {
  artInFlight += 1;
  invoke('art', { appId })
    .then((bytes) => settle(URL.createObjectURL(new Blob([bytes]))))
    .catch(() => settle(null))
    .finally(() => {
      artInFlight -= 1;
      nextArt();
    });
}

function nextArt() {
  while (artInFlight < ART_AT_ONCE && waitingForArt.length > 0) {
    loadArt(waitingForArt.shift());
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

export const el = (id) => document.querySelector(`#${id}`);

/* The window's text is British English, and so are its numbers and dates: the system's own
   format would put "2'860" beside the core's "2,860" in one sentence. */
export const LOCALE = 'en-GB';

export const whole = new Intl.NumberFormat(LOCALE);
export const day = shortMonths(
  new Intl.DateTimeFormat(LOCALE, { day: 'numeric', month: 'short', year: 'numeric' }),
);
export const when = shortMonths(
  new Intl.DateTimeFormat(LOCALE, {
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    month: 'short',
  }),
);

// A cell with no number behind it, drawn as the report draws one.
export const nothing = '–';

/* A share the window is not sent, or one that is not a number, reads as a dash and never as 0%. */
export const share = {
  format: (value) => (missing(value) ? nothing : written(value, false).text),
};

/* A rate with the two ends of its interval, all at one precision as `figure_text::percent_range`
   writes them. That is a tenth where any of the three reads with one, or where whole points make
   the ends read alike. Where tenths read alike too, the interval says nothing the rate beside it
   does not, and the ends are null. The rate is always the text returned here. */
export function shareRange(rate, low, high) {
  if (missing(rate) || missing(low) || missing(high)) {
    return [share.format(rate), null];
  }
  const ends = (forced) => [written(low, forced).text, written(high, forced).text];
  const [firstWhole, secondWhole] = ends(false);
  const tenths =
    [rate, low, high].some((value) => written(value, false).tenth) || firstWhole === secondWhole;
  const [first, second] = ends(tenths);
  return first === second
    ? [share.format(rate), null]
    : [written(rate, tenths).text, [first, second]];
}

/* What each figure of the reader's accuracy measures is written on this page. */
export const ACCURACY_PAGE = 'https://aureliolo.github.io/steamgauge/accuracy.html';

/* Below this many reviews raising a subject, how they split between praise and complaint is a
   handful of opinions, so it is withheld unless the person asks for it. The core's
   `ENOUGH_FOR_A_RATE`. */
export const ENOUGH_TO_LEAN = 30;

/* Whether the person asked to see splits below the floor: one choice for every page that offers
   it, so turning it on in one does not leave another saying something else. */
export const thin = {
  get shown() {
    return thinShown;
  },
  set shown(value) {
    thinShown = value;
  },
};

export function set(node, text) {
  node.textContent = text;
}

/* Text from the core with its commands in backticks, the commands set in code type. */
export function setWithCode(node, text) {
  node.replaceChildren(
    ...text
      .split('`')
      .map((part, at) => (at % 2 === 1 ? make('code', null, part) : part))
      .filter((part) => part !== ''),
  );
}

/* Builds an element from a tag, a class and children. Text is always set as text, never parsed:
   game names and reviews come from Steam. */
export function make(tag, className, ...children) {
  const node = document.createElement(tag);
  if (className) {
    node.className = className;
  }
  for (const child of children) {
    if (child === null || child === undefined || child === false) {
      continue;
    }
    node.append(child instanceof Node ? child : String(child));
  }
  return node;
}

/* A folder shown so a line breaks after a separator, where a reader expects it, and nowhere
   inside a name unless one name is wider than the line. */
export function setPath(node, path) {
  node.replaceChildren(
    ...path
      .split(AFTER_PATH_SEPARATOR)
      .flatMap((part, index) => (index === 0 ? [part] : [document.createElement('wbr'), part])),
  );
}

export function button(label, className, onClick) {
  const node = make('button', className, label);
  node.type = 'button';
  node.addEventListener('click', onClick);
  return node;
}

/* Each unit takes over where the one below would round to a thousand, so nothing reads "1000 KB". */
export function size(bytes) {
  if (bytes >= 999.95e9) {
    return `${(bytes / 1e12).toFixed(2)} TB`;
  }
  if (bytes >= 999.5e6) {
    return `${(bytes / 1e9).toFixed(1)} GB`;
  }
  if (bytes >= 999.5e3) {
    return `${Math.round(bytes / 1e6)} MB`;
  }
  return bytes > 0 ? `${Math.max(1, Math.round(bytes / 1e3))} KB` : '0 KB';
}

/* An estimate as a sentence says one: "about 3 minutes", but never "about under a minute". */
export function roughly(seconds) {
  return seconds < 60 ? duration(seconds) : `about ${duration(seconds)}`;
}

export function duration(seconds) {
  if (seconds < 60) {
    return 'under a minute';
  }
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) {
    return minutes === 1 ? 'a minute' : `${minutes} minutes`;
  }
  return `${Math.round(seconds / 360) / 10} hours`;
}

/* Time left, said the way a person reads a clock rather than a stopwatch. */
export function left(seconds) {
  if (seconds < 10) {
    return 'a few seconds left';
  }
  if (seconds < 60) {
    return `${Math.round(seconds / 5) * 5} s left`;
  }
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
  if (!(year && month)) {
    return label;
  }
  return threeLetters(
    new Date(Date.UTC(year, month - 1, 15)).toLocaleDateString(LOCALE, {
      month: 'short',
      timeZone: 'UTC',
      year: 'numeric',
    }),
  );
}

export function facts(list, entries) {
  list.replaceChildren();
  for (const [term, value, under] of entries) {
    const dd = make('dd', null, value);
    if (under) {
      dd.append(make('small', null, under));
    }
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
        make('dd', HAS_DIGIT.test(value) ? 'tile-value' : 'tile-value word', value),
        under ? make('dd', 'tile-under', under) : null,
      ),
    ),
  );
}

/* Steam's verdict as a pill coloured by which way it leans. */
export function verdictPill(verdict) {
  if (!verdict) {
    return null;
  }
  let leaning = 'warn';
  if (POSITIVE_VERDICT.test(verdict)) {
    leaning = 'good';
  } else if (NEGATIVE_VERDICT.test(verdict)) {
    leaning = 'bad';
  }
  return make('span', `pill ${leaning}`, verdict);
}

/* The picture's frame, filled in when the picture arrives. A game with none keeps the frame. */
export function art(appId, holder = make('span', 'art')) {
  holder.replaceChildren();
  holder.dataset.app = String(appId);
  artUrl(appId).then((url) => {
    if (!url || holder.dataset.app !== String(appId)) {
      return;
    }
    const picture = make('img');
    picture.alt = '';
    picture.decoding = 'async';
    picture.addEventListener('load', () => picture.classList.add('loaded'));
    picture.src = url;
    holder.replaceChildren(picture);
  });
  return holder;
}

/* Pages register what to do when they are shown. Anything can ask to go to one. A page with no
   place of its own in the sidebar, a game's, names the one it is reached from, which stays marked
   while it is open. */
export function page(name, node, onShow, under = name) {
  pages.set(name, { node, onShow, under });
}

export function go(name, ...args) {
  for (const [other, { node }] of pages) {
    node.hidden = other !== name;
  }
  current = name;
  const marked = pages.get(name)?.under ?? name;
  /* Only the rail's list marks where the person is. The name above it and the links on a page
     are ways in, and lit up too they would read as a second place. */
  for (const link of document.querySelectorAll('.nav [data-go]')) {
    if (link.dataset.go === marked) {
      link.setAttribute('aria-current', marked === name ? 'page' : 'true');
    } else {
      link.removeAttribute('aria-current');
    }
  }
  el('stage').scrollTop = 0;
  pages.get(name)?.onShow?.(...args);
}

export const showing = () => current;

/* Each button under `root` that names an element in data-copies copies that element's text. */
export function copiers(root) {
  for (const copier of root.querySelectorAll('[data-copies]')) {
    copier.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(el(copier.dataset.copies).textContent);
        set(copier, 'Copied');
      } catch {
        set(copier, 'Select it to copy');
      }
      setTimeout(() => set(copier, 'Copy'), 2000);
    });
  }
}

export function openOutside(url) {
  const { opener } = globalThis.__TAURI__;
  if (opener?.openUrl) {
    opener.openUrl(url);
  } else {
    invoke('plugin:opener|open_url', { url });
  }
}

/* The way to the accuracy page from a figure that says how well the reader agrees. */
export function accuracyLink(label = 'Accuracy') {
  const link = make('a', 'accuracy-link', label);
  link.href = ACCURACY_PAGE;
  link.addEventListener('click', (event) => {
    event.preventDefault();
    openOutside(ACCURACY_PAGE);
  });
  return link;
}

/* A reader's role as a sentence, with the way to the accuracy page after the ones that state an
   agreement. `after` is whatever else the line says. */
export function roleLine(role, after = '') {
  return [
    `${role.charAt(0).toUpperCase()}${role.slice(1)}.${after}`,
    ...(role.includes('agreement') ? [' ', accuracyLink()] : []),
  ];
}

/* Steam's language codes are its own. The ones a browser needs for hyphenation and font
   selection are not, and getting this wrong renders Chinese in a Japanese face. */
export function bcp47(steam) {
  const known = {
    brazilian: 'pt-BR',
    bulgarian: 'bg',
    czech: 'cs',
    danish: 'da',
    dutch: 'nl',
    english: 'en',
    finnish: 'fi',
    french: 'fr',
    german: 'de',
    greek: 'el',
    hungarian: 'hu',
    indonesian: 'id',
    italian: 'it',
    japanese: 'ja',
    koreana: 'ko',
    latam: 'es-419',
    norwegian: 'no',
    polish: 'pl',
    portuguese: 'pt',
    romanian: 'ro',
    russian: 'ru',
    schinese: 'zh-Hans',
    spanish: 'es',
    swedish: 'sv',
    tchinese: 'zh-Hant',
    thai: 'th',
    turkish: 'tr',
    ukrainian: 'uk',
    vietnamese: 'vi',
  };
  return known[steam] ?? '';
}

export { invoke, listen };
