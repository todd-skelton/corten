// Checks list() in a real DOM against random edits: after every set(), the children must show
// exactly the items, in order, and keep their elements for kept keys. node list-check.mjs
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const source = readFileSync(join(here, "..", "..", "crates", "corten", "js", "state.js"), "utf8");
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
await page.setContent('<ul id="l"></ul><template id="t"><li> </li></template>');
const result = await page.evaluate(async (src) => {
  const mod = await import(URL.createObjectURL(new Blob([src], { type: "text/javascript" })));
  const ul = document.getElementById("l");
  const tpl = mod.template(document.getElementById("t"), (li) => [li.firstChild], (h, item, before) => {
    if (item.v !== before?.v) h[0].data = item.k + ":" + item.v;
  });
  const l = mod.list(ul, tpl, (x) => x.k);
  let seed = 1;
  const rnd = (n) => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) >>> 8) % n;
  let items = [];
  let nextKey = 0;
  let failures = 0;
  for (let step = 0; step < 3000; step++) {
    const before = new Map(Array.from(ul.children, (el, i) => [items[i]?.k, el]));
    let next = items.slice();
    const kind = rnd(7);
    if (kind === 0) next = Array.from({ length: rnd(30) }, () => ({ k: nextKey++, v: 0 }));
    else if (kind === 1) for (let i = rnd(5); i > 0; i--) next.splice(rnd(next.length + 1), 0, { k: nextKey++, v: 0 });
    else if (kind === 2) for (let i = rnd(5); i > 0 && next.length; i--) next.splice(rnd(next.length), 1);
    else if (kind === 3) for (let i = next.length - 1; i > 0; i--) { const j = rnd(i + 1); [next[i], next[j]] = [next[j], next[i]]; }
    else if (kind === 4 && next.length > 1) { const a = rnd(next.length), b = rnd(next.length); [next[a], next[b]] = [next[b], next[a]]; }
    else if (kind === 5) next = next.map((x) => (rnd(3) ? x : { ...x, v: x.v + 1 }));
    else next.reverse();
    l.set(next);
    const kids = Array.from(ul.children);
    const ok = kids.length === next.length && kids.every((el, i) => el.textContent === next[i].k + ":" + next[i].v && (!before.has(next[i].k) || before.get(next[i].k) === el));
    if (!ok) failures++;
    items = next;
  }
  return { failures, last: items.length };
}, source);
await browser.close();
console.log(`3000 random edits: ${result.failures} failures`);
process.exit(result.failures ? 1 : 0);
