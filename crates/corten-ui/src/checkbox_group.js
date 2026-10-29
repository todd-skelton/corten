// @ts-check
// CheckboxGroup (checkbox_group.rs): at least one box of the group checked, or the browser does not
// send the form and says why at the first box it would send.

/**
 * Checks the group as drawn (a page with boxes checked, a refused form drawn again), and again
 * after a form reset, once the boxes are back to how they were drawn.
 * @param {HTMLFieldSetElement} group @param {string} missing @param {AbortSignal} signal
 */
export function connect(group, missing, signal) {
  check(group, missing);
  group.form?.addEventListener("reset", () => setTimeout(() => check(group, missing)), { signal });
}

/**
 * Keeps the group's first enabled box invalid with `missing` while no enabled box is checked (a
 * disabled box is not sent, so it is no choice), and every box valid once one is. Runs on each
 * change in the group.
 * @param {HTMLFieldSetElement} group @param {string} missing
 */
export function check(group, missing) {
  const boxes = Array.from(group.querySelectorAll('input[type="checkbox"]'), (box) => /** @type {HTMLInputElement} */ (box));
  const none = !boxes.some((box) => box.checked && !box.disabled);
  const first = boxes.find((box) => !box.disabled);
  for (const box of boxes) box.setCustomValidity(none && box === first ? missing : "");
}
