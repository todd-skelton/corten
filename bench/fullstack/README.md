# Full-stack comparison: one marketplace item page in six frameworks

The same item detail page (title, images, sorted listings table, offers with "show all", a
price-alert form that posts and patches a message) built in:

| App | Stack | Directory |
|---|---|---|
| corten | Corten 0.1 (Rust, axum, `html!` views, custom-element islands; measured before handlers replaced them) | [examples/item-page](../../examples/item-page) |
| nextjs | Next.js 16.4.2, App Router, React Server Components, server action | [apps/nextjs](apps/nextjs) |
| tanstack-start | TanStack Start 1.169.3, React 19, server functions, Nitro | [apps/tanstack-start](apps/tanstack-start) |
| sveltekit | SvelteKit 3.1.0, Svelte 5.57, adapter-node 6, form action | [apps/sveltekit](apps/sveltekit) |
| react-router | React Router 7.18.4 on Express | [apps/react-router](apps/react-router) |
| leptos | Leptos 0.8 (Rust, SSR plus WASM hydration, axum) | [apps/leptos](apps/leptos) |

Every app renders the same visible text for the same 16 URLs, 404 included (`parity.mjs`), and
each has the same interactions: a client-side sort, a form action that updates part of the page,
and client-side navigation from the home page. The Corten app also runs everything a production
deployment would (CSP with nonces, CSRF check, compression, versioned assets, service worker,
error pages, health, MCP tools); the others run their framework defaults.

## Results

All numbers are measured, from one run (`results/r1/`, 2026-10-21). "1 CPU" means each app's
container was limited to one CPU.

### Cold start (no traffic before the first request)

| App | First 200 after process start, median (min-max) of 10 | Memory after first request | Image |
|---|---|---|---|
| baseline (busybox httpd, static file) | 210 ms (129-305) | 0.5 MiB | 2 MB |
| **corten** | **184 ms** (138-385) | **1.6 MiB** | **80 MB** |
| leptos | 220 ms (170-347) | 1.7 MiB | 85 MB |
| sveltekit | 425 ms (362-546) | 23.5 MiB | 231 MB |
| tanstack-start | 496 ms (350-1,178) | 26.4 MiB | 230 MB |
| react-router | 1,035 ms (800-1,264) | 33.0 MiB | 245 MB |
| nextjs | 1,679 ms (1,345-2,096) | 49.2 MiB | 266 MB |

Corten's start is indistinguishable from Docker's own overhead (the baseline): views are
compiled into the binary and static files load in a few milliseconds.

### Server rendering without a database (items in memory, 1 CPU)

| App | 300/s offered: served, p50 / p99 | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | **300/s, 0.8 / 1.1 ms** (14% CPU) | **3,355, 17 / 50 ms** | **2,566, 182 ms, 0%** | 19-24 MiB |
| leptos | 300/s, 2.7 / 121 ms (72% CPU) | 440, 140 / 262 ms | 458, 1.3 s, 0% | 18-35 MiB |
| sveltekit | 163/s, 815 ms / 56 s | 224, 283 / 545 ms | 329, 6.0 s, 0.3% | 150-160 MiB |
| react-router | 142/s, 902 ms / 60 s | 173, 353 / 616 ms | 154, 38 s, 0.6% | 131-133 MiB |
| tanstack-start | 145/s, 896 ms / 60 s | 137, 423 / 989 ms | 153, 60 s, 1.3% | 117-122 MiB |
| nextjs | 55/s, 2.1 s / 60 s | 52, 1,308 / 1,805 ms | 85, 60 s, 1.2% | 185-198 MiB |

