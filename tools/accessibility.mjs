// What a person using a screen reader, a keyboard, or eyes that need more contrast would find
// wrong with a page, as axe-core finds it against WCAG 2.2 at levels A and AA.
//
// axe-core is the one package the checks under tools/ install (`npm ci` in tools/). It is put
// into the page through the DevTools protocol rather than a script tag, so the page's own
// content security policy stays exactly what it ships with.
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const AXE = join(dirname(fileURLToPath(import.meta.url)), "node_modules", "axe-core", "axe.min.js");

const RULES = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

let source = null;

/// Every rule `where` breaks, one line each: the page or state, what is wrong, and the first
/// elements it is wrong on. `evaluate` is the connection's, which awaits and returns by value.
export async function audit(evaluate, where) {
  source ??= await readFile(AXE, "utf8").catch(() => {
    throw new Error(`${AXE} is missing: run \`npm ci\` in tools/`);
  });
  const present = await evaluate("typeof window.axe === 'object'");
  if (present.result?.result?.value !== true) await evaluate(source);
  const found = await evaluate(`axe.run(document, {
    runOnly: { type: 'tag', values: ${JSON.stringify(RULES)} },
    resultTypes: ['violations'],
  }).then(function (results) {
    return results.violations.map(function (rule) {
      var at = rule.nodes.slice(0, 3).map(function (node) { return node.target.join(' '); });
      var more = rule.nodes.length > 3 ? ' and ' + (rule.nodes.length - 3) + ' more' : '';
      return rule.help + ' (' + rule.id + ', ' + rule.impact + '): ' + at.join(', ') + more;
    });
  })`);
  if (found.result?.exceptionDetails) {
    const thrown = found.result.exceptionDetails;
    return [`${where}: the accessibility check threw: ${thrown.exception?.description ?? thrown.text}`];
  }
  return (found.result?.result?.value ?? []).map((line) => `${where}: ${line}`);
}
