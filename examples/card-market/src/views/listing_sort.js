// @ts-check
// ListingSort (listing_sort.rs).

/**
 * Flips the listings between cheapest and dearest first. The rule is sort_listings in src/model.rs.
 * @param {HTMLTableSectionElement} rows @param {HTMLSpanElement} ascending @param {HTMLSpanElement} descending
 */
export function toggle(rows, ascending, descending) {
  const down = descending.hidden;
  const sorted = Array.from(rows.rows).sort((a, b) => Number(a.dataset.price) - Number(b.dataset.price));
  if (down) sorted.reverse();
  rows.replaceChildren(...sorted);
  ascending.hidden = down;
  descending.hidden = !down;
}