Every JavaScript app was CPU-bound below 300/s, so their fixed-rate rows show queueing (60 s is
the load generator's timeout). Rendering this page, Corten serves 7.6x Leptos and 15-65x the
JavaScript frameworks on the same CPU.

These numbers were taken when Corten rendered with askama templates. After the switch to `html!`,
Corten alone was run again twice back to back on 2026-10-21 (`results/askama-recheck/` with the
old build, `results/html-macro/` with the new one): 64 users 8,496 req/s (askama) and 6,928
(`html!`), 256 users 6,087 and 8,321, all 0% errors. The two swap places between columns, so
the difference is noise; both runs are faster than r1 because nothing else was running. The other
frameworks were not rerun, so the table keeps the r1 numbers.

### Server rendering with the item API (1 CPU)

Each request fetches and parses the item (about 55 KB of JSON) from the Rust item API.

| App | 150/s offered: served, p50 / p99 | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | **150/s, 3.3 / 12.8 ms** (34% CPU) | 418, 151 / 290 ms | 226, 1.8 s, 0% | 20-64 MiB |
| leptos | 150/s, 3.9 / 53 ms (54% CPU) | 492, 126 / 245 ms | 444, 1.5 s, 0% | 17-58 MiB |
| react-router | 111/s, 2.0 s / 47 s | 146, 417 / 773 ms | 188, 13.5 s, 0% | 147-158 MiB |
| sveltekit | 138/s, 73 ms / 9.5 s | 144, 450 / 616 ms | 146, 41 s, 0.6% | 133-155 MiB |
| tanstack-start | 95/s, 2.0 s / 52 s | 129, 495 / 898 ms | 132, 48 s, 0.8% | 161-174 MiB |
| nextjs | 34/s, 4.3 s / 60 s | 33, 1,841 / 3,958 ms | 39, 60 s, 2.5% | 461-520 MiB |

With a real data fetch, both Rust apps spend most of their CPU parsing the API response, so they
end up close to each other and about 3x the best JavaScript framework. The 256-user column for
the Rust apps varies between runs: a second run of only Corten and Leptos (`results/recheck/`)
gave Corten 499 req/s, p99 960 ms (r1: 226) and Leptos 272 req/s, p99 2.2 s (r1: 444), both with
0 errors; at 64 users Corten 404 and Leptos 414 req/s. Treat the two as equal with a database.

### Bytes the item page loads (raw / gzip, KB)

| App | HTML | JavaScript (files) | WASM | CSS |
|---|---|---|---|---|
| **corten** | 11.0 / 2.3 | **16.8 / 6.2** (4) | 0 | 2.3 / 0.9 |
| leptos | 17.8 / 3.5 | 24.8 / 7.0 (1) | 719.9 / 286.7 | 2.0 / 0.8 |
| sveltekit | 16.3 / 3.6 | 96.3 / 37.0 (11) | 0 | 1.9 / 0.8 |
| react-router | 19.5 / 4.9 | 311.2 / 99.2 (4) | 0 | 1.9 / 0.8 |
| tanstack-start | 17.2 / 4.2 | 341.2 / 108.6 (3) | 0 | 1.9 / 0.8 |
| nextjs | 25.3 / 4.8 | 456.2 / 136.1 (7) | 0 | 1.9 / 0.8 |

Corten's JavaScript is the runtime (`corten.js`), the two islands this page uses and the app's
`app.js`, and that includes the service worker registration and prefetching.

Since this run the runtime gained typed seams and `#[island]` actions. Re-measured the same way
on 2026-10-21 (Corten only, one run): 21.0 / 7.6 KB before the island change, **22.3 / 8.0 KB**
after it (4 files); HTML 10.8 / 2.3 KB. The runtime is now served minified from its documented
source (`scripts/build-js.mjs`): **10.5 / 4.5 KB** in the same 4 files (2026-10-21, Corten only,
one run, `/items/item_5`, the runtime 9.4 KB of it).

Styles then moved into a CSS file per component and a token file (2026-10-21, Corten only, the
files as served, gzip -9 in Node): CSS **4.7 / 1.6 KB in 2 files** (`style.css` 1.5 / 0.7,
`components.css` 3.2 / 1.0), up from 2.5 / 1.0 KB in 1. The increase is what the example
added, not scoping: the token file's dark and compact themes and responsive utility classes
(1.1 KB raw of its 1.7 KB) and the longer scoped class names.

### Browser, median of 10 (p90 in parentheses where it matters)

Each cold load starts on a page from another origin, as when arriving from a search result.

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| **corten** | **179 (304) ms** | 188 ms | 0 ms | 89 ms | **11 (27) ms** | **1.3 MB** |
| sveltekit | 217 (292) ms | 188 ms | 0 ms | 87 ms | 22 (31) ms | 2.1 MB |
| tanstack-start | 229 (368) ms | 184 ms | 3 ms | 90 ms | 32 (48) ms | 2.8 MB |
| nextjs | 253 (383) ms | 180 ms | 6 ms | 122 ms | 200 (266) ms | 3.1 MB |
| react-router | 272 (504) ms | 212 ms | 0 ms | 108 ms | 35 (53) ms | 2.5 MB |
| leptos | 293 (338) ms | 264 ms | 0 ms | 84 ms | 32 (43) ms | 1.7 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| **corten** | **797 (1,003) ms** | 780 ms | **299 ms** | **349 ms** | **207 (228) ms** | **1.3 MB** |
| sveltekit | 1,195 (1,304) ms | 820 ms | 397 ms | 407 ms | 336 (366) ms | 2.1 MB |
| react-router | 1,362 (1,528) ms | 820 ms | 436 ms | 497 ms | 334 (381) ms | 2.6 MB |
| nextjs | 1,831 (1,887) ms | 748 ms | 694 ms | 410 ms | 751 (829) ms | 3.1 MB |
| leptos | 2,050 (2,091) ms | 724 ms | 485 ms | 426 ms | 369 (389) ms | 1.7 MB |
| tanstack-start | 2,707 (2,796) ms | 748 ms | 514 ms | 431 ms | 437 (459) ms | 2.9 MB |

Starting each load from `about:blank` instead (`results/r1/browser-about-blank.json`) slows only
Corten's first paint by about 80 ms. Corten sends `Cross-Origin-Opener-Policy: same-origin` (part
of its default security policy; the other apps do not set it), and Chromium moves such a page into
a new process when it is opened from `about:blank`. Arriving from another site, every page pays
that switch, so the run above is the realistic one. Serving identical bytes with and without the
header from `about:blank`, five loads each: first subresource request at 6-18 ms without it and
76-100 ms with it. Corten keeps the header: it stops other sites' windows from reaching the page.

