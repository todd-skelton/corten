// Browser smoke test and timings for the seller desk.
// Usage: node examples/seller-desk/smoke.mjs http://localhost:18484
import { chromium } from "@playwright/test";
import { stall } from "../../scripts/stall.mjs";

const base = process.argv[2];
if (!base) throw new Error("usage: node examples/seller-desk/smoke.mjs <base url>");
const failures = [];
const seen = stall(5);
const check = (name, ok, detail = "") => {
  seen(name);
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
    input.value = "tidebreak";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  const visible = await page.$$eval("tbody tr", (rows) => rows.filter((r) => !r.hidden).length);
  const inSet = await page.$$eval("tbody tr", (rows) => rows.filter((r) => r.cells[2].textContent === "Tidebreak").length);
  check("filter hides rows that do not match", visible === inSet && visible > 0, `${visible} of ${inSet}`);
  const shownText = () => page.$eval("#item-count", (p) => p.previousElementSibling?.textContent ?? "");
  check(
    "the filter says how many it shows of how many, in the item count's place",
    (await shownText()) === `${visible.toLocaleString("en")} of ${nav.rows.toLocaleString("en")} shown` && (await page.isHidden("#item-count")),
    await shownText(),
  );
  await page.click("thead input[type=checkbox]");
  check("select all with a filter selects only the visible rows", (await count()) === String(visible), await count());
  await page.click("[role=group] button:nth-of-type(2)");
  await page.fill("input[type=search]", "");
  check("with the filter cleared the item count is back", (await page.isVisible("#item-count")) && (await shownText()) === "");
  check("on a desktop the set and condition are their own columns, not repeated under the name", await page.isHidden("tbody tr td:nth-child(2) small"));

  await time("sort by price (moves every row)", () => /** @type {HTMLButtonElement} */ (document.querySelectorAll("thead button")[3]).click());
  const prices = await page.$$eval("tbody tr", (rows) => rows.slice(0, 5).map((r) => Number(/** @type {HTMLInputElement} */ (r.cells[5].querySelector("input")).value)));
  check("sort orders rows by price", prices.every((p, i) => i === 0 || prices[i - 1] <= p), prices.join(" "));
  await time("sort again (descending)", () => /** @type {HTMLButtonElement} */ (document.querySelectorAll("thead button")[3]).click());
  await page.click("thead button >> nth=0");
  const names = await page.$$eval("tbody tr", (rows) => rows.slice(0, 30).map((r) => r.cells[1].firstElementChild?.textContent ?? ""));
  check("sort orders rows by name, read from the row's field", names.every((n, i) => i === 0 || names[i - 1].localeCompare(n, "en") <= 0), names.slice(0, 5).join(", "));
  await page.click("tbody tr:first-child input[type=checkbox]");
  check("handlers still run on rows after they moved", (await count()) === "1");
  await page.click("[role=group] button:nth-of-type(2)");

  // A price edit: Save shows, Escape restores, an invalid price comes back with its error, a
  // valid one replaces the cell.
  // Rows by id, so a sort below does not move the cells under the test.
  const cell = `#${await page.$eval("tbody tr:nth-child(2)", (r) => r.id)} td:last-child`;
  const before = await page.inputValue(`${cell} input`);
  await page.fill(`${cell} input`, "abc");
  check("typing a price shows Save", await page.isVisible(`${cell} button`));
  check("the grid counts the unsaved price in the catalog's words", (await page.textContent("[role=group] + output")) === "1 price not saved", await page.textContent("[role=group] + output"));
  // A second unsaved price: the plural message picks its other form in the browser.
  const third = `#${await page.$eval("tbody tr:nth-child(4)", (r) => r.id)} td:last-child`;
  const thirdBefore = await page.inputValue(`${third} input`);
  await page.fill(`${third} input`, "abc");
  check("two unsaved prices take the message's other form", (await page.textContent("[role=group] + output")) === "2 prices not saved", await page.textContent("[role=group] + output"));
  await page.press(`${third} input`, "Escape");
  check("Escape there puts its price back and the count goes down", (await page.inputValue(`${third} input`)) === thirdBefore && (await page.textContent("[role=group] + output")) === "1 price not saved", await page.textContent("[role=group] + output"));
  await page.press(`${cell} input`, "Escape");
  check("Escape puts the saved price back", (await page.inputValue(`${cell} input`)) === before && !(await page.isVisible(`${cell} button`)));
  await page.fill(`${cell} input`, "abc");
  await page.click(`${cell} button`);
  await page.waitForSelector(`${cell} input[aria-invalid=true]`, { timeout: 5000 }).catch(() => {});
  check("an invalid price comes back with its error, focused", (await page.evaluate(() => document.activeElement?.getAttribute("aria-invalid"))) === "true");
  // A second row refused too: each field is described by its own row's message.
  const second = `#${await page.$eval("tbody tr:nth-child(3)", (r) => r.id)} td:last-child`;
  await page.fill(`${second} input`, "xyz");
  await page.click(`${second} button`);
  await page.waitForSelector(`${second} input[aria-invalid=true]`, { timeout: 5000 }).catch(() => {});
  const described = await page.evaluate(() =>
    Array.from(document.querySelectorAll("input[aria-invalid=true]"), (input) => {
      const message = document.getElementById(input.getAttribute("aria-describedby") ?? "");
      return { own: !!message && input.closest("form")?.contains(message), ids: document.querySelectorAll(`[id="${message?.id}"]`).length };
    }),
  );
  check("two refused rows: each field is described by its own message", described.length === 2 && described.every((d) => d.own && d.ids === 1), JSON.stringify(described));
  await page.click("thead button >> nth=3");
  if ((await page.getAttribute("thead th:nth-child(6)", "aria-sort")) !== "ascending") await page.click("thead button >> nth=3");
  const last = await page.$$eval("tbody tr", (rows) => rows.slice(-2).map((r) => /** @type {HTMLInputElement} */ (r.cells[5].querySelector("input")).value).sort().join(" "));
  check("sorting by price puts the refused prices last", last === "abc xyz", last);
  await page.fill(`${second} input`, "3");
  await page.click(`${second} button`);
  await page.waitForFunction((sel) => document.querySelector(sel)?.getAttribute("aria-invalid") === null, `${second} input`, { timeout: 5000 }).catch(() => {});
  await page.fill(`${cell} input`, "7.5");
  await page.click(`${cell} button`);
  await page.waitForFunction((sel) => document.querySelector(sel)?.getAttribute("aria-invalid") === null, `${cell} input`, { timeout: 5000 }).catch(() => {});
  check("a valid price replaces the cell", (await page.inputValue(`${cell} input`)) === "7.50");
  await page.fill(`${cell} input`, "8");
  await page.press(`${cell} input`, "Escape");
  check("the patched cell's handlers know the new saved price", (await page.inputValue(`${cell} input`)) === "7.50");

  // Text typed while the form is on its way is kept when the answer lands, with the focus and
  // caret; the server's markup (the error) still applies.
  // Each price request is held until `release()`. `held()` waits until one is, so a release never
  // comes before its request, and a release with any other number held fails the run.
  /** @type {(() => void)[]} */
  const pending = [];
  /** @type {() => void} */
  let arrived = () => {};
  await page.route("**/items/*/price", async (route) => {
    await new Promise((resolve) => {
      pending.push(() => resolve(undefined));
      arrived();
    });
    await route.continue();
  });
  const held = () => (pending.length ? Promise.resolve() : new Promise((resolve) => (arrived = () => resolve(undefined))));
  const release = () => {
    if (pending.length !== 1) throw new Error(`expected one held price request, found ${pending.length}`);
    pending.splice(0).forEach((go) => go());
  };
  await page.fill(`${cell} input`, "x");
  await page.click(`${cell} button`);
  await held();
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
  await held();
  const answered = page.waitForResponse("**/items/*/price");
  await page.evaluate((sel) => ((/** @type {any} */ (window)).__oldCell = document.querySelector(sel)), `${cell} input`);
  release();
  await answered;
  await page.waitForFunction((sel) => document.querySelector(sel) !== (/** @type {any} */ (window)).__oldCell, `${cell} input`, { timeout: 5000 }).catch(() => {});
  check(
    "an invalid answer does not take the focus from a field the user moved to",
    await page.evaluate((sel) => document.activeElement === document.querySelector(sel), elsewhere),
  );
  pending.splice(0).forEach((go) => go());
  await page.unroute("**/items/*/price");
  await page.fill(`${cell} input`, "7.50");
  await page.click(`${cell} button`);
  await page.waitForFunction((sel) => document.querySelector(sel)?.getAttribute("aria-invalid") === null, `${cell} input`, { timeout: 5000 }).catch(() => {});

  // Archive two rows through the confirm dialog, which the bulk bar's button opens with commandfor,
  // with the filter on: its count follows the rows the patch removes.
  await page.fill("input[type=search]", "tidebreak");
  const filtered = await page.$$eval("tbody tr", (rows) => rows.filter((r) => !r.hidden).length);
  const ids = await page.$$eval("tbody tr", (rows) => rows.filter((r) => !r.hidden).slice(0, 2).map((r) => r.id));
  await page.click(`#${ids[0]} input[type=checkbox]`);
  await page.click(`#${ids[1]} input[type=checkbox]`);
  await page.click("[role=group] button:nth-of-type(1)");
  check("Archive opens the dialog", await page.isVisible("dialog[open]"));
  const itemCount = () => page.textContent("#item-count");
  const countBefore = await itemCount();
  await page.click("dialog button[value=confirm]");
  await page.waitForFunction(() => /Archived/.test(document.getElementById("desk-status")?.textContent ?? ""), null, { timeout: 5000 }).catch(() => {});
  check("confirming archives the rows", (await page.textContent("#desk-status")) === "Archived 2 items.", await page.textContent("#desk-status"));
  const left = await page.$$eval(ids.map((id) => `#${id}`).join(","), (rows) => rows.length);
  check("the patch removed the archived rows and the selection is clear", left === 0 && (await count()) === "0", `${left} left`);
  check(
    "the filter's count follows the archive",
    (await shownText()) === `${(filtered - 2).toLocaleString("en")} of ${(nav.rows - 2).toLocaleString("en")} shown`,
    await shownText(),
  );
  await page.fill("input[type=search]", "");
  check(
    "the archive's answer counts the items left",
    countBefore === `${nav.rows.toLocaleString("en")} items` && (await itemCount()) === `${(nav.rows - 2).toLocaleString("en")} items`,
    `${countBefore} -> ${await itemCount()}`,
  );
  await page.click("tbody tr:nth-child(1) input[type=checkbox]");
  check("the rows that moved up still select", (await count()) === "1");
  await page.click("[role=group] button:nth-of-type(2)");
  check("the dialog closed", !(await page.isVisible("dialog[open]")));

  // Live levels: the feed changes some stock cells.
  const levels = () => page.$$eval("tbody tr > td:nth-child(5)", (cells) => cells.map((c) => c.textContent).join());
  const first = await levels();
  const changed = await page
    .waitForFunction((was) => [...document.querySelectorAll("tbody tr > td:nth-child(5)")].map((c) => c.textContent).join() !== was, first, { timeout: 10_000 })
    .then(() => true, () => false);
  check("live levels update stock cells", changed);
  // A row at 0 is marked sold out, and only such a row: the live levels sell rows out and restock
  // them, so wait for a row whose mark the script changed, then compare every row.
  const marked = () => page.$$eval("tbody tr[data-sold-out]", (rows) => rows.map((r) => r.id).sort().join());
  const marksBefore = await marked();
  const flipped = await page
    .waitForFunction((was) => [...document.querySelectorAll("tbody tr[data-sold-out]")].map((r) => r.id).sort().join() !== was, marksBefore, { timeout: 15_000 })
    .then(() => true, () => false);
  const soldOutMarked = await page.$$eval("tbody tr", (rows) => rows.every((r) => (r.querySelector("td:nth-child(5)")?.textContent === "0") === r.hasAttribute("data-sold-out")));
  check("a row is marked sold out exactly while its quantity is 0, as the levels change", flipped && soldOutMarked && marksBefore !== "", `${flipped} ${soldOutMarked} ${marksBefore.split(",").length} marked before`);

  // On a phone the table keeps the name, quantity and price, and a price being edited (its Save
  // button, then its error) fits without the grid scrolling sideways.
  await page.setViewportSize({ width: 390, height: 844 });
  const phoneCell = "tbody tr:not([hidden]) td:last-child";
  const fits = () => page.evaluate((sel) => {
    const scroll = /** @type {HTMLElement} */ (document.querySelector("tbody")?.closest("table")?.parentElement);
    const box = scroll.getBoundingClientRect();
    const parts = [...(document.querySelector(sel)?.querySelectorAll("input, button, p") ?? [])].map((el) => el.getBoundingClientRect());
    const inside = parts.every((r) => r.width > 0 && r.left >= box.left && r.right <= box.right);
    const headers = [...document.querySelectorAll("thead th")].filter((th) => getComputedStyle(th).display !== "none").length;
    const under = document.querySelector("tbody tr:not([hidden]) td:nth-child(2) small");
    const subline = under !== null && getComputedStyle(under).display !== "none" && /\S \u00b7 \S/.test(under.textContent ?? "");
    const cells = [...(document.querySelector("tbody tr")?.children ?? [])].filter((td) => getComputedStyle(td).display !== "none").length;
    return { inside, sideways: scroll.scrollWidth > scroll.clientWidth, headers, cells, parts: parts.length, subline };
  }, phoneCell);
  await page.fill(`${phoneCell} input`, "12.34");
  const editing = await fits();
  check(
    "on a phone the edited price and its Save button fit, every cell under its header, the set and condition under the name",
    editing.inside && !editing.sideways && editing.headers === 4 && editing.cells === 4 && editing.parts >= 2 && editing.subline,
    JSON.stringify(editing),
  );
  await page.fill(`${phoneCell} input`, "abc");
  await page.click(`${phoneCell} button`);
  await page.waitForSelector(`${phoneCell} p`, { timeout: 5000 }).catch(() => {});
  const refused = await fits();
  check("on a phone a refused price's error fits too", refused.inside && !refused.sideways && refused.parts >= 3, JSON.stringify(refused));

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
