// @ts-check
// NumberField (number_field.rs). The input is a native number input, whose stepUp and stepDown
// already keep min, max and step; this adds the buttons, press and hold, Page Up and Page Down,
// Home and End, and the step's decimals. A step sends `input` and `change` like typing does, and
// the input's own on:input (`sync`) then disables a button at its bound.

import { keep } from "corten";

/** Milliseconds a button is held before it repeats, and between repeats. */
const HOLD = 400;
const REPEAT = 60;

/** The field's press and hold, and the button a pointer is pressing. */
const held = () => keep(() => ({ timer: 0, pressed: /** @type {Element | null} */ (null) }));

/**
 * When the field joins the page: the buttons match the value, and a hold ends wherever the
 * pointer is released.
 * @param {HTMLButtonElement} decrease @param {HTMLInputElement} input @param {HTMLButtonElement} increase
 * @param {AbortSignal} signal
 */
export function connect(decrease, input, increase, signal) {
  sync(decrease, input, increase);
  const s = held();
  /** @param {Event} event */
  const release = (event) => {
    clearTimeout(s.timer);
    // Released on the button it pressed, a click follows, the press's own (`click` takes it);
    // released anywhere else, cancelled or the window left, none does.
    if (event.type !== "pointerup" || !s.pressed?.contains(/** @type {Node} */ (event.target))) s.pressed = null;
  };
  for (const type of ["pointerup", "pointercancel", "blur"]) addEventListener(type, release, { signal });
  signal.addEventListener("abort", () => clearTimeout(s.timer));
}

/**
 * Steps by `by` while the button is held.
 * @param {PointerEvent} event @param {HTMLButtonElement} button @param {HTMLInputElement} input @param {number} by
 */
export function press(event, button, input, by) {
  if (event.button !== 0 || button.disabled) return;
  // Keeps focus where it was (usually the input) and selects no text.
  event.preventDefault();
  const s = held();
  s.pressed = button;
  /** @param {number} delay */
  const tick = (delay) => {
    if (step(input, by)) s.timer = setTimeout(() => tick(REPEAT), delay);
  };
  clearTimeout(s.timer);
  tick(HOLD);
}

/**
 * Enter or Space on a button, or an assistive technology's click; the click that ends a pointer's
 * press has stepped already, on the press. A key's click has `detail` 0, so it always steps.
 * @param {MouseEvent} event @param {HTMLButtonElement} button @param {HTMLInputElement} input @param {number} by
 */
export function click(event, button, input, by) {
  const s = held();
  if (s.pressed === button && event.detail > 0) s.pressed = null;
  else step(input, by);
}

/** @param {KeyboardEvent} event @param {HTMLInputElement} input */
export function key(event, input) {
  if (event.key === "PageUp" || event.key === "PageDown") step(input, event.key === "PageUp" ? 10 : -10);
  else if (event.key === "Home" && input.min !== "") set(input, input.min);
  else if (event.key === "End" && input.max !== "") set(input, input.max);
  else return;
  event.preventDefault();
}

/**
 * Disables each button at its bound.
 * @param {HTMLButtonElement} decrease @param {HTMLInputElement} input @param {HTMLButtonElement} increase
 */
export function sync(decrease, input, increase) {
  const value = input.valueAsNumber;
  const known = !Number.isNaN(value);
  decrease.disabled = input.disabled || (known && input.min !== "" && value <= Number(input.min));
  increase.disabled = input.disabled || (known && input.max !== "" && value >= Number(input.max));
}

/**
 * Adds `by` steps; false when the value did not change (at a bound, or disabled).
 * @param {HTMLInputElement} input @param {number} by
 */
function step(input, by) {
  if (input.disabled || input.readOnly) return false;
  const before = input.value;
  try {
    if (by > 0) input.stepUp(by);
    else input.stepDown(-by);
  } catch {
    return false; // step="any" has no step
  }
  return commit(input, before);
}

/** @param {HTMLInputElement} input @param {string} value */
function set(input, value) {
  if (input.disabled || input.readOnly) return;
  const before = input.value;
  input.value = value;
  commit(input, before);
}

/**
 * Shows the step's decimals and, when the value changed, tells the page as typing would.
 * @param {HTMLInputElement} input @param {string} before
 */
function commit(input, before) {
  if (input.value !== "" && input.step !== "any") {
    input.value = Number(input.value).toFixed((input.step.split(".")[1] ?? "").length);
  }
  if (input.value === before) return false;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
  return true;
}
