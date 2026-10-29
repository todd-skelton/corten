// @ts-check
// ListingTable (listing_table.rs): follows the seller's listings feed with one subscription. It
// counts what arrived in the outputs (the page holds the state) and redraws the shown rows a
// snapshot or a change names; a listing the page does not show is counted, not drawn.
import { subscribe } from "corten";
import { ListingRow, rows } from "corten/views";
/** @import { MoneyFormat } from "corten" */
/** @import { Holds } from "corten/views" */

/** @typedef {import("../types/app").Listing} Listing Generated from `Listing` in src/listing.rs. */

/**
 * The outputs the table writes. The markup builds it once, `js::Shown { .. }`.
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
 * Follows the feed until the table leaves the page.
 * @param {Shown} shown @param {Holds<ListingRow, HTMLTableSectionElement>} body
 * @param {MoneyFormat} money @param {string} feed @param {AbortSignal} signal
 */
export function follow(shown, body, money, feed, signal) {
  const list = rows(body, ListingRow);
  /** @param {Listing} listing */
  const draw = (listing) => {
    const row = list.get(listing.id);
    if (!row) return;
    row.price = money(listing.price_cents);
    row.market = money(listing.market_cents);
    row.qty = listing.quantity;
    row.paused = listing.status === "paused";
    row.above = listing.price_cents > listing.market_cents;
  };
  subscribe(
    feed,
    {
      /** @param {Listing[]} listings @param {number} seq */
      snapshot(listings, seq) {
        shown.listings.value = String(listings.length);
        shown.through.value = String(seq);
        add(shown.snapshots, 1);
        listings.forEach(draw);
      },
      patch(changes, seq) {
        shown.through.value = String(seq);
        add(shown.changes, 1);
        for (const change of changes) if (change.op === "upsert") draw(change.value);
      },
      status(status) {
        shown.status.value = status;
      },
    },
    { signal },
  );
}
