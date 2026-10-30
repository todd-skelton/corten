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

/** How many groups of each id are on the page: a patch connects the new one before the old ends. */
const groups = new Map();

/**
 * Without this script every option is reachable with Tab (links work on their own); with it, only
 * the chosen one is (the first when none is), and the arrow keys move between them. A group that
 * joins the page holds the choice the server drew, and its choice is forgotten when the last group
 * of its id leaves, so the next page's server choice holds.
 * @param {HTMLElement[]} radios @param {string} group @param {string[]} values @param {AbortSignal} signal
 */
export function connect(radios, group, values, signal) {
  const at = radios.findIndex((radio) => radio.getAttribute("aria-checked") === "true");
  radios.forEach((radio, i) => (radio.tabIndex = i === Math.max(at, 0) ? 0 : -1));
  groups.set(group, (groups.get(group) ?? 0) + 1);
  if (at >= 0) show(group, values[at]);
  signal.addEventListener("abort", () => {
    const left = groups.get(group) - 1;
    if (left > 0) return groups.set(group, left);
    groups.delete(group);
    chosen.delete(group);
  });
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
  if (radio instanceof HTMLAnchorElement) {
    const to = address(radio, radios);
    if (to !== location.href) history.replaceState(history.state, "", to);
  }
  show(group, values[at]);
}

/**
 * The address with `radio` chosen: its link's, for a link to another page; on this page, the
 * current address with only the query keys the options' links differ in taken from it, so another
 * control's keys (a tab, a page) stay as they are.
 * @param {HTMLAnchorElement} radio @param {HTMLElement[]} radios @returns {string}
 */
function address(radio, radios) {
  const to = new URL(radio.href);
  const now = new URL(location.href);
  if (to.origin !== now.origin || to.pathname !== now.pathname) return to.href;
  const links = radios.filter((l) => l instanceof HTMLAnchorElement).map((l) => new URL(/** @type {HTMLAnchorElement} */ (l).href).searchParams);
  const differ = (/** @type {string} */ key) => new Set(links.map((q) => q.getAll(key).join("\u0000"))).size > 1;
  // In the link's order, the route's, then any key only the address has.
  const query = new URLSearchParams();
  for (const key of new Set([...to.searchParams.keys(), ...now.searchParams.keys()])) {
    for (const value of (differ(key) ? to.searchParams : now.searchParams).getAll(key)) query.append(key, value);
  }
  now.search = query.toString();
  return now.href;
}

/**
 * Remembers `value` as the group's choice and shows or hides every part of the group for it.
 * @param {string} group @param {string} value
 */
function show(group, value) {
  chosen.set(group, value);
  for (const { part, values: shown } of parts.get(group) ?? []) part.hidden = !shown.includes(value);
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
