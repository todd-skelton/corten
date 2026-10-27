// Checks the browser's formats (crates/corten/js/format.js) against the server's: each case
// Intl wrote (crates/corten/src/i18n/cases.tsv, which i18n.rs checks the Rust side against), run
// with the format the server sends a script for it (src/i18n/specs.tsv, written by i18n.rs).
// Run by npm run check.
import { readFileSync } from "node:fs";
import { formatter } from "../crates/corten/js/format.js";

const rows = (file) =>
  readFileSync(new URL(`../crates/corten/src/i18n/${file}`, import.meta.url), "utf8")
    .split("\n")
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => line.split("\t"));
const specs = new Map(rows("specs.tsv").map(([tag, key, json]) => [`${tag} ${key}`, JSON.parse(json)]));

/** An amount in minor units, or null when it has more digits than the currency shows (rounding it is the server's). @param {string} amount @param {number} d */
function minor(amount, d) {
  const [, sign, int, frac = ""] = /** @type {RegExpExecArray} */ (/^(-?)(\d+)(?:\.(\d+))?$/.exec(amount));
  if (frac.length > d) return null;
  return Number(sign + int + frac.padEnd(d, "0"));
}

let failed = 0;
let checked = 0;
const check = (what, got, expected) => {
  checked += 1;
  if (got !== expected) {
    failed += 1;
    if (failed <= 20) console.error(`format: ${what}\n  expected: ${JSON.stringify(expected)}\n  got:      ${JSON.stringify(got)}`);
  }
};
for (const [tag, kind, input, expected] of rows("cases.tsv")) {
  const [what, style, offset] = kind.split(" ");
  if (what === "number") {
    const format = formatter(specs.get(`${tag} number`));
    check(`${tag} number ${input}`, format(input), expected);
    // A number the script computed, when it is the same value.
    if (String(Number(input)) === input) check(`${tag} number ${input} (a number)`, format(Number(input)), expected);
  } else if (what === "percent") {
    const format = formatter(specs.get(`${tag} percent ${style}`));
    check(`${tag} percent ${style} ${input}`, format(input), expected);
    if (String(Number(input)) === input) check(`${tag} percent ${style} ${input} (a number)`, format(Number(input)), expected);
  } else if (what === "money") {
    const [amount, code] = input.split(" ");
    const spec = specs.get(`${tag} money ${code}`);
    const cents = minor(amount, /** @type {number} */ (spec.d));
    if (cents !== null) check(`${tag} money ${input}`, formatter(spec)(cents), expected);
  } else {
    const spec = { ...specs.get(`${tag} date ${style}`), o: Number(offset ?? 0) };
    check(`${tag} ${kind} ${input}`, formatter(spec)(input), expected);
    check(`${tag} ${kind} ${input} (a Date)`, formatter(spec)(new Date(input)), expected);
  }
}
console.log(`format: ${checked - failed} of ${checked} cases write what the server writes`);
process.exit(failed || checked < 9000 ? 1 : 0);
