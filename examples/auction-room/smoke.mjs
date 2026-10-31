// Browser smoke test for the auction: two bidders in two browsers bid on one lot against each
// other, a third bids without JavaScript, and the house ends the lot.
// Usage: node examples/auction-room/smoke.mjs http://localhost:18486 <house key>
import { chromium } from "@playwright/test";
import { steady } from "../../scripts/steady.mjs";
import { stall } from "../../scripts/stall.mjs";

const [base, houseKey] = process.argv.slice(2);
if (!base || !houseKey) throw new Error("usage: node examples/auction-room/smoke.mjs <base url> <house key>");
const failures = [];
const seen = stall(5);
const check = (name, ok, detail = "") => {
  seen(name);
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};

const LOT = "/lots/ashen-drake";
const browser = steady(await chromium.launch({ headless: true }));
/** @type {string[]} */
const errors = [];

/** A browser of its own (its own cookies), signed in as `name`. */
async function bidder(name, { javaScript = true, key = "" } = {}) {
  const context = await browser.newContext({ javaScriptEnabled: javaScript });
  const page = await context.newPage();
  page.on("console", (m) => m.type() === "error" && errors.push(`${name}: ${m.text()}`));
  page.on("pageerror", (e) => errors.push(`${name}: ${e.message}`));
  await page.goto(`${base}/sign-in`);
  await page.fill("input[name=name]", name);
  if (key) {
    await page.click("summary");
    await page.fill("input[name=house_key]", key);
  }
  await page.click("form[action='/sign-in'] button");
  // Signed in once the header has the sign-out form, on the page the sign-in sent it to.
  await page.locator("header form[action='/sign-out']").waitFor({ state: "attached" });
  await page.waitForLoadState("load");
  return page;
}

/** The lot's standing, as the page shows it now. */
const standing = (page) => page.textContent(`corten-live[src='${LOT}/live'] section`);
const notice = (page) => page.textContent("corten-live[src='/me/notices'] p");
/** Waits until the lot's live stream is open, so a change after this is pushed to the page. */
const live = (page) => page.waitForSelector(`corten-live[src='${LOT}/live'][status=live]`, { state: "attached" });

async function bid(page, amount) {
  await page.fill("form[action^='/me/bids/'] input[name=amount]", amount);
  await page.click("form[action^='/me/bids/'] button");
}

