// @ts-check
// PriceCell (price_cell.rs): Save shows once the price differs from the saved one (the input's
// value attribute); Escape puts it back. A cell the server sent back with an error keeps Save.

/** @param {HTMLInputElement} input @param {HTMLButtonElement} save */
export function edited(input, save) {
  save.hidden = input.value === input.defaultValue && !input.hasAttribute("aria-invalid");
}

/** @param {KeyboardEvent} event @param {HTMLInputElement} input @param {HTMLButtonElement} save */
export function revert(event, input, save) {
  if (event.key !== "Escape" || input.hasAttribute("aria-invalid")) return;
  input.value = input.defaultValue;
  save.hidden = true;
}
