// Checks state() and render scheduling in a real browser: a render runs once per batch of
// assignments, only for keys it read in its last run, stops when its signal aborts, and ignores
// assignments of the same value. node state-check.mjs
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(join(here, "..", "..", "package.json"))("@playwright/test");
const source = readFileSync(join(here, "..", "..", "crates", "corten", "js", "state.js"), "utf8");
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();
await page.setContent("<div id=root></div>");
const failures = await page.evaluate(async (src) => {
  const { state, mount } = await import(URL.createObjectURL(new Blob([src], { type: "text/javascript" })));
  const tick = () => new Promise((r) => setTimeout(r, 0));
  const failed = [];
  const expect = (name, got, want) => { if (JSON.stringify(got) !== JSON.stringify(want)) failed.push(`${name}: got ${JSON.stringify(got)}, want ${JSON.stringify(want)}`); };
  const s = state({ flag: true, a: 1, b: 1, c: 1 });
  const runs = { pick: 0, both: 0 };
  const life = new AbortController();
  mount(document.getElementById("root"), () => ({
    render: {
      pick() { runs.pick++; void (s.flag ? s.a : s.b); },
      both() { runs.both++; void (s.a + s.c); },
    },
  }), { signal: life.signal });
  expect("first run", runs, { pick: 1, both: 1 });
  s.a = 2; s.a = 3; s.c = 2;
  await tick();
  expect("one run per batch", runs, { pick: 2, both: 2 });
  s.b = 2;
  await tick();
  expect("unread key", runs, { pick: 2, both: 2 });
  s.flag = false;
  await tick();
  expect("switch", runs, { pick: 3, both: 2 });
  s.a = 4;
  await tick();
  expect("key no longer read", runs, { pick: 3, both: 3 });
  s.b = 3;
  await tick();
  expect("key now read", runs, { pick: 4, both: 3 });
  s.b = 3;
  await tick();
  expect("same value", runs, { pick: 4, both: 3 });
  s.d = 1;
  await tick();
  expect("new key nobody reads", runs, { pick: 4, both: 3 });
  for (let i = 0; i < 5; i++) { s.flag = !s.flag; await tick(); }
  s.a = 10; s.b = 10;
  await tick();
  expect("after many switches", runs, { pick: 10, both: 4 });
  life.abort();
  s.a = 11; s.b = 11; s.flag = !s.flag;
  await tick();
  expect("aborted", runs, { pick: 10, both: 4 });
  return failed;
}, source);
await browser.close();
console.log(failures.length ? failures.join("\n") : "state: all checks passed");
process.exit(failures.length ? 1 : 0);
