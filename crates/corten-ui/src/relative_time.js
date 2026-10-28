// @ts-check
// RelativeTime (relative_time.rs). The server wrote the time relative to now; this writes it again,
// in the same locale and style, whenever its text would change. One timer serves every relative
// time on the page, set for the soonest change, and the page's own clock is read again when it
// comes back (from sleep, a hidden tab or the back/forward cache), since a timer may not have run.

/** The units, largest first, in seconds (the server's: crates/corten/src/i18n/format.rs). */
const UNITS = /** @type {const} */ ([
  ["day", 86_400],
  ["hour", 3_600],
  ["minute", 60],
  ["second", 1],
]);

/** The relative times on the page: the instant each counts to, and its format. @type {Map<HTMLTimeElement, { at: number, format: Intl.RelativeTimeFormat }>} */
const live = new Map();
/** One format per locale and style; `null` for a locale the browser does not have. @type {Map<string, Intl.RelativeTimeFormat | null>} */
const formats = new Map();
let timer = 0;
/** When the timer fires (`Date.now()` time), or Infinity. */
let due = Infinity;
let watching = false;

/**
 * Keeps `time` current, until `signal` aborts. A locale the browser does not have keeps the
 * server's text, which Intl would otherwise write in the browser's own language.
 * @param {HTMLTimeElement} time @param {string} locale @param {string} style
 * @param {AbortSignal} signal
 */
export function connect(time, locale, style, signal) {
  const at = Date.parse(time.dateTime);
  if (Number.isNaN(at) || signal.aborted) return;
  const key = `${locale} ${style}`;
  let format = formats.get(key);
  if (format === undefined) {
    const options = { style: /** @type {Intl.RelativeTimeFormatStyle} */ (style), numeric: /** @type {const} */ ("always") };
    format = new Intl.RelativeTimeFormat(locale, options);
    if (!speaks(format, locale)) format = null;
    formats.set(key, format);
  }
  if (!format) return;
  live.set(time, { at, format });
  // Gone: left out of the next tick. A wake for it alone finds nothing to change.
  signal.addEventListener("abort", () => live.delete(time), { once: true });
  const now = Date.now();
  draw(time, now);
  watch();
  const next = now + untilChange(at, now);
  // A timer whose time has passed has not run yet (the page slept, the clock was set forward):
  // it may be a day away by the timer's own clock, so this time sets its own.
  if (next < due || due <= now) setTimer(next, now);
}

/**
 * Whether `format` writes `locale`'s language in its script. A browser without the locale falls
 * back to a parent (`sr-Latn` to `sr`, in Cyrillic) or to its own language, and the server's
 * text is then the right one to keep. A region the browser lacks (`de-AT` as `de`) is the same
 * words.
 * @param {Intl.RelativeTimeFormat} format @param {string} locale
 */
function speaks(format, locale) {
  const want = new Intl.Locale(locale).maximize();
  const got = new Intl.Locale(format.resolvedOptions().locale).maximize();
  return want.language === got.language && want.script === got.script;
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
  // The same text node, changed in place, so a selection in it is kept.
  const node = time.firstChild;
  if (node instanceof Text && node === time.lastChild) {
    if (node.data !== text) node.data = text;
  } else if (time.textContent !== text) time.textContent = text;
}

/** Milliseconds until the text for `at` changes, from `now`. @param {number} at @param {number} now */
function untilChange(at, now) {
  const ms = at - now;
  const { step } = unitOf(ms);
  // Ahead: when the count drops below the whole units it has. Behind (or now): when one more
  // unit has passed.
  return ms > 0 ? (ms % step) + 1 : step - (-ms % step);
}

/** @param {number} next @param {number} now */
function setTimer(next, now) {
  clearTimeout(timer);
  due = next;
  timer = setTimeout(tick, next - now);
}

/** Writes every relative time as of now and sets the timer for the soonest change. */
function tick() {
  clearTimeout(timer);
  timer = 0;
  due = Infinity;
  const now = Date.now();
  let soonest = Infinity;
  for (const [time, { at }] of live) {
    draw(time, now);
    soonest = Math.min(soonest, untilChange(at, now));
  }
  if (soonest < Infinity) setTimer(now + soonest, now);
}

/** Once a page has a relative time: read the clock again when the page comes back. */
function watch() {
  if (watching) return;
  watching = true;
  document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && live.size && tick());
  addEventListener("pageshow", (event) => event.persisted && live.size && tick());
}
