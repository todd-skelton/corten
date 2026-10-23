// @ts-check
// corten/view (prototype): plain JavaScript as the framework. #[component(view)] generates a class
// (served as `corten/views`) whose properties are the component's holes: `row.label = "x"`
// writes that text node and nothing else, and `row.label` reads it back. A script changes
// the page by assigning to them, exactly as plain JavaScript would write to the DOM, but by name
// and with types. There is no state to compare, no tracking and no scheduler: the DOM holds what
// is shown, and a handler writes what it changed.
//
// rows(parent, Row, template) is an array of those views kept in the page: push, remove, swap,
// replace and clear change the elements, and at(i) / of(event.target) give the view to write. New
// rows are clones of the server-rendered `<template>` (one Row in it), filled with their data.

/**
 * What #[component(view)] generates for a component whose root has `key={..}`.
 * @template D the data it is made from: every hole's value
 * @template {object} V the view
 * @typedef {{
 *   new (el: Element): V,
 *   fill(el: Element, data: D): void,
 *   key(el: Element): string,
 *   keyOf(data: D): string,
 * }} ViewClass
 */

/**
 * The views of every child of `parent`, all made from one component.
 * @template D
 * @template {{ el: Element }} V
 * @param {Element} parent
 * @param {ViewClass<D, V>} View
 * @param {HTMLTemplateElement} [template] holds one server-rendered row, cloned for new rows
 */
export function rows(parent, View, template) {
  /** @type {Element[]} */
  let els = Array.from(parent.children);
  const proto = template?.content.firstElementChild;
  /** @param {D} data */
  const make = (data) => {
    if (!proto) throw new Error("corten: rows() makes new rows from a <template> with one row in it");
    const el = /** @type {Element} */ (proto.cloneNode(true));
    View.fill(el, data);
    return el;
  };
  /** @param {readonly D[]} items @param {Node | null} before */
  const add = (items, before) => {
    const frag = document.createDocumentFragment();
    const made = new Array(items.length);
    for (let i = 0; i < items.length; i += 1) frag.appendChild((made[i] = make(items[i])));
    parent.insertBefore(frag, before);
    return made;
  };
  return {
    get length() {
      return els.length;
    },
    /** The view of the i-th row. @param {number} i @returns {V} */
    at(i) {
      return new View(els[i]);
    },
    /** The view of the row that holds `target` (an event's target), if any. @param {EventTarget | null} target @returns {V | undefined} */
    of(target) {
      let el = /** @type {Node | null} */ (target);
      while (el && el.parentNode !== parent) el = el.parentNode;
      return el ? new View(/** @type {Element} */ (el)) : undefined;
    },
    /** Adds rows at the end. @param {readonly D[]} items */
    push(items) {
      for (const el of add(items, null)) els.push(el);
    },
    /** Takes out a row. @param {V} view */
    remove(view) {
      const i = els.indexOf(view.el);
      if (i < 0) return;
      els.splice(i, 1);
      view.el.remove();
    },
    /** Swaps two rows. @param {number} i @param {number} j */
    swap(i, j) {
      const a = els[i];
      const b = els[j];
      if (!a || !b || a === b) return;
      // Either may come first, and they may be neighbors: a goes where b is, then b where a was.
      const afterA = a.nextSibling;
      if (afterA === b) parent.insertBefore(b, a);
      else {
        parent.insertBefore(a, b);
        parent.insertBefore(b, afterA);
      }
      els[i] = b;
      els[j] = a;
    },
    clear() {
      parent.textContent = "";
      els = [];
    },
    /**
     * Shows exactly `items` (new data from the server, say): rows with a key it had keep their
     * element and are filled again, the rest are made, and the order follows `items`.
     * @param {readonly D[]} items
     */
    replace(items) {
      /** @type {Map<string, Element>} */
      const had = new Map();
      for (const el of els) had.set(View.key(el), el);
      /** @type {Element[]} */
      const next = new Array(items.length);
      let kept = 0;
      for (let i = 0; i < items.length; i += 1) {
        const el = had.get(View.keyOf(items[i]));
        if (el) {
          View.fill(el, items[i]);
          had.delete(View.keyOf(items[i]));
          next[i] = el;
          kept += 1;
        }
      }
      if (kept === 0) {
        parent.textContent = "";
        els = add(items, null);
        return;
      }
      for (const el of had.values()) el.remove();
      // Prototype: the made rows go in, then the order is fixed with one pass of moves (a keyed
      // list with a longest-run step, like state.js's, would move fewer).
      for (let i = 0; i < items.length; i += 1) if (!next[i]) next[i] = make(items[i]);
      let at = parent.firstChild;
      for (const el of next) {
        if (el === at) at = el.nextSibling;
        else parent.insertBefore(el, at);
      }
      els = next;
    },
    *[Symbol.iterator]() {
      for (const el of els) yield new View(el);
    },
  };
}
