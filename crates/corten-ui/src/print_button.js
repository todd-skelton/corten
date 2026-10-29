// @ts-check
// PrintButton (print_button.rs). The button is drawn hidden: without a script it cannot print,
// and the browser's own Print command still prints the page. This shows it, and a click opens
// the print dialog.

/**
 * Shown once its script runs.
 * @param {HTMLButtonElement} button
 */
export function connect(button) {
  button.hidden = false;
}

/** Opens the browser's print dialog for the page. */
export function print() {
  window.print();
}
