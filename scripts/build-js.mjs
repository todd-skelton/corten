// Builds the client code the corten crate embeds, from the sources in crates/corten/js:
//   dist/corten.js  the runtime, minified (the source keeps its comments; browsers get none)
//   dist/solid.js   Solid + corten/solid, bundled and minified
// `node scripts/build-js.mjs --check` fails when a built file is not what its source builds to.
import { build } from "esbuild";
import { readFileSync, writeFileSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const js = join(root, "crates", "corten", "js");
const check = process.argv.includes("--check");
const solid = JSON.parse(readFileSync(join(root, "node_modules", "solid-js", "package.json"), "utf8"));
const common = { bundle: true, format: "esm", minify: true, target: "es2022", legalComments: "none", write: false };
const outputs = [
  {
    name: "corten.js",
    entryPoints: [join(js, "corten.js")],
    banner: { js: "// Corten runtime (MIT). Built from crates/corten/js/corten.js by scripts/build-js.mjs; do not edit." },
  },
  {
    name: "solid.js",
    entryPoints: [join(js, "solid.js")],
    // "corten" stays an import, resolved by the page's import map.
    external: ["corten"],
    conditions: ["browser"],
    banner: { js: `// Solid ${solid.version} (MIT, Ryan Carniato) + corten/solid. Built by scripts/build-js.mjs; do not edit.` },
  },
];

let stale = 0;
for (const { name, ...options } of outputs) {
  const out = join(js, "dist", name);
  const result = await build({ ...common, ...options, outfile: out });
  const text = result.outputFiles[0].text;
  let current = "";
  try {
    current = readFileSync(out, "utf8");
  } catch {}
  if (check) {
    if (current !== text) {
      console.error(`${out} is stale: run npm run build:js`);
      stale += 1;
    }
  } else {
    writeFileSync(out, text);
    console.log(`${out}: ${text.length} B, ${gzipSync(text, { level: 9 }).length} B gzip`);
  }
}
process.exit(stale ? 1 : 0);
