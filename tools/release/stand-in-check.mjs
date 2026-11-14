// Refuses a program that sends its requests to the origin STEAMGAUGE_STAND_IN names, which only
// a debug build may do.
//
//   node tools/release/stand-in-check.mjs <program>
//
// A search of the binary for the name cannot settle it. On Linux the compiler builds a short name
// from immediate stores, so the bytes of a name the program reads need not appear in a row. So the
// program is run instead: `update`, with the variable naming a listener here and every other host
// behind a proxy that is not there. A request the listener hears is a program that reads it.
//
// A first run sends `update` through a proxy that is a second listener. Hearing nothing there means
// the check could not see the program's requests at all, and it says so rather than passing.
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import process from 'node:process';
import { isMain } from './github.mjs';

/** A port on this machine nothing listens on, so a request sent there fails at once. */
const NOWHERE = 'http://127.0.0.1:9';

/** How long one run of the program may take before it is stopped. */
const LIMIT_MS = 60_000;

/** The proxy variables a program's HTTP client reads, in both cases. */
const PROXIES = [
  'HTTP_PROXY',
  'HTTPS_PROXY',
  'ALL_PROXY',
  'http_proxy',
  'https_proxy',
  'all_proxy',
];

/** A listener on 127.0.0.1 that counts the connections made to it and closes each. */
function listener() {
  return new Promise((resolve) => {
    let heard = 0;
    const server = createServer((socket) => {
      heard += 1;
      socket.destroy();
    });
    server.listen(0, '127.0.0.1', () => {
      resolve({
        origin: `http://127.0.0.1:${server.address().port}`,
        heard: () => heard,
        close: () => new Promise((closed) => server.close(closed)),
      });
    });
  });
}

/** `command` with `update` after it, in an environment of this one with `changes` applied. */
function ran(command, changes) {
  const environment = { ...process.env };
  for (const name of [...PROXIES, 'NO_PROXY', 'no_proxy', 'STEAMGAUGE_STAND_IN']) {
    delete environment[name];
  }
  Object.assign(environment, changes);
  const [program, ...args] = command;
  return new Promise((resolve, reject) => {
    const child = spawn(program, [...args, 'update'], {
      env: environment,
      stdio: 'ignore',
      timeout: LIMIT_MS,
      windowsHide: true,
    });
    child.on('error', reject);
    child.on('exit', () => resolve());
  });
}

/** Every proxy variable set to `origin`. */
function behind(origin) {
  return Object.fromEntries(PROXIES.map((name) => [name, origin]));
}

/**
 * What the listeners heard from `command`: `seen` from its run behind a listening proxy, `standIn`
 * from its run with the variable naming a listener and every other host behind a proxy that is
 * not there.
 */
async function heardFrom(command) {
  const proxy = await listener();
  await ran(command, behind(proxy.origin));
  await proxy.close();

  const standIn = await listener();
  await ran(command, {
    ...behind(NOWHERE),
    NO_PROXY: '127.0.0.1',
    no_proxy: '127.0.0.1',
    STEAMGAUGE_STAND_IN: standIn.origin,
  });
  await standIn.close();
  return { seen: proxy.heard(), standIn: standIn.heard() };
}

if (isMain(import.meta.url)) {
  const [program, ...rest] = process.argv.slice(2);
  if (program === undefined || rest.length > 0) {
    console.error('usage: stand-in-check.mjs <program>');
    process.exitCode = 2;
  } else {
    const { seen, standIn } = await heardFrom([program]);
    if (seen === 0) {
      console.error(
        `${program} sent no request through the proxy, so this cannot tell what it reads.`,
      );
      process.exitCode = 2;
    } else if (standIn > 0) {
      console.error(
        `${program} asked the origin STEAMGAUGE_STAND_IN names, which only a debug build may.`,
      );
      process.exitCode = 1;
    } else {
      console.log(`${program} ignores STEAMGAUGE_STAND_IN.`);
    }
  }
}

export { heardFrom };
