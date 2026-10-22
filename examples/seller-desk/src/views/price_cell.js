// @ts-check
// PriceCell (price_cell.rs): Save shows once the price differs from the saved one; Escape puts
// the saved price back.

/** @param {HTMLInputElement} input @param {HTMLButtonElement} save @param {string} saved */
export function edited(input, save, saved) {
  save.hidden = input.value === saved;
}

/** @param {KeyboardEvent} event @param {HTMLInputElement} input @param {HTMLButtonElement} save @param {string} saved */
export function revert(event, input, save, saved) {
  if (event.key !== "Escape" || !saved) return;
  input.value = saved;
  input.removeAttribute("aria-invalid");
  save.hidden = true;
}
