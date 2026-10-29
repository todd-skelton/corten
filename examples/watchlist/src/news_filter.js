// @ts-check
// NewsFilter (news.rs): applies the filter as soon as it changes.

/** @param {HTMLFormElement} form */
export function apply(form) {
  form.requestSubmit();
}
