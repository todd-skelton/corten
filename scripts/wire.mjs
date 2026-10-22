// Checks the runtime's data-on parser against the strings the Rust side writes:
// crates/corten/src/script/wire.json (script/tests.rs checks the Rust side). Run by npm run check.
import { readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
import { parseOn } from "../crates/corten/js/on.js";

const { cases } = JSON.parse(readFileSync(new URL("../crates/corten/src/script/wire.json", import.meta.url), "utf8"));
let failed = 0;
for (const { name, attr, parsed } of cases) {
  let got;
  try {
    got = parseOn(attr);
  } catch (error) {
    got = String(error);
  }
  if (!isDeepStrictEqual(got, parsed)) {
    failed += 1;
    console.error(`wire: ${name}\n  attr:     ${attr}\n  expected: ${JSON.stringify(parsed)}\n  got:      ${JSON.stringify(got)}`);
  }
}
console.log(`wire: ${cases.length - failed} of ${cases.length} data-on cases parse as the server meant`);
process.exit(failed ? 1 : 0);
