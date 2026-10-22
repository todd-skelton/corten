// @ts-check
// MenuButton (menu_button.rs). The browser opens and dismisses the menu (popover,
// popovertarget); this adds the menu keys, focus and aria-expanded, and names the button as the
// menu's anchor, so the design system places the menu with CSS anchor positioning.
import { state } from "corten";

/** The item to focus when the menu opens: first (0) or last (-1). @param {HTMLDivElement} menu */
const opening = (menu) => state(menu, () => ({ start: 0 }));

/** @param {HTMLButtonElement} trigger @param {HTMLDivElement} menu @param {HTMLElement[]} items */
export function connect(trigger, menu, items) {
  // Without this script Tab reaches each item; with it, the arrow keys do.
  for (const item of items) item.tabIndex = -1;
  const name = `--${CSS.escape(trigger.id)}`;
  trigger.style.setProperty("anchor-name", name);
  menu.style.setProperty("position-anchor", name);
}

/** Arrow Down or Up on the button opens the menu at the first or last item. @param {KeyboardEvent} event @param {HTMLDivElement} menu */
export function open(event, menu) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();
  opening(menu).start = event.key === "ArrowDown" ? 0 : -1;
  menu.showPopover();
}

/** @param {ToggleEvent} event @param {HTMLButtonElement} trigger @param {HTMLDivElement} menu @param {HTMLElement[]} items */
export function toggled(event, trigger, menu, items) {
  const open = event.newState === "open";
  trigger.setAttribute("aria-expanded", String(open));
  if (open) {
    const s = opening(menu);
    items.at(s.start)?.focus();
    s.start = 0;
  } else if (menu.contains(document.activeElement)) {
    trigger.focus();
  }
}

/** @param {KeyboardEvent} event @param {HTMLDivElement} menu @param {HTMLElement[]} items */
export function step(event, menu, items) {
  const at = items.indexOf(/** @type {any} */ (document.activeElement));
  /** @type {number} */
  let to;
  if (event.key === "ArrowDown") to = (at + 1) % items.length;
  else if (event.key === "ArrowUp") to = (at - 1 + items.length) % items.length;
  else if (event.key === "Home") to = 0;
  else if (event.key === "End") to = items.length - 1;
  else if (event.key === "Tab") return void menu.hidePopover();
  else if (event.key.length === 1 && /\S/.test(event.key)) to = typeahead(items, at, event.key);
  else return;
  event.preventDefault();
  items[to]?.focus();
}

/** After the click: a link still navigates and a form still submits. @param {HTMLDivElement} menu */
export function used(menu) {
  queueMicrotask(() => menu.matches(":popover-open") && menu.hidePopover());
}

/**
 * The next item after `at` whose text starts with `key`, or `at`.
 * @param {HTMLElement[]} items @param {number} at @param {string} key
 */
function typeahead(items, at, key) {
  const letter = key.toLocaleLowerCase();
  for (let i = 1; i <= items.length; i++) {
    const n = (at + i) % items.length;
    if (items[n].textContent?.trim().toLocaleLowerCase().startsWith(letter)) return n;
  }
  return at;
}
