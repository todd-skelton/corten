// @ts-check
// AboutFilter (app.rs): applies the filter as soon as it changes.

/** @param {HTMLFormElement} form */
export function apply(form) {
  form.requestSubmit();
}