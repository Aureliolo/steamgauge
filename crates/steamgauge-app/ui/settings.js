/* How the app runs on this computer: which reader reads every review, how much of the graphics
   card it takes, and what happens on its own. */

import { invoke, el, set, make, roundShare, size, page } from './common.js';
import { showNewer } from './newer.js';

let shown = null;
let options = null;

const SIZE_TEXT = {
  small: ['Small reader', 'Fast, and a little less accurate.'],
  standard: ['Standard reader', 'The most accurate.'],
};

/* Card memory is sold in binary gigabytes: a 24 GB card reports 25.8 decimal ones. */
const gib = (bytes) => `${Math.round(bytes / 2 ** 30)} GB`;

/* Why this computer is recommended the reader it is, in one line. */
export function reason(found) {
  const recommended = found.sizes.find((one) => one.name === found.recommended);
  const largest = found.sizes[found.sizes.length - 1];
  if (found.on_processor) {
    return found.reaches_card
      ? 'No graphics card the reader can use was found, so reading runs on the processor, where the small reader is the quickest.'
      : 'This build reads on the processor, where the small reader is the quickest.';
  }
  const card = `${found.card ?? 'The graphics card'}${found.card_bytes ? `, ${gib(found.card_bytes)}` : ''}`;
  return recommended === largest
    ? `${card}: the most accurate reader fits in its memory.`
    : `${card}: the ${SIZE_TEXT[largest.name]?.[0].toLowerCase() ?? largest.name} needs ${gib(largest.needs)} of its memory, more than it can spare.`;
}

function sizeCard(found, option) {
  const [title, about] = SIZE_TEXT[option.name] ?? [option.name, ''];
  const input = make('input');
  input.type = 'radio';
  input.name = 'reader-size';
  input.value = option.name;
  input.checked = found.reads_with === option.name;
  input.disabled = !option.runs_here || !option.published;

  const pills = make(
    'span',
    'size-pills',
    option.name === found.recommended ? make('span', 'pill accent', 'Recommended') : null,
    option.bytes_left === 0 && option.published ? make('span', 'pill good', 'On this computer') : null,
  );
  const lines = [about];
  if (!option.published) lines.push('Not published yet.');
  else if (!option.runs_here) lines.push(`Needs ${gib(option.needs)} of card memory, more than this card can spare.`);
  else if (option.bytes_left > 0) lines.push(`A ${size(option.bytes_left)} download, the first time it reads.`);
  if (found.on_processor && option.times > 1.05) lines.push(`About ${Math.round(option.times)} times as long as the small reader.`);

  const label = make(
    'label',
    `size-card${input.disabled ? ' off' : ''}`,
    input,
    make('span', 'size-body', make('span', 'size-title', make('strong', null, title), pills), make('span', 'size-about', lines.join(' '))),
  );
  return label;
}

export function setUpSettings() {
  page('settings', el('settings'), load);
  el('settings-form').addEventListener('change', save);

  for (const copier of el('settings').querySelectorAll('[data-copies]')) {
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

  el('new-http-token').addEventListener('click', async () => {
    try {
      shown = await invoke('new_http_token');
      draw();
      el('settings-note').classList.remove('bad');
      set(el('settings-note'), 'A new token. Programs with the old one are shut out.');
    } catch (failure) {
      el('settings-note').classList.add('bad');
      set(el('settings-note'), String(failure));
    }
  });

  async function load() {
    try {
      [shown, options] = await Promise.all([invoke('settings'), invoke('reader_options')]);
    } catch (failure) {
      set(el('settings-note'), String(failure));
      return;
    }
    draw();
  }

  function draw() {
    const shares = el('gpu-share');
    shares.replaceChildren(
      ...shown.shares.map((value) => {
        const input = make('input');
        input.type = 'radio';
        input.name = 'gpu-share';
        input.value = String(value);
        input.checked = Math.abs(value - shown.gpu_share) < 1e-9;
        return make('label', 'option', input, value === 1 ? 'All of it' : roundShare.format(value));
      }),
    );
    el('gpu-setting').hidden = options.on_processor;

    set(el('reader-machine'), reason(options));
    el('reader-sizes').replaceChildren(...options.sizes.map((option) => sizeCard(options, option)));

    el('first-language').value = shown.language ?? '';
    el('read-after-download').checked = shown.read_after_download;
    el('search-every-game').checked = shown.search_every_game;
    el('check-steam').checked = shown.check_steam;
    el('keep-up-to-date').checked = shown.keep_up_to_date;
    /* Without Steam's counts there is nothing to tell which games have new reviews. */
    el('keep-up-to-date').disabled = !shown.check_steam;
    el('notify-moves').checked = shown.notify_moves;
    el('check-newer-version').checked = shown.check_newer_version;

    set(el('claude-command'), shown.claude_command);
    el('answer-over-http').checked = shown.answer_over_http;
    el('http-port').value = String(shown.http_port);
    el('http-reach').hidden = !shown.http.address;
    set(el('http-address'), shown.http.address ?? '');
    set(el('http-token'), shown.http.token ?? '');
    el('http-problem').hidden = !shown.http.problem;
    set(el('http-problem'), shown.http.problem ?? '');
  }

  async function save() {
    const picked = el('gpu-share').querySelector('input:checked');
    const reader = el('reader-sizes').querySelector('input:checked')?.value ?? null;
    const settings = {
      gpu_share: picked ? Number(picked.value) : shown.gpu_share,
      /* The recommended size is kept as no choice at all, so a better card later is taken up by
         itself. */
      reader: reader === options.recommended ? null : reader,
      language: el('first-language').value || null,
      check_steam: el('check-steam').checked,
      keep_up_to_date: el('keep-up-to-date').checked,
      notify_moves: el('notify-moves').checked,
      read_after_download: el('read-after-download').checked,
      check_newer_version: el('check-newer-version').checked,
      answer_over_http: el('answer-over-http').checked,
      http_port: Number.parseInt(el('http-port').value, 10) || shown.http_port,
    };
    try {
      shown = await invoke('save_settings', { settings, searchEveryGame: el('search-every-game').checked });
      options = await invoke('reader_options');
      draw();
      el('settings-note').classList.remove('bad');
      set(el('settings-note'), 'Saved. Work already running keeps the settings it started with.');
      showNewer();
    } catch (failure) {
      el('settings-note').classList.add('bad');
      set(el('settings-note'), String(failure));
    }
  }
}
