// @ts-check
// SaveNote (save_note.rs): the answer to a save, as it joins the page, told to the builder around
// it. The page as first loaded has no answer, and tells nothing.
import { DeckAnswered } from "corten/events";

/** @param {HTMLSpanElement} note @param {string} outcome @param {number} version */
export function answered(note, outcome, version) {
  if (outcome) note.dispatchEvent(new DeckAnswered({ outcome, version }));
}