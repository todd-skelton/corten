// Browser smoke test of every runtime behavior, against the hello example, in Chromium, Firefox
// and WebKit (Playwright's builds: `npx playwright install chromium firefox webkit`).
// Usage: node scripts/smoke.mjs [http://localhost:18480] [chromium|firefox|webkit ...]
import { chromium, firefox, webkit } from "@playwright/test";

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
  // stylesheet that failed) as an error, in one of two wordings.
  const cut = /was interrupted while the page was loading|can.t establish a connection to the server at \S+\/live\/trades/;
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
        /** @type {HTMLAnchorElement} */ (document.querySelector('header a[href="/about"]')).click();
        const about = await until(() => document.title === "About | Corten");
        const note = document.querySelector(".about-note__note");
        const border = note ? getComputedStyle(note).borderInlineStartWidth : "";
        history.back();
        const back = await until(() => document.title === "Watchlist | Corten");
        return { about, border, back, kept: (/** @type {any} */ (window)).__marker === 1, path: location.pathname };
      });
      check(
        "WebKit swaps a page in with its section stylesheet loaded, and back restores the last one",
        seen.about && seen.border === "4px" && seen.back && seen.kept && seen.path === "/",
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

  // A handler with per-component state: on:click={js::pause(scope, target, price)}.
  check("the script reveals its controls", await page.isVisible(`${live} button`));
  await page.click(`${live} button`);
  const paused = await prices();
  await page.waitForTimeout(2500);
  check(
    "a handler changes the component's state (pause holds the prices)",
    (await page.getAttribute(`${live} button`, "aria-pressed")) === "true" && JSON.stringify(await prices()) === JSON.stringify(paused),
  );
  await page.click(`${live} button`);
  // The page's prices by stock id, and the feed's.
  const shown = () => page.$$eval('[data-ref="price"]', (cells) => JSON.stringify(cells.map((c) => [c.getAttribute("data-key"), c.textContent]).sort()));
  const latest = async () => {
    const snap = await (await fetch(`${base}/feed/stocks`, { headers: { accept: "application/json" } })).json();
    return JSON.stringify(snap.rows.map((/** @type {{ id: string, price: string }} */ r) => [r.id, r.price]).sort());
  };
  let resumed = false;
  for (let i = 0; i < 20 && !resumed; i += 1) {
    resumed = (await shown()) === (await latest());
    if (!resumed) await page.waitForTimeout(150);
  }
  check("resuming shows the latest prices", resumed && (await page.getAttribute(`${live} button`, "aria-pressed")) === "false");

  // Many rows joining at once are scanned for ref names one query per name: a name that only the
  // last of them has still refreshes the refs the component has cached (pause just resolved them).
  await page.click(`${live} button`);
  await page.evaluate((live) => {
    const body = /** @type {HTMLTableSectionElement} */ (document.querySelector(`${live} tbody`));
    const old = /** @type {HTMLElement} */ (body.querySelector('[data-ref="price"][data-key="s1"]'));
    old.removeAttribute("data-ref");
    old.id = "old-s1";
    const rows = document.createDocumentFragment();
    for (let i = 0; i < 20; i += 1) {
      const tr = document.createElement("tr");
      tr.className = "wide";
      tr.innerHTML = i < 19 ? '<td data-ref="note">n</td>' : '<td data-ref="price" data-key="s1" id="new-s1">-</td>';
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
  await page.selectOption("select[name=id]", "s2");
  await page.click("main form[data-patch] button");
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent?.startsWith("Watching Globex"));
  check("form patch replaces the message by id", true);
  check("form patch keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));

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
  await page.click("main form[data-patch] button");
  check("a submit the network fails dispatches corten:error with status 0", (await failed) === 0);
  check(
    "a failed submit leaves the page and the form as they were",
    (await page.textContent("#watch-message")) === messageBefore && (await page.getAttribute("main form[data-patch]", "aria-busy")) === null,
  );

  // A submit button's formaction and formmethod win over the form's, as in a plain post.
  await page.evaluate(() =>
    document.querySelector("main")?.insertAdjacentHTML(
      "beforeend",
      '<form id="via-button" method="get" action="/nowhere" data-patch><input type="hidden" name="id" value="s1"><button formmethod="post" formaction="/watch">Watch</button></form>',
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
      '<form id="shadowed" method="post" action="/watch" data-patch><input type="hidden" name="id" value="s2"><input type="hidden" name="method" value="x"><input type="hidden" name="enctype" value="x"><input type="hidden" name="elements" value="x"><input type="hidden" name="setAttribute" value="x"><button name="action" value="go">Watch</button></form>',
    ),
  );
  const shadowedPosts = /** @type {string[]} */ ([]);
  const onShadowed = (/** @type {import("@playwright/test").Request} */ r) => r.resourceType() === "fetch" && shadowedPosts.push(`${r.method()} ${new URL(r.url()).pathname}`);
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
  await page.selectOption("select[name=id]", "s1");
  await page.click("main form[data-patch] button");
  await page.selectOption("select[name=id]", "s3");
  await page.click("main form[data-patch] button");
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent?.startsWith("Watching Initech"));
  await page.waitForTimeout(1600);
  check(
    "a newer submit replaces a pending one",
    ((await page.textContent("#watch-message")) ?? "").startsWith("Watching Initech") && abandoned.length === 1,
    `${await page.textContent("#watch-message")}; abandoned: ${abandoned.join()}`,
  );
  check("the form is not left busy", (await page.getAttribute("main form[data-patch]", "aria-busy")) === null);

  const current = () => page.$$eval("a[data-nav][aria-current=page]", (links) => links.map((l) => l.getAttribute("href")).join());
  check("the shell marks the current link", (await current()) === "/");
  check("one feed stream while the component is on the page", (await openFeeds()) === 1);
  // The about page's note is in an area (Area::components): its stylesheet loads before the page is
  // swapped in, so the note is styled the moment it joins the page.
  await page.evaluate(() => {
    new MutationObserver((_, observer) => {
      const note = document.querySelector(".about-note__note");
      if (!note) return;
      (/** @type {any} */ (window)).__noteBorder = getComputedStyle(note).borderInlineStartWidth;
      observer.disconnect();
    }).observe(document.body, { childList: true, subtree: true });
  });
  const noteBorder = () => page.$eval(".about-note__note", (note) => getComputedStyle(note).borderInlineStartWidth);
  const styleLinks = () => page.$$eval("link[data-corten-style]", (links) => links.length);
  check("a page outside the section does not link its stylesheet", (await styleLinks()) === 0);
  // A navigation abandons a pending submit, whose reply was for the page being left.
  delayWatch = 1200;
  await page.click("main form[data-patch] button");
  await page.click('header a[href="/about"]');
  await page.waitForFunction(() => document.title === "About | Corten");
  await page.waitForTimeout(1600);
  check("navigating away abandons a pending submit", abandoned.length === 2, abandoned.join());
  check("the feed stream closes when its component leaves the page (its signal aborts)", (await openFeeds()) === 0);
  check("the <corten-live> stream closes when it leaves the page", (await openStreams("/live/trades")) === 0);
  check("client navigation moves aria-current in the shell", (await current()) === "/about");
  check("main is not left busy", (await page.getAttribute("main", "aria-busy")) === null);
  check("client navigation swaps <main>", (await page.textContent("main h1")) === "About");
  if (swaps) {
    const border = await page.evaluate(() => (/** @type {any} */ (window)).__noteBorder);
    check("a section's stylesheet loads before its page is swapped in", border === "4px", border);
  } else {
    check("a section's page links its stylesheet", (await noteBorder()) === "4px" && (await styleLinks()) === 1);
  }
  if (swaps) check("client navigation keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));
  else check("without the Navigation API a link loads the whole page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === undefined));
  check("navigation updates the URL", new URL(page.url()).pathname === "/about");

  await page.goBack();
  await page.waitForFunction(() => document.title === "Watchlist | Corten");
  await page.waitForSelector(`${live}[data-status="live"]`, { timeout: 5000 }).catch(() => {});
  check("back navigation restores the watchlist and on:connect resubscribes", (await page.getAttribute(live, "data-status")) === "live");
  if (swaps) {
    check("a section's stylesheet stays when the page leaves the section", (await styleLinks()) === 1);
    await page.goForward();
    await page.waitForFunction(() => document.title === "About | Corten");
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
  await page.goto(`${base}/about`);
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
  await page.click('header a[href="/about"]');
  await page.waitForFunction(() => document.title === "About | Corten");
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
    (await page.textContent("#once")) === "Ready" && onceRequests === 1 && (await openStreams("/live/once")) === 0,
    `${await page.textContent("#once")}, ${onceRequests} requests`,
  );
  await page.evaluate(() => document.querySelector("#once-live")?.remove());

  // A <corten-live since> sends the page's cursor on its first connection; a fragment's id is the
  // cursor a reconnect sends back as Last-Event-ID, so the stream resumes there.
  /** @type {{ since: string | null, last: string | null }[]} */
  const resumes = [];
  await page.route("**/live/resume*", (route) => {
    const req = route.request();
    resumes.push({ since: new URL(req.url()).searchParams.get("since"), last: req.headers()["last-event-id"] ?? null });
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
  check(
    "<corten-live since> starts from the page's cursor and resumes from the last fragment's",
    (await page.textContent("#resume")) === "At 9" && resumes.length === 2 && resumes[0].since === "c7" && resumes[0].last === null && resumes[1].last === "c8",
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
  await page.focus("#kept button");
  const buttonFocus = await page.evaluate(async (html) => {
    const { patch } = await import("corten");
    await patch(html);
    return document.activeElement === document.querySelector("#kept button");
  }, fresh);
  check("a patch keeps focus on an element without an id or name (by its place)", buttonFocus);
  await page.evaluate(() => document.querySelector("#kept")?.remove());

  // Swaps only: these links live in the shell, which a full page load replaces.
  if (swaps) {
    // data-nav="section": current on its page and every page under it.
    await page.evaluate(() => document.querySelector("header nav")?.insertAdjacentHTML("beforeend", '<a id="section-link" href="/" data-nav="section">All</a>'));
    await page.click('header a[href="/"]:not(#section-link)');
    await page.waitForFunction(() => document.title === "Watchlist | Corten");
    await page.click('header a[href="/about"]');
    await page.waitForFunction(() => document.title === "About | Corten");
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
      document.querySelector("header nav")?.insertAdjacentHTML("beforeend", Array.from({ length: 9 }, (_, i) => `<a class="hovered" href="/about?p=${i}">${i}</a>`).join(" ")),
    );
    for (let i = 0; i < 9; i += 1) {
      await page.hover(`header a.hovered[href="/about?p=${i}"]`);
      await page.waitForTimeout(150);
    }
    const hoveredCount = navRequests.length;
    await page.click('header a.hovered[href="/about?p=8"]');
    await page.waitForFunction(() => location.search === "?p=8");
    const afterNewest = navRequests.length;
    await page.click('header a.hovered[href="/about?p=0"]');
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
    await page.click('header a[href="/about"]');
    await page.waitForFunction(() => document.title === "About | Corten" && document.readyState === "complete");
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
  await saving.goto(`${base}/about`);
  await saving.hover('header a[href="/"]');
  await saving.waitForTimeout(300);
  check("data saver: hovering a link fetches nothing", savingRequests.length === 0, savingRequests.join());
  await saver.close();

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
      '<form id="multipart" method="post" action="/watch" enctype="multipart/form-data" data-patch><input type="hidden" name="id" value="s1"><button>Watch</button></form>',
    ),
  );
  await page.click("#multipart button");
  await page.waitForFunction(() => document.title !== "Watchlist | Corten", null, { timeout: 4000 }).catch(() => {});
  page.off("request", onRequest);
  check("a multipart form is posted as multipart/form-data", types.length === 1 && types[0].startsWith("multipart/form-data"), types.join());
  check("a request the server rejects shows the app's error page", (await page.textContent("main h1")) === "Something went wrong" && (await page.title()) === "Something went wrong");

  // The multipart post was answered 415 on purpose; the browser logs that as a failed resource.
  check("no console errors after the injected forms", errors.every((e) => e.includes("415")), errors.join(" | "));

  const data = await page.request.get(base, { headers: { accept: "application/json" } });
  const json = await data.json();
  check("same URL as JSON for data clients", json.title === "Watchlist | Corten" && json.data.stocks.length === 3);

  const noJs = await browser.newContext({ javaScriptEnabled: false });
  const plain = await noJs.newPage();
  await plain.goto(base);
  await plain.selectOption("select[name=id]", "s3");
  await Promise.all([plain.waitForNavigation(), plain.click("main form button")]);
  check("without JavaScript the form posts and renders the page", ((await plain.textContent("#watch-message")) ?? "").startsWith("Watching Initech"));
  check("without JavaScript prices are server-rendered", (await plain.$$('[data-ref="price"]')).length === 3);
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
      form.action = "/about";
      form.hidden = true;
      document.body.append(form);
      form.requestSubmit();
    });
  });
  await early.goto(base, { waitUntil: "commit" });
  await early.waitForFunction(() => document.title === "About | Corten", null, { timeout: 5000 }).catch(() => {});
  release();
  await early.waitForFunction(() => /** @type {PerformanceNavigationTiming} */ (performance.getEntriesByType("navigation")[0])?.loadEventEnd > 0, null, { timeout: 5000 }).catch(() => {});
  const loaded = await early.evaluate(() => {
    const entry = /** @type {PerformanceNavigationTiming} */ (performance.getEntriesByType("navigation")[0]);
    return { path: new URL(entry.name).pathname, load: entry.loadEventEnd > 0, title: document.title };
  });
  check(
    "a form submitted before the first page loads navigates, and the page loads",
    loaded.title === "About | Corten" && loaded.load,
    JSON.stringify(loaded),
  );
  await early.close();
}
