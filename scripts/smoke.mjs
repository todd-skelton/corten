// Browser smoke test of every runtime behavior, against the hello example.
// Usage: node scripts/smoke.mjs [http://localhost:18480]
import { chromium } from "@playwright/test";

const base = process.argv[2] ?? "http://localhost:18480";
const failures = [];
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};

const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
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
  await page.route("**/watch", async (route) => {
    const wait = delayWatch;
    delayWatch = 0;
    if (failWatch) {
      failWatch = false;
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
    if (request.url().endsWith("/watch") && !error.includes("DISCONNECTED")) abandoned.push(error);
  });

  await page.goto(base);
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
  check("client navigation keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));
  check("navigation updates the URL", new URL(page.url()).pathname === "/about");

  await page.goBack();
  await page.waitForFunction(() => document.title === "Watchlist | Corten");
  await page.waitForSelector(`${live}[data-status="live"]`, { timeout: 5000 }).catch(() => {});
  check("back navigation restores the watchlist and on:connect resubscribes", (await page.getAttribute(live, "data-status")) === "live");
  // The submit failed on purpose above; the browser logs that request.
  const unexpected = errors.filter((e) => !e.includes("ERR_INTERNET_DISCONNECTED"));
  check("no console errors", unexpected.length === 0, unexpected.join(" | "));
  errors.length = 0;

  // Signing in changes the shell's context, so the redirect loads the whole document.
  await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 2));
  await page.click("header form[data-patch] button");
  await page.waitForFunction(() => document.querySelector("header")?.textContent?.includes("Signed in as Ada"), null, { timeout: 5000 }).catch(() => {});
  check("signing in shows the viewer in the shell", ((await page.textContent("header")) ?? "").includes("Signed in as Ada"));
  check("a new shell context loads the whole document", await page.evaluate(() => (/** @type {any} */ (window)).__marker === undefined));
  await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 3));
  await page.click('header a[href="/about"]');
  await page.waitForFunction(() => document.title === "About | Corten");
  check("same context: navigation swaps again", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 3));

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
  check(
    "prefetch keeps the 8 newest pages: the newest is reused, the oldest is fetched again",
    hoveredCount === 10 && afterNewest === 10 && navRequests.length === 11,
    `${hoveredCount}, ${afterNewest}, ${navRequests.length}`,
  );
  await page.evaluate(() => document.querySelectorAll("header a.hovered").forEach((a) => a.remove()));

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
} finally {
  await browser.close();
}
console.log(failures.length ? `\n${failures.length} failed` : "\nall passed");
process.exit(failures.length ? 1 : 0);
