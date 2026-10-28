// @ts-check
// RelativeTime (relative_time.rs). The server wrote the time relative to now; this writes it again,
// in the same locale and style, whenever its text would change. One timer serves every relative
// time on the page, set for the soonest change.

/** The units, largest first, in seconds (the server's: crates/corten/src/i18n/format.rs). */
const UNITS = /** @type {const} */ ([
  ["day", 86_400],
  ["hour", 3_600],
  ["minute", 60],
  ["second", 1],
]);

/** The relative times on the page: the instant each counts to, and its format. @type {Map<HTMLTimeElement, { at: number, format: Intl.RelativeTimeFormat }>} */
const live = new Map();
let timer = 0;

/**
 * Keeps `time` current, until `signal` aborts.
 * @param {HTMLTimeElement} time @param {string} locale @param {string} style
 * @param {AbortSignal} signal
 */
export function connect(time, locale, style, signal) {
  const at = Date.parse(time.dateTime);
  if (Number.isNaN(at) || signal.aborted) return;
  const options = { style: /** @type {Intl.RelativeTimeFormatStyle} */ (style), numeric: /** @type {const} */ ("always") };
  live.set(time, { at, format: new Intl.RelativeTimeFormat(locale, options) });
  signal.addEventListener(
    "abort",
    () => {
      live.delete(time);
      schedule();
    },
    { once: true },
  );
  draw(time, Date.now());
  schedule();
}

/** The unit `ms` (from now; negative: before) is written in, and its size in milliseconds. @param {number} ms */
function unitOf(ms) {
  const seconds = Math.floor(Math.abs(ms) / 1000);
  const [unit, size] = UNITS.find(([, size]) => seconds >= size) ?? UNITS[UNITS.length - 1];
  return { unit, step: size * 1000 };
}

/** Writes `time`'s text as of `now`, as the server does: whole units toward zero, after `now` the future. @param {HTMLTimeElement} time @param {number} now */
function draw(time, now) {
  const entry = live.get(time);
  if (!entry) return;
  const ms = entry.at - now;
  const { unit, step } = unitOf(ms);
  const n = Math.floor(Math.abs(ms) / step);
  const text = entry.format.format(ms > 0 ? n : -n, unit);
  if (time.textContent !== text) time.textContent = text;
}

/** Milliseconds until the text for `at` changes, from `now`. @param {number} at @param {number} now */
function untilChange(at, now) {
  const ms = at - now;
  const { step } = unitOf(ms);
  // Ahead: when the count drops below the whole units it has. Behind (or now): when one more
  // unit has passed.
  return ms > 0 ? (ms % step) + 1 : step - (-ms % step);
}

/** Sets the one timer for the soonest change. */
function schedule() {
  clearTimeout(timer);
  timer = 0;
  if (!live.size) return;
  const now = Date.now();
  let soonest = Infinity;
  for (const { at } of live.values()) soonest = Math.min(soonest, untilChange(at, now));
  timer = setTimeout(tick, soonest);
}

function tick() {
  const now = Date.now();
  for (const time of live.keys()) draw(time, now);
  schedule();
}
