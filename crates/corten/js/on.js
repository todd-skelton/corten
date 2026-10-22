// @ts-check
// The `data-on` wire format: what `on:click={js::confirm(event, dialog)}` prints and the runtime
// reads. No DOM here, so `scripts/wire.mjs` checks this parser against the strings the Rust side
// writes (`crates/corten/src/script/wire.json`). Bundled into dist/corten.js.

/**
 * One handler call of a `data-on` attribute: `click:cart_line.confirm($e,@dialog,"Lamp")`.
 * @typedef {{ type: string, module: string, name: string, args: Arg[] }} Binding
 * @typedef {{ kind: "@" | "*" | "#" | "^", owner: string | null, name: string } | { kind: "$e" | "$t" | "$s" | "$c", owner: string | null } | { kind: "v", value: unknown } | { kind: "{", fields: [string, Arg][] }} Arg
 */

/**
 * Parses `type:module.name(arg,arg) type:module.name()`. Arguments are refs (`@rows`, every one:
 * `*rows`, every one by its `data-key`: `#rows`, the one around the event's target: `^rows`,
 * another component's: `@offer-list/rows`), the browser's values (`$e` event, `$t` the element
 * with the attribute, `$s` its signal, `$c` the component's root, `$c/offer-list`), a group of
 * arguments (`{rows:*row,all:@all}`), or JSON. A module name may have dots (`ui.combo_box`).
 * @param {string} source
 * @returns {Binding[]}
 */
export function parseOn(source) {
  /** @type {Binding[]} */
  const list = [];
  const n = source.length;
  let i = 0;
  while (i < n) {
    if (source[i] === " ") {
      i += 1;
      continue;
    }
    const colon = source.indexOf(":", i);
    const open = source.indexOf("(", colon);
    const dot = source.lastIndexOf(".", open);
    if (colon < 0 || open < 0 || dot < colon) throw new Error(`corten: data-on="${source}" is not type:module.handler(..)`);
    /** @type {Arg[]} */
    const args = [];
    const close = split(source, open + 1, ")", (part) => args.push(parseArg(part)));
    list.push({ type: source.slice(i, colon), module: source.slice(colon + 1, dot), name: source.slice(dot + 1, open), args });
    i = close + 1;
  }
  return list;
}

/**
 * Calls `each` with every comma-separated part of `source` from `start` to the `end` character at
 * depth 0 (strings, arrays and objects nest), and returns where that character is.
 * @param {string} source @param {number} start @param {string} end @param {(part: string) => void} each
 */
function split(source, start, end, each) {
  let depth = 0;
  let j = start;
  for (; j < source.length; j += 1) {
    const c = source[j];
    if (c === '"') {
      for (j += 1; j < source.length && source[j] !== '"'; j += 1) if (source[j] === "\\") j += 1;
    } else if (c === "[" || c === "{") depth += 1;
    else if (depth > 0 && (c === "]" || c === "}")) depth -= 1;
    else if (depth === 0 && (c === "," || c === end)) {
      if (j > start) each(source.slice(start, j));
      start = j + 1;
      if (c === end) break;
    }
  }
  return j;
}

/** @param {string} text @returns {Arg} */
function parseArg(text) {
  const c = text[0];
  if (c === "@" || c === "*" || c === "#" || c === "^") {
    const slash = text.indexOf("/");
    return slash < 0 ? { kind: c, owner: null, name: text.slice(1) } : { kind: c, owner: text.slice(1, slash), name: text.slice(slash + 1) };
  }
  if (c === "$") {
    const kind = /** @type {"$e" | "$t" | "$s" | "$c"} */ (text.slice(0, 2));
    return { kind, owner: text[2] === "/" ? text.slice(3) : null };
  }
  // A group's keys are bare names; a JSON object's are quoted.
  if (c === "{" && text[1] !== '"' && text[1] !== "}") {
    /** @type {[string, Arg][]} */
    const fields = [];
    split(text, 1, "}", (part) => {
      const colon = part.indexOf(":");
      fields.push([part.slice(0, colon), parseArg(part.slice(colon + 1))]);
    });
    return { kind: "{", fields };
  }
  return { kind: "v", value: JSON.parse(text) };
}
