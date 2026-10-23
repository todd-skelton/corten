// @ts-check
// The same rows in every app: the js-framework-benchmark word lists and a 32-bit LCG.
const ADJECTIVES = ["pretty", "large", "big", "small", "tall", "short", "long", "handsome", "plain", "quaint", "clean", "elegant", "easy", "angry", "crazy", "helpful", "mushy", "odd", "unsightly", "adorable", "important", "inexpensive", "cheap", "expensive", "fancy"];
const COLOURS = ["red", "yellow", "blue", "green", "pink", "brown", "purple", "brown", "white", "black", "orange"];
const NOUNS = ["table", "chair", "house", "bbq", "desk", "car", "pony", "cookie", "sandwich", "burger", "pizza", "mouse", "keyboard"];

let seed = 7;
/** @param {number} max */
function next(max) {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  return (seed >>> 8) % max;
}

export function label() {
  return `${ADJECTIVES[next(25)]} ${COLOURS[next(11)]} ${NOUNS[next(13)]}`;
}

let id = 1;
export const nextId = () => id++;

/** The rows a live-price patch touches: 100 different positions, the same in every app. @param {number} length */
export function patched(length) {
  /** @type {number[]} */
  const at = [];
  for (let i = 0; i < 100 && i < length; i += 1) at.push((i * 97) % length);
  return at;
}
