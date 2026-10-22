// @ts-check
/** @param {HTMLFormElement} form @param {HTMLElement} root */
export function submit(form, root) {
  root.toggleAttribute("aria-busy", true);
  form.requestSubmit();
}
