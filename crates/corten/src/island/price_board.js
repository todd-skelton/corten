// Test fixture for the #[island] in tests.rs; never served.
import { island } from "corten";

export default island("price-board", {
  actions: {
    pick({ target }) {
      target.toggleAttribute("aria-selected");
    },
    narrow({ all, target }) {
      for (const price of all.price) price.hidden = !price.textContent?.includes(target.value);
    },
  },
});
