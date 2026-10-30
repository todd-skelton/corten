// @ts-check
// MenuButton (menu_button.rs). The browser opens and dismisses the menu (popover,
// popovertarget); this adds the menu keys, focus and aria-expanded, and names the button as the
// menu's anchor, so the design system places the menu with CSS anchor positioning.
import { keep } from "corten";

/**
 * The item to focus when the menu opens (first, 0, or last, -1), and whether focus was in the
 * menu as it closed.
 */
const opening = () => keep(() => ({ start: 0, focused: false }));

/** An item of the menu, of any of the three kinds. */
const ITEM = "[role=menuitem],[role=menuitemcheckbox],[role=menuitemradio]";

/**
 * The menu's items, the app's own elements: read when they are needed, so one a patch adds or
 * takes away counts too.
 * @param {HTMLDivElement} menu
 * @returns {HTMLElement[]}
 */
const itemsOf = (menu) => /** @type {HTMLElement[]} */ ([...menu.querySelectorAll(ITEM)]);

/**
 * The items the arrow keys and letters reach: a disabled `<button>` cannot take focus, so it is
 * passed over (one that stays focusable while disabled says `aria-disabled="true"` instead).
 * @param {HTMLDivElement} menu
 */
const reachable = (menu) => itemsOf(menu).filter((item) => !item.matches(":disabled"));

/** Without this script Tab reaches each item; with it, the arrow keys do. @param {HTMLDivElement} menu */
function outOfTabOrder(menu) {
  for (const item of itemsOf(menu)) item.tabIndex = -1;
}

/** @param {HTMLButtonElement} trigger @param {HTMLDivElement} menu */
export function connect(trigger, menu) {
  outOfTabOrder(menu);
  const name = `--${CSS.escape(trigger.id)}`;
  trigger.style.setProperty("anchor-name", name);
  menu.style.setProperty("position-anchor", name);
}

/** Arrow Down or Up on the button opens the menu at the first or last item. @param {KeyboardEvent} event @param {HTMLDivElement} menu */
export function open(event, menu) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();
  opening().start = event.key === "ArrowDown" ? 0 : -1;
  menu.showPopover();
}

/**
 * Before the menu closes, while focus is still in it. WebKit moves focus out of a closing popover
 * (to whatever had it before, often the body: a clicked button is not focused there) before
 * `toggle` runs, so `toggled` cannot tell from `document.activeElement` alone.
 * @param {ToggleEvent} event @param {HTMLDivElement} menu
 */
export function closing(event, menu) {
  if (event.newState === "closed") opening().focused = menu.contains(document.activeElement);
}

/** @param {ToggleEvent} event @param {HTMLButtonElement} trigger @param {HTMLDivElement} menu */
export function toggled(event, trigger, menu) {
  const open = event.newState === "open";
  trigger.setAttribute("aria-expanded", String(open));
  const s = opening();
  if (open) {
    outOfTabOrder(menu);
    reachable(menu).at(s.start)?.focus();
    s.start = 0;
    return;
  }
  // Back to the button when focus was in the menu, unless a click outside moved it somewhere.
  const now = document.activeElement;
  if (s.focused && (!now || now === document.body || menu.contains(now))) trigger.focus();
  s.focused = false;
  // An item opened a dialog: closing it gives focus back to the item, now hidden in the closed
  // menu, so some browsers leave it on the page's body. The button takes it then.
  const dialog = now?.closest("dialog[open]");
  if (dialog && !dialog.contains(trigger)) {
    dialog.addEventListener(
      "close",
      () => {
        const at = document.activeElement;
        if (!at || at === document.body || menu.contains(at)) trigger.focus();
      },
      { once: true },
    );
  }
}

/** @param {KeyboardEvent} event @param {HTMLDivElement} menu */
export function step(event, menu) {
  const items = reachable(menu);
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

/**
 * A click on an item closes the menu, after the click: a link still navigates, a form still
 * submits and a command still opens its dialog.
 * @param {MouseEvent} event @param {HTMLDivElement} menu
 */
export function used(event, menu) {
  const item = event.target instanceof Element && event.target.closest(ITEM);
  if (!item || !menu.contains(item)) return;
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
