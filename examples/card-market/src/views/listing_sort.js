// @ts-check
// ListingSort (listing_sort.rs).

/**
 * Flips the listings between cheapest and dearest first, by the cents each row carries (an integer,
 * the largest for a listing without a price). The sort is stable either way, so rows with the same
 * price keep the server's order and a second click restores it. The rule is sort_listings in
 * src/model.rs.
 * @param {HTMLTableSectionElement} rows @param {HTMLSpanElement} ascending @param {HTMLSpanElement} descending
 */
export function toggle(rows, ascending, descending) {
  const down = descending.hidden;
  const sign = down ? -1 : 1;
  const sorted = Array.from(rows.rows).sort((a, b) => sign * (Number(a.dataset.cents) - Number(b.dataset.cents)));
  rows.replaceChildren(...sorted);
  ascending.hidden = down;
  descending.hidden = !down;
}
