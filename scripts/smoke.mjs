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

  await page.goto(base);
  await page.waitForFunction(() => !!customElements.get("live-prices"));
  check("island defined from its module", true);
  await page.waitForSelector('live-prices[data-status="live"]', { timeout: 5000 }).catch(() => {});
  check("feed subscription is live", (await page.getAttribute("live-prices", "data-status")) === "live");

  const prices = () => page.$$eval('[data-ref="price"]', (cells) => cells.map((c) => c.textContent));
  const before = await prices();
  await page.waitForFunction((b) => JSON.stringify([...document.querySelectorAll('[data-ref="price"]')].map((c) => c.textContent)) !== b, JSON.stringify(before), { timeout: 4000 }).catch(() => {});
  check("live patches update prices", JSON.stringify(await prices()) !== JSON.stringify(before), `${before.join(", ")} -> ${(await prices()).join(", ")}`);

  // A marker on window survives client navigations and patches, and disappears on a full load.
  await page.evaluate(() => ((/** @type {any} */ (window)).__marker = 1));
  await page.selectOption("select[name=id]", "c2");
  await page.click("form[data-patch] button");
  await page.waitForFunction(() => document.getElementById("watch-message")?.textContent?.startsWith("Watching Umbreon"));
  check("form patch replaces the message by id", true);
  check("form patch keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));

  await page.click('header a[href="/about"]');
  await page.waitForFunction(() => document.title === "About | Corten");
  check("client navigation swaps <main>", (await page.textContent("main h1")) === "About");
  check("client navigation keeps the page", await page.evaluate(() => (/** @type {any} */ (window)).__marker === 1));
  check("navigation updates the URL", new URL(page.url()).pathname === "/about");

  await page.goBack();
  await page.waitForFunction(() => document.title === "Watchlist | Corten");
  await page.waitForSelector('live-prices[data-status="live"]', { timeout: 5000 }).catch(() => {});
  check("back navigation restores the watchlist and resubscribes", (await page.getAttribute("live-prices", "data-status")) === "live");
  check("no console errors", errors.length === 0, errors.join(" | "));

  const data = await page.request.get(base, { headers: { accept: "application/json" } });
  const json = await data.json();
  check("same URL as JSON for data clients", json.title === "Watchlist | Corten" && json.data.cards.length === 3);

  const noJs = await browser.newContext({ javaScriptEnabled: false });
  const plain = await noJs.newPage();
  await plain.goto(base);
  await plain.selectOption("select[name=id]", "c3");
  await Promise.all([plain.waitForNavigation(), plain.click("form button")]);
  check("without JavaScript the form posts and renders the page", ((await plain.textContent("#watch-message")) ?? "").startsWith("Watching Pikachu"));
  check("without JavaScript prices are server-rendered", (await plain.$$('[data-ref="price"]')).length === 3);
  await noJs.close();
} finally {
  await browser.close();
}
console.log(failures.length ? `\n${failures.length} failed` : "\nall passed");
process.exit(failures.length ? 1 : 0);
