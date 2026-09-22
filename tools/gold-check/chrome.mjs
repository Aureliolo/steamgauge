// What the adjudication checks add to the shared browser: knowing when the page can be
// answered, which is not the same moment as loaded.
import { connect as attach, sleep } from "../chrome.mjs";

export { browser, debuggerUrl, open, sleep } from "../chrome.mjs";

export async function connect(url) {
  const driven = await attach(url);
  return {
    ...driven,
    // The page renders its first question from a megabyte of embedded JSON, so "loaded" is not
    // the same moment as "answerable" and every step has to wait for the second one.
    async ready() {
      for (let attempt = 0; attempt < 80; attempt += 1) {
        const there = await driven.evaluate(
          "document.readyState === 'complete' && !!document.querySelector('button.pick')",
        );
        if (there.result?.result?.value === true) return true;
        await sleep(250);
      }
      return false;
    },
  };
}
