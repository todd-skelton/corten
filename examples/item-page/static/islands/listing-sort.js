// @ts-check
import { Island } from "corten";

/** Sorts the listing rows by data-price. Same rule as sort_listings in src/model.rs. */
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