try {
  const ada = await bidder("Ada");
  const grace = await bidder("Grace");
  check("signing in shows the name in the header", (await ada.textContent("header")).includes("Ada"));

  await ada.goto(base + LOT);
  await grace.goto(base + LOT);
  await Promise.all([live(ada), live(grace), ada.waitForSelector("corten-live[src='/me/notices'][status=live]", { state: "attached" })]);
  check("an open lot shows its opening price and the time left", /\$25\.00/.test(await standing(grace)) && (await grace.isVisible(`corten-live[src='${LOT}/live'] time`)));

  // Ada bids; Grace's page shows it without reloading.
  await bid(ada, "25");
  await ada.waitForFunction(() => document.querySelector("corten-live section")?.textContent?.includes("You lead."));
  await grace.waitForFunction(() => document.querySelector("corten-live section")?.textContent?.includes("1 bid"));
  check("a bid reaches the other bidder's page live", (await standing(grace)).includes("Ada") && !(await standing(grace)).includes("You lead."));
  check("the bid form is cleared for the next bid", (await ada.inputValue("input[name=amount]")) === "");

  // A bid below the price and the step is refused in place, with the least it may be.
  await bid(grace, "25.50");
  await grace.waitForSelector("input[name=amount][aria-invalid=true]");
  check("a bid below the price is refused with the minimum", (await grace.textContent("form[action^='/me/bids/']")).includes("Bid $26.00 or more."));

  // Grace outbids Ada: Ada alone is told, on the page she has open.
  await bid(grace, "26");
  await ada.waitForFunction(() => document.querySelector("corten-live[src='/me/notices'] p")?.textContent?.includes("outbid"));
  check("the outbid bidder is told", (await notice(ada)).includes("You were outbid on Ashen Drake: it is at $26.00."));
  check("the leader is not", (await notice(grace)) === "");
  await ada.waitForFunction(() => !document.querySelector("corten-live section")?.textContent?.includes("You lead."));
  check("the old leader's page no longer says they lead", !(await standing(ada)).includes("You lead."));
  // Grace's own stream carried that change to her page too, and nothing for her notices with it.
  await grace.waitForFunction(() => document.querySelector("corten-live section")?.textContent?.includes("You lead."));
  check("the leader is still not told", (await notice(grace)) === "");

  // The same submit twice (a double click, a retry) is one bid: post one key twice. (Not the
  // form's own key, or the form's next bid would be this one again.)
  const counted = await ada.evaluate(async () => {
    const form = /** @type {HTMLFormElement} */ (document.querySelector("form[action^='/me/bids/']"));
    const body = new URLSearchParams({ amount: "30", submit: crypto.randomUUID() });
    const send = () => fetch(form.action, { method: "POST", body, headers: { corten: "action" } }).then((r) => r.status);
    return [await send(), await send()];
  });
  await grace.waitForFunction(() => document.querySelector("corten-live section")?.textContent?.includes("3 bids"));
  check("the same submit sent twice is one bid", counted.join() === "200,200" && (await standing(grace)).includes("3 bids"), counted.join());

  // Without JavaScript: the bid is a form, the page comes back, and an open lot reads itself again.
  const noJs = await bidder("Lin", { javaScript: false });
  await noJs.goto(base + LOT);
  check("without JavaScript an open lot's page reads itself again", (await noJs.locator("meta[http-equiv=refresh]").count()) === 1);
  // Each bid is a navigation: the POST, then (taken) the redirect back to the lot.
  const [placed] = await Promise.all([noJs.waitForNavigation(), bid(noJs, "31")]);
  check("without JavaScript a bid posts and the page shows it", new URL(noJs.url()).pathname === LOT && placed?.status() === 200 && (await standing(noJs)).includes("You lead."), noJs.url());
  const [tooLow] = await Promise.all([noJs.waitForNavigation(), bid(noJs, "1")]);
  check("without JavaScript a refused bid is the page with its error", tooLow?.status() === 422 && (await noJs.textContent("form[action^='/me/bids/']")).includes("Bid $32.00 or more."));
  check("a refused bid's page does not read itself again (it answers a POST)", (await noJs.locator("meta[http-equiv=refresh]").count()) === 0);

  // Ada takes the lead back; Lin, without JavaScript, reads the notice on the next page.
  await bid(ada, "33");
  await ada.waitForFunction(() => document.querySelector("corten-live section")?.textContent?.includes("You lead."));
  await noJs.goto(base + LOT);
  check("without JavaScript the outbid notice is on the next page", (await notice(noJs)).includes("You were outbid on Ashen Drake: it is at $33.00."));

  // A bidder past the limit is refused (429) until the minute turns.
  const flood = await bidder("Flood");
  await flood.goto(base + LOT);
  const statuses = await flood.evaluate(async () => {
    const form = /** @type {HTMLFormElement} */ (document.querySelector("form[action^='/me/bids/']"));
    const out = [];
    for (let i = 0; i < 21; i++) {
      const body = new URLSearchParams({ amount: "1", submit: crypto.randomUUID() });
      out.push((await fetch(form.action, { method: "POST", body, headers: { corten: "action" } })).status);
    }
    return out;
  });
  check("the 21st bid in a minute is refused with 429", statuses.slice(0, 20).every((s) => s === 422) && statuses[20] === 429, statuses.join(","));

  // Only the house reaches its desk.
  const refused = await ada.goto(`${base}/house`);
  check("a bidder is refused the house's desk", refused?.status() === 403 && (await ada.textContent("h1")) === "Not for you");
  await ada.goto(base + LOT);
  await live(ada);

  // The house ends the lot: everyone's page shows it closed, and the winner is told.
  const house = await bidder("House", { key: houseKey });
  check("the house signs in to its desk", new URL(house.url()).pathname === "/house");
  await house.click(`form[action='/house${LOT}/end'] button`);
  await house.waitForSelector(`form[action='/house${LOT}/end']`, { state: "detached" });
  await grace.waitForFunction(() => document.querySelector("corten-live section")?.textContent?.includes("Sold to Ada"));
  await ada.waitForFunction(() => document.querySelector("corten-live section")?.textContent?.includes("You won this lot."));
  check("the closed lot reaches every page live", (await standing(grace)).includes("Sold to Ada · 5 bids"));
  await ada.waitForFunction(() => document.querySelector("corten-live[src='/me/notices'] p")?.textContent?.includes("You won"));
  check("the winner is told", (await notice(ada)).includes("You won Ashen Drake for $33.00."));

  // The bids download as CSV.
  const csv = await grace.evaluate(async (lot) => {
    const res = await fetch(`${lot}/bids`);
    return { disposition: res.headers.get("content-disposition"), text: await res.text() };
  }, LOT);
  check("a lot's bids download as CSV", csv.disposition?.startsWith("attachment") && csv.text.split("\n").filter(Boolean).length === 6, csv.text);

  // The browser logs each 4xx answer; these three are the refusals this test asks for.
  const other = errors.filter((e) => !/Failed to load resource: the server responded with a status of (403|422|429)\b/.test(e));
  check("no other console errors", other.length === 0, other.join(" | "));
} finally {
  await browser.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}