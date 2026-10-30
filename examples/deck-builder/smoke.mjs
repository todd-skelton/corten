// Browser smoke test and timings for the deck builder: every move in the browser, autosave with
// its refusals, another tab's edits, and the same deck built without JavaScript.
// Usage: node examples/deck-builder/smoke.mjs http://localhost:18487
import http from "node:http";
import { chromium } from "@playwright/test";
import { stall } from "../../scripts/stall.mjs";

const base = process.argv[2];
if (!base) throw new Error("usage: node examples/deck-builder/smoke.mjs <base url>");
const failures = [];
const seen = stall(5);
const check = (name, ok, detail = "") => {
  seen(name);
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};
/** @type {Record<string, string>} */
const timings = {};
/** @type {string[]} */
const errors = [];

const browser = await chromium.launch({ headless: true, args: ["--enable-precise-memory-info", "--js-flags=--expose-gc"] });

/** A page of its own browser (its own cookies), or another tab of `context`. */
async function open(context) {
  const page = await context.newPage();
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(base);
  // The scripts are in, and the deck's feed is open (its snapshot has arrived).
  await page.evaluate(async () => {
    const { ready } = await import("corten");
    await ready;
  });
  return page;
}

/** The deck as the page shows it: `card id -> copies`. */
const deck = (page) =>
  page.evaluate(() =>
    Object.fromEntries(
      Array.from(document.querySelectorAll("ul li[data-key]"), (li) => [li.getAttribute("data-key"), Number(/** @type {HTMLInputElement} */ (li.querySelector("input[type=number]")).value)]),
    ),
  );
const total = (page) => page.textContent("p[class$=__total] output");
const status = (page) => page.evaluate(() => {
  const s = /** @type {HTMLElement} */ (document.querySelector("[role=status][data-state]"));
  return { state: s.dataset.state, note: document.getElementById("save-note")?.textContent ?? "" };
});
/** Waits for the save on its way to be answered and taken: the status shows the note again. */
const settled = (page, note = "Saved.") =>
  page.waitForFunction((note) => {
    const s = /** @type {HTMLElement} */ (document.querySelector("[role=status][data-state]"));
    return s.dataset.state === "" && document.getElementById("save-note")?.textContent?.startsWith(note);
  }, note);
/** Whether `page` comes to show `count` copies of `id` in its deck within five seconds. */
const comesToShow = (page, id, count) =>
  page
    .waitForFunction(
      ([id, count]) => /** @type {HTMLInputElement | null} */ (document.querySelector(`ul li[data-key="${id}"] input[type=number]`))?.value === String(count),
      [id, count],
      { timeout: 5000 },
    )
    .then(
      () => true,
      () => false,
    );
const row = (page, id) => page.locator(`tbody tr[data-key="${id}"]`);
const version = (page) => page.inputValue("input[name=version]");

/** Runs `act` in the page and times it: handlers run while the event is dispatched. */
const time = (page, name, act) =>
  page
    .evaluate(async (code) => {
      const fn = new Function(code);
      const start = performance.now();
      fn();
      const handled = performance.now() - start;
      await new Promise((r) => requestAnimationFrame(() => setTimeout(r)));
      return { handled, settled: performance.now() - start };
    }, `(${act.toString()})()`)
    .then((t) => (timings[name] = `${t.handled.toFixed(2)} ms (settled ${t.settled.toFixed(2)} ms)`));

