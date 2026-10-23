// Builds every app into out/<app>/: index.html and one minified app.js (esbuild; Solid's JSX
// through babel-preset-solid). Prints each app's JavaScript size.
import { build } from "esbuild";
import { transformAsync } from "@babel/core";
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { gzipSync, brotliCompressSync } from "node:zlib";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const js = join(here, "..", "..", "crates", "corten", "js");
const out = join(here, "out");

/** @param {{ generate: "dom" | "ssr", hydratable?: boolean }} options */
const solidJsx = (options) => ({
  name: "solid-jsx",
  setup(b) {
    b.onLoad({ filter: /\.jsx$/ }, async (args) => {
      const source = readFileSync(args.path, "utf8");
      const result = await transformAsync(source, { filename: args.path, cwd: here, presets: [["babel-preset-solid", options]] });
      return { contents: result.code, loader: "js" };
    });
  },
});

const BUTTONS = [
  ["create", "Create 1,000"], ["create10k", "Create 10,000"], ["append", "Append 1,000"], ["update", "Update every 10th"],
  ["swap", "Swap rows"], ["clear", "Clear"], ["patch1", "Patch 1"], ["patch100", "Patch 100"],
];
/** The markup the server would render: controls, the table (with `rows`), and the row template. */
function body(on, rows = []) {
  const buttons = BUTTONS.map(([id, text]) => `<button id="${id}"${on ? ` data-on="click:${id}"` : ""}>${text}</button>`).join("");
  const a = (cls, inner) => `<a class="${cls}"${on ? ` data-on="click:${cls}"` : ""}>${inner}</a>`;
  const row = (r = { id: " ", label: " " }) => `<tr><td class="col-md-1">${r.id}</td><td class="col-md-4">${a("select", r.label)}</td><td class="col-md-1">${a("remove", '<span class="remove" aria-hidden="true">x</span>')}</td><td class="col-md-6"></td></tr>`;
  return `<div class="controls">${buttons}</div><table><tbody id="tbody"${on ? ' data-ref="body"' : ""}>${rows.map(row).join("")}</tbody></table><template id="row">${row()}</template>${rows.length ? data(rows) : ""}`;
}
/** The dashboard as the server renders it. */
function dashboard(on) {
  const { SYMBOLS, start } = market;
  const b = (id, text) => `<button id="${id}"${on ? ` data-on="click:${id}"` : ""}>${text}</button>`;
  const cards = SYMBOLS.map((s, i) => `<div class="card" data-symbol="${s}"><b>${s}</b> <span data-ref="price" class="up">${start(i).toFixed(2)}</span></div>`).join("");
  const total = SYMBOLS.reduce((t, _, i) => t + start(i), 0).toFixed(2);
  return `<div class="controls">${b("tick20", "Tick 20")}${b("tick200", "Tick 200")}</div><p>Total <span data-ref="total">${total}</span></p><div class="cards">${cards}</div>`;
}
const data = (rows) => `<script type="application/json" id="data">${JSON.stringify(rows)}</script>`;
/** A page around `main`: its contents, or the whole `<main>` when the Rust generator rendered it. */
const page = (title, main, head = "", events = "") => `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>${title}</title><link rel="stylesheet" href="../app.css"><script type="module" src="app.js"></script>${head}</head>
<body>${main.startsWith("<main") ? main : `<main id="main"${events ? ` data-events="${events}"` : ""}>${main}</main>`}</body></html>
`;
/**
 * A page the way Corten serves one: the import map, the runtime, the handler module the markup
 * names as a module script of the page (it hands its exports to the runtime as it runs), and a preload
 * of the views it imports.
 */
const cortenPage = (title, main, module) => `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>${title}</title><link rel="stylesheet" href="../app.css"><script type="importmap">{"imports":{"corten":"./runtime.js","corten/views":"./views.js"}}</script><meta name="corten-scripts" content="./scripts/"><script type="module" src="./runtime.js"></script><script type="module" src="./scripts/${module}.js"></script><link rel="modulepreload" href="./views.js"></head>
<body>${main}</body></html>
`;

// The rows the server renders on the server-rendered pages, with the apps' own generator.
const { label, nextId } = await import("./apps/shared/labels.js");
const market = await import("./apps/shared/market.js");
const initial = Array.from({ length: 1000 }, () => ({ id: nextId(), label: label() }));

/** Bundles a server entry for Node and returns its render(initial). */
async function server(entry, plugins = []) {
  const outfile = join(out, "_server", entry.replace(/[\\/]/g, "_") + ".mjs");
  await build({ entryPoints: [join(here, entry)], bundle: true, format: "esm", platform: "node", outfile, plugins, jsx: "automatic", jsxImportSource: "remix/component", nodePaths: [join(here, "node_modules")], logLevel: "error" });
  return (await import(pathToFileURL(outfile).href)).render;
}
// The view apps' pages come from Rust: html! renders them and #[component(view)] generates the
// Row class they import as corten/views (gen/, a crate outside the workspace).
const gen = join(out, "_gen");
mkdirSync(gen, { recursive: true });
writeFileSync(join(gen, "rows.json"), JSON.stringify(initial));
execFileSync("cargo", ["run", "-q", "--release", "--manifest-path", join(here, "gen", "Cargo.toml"), "--", join(gen, "rows.json"), join(gen, "view")], { stdio: "inherit" });
const viewPage = (name) => readFileSync(join(gen, "view", `${name}-main.html`), "utf8");

