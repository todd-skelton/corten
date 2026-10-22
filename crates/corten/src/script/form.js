// @ts-check
/** @param {HTMLButtonElement} save @param {HTMLElement} root */
export function enable(save, root) {
  save.disabled = !root.querySelector("input")?.value;
}
