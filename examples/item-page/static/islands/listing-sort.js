// @ts-check
import { Island } from "corten";

/** @typedef {import("../../types/app").ListingSortRef} Ref Generated from `ListingSortRef` in src/views.rs. */

/**
 * Sorts the listing rows by data-price. Same rule as sort_listings in src/model.rs.
 * @extends {Island<unknown, Ref>}
 */
export default class ListingSort extends Island {
  setup() {
    this.on("click", "toggle", (button) => {
      const descending = this.dataset.order !== "desc";
      const rows = this.ref("rows");
      const sorted = Array.from(rows.children, (row) => /** @type {HTMLElement} */ (row));
      sorted.sort((a, b) => Number(a.dataset.price) - Number(b.dataset.price));
      if (descending) sorted.reverse();
      rows.replaceChildren(...sorted);
      this.dataset.order = descending ? "desc" : "asc";
      button.textContent = (descending ? button.dataset.desc : button.dataset.asc) ?? "";
    });
  }
}
