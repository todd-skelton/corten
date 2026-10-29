// Browser smoke test for the seller's page, against one instance with Postgres and NATS.
// Usage: node examples/listings-feed/smoke.mjs http://localhost:18485 (seller s1, the default seed)
import { chromium } from "@playwright/test";
import { stall } from "../../scripts/stall.mjs";

const base = process.argv[2];
if (!base) throw new Error("usage: node examples/listings-feed/smoke.mjs <base url>");
const seller = "s1";
const failures = [];
const seen = stall(5);
const check = (name, ok, detail = "") => {
  seen(name);
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};
const committed = async () => Number(await (await fetch(`${base}/api/feeds/${seller}/seq`)).json());
/** The listings as the feed has them now, by id. */
const snapshot = async () => {
  const body = await (await fetch(`${base}/feeds/${seller}`, { headers: { accept: "application/json" } })).json();
  return { seq: body.seq, rows: new Map(body.rows.map((/** @type {any} */ r) => [r.id, r])) };
};
const usd = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

const browser = await chromium.launch();
try {
  const page = await (await browser.newContext()).newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(e.message));
  const outputs = () =>
    page.evaluate(() => Object.fromEntries([...document.querySelectorAll("output[name]")].map((o) => [o.getAttribute("name"), o.textContent ?? ""])));
  /** Each shown row's id with its price and market low cells. */
  const shownRows = () =>
    page.evaluate(() =>
      [...document.querySelectorAll("tbody tr")].map((tr) => ({
        id: tr.getAttribute("data-key") ?? "",
        price: tr.cells[4].textContent ?? "",
        market: tr.cells[5].textContent ?? "",
        above: tr.hasAttribute("data-above"),
      })),
    );
  /** Until the page has every change the database committed. */
  const caughtUp = async () => {
    const seq = await committed();
    await page.waitForFunction((s) => Number(document.querySelector('output[name="through"]')?.textContent) >= s, seq);
    return seq;
  };

  await page.goto(`${base}/sellers/${seller}`);
  await page.waitForFunction(() => document.querySelector('output[name="status"]')?.textContent === "live");
  const first = await outputs();
  const rows = await shownRows();
  check("the page draws the first 500 listings and counts the seller's", rows.length === 500 && first.listings === "5000", `${rows.length} rows, ${first.listings}`);
  check("the script follows the feed: one snapshot, live", first.snapshots === "1" && first.status === "live", JSON.stringify(first));

  // The rows are what the feed holds once the page has every change, written in the page's money.
  const matches = async () => {
    const { rows: feed } = await snapshot();
    const wrong = (await shownRows()).filter((r) => {
      const l = feed.get(r.id);
      return !l || r.price !== usd.format(l.price_cents / 100) || r.market !== usd.format(l.market_cents / 100) || r.above !== l.price_cents > l.market_cents;
    });
    return wrong.map((r) => r.id);
  };
  const before = await committed();
  const changesBefore = Number((await outputs()).changes);
  await page.click("form button");
  await page.waitForFunction(() => (document.querySelector("[role=status]")?.textContent ?? "") !== "");
  const after = await caughtUp();
  check("the form's write commits once and its answer patches the status line", after === before + 1, `${before} -> ${after}`);
  const wrong = await matches();
  check("each shown row is redrawn from the change the feed brings", wrong.length === 0, wrong.length ? wrong.slice(0, 5).join(", ") : `${Number((await outputs()).changes) - changesBefore} change(s)`);

  // A change to listings the page does not draw is counted, not drawn, and leaves the shown rows as the feed has them.
  const drawnBefore = new Map((await shownRows()).map((r) => [r.id, r.market]));
  for (let i = 0; i < 5; i++) await fetch(`${base}/api/feeds/${seller}/market?n=100`, { method: "POST" });
  await caughtUp();
  const after100 = await matches();
  const moved = (await shownRows()).filter((r) => drawnBefore.get(r.id) !== r.market).length;
  check("500 more moves: the shown rows that moved are redrawn, the rest counted", after100.length === 0 && moved > 0 && (await shownRows()).length === 500, `${moved} redrawn${after100.length ? `, wrong: ${after100.slice(0, 5).join(", ")}` : ""}`);

  // A submit sent again with the same key (its answer lost) commits once.
  const resent = await page.evaluate(async () => {
    const form = /** @type {HTMLFormElement} */ (document.querySelector("form"));
    const body = new URLSearchParams(/** @type {any} */ (new FormData(form)));
    const send = () => fetch(form.action, { method: "POST", body, headers: { corten: "action" } }).then((r) => r.status);
    return [await send(), await send()];
  });
  const once = await committed();
  await caughtUp();
  check("a submit sent again with its key commits once", resent.every((s) => s < 300) && once === after + 5 + 1, `${resent} -> seq ${once}`);
  check("no console errors", errors.length === 0, errors.join(" | "));

  // Without JavaScript the form posts, and the page it lands on shows the write.
  const plain = await (await browser.newContext({ javaScriptEnabled: false })).newPage();
  await plain.goto(`${base}/sellers/${seller}`);
  const posted = await committed();
  await plain.click("form button");
  await plain.waitForURL((u) => u.searchParams.has("after"));
  const landed = Number(new URL(plain.url()).searchParams.get("after"));
  check("without JavaScript the post lands on the page, asked for its write", landed === posted + 1, plain.url());
  // The page asks to be read again until its instance has the write; then it no longer asks.
  await plain.waitForFunction(() => !document.querySelector("noscript"), null, { timeout: 10_000 }).catch(() => {});
  const html = await plain.content();
  const { rows: feed, seq } = await snapshot();
  const drawn = [...html.matchAll(/<tr data-key="([^"]+)"[^>]*>[\s\S]*?<\/tr>/g)].map((m) => m[0]);
  const stale = drawn.filter((tr) => {
    const id = /data-key="([^"]+)"/.exec(tr)?.[1] ?? "";
    const l = feed.get(id);
    return l && !tr.includes(usd.format(l.market_cents / 100));
  });
  check("without JavaScript the page it lands on has the write", seq >= landed && drawn.length === 500 && stale.length === 0, `seq ${seq}, ${stale.length} stale`);
} finally {
  await browser.close();
}
if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nall passed");
process.exit(0);