### After the type and performance pass (before/after, Corten only)

`ab.mjs` runs the image the tables above were measured with (`cb-corten`) and the current build
(`cb-corten-next`) one at a time on one CPU, items from memory, alternating, two rounds each
(`results/ab-r1`, `results/ab-r2`; the r2 build also has the faster island scan). "browser" sends
`Accept-Encoding: gzip, deflate, br, zstd` as Chrome, Edge, Firefox and current Safari do, so
compression is measured; the tables above sent `identity`. Measured, Docker Desktop with other
containers running, so throughput at 64 users moved by up to 25% between rounds of the same image.

| Accept-Encoding | Load | Before | After |
|---|---|---|---|
| identity | 300/s: CPU, p50 | 15-16%, 0.88 ms | 12%, 0.77 ms |
| identity | 64 users: req/s (4 runs) | 1,993-3,045 (median 2,700) | 2,702-3,488 (median 3,400) |
| browser | 300/s: CPU, p50 | 31-33%, 1.45 ms | 15%, 0.87 ms |
| browser | 64 users: req/s, p99 | 635-789, 145-169 ms | 1,835-2,401, 63-79 ms |
| browser | bytes per response | 2,800 (brotli 4) | 3,070 (zstd 1) |

What changed: pages compressed with zstd 1 (brotli 2, gzip 1 as fallbacks) by `corten::compress`
instead of tower-http's defaults (brotli 4 here), the runtime and assets compressed once at brotli
11, the item page borrowing the cached item instead of cloning it (about 40-50 µs per request
in `render_cost`), and the island scan looking for dashes instead of every tag (21 µs to 2.4 µs
on this page). Compressed responses are 9% larger for 2.4-3x the throughput.

### Summary

1. Cold start and memory: Corten starts as fast as an empty container (184 ms median vs 210 ms
   baseline), uses 1.6 MiB after its first request and stays under 64 MiB under load. The
   JavaScript frameworks start 2.3-9x slower and use 15-30x the memory after the first request
   and 5-10x under load.
