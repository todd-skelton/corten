// @ts-check
// LocalTime (local_time.rs). On a browser's first page the server does not know its time zone
// yet and wrote the date in UTC; this writes it again in the browser's zone, with the server's
// format (the locale and style it was written in), at the browser's offset for that instant.

/** @import { DateFormat } from "corten" */

/**
 * @param {HTMLTimeElement} time @param {DateFormat} format the date's format, in UTC
 */
export function connect(time, format) {
  const at = Date.parse(time.dateTime);
  if (Number.isNaN(at)) return;
  time.textContent = format(at, { offset: -new Date(at).getTimezoneOffset() * 60 });
}
