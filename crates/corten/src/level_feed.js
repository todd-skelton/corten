// @ts-check
// The script of the doctests of a feed's URL (script.rs): a handler that takes a FeedUrl<Level>.
import { subscribe } from "corten";
/** @import { FeedUrl } from "corten" */

/** @typedef {{ id: string, qty: number }} Level A row of the levels feed. */

/** @param {HTMLTableSectionElement} body @param {FeedUrl<Level>} feed @param {AbortSignal} signal */
export function follow(body, feed, signal) {
  subscribe(feed, { snapshot: (levels) => (body.dataset.rows = String(levels.length)), patch() {} }, { signal });
}
