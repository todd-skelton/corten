// @ts-check
import { keep } from "corten";

/** @param {HTMLElement} button */
export function bump(button) {
  const kept = keep(() => ({ n: 0 }));
  kept.n += 1;
  button.textContent = String(kept.n);
}
