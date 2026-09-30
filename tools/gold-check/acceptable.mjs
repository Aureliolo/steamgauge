// Drives a page of acceptability questions in headless Chrome and fails if it does not do what
// it promises: each question asks whether a reader's other subject would also do, answered yes
// or no from the keyboard or the mouse, kept, and exported with the subject it was about.
//
//   cargo run -p steamgauge-core --example sample-gold -- accept.html --acceptable
//   node tools/gold-check/acceptable.mjs accept.html
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { connect, debuggerUrl, open } from "./chrome.mjs";

const PROBE = `(function () {
  var wrong = [];
  var check = function (claim, ok) { if (!ok) { wrong.push(claim); } };
  var press = function (key) {
    document.dispatchEvent(new KeyboardEvent('keydown', { key: key, bubbles: true }));
  };
  var at = function () {
    return Number(/^(\\d+) of/.exec(document.querySelector('header.bar .count').textContent)[1]);
  };
  var data = JSON.parse(document.getElementById('data').textContent);
  var question = data.questions[0];

  check('a subject can be picked on a question that asks yes or no',
    document.querySelectorAll('button.pick').length === 0);
  check('the question does not offer yes and no',
    document.querySelectorAll('button[data-acceptable]').length === 2);
  var text = document.body.textContent;
  check('the person is not shown their own answer', text.indexOf('You said:') !== -1);
  check('the reader\\'s answer is not shown', text.indexOf('The reader said:') !== -1);
  check('the rules for both subjects are not both shown',
    document.querySelectorAll('.rules p').length === 2);
  var mark = document.querySelector('.review mark');
  check('the claim is not marked inside its review', !!mark && mark.textContent === question.claim);

  // A letter picks a subject on every other page; here it must answer nothing.
  press('s');
  check('a letter moved the page', at() === 1);
  press('y');
  check('y did not answer and move on', at() === 2);
  document.querySelector('button[data-acceptable="false"]').click();
  check('no did not answer and move on', at() === 3);

  var kept = Object.keys(localStorage).filter(function (k) { return k.indexOf('steamgauge-gold') === 0; });
  var held = JSON.parse(localStorage.getItem(kept[0]) || '{}');
  var rows = Object.keys(held).map(function (k) { return held[k]; });
  check('two answers were given and ' + rows.length + ' kept', rows.length === 2);
  var yes = rows.filter(function (row) { return row.review_id === data.questions[0].review_id; })[0] || {};
  var no = rows.filter(function (row) { return row.review_id === data.questions[1].review_id; })[0] || {};
  check('a yes was not kept as a yes', yes.acceptable === true);
  check('a no was not kept as a no', no.acceptable === false);
  check('an answer does not say which subject it was about',
    yes.offered === data.questions[0].offered && no.offered === data.questions[1].offered);

  var out = window.__exportForCheck ? window.__exportForCheck() : [];
  check('the export dropped the judgements', out.length === 2);
  check('an exported judgement does not say which sheet it answered',
    out.every(function (row) { return row.sheet === data.taxonomy; }));

  press('ArrowLeft');
  check('an answered question does not show its answer again',
    document.querySelectorAll('button[data-acceptable].chosen').length === 1);
  return wrong;
})()`;

const RELOADED = `(function () {
  var at = Number(/^(\\d+) of/.exec(document.querySelector('header.bar .count').textContent)[1]);
  return at === 3 ? [] : ['a reload starts at question ' + at + ' rather than the first unanswered one'];
})()`;

const file = resolve(process.argv[2] ?? "accept.html");
const page = pathToFileURL(file).href;
const chrome = await open(page, { prefix: "steamgauge-acceptable-check-" });

let failed = true;
try {
  const { socket, send, evaluate, ready } = await connect(await debuggerUrl(chrome.port));
  await send("Page.reload", { ignoreCache: true });
  await ready();
  const wrong = (await evaluate(PROBE)).result?.result?.value ?? ["the page could not be driven at all"];
  await send("Page.reload", { ignoreCache: true });
  await ready();
  const again = (await evaluate(RELOADED)).result?.result?.value ?? ["the page could not be read after a reload"];
  const all = [...wrong, ...again];
  if (all.length) {
    console.error(`${file}\n  ${all.join("\n  ")}`);
  } else {
    console.log(`${file}: acceptability questions keep every promise they make`);
    failed = false;
  }
  socket.close();
} finally {
  await chrome.close();
}

process.exit(failed ? 1 : 0);
