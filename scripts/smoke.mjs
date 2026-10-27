// Browser smoke test of every runtime behavior, against the watchlist example, in Chromium, Firefox
// and WebKit (Playwright's builds: `npx playwright install chromium firefox webkit`).
// Usage: node scripts/smoke.mjs [http://localhost:18480] [chromium|firefox|webkit ...]
import { chromium, firefox, webkit } from "@playwright/test";
import http from "node:http";
import zlib from "node:zlib";

const base = process.argv[2] ?? "http://localhost:18480";
const engines = { chromium, firefox, webkit };
const names = process.argv.slice(3).length ? process.argv.slice(3) : Object.keys(engines);
const failures = [];
let engine = "";
const skipped = [];
/** A check this browser cannot make, and why. */
const skip = (/** @type {string} */ name, /** @type {string} */ why) => {
  console.log(`skip [${engine}] ${name} (${why})`);
  skipped.push(`[${engine}] ${name}`);
};
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"} [${engine}] ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(`[${engine}] ${name}`);
};

// A live stream is compressed for clients that accept it and flushed after each event: the
// first event decodes while the stream is still open.
engine = "http";
/** @param {string} accept @returns {Promise<{ coding: string | undefined, first: string }>} */
const liveEvent = (accept) =>
  new Promise((resolve, reject) => {
    const req = http.get(`${base}/live/trades`, { headers: { "accept-encoding": accept } }, (res) => {
      const coding = res.headers["content-encoding"];
      const body = coding === "br" ? res.pipe(zlib.createBrotliDecompress()) : res;
      let text = "";
      const timer = setTimeout(() => (req.destroy(), resolve({ coding, first: text })), 5000);
      body.on("data", (chunk) => {
        text += chunk.toString();
        if (text.includes("event: patch")) {
          clearTimeout(timer);
          req.destroy();
          resolve({ coding, first: text });
        }
      });
      body.on("error", () => {});
    });
    req.on("error", (e) => (e.message.includes("socket hang up") ? undefined : reject(e)));
  });
{
  const zstd = await liveEvent("gzip, deflate, br, zstd");
  check("a live stream is zstd for a client that accepts it", zstd.coding === "zstd", String(zstd.coding));
  const br = await liveEvent("gzip, br");
  check("a live stream's first event decodes before the stream ends (brotli, flushed per event)", br.coding === "br" && br.first.includes("Last trade"), `${br.coding}: ${br.first.slice(0, 60)}`);
  const plain = await liveEvent("gzip");
  check("a gzip-only client gets the stream as it is", plain.coding === undefined && plain.first.includes("event: patch"), String(plain.coding));
}

// A DNS-rebound page reaches the server under a name the app is not: 421, and nothing of the app.
{
  /** @param {string} host @returns {Promise<[number | undefined, string]>} */
  const get = (host) =>
    new Promise((resolve, reject) => {
      const req = http.get(base, { headers: { host } }, (res) => {
        let text = "";
        res.on("data", (chunk) => (text += chunk));
        res.on("end", () => resolve([res.statusCode, text]));
      });
      req.on("error", reject);
    });
  const port = new URL(base).port;
  const [rebound, body] = await get(`rebound.attacker.example:${port}`);
  check("a request for another host (DNS rebinding) is misdirected", rebound === 421 && !body.includes("<html"), `${rebound} ${body.slice(0, 40)}`);
  const [local] = await get(`localhost:${port}`);
  check("localhost is answered", local === 200, String(local));
}

for (engine of names) {
  const browser = await engines[engine].launch({ headless: true });
  try {
    await run(browser);
  } finally {
    await browser.close();
  }
}
if (skipped.length) console.log(`\n${skipped.length} skipped`);
console.log(failures.length ? `\n${failures.length} failed:\n${failures.join("\n")}` : "\nall passed");
process.exit(failures.length ? 1 : 0);

