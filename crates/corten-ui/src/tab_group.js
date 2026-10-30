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
 * Selects `tab`: its panel shows, and the address follows a link tab (in the keys the tabs
 * differ in).
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
  if (tab instanceof HTMLAnchorElement) {
    const to = address(tab, tabs);
    if (to !== location.href) history.replaceState(history.state, "", to);
  }
}

/**
 * The address with `tab` selected: its link's, for a link to another page; on this page, the
 * current address with only the query keys the tabs' links differ in taken from it, so another
 * control's keys (a choice, a page) stay as they are.
 * @param {HTMLAnchorElement} tab @param {HTMLElement[]} tabs @returns {string}
 */
function address(tab, tabs) {
  const to = new URL(tab.href);
  const now = new URL(location.href);
  if (to.origin !== now.origin || to.pathname !== now.pathname) return to.href;
  const links = tabs.filter((l) => l instanceof HTMLAnchorElement).map((l) => new URL(/** @type {HTMLAnchorElement} */ (l).href).searchParams);
  const differ = (/** @type {string} */ key) => new Set(links.map((q) => q.getAll(key).join("\u0000"))).size > 1;
  // In the link's order, the route's, then any key only the address has.
  const query = new URLSearchParams();
  for (const key of new Set([...to.searchParams.keys(), ...now.searchParams.keys()])) {
    for (const value of (differ(key) ? to.searchParams : now.searchParams).getAll(key)) query.append(key, value);
  }
  now.search = query.toString();
  return now.href;
}
