// @ts-check
import { island } from "corten";

// Flips the listings between cheapest and dearest first. The rule is sort_listings in src/model.rs.
export default island("listing-sort", {
  actions: {
    toggle({ refs }) {
      const descending = refs.descending.hidden;
      const rows = Array.from(refs.rows.rows).sort((a, b) => Number(a.dataset.price) - Number(b.dataset.price));
      if (descending) rows.reverse();
      refs.rows.replaceChildren(...rows);
      refs.ascending.hidden = descending;
      refs.descending.hidden = !descending;
    },
  },
});