/** @param {import("@playwright/test").Browser} browser */
async function run(browser) {
  const page = await browser.newPage();
  const errors = [];
  // Firefox logs an open stream that a full page load cuts (data-reload, an error page, a section
  // stylesheet that failed), and the stream the status check drops on purpose, as an error, in one
  // of two wordings.
  const cut = /was interrupted while the page was loading|can.t establish a connection to the server at \S+\/live\/(trades|flaky)/;
  page.on("console", (m) => m.type() === "error" && !cut.test(m.text()) && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(e.message));
  // Every EventSource the page opens, to see which are still open.
  await page.addInitScript(() => {
    const sources = ((/** @type {any} */ (window)).__sources = /** @type {EventSource[]} */ ([]));
    window.EventSource = class extends EventSource {
      /** @param {ConstructorParameters<typeof EventSource>} args */
      constructor(...args) {
        super(...args);
        sources.push(this);
      }
    };
  });
  /** @param {string} part */
  const openStreams = (part) =>
    page.evaluate((p) => (/** @type {any} */ (window)).__sources.filter((/** @type {EventSource} */ s) => s.url.includes(p) && s.readyState !== EventSource.CLOSED).length, part);
  const openFeeds = () => openStreams("/feed/");
  /** Every feed source the page has opened, oldest first: true while open. */
  const feedSources = () =>
    page.evaluate(() => (/** @type {any} */ (window)).__sources.filter((/** @type {EventSource} */ s) => s.url.includes("/feed/")).map((/** @type {EventSource} */ s) => s.readyState !== EventSource.CLOSED));
  // The next /watch post waits this long before reaching the server, or fails as if offline.
  let delayWatch = 0;
  let failWatch = false;
  /** @type {string[]} */
  const abandoned = [];
  /** The requests failed on purpose, which are not abandoned. */
  const offline = new Set();
  await page.route("**/watch", async (route) => {
    const wait = delayWatch;
    delayWatch = 0;
    if (failWatch) {
      failWatch = false;
      offline.add(route.request());
      return route.abort("internetdisconnected");
    }
    if (wait) await new Promise((resolve) => setTimeout(resolve, wait));
    await route.continue().catch(() => {});
  });
  // Navigation requests the runtime sends (`corten: nav`), prefetches included.
  /** @type {string[]} */
  const navRequests = [];
  page.on("request", (request) => request.headers()["corten"] === "nav" && navRequests.push(request.url()));
  page.on("requestfailed", (request) => {
    const error = request.failure()?.errorText ?? "";
    if (request.url().endsWith("/watch") && !offline.has(request)) abandoned.push(error);
  });

  // Playwright's WebKit with the Navigation API (its Linux build) loses track of the page once the
  // runtime intercepts a navigation: every later action waits for a navigation that never ends.
  // WebKit's swaps are checked from inside the page here; the rest of the run hides the API, so
  // its navigations are full page loads, the documented fallback.
  if (engine === "webkit") {
    const probe = await browser.newPage();
    await probe.goto(base);
    if (await probe.evaluate(() => "navigation" in window)) {
      const seen = await probe.evaluate(async () => {
        const until = async (/** @type {() => boolean} */ done) => {
          for (let i = 0; i < 100 && !done(); i += 1) await new Promise((resolve) => setTimeout(resolve, 50));
          return done();
        };
        (/** @type {any} */ (window)).__marker = 1;
        /** @type {HTMLAnchorElement} */ (document.querySelector('header a[href="/news"]')).click();
        const news = await until(() => document.title === "News | Corten");
        const note = document.querySelector(".disclaimer__note");
        const border = note ? getComputedStyle(note).borderInlineStartWidth : "";
        history.back();
        const back = await until(() => document.title === "Watchlist | Corten");
        return { news, border, back, kept: (/** @type {any} */ (window)).__marker === 1, path: location.pathname };
      });
      check(
        "WebKit swaps a page in with its section stylesheet loaded, and back restores the last one",
        seen.news && seen.border === "4px" && seen.back && seen.kept && seen.path === "/",
        JSON.stringify(seen),
      );
    }
    await probe.close();
    await page.addInitScript(() => {
      delete (/** @type {any} */ (Window.prototype)).navigation;
      delete (/** @type {any} */ (window)).navigation;
    });
  }

  await page.goto(base);
  // Without the Navigation API every navigation is a full page load, the documented fallback: the
  // checks of what a swap keeps check that instead, or are skipped.
  const swaps = await page.evaluate(() => "navigation" in window);
  const noSwaps = "no Navigation API: navigations are full page loads";
  // Playwright's WebKit stores a Secure cookie from http://localhost but never sends it back.
  const signsIn = engine !== "webkit";
  const live = "[data-scope=live-prices]";
  await page.waitForSelector(`${live}[data-status="live"]`, { timeout: 5000 }).catch(() => {});
  check("on:connect runs the script's watch(), which subscribes to the feed", (await page.getAttribute(live, "data-status")) === "live");

  const prices = () => page.$$eval('[data-ref="price"]', (cells) => cells.map((c) => c.textContent));
  const before = await prices();
  await page.waitForFunction((b) => JSON.stringify([...document.querySelectorAll('[data-ref="price"]')].map((c) => c.textContent)) !== b, JSON.stringify(before), { timeout: 4000 }).catch(() => {});
  check("live patches update prices", JSON.stringify(await prices()) !== JSON.stringify(before), `${before.join(", ")} -> ${(await prices()).join(", ")}`);
  // Text a script computes, from the catalog: on:connect={js::watch(.., updates, t.js().prices_updates())}.
  const counted = await page.textContent(`${live} [data-ref="updates"]`);
  check("a script fills in a message of the page's locale", /^Price updates: [1-9]\d*$/.test(counted ?? ""), String(counted));

  // A handler with per-component state: on:click={js::pause(scope, target, price, change)}.
  check("the script reveals its controls", await page.isVisible(`${live} button`));
  await page.click(`${live} button`);
  const paused = await prices();
  await page.waitForTimeout(2500);
  check(
    "a handler changes the component's state (pause holds the prices)",
    (await page.getAttribute(`${live} button`, "aria-pressed")) === "true" && JSON.stringify(await prices()) === JSON.stringify(paused),
  );
  await page.click(`${live} button`);
  // The page's prices by stock id, written by the script with the page's money format
  // (t.js().money(USD)), and the feed's, which the server wrote.
  // Today's change too: its trend, and the sign of its text.
  const shown = () =>
    page.$$eval('[data-ref="price"]', (cells) =>
      JSON.stringify(
        cells
          .map((c) => {
            const id = c.getAttribute("data-key");
            const change = document.querySelector(`[data-ref="change"][data-key="${id}"]`);
            return [id, c.textContent, change?.getAttribute("data-trend")];
          })
          .sort(),
      ),
    );
  // The feed has the whole market; the page, the stocks on the list.
  const latest = async () => {
    const ids = await page.$$eval('[data-ref="price"]', (cells) => cells.map((c) => c.getAttribute("data-key")));
    const snap = await (await fetch(`${base}/feed/stocks`, { headers: { accept: "application/json" } })).json();
    /** @param {number} bp */
    const trend = (bp) => (bp > 0 ? "up" : bp < 0 ? "down" : "flat");
    return JSON.stringify(
      snap.rows
        .filter((/** @type {{ id: string }} */ r) => ids.includes(r.id))
        .map((/** @type {{ id: string, price: string, change_bp: number }} */ r) => [r.id, r.price, trend(r.change_bp)])
        .sort(),
    );
  };
  let resumed = false;
  for (let i = 0; i < 20 && !resumed; i += 1) {
    resumed = (await shown()) === (await latest());
    if (!resumed) await page.waitForTimeout(150);
  }
  check("resuming shows the latest prices and changes", resumed && (await page.getAttribute(`${live} button`, "aria-pressed")) === "false", `${await shown()} vs ${await latest()}`);
  // A change's text is a percent whose sign is its trend's.
  const changes = await page.$$eval('[data-ref="change"]', (cells) => cells.map((c) => [c.textContent ?? "", c.getAttribute("data-trend")]));
  check(
    "today's change is a percent with its trend",
    changes.length > 0 &&
      changes.every(([text, trend]) => /%$/.test(text) && (trend === "down") === /^[-\u2212]/.test(text) && (trend === "flat") === !/[1-9]/.test(text)),
    JSON.stringify(changes),
  );

  // Many rows joining at once are scanned for ref names one query per name: a name that only the
  // last of them has still refreshes the refs the component has cached (pause just resolved them).
  await page.click(`${live} button`);
  await page.evaluate((live) => {
    const body = /** @type {HTMLTableSectionElement} */ (document.querySelector(`${live} tbody`));
    const old = /** @type {HTMLElement} */ (body.querySelector('[data-ref="price"][data-key="ACME"]'));
    old.removeAttribute("data-ref");
    old.id = "old-s1";
    const rows = document.createDocumentFragment();
    for (let i = 0; i < 20; i += 1) {
      const tr = document.createElement("tr");
      tr.className = "wide";
      tr.innerHTML = i < 19 ? '<td data-ref="note">n</td>' : '<td data-ref="price" data-key="ACME" id="new-s1">-</td>';
      rows.append(tr);
    }
    body.append(rows);
  }, live);
  await page.click(`${live} button`);
  const repainted = await page.textContent("#new-s1");
  check("a ref name only the last row of a 20-row insert has is found", repainted !== "-", String(repainted));
  await page.evaluate(() => {
    for (const tr of Array.from(document.querySelectorAll("tr.wide"))) tr.remove();
    const old = /** @type {HTMLElement} */ (document.getElementById("old-s1"));
    old.setAttribute("data-ref", "price");
    old.removeAttribute("id");
  });

  // A marker on window survives client navigations and patches, and disappears on a full load.
  await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 1));
  const sourcesBefore = await feedSources();
  await page.selectOption("select[name=id]", "HLTH");
  await page.click("#watch-form button");
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent?.startsWith("Watching Halcyon Health"));
  check("form patch replaces the message by id", true);
  check("watching a stock adds its row, and the form no longer offers it", (await page.$('[data-ref="price"][data-key="HLTH"]')) !== null && (await page.$('select[name=id] option[value="HLTH"]')) === null);
  await page.waitForSelector(`${live}[data-status="live"]`, { timeout: 5000 }).catch(() => {});
  const sources = await feedSources();
  check(
    "the list a patch put in subscribes again, and the one it replaced closes its stream",
    sources.length === sourcesBefore.length + 1 && sources.at(-1) === true && sources.slice(0, -1).every((open) => !open),
    `${JSON.stringify(sourcesBefore)} -> ${JSON.stringify(sources)}`,
  );
  // The list is kept on the server, under the browser's cookie: the page read again shows it.
  if (signsIn) {
    const kept = await page.evaluate(async () => (await (await fetch("/")).text()).includes('data-ref="price" class="live-prices__amount" data-key="HLTH"'));
    check("the list is the same after a reload", kept);
  } else skip("the list is the same after a reload", "Playwright's WebKit never sends a Secure cookie back over http");
  check("form patch keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));
  // A row's Remove button submits the watch form to /unwatch.
  await page.click('button[formaction="/unwatch"][value="HLTH"]');
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent === "Stopped watching Halcyon Health.", null, { timeout: 4000 }).catch(() => {});
  check(
    "unwatching a stock takes its row away and offers it again",
    (await page.$('[data-ref="price"][data-key="HLTH"]')) === null && (await page.$('select[name=id] option[value="HLTH"]')) !== null,
    String(await page.textContent("#watch-message")),
  );

  // <corten-live src>: the server streams fragments and the runtime swaps them in by id.
  const firstTrade = await page.textContent("#last-trade");
  await page.waitForFunction((t) => document.getElementById("last-trade")?.textContent !== t, firstTrade, { timeout: 4000 }).catch(() => {});
  check("<corten-live> swaps in the fragments the server streams", ((await page.textContent("#last-trade")) ?? "").startsWith("Last trade: "), `${firstTrade} -> ${await page.textContent("#last-trade")}`);

  // A submit the network fails: the form stays, and the page hears about it.
  const failed = page.evaluate(
    () => new Promise((resolve) => document.addEventListener("corten:error", (e) => resolve(/** @type {CustomEvent} */ (e).detail.status), { once: true })),
  );
  failWatch = true;
  const messageBefore = await page.textContent("#watch-message");
  await page.click("#watch-form button");
  check("a submit the network fails dispatches corten:error with status 0", (await failed) === 0);
  check(
    "a failed submit leaves the page and the form as they were",
    (await page.textContent("#watch-message")) === messageBefore && (await page.getAttribute("#watch-form", "aria-busy")) === null,
  );

  // A submit key (SubmitKey::field) is sent again until the submission has an answer, then renewed
  // as another UUID v4.
  const zero = "00000000-0000-4000-8000-000000000000";
  await page.evaluate((zero) =>
    document.querySelector("main")?.insertAdjacentHTML(
      "beforeend",
      `<form id="keyed" method="post" action="/watch" data-patch><input type="hidden" name="id" value="ACME"><input type="hidden" name="submit" data-submit value="${zero}"><button>Watch</button></form>`,
    ), zero);
  /** @type {(string | null)[]} */
  const keys = [];
  /** @param {import("@playwright/test").Request} r */
  const keyOf = (r) => r.method() === "POST" && r.url().endsWith("/watch") && keys.push(new URLSearchParams(r.postData() ?? "").get("submit"));
  page.on("request", keyOf);
  const keyedFailed = page.evaluate(() => new Promise((resolve) => document.addEventListener("corten:error", resolve, { once: true })));
  failWatch = true;
  await page.click("#keyed button");
  await keyedFailed;
  const kept = await page.inputValue("#keyed [name=submit]");
  await page.click("#keyed button");
  await page.waitForFunction((zero) => /** @type {HTMLInputElement | null} */ (document.querySelector("#keyed [name=submit]"))?.value !== zero, zero, { timeout: 5000 }).catch(() => {});
  const renewed = await page.inputValue("#keyed [name=submit]");
  page.off("request", keyOf);
  check(
    "a submit key is sent again after a failed submit and renewed once the submit is answered",
    kept === zero && keys.length === 2 && keys.every((k) => k === zero) && /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(renewed) && renewed !== zero,
    `${keys.join(", ")} -> ${renewed}`,
  );
  await page.evaluate(() => document.querySelector("#keyed")?.remove());

  // A submit button's formaction and formmethod win over the form's, as in a plain post.
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML(
      "beforeend",
      '<form id="via-button" method="get" action="/nowhere" data-patch><input type="hidden" name="id" value="ACME"><button formmethod="post" formaction="/watch">Watch</button></form>',
    ),
  );
  await page.click("#via-button button");
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent?.startsWith("Watching Acme Corp"), null, { timeout: 4000 }).catch(() => {});
  check("a submit button's formaction and formmethod are used", ((await page.textContent("#watch-message")) ?? "").startsWith("Watching Acme Corp"));

  // Controls named like the form's properties (`<button name="action">` makes `form.action` that
  // button) do not change where or how the runtime posts it.
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML(
      "beforeend",
      '<form id="shadowed" method="post" action="/watch" data-patch><input type="hidden" name="id" value="GLBX"><input type="hidden" name="method" value="x"><input type="hidden" name="enctype" value="x"><input type="hidden" name="elements" value="x"><input type="hidden" name="setAttribute" value="x"><button name="action" value="go">Watch</button></form>',
    ),
  );
  const shadowedPosts = /** @type {string[]} */ ([]);
  // The answer's list subscribes to its feed again (`GET /feed/stocks`), which is not the post.
  const onShadowed = (/** @type {import("@playwright/test").Request} */ r) =>
    r.resourceType() === "fetch" && !new URL(r.url()).pathname.startsWith("/feed/") && shadowedPosts.push(`${r.method()} ${new URL(r.url()).pathname}`);
  page.on("request", onShadowed);
  await page.click("#shadowed button");
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent?.startsWith("Watching Globex"), null, { timeout: 4000 }).catch(() => {});
  page.off("request", onShadowed);
  check(
    "controls named action, method, id and the like do not change the post",
    ((await page.textContent("#watch-message")) ?? "").startsWith("Watching Globex") && shadowedPosts.join() === "POST /watch",
    `${shadowedPosts.join()}; ${await page.textContent("#watch-message")}`,
  );
  await page.evaluate(() => document.getElementById("shadowed")?.remove());

  // The latest submit wins: a slow earlier reply never overwrites a later one.
  delayWatch = 1200;
  await page.selectOption("select[name=id]", "PNTR");
  await page.click("#watch-form button");
  await page.selectOption("select[name=id]", "VTGO");
  await page.click("#watch-form button");
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent?.startsWith("Watching Vertigo Robotics"));
  await page.waitForTimeout(1600);
  check(
    "a newer submit replaces a pending one",
    ((await page.textContent("#watch-message")) ?? "").startsWith("Watching Vertigo Robotics") && abandoned.length === 1,
    `${await page.textContent("#watch-message")}; abandoned: ${abandoned.join()}`,
  );
  check("the form is not left busy", (await page.getAttribute("#watch-form", "aria-busy")) === null);

  const current = () => page.$$eval("a[data-nav][aria-current=page]", (links) => links.map((l) => l.getAttribute("href")).join());
  check("the shell marks the current link", (await current()) === "/");
  check("one feed stream while the component is on the page", (await openFeeds()) === 1);
  // The news page's disclaimer is in an area (Area::components): its stylesheet loads before the page is
  // swapped in, so the note is styled the moment it joins the page.
  await page.evaluate(() => {
    new MutationObserver((_, observer) => {
      const note = document.querySelector(".disclaimer__note");
      if (!note) return;
      (/** @type {any} */ (window)).__noteBorder = getComputedStyle(note).borderInlineStartWidth;
      observer.disconnect();
    }).observe(document.body, { childList: true, subtree: true });
  });
  const noteBorder = () => page.$eval(".disclaimer__note", (note) => getComputedStyle(note).borderInlineStartWidth);
  const styleLinks = () => page.$$eval("link[data-corten-style]", (links) => links.length);
  check("a page outside the section does not link its stylesheet", (await styleLinks()) === 0);
  // A navigation abandons a pending submit, whose reply was for the page being left.
  delayWatch = 1200;
  await page.click("#watch-form button");
  await page.click('header a[href="/news"]');
  await page.waitForFunction(() => document.title === "News | Corten");
  await page.waitForTimeout(1600);
  check("navigating away abandons a pending submit", abandoned.length === 2, abandoned.join());
  check("the feed stream closes when its component leaves the page (its signal aborts)", (await openFeeds()) === 0);
  check("the <corten-live> stream closes when it leaves the page", (await openStreams("/live/trades")) === 0);
  check("client navigation moves aria-current in the shell", (await current()) === "/news");
  check("main is not left busy", (await page.getAttribute("main", "aria-busy")) === null);
  check("client navigation swaps <main>", (await page.textContent("main h1")) === "Market news");
  if (swaps) {
    const border = await page.evaluate(() => (/** @type {any} */ (window)).__noteBorder);
    check("a section's stylesheet loads before its page is swapped in", border === "4px", border);
  } else {
    check("a section's page links its stylesheet", (await noteBorder()) === "4px" && (await styleLinks()) === 1);
  }
  if (swaps) check("client navigation keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));
  else check("without the Navigation API a link loads the whole page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === undefined));
  check("navigation updates the URL", new URL(page.url()).pathname === "/news");
  // A filter applied on change (data-keep-place): the page changes under the user, who stays
  // where they were, on the box they checked.
  if (swaps) {
    const entry = await page.evaluate(() => navigation.currentEntry.key);
    const size = page.viewportSize();
    await page.setViewportSize({ width: 360, height: 160 });
    // The user opens both groups of the filter rail (which scrolls on its own), scrolls to the
    // box and checks it. The server draws a group open only when its box is checked.
    const rail = ".news-filter__rail";
    const groups = () => page.$$eval(`${rail} details`, (all) => all.map((d) => /** @type {HTMLDetailsElement} */ (d).open).join());
    await page.evaluate((rail) => {
      for (const group of Array.from(document.querySelectorAll(`${rail} details`))) /** @type {HTMLDetailsElement} */ (group).open = true;
      const box = /** @type {HTMLInputElement} */ (document.querySelector("main input[name=mine]"));
      box.scrollIntoView({ block: "end" });
      box.focus();
    }, rail);
    // Read once the scroll has settled: Firefox rounds a scroll to a fractional place a frame later.
    const before = await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => done(scrollY)))));
    const railBefore = await page.$eval(rail, (r) => r.scrollTop);
    await page.keyboard.press("Space");
    await page.waitForFunction(() => location.search === "?mine=on" && document.getElementById("stories-title")?.textContent === "Stories about your stocks");
    const after = await page.evaluate(() => ({
      y: scrollY,
      focused: document.activeElement?.getAttribute("name"),
      checked: /** @type {HTMLInputElement} */ (document.activeElement).checked,
      marker: (/** @type {any} */ (window)).__marker,
    }));
    const railAfter = await page.$eval(rail, (r) => r.scrollTop);
    check("the rail keeps its own scroll", railBefore > 0 && railAfter === railBefore, `${railBefore} -> ${railAfter}`);
    check("a group the user opened stays open though the server drew it closed", (await groups()) === "true,true", await groups());
    check("a data-keep-place form swaps the page in place", after.marker === 1, JSON.stringify(after));
    check("it keeps the scroll position", before > 0 && after.y === before, `${before} -> ${after.y}`);
    check("focus stays on the control the user changed", after.focused === "mine" && after.checked, JSON.stringify(after));
    await page.evaluate(() => /** @type {HTMLInputElement} */ (document.querySelector("main input[name=mine]")).focus());
    await page.keyboard.press("Space");
    await page.waitForFunction(() => location.search === "");
    check("unchecking applies again, focus still on it", await page.evaluate(() => document.activeElement?.getAttribute("name") === "mine" && scrollY > 0));
    check("the group of the box unchecked stays open, so focus is not lost to the body", (await groups()) === "true,true", await groups());
    // Clicked from the page, so Playwright does not scroll to the link first.
    await page.evaluate(() => /** @type {HTMLAnchorElement} */ (document.querySelector(`header a[href="/news"]`)).click());
    await page.waitForFunction(() => location.search === "" && location.href.endsWith("/news") && scrollY === 0, null, { timeout: 5000 }).catch(() => {});
    check("a plain link still goes to the top", await page.evaluate(() => scrollY === 0));
    // A write that sends the browser back to a place on the page (`News { .. }.at(BriefingId)`,
    // below the fold): the address keeps `#briefing`, the page is fetched again and the form is in
    // view. A second time, when only the `#id` matches the address, the page is still fetched again.
    {
      let loads = 0;
      const onNews = (/** @type {import("@playwright/test").Request} */ r) => r.method() === "GET" && new URL(r.url()).pathname === "/news" && loads++;
      page.on("request", onNews);
      const back = () =>
        page
          .waitForFunction(() => {
            const form = document.getElementById("briefing");
            const top = form?.getBoundingClientRect().top ?? -1;
            // Layout in fractions of a pixel can leave the top a fraction above the scroll position.
            return location.hash === "#briefing" && !document.querySelector("main[aria-busy]") && scrollY > 0 && top > -1 && top < innerHeight;
          }, null, { timeout: 5000 })
          .then(() => true, () => false);
      const thank = () =>
        page.evaluate(() => {
          /** @type {HTMLInputElement} */ (document.querySelector("#briefing [name=email]")).value = "ada@example.com";
          /** @type {HTMLButtonElement} */ (document.querySelector("#briefing button")).click();
        });
      await thank();
      const first = await back();
      check("a redirect to a place keeps its #id and scrolls to it", first && loads === 1, `${await page.evaluate(() => [location.href, scrollY])} loads ${loads}`);
      await page.evaluate(() => scrollTo(0, 0));
      await thank();
      const second = await back();
      check("the same place again fetches the page again", second && loads === 2, `loads ${loads}`);
      check("both kept the document", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));
      page.off("request", onNews);
    }
    if (size) await page.setViewportSize(size);
    // Back over the four entries this added (checked, unchecked, the link, the place) to the
    // news page the watchlist led to.
    await page.evaluate(() => history.go(-4));
    await page.waitForFunction((key) => navigation.currentEntry.key === key && !document.querySelector("main[aria-busy]"), entry);
  } else {
    for (const name of [
      "a data-keep-place form swaps the page in place",
      "it keeps the scroll position",
      "focus stays on the control the user changed",
      "the rail keeps its own scroll",
      "a group the user opened stays open though the server drew it closed",
    ])
      skip(name, noSwaps);
  }
  // The rail scrolled in the same frame as the change, so the swap lands before the `scroll` event
  // that would name it (a slow machine, or a control focus scrolled into view): it keeps its scroll
  // all the same. In a page of its own, so this page's history stays as it is.
  if (swaps) {
    const quick = await browser.newPage({ viewport: { width: 360, height: 160 } });
    await quick.goto(`${base}/news`);
    const top = await quick.evaluate(() => {
      const rail = /** @type {Element} */ (document.querySelector(".news-filter__rail"));
      for (const group of Array.from(rail.querySelectorAll("details"))) /** @type {HTMLDetailsElement} */ (group).open = true;
      const box = /** @type {HTMLInputElement} */ (document.querySelector("main input[name=mine]"));
      box.scrollIntoView({ block: "end" });
      box.focus();
      box.click();
      return rail.scrollTop;
    });
    await quick.waitForFunction(() => location.search === "?mine=on" && document.getElementById("stories-title")?.textContent === "Stories about your stocks");
    const after = await quick.$eval(".news-filter__rail", (r) => r.scrollTop);
    check("the rail keeps a scroll made in the same frame as the change", top > 0 && after === top, `${top} -> ${after}`);
    await quick.close();
  } else skip("the rail keeps a scroll made in the same frame as the change", noSwaps);
  // Load more (view::Append): the answer adds only the new rows; the ones the page has stay. In a
  // page of its own, so it runs in every engine and leaves this page's history as it is.
  {
    const more = await browser.newPage();
    const moreNavs = [];
    more.on("request", (request) => request.headers()["corten"] === "nav" && moreNavs.push(request.url()));
    await more.goto(`${base}/news`);
    await more.waitForFunction(() => document.querySelectorAll("#stories > li").length === 3);
    await more.evaluate(() => ((/** @type {any} */ (document.querySelector("#stories > li"))).__kept = true));
    await more.click("#load-more button");
    await more.waitForFunction(() => document.querySelectorAll("#stories > li").length === 6);
    const grown = await more.evaluate(() => ({
      rows: Array.from(document.querySelectorAll("#stories h3"), (h) => h.textContent).join("|"),
      kept: (/** @type {any} */ (document.querySelector("#stories > li"))).__kept === true,
      search: location.search,
    }));
    check(
      "Load more appends the next rows in order",
      grown.rows ===
        "Acme Corp raises its full-year outlook|Globex wins approval for its northern pipeline|Initech moves its reporting software to subscriptions|Halcyon Health's flu vaccine enters its final trial|Pantry & Co recalls one batch of cereal over a labeling error|Vertigo Robotics signs a warehouse automation deal",
      grown.rows,
    );
    check("the rows the page had are the same elements", grown.kept);
    check("Load more is a patch, not a navigation", moreNavs.length === 0 && grown.search === "", `${moreNavs.length} nav(s), ${grown.search}`);
    await more.click("#load-more button");
    await more.waitForFunction(() => document.querySelectorAll("#stories > li").length === 9);
    check("the last page hides the button", await more.locator("#load-more button").isHidden());
    // A sector's box: the stories of that sector alone, three of the nine. (A navigation the
    // runtime intercepts stalls Playwright's WebKit, above.)
    if (swaps) {
      // The box is in a closed group of the rail; clicked from the page.
      await more.evaluate(() => /** @type {HTMLInputElement} */ (document.querySelector("main input[name=sector][value=energy]")).click());
      await more.waitForFunction(() => location.search === "?sector=energy" && !document.querySelector("main[aria-busy]"));
      const energy = await more.$$eval("#stories > li", (items) => items.map((li) => li.querySelector("p:last-child a")?.textContent));
      check("a sector's box lists only its stories", energy.join() === "Energy,Energy,Energy" && (await more.locator("#load-more button").isHidden()), energy.join());
    } else skip("a sector's box lists only its stories", noSwaps);
    await more.close();
  }
  // A story's link, a route that redirects to a place on another site (`/news/{id}/read`,
  // `Elsewhere::at`): the runtime's fetch cannot follow it (connect-src, CORS), so the server names
  // the address and the browser goes there, in one request to the app and without a console error,
  // the hover's prefetch included.
  {
    const away = await browser.newPage();
    const awayErrors = [];
    away.on("console", (m) => m.type() === "error" && awayErrors.push(m.text()));
    await away.route("https://news.example.com/**", (route) => route.fulfill({ contentType: "text/html", body: "<title>Elsewhere</title>" }));
    let asked = 0;
    away.on("request", (r) => new URL(r.url()).pathname === "/news/acme-outlook/read" && asked++);
    await away.goto(`${base}/news`);
    await away.hover(`main a[href="/news/acme-outlook/read"]`);
    await away.waitForTimeout(300);
    // The browser's request to the other site is what matters, not that site's load (WebKit does
    // not always hand a redirected navigation to the route above), so the click does not wait.
    // The request leaves out the place (`#guidance`), which only the browser's address keeps.
    const left = away.waitForRequest((r) => r.url() === "https://news.example.com/markets/acme-outlook", { timeout: 5000 }).then(() => true, () => false);
    await away.evaluate(() => /** @type {HTMLAnchorElement} */ (document.querySelector(`main a[href="/news/acme-outlook/read"]`)).click());
    const arrived = await left;
    check("a redirect to another site leaves the app in one request", arrived && asked === 1, `left ${arrived} requests ${asked}`);
    // The browser's address keeps the place.
    if (engine !== "webkit") {
      const placed = await away.waitForURL("https://news.example.com/markets/acme-outlook#guidance", { timeout: 5000 }).then(() => true, () => false);
      check("the address keeps the place on the other site", placed, away.url());
    } else skip("the address keeps the place on the other site", "WebKit does not always hand a redirected navigation to the route");
    check("without a console error", awayErrors.length === 0, awayErrors.join(" | "));
    await away.close();
  }

  await page.goBack();
  await page.waitForFunction(() => document.title === "Watchlist | Corten");
  await page.waitForSelector(`${live}[data-status="live"]`, { timeout: 5000 }).catch(() => {});
  check("back navigation restores the watchlist and on:connect resubscribes", (await page.getAttribute(live, "data-status")) === "live");
  if (swaps) {
    check("a section's stylesheet stays when the page leaves the section", (await styleLinks()) === 1);
    await page.goForward();
    await page.waitForFunction(() => document.title === "News | Corten");
    check("coming back into the section does not link its stylesheet twice", (await styleLinks()) === 1 && (await noteBorder()) === "4px");
    await page.goBack();
    await page.waitForFunction(() => document.title === "Watchlist | Corten");
    await page.waitForSelector(`${live}[data-status="live"]`, { timeout: 5000 }).catch(() => {});
  } else {
    for (const name of ["a section's stylesheet stays when the page leaves the section", "coming back into the section does not link its stylesheet twice"]) skip(name, noSwaps);
  }
  // The submit failed on purpose above; the browser logs that request.
  const unexpected = errors.filter((e) => !e.includes("ERR_INTERNET_DISCONNECTED"));
  check("no console errors", unexpected.length === 0, unexpected.join(" | "));
  errors.length = 0;

  // /account is guarded (Area::guard): signed out, a navigation there follows the redirect to the
  // watchlist, where the sign-in button is.
  await page.goto(`${base}/news`);
  await page.click('header a[href="/account"]');
  await page.waitForFunction(() => document.title === "Watchlist | Corten", null, { timeout: 5000 }).catch(() => {});
  check(
    "a guarded page sends someone signed out to sign in",
    (await page.title()) === "Watchlist | Corten" && new URL(page.url()).pathname === "/",
    page.url(),
  );

  // Signing in changes the shell's context, so the redirect loads the whole document.
  if (signsIn) {
    await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 2));
    await page.click("header form[data-patch] button");
    await page.waitForFunction(() => document.querySelector("header")?.textContent?.includes("Signed in as Ada"), null, { timeout: 5000 }).catch(() => {});
    check("signing in shows the viewer in the shell", ((await page.textContent("header")) ?? "").includes("Signed in as Ada"));
    check("a new shell context loads the whole document", await page.evaluate(() => (/** @type {any} */ (window)).__marker === undefined));
    await page.click('header a[href="/account"]');
    await page.waitForFunction(() => document.title === "Account | Corten", null, { timeout: 5000 }).catch(() => {});
    check("a guarded page opens for someone signed in", ((await page.textContent("main")) ?? "").includes("Signed in as Ada."));
  } else {
    skip("signing in shows the viewer in the shell", "the browser does not send Secure cookies over http");
    skip("a guarded page opens for someone signed in", "the browser does not send Secure cookies over http");
  }
  await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 3));
  await page.click('header a[href="/news"]');
  await page.waitForFunction(() => document.title === "News | Corten");
  if (swaps) check("same context: navigation swaps again", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 3));
  else skip("same context: navigation swaps again", noSwaps);

  // A <corten-live> whose stream finishes (here one fragment, then `end`) applies it and stays
  // closed: no reconnect after the browser's retry delay (about 3 s in Chromium). Without the
  // Navigation API the page above was a full load, whose title is in before <main> is.
  await page.waitForLoadState("domcontentloaded");
  let onceRequests = 0;
  await page.route("**/live/once", (route) => {
    onceRequests += 1;
    return route.fulfill({
      status: 200,
      headers: { "content-type": "text/event-stream" },
      body: 'event: patch\ndata: <p id="once">Ready</p>\n\nevent: end\ndata: end\n\n',
    });
  });
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML("beforeend", '<corten-live id="once-live" src="/live/once"><p id="once">Waiting</p></corten-live>'),
  );
  await page.waitForFunction(() => document.querySelector("#once")?.textContent === "Ready", null, { timeout: 5000 }).catch(() => {});
  await page.waitForTimeout(4500);
  check(
    "a <corten-live> stream that ends is applied once and not reopened",
    (await page.textContent("#once")) === "Ready" && onceRequests === 1 && (await openStreams("/live/once")) === 0 && (await page.getAttribute("#once-live", "status")) === "ended",
    `${await page.textContent("#once")}, ${onceRequests} requests`,
  );
  await page.evaluate(() => document.querySelector("#once-live")?.remove());

  // <corten-live> shows its connection: connecting, live once open, reconnecting (with when it
  // dropped) when the stream breaks, closed when the server answers it with no stream.
  let flaky = 0;
  await page.route("**/live/flaky", (route) =>
    (flaky += 1) === 1
      ? route.fulfill({ status: 200, contentType: "text/event-stream", body: 'retry: 300\nevent: patch\ndata: <p id="flaky">One</p>\n\n' })
      : route.fulfill({ status: 204, body: "" }),
  );
  await page.evaluate(() => {
    const w = /** @type {any} */ (window);
    w.__liveStates = [];
    w.__statusEvents = [];
    // Each change also dispatches a bubbling corten-status, which a handler takes with on:corten-status.
    document.addEventListener("corten-status", (event) => {
      const live = /** @type {Element} */ (event.target);
      if (live.id === "flaky-live") w.__statusEvents.push(`${live.getAttribute("status")}${live.hasAttribute("stale-since") ? "+stale" : ""}`);
    });
    document.querySelector("main")?.insertAdjacentHTML("beforeend", '<corten-live id="flaky-live" src="/live/flaky"><p id="flaky">Waiting</p></corten-live>');
    const el = /** @type {Element} */ (document.querySelector("#flaky-live"));
    const record = () => w.__liveStates.push(`${el.getAttribute("status")}${el.hasAttribute("stale-since") ? "+stale" : ""}`);
    record();
    new MutationObserver(record).observe(el, { attributes: true, attributeFilter: ["status"] });
  });
  await page.waitForFunction(() => document.querySelector("#flaky-live")?.getAttribute("status") === "closed", null, { timeout: 5000 }).catch(() => {});
  const liveStates = await page.evaluate(() => (/** @type {any} */ (window)).__liveStates.join(" "));
  const staleSince = await page.getAttribute("#flaky-live", "stale-since");
  check(
    "<corten-live> shows connecting, live, reconnecting since it dropped, then closed",
    liveStates === "connecting live reconnecting+stale closed+stale" && !Number.isNaN(Date.parse(staleSince ?? "")) && (await page.textContent("#flaky")) === "One",
    `${liveStates}; stale since ${staleSince}`,
  );
  const statusEvents = await page.evaluate(() => (/** @type {any} */ (window)).__statusEvents.join(" "));
  check("each status change dispatches a bubbling corten-status", statusEvents === liveStates, `${statusEvents} against ${liveStates}`);
  await page.evaluate(() => document.querySelector("#flaky-live")?.remove());
  await page.unroute("**/live/flaky");

  // A <corten-live since> sends the page's cursor on its first connection; a fragment's id is the
  // cursor a reconnect sends back as Last-Event-ID, so the stream resumes there.
  /** @type {{ since: string | null, last: string | null }[]} */
  const resumes = [];
  await page.route("**/live/resume*", async (route) => {
    const req = route.request();
    resumes.push({ since: new URL(req.url()).searchParams.get("since"), last: (await req.allHeaders())["last-event-id"] ?? null });
    // The first answer ends without `end`, as a server that shuts down does: the browser reconnects.
    const body =
      resumes.length === 1
        ? 'retry: 100\n\nevent: patch\nid: c8\ndata: <p id="resume">At 8</p>\n\n'
        : 'event: patch\nid: c9\ndata: <p id="resume">At 9</p>\n\nevent: end\ndata: end\n\n';
    return route.fulfill({ status: 200, headers: { "content-type": "text/event-stream" }, body });
  });
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML("beforeend", '<corten-live id="resume-live" src="/live/resume" since="c7"><p id="resume">At 7</p></corten-live>'),
  );
  await page.waitForFunction(() => document.querySelector("#resume")?.textContent === "At 9", null, { timeout: 8000 }).catch(() => {});
  // Chromium adds Last-Event-ID below what Playwright's routing sees (a real server gets it, the
  // same as from the other two), so there the check is that the stream reconnected and went on.
  const headerSeen = engine !== "chromium";
  check(
    "<corten-live since> starts from the page's cursor and resumes from the last fragment's",
    (await page.textContent("#resume")) === "At 9" &&
      resumes.length === 2 &&
      resumes[0].since === "c7" &&
      resumes[0].last === null &&
      (!headerSeen || resumes[1].last === "c8"),
    JSON.stringify(resumes),
  );
  await page.evaluate(() => document.querySelector("#resume-live")?.remove());

  // A patch replaces markup but keeps what the user is doing: edited fields keep their state,
  // untouched ones take the server's, and focus and caret stay put (here a <corten-live> style
  // patch, with no form sent).
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML(
      "beforeend",
      '<form id="kept"><input name="q" value="a"><input name="r" value="a"><input type="checkbox" name="c" value="1"><select name="s"><option value="1">1</option><option value="2">2</option></select><select name="t"><option value="1">1</option><option value="2">2</option></select><select name="m" multiple><option value="1">1</option><option value="2">2</option><option value="3">3</option></select><p><button type="button">Go</button></p></form>',
    ),
  );
  await page.selectOption("#kept [name=m]", ["1", "3"]);
  await page.fill("#kept [name=q]", "typed");
  await page.check("#kept [name=c]");
  await page.selectOption("#kept [name=s]", "2");
  await page.focus("#kept [name=q]");
  await page.evaluate(() => /** @type {HTMLInputElement} */ (document.querySelector("#kept [name=q]")).setSelectionRange(1, 3));
  const fresh =
    '<form id="kept" class="new"><input name="q" value="b"><input name="r" value="b"><input type="checkbox" name="c" value="1"><select name="s"><option value="1">1</option><option value="2">2</option></select><select name="t"><option value="1">1</option><option value="2" selected>2</option></select><select name="m" multiple><option value="1">1</option><option value="2" selected>2</option><option value="3">3</option></select><p><button type="button">Go</button></p></form>';
  const afterLive = await page.evaluate(async (html) => {
    const { patch } = await import("corten");
    await patch(html);
    const q = /** @type {HTMLInputElement} */ (document.querySelector("#kept [name=q]"));
    return {
      server: document.querySelector("#kept")?.className,
      q: q.value,
      r: /** @type {HTMLInputElement} */ (document.querySelector("#kept [name=r]")).value,
      c: /** @type {HTMLInputElement} */ (document.querySelector("#kept [name=c]")).checked,
      s: /** @type {HTMLSelectElement} */ (document.querySelector("#kept [name=s]")).value,
      t: /** @type {HTMLSelectElement} */ (document.querySelector("#kept [name=t]")).value,
      m: Array.from(/** @type {HTMLSelectElement} */ (document.querySelector("#kept [name=m]")).selectedOptions, (o) => o.value).join(),
      focused: document.activeElement === q,
      selection: [q.selectionStart, q.selectionEnd],
    };
  }, fresh);
  check(
    "a patch keeps edited fields, focus and selection, and applies the rest",
    afterLive.server === "new" && afterLive.q === "typed" && afterLive.r === "b" && afterLive.c && afterLive.s === "2" && afterLive.t === "2" && afterLive.m === "1,3" && afterLive.focused && afterLive.selection.join() === "1,3",
    JSON.stringify(afterLive),
  );

  // What the user opened stays open across a patch (a modal dialog modal), and what the server
  // sent open and then closes closes.
  const block = (/** @type {number} */ n, /** @type {string} */ extra) =>
    `<section id="opened"><p>${n}</p><dialog id="opened-modal"><button type="button" id="opened-in">In</button></dialog><dialog id="opened-plain"></dialog><details id="opened-more"><summary>More</summary><p>Folded</p></details><div id="opened-pop" popover>Pop</div><dialog id="opened-served"${extra}></dialog></section>`;
  await page.evaluate((html) => document.querySelector("main")?.insertAdjacentHTML("beforeend", html), block(1, " open"));
  const openedState = await page.evaluate(async ([first, second]) => {
    const { patch } = await import("corten");
    const $ = (/** @type {string} */ id) => /** @type {any} */ (document.getElementById(id));
    // Markup a patch put in is what the server sent: open the served one, then the user's.
    await patch(first);
    $("opened-modal").showModal();
    $("opened-plain").show();
    $("opened-more").open = true;
    $("opened-pop").showPopover();
    // Last: opening a dialog moves focus.
    $("opened-in").focus();
    await patch(second);
    return {
      count: document.querySelector("#opened p")?.textContent,
      modal: $("opened-modal").matches(":modal"),
      plain: $("opened-plain").open && !$("opened-plain").matches(":modal"),
      more: $("opened-more").open,
      pop: $("opened-pop").matches(":popover-open"),
      served: $("opened-served").open,
      focused: document.activeElement?.id,
    };
  }, [block(2, " open"), block(3, "")]);
  await page.evaluate(() => {
    for (const d of Array.from(document.querySelectorAll("#opened dialog"))) /** @type {HTMLDialogElement} */ (d).close();
    document.querySelector("#opened")?.remove();
  });
  check(
    "a patch keeps a dialog, details and popover the user opened, and closes one the server closes",
    openedState.count === "3" && openedState.modal && openedState.plain && openedState.more && openedState.pop && openedState.served === false && openedState.focused === "opened-in",
    JSON.stringify(openedState),
  );
  await page.focus("#kept button");
  const buttonFocus = await page.evaluate(async (html) => {
    const { patch } = await import("corten");
    await patch(html);
    return document.activeElement === document.querySelector("#kept button");
  }, fresh);
  check("a patch keeps focus on an element without an id or name (by its place)", buttonFocus);
  await page.evaluate(() => document.querySelector("#kept")?.remove());

  // A form with a control named id (`form.id` is that control): its patch still finds it, and in
  // a patched block an edited field inside it keeps the user's text.
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML("beforeend", '<form id="named-id"><input name="id" value="7"><p>old</p></form><div id="holds-named"><form><input name="id"><input name="q" value="a"></form></div>'),
  );
  await page.fill("#holds-named [name=q]", "typed");
  const namedId = await page.evaluate(async () => {
    const { patch } = await import("corten");
    await patch('<form id="named-id"><input name="id" value="7"><p>new</p></form><div id="holds-named" class="new"><form><input name="id"><input name="q" value="b"></form></div>');
    return {
      form: document.querySelector("#named-id p")?.textContent,
      block: document.querySelector("#holds-named")?.className,
      q: /** @type {HTMLInputElement} */ (document.querySelector("#holds-named [name=q]")).value,
    };
  });
  check("a form with a control named id is patched, and keeps what the user typed", namedId.form === "new" && namedId.block === "new" && namedId.q === "typed", JSON.stringify(namedId));
  await page.evaluate(() => {
    document.querySelector("#named-id")?.remove();
    document.querySelector("#holds-named")?.remove();
  });

  // A file input keeps the files the user chose across a patch, which markup cannot give back: a
  // refusal redraws it marked invalid with the file still chosen, a live patch keeps it, and only
  // the answer that took those files clears them.
  const part = (/** @type {string} */ extra) => `<div id="upload-part"${extra}><input type="file" name="photo"${extra.includes("refused") ? ' aria-invalid="true"' : ""}></div>`;
  await page.evaluate(
    (html) => document.querySelector("main")?.insertAdjacentHTML("beforeend", `<form id="upload" method="post" action="/upload-test" enctype="multipart/form-data" data-patch>${html}<button>Send</button></form>`),
    part(""),
  );
  const errorsBeforeUpload = errors.length;
  let uploadStatus = 422;
  await page.route("**/upload-test", (route) =>
    route.fulfill({ status: uploadStatus, headers: { "corten-patch": "1", "content-type": "text/html; charset=utf-8" }, body: part(uploadStatus === 422 ? ' class="refused"' : ' class="taken"') }),
  );
  await page.setInputFiles("#upload [name=photo]", { name: "card.png", mimeType: "image/png", buffer: Buffer.from("png") });
  const chosenFiles = () => page.evaluate(() => Array.from(/** @type {HTMLInputElement} */ (document.querySelector("#upload [name=photo]")).files ?? [], (f) => f.name).join());
  await page.click("#upload button");
  await page.waitForSelector("#upload-part.refused", { timeout: 4000 }).catch(() => {});
  const refusedFiles = await chosenFiles();
  const markedInvalid = await page.getAttribute("#upload [name=photo]", "aria-invalid");
  await page.evaluate(async (html) => {
    const { patch } = await import("corten");
    await patch(html);
  }, part(' class="live"'));
  const liveFiles = await chosenFiles();
  uploadStatus = 200;
  await page.click("#upload button");
  await page.waitForSelector("#upload-part.taken", { timeout: 4000 }).catch(() => {});
  const takenFiles = await chosenFiles();
  await page.unroute("**/upload-test");
  await page.evaluate(() => document.querySelector("#upload")?.remove());
  // The refusal is a 422 on purpose; the browser logs that as a failed resource.
  errors.splice(errorsBeforeUpload, Infinity, ...errors.slice(errorsBeforeUpload).filter((e) => !e.includes("422")));
  check(
    "a patch keeps a file input's chosen files until the answer that took them",
    refusedFiles === "card.png" && markedInvalid === "true" && liveFiles === "card.png" && takenFiles === "",
    JSON.stringify({ refusedFiles, markedInvalid, liveFiles, takenFiles }),
  );

  // Swaps only: these links live in the shell, which a full page load replaces.
  if (swaps) {
    // data-nav="section": current on its page and every page under it.
    await page.evaluate(() => document.querySelector("header nav")?.insertAdjacentHTML("beforeend", '<a id="section-link" href="/" data-nav="section">All</a>'));
    await page.click('header a[href="/"]:not(#section-link)');
    await page.waitForFunction(() => document.title === "Watchlist | Corten");
    await page.click('header a[href="/news"]');
    await page.waitForFunction(() => document.title === "News | Corten");
    check(
      'data-nav="section" marks the link on pages under it',
      (await page.getAttribute("#section-link", "aria-current")) === "page" && (await page.getAttribute('header a[href="/"]:not(#section-link)', "aria-current")) === null,
    );

    // data-no-prefetch: hovering does not fetch; an ordinary link does (the control).
    await page.evaluate(() =>
      document.querySelector("main")?.insertAdjacentHTML("beforeend", '<p><a id="no-prefetch" href="/" data-no-prefetch>Home</a> <a id="prefetch" href="/?from=hover">Home</a></p>'),
    );
    navRequests.length = 0;
    await page.hover("#no-prefetch");
    await page.waitForTimeout(300);
    const afterOptOut = navRequests.length;
    await page.hover("#prefetch");
    await page.waitForTimeout(300);
    check("data-no-prefetch: hovering the link fetches nothing", afterOptOut === 0 && navRequests.length === 1, `${afterOptOut} then ${navRequests.join()}`);

    // The prefetch cache keeps the 8 newest pages: after hovering 9 links, following the newest
    // reuses its prefetch and following the oldest fetches it again. The links sit in the shell,
    // which navigation keeps.
    await page.evaluate(() =>
      document.querySelector("header nav")?.insertAdjacentHTML("beforeend", Array.from({ length: 9 }, (_, i) => `<a class="hovered" href="/news?p=${i}">${i}</a>`).join(" ")),
    );
    for (let i = 0; i < 9; i += 1) {
      await page.hover(`header a.hovered[href="/news?p=${i}"]`);
      await page.waitForTimeout(150);
    }
    const hoveredCount = navRequests.length;
    await page.click('header a.hovered[href="/news?p=8"]');
    await page.waitForFunction(() => location.search === "?p=8");
    const afterNewest = navRequests.length;
    await page.click('header a.hovered[href="/news?p=0"]');
    await page.waitForFunction(() => location.search === "?p=0");
    // The address changes when the navigation starts, before the runtime sends its request.
    for (let waited = 0; navRequests.length < 11 && waited < 2000; waited += 50) await page.waitForTimeout(50);
    check(
      "prefetch keeps the 8 newest pages: the newest is reused, the oldest is fetched again",
      hoveredCount === 10 && afterNewest === 10 && navRequests.length === 11,
      `${hoveredCount}, ${afterNewest}, ${navRequests.length}`,
    );
    await page.evaluate(() => document.querySelectorAll("header a.hovered").forEach((a) => a.remove()));

    // A section stylesheet that fails to load: the runtime loads the page in full instead of
    // showing it unstyled. A fresh document, so the stylesheet is not already in it.
    await page.goto(base);
    let failStyle = true;
    await page.route("**/sections/**", (route) => {
      if (!failStyle) return route.continue();
      failStyle = false;
      return route.abort("internetdisconnected");
    });
    const logged = errors.length;
    await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 5));
    await page.click('header a[href="/news"]');
    await page.waitForFunction(() => document.title === "News | Corten" && document.readyState === "complete");
    check(
      "a section stylesheet that fails to load makes the page load in full",
      !failStyle && (await page.evaluate(() => (/** @type {any} */ (window)).__marker === undefined)) && (await noteBorder()) === "4px",
    );
    await page.unroute("**/sections/**");
    // The browser logs the stylesheet it could not load.
    errors.splice(logged, errors.length, ...errors.slice(logged).filter((e) => !/Failed to load resource|ERR_INTERNET_DISCONNECTED/.test(e)));
  } else {
    for (const name of [
      'data-nav="section" marks the link on pages under it',
      "data-no-prefetch: hovering the link fetches nothing",
      "prefetch keeps the 8 newest pages",
      "a section stylesheet that fails to load makes the page load in full",
    ])
      skip(name, noSwaps);
  }

  // With the browser's data saver on, hovering fetches nothing.
  const saver = await browser.newContext();
  await saver.addInitScript(() => Object.defineProperty(Navigator.prototype, "connection", { get: () => ({ saveData: true }) }));
  const saving = await saver.newPage();
  /** @type {string[]} */
  const savingRequests = [];
  saving.on("request", (request) => request.headers()["corten"] === "nav" && savingRequests.push(request.url()));
  await saving.goto(`${base}/news`);
  await saving.hover('header a[href="/"]');
  await saving.waitForTimeout(300);
  check("data saver: hovering a link fetches nothing", savingRequests.length === 0, savingRequests.join());
  await saver.close();

  // A page that waits for a write (ReadAgain) reads itself again until the write is in,
  // however it was reached: a full load, or the runtime's navigation (a link, or a data-patch
  // form's corten-location, which navigates the same way). Each read is the same URL in place of
  // the history entry, and a navigation away stops it.
  {
    const due = (/** @type {number} */ ms) => `${base}/alerts?until=${Date.now() + ms}`;
    /** Navigates the runtime's way, and waits until the page is there, still saving. @param {string} url */
    const go = async (url) => {
      await page.evaluate((url) => (/** @type {any} */ (window)).navigation.navigate(url), url);
      await page.waitForFunction((url) => location.href === url && document.getElementById("alert-status")?.textContent === "Saving your alert...", url);
    };
    const saved = () => page.textContent("#alert-status");
    const savedSoon = () =>
      page.waitForFunction(() => document.getElementById("alert-status")?.textContent === "Your alert is saved.", null, { timeout: 6000 }).catch(() => {});
    /** @type {string[]} */
    const reads = [];
    const onSaved = (/** @type {import("@playwright/test").Request} */ r) => r.url().includes("/alerts") && reads.push(r.url());
    page.on("request", onSaved);
    await page.goto(due(1500));
    const waiting = await saved();
    await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 5));
    await savedSoon();
    check("a page waiting for a write reads itself again after a full load", waiting === "Saving your alert..." && (await saved()) === "Your alert is saved.", `${waiting} -> ${await saved()}`);
    if (swaps) check("reading again keeps the document", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 5));
    else skip("reading again keeps the document", noSwaps);
    const settled = reads.length;
    await page.waitForTimeout(2200);
    check("it stops once the page no longer asks", reads.length === settled, `${reads.length - settled} more reads`);
    // At a place on the page (a write redirected to `route.at(..)`): read again all the same.
    await page.goto(`${due(1500)}#alert-status`);
    await savedSoon();
    check("a page at a place (#id) reads itself again too", (await saved()) === "Your alert is saved." && new URL(page.url()).hash === "#alert-status", `${await saved()} ${page.url()}`);
    if (swaps) {
      await page.evaluate(() => (/** @type {any} */ (window)).__marker = 6);
      const entries = await page.evaluate(() => history.length);
      await go(due(1500));
      const reached = await saved();
      await savedSoon();
      check(
        "a page reached by the runtime's navigation reads itself again in place",
        reached === "Saving your alert..." && (await saved()) === "Your alert is saved." && (await page.evaluate(() => (/** @type {any} */ (window)).__marker === 6)),
        `${reached} -> ${await saved()}`,
      );
      check("each read replaces the history entry", (await page.evaluate(() => history.length)) === entries + 1, `${entries} -> ${await page.evaluate(() => history.length)}`);
      await go(due(10_000));
      await page.click('header a[href="/news"]');
      await page.waitForFunction(() => document.title === "News | Corten");
      const left = reads.length;
      await page.waitForTimeout(2200);
      check("navigating away stops the reads", reads.length === left, `${reads.length - left} more reads`);
    } else {
      for (const name of ["a page reached by the runtime's navigation reads itself again in place", "each read replaces the history entry", "navigating away stops the reads"]) skip(name, noSwaps);
    }
    page.off("request", onSaved);

    // A data-patch write answered in place while its read catches up (ReadAgain::from): the
    // status line is patched, then read again from its route (`corten: action`) until the write
    // is in, with no navigation; the button keeps focus, and a navigation away stops the reads.
    /** @type {string[]} */
    const again = [];
    const onAgain = (/** @type {import("@playwright/test").Request} */ r) =>
      r.method() === "GET" && r.url().includes("/alerts?until=") && r.headers()["corten"] === "action" && again.push(r.url());
    page.on("request", onAgain);
    const save = 'form[action="/alerts"] button';
    const submit = async () => {
      await page.fill("input[name=above]", "150");
      await page.focus(save);
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => document.getElementById("alert-status")?.textContent === "Saving your alert...");
    };
    await page.goto(`${base}/alerts`);
    // A price the server refuses: the form comes back with the field marked and its message,
    // and focus on it.
    const errorsAtRefusal = errors.length;
    await page.fill("input[name=above]", "1.234");
    await page.click(save);
    await page.waitForSelector('input[name=above][aria-invalid="true"]', { timeout: 5000 }).catch(() => {});
    const refused = await page.evaluate(() => ({
      message: document.getElementById("alert-form-above-error")?.textContent,
      focused: document.activeElement?.getAttribute("name"),
      value: /** @type {HTMLInputElement | null} */ (document.querySelector("input[name=above]"))?.value,
    }));
    check(
      "a refused field comes back marked, with its message and focus",
      refused.message === "Enter a price in dollars and cents, such as 129.99." && refused.focused === "above" && refused.value === "1.234",
      JSON.stringify(refused),
    );
    // The browser logs the 422 as a failed resource; Corten adds nothing.
    const refusedLogs = errors.slice(errorsAtRefusal);
    check("a refused field logs no console error of Corten's", refusedLogs.every((e) => e.includes("422")), refusedLogs.join(" | "));
    errors.splice(errorsAtRefusal);
    await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 7));
    await submit();
    await savedSoon();
    check(
      "a patch waiting for a write is read again from its route until the write is in",
      (await saved()) === "Your alert is saved." && again.length > 0 && page.url() === `${base}/alerts` && (await page.evaluate(() => (/** @type {any} */ (window)).__marker === 7)),
      `${await saved()} after ${again.length} reads at ${page.url()}`,
    );
    check("reading a patch again keeps focus", await page.evaluate(() => document.activeElement?.textContent === "Save alert"));
    const done = again.length;
    await page.waitForTimeout(2200);
    check("it stops once the patch no longer asks", again.length === done, `${again.length - done} more reads`);
    await submit();
    await page.click('header a[href="/news"]');
    await page.waitForFunction(() => document.title === "News | Corten");
    const away = again.length;
    await page.waitForTimeout(2200);
    check("navigating away stops a patch's reads", again.length === away, `${again.length - away} more reads`);
    // A read the server fails (its error page, not a patch) stops the reads without a console
    // error: the server logged the failure, and the part stays as it was.
    const errorsBefore = errors.length;
    const failRead = (/** @type {import("@playwright/test").Route} */ route) =>
      route.request().headers()["corten"] === "action" ? route.fulfill({ status: 502, contentType: "text/html", body: "<p>Bad gateway</p>" }) : route.fallback();
    await page.route("**/alerts?until=*", failRead);
    await page.goto(`${base}/alerts`);
    const readsBefore = again.length;
    await submit();
    for (let i = 0; i < 40 && again.length === readsBefore; i++) await page.waitForTimeout(100);
    const failedAt = again.length;
    await page.waitForTimeout(2200);
    // The browser itself reports the 502 as a failed resource; Corten adds nothing.
    const logged = errors.slice(errorsBefore).filter((e) => !e.includes("502"));
    check(
      "a read the server fails stops the reads, without a console error",
      failedAt === readsBefore + 1 && again.length === failedAt && (await saved()) === "Saving your alert..." && logged.length === 0,
      `${failedAt - readsBefore} then ${again.length - failedAt} reads, ${logged.join(" | ")}`,
    );
    errors.splice(errorsBefore);
    await page.unroute("**/alerts?until=*", failRead);
    page.off("request", onAgain);
  }

  // data-reload: the link loads the whole document instead of swapping <main>.
  await page.evaluate(() => {
    (/** @type {any} */ (window)).__marker = 4;
    document.querySelector("main")?.insertAdjacentHTML("beforeend", '<a id="reload-link" href="/" data-reload>Home</a>');
  });
  await page.click("#reload-link");
  await page.waitForFunction(() => document.title === "Watchlist | Corten");
  check("data-reload loads the whole document", await page.evaluate(() => (/** @type {any} */ (window)).__marker === undefined));

  // A multipart form is sent as FormData. /watch reads a URL-encoded form, so axum rejects it with
  // plain text, which a browser never sees: it gets the app's error page.
  await page.waitForSelector(`${live}[data-status="live"]`, { timeout: 5000 }).catch(() => {});
  /** @type {string[]} */
  const types = [];
  const onRequest = (/** @type {import("@playwright/test").Request} */ r) => r.url().endsWith("/watch") && types.push(r.headers()["content-type"] ?? "");
  page.on("request", onRequest);
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML(
      "beforeend",
      '<form id="multipart" method="post" action="/watch" enctype="multipart/form-data" data-patch><input type="hidden" name="id" value="ACME"><button>Watch</button></form>',
    ),
  );
  await page.click("#multipart button");
  await page.waitForFunction(() => document.title !== "Watchlist | Corten", null, { timeout: 4000 }).catch(() => {});
  page.off("request", onRequest);
  check("a multipart form is posted as multipart/form-data", types.length === 1 && types[0].startsWith("multipart/form-data"), types.join());
  check("a request the server rejects shows the app's error page, in its layout", (await page.textContent("main h1")) === "Something went wrong" && (await page.title()) === "Something went wrong | Corten", await page.title());

  // A form with a file is read as it arrives: each engine's multipart body, files and fields.
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML(
      "beforeend",
      '<form id="avatar" method="post" action="/avatar" enctype="multipart/form-data" data-patch><input name="name" value="Mew &quot;1st&quot;"><input type="file" name="picture" multiple><p id="avatar-result"></p><button>Send</button></form>',
    ),
  );
  await page.setInputFiles("#avatar [name=picture]", [
    { name: "card.png", mimeType: "image/png", buffer: Buffer.from("\r\n--png\r\n") },
    { name: "big.jpg", mimeType: "image/jpeg", buffer: Buffer.alloc(70 << 10, 7) },
  ]);
  await page.click("#avatar button");
  await page.waitForFunction(() => document.querySelector("#avatar-result")?.textContent, null, { timeout: 4000 }).catch(() => {});
  const uploaded = await page.textContent("#avatar-result");
  check("a form with files is read as it arrives", uploaded === 'Mew "1st": card.png (image/png, 9 bytes), big.jpg (image/jpeg, too large)', uploaded ?? "");
  // A body over the handler's limit (1 MiB): the handler answers with the form's message, and the
  // browser, still sending when the answer comes, shows it instead of a network error.
  await page.setInputFiles("#avatar [name=picture]", [{ name: "huge.jpg", mimeType: "image/jpeg", buffer: Buffer.alloc(3 << 20, 7) }]);
  await page.click("#avatar button");
  await page.waitForFunction(() => document.querySelector("#avatar-result")?.textContent?.startsWith("The pictures"), null, { timeout: 8000 }).catch(() => {});
  const over = await page.textContent("#avatar-result");
  await page.evaluate(() => document.querySelector("#avatar")?.remove());
  check("a body over its limit is answered in the form", over === "The pictures are over 1 MB together.", over ?? "");

  // The posts answered 415 and 413 on purpose; the browser logs those as failed resources.
  check("no console errors after the injected forms", errors.every((e) => e.includes("415") || e.includes("413")), errors.join(" | "));

  // Without the page's cookie: the three stocks a first visit watches.
  const json = await (await fetch(base, { headers: { accept: "application/json" } })).json();
  check("same URL as JSON for data clients", json.title === "Watchlist | Corten" && json.data.stocks.length === 3, JSON.stringify(json.data.stocks.map((/** @type {{ id: string }} */ s) => s.id)));

  // A WebSocket (GET /echo): the page's own origin opens it and is answered. Another site's page
  // cannot, though the browser would send the user's cookies with its handshake.
  const talk = (/** @type {string} */ url) =>
    new Promise((resolve) => {
      const socket = new WebSocket(url);
      socket.onopen = () => socket.send("hi");
      socket.onmessage = (e) => {
        resolve(e.data);
        socket.close();
      };
      socket.onerror = () => resolve("refused");
      setTimeout(() => resolve("no answer"), 4000);
    });
  const echoUrl = `${base.replace(/^http/, "ws")}/echo`;
  const echoed = await page.evaluate(talk, echoUrl);
  check("a WebSocket from the page's own origin is answered", echoed === "HI", String(echoed));
  const elsewhere = await browser.newPage();
  await elsewhere.route("http://other.test/", (route) => route.fulfill({ contentType: "text/html", body: "<title>Other</title>" }));
  await elsewhere.goto("http://other.test/");
  const crossed = await elsewhere.evaluate(talk, echoUrl);
  check("another site's page cannot open it", crossed === "refused", String(crossed));
  await elsewhere.close();

  const noJs = await browser.newContext({ javaScriptEnabled: false });
  const plain = await noJs.newPage();
  await plain.goto(base);
  check("without JavaScript prices are server-rendered", (await plain.$$('[data-ref="price"]')).length === 3);
  await plain.selectOption("select[name=id]", "SOLR");
  await Promise.all([plain.waitForNavigation(), plain.click("#watch-form button")]);
  check("without JavaScript the form posts and renders the page", ((await plain.textContent("#watch-message")) ?? "").startsWith("Watching Sunreach Solar"));
  check("the page it renders has the stock on the list", (await plain.$('[data-ref="price"][data-key="SOLR"]')) !== null);
  await noJs.close();

  // A navigation started before the first document has loaded is the browser's own: a script's
  // form submit (one made from on:connect) makes Chromium stop loading the document, and a page
  // taken over then never fires load.
  const early = await browser.newPage();
  /** @type {() => void} */
  let release = () => {};
  const held = new Promise((/** @type {(v?: unknown) => void} */ r) => (release = r));
  await early.route("**/slow.png", async (route) => {
    await held;
    await route.fulfill({ status: 404, body: "" }).catch(() => {});
  });
  await early.addInitScript(() => {
    if (location.pathname !== "/") return;
    document.addEventListener("DOMContentLoaded", () => {
      const img = new Image();
      img.src = "/slow.png";
      document.body.append(img);
      const form = document.createElement("form");
      form.method = "get";
      form.action = "/news";
      form.hidden = true;
      document.body.append(form);
      form.requestSubmit();
    });
  });
  await early.goto(base, { waitUntil: "commit" });
  await early.waitForFunction(() => document.title === "News | Corten", null, { timeout: 5000 }).catch(() => {});
  release();
  await early.waitForFunction(() => /** @type {PerformanceNavigationTiming} */ (performance.getEntriesByType("navigation")[0])?.loadEventEnd > 0, null, { timeout: 5000 }).catch(() => {});
  const loaded = await early.evaluate(() => {
    const entry = /** @type {PerformanceNavigationTiming} */ (performance.getEntriesByType("navigation")[0]);
    return { path: new URL(entry.name).pathname, load: entry.loadEventEnd > 0, title: document.title };
  });
  check(
    "a form submitted before the first page loads navigates, and the page loads",
    loaded.title === "News | Corten" && loaded.load,
    JSON.stringify(loaded),
  );
  await early.close();
}
