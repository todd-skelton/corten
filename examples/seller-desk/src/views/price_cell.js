// @ts-check
// PriceCell (price_cell.rs): Save shows once the price differs from the saved one (the input's
// value attribute); Escape puts it back. A cell the server sent back with an error keeps Save.
// Each change tells the grid whether the price is not saved yet (PriceEdited, which bubbles).
import { PriceEdited } from "corten/events";

/** @param {HTMLInputElement} input @param {HTMLButtonElement} save */
export function edited(input, save) {
  save.hidden = input.value === input.defaultValue && !input.hasAttribute("aria-invalid");
  input.dispatchEvent(new PriceEdited({ unsent: input.value !== input.defaultValue }));
}

/** @param {KeyboardEvent} event @param {HTMLInputElement} input @param {HTMLButtonElement} save */
export function revert(event, input, save) {
  if (event.key !== "Escape" || input.hasAttribute("aria-invalid")) return;
  input.value = input.defaultValue;
  save.hidden = true;
  input.dispatchEvent(new PriceEdited({ unsent: false }));
}