const solidPage = await (await server("apps/solid/server.jsx", [solidJsx({ generate: "ssr", hydratable: true })]))(initial);
const remixPage = await (await server("apps/remix/server.tsx"))(initial);

const apps = [
  { name: "vanilla", entry: "apps/vanilla/app.js", main: body(false) },
  { name: "state", entry: "apps/state/app.js", main: body(true) },
  // Corten's runtime runs these: the handlers are the Rust components' scripts (gen/src/*.js).
  { name: "view", module: "bench", main: viewPage("view") },
  { name: "solid", entry: "apps/solid/app.jsx", main: "", plugins: [solidJsx({ generate: "dom" })] },
  { name: "solid-h", entry: "apps/solid-h/app.js", main: "" },
  { name: "remix", entry: "apps/remix/app.tsx", main: "" },
  // Server-rendered: 1,000 rows in the HTML that the client has to take over.
  { name: "vanilla-ssr", entry: "apps/vanilla/app.js", main: body(false, initial) },
  { name: "state-ssr", entry: "apps/state/app.js", main: body(true, initial) },
  { name: "view-ssr", module: "bench", main: viewPage("view-ssr") },
  { name: "solid-ssr", entry: "apps/solid/hydrate.jsx", main: solidPage.html + data(initial), head: solidPage.head, plugins: [solidJsx({ generate: "dom", hydratable: true })] },
  { name: "remix-ssr", entry: "apps/remix/hydrate.tsx", main: remixPage.html },
  // The dashboard: 200 cards on one shared store, and a total.
  { name: "dashboard-vanilla", entry: "apps/dashboard-vanilla/app.js", main: dashboard(false) },
  { name: "dashboard-state", entry: "apps/dashboard-state/app.js", main: dashboard(true) },
  { name: "dashboard-view", module: "dashboard", main: viewPage("dashboard-view") },
  { name: "dashboard-solid", entry: "apps/dashboard-solid/app.jsx", main: "", plugins: [solidJsx({ generate: "dom" })] },
  { name: "dashboard-remix", entry: "apps/dashboard-remix/app.tsx", main: "" },
];

mkdirSync(out, { recursive: true });
copyFileSync(join(here, "apps", "shared", "app.css"), join(out, "app.css"));
const sizes = {};
for (const app of apps) {
  const dir = join(out, app.name);
  mkdirSync(dir, { recursive: true });
  if (app.module) {
    // What the browser gets from Corten: the runtime, the views module, and the handler module
    // (its own imports bundled in; "corten" and "corten/views" stay imports, for the import map).
    const runtime = readFileSync(join(js, "dist", "corten.js"));
    const views = readFileSync(join(gen, "view", "views.js"));
    const script = await build({
      entryPoints: [join(here, "gen", "src", `${app.module}.js`)],
      bundle: true, format: "esm", minify: process.env.MINIFY !== "0", target: "es2022", legalComments: "none", write: false,
      external: ["corten", "corten/views"],
    });
    // Then the line Corten appends to every handler module it serves (written by gen/).
    const footer = readFileSync(join(gen, "view", `${app.module}.footer.js`), "utf8");
    const code = Buffer.from(script.outputFiles[0].text.replace(/\n?$/, "\n") + footer.replace(/^\n/, ""));
    mkdirSync(join(dir, "scripts"), { recursive: true });
    writeFileSync(join(dir, "runtime.js"), runtime);
    writeFileSync(join(dir, "views.js"), views);
    writeFileSync(join(dir, "scripts", `${app.module}.js`), code);
    writeFileSync(join(dir, "index.html"), cortenPage(app.name, app.main, app.module));
    const files = [runtime, views, code];
    const sum = (f) => files.reduce((t, b) => t + f(b), 0);
    sizes[app.name] = { raw: sum((b) => b.length), gzip: sum((b) => gzipSync(b, { level: 9 }).length), brotli: sum((b) => brotliCompressSync(b).length) };
    console.log(`${app.name.padEnd(8)} ${String(sizes[app.name].raw).padStart(7)} B  ${String(sizes[app.name].gzip).padStart(6)} B gzip  ${String(sizes[app.name].brotli).padStart(6)} B br (runtime + views + ${app.module}.js)`);
    continue;
  }
  const result = await build({
    entryPoints: [join(here, app.entry)],
    bundle: true, format: "esm", minify: process.env.MINIFY !== "0", target: "es2022", legalComments: "none", write: false,
    outfile: join(dir, "app.js"),
    conditions: ["browser"],
    alias: { "corten/state": join(here, "apps", "shared", "state.js"), "corten/solid": join(js, "solid.js") },
    jsx: "automatic", jsxImportSource: "remix/component",
    define: { "process.env.NODE_ENV": '"production"' },
    plugins: app.plugins ?? [],
    nodePaths: [join(here, "node_modules")],
  });
  const code = result.outputFiles[0].contents;
  writeFileSync(join(dir, "app.js"), code);
  writeFileSync(join(dir, "index.html"), page(app.name, app.main, app.head, app.main.includes("data-on=") ? "click" : ""));
  sizes[app.name] = { raw: code.length, gzip: gzipSync(code, { level: 9 }).length, brotli: brotliCompressSync(code).length };
  console.log(`${app.name.padEnd(8)} ${String(code.length).padStart(7)} B  ${String(sizes[app.name].gzip).padStart(6)} B gzip  ${String(sizes[app.name].brotli).padStart(6)} B br`);
}
writeFileSync(join(out, "sizes.json"), JSON.stringify(sizes, null, 2));
