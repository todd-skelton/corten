// @ts-check
// TabGroup (tab_group.rs): roving focus and switching for the tabs the server rendered.

/**
 * Without this script every tab is reachable with Tab (links work on their own); with it, only
 * the selected one is, and the arrow keys move between them.
 * @param {HTMLElement[]} tabs
 */
export function connect(tabs) {
  for (const tab of tabs) tab.tabIndex = tab.getAttribute("aria-selected") === "true" ? 0 : -1;
}

/** @param {MouseEvent} event @param {HTMLElement} tab @param {HTMLElement[]} tabs @param {HTMLDivElement[]} panels */
export function choose(event, tab, tabs, panels) {
  // A modified click on a link (new tab, new window) is the browser's.
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  event.preventDefault();
  select(tab, tabs, panels);
}

/** @param {KeyboardEvent} event @param {HTMLElement} tab @param {HTMLElement[]} tabs @param {HTMLDivElement[]} panels */
export function step(event, tab, tabs, panels) {
  const at = tabs.indexOf(tab);
  const to =
    event.key === "ArrowRight" ? (at + 1) % tabs.length
    : event.key === "ArrowLeft" ? (at - 1 + tabs.length) % tabs.length
    : event.key === "Home" ? 0
    : event.key === "End" ? tabs.length - 1
    : -1;
  if (to < 0) return;
  event.preventDefault();
  tabs[to].focus();
  select(tabs[to], tabs, panels);
}

/**
 * Selects `tab`: its panel shows, and a link tab's URL becomes the address.
 * @param {HTMLElement} tab @param {HTMLElement[]} tabs @param {HTMLDivElement[]} panels
 */
function select(tab, tabs, panels) {
  for (const t of tabs) {
    const on = t === tab;
    t.setAttribute("aria-selected", String(on));
    t.tabIndex = on ? 0 : -1;
  }
  const panel = tab.getAttribute("aria-controls");
  for (const p of panels) p.hidden = p.id !== panel;
  if (tab instanceof HTMLAnchorElement && tab.href !== location.href) history.replaceState(history.state, "", tab.href);
}
