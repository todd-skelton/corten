// @ts-check
// FeedMonitor (feed_monitor.rs): follows a seller's listings feed and shows what arrived. The counts
// are the outputs' text, so the page holds the state.
import { subscribe } from "corten";

/**
 * The outputs the monitor writes. The markup builds it once, `js::Shown { .. }`.
 * @typedef {object} Shown
 * @property {HTMLOutputElement} status
 * @property {HTMLOutputElement} listings
 * @property {HTMLOutputElement} through
 * @property {HTMLOutputElement} changes
 * @property {HTMLOutputElement} snapshots
 */

/** @param {HTMLOutputElement} output @param {number} by */
const add = (output, by) => {
  output.value = String(Number(output.value) + by);
};

/**
 * Follows the feed until the monitor leaves the page.
 * @param {Shown} shown @param {string} feed @param {AbortSignal} signal
 */
export function follow(shown, feed, signal) {
  subscribe(
    feed,
    {
      snapshot(rows, seq) {
        shown.listings.value = String(rows.length);
        shown.through.value = String(seq);
        add(shown.snapshots, 1);
      },
      patch(_changes, seq) {
        shown.through.value = String(seq);
        add(shown.changes, 1);
      },
      status(status) {
        shown.status.value = status;
      },
    },
    { signal },
  );
}
