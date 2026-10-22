// @ts-check
import { island } from "corten";

// TabGroup (tab_group.rs): roving focus and switching for the tabs the server rendered.

export default island("tab-group", {
  connect({ all }) {
    // Without this script every tab is reachable with Tab (links work on their own); with it,
    // only the selected one is, and the arrow keys move between them.
    for (const tab of all.trigger) tab.tabIndex = tab.getAttribute("aria-selected") === "true" ? 0 : -1;
  },
  actions: {
    choose({ all, target, event }) {
      // A modified click on a link (new tab, new window) is the browser's.
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      select(all, target);
    },
    move({ all, target, event }) {
      const tabs = all.trigger;
      const at = tabs.indexOf(target);
      const to =
        event.key === "ArrowRight" ? (at + 1) % tabs.length
        : event.key === "ArrowLeft" ? (at - 1 + tabs.length) % tabs.length
        : event.key === "Home" ? 0
        : event.key === "End" ? tabs.length - 1
        : -1;
      if (to < 0) return;
      event.preventDefault();
      tabs[to].focus();
      select(all, tabs[to]);
    },
  },
});

/**
 * Selects `tab`: its panel shows, and a link tab's URL becomes the address.
 * @param {{ trigger: (HTMLAnchorElement | HTMLButtonElement)[], panel: HTMLDivElement[] }} all
 * @param {HTMLAnchorElement | HTMLButtonElement} tab
 */
function select(all, tab) {
  for (const t of all.trigger) {
    const on = t === tab;
    t.setAttribute("aria-selected", String(on));
    t.tabIndex = on ? 0 : -1;
  }
  const panel = tab.getAttribute("aria-controls");
  for (const p of all.panel) p.hidden = p.id !== panel;
  if (tab instanceof HTMLAnchorElement && tab.href !== location.href) history.replaceState(history.state, "", tab.href);
}
