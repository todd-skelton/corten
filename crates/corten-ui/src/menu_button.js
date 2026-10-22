// @ts-check
import { island } from "corten";

// MenuButton (menu_button.rs). The browser opens and dismisses the menu (popover,
// popovertarget); this adds the menu keys, focus and aria-expanded, and names the button as the
// menu's anchor, so the design system places the menu with CSS anchor positioning.

/** @typedef {{ trigger: HTMLButtonElement, menu: HTMLDivElement }} Parts */

export default island("menu-button", {
  state: () => ({ /** The item to focus when the menu opens: first (0) or last (-1). */ start: 0 }),
  connect({ refs, all }) {
    // Without this script Tab reaches each item; with it, the arrow keys do.
    for (const item of all.item) item.tabIndex = -1;
    const name = `--${CSS.escape(refs.trigger.id)}`;
    refs.trigger.style.setProperty("anchor-name", name);
    refs.menu.style.setProperty("position-anchor", name);
  },
  actions: {
    open({ refs, state, event }) {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      event.preventDefault();
      state.start = event.key === "ArrowDown" ? 0 : -1;
      refs.menu.showPopover();
    },
    toggled({ refs, all, state, event }) {
      const open = /** @type {ToggleEvent} */ (event).newState === "open";
      refs.trigger.setAttribute("aria-expanded", String(open));
      if (open) {
        all.item.at(state.start)?.focus();
        state.start = 0;
      } else if (refs.menu.contains(document.activeElement)) {
        refs.trigger.focus();
      }
    },
    move({ refs, all, event }) {
      const items = all.item;
      const at = items.indexOf(/** @type {any} */ (document.activeElement));
      /** @type {number} */
      let to;
      if (event.key === "ArrowDown") to = (at + 1) % items.length;
      else if (event.key === "ArrowUp") to = (at - 1 + items.length) % items.length;
      else if (event.key === "Home") to = 0;
      else if (event.key === "End") to = items.length - 1;
      else if (event.key === "Tab") return void refs.menu.hidePopover();
      else if (event.key.length === 1 && /\S/.test(event.key)) to = typeahead(items, at, event.key);
      else return;
      event.preventDefault();
      items[to]?.focus();
    },
    used({ refs }) {
      // After the click: a link still navigates and a form still submits.
      queueMicrotask(() => refs.menu.matches(":popover-open") && refs.menu.hidePopover());
    },
  },
});

/**
 * The next item after `at` whose text starts with `key`, or `at`.
 * @param {HTMLElement[]} items
 * @param {number} at
 * @param {string} key
 */
function typeahead(items, at, key) {
  const letter = key.toLocaleLowerCase();
  for (let i = 1; i <= items.length; i++) {
    const n = (at + i) % items.length;
    if (items[n].textContent?.trim().toLocaleLowerCase().startsWith(letter)) return n;
  }
  return at;
}
