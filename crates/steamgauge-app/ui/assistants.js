/* The AI assistants that steer SteamGauge: the ones on this computer it adds itself to, what to
   give any other, and the HTTP server for an assistant that cannot start a program. */

import { invoke, el, set, make, page, copiers } from './common.js';

let shown = null;

/* Where an MCP client stands with this copy, in one line. */
function clientState(client) {
  switch (client.state) {
    case 'installed':
      return 'Not added.';
    case 'added':
      return client.then ? `Starts SteamGauge. ${client.then}` : 'Starts SteamGauge.';
    case 'elsewhere':
      return `Starts ${client.command}, which is not this copy.`;
    case 'unreadable':
      return `Its settings file does not parse (${client.reason}). Fix the file, then add SteamGauge.`;
    default:
      return '';
  }
}

function clientRow(client, act) {
  const button = (label, command) => {
    const pressed = make('button', 'quiet-button small', label);
    pressed.type = 'button';
    pressed.setAttribute('aria-label', `${label}: ${client.name}`);
    pressed.addEventListener('click', () => act(command, client));
    return pressed;
  };
  const actions = make('div', 'actions');
  if (client.state === 'installed') actions.append(button('Add', 'connect_client'));
  if (client.state === 'elsewhere') actions.append(button('Start this copy', 'connect_client'));
  if (client.state === 'added' || client.state === 'elsewhere') actions.append(button('Remove', 'disconnect_client'));
  return make('li', 'mcp-client', make('strong', null, client.name), make('span', null, clientState(client)), make('span', null, client.file), actions);
}

function say(text, bad = false) {
  el('assistants-note').classList.toggle('bad', bad);
  set(el('assistants-note'), text);
}

function draw() {
  el('mcp-clients').replaceChildren(...shown.clients.map((client) => clientRow(client, steerFrom)));
  el('mcp-clients').hidden = shown.clients.length === 0;
  el('mcp-clients-none').hidden = shown.clients.length > 0;
  set(el('mcp-paste'), shown.paste);
  set(el('mcp-command'), shown.mcp_command);
  el('answer-over-http').checked = shown.answer_over_http;
  el('http-port').value = String(shown.http_port);
  el('http-reach').hidden = !shown.http.address;
  set(el('http-address'), shown.http.address ?? '');
  set(el('http-token'), shown.http.token ?? '');
  el('http-problem').hidden = !shown.http.problem;
  set(el('http-problem'), shown.http.problem ?? '');
}

async function steerFrom(command, client) {
  try {
    shown = await invoke(command, { client: client.id });
    draw();
    say(
      command === 'connect_client'
        ? `Added to ${client.name}. ${client.then ?? 'It reaches SteamGauge from its next session, while this window is open.'}`
        : `Taken out of ${client.name}.`,
    );
  } catch (failure) {
    say(String(failure), true);
  }
}

async function load() {
  try {
    shown = await invoke('settings');
  } catch (failure) {
    say(String(failure), true);
    return;
  }
  draw();
}

/* Only the two choices on this page change. Every other setting is saved as it stands now, read
   afresh, so a choice made on the Settings page or by an assistant since this page was drawn is
   kept. */
async function save() {
  try {
    const now = await invoke('settings');
    const { shares, check_hours, library, clients, mcp_command, paste, http, search_every_game, ...settings } = now;
    settings.answer_over_http = el('answer-over-http').checked;
    settings.http_port = Number.parseInt(el('http-port').value, 10) || now.http_port;
    shown = await invoke('save_settings', { settings, searchEveryGame: search_every_game });
    draw();
    say('Saved.');
  } catch (failure) {
    say(String(failure), true);
  }
}

export function setUpAssistants() {
  page('assistants', el('assistants'), load);
  copiers(el('assistants'));
  el('assistants-form').addEventListener('change', save);
  el('new-http-token').addEventListener('click', async () => {
    try {
      shown = await invoke('new_http_token');
      draw();
      say('A new token. Programs with the old one are shut out.');
    } catch (failure) {
      say(String(failure), true);
    }
  });
}
