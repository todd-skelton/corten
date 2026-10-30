// @ts-check
// ComboBox (combo_box.rs): the ARIA 1.2 combobox with a listbox popup. The <select> keeps the
// value; this shows a text input instead, filters and ranks the options as the user types, and
// writes the pick back to the select. `connect` keeps the parts and the highlight by the component's
// root, so each handler takes only the root (`scope`).

/**
 * `all` is every option in the select's order and `words` each one's label and terms, lowercased.
 * @typedef {{ select: HTMLSelectElement, input: HTMLInputElement, popup: HTMLDivElement, list: HTMLDivElement, empty: HTMLParagraphElement, all: HTMLElement[], words: string[][] }} Parts
 */
/** @typedef {Parts & { shown: HTMLElement[], active: number }} Box */

/** @type {WeakMap<HTMLElement, Box>} */
const boxes = new WeakMap();

/** The combobox whose root is `root`, once connected. @param {HTMLElement} root */
const box = (root) => boxes.get(root);

/**
 * @param {HTMLElement} root @param {HTMLSelectElement} select @param {HTMLInputElement} input
 * @param {HTMLDivElement} popup @param {HTMLDivElement} list @param {HTMLParagraphElement} empty
 */
export function connect(root, select, input, popup, list, empty) {
  if (boxes.has(root)) return; // back after a move: already built
  /** @type {Box} */
  const b = { select, input, popup, list, empty, all: [], words: [], shown: [], active: -1 };
  boxes.set(root, b);
  // The app's <label for> follows the control that shows.
  const id = select.id;
  select.id = `${id}-value`;
  input.id = id;
  select.hidden = true;
  input.hidden = false;
  for (const option of Array.from(select.options)) {
    if (option.value === "") continue;
    const el = document.createElement("div");
    el.id = `${list.id}-${option.index}`;
    el.setAttribute("role", "option");
    el.dataset.value = option.value;
    el.textContent = option.text;
    if (option.disabled) el.setAttribute("aria-disabled", "true");
    list.append(el);
    b.all.push(el);
    const terms = option.dataset.terms ? option.dataset.terms.split("\n").filter((t) => t !== "") : [];
    b.words.push([option.text, ...terms].map((w) => w.toLocaleLowerCase()));
  }
  mark(b);
  anchor(popup, input);
}

/** @param {HTMLElement} root */
export function filter(root) {
  const b = box(root);
  if (!b) return;
  b.input.setCustomValidity("");
  open(b, true);
}

/** @param {HTMLElement} root */
export function toggle(root) {
  const b = box(root);
  if (!b) return;
  if (b.popup.matches(":popover-open")) close(b);
  else open(b, false);
}

/** @param {KeyboardEvent} event @param {HTMLElement} root */
export function key(event, root) {
  const b = box(root);
  if (!b) return;
  const isOpen = b.popup.matches(":popover-open");
  switch (event.key) {
    case "ArrowDown":
    case "ArrowUp": {
      const by = event.key === "ArrowDown" ? 1 : -1;
      if (!isOpen) {
        open(b, false);
        if (b.active < 0) highlight(b, by > 0 ? 0 : b.shown.length - 1);
      } else if (b.shown.length) {
        const next = b.active < 0 ? (by > 0 ? 0 : -1) : b.active + by;
        highlight(b, (next + b.shown.length) % b.shown.length);
      }
      break;
    }
    case "Enter":
      if (!isOpen) return; // Enter in a closed field submits the form
      if (b.active >= 0) choose(b, b.shown[b.active]);
      else close(b);
      break;
    case "Escape":
      if (!isOpen) return;
      b.input.value = label(b);
      close(b);
      break;
    default:
      return;
  }
  event.preventDefault();
}

/** @param {HTMLElement} root */
export function leave(root) {
  const b = box(root);
  if (!b) return;
  const text = b.input.value.trim();
  const typed = text.toLocaleLowerCase();
  const exact = options(b).find((el) => el.textContent?.toLocaleLowerCase() === typed && !disabled(el));
  if (exact) choose(b, exact);
  else if (text === "" && b.select.value !== "") set(b, "");
  else b.input.value = label(b);
  close(b);
}

/** A press in the popup keeps focus in the input, so leaving does not run first. @param {MouseEvent} event */
export function hold(event) {
  event.preventDefault();
}

/** @param {MouseEvent} event @param {HTMLElement} root */
export function pick(event, root) {
  const b = box(root);
  const el = event.target instanceof Element ? event.target.closest('[role="option"]') : null;
  if (b && el instanceof HTMLElement) choose(b, el);
}

