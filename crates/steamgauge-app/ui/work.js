/* The work board: every download, update, read, preparation and report, as the core runs them.
   The core sends the whole board whenever a job moves, so every page that shows work draws from
   the same list and nothing is lost by leaving a page. */

import { invoke, listen, make, button, whole, size, left, language } from './common.js';

let board = [];
const drawers = new Set();

export const jobs = () => board;

export const active = (job) => job.state === 'queued' || job.state === 'running';

/* Jobs about one game, newest first. */
export const jobsFor = (appId) => board.filter((job) => job.task.app_id === appId).reverse();

export function onWork(draw) {
  drawers.add(draw);
  draw(board);
}

function drawAll() {
  for (const draw of drawers) draw(board);
}

export async function startWork() {
  await listen('work', ({ payload }) => {
    board = payload;
    drawAll();
  });
  board = await invoke('work');
  drawAll();
}

export const queue = (...tasks) => invoke('queue', { tasks });
export const stop = (id) => invoke('stop_job', { id });

export function taskName(task) {
  switch (task.kind) {
    case 'download':
      return 'Download every review';
    case 'update':
      return 'Bring up to date';
    case 'read':
      return task.language ? `Read the ${language(task.language)} reviews` : 'Read every review';
    case 'prepare':
      return 'Prepare for search by meaning';
    case 'check':
      return 'Check against Steam';
    case 'export':
      return 'Save a report';
    default:
      return task.kind;
  }
}

function amount(job) {
  const count = (value) => {
    switch (job.unit) {
      case 'bytes':
        return size(value);
      case 'pages':
        return `${whole.format(Math.round(value))} pages`;
      case 'games':
        return `${whole.format(Math.round(value))} games`;
      case 'points':
        return `${whole.format(Math.round(value))} points`;
      default:
        return `${whole.format(Math.round(value))} reviews`;
    }
  };
  if (job.total === null) return job.done > 0 ? count(job.done) : '';
  if (job.unit === 'bytes') return `${size(job.done)} of ${size(job.total)}`;
  return `${whole.format(Math.round(job.done))} of ${count(job.total)}`;
}

function speed(job) {
  if (job.rate === null || job.rate <= 0) return '';
  switch (job.unit) {
    case 'bytes':
      return `${size(job.rate)}/s`;
    case 'pages':
      return `${job.rate.toFixed(1)} pages/s`;
    case 'games':
      return '';
    default:
      return `${whole.format(Math.round(job.rate))}/s`;
  }
}

/* One job, drawn whole: what it is, what it is doing, how far, how fast, how long, and what can
   be done about it. */
export function jobItem(job, { named = true } = {}) {
  const item = make('li', `job ${job.state}`);
  const state = {
    done: ['Done', 'good'],
    failed: ['Failed', 'bad'],
    stopped: ['Stopped', ''],
    running: ['Running', 'accent'],
  }[job.state];
  const head = make(
    'div',
    'job-head',
    named ? make('span', 'job-name', job.name) : null,
    make('span', 'job-kind', taskName(job.task)),
    state ? make('span', `pill ${state[1]}`, state[0]) : null,
  );
  const actions = make('span', 'job-actions');
  if (job.state === 'running') {
    actions.append(button('Stop', 'quiet-button small', () => stop(job.id)));
  } else if (job.state === 'queued') {
    /* Waiting is one line: what it is, that it waits, and the way off the list. */
    actions.append(make('span', 'job-wait', 'Waiting its turn'), button('Take off the list', 'ghost small', () => stop(job.id)));
  } else if (job.state === 'done' && job.task.kind === 'export') {
    actions.append(button('Open', 'quiet-button small', () => invoke('open_report', { id: job.id })));
  }
  head.append(actions);
  item.append(head);

  if (job.state === 'running') {
    const figures = [amount(job), speed(job), job.left === null ? '' : left(job.left)].filter(Boolean);
    item.append(make('p', 'work-line', make('span', null, job.step), make('span', 'num', figures.join(' · '))));
    const fill = make('span', 'fill');
    if (job.total === null || job.total <= 0) {
      /* No total to divide by: a bar that invents a denominator is a bar that lies. */
      fill.classList.add('working');
      fill.style.width = '100%';
    } else {
      fill.style.width = `${Math.min(100, (100 * job.done) / job.total).toFixed(1)}%`;
    }
    item.append(make('div', 'track', fill));
  }
  if (job.note) item.append(make('p', job.state === 'failed' ? 'job-note bad' : 'job-note', job.note));
  return item;
}

/* The board in brief, for the foot of the rail: what is running and how much is waiting. */
export function drawBrief(holder) {
  onWork((all) => {
    const running = all.filter((job) => job.state === 'running');
    const waiting = all.filter((job) => job.state === 'queued').length;
    holder.replaceChildren();
    holder.hidden = running.length === 0 && waiting === 0;
    for (const job of running) {
      const fill = make('span', 'fill');
      if (job.total === null || job.total <= 0) {
        fill.classList.add('working');
        fill.style.width = '100%';
      } else {
        fill.style.width = `${Math.min(100, (100 * job.done) / job.total).toFixed(1)}%`;
      }
      const figures = job.left === null ? job.step : `${job.step}, ${left(job.left)}`;
      holder.append(
        make(
          'div',
          'brief',
          make('span', 'brief-name', job.name),
          make('span', 'brief-step', figures),
          make('div', 'track', fill),
        ),
      );
    }
    if (waiting > 0) holder.append(make('p', 'brief-waiting', `${whole.format(waiting)} waiting`));
  });
}
