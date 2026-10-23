// Browser smoke test and timings for the seller desk.
// Usage: node examples/seller-desk/smoke.mjs http://localhost:18484
import { chromium } from "@playwright/test";

const base = process.argv[2];
if (!base) throw new Error("usage: node examples/seller-desk/smoke.mjs <base url>");
const failures = [];
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};
/** @type {Record<string, string>} */
const timings = {};

const browser = await chromium.launch({ headless: true, args: ["--enable-precise-memory-info", "--js-flags=--expose-gc"] });
try {
  const page = await (await browser.newContext()).newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(e.message));

  // At DOMContentLoaded the page's module scripts have run and handed their handlers to the
  // runtime: a click then already runs the grid's handler (and a second click puts it back). The
  // hand-off is not an API: a forged one from another URL, sent just before the real module's,
  // is ignored (or this click would find no handler).
  await page.addInitScript(() => {
    let forged = false;
    document.addEventListener("corten:module", (event) => {
      if (forged) return;
      forged = true;
      const { name } = /** @type {CustomEvent<{ name: string }>} */ (event).detail;
      const detail = { name, url: new URL("/forged.js", location.href).href, exports: {} };
      document.dispatchEvent(new CustomEvent("corten:module", { detail }));
    }, { capture: true });
    document.addEventListener("DOMContentLoaded", () => {
      const box = /** @type {HTMLInputElement | null} */ (document.querySelector("tbody input[type=checkbox]"));
      box?.click();
      /** @type {any} */ (window).__countAtDcl = document.querySelector("output")?.textContent;
      box?.click();
    });
  });
  await page.goto(base);
  check("handlers run from DOMContentLoaded", (await page.evaluate(() => /** @type {any} */ (window).__countAtDcl)) === "1");
  const nav = await page.evaluate(async () => {
    const { ready } = await import("corten");
    await ready;
    const t = performance.getEntriesByType("navigation")[0];
    return { ready: performance.now(), dcl: t.domContentLoadedEventEnd, rows: document.querySelectorAll("tbody tr").length, bytes: t.transferSize };
  });
  timings["first load: DOMContentLoaded / scripts ready"] = `${nav.dcl.toFixed(0)} / ${nav.ready.toFixed(0)} ms, ${nav.rows} rows, ${(nav.bytes / 1024).toFixed(0)} KB transferred`;

  // Each timing runs the real event in the page: handlers run synchronously while it is
  // dispatched, so the time is the handler and the runtime's routing to it.
  const time = (/** @type {string} */ name, /** @type {() => void} */ act) => page.evaluate(async (code) => {
    const fn = new Function(code);
    const start = performance.now();
    fn();
    const handled = performance.now() - start;
    await new Promise((r) => setTimeout(r)); // the MutationObserver's work, if any
    return { handled, settled: performance.now() - start };
  }, `(${act.toString()})()`).then((t) => (timings[name] = `${t.handled.toFixed(2)} ms (settled ${t.settled.toFixed(2)} ms)`, t));

  const count = () => page.textContent("output");
  await time("toggle one row", () => /** @type {HTMLInputElement} */ (document.querySelectorAll("tbody input[type=checkbox]")[3]).click());
  check("a row's click selects it and the grid counts it", (await count()) === "1" && (await page.isVisible("[role=group]")));
  await page.click("tbody tr:nth-child(6) td:nth-child(2)");
  check("a click outside a checkbox is not the handler's (ref.closest() finds none)", (await count()) === "1", await count());
  await time("shift-click a 200-row range", () => {
    const box = /** @type {HTMLInputElement} */ (document.querySelectorAll("tbody input[type=checkbox]")[203]);
    box.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, shiftKey: true }));
  });
  check("shift-click selects the range (one handler on the table body)", (await count()) === "201", await count());
  await time("select all", () => /** @type {HTMLInputElement} */ (document.querySelector("thead input[type=checkbox]")).click());
  check("select all selects every row", (await count()) === String(nav.rows), await count());
  await page.click("[role=group] button:nth-of-type(2)");
  check("Clear (a handler passed to a child) clears the selection", (await count()) === "0" && !(await page.isVisible("[role=group]")));

  await time("filter keystroke", () => {
    const input = /** @type {HTMLInputElement} */ (document.querySelector("input[type=search]"));
    input.value = "jungle";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  const visible = await page.$$eval("tbody tr", (rows) => rows.filter((r) => !r.hidden).length);
  const jungle = await page.$$eval("tbody tr", (rows) => rows.filter((r) => r.cells[2].textContent === "Jungle").length);
  check("filter hides rows that do not match", visible === jungle && visible > 0, `${visible} of ${jungle}`);
  await page.click("thead input[type=checkbox]");
  check("select all with a filter selects only the visible rows", (await count()) === String(visible), await count());
  await page.click("[role=group] button:nth-of-type(2)");
  await page.fill("input[type=search]", "");

  await time("sort by price (moves every row)", () => /** @type {HTMLButtonElement} */ (document.querySelectorAll("thead button")[3]).click());
  const prices = await page.$$eval("tbody tr", (rows) => rows.slice(0, 5).map((r) => Number(/** @type {HTMLInputElement} */ (r.cells[5].querySelector("input")).value)));
  check("sort orders rows by price", prices.every((p, i) => i === 0 || prices[i - 1] <= p), prices.join(" "));
  await time("sort again (descending)", () => /** @type {HTMLButtonElement} */ (document.querySelectorAll("thead button")[3]).click());
  await page.click("tbody tr:first-child input[type=checkbox]");
  check("handlers still run on rows after they moved", (await count()) === "1");
  await page.click("[role=group] button:nth-of-type(2)");

  // A price edit: Save shows, Escape restores, an invalid price comes back with its error, a
  // valid one replaces the cell.
  const cell = "tbody tr:nth-child(2) td:last-child";
  const before = await page.inputValue(`${cell} input`);
  await page.fill(`${cell} input`, "abc");
  check("typing a price shows Save", await page.isVisible(`${cell} button`));
  await page.press(`${cell} input`, "Escape");
  check("Escape puts the saved price back", (await page.inputValue(`${cell} input`)) === before && !(await page.isVisible(`${cell} button`)));
  await page.fill(`${cell} input`, "abc");
  await page.click(`${cell} button`);
  await page.waitForSelector(`${cell} input[aria-invalid=true]`, { timeout: 5000 }).catch(() => {});
  check("an invalid price comes back with its error, focused", (await page.evaluate(() => document.activeElement?.getAttribute("aria-invalid"))) === "true");
  await page.fill(`${cell} input`, "7.5");
  await page.click(`${cell} button`);
  await page.waitForFunction((sel) => document.querySelector(sel)?.getAttribute("aria-invalid") === null, `${cell} input`, { timeout: 5000 }).catch(() => {});
  check("a valid price replaces the cell", (await page.inputValue(`${cell} input`)) === "7.50");
  await page.fill(`${cell} input`, "8");
  await page.press(`${cell} input`, "Escape");
  check("the patched cell's handlers know the new saved price", (await page.inputValue(`${cell} input`)) === "7.50");

  // Text typed while the form is on its way is kept when the answer lands, with the focus and
  // caret; the server's markup (the error) still applies.
  let release = () => {};
  await page.route("**/items/*/price", async (route) => {
    await new Promise((resolve) => (release = () => resolve(undefined)));
    await route.continue();
  });
  await page.fill(`${cell} input`, "x");
  await page.click(`${cell} button`);
  await page.focus(`${cell} input`);
  await page.keyboard.type("yz");
  await page.keyboard.press("ArrowLeft");
  release();
  await page.waitForSelector(`${cell} input[aria-invalid=true]`, { timeout: 5000 }).catch(() => {});
  const kept = await page.evaluate((sel) => {
    const input = /** @type {HTMLInputElement} */ (document.querySelector(sel));
    return { value: input.value, focused: document.activeElement === input, caret: input.selectionStart };
  }, `${cell} input`);
  check(
    "a field edited while its form was sent keeps the edit, focus and caret",
    kept.value === "xyz" && kept.focused && kept.caret === 2,
    JSON.stringify(kept),
  );
  // Focus the user moved elsewhere while waiting is not pulled back to the invalid field.
  await page.fill(`${cell} input`, "q");
  await page.click(`${cell} button`);
  const elsewhere = "tbody tr:nth-child(3) td:last-child input";
  await page.focus(elsewhere);
  await page.waitForTimeout(200);
  const answered = page.waitForResponse("**/items/*/price");
  await page.evaluate((sel) => ((/** @type {any} */ (window)).__oldCell = document.querySelector(sel)), `${cell} input`);
  release();
  await answered;
  await page.waitForFunction((sel) => document.querySelector(sel) !== (/** @type {any} */ (window)).__oldCell, `${cell} input`, { timeout: 5000 }).catch(() => {});
  check(
    "an invalid answer does not take the focus from a field the user moved to",
    await page.evaluate((sel) => document.activeElement === document.querySelector(sel), elsewhere),
  );
  await page.unroute("**/items/*/price");
  await page.fill(`${cell} input`, "7.50");
  await page.click(`${cell} button`);
  await page.waitForFunction((sel) => document.querySelector(sel)?.getAttribute("aria-invalid") === null, `${cell} input`, { timeout: 5000 }).catch(() => {});

  // Archive two rows through the confirm dialog (a child given the grid's dialog ref and handler).
  const ids = await page.$$eval("tbody tr", (rows) => rows.slice(0, 2).map((r) => r.id));
  await page.click("tbody tr:nth-child(1) input[type=checkbox]");
  await page.click("tbody tr:nth-child(2) input[type=checkbox]");
  await page.click("[role=group] button:nth-of-type(1)");
  check("Archive opens the dialog", await page.isVisible("dialog[open]"));
  await page.click("dialog button[value=confirm]");
  await page.waitForFunction(() => /Archived/.test(document.getElementById("desk-status")?.textContent ?? ""), null, { timeout: 5000 }).catch(() => {});
  check("confirming archives the rows", (await page.textContent("#desk-status")) === "Archived 2 items.", await page.textContent("#desk-status"));
  const left = await page.$$eval(ids.map((id) => `#${id}`).join(","), (rows) => rows.length);
  check("the patch removed the archived rows and the selection is clear", left === 0 && (await count()) === "0", `${left} left`);
  await page.click("tbody tr:nth-child(1) input[type=checkbox]");
  check("the rows that moved up still select", (await count()) === "1");
  await page.click("[role=group] button:nth-of-type(2)");
  check("the dialog closed", !(await page.isVisible("dialog[open]")));

  // Live levels: the feed changes some stock cells.
  const levels = () => page.$$eval("tbody tr > td:nth-child(5)", (cells) => cells.map((c) => c.textContent).join());
  const first = await levels();
  await page.waitForTimeout(1200);
  check("live levels update stock cells", (await levels()) !== first);

  // Memory with the page settled.
  const heap = await page.evaluate(() => {
    /** @type {any} */ (window).gc?.();
    return /** @type {any} */ (performance).memory?.usedJSHeapSize ?? 0;
  });
  timings["JS heap after GC"] = `${(heap / 1048576).toFixed(1)} MB`;

  const other = errors.filter((e) => !/Failed to load resource/.test(e));
  check("no console errors", other.length === 0, other.join(" | "));
} finally {
  await browser.close();
}
for (const [name, value] of Object.entries(timings)) console.log(`time ${name}: ${value}`);
console.log(failures.length ? `${failures.length} failed` : "all passed");
process.exit(failures.length ? 1 : 0);
