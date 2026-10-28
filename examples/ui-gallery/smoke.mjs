// Browser smoke test of corten-ui's components, against the ui-gallery example, in Chromium,
// Firefox and WebKit (Playwright's builds: `npx playwright install chromium firefox webkit`).
// Usage: node examples/ui-gallery/smoke.mjs [http://localhost:18482] [chromium|firefox|webkit ...]
import { chromium, firefox, webkit } from "@playwright/test";

const base = process.argv[2] ?? "http://localhost:18482";
const engines = { chromium, firefox, webkit };
const names = process.argv.slice(3).length ? process.argv.slice(3) : Object.keys(engines);
const failures = [];
let engine = "";
const check = (name, ok, detail = "") => {
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
  const browser = await engines[engine].launch({ headless: true });
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
  const page = await (await browser.newContext()).newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
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
  check("the six scripts load and connect", loaded === 6, `${loaded} script(s)`);

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
  check("a step keeps the step's decimals", (await page.locator("#price").inputValue()) === "12.51", await page.locator("#price").inputValue());
  // Each field keeps its own state (keep()): while the quantity's button is held, the price's
  // button still steps from the keyboard. Sharing one, the held pointer would make it skip.
  await qty.focus();
  await page.keyboard.press("Home");
  const before = Number(await qty.inputValue());
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  const holding = Number(await qty.inputValue());
  await page.locator("[data-scope=ui-number-field]").nth(1).locator("button").nth(1).press("Enter");
  const own = await page.locator("#price").inputValue();
  await page.mouse.up();
  // The quantity stepped on the press, so its pointer was held while the price stepped.
  check("each field keeps its own state", holding > before && own === "12.52", `quantity ${before} -> ${holding}, price ${own}`);
  const layout = await page.evaluate(() => {
    // The number field's root is a div; the wrapper that lays out its parts and the adornment holds it.
    const field = /** @type {Element} */ (document.querySelector("#price")?.closest("[data-scope=ui-number-field]")?.parentElement);
    return Array.from(field.querySelectorAll("button, span, input"), (el) => [el.localName, Math.round(el.getBoundingClientRect().x)])
      .sort((a, b) => Number(a[1]) - Number(b[1]))
      .map((p) => p[0])
      .join(" ");
  });
  check("the design system orders an adornment between the parts", layout === "button span input button", layout);

  // ComboBox
  const color = page.getByLabel("Color");
  check("the label names the text input once the script runs", (await color.getAttribute("role")) === "combobox");
  check("the select is hidden", await page.locator("[data-scope=ui-combo-box] select").isHidden());
  await color.fill("sand");
  const popup = page.locator("[data-scope=ui-combo-box] [popover]");
  check("typing opens the list", (await popup.evaluate((p) => p.matches(":popover-open"))) && (await color.getAttribute("aria-expanded")) === "true");
  const shown = await page.locator('[data-scope=ui-combo-box] [role="option"]:visible').allTextContents();
  check("the list filters by what was typed", shown.join() === "Desert Sand", shown.join());
  check("the first match is highlighted", (await color.getAttribute("aria-activedescendant")) !== null);
  await page.keyboard.press("Enter");
  check("Enter picks it", (await color.inputValue()) === "Desert Sand" && (await page.locator("[data-scope=ui-combo-box] select").inputValue()) === "sand");
  check("picking closes the list", (await color.getAttribute("aria-expanded")) === "false" && !(await popup.evaluate((p) => p.matches(":popover-open"))));
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  const active = await color.getAttribute("aria-activedescendant");
  check("Arrow Down opens at the value and moves", active !== null && (await page.locator(`#${active}`).textContent()) === "Forest Green");
  await page.keyboard.press("Escape");
  check("Escape closes and keeps the value", (await color.inputValue()) === "Desert Sand" && (await color.getAttribute("aria-expanded")) === "false");
  await color.fill("zzz");
  check("no match shows the status", await page.locator('[data-scope=ui-combo-box] [role="status"]').isVisible());
  await page.keyboard.press("Tab");
  check("leaving restores the value's text", (await color.inputValue()) === "Desert Sand");
  await color.click();
  await page.locator('[data-scope=ui-combo-box] [role="option"]', { hasText: "Salt & Pepper Gray" }).click();
  check("a click picks", (await page.locator("[data-scope=ui-combo-box] select").inputValue()) === "gray" && (await color.inputValue()) === "Salt & Pepper Gray");
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
  await page.getByRole("button", { name: "Order" }).click();
  const message = await color.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage);
  check("a required, empty combobox stops the form on the input", message !== "", message);
  await color.fill("forest green");
  await page.keyboard.press("Tab");
  check("typing a label exactly picks it", (await page.locator("[data-scope=ui-combo-box] select").inputValue()) === "forest");

  await qty.fill("3");
  await page.getByRole("button", { name: "Order" }).click();
  await page.waitForFunction(() => document.querySelector("#result")?.textContent?.startsWith("Ordered"));
  check("the form sends the components' values", (await page.textContent("#result")) === "Ordered 3 in Forest Green at $12.52.", await page.textContent("#result"));

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

  // MenuButton
  // The popover's toggle event comes a task after it opens or closes: wait for what it sets.
  const trigger = page.getByRole("button", { name: "Actions" });
  const expanded = (/** @type {string} */ value) =>
    page.waitForFunction((v) => document.querySelector("#actions-button")?.getAttribute("aria-expanded") === v, value);
  await trigger.click();
  await expanded("true");
  check("the button opens the menu", (await trigger.getAttribute("aria-expanded")) === "true");
  check("focus moves to the first item", (await page.evaluate(() => document.activeElement?.textContent)) === "Help");
  const menuBox = await page.evaluate(() => {
    const button = document.querySelector("#actions-button")?.getBoundingClientRect();
    const menu = document.querySelector("#actions")?.getBoundingClientRect();
    return button && menu && { gap: menu.top - button.bottom, left: menu.left - button.left };
  });
  check("the menu is anchored under its button", !!menuBox && menuBox.gap >= 0 && menuBox.gap <= 8 && Math.abs(menuBox.left) <= 1, JSON.stringify(menuBox));
  await page.keyboard.press("ArrowDown");
  check("Arrow Down moves", (await page.evaluate(() => document.activeElement?.textContent)) === "Archive");
  await page.keyboard.press("w");
  check("typing a letter moves to the item", (await page.evaluate(() => document.activeElement?.textContent)) === "Watch");
  await page.keyboard.press("Escape");
  await expanded("false");
  check("Escape closes and returns focus", (await trigger.getAttribute("aria-expanded")) === "false" && (await page.evaluate(() => document.activeElement?.textContent)) === "Actions");
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

  // Toaster (2 seconds in the gallery)
  const toast = page.locator("#ui-toast");
  const dismiss = page.getByRole("button", { name: "Dismiss" });
  check("no message: nothing shows", (await toast.isHidden()) && (await dismiss.isHidden()));
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

  // dialog::modalClosedBy: a sheet the address opens, which Escape closes by following its link.
  await page.goto(`${base}/`);
  /** This entry's place in the history, where the browser has the Navigation API (a browser without it loads whole pages, and the link's address replaces the sheet's). */
  const at = () => page.evaluate(() => /** @type {any} */ (window).navigation?.currentEntry?.index ?? -1);
  const listEntry = await at();
  await Promise.all([page.waitForURL("**/?sale=last"), page.getByRole("link", { name: "Last sale" }).click()]);
  const sheet = page.locator('dialog[aria-label="Last sale"]');
  const sheetModal = () => page.evaluate(() => !!document.querySelector('dialog[aria-label="Last sale"]')?.matches(":modal"));
  await page.waitForFunction(() => document.querySelector('dialog[aria-label="Last sale"]')?.matches(":modal"), null, { timeout: 5000 }).catch(() => {});
  check("the address opens the sheet as a modal", await sheetModal());
  /** Escape on the sheet, then the page without it: the address drops `sale` and the sheet goes. */
  const escape = async (/** @type {string} */ how) => {
    await page.keyboard.press("Escape");
    await page.waitForURL((url) => !url.searchParams.has("sale"), { timeout: 5000 }).catch(() => {});
    await sheet.waitFor({ state: "detached", timeout: 5000 }).catch(() => {});
    check(`${how}: Escape follows the close link, the address no longer names the sale`, !new URL(page.url()).searchParams.has("sale"), page.url());
    check(`${how}: the page without it replaces the sheet`, (await sheet.count()) === 0);
  };
  const openSheet = async () => {
    await Promise.all([page.waitForURL("**/?sale=last"), page.getByRole("link", { name: "Last sale" }).click()]);
    await page.waitForFunction(() => document.querySelector('dialog[aria-label="Last sale"]')?.matches(":modal"), null, { timeout: 5000 }).catch(() => {});
  };
  // Opened from the list, closing goes back to the list's entry: no entry is added, so Back
  // after it leaves the list, and Android's Back on the open sheet cannot loop through it.
  await escape("opened by a click");
  check("opened by a click: closing goes back to the list's entry", (await at()) === listEntry, `${listEntry} -> ${await at()}`);
  await page.reload();
  check("a reload does not open it again", (await sheet.count()) === 0);
  // A close request other than Escape (Android's Back, closedby="any") is the same `cancel`.
  if (await page.evaluate(() => "requestClose" in HTMLDialogElement.prototype)) {
    await openSheet();
    await sheet.evaluate((d) => /** @type {any} */ (d).requestClose());
    await page.waitForURL((url) => !url.searchParams.has("sale"), { timeout: 5000 }).catch(() => {});
    await sheet.waitFor({ state: "detached", timeout: 5000 }).catch(() => {});
    check("requestClose() leaves the sheet's address too", !new URL(page.url()).searchParams.has("sale") && (await sheet.count()) === 0, page.url());
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
  check("Back from there goes to the page before the shared link", new URL(page.url()).searchParams.get("tab") === "offers", page.url());
  // The refused renames are 422s, which Chromium and WebKit log; nothing else may be.
  const unexpected = errors.filter((e) => !/status of 422/.test(e));
  check("no console errors", unexpected.length === 0, unexpected.join(" | "));

  // Without JavaScript
  const plain = await (await browser.newContext({ javaScriptEnabled: false })).newPage();
  await plain.goto(`${base}/`);
  check("no script: the select is the control", (await plain.locator("[data-scope=ui-combo-box] select").isVisible()) && (await plain.locator("[data-scope=ui-combo-box] input").isHidden()));
  await plain.locator("[data-scope=ui-combo-box] select").selectOption("sand");
  // The post loads a new page; waitForLoadState alone can return for the page it left.
  await Promise.all([plain.waitForURL("**/order"), plain.getByRole("button", { name: "Order" }).click()]);
  check("no script: the form posts the page", (await plain.textContent("#result")) === "Ordered 1 in Desert Sand at $12.50.", await plain.textContent("#result"));
  await plain.getByRole("tab", { name: "Offers" }).click();
  await plain.waitForURL("**/?tab=offers");
  check("no script: a tab is a link to its page", (await plain.getByRole("tabpanel").textContent()) === "Two offers, the best at $10.00.");
  await plain.getByRole("button", { name: "Actions" }).click();
  check("no script: the menu opens (popover)", await plain.getByRole("menuitem", { name: "Help" }).isVisible());
  await plain.goto(`${base}/`);
  await Promise.all([plain.waitForURL("**/save"), plain.getByRole("button", { name: "Save draft" }).click()]);
  check("no script: the page the post answers with shows the toast", (await plain.locator("#ui-toast").textContent()) === "Draft saved." && (await plain.locator("#ui-toast").isVisible()));
  await plain.getByRole("button", { name: "Rename draft" }).click();
  check("no script: the button opens the dialog", await plain.locator("#rename").isVisible());
  await Promise.all([plain.waitForURL("**/rename"), plain.getByRole("button", { name: "Rename", exact: true }).click()]);
  check("no script: refused, the dialog is open with the error", (await plain.locator("#rename").isVisible()) && ((await plain.locator("#rename").textContent()) ?? "").includes("Give the draft a name."));
  await plain.goto(`${base}/?sale=last`);
  const plainSheet = plain.locator('dialog[aria-label="Last sale"]');
  check("no script: the address opens the sheet", await plainSheet.isVisible());
  await Promise.all([plain.waitForURL((url) => !url.searchParams.has("sale")), plain.getByRole("link", { name: "Close" }).click()]);
  check("no script: its link closes it", (await plainSheet.count()) === 0);
}
