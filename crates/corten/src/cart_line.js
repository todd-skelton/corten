// @ts-check
/** @param {HTMLButtonElement} save */
export function enable(save) {
  save.disabled = false;
}

/** @param {MouseEvent} event @param {HTMLDialogElement} dialog @param {string} name */
export function confirm(event, dialog, name) {
  event.preventDefault();
  dialog.dataset.name = name;
  dialog.showModal();
}

/** @param {KeyboardEvent} event @param {HTMLLIElement} line */
export function shortcut(event, line) {
  if (event.key === "Delete") line.remove();
}

/** @param {HTMLElement} line */
export function remove(line) {
  line.remove();
}
