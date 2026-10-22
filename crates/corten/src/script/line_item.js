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
