// @ts-check
// Sku (gallery.rs): counts the copies its CopyButton reports, as an app counts a shared
// link's. Only a write the browser made is reported.

/** @typedef {import("corten/events").Copied} Copied */

/**
 * One more copy.
 * @param {Copied} event @param {HTMLOutputElement} copies
 */
export function counted(event, copies) {
  copies.textContent = String(Number(copies.textContent) + 1);
}
