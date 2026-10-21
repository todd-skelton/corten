// @ts-check
import { Island } from "corten";

/** "Show all offers": moves the server-rendered rows out of <template data-ref="rest">. */
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
