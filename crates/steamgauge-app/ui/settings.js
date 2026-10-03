/* How the app runs on this computer: how much of the graphics card it takes, which reader a
   computer without one uses, and what happens on its own. */

import { invoke, el, set, make, roundShare, size, page } from './common.js';

let shown = null;

export function setUpSettings() {
  page('settings', el('settings'), load);

  el('settings-form').addEventListener('change', save);

  async function load() {
    let offer;
    try {
      [shown, offer] = await Promise.all([invoke('settings'), invoke('read_offer', { appId: null, language: null })]);
    } catch (failure) {
      set(el('settings-note'), String(failure));
      return;
    }
    draw(offer);
  }

  function draw(offer) {
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

    /* Only where the computer reads on its processor is the size a person's choice: on a card,
       the card's memory decides. */
    el('reader-setting').hidden = offer.choices.length < 2;
    el('reader-choice').replaceChildren(
      ...offer.choices.map((choice, at) => {
        const option = make(
          'option',
          null,
          (at === 0 ? 'The fast reader' : 'The most accurate reader') +
            (at === 0 ? '' : `, about ${Math.round(choice.times)} times as long`),
        );
        option.value = choice.name;
        option.selected = (shown.reader ?? offer.choices[0].name) === choice.name;
        return option;
      }),
    );

    el('first-language').value = shown.language ?? '';
    el('read-after-download').checked = shown.read_after_download;
    el('search-every-game').checked = shown.search_every_game;
    el('check-steam').checked = shown.check_steam;
    set(el('library-place'), shown.library);
    set(
      el('reader-download'),
      offer.here
        ? `The ${offer.reader} reader is on this computer.`
        : offer.published
          ? `The ${offer.reader} reader is a ${size(offer.download_bytes)} download, fetched the first time a game is read.`
          : `The ${offer.reader} reader has not been published yet.`,
    );
  }

  async function save() {
    const picked = el('gpu-share').querySelector('input:checked');
    const settings = {
      gpu_share: picked ? Number(picked.value) : shown.gpu_share,
      reader: el('reader-setting').hidden ? shown.reader : el('reader-choice').value || null,
      language: el('first-language').value || null,
      check_steam: el('check-steam').checked,
      read_after_download: el('read-after-download').checked,
    };
    try {
      shown = await invoke('save_settings', { settings, searchEveryGame: el('search-every-game').checked });
      set(el('settings-note'), 'Saved. Work already running keeps the settings it started with.');
    } catch (failure) {
      el('settings-note').classList.add('bad');
      set(el('settings-note'), String(failure));
    }
  }
}
