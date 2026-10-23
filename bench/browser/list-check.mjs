// Checks list() in a real DOM against random edits: after every set(), the children must show
// exactly the items, in order, keep their elements for kept keys, and map to their items both
// ways (item(), node()), including lists that start from server-rendered elements. node list-check.mjs
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const source = readFileSync(join(here, "apps", "shared", "state.js"), "utf8");
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
await page.setContent('<ul id="l"></ul><template id="t"><li> </li></template>');
const result = await page.evaluate(async (src) => {
  const mod = await import(URL.createObjectURL(new Blob([src], { type: "text/javascript" })));
  const ul = document.getElementById("l");
  const tpl = mod.template(document.getElementById("t"), (li) => [li.firstChild], (h, item, before) => {
    if (item.v !== before?.v) h[0].data = item.k + ":" + item.v;
  });
  let seed = 1;
  const rnd = (n) => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) >>> 8) % n;
  let failures = 0;
  // Every 300 steps the list starts over from server-rendered elements (adopted, not made), and
  // item() must find each one's item before and after edits.
  let items = [];
  let nextKey = 0;
  let l;
  const start = () => {
    items = Array.from({ length: rnd(40) }, () => ({ k: nextKey++, v: 0 }));
    ul.innerHTML = items.map((x) => `<li>${x.k}:${x.v}</li>`).join("");
    l = mod.list(ul, tpl, (x) => x.k, items);
    if (!Array.from(ul.children).every((el, i) => l.item(el.firstChild) === items[i])) failures++;
  };
  for (let step = 0; step < 3000; step++) {
    if (step % 300 === 0) start();
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
    const ok = kids.length === next.length && kids.every((el, i) => el.textContent === next[i].k + ":" + next[i].v && (!before.has(next[i].k) || before.get(next[i].k) === el) && l.item(el) === next[i] && l.node(next[i].k) === el);
    if (!ok) failures++;
    items = next;
  }
  return { failures, last: items.length };
}, source);
await browser.close();
console.log(`3000 random edits: ${result.failures} failures`);
process.exit(result.failures ? 1 : 0);