try {
  const context = await browser.newContext();
  const page = await open(context);
  const nav = await page.evaluate(() => {
    const t = /** @type {PerformanceNavigationTiming} */ (performance.getEntriesByType("navigation")[0]);
    return { ready: performance.now(), dcl: t.domContentLoadedEventEnd, rows: document.querySelectorAll("tbody tr").length, bytes: t.transferSize };
  });
  timings["first load: DOMContentLoaded / scripts ready"] = `${nav.dcl.toFixed(0)} / ${nav.ready.toFixed(0)} ms, ${nav.rows} rows, ${(nav.bytes / 1024).toFixed(0)} KB transferred`;
  const start = await deck(page);
  check("the page shows the collection and the saved deck", nav.rows === 240 && start["gravel-sprite"] === 3 && (await total(page)) === "10", JSON.stringify(start));

  // Filter and sort, in the page.
  await time(page, "filter 240 rows", () => {
    const input = /** @type {HTMLInputElement} */ (document.querySelector("input[type=search]"));
    input.value = "drake";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  const visible = await page.locator("tbody tr:not([hidden])").count();
  check("the filter shows the matching cards and how many", visible === 1 && (await page.textContent("[class$=__counts] output")) === "1 of 240 shown", `${visible}`);
  await page.fill("input[type=search]", "");
  await page.dispatchEvent("input[type=search]", "input");
  await time(page, "sort 240 rows by cost", () => /** @type {HTMLElement} */ (document.querySelector("thead th:nth-child(4) a")).click());
  const costs = await page.$$eval("tbody tr td:nth-child(4)", (tds) => tds.map((td) => Number(td.textContent)));
  check("a column header sorts in the page, cheapest first", costs.every((c, i) => i === 0 || costs[i - 1] <= c) && page.url() === `${base}/`, `${costs.slice(0, 6)}`);
  await page.click("thead th:nth-child(4) a");
  const desc = await page.$$eval("tbody tr td:nth-child(4)", (tds) => tds.map((td) => Number(td.textContent)));
  check("a second click sorts the other way", desc[0] === 7 && (await page.getAttribute("thead th:nth-child(4)", "aria-sort")) === "descending", `${desc.slice(0, 4)}`);
  await page.click("thead th:nth-child(3) a");

  // Add: shown at once, then saved. The save is held, so the line is checked before any answer.
  let release = () => {};
  const loaded = Number(await version(page));
  await page.route("**/deck", async (route) => {
    await new Promise((r) => (release = r));
    await route.continue();
  });
  await row(page, "pearl-diver").locator("button", { hasText: "Add" }).click();
  check("Add shows the card in the deck at once, before the server answers", (await deck(page))["pearl-diver"] === 1 && (await status(page)).state === "saving");
  const drawn = [await total(page), await page.textContent("figure ol li:nth-child(3) span"), await row(page, "pearl-diver").locator("td:nth-child(6) span").first().textContent()];
  check("the count, the curve and the collection row follow", drawn.join() === "11,5,1", drawn.join());
  const answered = page.waitForResponse("**/deck");
  release();
  await answered;
  await page.unroute("**/deck");
  await settled(page);
  check("the save is answered and the page takes the next version", Number(await version(page)) === loaded + 1, `${loaded} then ${await version(page)}`);
  await time(page, "Add (a change drawn and sent)", () => /** @type {HTMLElement} */ (document.querySelector('tbody tr[data-key="undertow-serpent"] td:last-child button')).click());
  await settled(page);

  // Undo and redo: buttons and keys, each saved.
  await page.click("button:has-text('Undo')");
  check("Undo takes the last change back", (await deck(page))["undertow-serpent"] === undefined);
  await settled(page);
  await page.click("button:has-text('Redo')");
  check("Redo does it again", (await deck(page))["undertow-serpent"] === 1);
  await settled(page);
  await page.locator("body").click({ position: { x: 5, y: 5 } });
  await page.keyboard.press("Control+z");
  check("Ctrl+Z undoes", (await deck(page))["undertow-serpent"] === undefined);
  await settled(page);
  await page.keyboard.press("Control+Shift+z");
  check("Ctrl+Shift+Z redoes", (await deck(page))["undertow-serpent"] === 1);
  await settled(page);

  // Select several and add them.
  await row(page, "lantern-warden").locator("input[type=checkbox]").check();
  await row(page, "moth-court-herald").locator("input[type=checkbox]").check();
  check("checking rows counts them", (await page.textContent("[class$=__picks] output")) === "2");
  await page.click("button:has-text('Add selected')");
  const picked = await deck(page);
  check("Add selected adds one of each and clears the selection", picked["lantern-warden"] === 3 && picked["moth-court-herald"] === 3 && (await page.textContent("[class$=__picks] output")) === "0", JSON.stringify(picked));
  await settled(page);

  // A change the rules refuse is rolled back, with their words.
  await row(page, "ashen-drake").locator("button", { hasText: "Add" }).click();
  await settled(page);
  await page.route("**/deck", async (route) => {
    await new Promise((r) => (release = r));
    await route.continue();
  });
  await row(page, "ashen-drake").locator("button", { hasText: "Add" }).click();
  check("a third Ashen Drake shows at once", (await deck(page))["ashen-drake"] === 3, JSON.stringify(await deck(page)));
  release();
  await settled(page, "Not saved:");
  await page.unroute("**/deck");
  check("the server refuses it and the page rolls it back with the reason", (await deck(page))["ashen-drake"] === 2 && (await status(page)).note === "Not saved: You own 2 copies of Ashen Drake.", (await status(page)).note);
  await page.click("button:has-text('Undo')");
  check("the history is as it was at the last save", (await deck(page))["ashen-drake"] === 1);
  await settled(page);

  // A count typed in.
  await page.fill('li[data-key="pearl-diver"] input[type=number]', "3");
  await page.press('li[data-key="pearl-diver"] input[type=number]', "Enter");
  check("a typed count changes the deck", (await deck(page))["pearl-diver"] === 3);
  await settled(page);
  await page.fill('li[data-key="pearl-diver"] input[type=number]', "0");
  await page.press('li[data-key="pearl-diver"] input[type=number]', "Enter");
  check("a count of 0 takes the card out", (await deck(page))["pearl-diver"] === undefined);
  await settled(page);

  // Drag and drop, both ways.
  await row(page, "salt-oracle").locator("td:nth-child(2) button").dragTo(page.locator("section[class$=__deck]"));
  check("dragging a card onto the deck adds it", (await deck(page))["salt-oracle"] === 1);
  await settled(page);
  await page.locator('li[data-key="salt-oracle"] button').dragTo(page.locator("section[class$=__collection] tbody"));
  check("dragging a line onto the collection takes it out", (await deck(page))["salt-oracle"] === undefined);
  await settled(page);

  // The same moves by keyboard: Space picks up, the arrows move, Space drops, Escape puts back.
  const announced = () => page.textContent("p[aria-live=assertive]");
  await row(page, "gravel-sprite").locator("td:nth-child(2) button").focus();
  await page.keyboard.press(" ");
  check("Space picks a card up and takes it over the deck", (await announced()) === "Picked up Gravel Sprite. Over the deck." && (await page.evaluate(() => document.activeElement?.matches("section[class$=__deck]"))), await announced());
  await page.keyboard.press("ArrowLeft");
  check("an arrow key moves it over the collection", (await announced()) === "Over the collection.");
  await page.keyboard.press("Escape");
  check("Escape puts it back and returns focus", (await announced()) === "Gravel Sprite put back." && (await page.evaluate(() => document.activeElement?.closest("tr")?.getAttribute("data-key"))) === "gravel-sprite");
  await row(page, "kiln-colossus").locator("td:nth-child(2) button").focus();
  await page.keyboard.press(" ");
  await page.keyboard.press(" ");
  check("Space drops it on the deck", (await deck(page))["kiln-colossus"] === 1 && (await announced()) === "Kiln Colossus added to the deck.");
  await settled(page);
  await page.locator('li[data-key="kiln-colossus"] button').focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Enter");
  check("a deck line dropped on the collection by keyboard leaves the deck", (await deck(page))["kiln-colossus"] === undefined && (await announced()) === "Kiln Colossus taken out of the deck.");
  await settled(page);

  // Another tab saves: an idle page takes it at once.
  const other = await open(context);
  await other.locator('tbody tr[data-key="pearl-diver"] td:last-child button').click();
  await settled(other);
  await page.waitForFunction(() => document.querySelector('ul li[data-key="pearl-diver"]'));
  check("an idle page takes another tab's save and says so", (await status(page)).state === "updated" && (await version(page)) === (await version(other)));

  // Both change at once: the slower save is stale, and the player chooses.
  await page.route("**/deck", async (route) => {
    await new Promise((r) => (release = r));
    await route.continue();
  });
  await row(page, "kiln-colossus").locator("button", { hasText: "Add" }).click();
  await other.locator('tbody tr[data-key="undertow-serpent"] td:last-child button').click();
  await settled(other);
  const stale = page.waitForResponse("**/deck");
  release();
  check("a save built on an old version is refused as stale", (await (await stale).status()) === 409);
  await page.unroute("**/deck");
  await page.waitForSelector("[role=alert]:not([hidden])");
  check("the page offers to keep both changes or take the other tab's deck", (await page.textContent("[role=alert]")).includes("changed in another tab"));
  await page.click("button:has-text('Keep both changes')");
  await settled(page);
  const both = await deck(page);
  check("keeping both saves the other tab's change with this one", both["undertow-serpent"] === 2 && both["kiln-colossus"] === 1, JSON.stringify(both));
  check("and the other tab takes the merged deck", await comesToShow(other, "kiln-colossus", 1));

  // Take theirs: this page's change is dropped.
  await page.route("**/deck", async (route) => {
    await new Promise((r) => (release = r));
    await route.continue();
  });
  await row(page, "lantern-warden").locator("button", { hasText: "Add" }).click();
  await other.fill('li[data-key="undertow-serpent"] input[type=number]', "1");
  await other.press('li[data-key="undertow-serpent"] input[type=number]', "Enter");
  await settled(other);
  release();
  await page.waitForSelector("[role=alert]:not([hidden])");
  await page.unroute("**/deck");
  await page.click("button:has-text(\"Use the other tab's deck\")");
  const theirs = await deck(page);
  check("taking the other tab's deck drops this page's change", theirs["undertow-serpent"] === 1 && theirs["lantern-warden"] === 3 && JSON.stringify(theirs) === JSON.stringify(await deck(other)), JSON.stringify(theirs));

  // The server cannot be reached: nothing is lost, and Try again sends it.
  await page.route("**/deck", (route) => route.abort());
  await row(page, "pearl-diver").locator("button", { hasText: "Add" }).click();
  await page.waitForFunction(() => /** @type {HTMLElement} */ (document.querySelector("[role=status][data-state]")).dataset.state === "offline");
  check("an unreachable server keeps the change and says it is not saved", (await deck(page))["pearl-diver"] === 2 && (await page.isVisible("button:has-text('Try again')")));
  await page.unroute("**/deck");
  await page.click("button:has-text('Try again')");
  await settled(page);
  check("Try again saves it", (await status(page)).note.startsWith("Saved.") && (await deck(page))["pearl-diver"] === 2);
  check("the other tab sees the retried save", await comesToShow(other, "pearl-diver", 2));

  // The server saves but its answer is lost, and another tab saves before Try again: the change
  // is in the deck once, with the other tab's.
  await page.route("**/deck", async (route) => {
    await route.fetch();
    await route.abort();
  });
  await row(page, "undertow-serpent").locator("button", { hasText: "Add" }).click();
  // Not saved, or, when the feed has already brought the deck with this change, saved.
  await page.waitForFunction(() => ["offline", "saved"].includes(/** @type {HTMLElement} */ (document.querySelector("[role=status][data-state]")).dataset.state ?? ""));
  await page.unroute("**/deck");
  // The other tab has the saved deck from the feed, then saves its own change.
  await comesToShow(other, "undertow-serpent", 2);
  await other.locator('tbody tr[data-key="pearl-diver"] td:last-child button').click();
  await settled(other);
  // The feed may already have told the page its save was made; else Try again asks the server.
  if (await page.isVisible("button:has-text('Try again')")) await page.click("button:has-text('Try again')");
  const once = (await comesToShow(page, "pearl-diver", 3)) && (await comesToShow(page, "undertow-serpent", 2));
  check("a save whose answer was lost is not made twice", once && (await comesToShow(other, "undertow-serpent", 2)) && !(await page.isVisible("[role=alert]")), JSON.stringify(await deck(page)));

  // The same, where only the server can say: a tab whose feed is held (so it cannot learn of the
  // save that way) saves, the server commits it, and the answer's body is cut off after its
  // headers. Another tab saves meanwhile. Try again goes with the same key, and the server answers
  // with the save it made: the change is in the deck once. A proxy of the smoke's own cuts the body
  // (a route cannot stream one to fail midway).
  const app = new URL(base);
  const cutProxy = http.createServer((req, res) => {
    const sent = [];
    req.on("data", (c) => sent.push(c));
    req.on("end", () => {
      // As the browser sent it to the app: the override makes it look cross-site, which the app
      // refuses for a write.
      const headers = { ...req.headers, host: app.host, origin: app.origin, "sec-fetch-site": "same-origin" };
      const upstream = http.request(new URL(req.url ?? "/", base), { method: req.method, headers }, (answer) => {
        const body = [];
        answer.on("data", (c) => body.push(c));
        answer.on("end", () => {
          const whole = Buffer.concat(body);
          const { "transfer-encoding": _, ...kept } = answer.headers;
          res.writeHead(answer.statusCode ?? 502, { ...kept, "content-length": String(whole.length) });
          res.write(whole.subarray(0, whole.length >> 1), () => res.socket?.destroy());
        });
      });
      upstream.end(Buffer.concat(sent));
    });
  });
  await new Promise((resolve) => cutProxy.listen(0, "127.0.0.1", () => resolve(undefined)));
  const proxyPort = /** @type {import("node:net").AddressInfo} */ (cutProxy.address()).port;
  const third = await context.newPage();
  third.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  third.on("pageerror", (e) => errors.push(e.message));
  let liveAsked = () => {};
  const asked = new Promise((resolve) => (liveAsked = () => resolve(undefined)));
  let letLive = () => {};
  const held = new Promise((resolve) => (letLive = () => resolve(undefined)));
  await third.route("**/feed/deck/live**", async (route) => {
    liveAsked();
    await held;
    await route.continue().catch(() => {});
  });
  await third.goto(base);
  // The snapshot has arrived when the feed asks for its live stream.
  await asked;
  /** @type {string[]} */
  const keys = [];
  third.on("request", (r) => r.method() === "POST" && r.url().endsWith("/deck") && keys.push(new URLSearchParams(r.postData() ?? "").get("submit") ?? ""));
  // Two cards in no deck yet and none the steps after this one add: this tab's and the other's.
  const [mine, spare] = await third.evaluate(() => {
    const taken = new Set(["salt-oracle", "regent-of-ash", "crown-of-thorns-and-glass", "kiln-colossus"]);
    for (const li of Array.from(document.querySelectorAll("ul li[data-key]"))) taken.add(li.getAttribute("data-key") ?? "");
    return Array.from(document.querySelectorAll("tbody tr[data-key]"), (tr) => tr.getAttribute("data-key") ?? "").filter((id) => !taken.has(id)).slice(0, 2);
  });
  await third.route(`${app.origin}/deck`, (route) => route.continue({ url: `http://127.0.0.1:${proxyPort}/deck` }));
  await row(third, mine).locator("button", { hasText: "Add" }).click();
  await third.waitForFunction(() => /** @type {HTMLElement} */ (document.querySelector("[role=status][data-state]")).dataset.state === "offline");
  await third.unroute(`${app.origin}/deck`);
  await comesToShow(other, mine, 1);
  await row(other, spare).locator("button", { hasText: "Add" }).click();
  await settled(other);
  await third.click("button:has-text('Try again')");
  await settled(third);
  check("Try again after a cut-off answer sends the same submit key", keys.length === 2 && keys[0] === keys[1], keys.join(", "));
  check("and the server answers with the save it made, once", (await deck(third))[mine] === 1, JSON.stringify(await deck(third)));
  letLive();
  check("the held feed then brings the other tab's save", await comesToShow(third, spare, 1));
  check("which keeps the change once", (await comesToShow(other, mine, 1)) && (await deck(third))[mine] === 1 && !(await third.isVisible("[role=alert]")), JSON.stringify(await deck(third)));
  await third.close();
  cutProxy.close();

  // The server has forgotten a key (it keeps them a day): a tab saves, the answer is lost, another
  // tab saves, and Try again is stale, and the server cannot say whether the lost save was made.
  // The retry goes with a fresh key, as the server would see the forgotten one, and its answer says
  // it is unsure. The page does not offer to add its change again, and taking the other tab's deck
  // sends nothing more: the change is in it once.
  const fourth = await context.newPage();
  fourth.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  fourth.on("pageerror", (e) => errors.push(e.message));
  let fourthAsked = () => {};
  const fourthFeed = new Promise((resolve) => (fourthAsked = () => resolve(undefined)));
  let letFourth = () => {};
  const fourthHeld = new Promise((resolve) => (letFourth = () => resolve(undefined)));
  await fourth.route("**/feed/deck/live**", async (route) => {
    fourthAsked();
    await fourthHeld;
    await route.continue().catch(() => {});
  });
  await fourth.goto(base);
  await fourthFeed;
  const [lost, theirsToo] = await fourth.evaluate((skip) => {
    const taken = new Set(["salt-oracle", "regent-of-ash", "crown-of-thorns-and-glass", "kiln-colossus", ...skip]);
    for (const li of Array.from(document.querySelectorAll("ul li[data-key]"))) taken.add(li.getAttribute("data-key") ?? "");
    return Array.from(document.querySelectorAll("tbody tr[data-key]"), (tr) => tr.getAttribute("data-key") ?? "").filter((id) => !taken.has(id)).slice(0, 2);
  }, [mine, spare]);
  await fourth.route(`${app.origin}/deck`, async (route) => {
    await route.fetch();
    await route.abort();
  });
  await row(fourth, lost).locator("button", { hasText: "Add" }).click();
  await fourth.waitForFunction(() => /** @type {HTMLElement} */ (document.querySelector("[role=status][data-state]")).dataset.state === "offline");
  await fourth.unroute(`${app.origin}/deck`);
  await comesToShow(other, lost, 1);
  await row(other, theirsToo).locator("button", { hasText: "Add" }).click();
  await settled(other);
  let sentAfter = 0;
  await fourth.route(`${app.origin}/deck`, async (route) => {
    const form = new URLSearchParams(route.request().postData() ?? "");
    form.set("submit", crypto.randomUUID());
    const answer = await route.fetch({ postData: form.toString() });
    const body = (await answer.text()).replace(/(&quot;stale&quot;,\d+,\d+,)false\)/, "$1true)");
    await route.fulfill({ response: answer, body });
  });
  await fourth.click("button:has-text('Try again')");
  await fourth.waitForFunction(() => document.getElementById("save-note")?.textContent?.startsWith("Not saved: this deck was changed"));
  await fourth.unroute(`${app.origin}/deck`);
  letFourth();
  await fourth.waitForSelector("[role=alert]:not([hidden])");
  check(
    "a stale retry the server cannot place warns and does not offer to keep both changes",
    !(await fourth.isVisible("button:has-text('Keep both changes')")) && (await fourth.isVisible("text=Your last change may already be in it")),
  );
  fourth.on("request", (r) => r.method() === "POST" && r.url().endsWith("/deck") && (sentAfter += 1));
  await fourth.click("button:has-text(\"Use the other tab's deck\")");
  const unsureDeck = await deck(fourth);
  check("taking the other tab's deck then keeps the change once and sends nothing", unsureDeck[lost] === 1 && unsureDeck[theirsToo] === 1 && sentAfter === 0, JSON.stringify(unsureDeck));
  await fourth.close();

  // After adds, removals, rollbacks and another tab's decks: the list is the lines as shown.
  const listed = await page.inputValue("textarea");
  const shownLines = await page.evaluate(() =>
    Array.from(
      document.querySelectorAll("ul li[data-key]"),
      (li) => `${/** @type {HTMLInputElement} */ (li.querySelector("input[type=number]")).value} ${li.querySelector(":scope > span:nth-of-type(2)")?.textContent}`,
    ).join("\n"),
  );
  check("the deck list follows the deck", listed === shownLines && listed.split("\n").includes("3 Pearl Diver"), listed.replaceAll("\n", "; "));
  await time(page, "undo (a change drawn and sent)", () => /** @type {HTMLElement} */ ([...document.querySelectorAll("button")].find((b) => b.textContent === "Undo")).click());
  await settled(page);
  const heap = await page.evaluate(() => {
    /** @type {any} */ (window).gc?.();
    return /** @type {any} */ (performance).memory?.usedJSHeapSize ?? 0;
  });
  timings["JS heap after GC"] = `${(heap / 1048576).toFixed(1)} MB`;

  // A phone: nothing wider than the screen.
  const phone = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const small = await open(phone);
  const wide = await small.evaluate(() => document.documentElement.scrollWidth);
  check("at 390 px the page fits the screen", wide <= 390, `${wide}`);

  // Without JavaScript: the same deck, built with forms.
  const plain = await (await browser.newContext({ javaScriptEnabled: false })).newPage();
  await plain.goto(base);
  const before = await deck(plain);
  await plain.locator('tbody tr[data-key="salt-oracle"] td:last-child button').click();
  await plain.waitForLoadState("load");
  check("without JavaScript Add posts the deck with the card added", (await deck(plain))["salt-oracle"] === (before["salt-oracle"] ?? 0) + 1, JSON.stringify(await deck(plain)));
  await plain.locator('tbody tr[data-key="regent-of-ash"] input[type=checkbox]').check();
  await plain.locator('tbody tr[data-key="crown-of-thorns-and-glass"] input[type=checkbox]').check();
  await plain.click("button:has-text('Add selected')");
  await plain.waitForLoadState("load");
  const added = await deck(plain);
  check("Add selected posts every checked card", added["regent-of-ash"] === 1 && added["crown-of-thorns-and-glass"] === 1, JSON.stringify(added));
  await plain.fill('li[data-key="regent-of-ash"] input[type=number]', "");
  await plain.click("form button:has-text('Save')");
  await plain.waitForLoadState("load");
  check("Save posts the counts as typed: a cleared one leaves the deck", (await deck(plain))["regent-of-ash"] === undefined);
  await plain.fill('li[data-key="crown-of-thorns-and-glass"] input[type=number]', "2");
  const refusedPage = plain.waitForResponse((res) => res.url().endsWith("/deck"));
  await plain.click("form button:has-text('Save')");
  check("a refused save answers 422 with the page and the reason", (await (await refusedPage).status()) === 422 && (await plain.textContent("#save-note")) === "Not saved: You own 1 copy of Crown of Thorns and Glass.");
  await plain.fill("input[type=search]", "oracle");
  await plain.click("form[role=search] button");
  await plain.waitForLoadState("load");
  check("the filter is a GET form the server answers", plain.url().includes("q=oracle") && (await plain.locator("tbody tr:not([hidden])").count()) === 1);

  // The refused, stale, unreachable and cut-off saves above are the browser's own lines for those
  // answers.
  const unexpected = errors.filter((e) => !/Failed to load resource: (the server responded with a status of (409|422)\b|net::ERR_(FAILED|CONTENT_LENGTH_MISMATCH))/.test(e));
  check("no other errors in the console", unexpected.length === 0, unexpected.join(" | "));
} finally {
  await browser.close();
}

for (const [name, value] of Object.entries(timings)) console.log(`time ${name}: ${value}`);
console.log(failures.length ? `${failures.length} failed` : "all passed");
process.exit(failures.length ? 1 : 0);