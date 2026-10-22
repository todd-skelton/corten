// Browser smoke test of corten-ui's islands, against the ui-gallery example.
// Usage: node examples/ui-gallery/smoke.mjs [http://localhost:18482]
import { chromium } from "@playwright/test";

const base = process.argv[2] ?? "http://localhost:18482";
const failures = [];
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "ok  " : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures.push(name);
};

const browser = await chromium.launch({ headless: true });
try {
  const page = await (await browser.newContext()).newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(`${base}/`);
  await page.waitForFunction(() => ["number-field", "combo-box", "tab-group", "menu-button"].every((t) => customElements.get(t)));
  check("the four islands load", true);

  // NumberField
  const qty = page.locator("#qty");
  const qtyButtons = page.locator("number-field").first().locator("button");
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
  await page.locator("number-field").nth(1).locator("button").nth(1).click();
  check("a step keeps the step's decimals", (await page.locator("#price").inputValue()) === "12.51", await page.locator("#price").inputValue());
  const layout = await page.evaluate(() => {
    const field = /** @type {Element} */ (document.querySelector("#price")?.closest("div"));
    return Array.from(field.querySelectorAll("button, span, input"), (el) => [el.localName, Math.round(el.getBoundingClientRect().x)])
      .sort((a, b) => Number(a[1]) - Number(b[1]))
      .map((p) => p[0])
      .join(" ");
  });
  check("the design system orders an adornment between the parts", layout === "button span input button", layout);

  // ComboBox
  const set = page.getByLabel("Set");
  check("the label names the text input once the script runs", (await set.getAttribute("role")) === "combobox");
  check("the select is hidden", await page.locator("combo-box select").isHidden());
  await set.fill("pal");
  const popup = page.locator("combo-box [popover]");
  check("typing opens the list", (await popup.evaluate((p) => p.matches(":popover-open"))) && (await set.getAttribute("aria-expanded")) === "true");
  const shown = await page.locator('combo-box [role="option"]:visible').allTextContents();
  check("the list filters by what was typed", shown.join() === "Paldea Evolved", shown.join());
  check("the first match is highlighted", (await set.getAttribute("aria-activedescendant")) !== null);
  await page.keyboard.press("Enter");
  check("Enter picks it", (await set.inputValue()) === "Paldea Evolved" && (await page.locator("combo-box select").inputValue()) === "pal");
  check("picking closes the list", (await set.getAttribute("aria-expanded")) === "false" && !(await popup.evaluate((p) => p.matches(":popover-open"))));
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  const active = await set.getAttribute("aria-activedescendant");
  check("Arrow Down opens at the value and moves", active !== null && (await page.locator(`#${active}`).textContent()) === "Obsidian Flames");
  await page.keyboard.press("Escape");
  check("Escape closes and keeps the value", (await set.inputValue()) === "Paldea Evolved" && (await set.getAttribute("aria-expanded")) === "false");
  await set.fill("zzz");
  check("no match shows the status", await page.locator('combo-box [role="status"]').isVisible());
  await page.keyboard.press("Tab");
  check("leaving restores the value's text", (await set.inputValue()) === "Paldea Evolved");
  await set.click();
  await page.locator('combo-box [role="option"]', { hasText: "Scarlet & Violet 151" }).click();
  check("a click picks", (await page.locator("combo-box select").inputValue()) === "151" && (await set.inputValue()) === "Scarlet & Violet 151");
  const placed = await popup.evaluate((p) => p.style.top !== "");
  check("the popup is placed under the input", placed);
  await set.fill("");
  await page.keyboard.press("Tab");
  check("emptying the field clears the value", (await page.locator("combo-box select").inputValue()) === "");
  await page.getByRole("button", { name: "Order" }).click();
  const message = await set.evaluate((i) => /** @type {HTMLInputElement} */ (i).validationMessage);
  check("a required, empty combobox stops the form on the input", message !== "", message);
  await set.fill("obsidian flames");
  await page.keyboard.press("Tab");
  check("typing a label exactly picks it", (await page.locator("combo-box select").inputValue()) === "obf");

  await qty.fill("3");
  await page.getByRole("button", { name: "Order" }).click();
  await page.waitForFunction(() => document.querySelector("#result")?.textContent?.startsWith("Ordered"));
  check("the form sends the islands' values", (await page.textContent("#result")) === "Ordered 3 of Obsidian Flames at $12.51.", await page.textContent("#result"));

  // TabGroup
  const tab = (name) => page.getByRole("tab", { name });
  await page.evaluate(() => (/** @type {any} */ (window).__tabs = document.querySelector("tab-group")));
  await tab("Offers").click();
  check("a click selects the tab and shows its panel", (await tab("Offers").getAttribute("aria-selected")) === "true" && (await page.getByRole("tabpanel").textContent()) === "Two offers, the best at $10.00.");
  check("the address follows the tab", page.url().endsWith("/?tab=offers"), page.url());
  check("an island's history.replaceState is not a navigation", await page.evaluate(() => (/** @type {any} */ (window).__tabs === document.querySelector("tab-group"))));
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
  check("no console errors", errors.length === 0, errors.join(" | "));

  // Without JavaScript
  const plain = await (await browser.newContext({ javaScriptEnabled: false })).newPage();
  await plain.goto(`${base}/`);
  check("no script: the select is the control", (await plain.locator("combo-box select").isVisible()) && (await plain.locator("combo-box input").isHidden()));
  await plain.locator("combo-box select").selectOption("pal");
  await plain.getByRole("button", { name: "Order" }).click();
  await plain.waitForLoadState();
  check("no script: the form posts the page", (await plain.textContent("#result")) === "Ordered 1 of Paldea Evolved at $12.50.", await plain.textContent("#result"));
  await plain.getByRole("tab", { name: "Offers" }).click();
  await plain.waitForURL("**/?tab=offers");
  check("no script: a tab is a link to its page", (await plain.getByRole("tabpanel").textContent()) === "Two offers, the best at $10.00.");
  await plain.getByRole("button", { name: "Actions" }).click();
  check("no script: the menu opens (popover)", await plain.getByRole("menuitem", { name: "Help" }).isVisible());
} finally {
  await browser.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nall passed");
