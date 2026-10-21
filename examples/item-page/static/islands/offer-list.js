// @ts-check
import { Island } from "corten";

/** @typedef {import("../../types/app").OfferListRef} Ref Generated from `OfferListRef` in src/views.rs. */

/**
 * "Show all offers": moves the server-rendered rows out of the `Rest` template.
 * @extends {Island<unknown, Ref>}
 */
export default class OfferList extends Island {
  setup() {
    this.on("click", "more", (button) => {
      /** @type {HTMLTemplateElement} */
      const rest = this.ref("rest");
      this.ref("rows").append(rest.content);
      rest.remove();
      button.remove();
    });
  }
}
