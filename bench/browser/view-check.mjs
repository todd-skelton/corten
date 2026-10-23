// Checks corten/view's rows() in a real DOM against random edits: after every replace, push,
// remove, swap or clear, the children must show exactly the items, in order, keep their elements
// for kept keys, and at(i) / of(target) must give the view of the right row. node view-check.mjs
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const source = readFileSync(join(here, "..", "..", "crates", "corten", "js", "view.js"), "utf8");
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
await page.setContent('<ul id="l"></ul><template id="t"><li><b> </b>:<i> </i></li></template>');
const result = await page.evaluate(async (src) => {
  const mod = await import(URL.createObjectURL(new Blob([src], { type: "text/javascript" })));
  const ul = document.getElementById("l");
  const proto = document.getElementById("t").content.firstElementChild;
  const k = (el) => el.firstChild.firstChild;
  const v = (el) => el.lastChild.firstChild;
  // A stand-in for what html! generates: one property per hole.
  class Item {
    constructor(el) { this.el = el; }
    get k() { return k(this.el).data; }
    get v() { return v(this.el).data; }
    set v(value) { v(this.el).data = value; }
    static make(d) { const el = proto.cloneNode(true); Item.fill(el, d); return el; }
    static fill(el, d) { k(el).data = String(d.k); v(el).data = String(d.v); }
    static key(el) { return k(el).data; }
    static keyOf(d) { return String(d.k); }
  }
  let seed = 7;
  const rnd = (n) => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) >>> 8) % n;
  const fresh = (n) => Array.from({ length: n }, () => ({ k: nextKey++, v: 0 }));
  let failures = 0;
  const notes = [];
  const fail = (step, why) => { if (failures++ < 5) notes.push(step + " kind " + lastKind + ": " + why); };
  let lastKind = "";
  let nextKey = 0;
  let items = [];
  let l;
  // Every 300 steps the rows start over from server-rendered elements.
  const start = () => {
    items = fresh(rnd(40));
    ul.innerHTML = items.map((x) => `<li><b>${x.k}</b>:<i>${x.v}</i></li>`).join("");
    l = mod.rows(ul, Item);
  };
  for (let step = 0; step < 3000; step++) {
    if (step % 300 === 0) start();
    const before = new Map(Array.from(ul.children, (el, i) => [items[i].k, el]));
    let next = items.slice();
    const kind = rnd(10);
    lastKind = String(kind);
    if (kind <= 5) {
      // replace with: new list, inserts, deletes, shuffle, swap, or changed values
      const how = rnd(6);
      if (how === 0) next = fresh(rnd(30));
      else if (how === 1) for (let i = rnd(5); i > 0; i--) next.splice(rnd(next.length + 1), 0, ...fresh(1));
      else if (how === 2) for (let i = rnd(5); i > 0 && next.length; i--) next.splice(rnd(next.length), 1);
      else if (how === 3) for (let i = next.length - 1; i > 0; i--) { const j = rnd(i + 1); [next[i], next[j]] = [next[j], next[i]]; }
      else if (how === 4) next.reverse();
      else next = next.map((x) => (rnd(3) ? x : { ...x, v: x.v + 1 }));
      l.replace(next);
    } else if (kind === 6) {
      const add = fresh(rnd(4));
      l.push(add);
      next.push(...add);
    } else if (kind === 7 && next.length) {
      const i = rnd(next.length);
      const view = l.of(ul.children[i].lastChild.firstChild);
      if (!view || view.el !== ul.children[i]) fail(step, "of() found the wrong row");
      l.remove(view);
      next.splice(i, 1);
    } else if (kind === 8 && next.length > 1) {
      const i = rnd(next.length), j = rnd(next.length);
      l.swap(i, j);
      [next[i], next[j]] = [next[j], next[i]];
    } else if (kind === 9 && next.length) {
      const i = rnd(next.length);
      next[i] = { ...next[i], v: next[i].v + 1 };
      l.at(i).v = String(next[i].v);
    } else if (rnd(20) === 0) {
      l.clear();
      next = [];
    }
    const kids = Array.from(ul.children);
    if (l.length !== next.length || kids.length !== next.length) fail(step, "length");
    kids.forEach((el, i) => {
      if (el.textContent !== next[i].k + ":" + next[i].v) fail(step, "content at " + i);
      if (before.has(next[i].k) && before.get(next[i].k) !== el) fail(step, "element not kept for " + next[i].k);
      if (l.at(i).el !== el || l.of(el.firstChild)?.el !== el) fail(step, "at/of at " + i);
    });
    const seen = Array.from(l, (view) => view.el);
    if (seen.length !== kids.length || seen.some((el, i) => el !== kids[i])) fail(step, "iterator");
    items = next;
  }
  return { failures, notes };
}, source);
await browser.close();
for (const n of result.notes) console.log(n);
console.log(`3000 random view edits: ${result.failures} failures`);
process.exit(result.failures ? 1 : 0);
