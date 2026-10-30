// @ts-check
// ChoiceGroup and ShownFor (choice_group.rs): a radio group whose choice shows and hides the
// parts of its group wherever they are on the page. The server draws the choice and the parts;
// this adds the roving focus and switches both in place.

/**
 * The parts on the page for each group (by its id), and the words each is shown for.
 * @type {Map<string, Set<{ part: HTMLElement, values: string[] }>>}
 */
const parts = new Map();

/**
 * The word each group on the page chose here, after the server drew it: what a part a patch adds
 * later shows for.
 * @type {Map<string, string>}
 */
const chosen = new Map();

/**
 * Without this script every option is reachable with Tab (links work on their own); with it,
 * only the chosen one is, and the arrow keys move between them. The group's choice is forgotten
 * when it leaves the page, so the next page's server choice holds.
 * @param {HTMLElement[]} radios @param {string} group @param {AbortSignal} signal
 */
export function connect(radios, group, signal) {
  for (const radio of radios) radio.tabIndex = radio.getAttribute("aria-checked") === "true" ? 0 : -1;
  signal.addEventListener("abort", () => chosen.delete(group));
}

/** @param {MouseEvent} event @param {HTMLElement} radio @param {HTMLElement[]} radios @param {string} group @param {string[]} values */
export function choose(event, radio, radios, group, values) {
  // A modified click on a link (new tab, new window) is the browser's.
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  event.preventDefault();
  select(radios.indexOf(radio), radios, group, values);
}

/** @param {KeyboardEvent} event @param {HTMLElement} radio @param {HTMLElement[]} radios @param {string} group @param {string[]} values */
export function step(event, radio, radios, group, values) {
  const at = radios.indexOf(radio);
  const to =
    event.key === "ArrowRight" || event.key === "ArrowDown" ? (at + 1) % radios.length
    : event.key === "ArrowLeft" || event.key === "ArrowUp" ? (at - 1 + radios.length) % radios.length
    : event.key === "Home" ? 0
    : event.key === "End" ? radios.length - 1
    : event.key === " " ? at
    : -1;
  if (to < 0) return;
  event.preventDefault();
  radios[to].focus();
  select(to, radios, group, values);
}

/**
 * Chooses the option at `at`: it is checked, a link option's URL becomes the address, and every
 * part of the group shows or hides for its word.
 * @param {number} at @param {HTMLElement[]} radios @param {string} group @param {string[]} values
 */
function select(at, radios, group, values) {
  radios.forEach((radio, i) => {
    radio.setAttribute("aria-checked", String(i === at));
    radio.tabIndex = i === at ? 0 : -1;
  });
  const radio = radios[at];
  if (radio instanceof HTMLAnchorElement && radio.href !== location.href) history.replaceState(history.state, "", radio.href);
  chosen.set(group, values[at]);
  for (const { part, values: shown } of parts.get(group) ?? []) part.hidden = !shown.includes(values[at]);
}

/**
 * A part of `group` shown for `values`: switched with the group's choice until it leaves the
 * page. One added after a choice was made here (a patch) shows for that choice.
 * @param {HTMLDivElement} part @param {string} group @param {string[]} values @param {AbortSignal} signal
 */
export function part(part, group, values, signal) {
  const entry = { part, values };
  let set = parts.get(group);
  if (!set) parts.set(group, (set = new Set()));
  set.add(entry);
  const now = chosen.get(group);
  if (now !== undefined) part.hidden = !values.includes(now);
  signal.addEventListener("abort", () => {
    const set = parts.get(group);
    set?.delete(entry);
    if (set?.size === 0) parts.delete(group);
  });
}