/** The select is hidden, so the browser cannot point at it: report on the input instead. @param {Event} event @param {HTMLElement} root */
export function invalid(event, root) {
  const b = box(root);
  if (!b) return;
  event.preventDefault();
  b.input.setCustomValidity(b.select.validationMessage);
  b.input.reportValidity();
}

/** Every option, in the select's order. @param {Parts} parts */
function options(parts) {
  return parts.all;
}

/** @param {Element} el */
function disabled(el) {
  return el.getAttribute("aria-disabled") === "true";
}

/** The selected option's text, or nothing. @param {Parts} parts */
function label(parts) {
  const option = parts.select.selectedOptions[0];
  return option && option.value !== "" ? option.text : "";
}

/**
 * How well an option's words match the typed text: 0 one is the text, 1 one starts with it,
 * 2 one holds it, -1 none does.
 * @param {string[]} words
 * @param {string} query
 */
function rank(words, query) {
  let best = -1;
  for (const w of words) {
    const r = w === query ? 0 : w.startsWith(query) ? 1 : w.includes(query) ? 2 : -1;
    if (r >= 0 && (best < 0 || r < best)) best = r;
  }
  return best;
}

/**
 * Shows the popup with every option (`typed` false) or those whose label or terms hold the
 * input's text, best matches first and each kind in the select's order.
 * @param {Box} parts
 * @param {boolean} typed
 */
function open(parts, typed) {
  const { input, popup, empty, list } = parts;
  const query = typed ? input.value.trim().toLocaleLowerCase() : "";
  /** @type {HTMLElement[][]} */
  const ranked = [[], [], []];
  parts.all.forEach((el, i) => {
    const r = query === "" ? 0 : rank(parts.words[i], query);
    el.hidden = r < 0;
    if (r >= 0) ranked[r].push(el);
  });
  // The highlight belongs to the old list: clear it before the list it indexes is replaced.
  highlight(parts, -1);
  parts.shown = ranked.flat();
  // The list reads in the order the arrow keys move, so a reorder moves the elements, and only
  // when the order changed.
  let at = 0;
  for (const el of /** @type {HTMLCollectionOf<HTMLElement>} */ (list.children)) {
    if (!el.hidden && el !== parts.shown[at++]) {
      list.append(...parts.shown);
      break;
    }
  }
  empty.hidden = parts.shown.length > 0;
  if (!popup.matches(":popover-open")) popup.showPopover();
  input.setAttribute("aria-expanded", "true");
  const selected = parts.shown.findIndex((el) => el.getAttribute("aria-selected") === "true");
  highlight(parts, typed && query !== "" ? (parts.shown.length ? 0 : -1) : selected);
}

/**
 * @param {Box} parts
 */
function close(parts) {
  if (parts.popup.matches(":popover-open")) parts.popup.hidePopover();
  parts.input.setAttribute("aria-expanded", "false");
  highlight(parts, -1);
}

/**
 * Marks the option Enter would pick (-1: none) and tells assistive technology.
 * @param {Box} parts
 * @param {number} index
 */
function highlight(parts, index) {
  parts.shown[parts.active]?.removeAttribute("data-highlighted");
  parts.active = index;
  const el = parts.shown[index];
  if (el) {
    el.setAttribute("data-highlighted", "");
    el.scrollIntoView({ block: "nearest" });
    parts.input.setAttribute("aria-activedescendant", el.id);
  } else {
    parts.input.removeAttribute("aria-activedescendant");
  }
}

/**
 * @param {Box} parts
 * @param {HTMLElement} el
 */
function choose(parts, el) {
  if (disabled(el)) return;
  parts.input.value = el.textContent ?? "";
  parts.input.setCustomValidity("");
  set(parts, el.dataset.value ?? "");
  close(parts);
}

/**
 * Writes the value to the select and tells the page, as picking in the select would.
 * @param {Parts} parts
 * @param {string} value
 */
function set(parts, value) {
  if (parts.select.value === value) return;
  parts.select.value = value;
  mark(parts);
  parts.select.dispatchEvent(new Event("input", { bubbles: true }));
  parts.select.dispatchEvent(new Event("change", { bubbles: true }));
}

/** @param {Parts} parts */
function mark(parts) {
  for (const el of options(parts)) el.setAttribute("aria-selected", String(el.dataset.value === parts.select.value));
}

/**
 * Makes `el` the popup's anchor, so the design system places the popup with CSS anchor
 * positioning (`position-area`, `anchor-size()`): the browser keeps it in place on scroll and
 * resize, with no script.
 * @param {HTMLElement} popup
 * @param {HTMLElement} el
 */
function anchor(popup, el) {
  const name = `--${CSS.escape(el.id)}`;
  el.style.setProperty("anchor-name", name);
  popup.style.setProperty("position-anchor", name);
}
