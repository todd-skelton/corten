// Checks rows() (crates/corten/js/view.js, served in corten/views) in a real DOM against random
// edits: after every replace, push, remove, swap or clear, and after changes made to the
// children without it (a server patch, a sort), the children must show exactly the items, in
// order, keep their elements for kept keys, and at(i) / get(key) / of(target) / indexOf(view)
// must give the view of the right row. Rows of a view without keyOf (the server makes them)
// refuse push and replace. node view-check.mjs
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const source = readFileSync(join(here, "..", "..", "crates", "corten", "js", "view.js"), "utf8");
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
// The template's row has empty holes, as a server renders it for empty values.
await page.setContent('<ul id="l"></ul><template id="t"><li><b></b>:<i></i></li></template><ol id="s"><li><b>a</b>:<i>1</i></li></ol>');
const result = await page.evaluate(async (src) => {
  const mod = await import(URL.createObjectURL(new Blob([src], { type: "text/javascript" })));
  const ul = document.getElementById("l");
  const template = document.getElementById("t");
  // The shape #[component(view)] generates (its `text` helper included): one property per hole.
  const text = (el, value) => {
    const t = el.firstChild;
    if (t === null) el.textContent = value;
    else if (t.data !== value) t.data = value;
  };
  class Item {
    constructor(el) { this.el = el; }
    get k() { return this.el.firstElementChild.textContent; }
    set k(v) { text(this.el.firstElementChild, String(v)); }
    get v() { return this.el.firstElementChild.nextElementSibling.textContent; }
    set v(v) { text(this.el.firstElementChild.nextElementSibling, String(v)); }
    static fill(el, data) { mod.text(el.firstElementChild, String(data.k)); mod.text(el.firstElementChild.nextElementSibling, String(data.v)); }
    static key(el) { return el.firstElementChild.textContent; }
    static keyOf(data) { return String(data.k); }
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
    // One list per parent: the same object every time, following the new children.
    const again = mod.rows(ul, Item, template);
    if (l && again !== l) fail(step, "rows() made a second list for one parent");
    l = again;
  };
  let step = 0;
  for (; step < 3000; step++) {
    if (step % 300 === 0) start();
    const before = new Map(Array.from(ul.children, (el, i) => [items[i].k, el]));
    let next = items.slice();
    const kind = rnd(12);
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
    } else if (kind === 10 && next.length > 1) {
      // Without the list: a sort moves a row to the end; the list sees it on its next use.
      const i = rnd(next.length);
      ul.appendChild(ul.children[i]);
      next.push(...next.splice(i, 1));
    } else if (kind === 11) {
      // Without the list: a patch adds a server-rendered row at the start.
      const [x] = fresh(1);
      ul.insertAdjacentHTML("afterbegin", `<li><b>${x.k}</b>:<i>${x.v}</i></li>`);
      next.unshift(x);
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
      if (l.get(String(next[i].k))?.el !== el || l.indexOf(l.at(i)) !== i) fail(step, "get/indexOf at " + i);
    });
    const seen = Array.from(l, (view) => view.el);
    if (seen.length !== kids.length || seen.some((el, i) => el !== kids[i])) fail(step, "iterator");
    items = next;
  }
  // A view whose key the markup also shows elsewhere has no keyOf: its rows are the server's.
  class Served {
    constructor(el) { this.el = el; }
    static fill(el, data) { mod.text(el.lastChild, String(data.v)); }
    static key(el) { return el.firstElementChild.textContent; }
  }
  const served = mod.rows(document.getElementById("s"), Served, template);
  const refuses = (fn) => { try { fn(); return false; } catch (e) { return /server makes these rows/.test(String(e)); } };
  if (served.get("a")?.el !== document.querySelector("#s li")) fail(step, "get on server-made rows");
  if (!refuses(() => served.push([{ v: 2 }])) || !refuses(() => served.replace([]))) fail(step, "server-made rows took push or replace");
  if (document.querySelectorAll("#s li").length !== 1) fail(step, "server-made rows changed");
  return { failures, notes };
}, source);
await browser.close();
for (const n of result.notes) console.log(n);
console.log(`3000 random view edits: ${result.failures} failures`);
process.exit(result.failures ? 1 : 0);
