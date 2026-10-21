// @ts-check
import { Island } from "corten";

/**
 * <click-counter>: the file name is the tag name, and Corten loads this module only on pages that
 * use the tag. The server renders the button; this adds the behavior.
 */
export default class ClickCounter extends Island {
  setup() {
    let count = 0;
    this.on("click", "button", (button) => {
      count += 1;
      button.textContent = `Clicked ${count} ${count === 1 ? "time" : "times"}`;
    });
  }
}
