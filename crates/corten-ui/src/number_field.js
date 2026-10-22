// @ts-check
import { island } from "corten";

// NumberField (number_field.rs). The input is a native number input, whose stepUp and stepDown
// already keep min, max and step; this adds the buttons, press and hold, Page Up and Page Down,
// Home and End, and the step's decimals.

/** Milliseconds a button is held before it repeats, and between repeats. */
const HOLD = 400;
const REPEAT = 60;

export default island("number-field", {
  state: () => ({ timer: 0, pointer: false }),
  connect({ refs, signal, state }) {
    sync(refs);
    const release = () => {
      clearTimeout(state.timer);
      // The click that follows this pointerup runs first, and must not step again.
      setTimeout(() => (state.pointer = false));
    };
    for (const type of ["pointerup", "pointercancel", "blur"]) addEventListener(type, release, { signal });
    signal.addEventListener("abort", () => clearTimeout(state.timer));
  },
  actions: {
    press({ refs, target, event, state }) {
      if (event.button !== 0 || target.disabled) return;
      // Keeps focus where it was (usually the input) and selects no text.
      event.preventDefault();
      state.pointer = true;
      const by = target === refs.increase ? 1 : -1;
      /** @param {number} delay */
      const tick = (delay) => {
        if (step(refs, by)) state.timer = setTimeout(() => tick(REPEAT), delay);
      };
      clearTimeout(state.timer);
      tick(HOLD);
    },
    click({ refs, target, state }) {
      // A pointer press has stepped already; Enter or Space on the button has not.
      if (!state.pointer) step(refs, target === refs.increase ? 1 : -1);
    },
    key({ refs, event }) {
      const { input } = refs;
      if (event.key === "PageUp" || event.key === "PageDown") step(refs, event.key === "PageUp" ? 10 : -10);
      else if (event.key === "Home" && input.min !== "") set(refs, input.min);
      else if (event.key === "End" && input.max !== "") set(refs, input.max);
      else return;
      event.preventDefault();
    },
    sync({ refs }) {
      sync(refs);
    },
  },
});

/**
 * @typedef {{ input: HTMLInputElement, decrease: HTMLButtonElement, increase: HTMLButtonElement }} Parts
 */

/**
 * Adds `by` steps; false when the value did not change (at a bound, or disabled).
 * @param {Parts} parts
 * @param {number} by
 */
function step(parts, by) {
  const { input } = parts;
  if (input.disabled || input.readOnly) return false;
  const before = input.value;
  try {
    if (by > 0) input.stepUp(by);
    else input.stepDown(-by);
  } catch {
    return false; // step="any" has no step
  }
  return commit(parts, before);
}

/**
 * @param {Parts} parts
 * @param {string} value
 */
function set(parts, value) {
  if (parts.input.disabled || parts.input.readOnly) return;
  const before = parts.input.value;
  parts.input.value = value;
  commit(parts, before);
}

/**
 * Shows the step's decimals and, when the value changed, tells the page as typing would.
 * @param {Parts} parts
 * @param {string} before
 */
function commit(parts, before) {
  const { input } = parts;
  if (input.value !== "" && input.step !== "any") {
    input.value = Number(input.value).toFixed((input.step.split(".")[1] ?? "").length);
  }
  if (input.value === before) return false;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
  return true;
}

/**
 * Disables each button at its bound.
 * @param {Parts} parts
 */
function sync({ input, decrease, increase }) {
  const value = input.valueAsNumber;
  const known = !Number.isNaN(value);
  decrease.disabled = input.disabled || (known && input.min !== "" && value <= Number(input.min));
  increase.disabled = input.disabled || (known && input.max !== "" && value >= Number(input.max));
}
