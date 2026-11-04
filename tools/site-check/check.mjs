// Holds the built documentation site to what a reader needs from it, page by page:
//
//   cargo run -p steamgauge-site -- build --out site --shots <shots> --report sample-report.html
//   node tools/site-check/check.mjs site
//
// The folder is served over HTTP from 127.0.0.1, as GitHub Pages serves it, since the pages'
// content security policy names their own origin and a file:// page has none. Each page is
// loaded in headless Chrome and must fetch nothing from another host, apply its stylesheet,
// never scroll sideways on a phone, and pass axe-core's WCAG 2.2 A and AA rules in the light
// scheme, the dark one and at a phone's width (`npm ci` in tools/ first). The sample report is
// the report check's to hold, so it is left out.

import { readdir, readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve, sep } from 'node:path';
import process from 'node:process';
import { audit } from '../accessibility.mjs';
import { connect, debuggerUrl, open, sleep } from '../chrome.mjs';

const folder = process.argv[2] && resolve(process.argv[2]);
if (!folder) {
  console.error('usage: node tools/site-check/check.mjs <built site folder>');
  process.exit(2);
}

const TYPES = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8',
  '.png': 'image/png',
  '.txt': 'text/plain; charset=utf-8',
  '.xml': 'application/xml',
};

const server = createServer(async (request, response) => {
  const path = decodeURIComponent(new URL(request.url, 'http://127.0.0.1').pathname);
  const file = normalize(join(folder, path.endsWith('/') ? `${path}index.html` : path));
  if (!file.startsWith(folder + sep)) {
    response.writeHead(403).end();
    return;
  }
  try {
    const body = await readFile(file);
    response.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' });
    response.end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((ready) => server.listen(0, '127.0.0.1', ready));
const origin = `http://127.0.0.1:${server.address().port}`;

const pages = (await readdir(folder))
  .filter((name) => name.endsWith('.html') && name !== 'sample-report.html')
  .sort();

const PROBE = `(function () {
  var wrong = [];
  if (getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() === '') {
    wrong.push('its stylesheet is not applied');
  }
  return wrong;
})()`;
const SIDEWAYS = `(function () {
  return document.documentElement.scrollWidth > window.innerWidth + 1
    ? ['the page scrolls sideways at ' + window.innerWidth + ' pixels wide, to ' +
       document.documentElement.scrollWidth]
    : [];
})()`;

const chrome = await open(`${origin}/index.html`, { prefix: 'steamgauge-site-check-' });
const wrong = [];
try {
  const { socket, send, asked, evaluate } = await connect(await debuggerUrl(chrome.port));
  await send('Network.enable', {});
  const value = (answer, page) => {
    if (answer.result?.exceptionDetails) {
      const thrown = answer.result.exceptionDetails;
      return [`${page}: the check threw: ${thrown.exception?.description ?? thrown.text}`];
    }
    return (answer.result?.result?.value ?? []).map((claim) => `${page}: ${claim}`);
  };
  for (const page of pages) {
    asked.length = 0;
    // biome-ignore lint/performance/noAwaitInLoops: the pages share one browser tab, so each is navigated, awaited and checked in turn.
    await send('Emulation.setEmulatedMedia', { media: 'screen' });
    await send('Emulation.clearDeviceMetricsOverride', {});
    await send('Page.navigate', { url: `${origin}/${page}` });
    for (let attempt = 0; attempt < 80; attempt += 1) {
      // biome-ignore lint/performance/noAwaitInLoops: a poll waits for the page to change, so each must finish before the next.
      const ready = await evaluate(
        `document.readyState === 'complete' && location.pathname === '/${page}'`,
      );
      if (ready.result?.result?.value === true) {
        break;
      }
      await sleep(250);
    }
    wrong.push(...value(await evaluate(PROBE), page));
    wrong.push(...(await audit(evaluate, `${page}, light`)));
    await send('Emulation.setDeviceMetricsOverride', {
      deviceScaleFactor: 1,
      height: 844,
      mobile: true,
      width: 390,
    });
    await sleep(200);
    wrong.push(...value(await evaluate(SIDEWAYS), page));
    wrong.push(...(await audit(evaluate, `${page}, on a phone`)));
    await send('Emulation.clearDeviceMetricsOverride', {});
    await send('Emulation.setEmulatedMedia', {
      features: [{ name: 'prefers-color-scheme', value: 'dark' }],
      media: 'screen',
    });
    await sleep(200);
    wrong.push(...(await audit(evaluate, `${page}, dark`)));
    const elsewhere = asked.filter((url) => !url.startsWith(origin));
    if (elsewhere.length > 0) {
      wrong.push(
        `${page}: fetched ${elsewhere.length} thing(s) from another host: ${elsewhere.slice(0, 5).join(', ')}`,
      );
    }
  }
  socket.close();
} finally {
  await chrome.close();
  server.close();
}

if (wrong.length === 0) {
  console.log(`the site's ${pages.length} pages behave as they say they do`);
  process.exit(0);
}
console.error(`${wrong.length} promise(s) the site does not keep:`);
for (const claim of wrong) {
  console.error(`  - ${claim}`);
}
process.exit(1);