2. Rendering: without a database Corten renders this page 15-65x faster than the JavaScript
   frameworks on one CPU, with p99 of 1.1 ms at 300/s. With the real API fetch, every framework
   is bound by parsing the item JSON; Corten and Leptos are still about 3x the best JavaScript
   framework.
3. Browser: 6 KB of gzipped JavaScript instead of 37-136 KB (or 287 KB of WASM). On a throttled
   phone Corten is interactive first (797 ms vs 1.2-2.7 s) with the least blocking time and the
   fastest form action. Client navigation is the fastest on both profiles (2-18x on desktop,
   1.6-3.6x on the phone) and the heap is the smallest. On desktop its median time to interactive
   is also the lowest (179 ms vs 217 ms next), but the p90s overlap. LCP is the same for everyone
   within noise, because all six render the page on the server.
4. Where Corten does not win: with a database Leptos matches it, and on desktop the form action
   is a tie with Leptos, SvelteKit and TanStack Start.

## Method

1. Machine: Intel Core i9-10885H (8 cores, 16 threads), 64 GB RAM, Windows 11, Docker Desktop
   29.8 (Linux VM with 16 CPUs and 31 GiB). Node 24 in the JavaScript images, Rust stable.
2. Each app runs in its own container limited to 1 CPU and 1 GiB. The load generator (k6, 4 CPUs)
   and the item API run in other containers on the same Docker network.
3. Cold start (`cold-start.mjs`): the time from Docker starting the server process to its first
   200 for an item page, measured inside the container's network namespace (no port proxy), 10
   runs each. `baseline` is busybox httpd serving a static file: Docker's own start overhead,
   which is inside every number.
4. SSR without a database (`run-ssr.mjs <round> fixture`): items come from a 27.7 MB JSON file of
   508 items held in memory, so only the web framework is measured. 20 s warmup, then 60 s at a
   fixed 300 requests/s, 60 s with 64 closed-loop users, 60 s with 256. Items are drawn from 1 to
   500 with a skew toward low ids. Responses are uncompressed.
5. SSR with the item API (`run-ssr.mjs <round> api 150`): every request fetches the item from a
   Rust API backed by Postgres with a warm Redis cache, as in production. Same phases, 150/s fixed.
6. Browser (`browser.mjs`): Playwright Chromium, cache disabled, 10 runs per app and profile,
   each cold load starting on a page from another origin.
   Desktop is unthrottled; mobile is 4x CPU slowdown, 150 ms RTT, 1.6 Mbps down. "Interactive" is
   when the page's own code marks itself ready (hydration finished, or islands connected). Action
   is fill, submit, and wait for the confirmation. Navigation is home page to item page.
7. Bytes (`bytes.mjs`): every script, WASM and stylesheet the item page loads, fetched raw and
   gzipped here at level 6, so server compression settings do not change the comparison.

## Caveats

1. One machine, one run per phase, under Docker Desktop. Differences under about 15% are noise.
2. The data is synthetic (generated catalog items), sized like a production marketplace's item pages.
3. JavaScript dependencies are pinned in each `package.json` but there are no lockfiles, so
   transitive versions can drift between builds.
4. The page is small. Apps with large client-side state (a seller grid of live rows) are a
   different test; Corten's answer there is a Solid island fed by its live feed.
5. Each framework's defaults were used. Each could be tuned (caching, streaming, partial
   prerendering). None of the apps cache rendered pages.

## Running it

```powershell
# images (from the repository root)
docker build -f examples/item-page/Dockerfile -t cb-corten .
docker build -t cb-nextjs bench/fullstack/apps/nextjs
docker build -t cb-tanstack-start bench/fullstack/apps/tanstack-start
docker build -t cb-sveltekit bench/fullstack/apps/sveltekit
docker build -t cb-react-router bench/fullstack/apps/react-router
docker build -t cb-leptos bench/fullstack/apps/leptos
# the item API on network "rustbench" as rb-rust:8080, then the fixture for no-database runs
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400
# everything, about an hour
cd bench/fullstack; node run-all.mjs r1 10
```

The item API is a separate Rust service and is not in this repository. Cold start,
the API runs and the browser runs need it; without it, `node run-ssr.mjs r1 fixture 300` still
runs given a fixture file in the same shape (`{"item_1": {...}, ...}`).
