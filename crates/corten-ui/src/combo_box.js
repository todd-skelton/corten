// @ts-check
import { island } from "corten";

// ComboBox (combo_box.rs): the ARIA 1.2 combobox with a listbox popup. The <select> keeps the
// value; this shows a text input instead, filters the options as the user types, and writes
// the pick back to the select.

/** @typedef {{ select: HTMLSelectElement, input: HTMLInputElement, popup: HTMLDivElement, list: HTMLDivElement, empty: HTMLParagraphElement }} Parts */
/** @typedef {{ shown: HTMLElement[], active: number }} State */

export default island("combo-box", {
  /** @returns {State} */
  state: () => ({ shown: [], active: -1 }),
  connect({ refs, signal }) {
    const { select, input, popup, list } = refs;
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
    }
    mark(refs);
    const follow = () => popup.matches(":popover-open") && place(popup, input);
    addEventListener("resize", follow, { signal });
    addEventListener("scroll", follow, { signal, capture: true, passive: true });
  },
  actions: {
    filter({ refs, state }) {
      refs.input.setCustomValidity("");
      open(refs, state, true);
    },
    toggle({ refs, state }) {
      if (refs.popup.matches(":popover-open")) close(refs, state);
      else open(refs, state, false);
    },
    key({ refs, state, event }) {
      const isOpen = refs.popup.matches(":popover-open");
      switch (event.key) {
        case "ArrowDown":
        case "ArrowUp": {
          const by = event.key === "ArrowDown" ? 1 : -1;
          if (!isOpen) {
            open(refs, state, false);
            if (state.active < 0) highlight(refs, state, by > 0 ? 0 : state.shown.length - 1);
          } else if (state.shown.length) {
            const next = state.active < 0 ? (by > 0 ? 0 : -1) : state.active + by;
            highlight(refs, state, (next + state.shown.length) % state.shown.length);
          }
          break;
        }
        case "Enter":
          if (!isOpen) return; // Enter in a closed field submits the form
          if (state.active >= 0) choose(refs, state, state.shown[state.active]);
          else close(refs, state);
          break;
        case "Escape":
          if (!isOpen) return;
          refs.input.value = label(refs);
          close(refs, state);
          break;
        default:
          return;
      }
      event.preventDefault();
    },
    leave({ refs, state }) {
      const text = refs.input.value.trim();
      const typed = text.toLocaleLowerCase();
      const exact = options(refs).find((el) => el.textContent?.toLocaleLowerCase() === typed && !disabled(el));
      if (exact) choose(refs, state, exact);
      else if (text === "" && refs.select.value !== "") set(refs, "");
      else refs.input.value = label(refs);
      close(refs, state);
    },
    hold({ event }) {
      // A press in the popup keeps focus in the input, so leaving does not run first.
      event.preventDefault();
    },
    pick({ refs, state, event }) {
      const el = event.target instanceof Element ? event.target.closest('[role="option"]') : null;
      if (el instanceof HTMLElement) choose(refs, state, el);
    },
    invalid({ refs, event }) {
      // The select is hidden, so the browser cannot point at it: report on the input instead.
      event.preventDefault();
      refs.input.setCustomValidity(refs.select.validationMessage);
      refs.input.reportValidity();
    },
  },
});

/** @param {Parts} parts */
function options(parts) {
  return /** @type {HTMLElement[]} */ (Array.from(parts.list.children));
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
 * Shows the popup with every option (`typed` false) or those that contain the input's text.
 * @param {Parts} parts
 * @param {State} state
 * @param {boolean} typed
 */
function open(parts, state, typed) {
  const { input, popup, empty } = parts;
  const query = typed ? input.value.trim().toLocaleLowerCase() : "";
  state.shown = [];
  for (const el of options(parts)) {
    el.hidden = query !== "" && !el.textContent?.toLocaleLowerCase().includes(query);
    if (!el.hidden) state.shown.push(el);
  }
  empty.hidden = state.shown.length > 0;
  if (!popup.matches(":popover-open")) popup.showPopover();
  input.setAttribute("aria-expanded", "true");
  place(popup, input);
  const selected = state.shown.findIndex((el) => el.getAttribute("aria-selected") === "true");
  highlight(parts, state, typed && query !== "" ? (state.shown.length ? 0 : -1) : selected);
}

/**
 * @param {Parts} parts
 * @param {State} state
 */
function close(parts, state) {
  if (parts.popup.matches(":popover-open")) parts.popup.hidePopover();
  parts.input.setAttribute("aria-expanded", "false");
  highlight(parts, state, -1);
}

/**
 * Marks the option Enter would pick (-1: none) and tells assistive technology.
 * @param {Parts} parts
 * @param {State} state
 * @param {number} index
 */
function highlight(parts, state, index) {
  state.shown[state.active]?.removeAttribute("data-highlighted");
  state.active = index;
  const el = state.shown[index];
  if (el) {
    el.setAttribute("data-highlighted", "");
    el.scrollIntoView({ block: "nearest" });
    parts.input.setAttribute("aria-activedescendant", el.id);
  } else {
    parts.input.removeAttribute("aria-activedescendant");
  }
}

/**
 * @param {Parts} parts
 * @param {State} state
 * @param {HTMLElement} el
 */
function choose(parts, state, el) {
  if (disabled(el)) return;
  parts.input.value = el.textContent ?? "";
  parts.input.setCustomValidity("");
  set(parts, el.dataset.value ?? "");
  close(parts, state);
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
 * Places a popover (fixed, in the top layer) under its anchor, or above it when only there is
 * room, and sets --anchor-width for the design system's CSS. A copy is in menu_button.js.
 * @param {HTMLElement} popup
 * @param {HTMLElement} anchor
 */
function place(popup, anchor) {
  const a = anchor.getBoundingClientRect();
  popup.style.setProperty("--anchor-width", `${a.width}px`);
  popup.style.inset = "auto";
  const height = popup.offsetHeight;
  const below = innerHeight - a.bottom;
  const top = below >= height || below >= a.top ? a.bottom : a.top - height;
  popup.style.top = `${Math.max(0, top)}px`;
  popup.style.left = `${Math.max(0, Math.min(a.left, innerWidth - popup.offsetWidth))}px`;
}
