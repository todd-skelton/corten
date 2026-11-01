// Browser smoke test of corten-ui's components, against the ui-gallery example, in Chromium,
// Firefox and WebKit (Playwright's builds: `npx playwright install chromium firefox webkit`).
// Usage: node examples/ui-gallery/smoke.mjs [http://localhost:18482] [chromium|firefox|webkit ...]
import { chromium, firefox, webkit } from "@playwright/test";
import { steady } from "../../scripts/steady.mjs";
import { stall } from "../../scripts/stall.mjs";
import { browserBug } from "../../scripts/browser-bugs.mjs";
import { networkInterfaces } from "node:os";

const base = process.argv[2] ?? "http://localhost:18482";
const engines = { chromium, firefox, webkit };
const names = process.argv.slice(3).length ? process.argv.slice(3) : Object.keys(engines);
const failures = [];
let engine = "";
const seen = stall(5);
/** A check this browser cannot make, and why. */
const skip = (/** @type {string} */ name, /** @type {string} */ why) => {
  seen(`[${engine}] ${name}`);
  console.log(`skip [${engine}] ${name} (${why})`);
};
const check = (name, ok, detail = "") => {
  seen(`[${engine}] ${name}`);
  console.log(`${ok ? "ok  " : "FAIL"} [${engine}] ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(`[${engine}] ${name}`);
};

/**
 * Requests the browser's pages have sent and not finished, with when each was sent: printed when
 * a run throws (a page that never fires `load`), so the report says which one hangs (#232, #252).
 * A request leaves when it finishes or fails, or when its page or context closes.
 * @type {Map<import("@playwright/test").Request, { at: number, context: import("@playwright/test").BrowserContext }>}
 */
const inflight = new Map();

/**
 * The app at an address of this machine that is not loopback, when `base` is loopback: plain http
 * there is not a secure origin, as a tool served on the LAN is not. `SMOKE_LAN` names it
 * (`http://192.168.1.5:18482`); otherwise the private (RFC 1918) addresses first, the first that
 * answers.
 * @param {string} base
 */
async function lanBase(base) {
  if (process.env.SMOKE_LAN) return process.env.SMOKE_LAN;
  const url = new URL(base);
  if (!["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)) return null;
  const all = Object.values(networkInterfaces()).flat().filter((a) => a && a.family === "IPv4" && !a.internal && !a.address.startsWith("169.254."));
  const private_ = (/** @type {string} */ a) => /^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(a);
  const ordered = [...all.filter((a) => a && private_(a.address)), ...all.filter((a) => a && !private_(a.address))];
  for (const a of ordered) {
    const lan = `http://${a?.address}:${url.port}`;
    if (await fetch(`${lan}/`, { signal: AbortSignal.timeout(2000) }).then((r) => r.ok, () => false)) return lan;
  }
  return null;
}
/** @param {import("@playwright/test").Browser} browser */
function track(browser) {
  /** @param {(r: import("@playwright/test").Request, at: { context: import("@playwright/test").BrowserContext }) => boolean} gone */
  const drop = (gone) => {
    for (const [r, at] of inflight) if (gone(r, at)) inflight.delete(r);
  };
  /** @param {import("@playwright/test").Request} r */
  const pageOf = (r) => {
    try {
      return r.frame().page();
    } catch {
      return null; // a service worker's request has no frame
    }
  };
  // `newPage` makes its context through `newContext`, so every context is watched here.
  const newContext = browser.newContext.bind(browser);
  browser.newContext = async (...args) => {
    const context = await newContext(...args);
    context.on("request", (r) => inflight.set(r, { at: Date.now(), context }));
    context.on("requestfinished", (r) => inflight.delete(r));
    context.on("requestfailed", (r) => inflight.delete(r));
    context.on("page", (page) => page.on("close", () => drop((r) => pageOf(r) === page)));
    context.on("close", () => drop((_, at) => at.context === context));
    return context;
  };
}
/** What is still loading, oldest first. */
function pending() {
  const now = Date.now();
  return [...inflight].sort((a, b) => a[1].at - b[1].at).map(([r, { at }]) => `  ${now - at}ms ${r.method()} ${r.url()} (${r.resourceType()})`);
}

for (engine of names) {
  const browser = steady(await engines[engine].launch({ headless: true }));
  track(browser);
  try {
    await run(browser);
  } catch (error) {
    console.log(`[${engine}] still loading when it failed:\n${pending().join("\n") || "  nothing"}`);
    throw error;
  } finally {
    inflight.clear();
    await browser.close();
  }
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n${failures.join("\n")}`);
  process.exit(1);
}
console.log("\nall passed");

/** @param {import("@playwright/test").Browser} browser */
async function run(browser) {
  // A console error the browser logs for its own bug (scripts/browser-bugs.mjs) is not the page's.
  const bug = browserBug(browser);
  const page = await (await browser.newContext()).newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && !bug(m.text()) && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(e.message));
  /** @type {string[]} */
  const failed = [];
  /** @param {import("@playwright/test").Request} r */
  const lost = (r) => failed.push(`${new URL(r.url()).pathname} ${r.failure()?.errorText}`);
  /** @param {import("@playwright/test").Response} r */
  const refused = (r) => r.status() >= 400 && failed.push(`${new URL(r.url()).pathname} ${r.status()}`);
  page.on("requestfailed", lost);
  page.on("response", refused);
  await page.goto(`${base}/`);
  // The combo box's input shows once its script connects (#252): on a timeout, say what had
  // loaded and what failed rather than throwing without the engine.
  const connected = await page
    .waitForFunction(() => document.querySelector('[data-scope=ui-combo-box] [role="combobox"]:not([hidden])') !== null)
    .then(() => true)
    .catch(() => false);
  if (!connected) {
    // The page may have crashed or gone: say so rather than lose the rest of the report.
    const state = await page
      .evaluate(() => ({
        readyState: document.readyState,
        scripts: performance.getEntriesByType("resource").map((e) => e.name).filter((n) => n.includes("/scripts/") || n.includes("corten")).map((n) => new URL(n).pathname),
      }))
      .catch((e) => ({ readyState: `unavailable: ${e instanceof Error ? e.message : e}`, scripts: [] }));
    check("the first page's combo box connects", false, JSON.stringify({ ...state, failed, errors }));
    return;
  }
  // Only the first load's: the gallery's refused forms answer 422 later, as they should.
  page.off("requestfailed", lost);
  page.off("response", refused);
  const loaded = await page.evaluate(() => performance.getEntriesByType("resource").map((e) => e.name).filter((n) => n.includes("/scripts/")).length);
  check("the fifteen scripts load and connect", loaded === 15, `${loaded} script(s)`);

  // NumberField
  const qty = page.locator("#qty");
  const qtyButtons = page.locator("[data-scope=ui-number-field]").first().locator("button");
  check("at the minimum, decrease is disabled", (await qtyButtons.nth(0).isDisabled()) && (await qty.inputValue()) === "1");
  await page.evaluate(() => {
    const w = /** @type {any} */ (window);
    w.changes = 0;
    document.querySelector("#qty")?.addEventListener("change", () => (w.changes += 1));
  });
  await qtyButtons.nth(1).click();
  check("increase steps once per click", (await qty.inputValue()) === "2", await qty.inputValue());
  check("a step sends change", (await page.evaluate(() => /** @type {any} */ (window).changes)) === 1);
  check("decrease is enabled above the minimum", !(await qtyButtons.nth(0).isDisabled()));
  await qty.focus();
  await page.keyboard.press("ArrowUp");
  check("the arrow keys step (native)", (await qty.inputValue()) === "3");
  await page.keyboard.press("PageUp");
  check("Page Up adds ten steps, up to the maximum", (await qty.inputValue()) === "10" && (await qtyButtons.nth(1).isDisabled()));
  await page.keyboard.press("Home");
  check("Home goes to the minimum", (await qty.inputValue()) === "1");
  const box = await qtyButtons.nth(1).boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.waitForTimeout(700);
  await page.mouse.up();
  const held = Number(await qty.inputValue());
  check("press and hold repeats", held >= 4, String(held));
  await page.waitForTimeout(200);
  check("release stops the repeat", Number(await qty.inputValue()) === held);
  check("the button kept focus where it was", await page.evaluate(() => document.activeElement?.id === "qty"));
  await page.locator("[data-scope=ui-number-field]").nth(1).locator("button").nth(1).click();
  check("a step keeps the step's decimals", (await page.locator("#weight").inputValue()) === "0.6", await page.locator("#weight").inputValue());
  // Each field keeps its own state (keep()): while the quantity's button is held, the weight's
  // button still steps from the keyboard. Sharing one, the held pointer would make it skip.
  await qty.focus();
  await page.keyboard.press("Home");
  const before = Number(await qty.inputValue());
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  const holding = Number(await qty.inputValue());
  await page.locator("[data-scope=ui-number-field]").nth(1).locator("button").nth(1).press("Enter");
  const own = await page.locator("#weight").inputValue();
  await page.mouse.up();
  // The quantity stepped on the press, so its pointer was held while the weight stepped.
  check("each field keeps its own state", holding > before && own === "0.7", `quantity ${before} -> ${holding}, weight ${own}`);
  // A press released off its button makes no click, so nothing is left to skip the next key's.
  const up = await page.locator("[data-scope=ui-number-field]").nth(1).locator("button").nth(1).boundingBox();
  await page.mouse.move(up.x + up.width / 2, up.y + up.height / 2);
  await page.mouse.down();
  await page.mouse.move(up.x + up.width / 2, up.y - 80);
  await page.mouse.up();
  await page.locator("[data-scope=ui-number-field]").nth(1).locator("button").nth(1).press("Enter");
  const after = await page.locator("#weight").inputValue();
  check("a press released elsewhere steps once, and the next key steps", after === "0.9", after);
  // A key's click right after a pointer's, in the same task (before any timer can run): the
  // pointer's stepped on its press, its click does not step again, and the key's does (#316).
  const sameTask = await page.locator("[data-scope=ui-number-field]").nth(1).locator("button").nth(1).evaluate((button) => {
    const input = /** @type {HTMLInputElement} */ (document.getElementById("weight"));
    const from = input.value;
    const at = { bubbles: true, cancelable: true, composed: true, button: 0, pointerId: 7, isPrimary: true };
    button.dispatchEvent(new PointerEvent("pointerdown", at));
    button.dispatchEvent(new PointerEvent("pointerup", at));
    button.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail: 1 }));
    button.click();
    return `${from} -> ${input.value}`;
  });
  check("a key's click right after a pointer's steps once more", sameTask === "0.9 -> 1.1", sameTask);
  const fewer = page.locator("[data-scope=ui-number-field]").nth(1).locator("button").nth(0);
  for (const key of ["Enter", "Space", "Enter", "Space"]) await fewer.press(key);
  check("Enter and Space on decrease step down", (await page.locator("#weight").inputValue()) === "0.7", await page.locator("#weight").inputValue());
  const layout = await page.evaluate(() => {
    // The number field's root is a div; the wrapper that lays out its parts and the adornment holds it.
    const field = /** @type {Element} */ (document.querySelector("#weight")?.closest("[data-scope=ui-number-field]")?.parentElement);
    return Array.from(field.querySelectorAll("button, span, input"), (el) => [el.localName, Math.round(el.getBoundingClientRect().x)])
      .sort((a, b) => Number(a[1]) - Number(b[1]))
      .map((p) => p[0])
      .join(" ");
  });
  check("the design system orders an adornment between the parts", layout === "button span input button", layout);

  // ComboBox
  const color = page.getByLabel("Condition");
  check("the label names the text input once the script runs", (await color.getAttribute("role")) === "combobox");
  check("the select is hidden", await page.locator("[data-scope=ui-combo-box] select").isHidden());
  await color.fill("light");
  const popup = page.locator("[data-scope=ui-combo-box] [popover]");
  check("typing opens the list", (await popup.evaluate((p) => p.matches(":popover-open"))) && (await color.getAttribute("aria-expanded")) === "true");
  const matched = await page.locator('[data-scope=ui-combo-box] [role="status"]').evaluate((p) => [p.hidden, getComputedStyle(p).display].join(" "));
  check("a part the component hides stays hidden under its design system's display (components.css)", matched === "true none", matched);
  const shown = await page.locator('[data-scope=ui-combo-box] [role="option"]:visible').allTextContents();
  check("the list filters by what was typed", shown.join() === "Lightly Played", shown.join());
  check("the first match is highlighted", (await color.getAttribute("aria-activedescendant")) !== null);
  await page.keyboard.press("Enter");
  check("Enter picks it", (await color.inputValue()) === "Lightly Played" && (await page.locator("[data-scope=ui-combo-box] select").inputValue()) === "lp");
  check("picking closes the list", (await color.getAttribute("aria-expanded")) === "false" && !(await popup.evaluate((p) => p.matches(":popover-open"))));
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  const active = await color.getAttribute("aria-activedescendant");
  check("Arrow Down opens at the value and moves", active !== null && (await page.locator(`#${active}`).textContent()) === "Moderately Played");
  await page.keyboard.press("Escape");
  check("Escape closes and keeps the value", (await color.inputValue()) === "Lightly Played" && (await color.getAttribute("aria-expanded")) === "false");
  const visible = () => page.locator('[data-scope=ui-combo-box] [role="option"]:visible').allTextContents();
  await color.fill("mp");
  const graded = await visible();
  check("a choice's terms find it (a grade the label does not show)", graded.join() === "Moderately Played", graded.join());
  await color.fill("d");
  const ranked = await visible();
  check("matches are ranked: a label or term that starts with the text first", ranked.join() === "Damaged,Lightly Played,Moderately Played,Heavily Played", ranked.join());
  const best = await color.getAttribute("aria-activedescendant");
  check("the best match is highlighted", best !== null && (await page.locator(`#${best}`).textContent()) === "Damaged");
  const marked = await page.locator("[data-scope=ui-combo-box] [data-highlighted]").count();
  check("only it is highlighted after the list was reordered", marked === 1, String(marked));
  await color.fill("");
  const all = await visible();
  check("an emptied query lists the options in their own order again", all.join() === "Near Mint,Lightly Played,Moderately Played,Heavily Played,Damaged", all.join());
  await page.keyboard.press("Escape");
  await color.fill("zzz");
  check("no match shows the status", await page.locator('[data-scope=ui-combo-box] [role="status"]').isVisible());
  await page.keyboard.press("Tab");
  check("leaving restores the value's text", (await color.inputValue()) === "Lightly Played");
  await color.click();
  await page.locator('[data-scope=ui-combo-box] [role="option"]', { hasText: "Heavily Played" }).click();
  check("a click picks", (await page.locator("[data-scope=ui-combo-box] select").inputValue()) === "hp" && (await color.inputValue()) === "Heavily Played");
  // Open, then measure: CSS anchor positioning puts the list under the input, at least as wide.
  await color.press("ArrowDown");
  const listBox = await page.evaluate(() => {
    const input = document.querySelector('[data-scope=ui-combo-box] [role="combobox"]')?.getBoundingClientRect();
    const list = document.querySelector("[data-scope=ui-combo-box] [popover]")?.getBoundingClientRect();
    return input && list && { gap: list.top - input.bottom, left: list.left - input.left, wider: list.width >= input.width - 1 };
  });
  check("the popup is anchored under the input (CSS anchor positioning)", !!listBox && listBox.gap >= 0 && listBox.gap <= 8 && Math.abs(listBox.left) <= 1 && listBox.wider, JSON.stringify(listBox));
  await color.press("Escape");
  await color.fill("");
  await page.keyboard.press("Tab");
  check("emptying the field clears the value", (await page.locator("[data-scope=ui-combo-box] select").inputValue()) === "");
  await page.getByRole("button", { name: "List for sale" }).click();
  const message = await color.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage);
  check("a required, empty combobox stops the form on the input", message !== "", message);
  await color.fill("near mint");
  await page.keyboard.press("Tab");
  check("typing a label exactly picks it", (await page.locator("[data-scope=ui-combo-box] select").inputValue()) === "nm");

  await qty.fill("3");
  await page.getByRole("button", { name: "List for sale" }).click();
  await page.waitForFunction(() => document.querySelector("#result")?.textContent?.startsWith("Listed"));
  check("the form sends the components' values", (await page.textContent("#result")) === "Listed 3 Near Mint Ashen Drake at $12.50 each, 0.7 oz to ship.", await page.textContent("#result"));

  // CheckboxGroup: no region checked stops the form at the group's first box.
  const lastResult = await page.textContent("#result");
  const sellValid = () => page.evaluate(() => /** @type {HTMLFormElement} */ (document.querySelector('form[action$="/sell"]')).checkValidity());
  const shipsValid = () => page.evaluate(() => Array.from(document.querySelectorAll('form[action$="/sell"] fieldset input[type="checkbox"]')).every((box) => /** @type {HTMLInputElement} */ (box).checkValidity()));
  const us = page.getByRole("checkbox", { name: "United States" });
  await us.uncheck();
  const shipMessage = await us.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage);
  await page.getByRole("button", { name: "List for sale" }).click();
  await page.waitForTimeout(300);
  check("a group with no box checked stops the form at its first box, with the app's message", shipMessage === "Choose where you ship." && !(await sellValid()) && (await page.textContent("#result")) === lastResult, shipMessage);
  // From the keyboard: Firefox's message for the refused box sits over the next box, and a click
  // there only closes the message, as it would for a user.
  await page.getByRole("checkbox", { name: "Canada" }).press("Space");
  check("one box checked, the group is valid", (await us.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage)) === "");
  // RequiredWhen: the lowest offer applies only to offers.
  const floor = page.getByLabel("Lowest offer");
  const floorRequired = () => floor.evaluate((i) => /** @type {HTMLInputElement} */ (i).required);
  check("for a fixed price the lowest offer is disabled and not required", (await floor.isDisabled()) && !(await floorRequired()));
  await page.getByLabel("Accept offers").check();
  check("accepting offers makes the lowest offer required", (await floor.isEnabled()) && (await floorRequired()));
  await page.getByRole("button", { name: "List for sale" }).click();
  await page.waitForTimeout(300);
  check("an empty lowest offer stops the form", (await floor.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage)) !== "" && (await page.textContent("#result")) === lastResult);
  await floor.fill("5");
  await qty.fill("2");
  await page.getByRole("button", { name: "List for sale" }).click();
  await page.waitForFunction(() => document.querySelector("#result")?.textContent?.startsWith("Listed 2"), null, { timeout: 5000 }).catch(() => {});
  check("with a lowest offer the listing is sent", (await page.textContent("#result")) === "Listed 2 Near Mint Ashen Drake at $12.50 each, 0.7 oz to ship.", await page.textContent("#result"));
  await page.getByLabel("Fixed price").check();
  check("back to a fixed price, the lowest offer is disabled again", await floor.isDisabled());
  // A reset puts the boxes back as drawn ("United States" checked), and the group is checked again.
  // Only the group's boxes: the reset also empties the condition, which is drawn with no choice.
  await us.uncheck();
  await page.getByRole("checkbox", { name: "Canada" }).uncheck();
  const unchecked = await us.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage);
  await page.evaluate(() => /** @type {HTMLFormElement} */ (document.querySelector('form[action$="/sell"]')).reset());
  await page.waitForTimeout(100);
  check("a form reset checks the group again", unchecked === "Choose where you ship." && (await us.isChecked()) && (await us.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage)) === "" && (await shipsValid()), unchecked);

  // TabGroup
  const tab = (name) => page.getByRole("tab", { name });
  await page.evaluate(() => (/** @type {any} */ (window).__tabs = document.querySelector("[data-scope=ui-tab-group]")));
  await tab("Offers").click();
  check("a click selects the tab and shows its panel", (await tab("Offers").getAttribute("aria-selected")) === "true" && (await page.getByRole("tabpanel").textContent()) === "Two offers, the best at $10.00.");
  check("the address follows the tab", page.url().endsWith("/?tab=offers"), page.url());
  check("a handler's history.replaceState is not a navigation", await page.evaluate(() => (/** @type {any} */ (window).__tabs === document.querySelector("[data-scope=ui-tab-group]"))));
  await page.keyboard.press("ArrowRight");
  check("Arrow Right moves and selects", (await page.evaluate(() => document.activeElement?.textContent)) === "Sales" && (await tab("Sales").getAttribute("aria-selected")) === "true");
  await page.keyboard.press("ArrowRight");
  check("the arrow keys wrap", (await tab("Listings").getAttribute("aria-selected")) === "true" && page.url() === `${base}/`);
  check("only the selected tab is in the tab order", (await page.$$eval('[role="tab"]', (t) => t.map((x) => /** @type {HTMLElement} */ (x).tabIndex).join())) === "0,-1,-1");
  await page.keyboard.press("End");
  check("End selects the last", (await tab("Sales").getAttribute("aria-selected")) === "true");

  // ChoiceGroup and ShownFor: one choice, parts in two cards.
  const period = (name) => page.getByRole("radio", { name });
  const showing = () => page.$$eval('[data-on^="connect:ui.choice.part"]', (parts) => parts.filter((p) => !(/** @type {HTMLElement} */ (p).hidden)).map((p) => p.textContent).join("|"));
  check("the server draws the default choice and only its parts", (await period("Month").getAttribute("aria-checked")) === "true" && (await showing()) === "Average $11.95 this month.|Sold 14 this month.", await showing());
  check("only the chosen option is in the tab order", (await page.$$eval('[role="radio"]', (r) => r.map((x) => /** @type {HTMLElement} */ (x).tabIndex).join())) === "-1,0");
  await period("Week").click();
  check("a click chooses in place, and every part of the group follows", (await period("Week").getAttribute("aria-checked")) === "true" && (await showing()) === "Average $12.10 this week.|Sold 3 this week.", await showing());
  check("the address follows the choice", new URL(page.url()).searchParams.get("period") === "week", page.url());
  await page.keyboard.press("ArrowRight");
  check("Arrow Right moves and chooses", (await page.evaluate(() => document.activeElement?.textContent)) === "Month" && (await showing()) === "Average $11.95 this month.|Sold 14 this month." && !new URL(page.url()).searchParams.has("period"), page.url());
  await page.keyboard.press("ArrowUp");
  check("Arrow Up moves back", (await period("Week").getAttribute("aria-checked")) === "true");
  await page.goto(`${base}/?tab=sales`);
  await period("Week").click();
  check("choosing keeps another control's key in the address", new URL(page.url()).search === "?tab=sales&period=week", page.url());
  await tab("Offers").click();
  check("and a tab keeps the choice", new URL(page.url()).search === "?tab=offers&period=week", page.url());
  await page.goto(`${base}/?period=week`);
  check("the address draws the choice on a load", (await period("Week").getAttribute("aria-checked")) === "true" && (await showing()) === "Average $12.10 this week.|Sold 3 this week.", await showing());
  await page.goto(`${base}/`);

  // MenuButton
  // The popover's toggle event comes a task after it opens or closes: wait for what it sets.
  const trigger = page.getByRole("button", { name: "More actions" });
  const expanded = (/** @type {string} */ value) =>
    page.waitForFunction((v) => document.querySelector("#actions-button")?.getAttribute("aria-expanded") === v, value);
  await trigger.click();
  await expanded("true");
  check("the button opens the menu", (await trigger.getAttribute("aria-expanded")) === "true");
  check("focus moves to the first item", (await page.evaluate(() => document.activeElement?.textContent)) === "Help");
  // Under the button, or above it where the viewport has no room below (flip-block).
  const menuBox = await page.evaluate(() => {
    const button = document.querySelector("#actions-button")?.getBoundingClientRect();
    const menu = document.querySelector("#actions")?.getBoundingClientRect();
    return button && menu && { gap: Math.max(menu.top - button.bottom, button.top - menu.bottom), left: menu.left - button.left };
  });
  check("the menu is anchored to its button", !!menuBox && menuBox.gap >= 0 && menuBox.gap <= 8 && Math.abs(menuBox.left) <= 1, JSON.stringify(menuBox));
  await page.keyboard.press("ArrowDown");
  check("Arrow Down moves", (await page.evaluate(() => document.activeElement?.textContent)) === "Archive");
  await page.keyboard.press("ArrowDown");
  check("a disabled item is passed over", (await page.evaluate(() => document.activeElement?.textContent)) === "Rename…");
  await page.keyboard.press("w");
  check("typing a letter moves to the item", (await page.evaluate(() => document.activeElement?.textContent)) === "Watch");
  await page.keyboard.press("Escape");
  await expanded("false");
  check("Escape closes and returns focus", (await trigger.getAttribute("aria-expanded")) === "false" && (await page.evaluate(() => document.activeElement?.textContent)) === "More actions");
  await page.keyboard.press("ArrowUp");
  await expanded("true");
  check("Arrow Up on the button opens at the last item", (await page.evaluate(() => document.activeElement?.textContent)) === "Watch");
  await page.keyboard.press("Escape");
  await expanded("false");
  await trigger.click();
  await page.getByRole("menuitem", { name: "Archive" }).click();
  await page.waitForFunction(() => document.querySelector("#menu-result")?.textContent === "Archived.");
  check("a form item posts and patches", true);
  await expanded("false");
  check("using an item closes the menu", (await trigger.getAttribute("aria-expanded")) === "false");
  // An item is any element the app writes: here a command that opens a dialog.
  await trigger.click();
  await expanded("true");
  await page.getByRole("menuitem", { name: "Rename…" }).click();
  await page.waitForFunction(() => /** @type {HTMLDialogElement | null} */ (document.querySelector("dialog[aria-label='Rename the draft']"))?.open === true);
  await expanded("false");
  check("a command item opens its dialog and closes the menu", (await trigger.getAttribute("aria-expanded")) === "false");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => /** @type {HTMLDialogElement | null} */ (document.querySelector("dialog[aria-label='Rename the draft']"))?.open === false);
  check("closing the dialog an item opened returns focus to the button", (await page.evaluate(() => document.activeElement?.id)) === "actions-button", String(await page.evaluate(() => document.activeElement?.outerHTML.slice(0, 80))));

  // Toaster (2 seconds in the gallery)
  const toast = page.locator("#ui-toast");
  const dismiss = page.getByRole("button", { name: "Dismiss" });
  check("no message: nothing shows", (await toast.isHidden()) && (await dismiss.isHidden()));
  const region = await page.evaluate(() => {
    const toaster = document.querySelector('[role="status"]:has(#ui-toast)');
    return [toaster, toaster?.parentElement, document.querySelector("#menu-result")].map((e) => (e ? getComputedStyle(e).display : "missing")).join();
  });
  check("empty live regions stay in the page, so a message is announced", !/none|missing/.test(region), region);
  const save = page.getByRole("button", { name: "Save draft" });
  await save.click();
  await page.waitForFunction(() => document.querySelector("#ui-toast")?.textContent === "Draft saved.");
  check("the patch puts the message in the live region", (await page.getByRole("status").filter({ has: toast }).count()) === 1 && (await toast.isVisible()));
  check("the close button shows with a message", await dismiss.isVisible());
  await dismiss.click();
  check("the button dismisses it", (await toast.isHidden()) && (await dismiss.isHidden()));
  await save.click();
  await toast.waitFor({ state: "visible" });
  await toast.hover();
  await page.waitForTimeout(2600);
  check("it waits while the pointer is over it", await toast.isVisible());
  await page.mouse.move(0, 0);
  await page.waitForTimeout(400);
  check("it is still there just after the pointer leaves (the rest of its time)", await toast.isVisible());
  await toast.waitFor({ state: "hidden", timeout: 4000 }).catch(() => {});
  check("then it goes after its time", (await toast.isHidden()) && (await dismiss.isHidden()));

  // dialog::modal: a dialog the server draws open is modal.
  const rename = page.locator("#rename");
  const isModal = () => rename.evaluate((d) => d.matches(":modal"));
  check("drawn closed, the dialog stays closed", !(await rename.evaluate((d) => /** @type {HTMLDialogElement} */ (d).open)));
  await page.getByRole("button", { name: "Rename draft" }).click();
  check("the button opens it as a modal (command=show-modal)", await isModal());
  await page.getByRole("button", { name: "Rename", exact: true }).click();
  await page.waitForFunction(() => document.querySelector("#rename [aria-invalid=true]") !== null);
  check("a refused patch keeps it modal, with the error", (await isModal()) && (await rename.textContent())?.includes("Give the draft a name."));
  check("focus is on the refused field", await page.evaluate(() => document.activeElement?.id === "draft-name"));
  await page.keyboard.press("Escape");
  check("Escape closes it", !(await rename.evaluate((d) => /** @type {HTMLDialogElement} */ (d).open)));
  // A rename that is taken redirects to the draft with a notice (`req.redirect_with_notice`): the
  // navigation's page does not run the layout, so the runtime hands it to the layout's toaster.
  // The notice rides in a Secure cookie, which Playwright's WebKit never sends back over http.
  const noticeChecks = [
    "a redirect's notice reaches the layout's toaster",
    "and the toaster offers to dismiss it",
    "the notice is said once",
    "a notice whose page loads in full instead reaches the toaster after the load",
  ];
  if (engine !== "webkit") {
    await page.getByRole("button", { name: "Rename draft" }).click();
    await page.locator("#draft-name").fill("Spring draft");
    await page.getByRole("button", { name: "Rename", exact: true }).click();
    const renamed = await page.waitForFunction(() => document.querySelector("#ui-toast")?.textContent === "Renamed to Spring draft.", null, { timeout: 5000 }).then(() => true, () => false);
    check(noticeChecks[0], renamed && (await page.locator("#ui-toast").isVisible()) && !(await rename.evaluate((d) => /** @type {HTMLDialogElement} */ (d).open)), await page.locator("#ui-toast").textContent());
    check(noticeChecks[1], await page.getByRole("button", { name: "Dismiss" }).isVisible());
    await page.getByRole("button", { name: "Dismiss" }).click();
    check(noticeChecks[2], !(await page.evaluate(async () => (await (await fetch("/")).text()).includes("Renamed to"))));
    // The navigation's page comes with another layout (here a shell key changed on the way): the
    // runtime loads it in full, and the notice, whose cookie that answer removed, goes with it.
    await page.route("**/*", async (route) => {
      if (route.request().method() !== "GET" || route.request().headers()["corten"] !== "nav") return route.fallback();
      const res = await route.fetch();
      await page.context().clearCookies({ name: "__Host-corten-notice" });
      await route.fulfill({ response: res, headers: { ...res.headers(), "corten-shell": "0000000000000000" } });
    });
    await page.evaluate(() => ((/** @type {any} */ (window)).__before = 1));
    await page.getByRole("button", { name: "Rename draft" }).click();
    await page.locator("#draft-name").fill("Autumn draft");
    await page.getByRole("button", { name: "Rename", exact: true }).click();
    const kept = await page.waitForFunction(() => document.querySelector("#ui-toast")?.textContent === "Renamed to Autumn draft.", null, { timeout: 5000 }).then(() => true, () => false);
    const loaded = await page.evaluate(() => (/** @type {any} */ (window)).__before === undefined);
    check(noticeChecks[3], kept && loaded && (await page.locator("#ui-toast").isVisible()), `${loaded} ${await page.locator("#ui-toast").textContent()}`);
    await page.unroute("**/*");
  } else for (const name of noticeChecks) skip(name, "Playwright's WebKit never sends a Secure cookie back over http");
  // Over plain http from an address that is not loopback (a tool on the LAN) the browser refuses
  // the Secure `__Host-` cookie, so the notice rides in a plain one there, in every engine (#498).
  const lan = await lanBase(base);
  const lanCheck = "a redirect's notice on a plain-http LAN address reaches the toaster";
  if (lan) {
    const onLan = await (await browser.newContext()).newPage();
    await onLan.goto(`${lan}/`);
    await onLan.getByRole("button", { name: "Rename draft" }).click();
    await onLan.locator("#draft-name").fill("Lan draft");
    await onLan.getByRole("button", { name: "Rename", exact: true }).click();
    const shown = await onLan.waitForFunction(() => document.querySelector("#ui-toast")?.textContent === "Renamed to Lan draft.", null, { timeout: 5000 }).then(() => true, () => false);
    // The page's answer removed it, so a reload does not say it again.
    const cookies = (await onLan.context().cookies()).map((c) => c.name).filter((name) => name.includes("corten-notice"));
    check(lanCheck, shown && (await onLan.locator("#ui-toast").isVisible()) && !cookies.length, `${lan} ${await onLan.locator("#ui-toast").textContent()}; notice cookies left: ${cookies.join(",") || "none"}`);
    await onLan.context().close();
  } else skip(lanCheck, "no address other than loopback to reach the app on");
  // A post without the runtime (no data-patch): the whole page, the dialog drawn open.
  await page.getByRole("button", { name: "Rename draft" }).click();
  await Promise.all([
    page.waitForURL("**/rename"),
    page.locator("#rename form[action]").evaluate((f) => {
      f.removeAttribute("data-patch");
      /** @type {HTMLFormElement} */ (f).requestSubmit();
    }),
  ]);
  await page.waitForFunction(() => document.querySelector("#rename")?.matches(":modal"), null, { timeout: 5000 }).catch(() => {});
  check("drawn open in a whole page, it becomes modal", await isModal());
  check("focus goes to the refused field", await page.evaluate(() => document.activeElement?.id === "draft-name"));
  const trapped = [];
  for (let i = 0; i < 6; i++) {
    await page.keyboard.press("Tab");
    trapped.push(await page.evaluate(() => !!document.activeElement?.closest("#rename") || document.activeElement === document.body));
  }
  check("Tab stays in the dialog (the page behind is inert)", trapped.every(Boolean), trapped.join());
  check("the page behind is inert to clicks", await page.evaluate(() => {
    const h1 = /** @type {HTMLElement} */ (document.querySelector("h1"));
    const r = h1.getBoundingClientRect();
    // The backdrop covers it: the point hits the dialog (its ::backdrop), not the heading.
    return document.elementFromPoint(r.left + 2, r.top + 2) === document.querySelector("#rename");
  }));
  await page.keyboard.press("Escape");
  check("Escape closes the page's dialog", !(await isModal()) && !(await rename.evaluate((d) => /** @type {HTMLDialogElement} */ (d).open)));

  // dialog::modalClosedBy: a sheet the address opens, which a close request leaves for its
  // link's address.
  await page.goto(`${base}/`);
  /**
   * This entry's place in the history, where the browser has the Navigation API (a browser
   * without it loads whole pages, and the link's address replaces the sheet's).
   */
  const at = () => page.evaluate(() => /** @type {any} */ (window).navigation?.currentEntry?.index ?? -1);
  /**
   * The page's own address. Not `page.url()` or `waitForURL`: Playwright's WebKit does not
   * report the runtime's navigation from `/` to `/?sale=last` (a push that changes only the
   * query), so its URL stays at `/` while `location` and the history have moved on, and a click
   * that waits for that navigation never returns (`noWaitAfter` on the link's clicks).
   */
  const href = () => page.evaluate(() => location.href);
  /** @param {boolean} open whether the address names the sale */
  const saleInAddress = (open) =>
    page.waitForFunction((open) => new URL(location.href).searchParams.has("sale") === open, open, { timeout: 5000 }).catch(() => {});
  const listEntry = await at();
  await page.getByRole("link", { name: "Last sale" }).click({ noWaitAfter: true });
  await saleInAddress(true);
  const sheet = page.locator('dialog[aria-label="Last sale"]');
  const sheetModal = () => page.evaluate(() => !!document.querySelector('dialog[aria-label="Last sale"]')?.matches(":modal"));
  await page.waitForFunction(() => document.querySelector('dialog[aria-label="Last sale"]')?.matches(":modal"), null, { timeout: 5000 }).catch(() => {});
  check("the address opens the sheet as a modal", await sheetModal());
  /** Escape on the sheet, then the page without it: the address drops `sale` and the sheet goes. */
  const escape = async (/** @type {string} */ how) => {
    await page.keyboard.press("Escape");
    await saleInAddress(false);
    await sheet.waitFor({ state: "detached", timeout: 5000 }).catch(() => {});
    check(`${how}: Escape goes to the close link's address, which does not name the sale`, !new URL(await href()).searchParams.has("sale"), await href());
    check(`${how}: the page without it replaces the sheet`, (await sheet.count()) === 0);
  };
  const openSheet = async () => {
    await page.getByRole("link", { name: "Last sale" }).click({ noWaitAfter: true });
    await saleInAddress(true);
    await page.waitForFunction(() => document.querySelector('dialog[aria-label="Last sale"]')?.matches(":modal"), null, { timeout: 5000 }).catch(() => {});
  };
  // Opened from the list, closing goes back to the list's entry: no entry is added, so Back
  // after it leaves the list, and Android's Back on the open sheet cannot loop through it.
  await escape("opened by a click");
  check("opened by a click: closing goes back to the list's entry", (await at()) === listEntry, `${listEntry} -> ${await at()}`);
  if (listEntry < 0) {
    // Without the Navigation API the list's address replaced the sheet's: Back shows the list
    // once more, not the sheet.
    await page.goBack();
    await page.waitForLoadState();
    check("no Navigation API: Back shows the list, not the sheet", new URL(page.url()).pathname === "/" && !new URL(page.url()).search && (await sheet.count()) === 0, page.url());
    await page.goForward();
    await page.waitForLoadState();
  }
  await page.reload();
  check("a reload does not open it again", (await sheet.count()) === 0);
  // A close request other than Escape (Android's Back, closedby="any") is the same `cancel`.
  if (await page.evaluate(() => "requestClose" in HTMLDialogElement.prototype)) {
    await openSheet();
    // Not the locator's evaluate: the close replaces the sheet as it runs, and a locator then
    // waits for it to come back.
    await page.evaluate(() => /** @type {any} */ (document.querySelector('dialog[aria-label="Last sale"]'))?.requestClose());
    await saleInAddress(false);
    await sheet.waitFor({ state: "detached", timeout: 5000 }).catch(() => {});
    check("requestClose() leaves the sheet's address too", !new URL(await href()).searchParams.has("sale") && (await sheet.count()) === 0, await href());
  }
  // Loaded at its address (a shared link), with no click since: Chromium does not let the page
  // keep it open on Escape, and the address is still left, in place of the sheet's entry (the
  // entry before is another page, so there is no going back to the list).
  await page.goto(`${base}/?tab=offers`);
  await page.goto(`${base}/?sale=last`);
  await page.waitForFunction(() => document.querySelector('dialog[aria-label="Last sale"]')?.matches(":modal"), null, { timeout: 5000 }).catch(() => {});
  check("loaded at its address, the sheet is modal", await sheetModal());
  const shared = await at();
  await escape("loaded at its address");
  check("loaded at its address: the list replaces the sheet's entry", (await at()) === shared, `${shared} -> ${await at()}`);
  await page.goBack();
  await page.waitForLoadState();
  check("Back from there goes to the page before the shared link", new URL(await href()).searchParams.get("tab") === "offers", await href());
  // CopyButton: shown where the browser has the Clipboard API; a click copies the SKU.
  const clip = await (await browser.newContext(engine === "chromium" ? { permissions: ["clipboard-read", "clipboard-write"] } : {})).newPage();
  clip.on("console", (m) => m.type() === "error" && !bug(m.text()) && errors.push(m.text()));
  clip.on("pageerror", (e) => errors.push(e.message));
  await clip.goto(`${base}/`);
  const copyButton = clip.getByRole("button", { name: "Copy", exact: true });
  await copyButton.waitFor({ state: "visible", timeout: 5000 }).catch(() => {});
  check("the copy button shows once its script connects", await copyButton.isVisible());
  await copyButton.click();
  const said = clip.locator('[data-scope=ui-copy-button] [aria-live="polite"]');
  await clip.waitForFunction(() => document.querySelector('[data-scope=ui-copy-button] [aria-live]')?.textContent === "Copied", null, { timeout: 5000 }).catch(() => {});
  check("a click says it copied, in its live region", (await said.textContent()) === "Copied", await said.textContent());
  const copies = clip.locator('output[aria-label="Copies"]');
  check("the copy is reported once to the page (Copied bubbles to the paragraph)", (await copies.textContent()) === "1", await copies.textContent());
  if (engine === "chromium") {
    check("the clipboard holds the SKU, from the page's own element", (await clip.evaluate(() => navigator.clipboard.readText())) === "ASH-DRAKE-NM");
  }
  await clip.waitForFunction(() => document.querySelector('[data-scope=ui-copy-button] [aria-live]')?.textContent === "Copy", null, { timeout: 5000 }).catch(() => {});
  check("then its label again", (await said.textContent()) === "Copy", await said.textContent());
  await clip.context().close();
  // A write the browser refuses changes nothing: the button never says it copied.
  const denied = await (await browser.newContext()).newPage();
  denied.on("pageerror", (e) => errors.push(e.message));
  await denied.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", { value: { writeText: () => Promise.reject(new DOMException("denied", "NotAllowedError")) } });
  });
  await denied.goto(`${base}/`);
  const deniedButton = denied.getByRole("button", { name: "Copy", exact: true });
  await deniedButton.waitFor({ state: "visible", timeout: 5000 }).catch(() => {});
  await deniedButton.click();
  await denied.waitForTimeout(300);
  check("a denied write leaves the label as it was", (await denied.locator('[data-scope=ui-copy-button] [aria-live]').textContent()) === "Copy");
  check("a denied write reports no copy", (await denied.locator('output[aria-label="Copies"]').textContent()) === "0");
  await denied.context().close();
  // Without the Clipboard API (an insecure origin) the button stays hidden.
  const insecure = await (await browser.newContext()).newPage();
  await insecure.addInitScript(() => Object.defineProperty(navigator, "clipboard", { value: undefined }));
  await insecure.goto(`${base}/`);
  await insecure.waitForFunction(() => document.querySelector("[data-scope=ui-combo-box] input")?.checkVisibility(), null, { timeout: 5000 }).catch(() => {});
  check("without the Clipboard API the button stays hidden", await insecure.locator("[data-scope=ui-copy-button]").isHidden());
  await insecure.context().close();
  // PrintButton: shown once its script connects; a click opens the print dialog (counted here).
  const printing = await (await browser.newContext()).newPage();
  printing.on("pageerror", (e) => errors.push(e.message));
  await printing.addInitScript(() => {
    window.print = () => {
      /** @type {any} */ (window).printed = (/** @type {any} */ (window).printed ?? 0) + 1;
    };
  });
  await printing.goto(`${base}/`);
  const printButton = printing.getByRole("button", { name: "Print label", exact: true });
  await printButton.waitFor({ state: "visible", timeout: 5000 }).catch(() => {});
  check("the print button shows once its script connects", await printButton.isVisible());
  await printButton.click();
  const printed = await printing.evaluate(() => /** @type {any} */ (window).printed ?? 0);
  check("a click opens the print dialog once", printed === 1, String(printed));
  await printing.context().close();
  // RelativeTime: the server wrote both times; the browser's clock (faked, from now) moves them.
  const timed = await (await browser.newContext()).newPage();
  const timedErrors = [];
  timed.on("pageerror", (e) => timedErrors.push(String(e)));
  timed.on("console", (m) => m.type() === "error" && !bug(m.text()) && timedErrors.push(m.text()));
  // Paused before the page loads, so only runFor moves its time: a slow read under load adds no
  // seconds, and the pause cannot land in the page's past.
  await timed.clock.install();
  await timed.clock.pauseAt(Date.now() + 1_000);
  await timed.goto(`${base}/`);
  const listed = timed.locator("time").first();
  const expires = timed.locator("time").nth(1);
  check("the server writes a time behind now", (await listed.textContent()) === "3 hr. ago", await listed.textContent());
  check("and one ahead, with its instant", (await expires.textContent()) === "in 1 minute" && /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d{3})?Z$/.test((await expires.getAttribute("datetime")) ?? ""), await expires.textContent());
  // To 58.5 seconds before the expiry, measured on the page, whatever the server's and the page's
  // clocks differ by.
  const until = Date.parse((await expires.getAttribute("datetime")) ?? "") - (await timed.evaluate(() => Date.now()));
  await timed.clock.runFor(Math.max(0, until - 58_500));
  const seconds = async () => Number(/^in (5\d) seconds$/.exec((await expires.textContent()) ?? "")?.[1]);
  const first = await seconds();
  check("under a minute ahead, it counts seconds", first > 50, await expires.textContent());
  await timed.clock.runFor(2_000);
  check("each second", (await seconds()) === first - 2, `${first} then ${await expires.textContent()}`);
  await timed.clock.runFor(60_000);
  check("past it, it says how long ago", /^\d+ seconds? ago$/.test((await expires.textContent()) ?? ""), await expires.textContent());
  check("the one behind kept its hour", (await listed.textContent()) === "3 hr. ago", await listed.textContent());
  await timed.clock.runFor(55 * 60_000);
  check("an hour on, the one behind moves", (await listed.textContent()) === "4 hr. ago" && (await expires.textContent()) === "55 minutes ago", `${await listed.textContent()}, ${await expires.textContent()}`);
  // Back from sleep: the clock moved two hours and no timer ran; the page coming back redraws.
  await timed.clock.setSystemTime((await timed.evaluate(() => Date.now())) + 2 * 3600_000);
  await timed.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  check("the page coming back redraws as of its clock", (await listed.textContent()) === "6 hr. ago" && (await expires.textContent()) === "2 hours ago", `${await listed.textContent()}, ${await expires.textContent()}`);
  // A time put in place of another, as a patch does: the new one is kept current, the old one let
  // go. The clock first moves past the page's timer with nothing to say so (a sleep), so the new
  // time cannot count on that timer.
  await timed.clock.setSystemTime((await timed.evaluate(() => Date.now())) + 2 * 3600_000);
  await timed.evaluate(() => {
    const old = /** @type {HTMLTimeElement} */ (document.querySelectorAll("time")[1]);
    const time = /** @type {HTMLTimeElement} */ (old.cloneNode(false));
    time.dateTime = new Date(Date.now() + 90_000).toISOString();
    time.textContent = "in 1 minute";
    old.replaceWith(time);
  });
  await timed.clock.runFor(31_000);
  check("a time that replaces another is kept current", /^in 5\d seconds$/.test((await expires.textContent()) ?? ""), await expires.textContent());
  check("no console errors with relative times", timedErrors.length === 0, timedErrors.join(" | "));
  await timed.context().close();
  // A browser without the page's locale falls back to another language: the server's text stays.
  const lacking = await (await browser.newContext()).newPage();
  const lackingErrors = [];
  lacking.on("pageerror", (e) => lackingErrors.push(String(e)));
  lacking.on("console", (m) => m.type() === "error" && !bug(m.text()) && lackingErrors.push(m.text()));
  await lacking.addInitScript(() => {
    const options = Intl.RelativeTimeFormat.prototype.resolvedOptions;
    Intl.RelativeTimeFormat.prototype.resolvedOptions = function () {
      /** @type {any} */ (window).asked = (/** @type {any} */ (window).asked ?? 0) + 1;
      return { ...options.call(this), locale: "fr" };
    };
  });
  await lacking.clock.install();
  await lacking.goto(`${base}/`);
  await lacking.clock.runFor(31_000);
  const asked = await lacking.evaluate(() => /** @type {any} */ (window).asked ?? 0);
  check(
    "a locale the browser lacks keeps the server's text",
    asked > 0 && lackingErrors.length === 0 && (await lacking.locator("time").nth(1).textContent()) === "in 1 minute",
    `${await lacking.locator("time").nth(1).textContent()}, asked ${asked}, ${lackingErrors.join(" | ")}`,
  );
  await lacking.context().close();

  // The refused renames are 422s, which Chromium and WebKit log; nothing else may be. WebKit also
  // reports a request to the gallery's own origin that did not finish (the hover prefetch of
  // /?sale=last, or a load the next navigation dropped) as a fetch that "cannot load
  // http://localhost:18482/?sale=last due to access control checks"; Playwright splits that page
  // error at its first colon, so the text here starts "/localhost:18482/?sale=last" (#301). A
  // request to its own origin is never refused by CORS, so only another origin's still counts.
  const ownOrigin = `/${new URL(base).host}/`;
  const unexpected = errors.filter((e) => {
    const refused = /(\S+) due to access control checks/.exec(e)?.[1];
    return !/status of 422/.test(e) && !refused?.startsWith(ownOrigin);
  });
  check("no console errors", unexpected.length === 0, unexpected.join(" | "));

  // LocalTime: a browser's first page is written in UTC, since its zone is not known yet, and the
  // script writes it again in the browser's; on the next page the server writes it in that zone.
  const kiwi = await (await browser.newContext({ timezoneId: "Pacific/Auckland" })).newPage();
  await kiwi.goto(`${base}/`);
  const drafted = kiwi.locator("p", { hasText: "Drafted" }).locator("time");
  await kiwi.waitForFunction(() => document.cookie.includes("corten-zone=") && document.body.textContent?.includes("GMT+13"), null, { timeout: 5000 }).catch(() => {});
  const firstDrafted = await drafted.textContent();
  check("a first page's date is written again in the browser's zone", firstDrafted === "Mar 23, 2026, 4:04 AM GMT+13", firstDrafted);
  await kiwi.reload();
  const nextDrafted = await drafted.textContent();
  check("the next page's server writes it in that zone, with no script", nextDrafted === firstDrafted && (await drafted.getAttribute("data-on")) === null, nextDrafted);
  await kiwi.context().close();

  // The orders page: the kit's other components, each as plain HTML that works before any script.
  const orders = await (await browser.newContext()).newPage();
  const ordersErrors = [];
  orders.on("console", (m) => m.type() === "error" && !bug(m.text()) && ordersErrors.push(m.text()));
  orders.on("pageerror", (e) => ordersErrors.push(e.message));
  await orders.goto(`${base}/orders`);
  // Alert's role follows its variant: a warning is an alert, read at once.
  const alertRole = await orders.locator("strong", { hasText: "past its ship-by date" }).evaluate((s) => s.parentElement?.getAttribute("role"));
  check("Alert: a warning is role=alert", alertRole === "alert", String(alertRole));
  // Badge's tones: the paid order's text and its background meet WCAG AA (4.5:1), as drawn.
  const paid = await orders.locator("td > span", { hasText: /^Paid$/ }).first().evaluate((el) => {
    // `rgb(r, g, b)`, or `color(srgb r g b)` (0 to 1) for a color-mix in sRGB.
    const rgb = (/** @type {string} */ c) => (c.match(/[\d.]+/g) ?? []).slice(0, 3).map((v) => (c.startsWith("color(srgb") ? Number(v) * 255 : Number(v)));
    const lum = (/** @type {number[]} */ c) => c.map((v) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)).reduce((s, v, i) => s + v * [0.2126, 0.7152, 0.0722][i], 0);
    const style = getComputedStyle(el);
    const [a, b] = [lum(rgb(style.color)), lum(rgb(style.backgroundColor))].sort((x, y) => y - x);
    return { ratio: (a + 0.05) / (b + 0.05), color: style.color, background: style.backgroundColor };
  });
  check("Badge: a soft success badge's text meets 4.5:1", paid.ratio >= 4.5, JSON.stringify(paid));
  // A look is classes on the app's own element: the badge is the cell's child, a button's parent is
  // the app's, and no element of the kit's takes no box around one.
  const looks = await orders.evaluate(() => ({
    badge: document.querySelector("td > [class*='badge__badge']")?.tagName ?? "missing",
    wrappers: [...document.querySelectorAll(["button", "input", "select", "textarea", "slider", "badge", "card"].map((n) => `[class*='${n}__']`).join())]
      .filter((e) => getComputedStyle(e).display === "contents").length,
    button: document.querySelector("[class*='button__button']")?.parentElement?.className.includes("button__") ?? "missing",
  }));
  check("Looks: the kit's classes are on the app's elements, with none of its own around them", looks.badge === "SPAN" && looks.wrappers === 0 && looks.button === false, JSON.stringify(looks));
  // Meter: the native element, named by its field's label, its fill in the state's color: the
  // limit's value sits between low and high with optimum under low, one range off the good one.
  // (By its label: Playwright names a meter from its fallback text, where browsers use the label.)
  const meter = await orders.getByLabel("Sold this month").evaluate((m) => {
    const el = /** @type {HTMLMeterElement} */ (m);
    const box = el.getBoundingClientRect();
    return { tag: el.tagName, value: el.value, max: el.max, width: Math.round(box.width), height: Math.round(box.height) };
  });
  check(
    "Meter: a <meter> named by its label, drawn as a bar",
    meter.tag === "METER" && meter.value === 12375 && meter.max === 15000 && meter.width > 100 && meter.height === 8,
    JSON.stringify(meter),
  );
  // Its fill, as drawn: the value is between low and high with optimum under low, so the warning
  // color, sampled from a screenshot inside the filled part (82.5% of the bar).
  // The token as the page has it, read without writing a style (the CSP allows none). The
  // screenshot is a page of its own: WebKit's adds a stylesheet the CSP refuses, which is
  // Playwright's, not the page's, so it stays out of this page's console check.
  const [token, track] = await orders.evaluate(() => ["--warning", "--border"].map((p) => getComputedStyle(document.documentElement).getPropertyValue(p).trim()));
  const probe = await (await browser.newContext()).newPage();
  await probe.goto(`${base}/orders`);
  const shot = (await probe.getByLabel("Sold this month").screenshot()).toString("base64");
  await probe.context().close();
  const blank = await orders.context().newPage();
  const { drawn, want, past, empty } = await blank.evaluate(
    async ([png, color, rest]) => {
      const img = new Image();
      img.src = `data:image/png;base64,${png}`;
      await img.decode();
      const canvas = document.createElement("canvas");
      [canvas.width, canvas.height] = [img.width, img.height];
      const g = /** @type {CanvasRenderingContext2D} */ (canvas.getContext("2d"));
      g.drawImage(img, 0, 0);
      const at = (/** @type {number} */ x) => [...g.getImageData(Math.round(img.width * x), Math.floor(img.height / 2), 1, 1).data].slice(0, 3);
      // Inside the fill, and past the value on the track (clear of its rounded end).
      const [drawn, past] = [at(0.4), at(0.95)];
      // Each token's color, as the canvas paints it.
      const paint = (/** @type {string} */ c) => {
        g.fillStyle = c;
        g.fillRect(0, 0, 1, 1);
        return [...g.getImageData(0, 0, 1, 1).data].slice(0, 3);
      };
      return { drawn, want: paint(color), past, empty: paint(rest) };
    },
    [shot, token, track],
  );
  await blank.close();
  check(
    "Meter: drawn in the warning color between low and high, up to its value",
    drawn.every((c, i) => Math.abs(c - want[i]) <= 6) && past.every((c, i) => Math.abs(c - empty[i]) <= 6),
    JSON.stringify({ drawn, want, past, empty }),
  );
  // Stats: a term and its descriptions, as many to a row as fit and two on a phone.
  const stats = await orders.evaluate(() => {
    const dl = document.querySelector("dl[class*='stats__stats']");
    const items = [...(dl?.children ?? [])];
    return {
      terms: items.map((i) => i.querySelector("dt")?.textContent).join(),
      rows: new Set(items.map((i) => Math.round(i.getBoundingClientRect().top))).size,
    };
  });
  check("Stats: labelled figures on one row", stats.terms === "Sales,Orders,To ship,Late" && stats.rows === 1, JSON.stringify(stats));
  await orders.setViewportSize({ width: 390, height: 844 });
  const phone = await orders.evaluate(() => [...document.querySelectorAll("dl[class*='stats__stats'] > div")].map((i) => Math.round(i.getBoundingClientRect().top)));
  check("Stats: two to a row on a phone", new Set(phone).size === 2 && phone[0] === phone[1] && phone[2] === phone[3], JSON.stringify(phone));
  await orders.setViewportSize({ width: 1280, height: 800 });
  // StatusDot: drawn beside the words, not read; Stack and Cluster: the gap is the space.
  const spacing = await orders.evaluate(() => {
    const dot = document.querySelector("[class*='status-dot__dot']");
    const stack = document.querySelector("form[class*='stack__stack']");
    return {
      dot: dot?.getAttribute("aria-hidden") === "true" && (dot.parentElement?.textContent ?? "").includes("Taking orders"),
      // Gap::Lg, 1.5rem.
      gap: stack ? parseFloat(getComputedStyle(stack).rowGap) / parseFloat(getComputedStyle(document.documentElement).fontSize) : "missing",
      // A paragraph's default margins (1em) are dropped: the form has one among its fields.
      paragraph: stack?.querySelector(":scope > p") ? 1 : 0,
      margins: stack ? [...stack.children].filter((c) => parseFloat(getComputedStyle(c).marginBlockStart) + parseFloat(getComputedStyle(c).marginBlockEnd) !== 0).length : -1,
    };
  });
  check("StatusDot and Stack: the dot is not read, and a stack's gap is its only space", spacing.dot && spacing.gap === 1.5 && spacing.paragraph === 1 && spacing.margins === 0, JSON.stringify(spacing));
  await orders.getByRole("button", { name: "Share the shop" }).click();
  check("Popover: its button opens it", await orders.locator("#share").evaluate((p) => p.matches(":popover-open")));
  await orders.keyboard.press("Escape");
  check("Popover: Escape closes it", !(await orders.locator("#share").evaluate((p) => p.matches(":popover-open"))));
  await orders.getByText("What if an order ships late?").click();
  const open = await orders.locator("details").evaluateAll((all) => all.map((d) => /** @type {HTMLDetailsElement} */ (d).open).join());
  check("Accordion: one section open at a time", open === "false,true,false", open);
  await orders.getByRole("link", { name: "Next" }).click();
  await orders.locator("caption", { hasText: "Page 2 of 3" }).waitFor({ timeout: 5000 }).catch(() => {});
  check("Pagination: Next goes to the next page", new URL(orders.url()).search === "?page=2" && (await orders.locator('[aria-current="page"]').last().textContent()) === "2", orders.url());
  await orders.getByLabel("Ship from").selectOption("toronto");
  await orders.getByRole("switch", { name: "Email me when an order ships" }).uncheck();
  await orders.getByRole("button", { name: "Save settings" }).click();
  if (engine !== "webkit") {
    await orders.locator("#ui-toast:not([hidden])").waitFor({ timeout: 5000 }).catch(() => {});
    const saved = await orders.locator("#ui-toast").textContent();
    check("Select, Slider, Switch and Textarea post with the form", saved === "Saved: ships from Toronto, ON in 2 days with your note; shipping emails off.", saved ?? "");
  } else skip("Select, Slider, Switch and Textarea post with the form", "Playwright's WebKit never sends a Secure cookie back over http");
  check("the orders page: no console errors", ordersErrors.length === 0, ordersErrors.join(" | "));
  await orders.context().close();

  // Without JavaScript
  const plain = await (await browser.newContext({ javaScriptEnabled: false })).newPage();
  await plain.goto(`${base}/`);
  check("no script: the server's relative time is the text", (await plain.locator("time").first().textContent()) === "3 hr. ago");
  const plainDrafted = plain.locator("p", { hasText: "Drafted" }).locator("time");
  check("no script: a first page's date is in UTC", (await plainDrafted.textContent()) === "Mar 22, 2026, 3:04 PM UTC", await plainDrafted.textContent());
  check("no script: the select is the control", (await plain.locator("[data-scope=ui-combo-box] select").isVisible()) && (await plain.locator("[data-scope=ui-combo-box] input").isHidden()));
  await plain.locator("[data-scope=ui-combo-box] select").selectOption("lp");
  // The post loads a new page; waitForLoadState alone can return for the page it left.
  await Promise.all([plain.waitForURL("**/sell"), plain.getByRole("button", { name: "List for sale" }).click()]);
  check("no script: the form posts the page", (await plain.textContent("#result")) === "Listed 1 Lightly Played Ashen Drake at $12.50 each, 0.5 oz to ship.", await plain.textContent("#result"));
  const plainFloor = plain.getByLabel("Lowest offer");
  check("no script: the lowest offer is enabled and optional (the handler checks it)", (await plainFloor.isEnabled()) && !(await plainFloor.evaluate((i) => /** @type {HTMLInputElement} */ (i).required)));
  await plain.getByRole("tab", { name: "Offers" }).click();
  await plain.waitForURL("**/?tab=offers");
  check("no script: a tab is a link to its page", (await plain.getByRole("tabpanel").textContent()) === "Two offers, the best at $10.00.");
  await plain.getByRole("button", { name: "More actions" }).click();
  check("no script: the menu opens (popover)", await plain.getByRole("menuitem", { name: "Help" }).isVisible());
  await plain.goto(`${base}/`);
  await Promise.all([plain.waitForURL("**/save"), plain.getByRole("button", { name: "Save draft" }).click()]);
  check("no script: the page the post answers with shows the toast", (await plain.locator("#ui-toast").textContent()) === "Draft saved." && (await plain.locator("#ui-toast").isVisible()));
  await plain.getByRole("button", { name: "Rename draft" }).click();
  check("no script: the button opens the dialog", await plain.locator("#rename").isVisible());
  await Promise.all([plain.waitForURL("**/rename"), plain.getByRole("button", { name: "Rename", exact: true }).click()]);
  check("no script: refused, the dialog is open with the error", (await plain.locator("#rename").isVisible()) && ((await plain.locator("#rename").textContent()) ?? "").includes("Give the draft a name."));
  if (engine !== "webkit") {
    await plain.locator("#draft-name").fill("Winter draft");
    await Promise.all([plain.waitForURL(`${base}/`), plain.getByRole("button", { name: "Rename", exact: true }).click()]);
    check("no script: the page a redirect lands on shows its notice in the layout", (await plain.locator("#ui-toast").textContent()) === "Renamed to Winter draft.", await plain.locator("#ui-toast").textContent());
    await plain.reload();
    check("no script: a reload does not say it again", (await plain.locator("#ui-toast").textContent()) === "");
  } else for (const name of ["no script: the page a redirect lands on shows its notice in the layout", "no script: a reload does not say it again"]) skip(name, "Playwright's WebKit never sends a Secure cookie back over http");
  await plain.goto(`${base}/?sale=last`);
  const plainSheet = plain.locator('dialog[aria-label="Last sale"]');
  check("no script: the address opens the sheet", await plainSheet.isVisible());
  // The sheet slides in; without the page's script Chromium does not tell Playwright the link has
  // stopped moving, so the click waits for the slide itself to end. Asked again until it has (a
  // promise does not settle in Firefox's page without script), for at most the 200 ms slide many
  // times over.
  const sliding = () => plainSheet.evaluate((sheet) => sheet.getAnimations().some((a) => a.playState === "running"));
  for (let tries = 0; tries < 40 && (await sliding()); tries++) await plain.waitForTimeout(50);
  await Promise.all([plain.waitForURL((url) => !url.searchParams.has("sale")), plain.getByRole("link", { name: "Close" }).click()]);
  check("no script: its link closes it", (await plainSheet.count()) === 0);
  await plain.goto(`${base}/`);
  check("no script: the SKU is on the page and the copy button is not", (await plain.locator("code", { hasText: "ASH-DRAKE-NM" }).isVisible()) && (await plain.locator("[data-scope=ui-copy-button]").isHidden()));
  const plainPrint = plain.locator("button", { hasText: "Print label" });
  check("no script: the print button is on the page, hidden", (await plainPrint.count()) === 1 && (await plainPrint.isHidden()));
  await plain.goto(`${base}/orders`);
  await plain.getByRole("button", { name: "Share the shop" }).click();
  check("no script: the popover opens", await plain.locator("#share").isVisible());
  await plain.keyboard.press("Escape");
  await Promise.all([plain.waitForURL("**/orders?page=3"), plain.getByRole("link", { name: "3", exact: true }).click()]);
  check("no script: a page number is a link to that page", (await plain.locator("caption").textContent()) === "Page 3 of 3");
}
