# Full-stack comparison: one marketplace item page in fourteen frameworks

The same item detail page (title, images, sorted listings table, offers with "show all", a
price-alert form that posts and patches a message), and the same item as JSON, built in:

| App | Stack | Directory |
|---|---|---|
| corten | Corten 0.1 (Rust, axum, `html!` views, handlers) | [examples/card-market](../../examples/card-market) |
| leptos | Leptos 0.8 (Rust, SSR plus WASM hydration, axum) | [apps/leptos](apps/leptos) |
| nextjs | Next.js 16.4.2, App Router, React Server Components, server action | [apps/nextjs](apps/nextjs) |
| tanstack-start | TanStack Start 1.169.3, React 19, server functions, Nitro | [apps/tanstack-start](apps/tanstack-start) |
| sveltekit | SvelteKit 3.1.0, Svelte 5.57, adapter-node 6, form action | [apps/sveltekit](apps/sveltekit) |
| react-router | React Router 7.18.4 on Express | [apps/react-router](apps/react-router) |
| astro | Astro 7.4.2, Node adapter (standalone), client router, Actions | [apps/astro](apps/astro) |
| nuxt | Nuxt 4.6.2, Vue 3.5, Nitro | [apps/nuxt](apps/nuxt) |
| aspnet | ASP.NET Core 10 Razor Pages on Kestrel, htmx | [apps/aspnet](apps/aspnet) |
| spring | Spring Boot 4.1.1, Java 25, Thymeleaf on Tomcat, htmx | [apps/spring](apps/spring) |
| django | Django 6.1.2, Python 3.14, gunicorn 26 (3 sync workers), htmx | [apps/django](apps/django) |
| rails | Rails 8.1.4, Ruby 3.4 with YJIT, Puma (3 threads), Turbo and Stimulus | [apps/rails](apps/rails) |
| laravel | Laravel 13.38, PHP 8.4, Octane on FrankenPHP (2 workers), Livewire 4.4 | [apps/laravel](apps/laravel) |
| phoenix | Phoenix 1.8.16, LiveView 1.2, Elixir 1.19 on OTP 28, Bandit | [apps/phoenix](apps/phoenix) |

Every app renders the same visible text for the same 16 URLs, 404 included (`parity.mjs`), and
answers `GET /api/items/{id}` with the same JSON (`check-app.mjs` checks both before an app joins
a round). Each has the same interactions: a client-side sort, a form action that updates part of
the page, and client-side navigation from the home page, each done the framework's usual way
(htmx for Razor Pages, Spring and Django; Turbo for Rails; Livewire for Laravel; LiveView for
Phoenix). The Corten app also runs everything a production deployment would (CSP, CSRF check,
compression, versioned assets, service worker, error pages, health, MCP tools); the others run
their framework defaults in production mode.

The same apps hold the live-updates comparison in [bench/live](../live/README.md).

## Results

All numbers are measured, from one run (`results/r6/`, 2026-10-27 UTC, the tree at `cf8ac0f`) on one
DigitalOcean c-8 droplet (Intel Xeon Platinum 8280); see [Method](#method). Failed means the app could not do the
test with its usual settings in one CPU and 1 GiB (see caveat 5); its other numbers are kept
where it produced any. r5 (`results/r5/`, a Platinum 8168, Corten at `f80d4a1`, before the
read-again, place-keeping and catalog-formatting runtime that r6's item page loads), r4 (`results/r4/`, the same CPU model, Corten at `05fcb74`, before the
JSON derives, the views runtime and the cheaper escape and route parameters), r3 (`results/r3/`, a Platinum 8280, Corten before it parsed the item API's
response into typed structs), r2 (`results/r2/`, a Platinum 8280, Corten before handlers replaced
islands and before `corten::Json`) and r1 (`results/r1/`, six frameworks, a Platinum 8168 and
20 s warmups) are kept for the record; times are compared within a round, not across rounds.

### Cold start

| App | Process start to first 200, median (min-max) of 10 | Memory after first request | Image |
|---|---|---|---|
| **corten** | 6.0 ms (5.1 ms-32 ms) | 1.105 MiB | 123 MB |
| leptos | 6.9 ms (6.1 ms-12 ms) | 1.484 MiB | 127 MB |
| sveltekit | 178 ms (170 ms-192 ms) | 24.31 MiB | 333 MB |
| tanstack-start | 222 ms (208 ms-283 ms) | 26.37 MiB | 332 MB |
| react-router | 313 ms (301 ms-358 ms) | 31.77 MiB | 353 MB |
| nuxt | 314 ms (306 ms-404 ms) | 35.68 MiB | 335 MB |
| astro | 328 ms (316 ms-393 ms) | 36.82 MiB | 526 MB |
| nextjs | 707 ms (697 ms-782 ms) | 51.36 MiB | 385 MB |
| aspnet | 749 ms (740 ms-788 ms) | 30.57 MiB | 342 MB |
| laravel | 876 ms (840 ms-945 ms) | 96.3 MiB | 958 MB |
| rails | 1.1 s (1.1 s-1.1 s) | 63.88 MiB | 431 MB |
| django | 1.1 s (1.1 s-1.3 s) | 63.22 MiB | 279 MB |
| phoenix | 1.3 s (1.3 s-1.4 s) | 125.5 MiB | 262 MB |
| spring | 8.5 s (8.3 s-8.8 s) | 146.6 MiB | 516 MB |

### Item page, items in memory (no database)

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.2 ms / 0.4 ms, 4% | 23,416, 2.1 ms / 11 ms | 20,831, 39 ms, 0% | 20-26 MiB |
| aspnet | 300/s, 0.5 ms / 2.5 ms, 17% | 4,298, 15 ms / 22 ms | 4,190, 95 ms, 0% | 86-89 MiB |
| phoenix | 300/s, 1.0 ms / 1.3 ms, 33% | 1,614, 39 ms / 52 ms | 1,522, 2.3 s, 0% | 280-297 MiB |
| leptos | 300/s, 0.9 ms / 1.1 ms, 23% | 1,487, 42 ms / 62 ms | 1,321, 311 ms, 0% | 16-33 MiB |
| astro | 300/s, 0.9 ms / 30 ms, 31% | 1,263, 45 ms / 114 ms | 1,230, 277 ms, 0% | 134-135 MiB |
| spring | 300/s, 1.1 ms / 5.3 ms, 40% | 1,114, 42 ms / 320 ms | 1,111, 1.0 s, 0% | 356-380 MiB |
| sveltekit | 300/s, 1.0 ms / 20 ms, 36% | 986, 56 ms / 124 ms | 861, 330 ms, 0% | 148-151 MiB |
| tanstack-start | 300/s, 1.4 ms / 37 ms, 49% | 705, 80 ms / 150 ms | 716, 446 ms, 0% | 168-169 MiB |
| react-router | 300/s, 1.8 ms / 46 ms, 60% | 550, 107 ms / 180 ms | 545, 534 ms, 0% | 143-145 MiB |
| rails | 300/s, 1.9 ms / 42 ms, 65% | 537, 119 ms / 137 ms | 478, 597 ms, 0% | 136-141 MiB |
| django | 300/s, 2.7 ms / 12 ms, 85% | 346, 185 ms / 203 ms | 342, 792 ms, 0% | 344-345 MiB |
| nuxt | 291/s, 717 ms / 21.7 s, 100% | 325, 196 ms / 244 ms | 304, 3.6 s, 0% | 132-136 MiB |
| nextjs | **failed**: 151/s served, 1.3 s / 57.2 s, 100% | 186, 362 ms / 413 ms | 184, 6.7 s, 0% | 214-224 MiB |
| laravel | **failed**: never answered /items/item_1 within a minute | - | - | - |

### The item as JSON, items in memory

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.2 ms / 0.4 ms, 4% | 27,625, 1.7 ms / 11 ms | 23,081, 37 ms, 0% | 20-25 MiB |
| leptos | 300/s, 0.2 ms / 0.4 ms, 4% | 21,360, 2.5 ms / 11 ms | 20,524, 38 ms, 0% | 15-18 MiB |
| aspnet | 300/s, 0.3 ms / 0.8 ms, 11% | 14,541, 4.1 ms / 11 ms | 12,703, 43 ms, 0% | 78-81 MiB |
| spring | 300/s, 0.3 ms / 1.2 ms, 14% | 7,970, 5.9 ms / 67 ms | 7,890, 324 ms, 0% | 353-376 MiB |
| nuxt | 300/s, 0.4 ms / 0.6 ms, 10% | 7,285, 7.6 ms / 35 ms | 6,378, 100 ms, 0% | 100-102 MiB |
| tanstack-start | 300/s, 0.5 ms / 4.1 ms, 13% | 3,691, 14 ms / 46 ms | 3,585, 116 ms, 0% | 140-142 MiB |
| sveltekit | 300/s, 0.4 ms / 3.7 ms, 13% | 3,411, 14 ms / 48 ms | 3,310, 134 ms, 0% | 122-125 MiB |
| react-router | 300/s, 0.6 ms / 2.1 ms, 18% | 2,534, 23 ms / 73 ms | 2,452, 177 ms, 0% | 117-120 MiB |
| phoenix | 300/s, 0.6 ms / 0.9 ms, 23% | 2,500, 25 ms / 37 ms | 2,445, 3.5 s, 0% | 293-326 MiB |
| astro | 300/s, 0.7 ms / 23 ms, 22% | 1,962, 28 ms / 80 ms | 1,963, 188 ms, 0% | 136-138 MiB |
| django | 300/s, 0.7 ms / 1.0 ms, 26% | 1,356, 45 ms / 60 ms | 1,332, 230 ms, 0% | 297-297 MiB |
| nextjs | 300/s, 0.9 ms / 34 ms, 31% | 1,140, 51 ms / 115 ms | 1,137, 285 ms, 0% | 173-174 MiB |
| rails | 300/s, 1.1 ms / 5.8 ms, 34% | 1,065, 59 ms / 77 ms | 1,051, 294 ms, 0% | 129-130 MiB |
| laravel | **failed**: never answered /api/items/item_1 within a minute | - | - | - |

### Item page, item fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.8 ms / 1.1 ms, 7% | 3,615, 16 ms / 25 ms | 3,506, 118 ms, 0% | 18-61 MiB |
| aspnet | 150/s, 1.7 ms / 8.3 ms, 35% | 1,171, 54 ms / 80 ms | 1,029, 290 ms, 0% | 72-99 MiB |
| leptos | 150/s, 1.8 ms / 2.2 ms, 22% | 787, 83 ms / 97 ms | 665, 508 ms, 0% | 15-55 MiB |
| spring | 150/s, 2.6 ms / 10 ms, 56% | 484, 130 ms / 237 ms | 403, 1.1 s, 0% | 425-456 MiB |
| astro | 150/s, 2.2 ms / 38 ms, 45% | 450, 146 ms / 175 ms | 437, 687 ms, 0% | 128-147 MiB |
| phoenix | 150/s, 2.9 ms / 3.8 ms, 45% | 406, 157 ms / 207 ms | 408, 5.5 s, 0% | 212-335 MiB |
| sveltekit | 150/s, 2.4 ms / 25 ms, 44% | 392, 164 ms / 195 ms | 371, 761 ms, 0% | 143-158 MiB |
| rails | 150/s, 3.2 ms / 30 ms, 51% | 387, 165 ms / 184 ms | 332, 829 ms, 0% | 107-110 MiB |
| tanstack-start | 150/s, 2.7 ms / 34 ms, 51% | 325, 201 ms / 233 ms | 304, 2.9 s, 0% | 135-167 MiB |
| react-router | 150/s, 3.3 ms / 91 ms, 60% | 283, 230 ms / 260 ms | 295, 996 ms, 0% | 137-154 MiB |
| nuxt | 150/s, 6.5 ms / 331 ms, 78% | 203, 312 ms / 365 ms | 197, 15.7 s, 0% | 132-189 MiB |
| django | 150/s, 5.0 ms / 9.2 ms, 76% | 183, 348 ms / 387 ms | 185, 1.4 s, 0% | 127-128 MiB |
| nextjs | **failed**: 79/s served, 2.6 s / 52.6 s, 100% | 112, 549 ms / 866 ms | 105, 24.9 s, 0% | 207-251 MiB |
| laravel | **failed**: 103/s served, 7.5 s / 9.8 s, 99% | 102, 624 ms / 732 ms | 101, 2.7 s, 0% | 117-125 MiB |

### The item as JSON, fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.8 ms / 1.1 ms, 7% | 3,753, 17 ms / 24 ms | 3,628, 107 ms, 0% | 17-61 MiB |
| aspnet | 150/s, 1.2 ms / 5.7 ms, 23% | 1,695, 37 ms / 55 ms | 1,570, 208 ms, 0% | 59-86 MiB |
| leptos | 150/s, 1.4 ms / 1.8 ms, 15% | 1,194, 54 ms / 62 ms | 1,149, 300 ms, 0% | 16-56 MiB |
| spring | 150/s, 1.7 ms / 6.0 ms, 33% | 846, 71 ms / 156 ms | 732, 631 ms, 0% | 413-448 MiB |
| nuxt | 150/s, 1.5 ms / 17 ms, 27% | 799, 81 ms / 123 ms | 695, 516 ms, 0% | 121-142 MiB |
| sveltekit | 150/s, 1.5 ms / 16 ms, 28% | 695, 96 ms / 118 ms | 619, 509 ms, 0% | 122-145 MiB |
| tanstack-start | 150/s, 1.6 ms / 18 ms, 29% | 690, 98 ms / 122 ms | 601, 502 ms, 0% | 120-145 MiB |
| react-router | 150/s, 1.9 ms / 28 ms, 36% | 572, 115 ms / 139 ms | 504, 573 ms, 0% | 121-138 MiB |
| astro | 150/s, 1.9 ms / 30 ms, 38% | 554, 119 ms / 141 ms | 497, 578 ms, 0% | 125-142 MiB |
| rails | 150/s, 2.3 ms / 22 ms, 37% | 470, 128 ms / 177 ms | 464, 619 ms, 0% | 83-88 MiB |
| phoenix | 150/s, 2.6 ms / 3.3 ms, 40% | 454, 140 ms / 179 ms | 456, 6.7 s, 0% | 226-311 MiB |
| nextjs | 150/s, 2.3 ms / 56 ms, 44% | 355, 191 ms / 218 ms | 330, 1.5 s, 0% | 150-186 MiB |
| django | 150/s, 3.0 ms / 3.7 ms, 45% | 337, 190 ms / 208 ms | 330, 828 ms, 0% | 77-78 MiB |
| laravel | 150/s, 4.1 ms / 10 ms, 58% | 262, 220 ms / 464 ms | 253, 1.8 s, 0% | 102-117 MiB |

### Instructions per request

| App | Item page: user / kernel | JSON: user / kernel |
|---|---|---|
| **corten** | 95,038 / 33,192 | 69,444 / 34,117 |
| aspnet | 1,027,009 / 60,415 | 268,061 / 60,771 |
| astro | 2,062,613 / 80,833 | 1,087,272 / 77,083 |
| phoenix | 2,142,108 / 91,072 | 1,549,527 / 79,845 |
| leptos | 2,731,557 / 35,143 | 203,756 / 33,018 |
| sveltekit | 4,465,666 / 45,179 | 686,953 / 33,814 |
| rails | 5,009,173 / 328,362 | 2,275,040 / 283,616 |
| tanstack-start | 5,315,131 / 49,850 | 512,569 / 33,377 |
| spring | 5,817,331 / 97,131 | 1,394,151 / 101,801 |
| react-router | 6,451,010 / 109,052 | 875,998 / 70,265 |
| nuxt | 9,877,929 / 67,335 | 421,756 / 32,435 |
| django | 12,616,229 / 348,975 | 1,885,395 / 171,430 |
| nextjs | 17,295,179 / 134,154 | 1,710,624 / 58,309 |
| laravel | **failed**: did not start | **failed**: did not start |

### Efficiency

What each request costs the machine, from the same runs as the tables above. Pages per
CPU-second is requests a second at 64 users over the share of its one core the app used: work per
request, which does not depend on whether the load kept the core busy. CPU for 300 pages/s is the
fixed-rate phase, what a steady modest load costs. Pages/s per MiB is the 64-user rate over the
app's peak memory in that phase. Instruction counts (above) are the steadiest form of the same
cost; these also include allocation, waiting and the kernel.

| App | Pages per CPU-second: memory / item API | JSON per CPU-second: memory / item API | CPU for 300 pages/s from memory | Pages/s per MiB, from memory | Peak memory, 64 users |
|---|---|---|---|---|---|
| **corten** | 25,124 / 3,544 | 31,572 / 3,676 | 4.1% | 1,110 | 21.1 MiB |
| aspnet | 4,285 / 1,162 | 14,512 / 1,680 | 17% | 50 | 86.2 MiB |
| phoenix | 1,608 / 404 | 2,480 / 452 | 33% | 5.7 | 280.9 MiB |
| leptos | 1,478 / 783 | 22,044 / 1,181 | 23% | 91 | 16.4 MiB |
| astro | 1,253 / 449 | 1,941 / 552 | 31% | 9.4 | 134.1 MiB |
| spring | 1,109 / 481 | 7,907 / 839 | 40% | 3.1 | 356.3 MiB |
| sveltekit | 979 / 390 | 3,371 / 691 | 36% | 6.7 | 148 MiB |
| tanstack-start | 701 / 323 | 3,636 / 687 | 49% | 4.2 | 168.1 MiB |
| react-router | 548 / 282 | 2,504 / 568 | 60% | 3.8 | 143.6 MiB |
| rails | 531 / 383 | 1,045 / 466 | 65% | 3.8 | 142 MiB |
| django | 344 / 182 | 1,342 / 336 | 85% | 1.0 | 344.3 MiB |
| nuxt | 324 / 202 | 7,256 / 796 | 100%, 291/s served | 2.5 | 131.8 MiB |
| nextjs | failed (185) / failed (111) | 1,129 / 354 | failed (100%, 151/s served) | failed (0.9) | 214.3 MiB |
| laravel | **failed** / failed (102) | **failed** / 263 | **failed** | **failed** | **failed** |

Corten renders a page from memory with 40 µs of CPU (25,124 per CPU-second), 5.9x ASP.NET Core
(233 µs) and 17x Leptos, and its JSON with 32 µs, against Leptos's 45 and ASP.NET Core's 69. At
300 pages a second it used 4.1% of its core, ASP.NET Core 17% and Leptos 23%; that is 7,317 pages
per CPU-second, under a third of its rate at 64 users, since at a low rate each request also pays
for waking the server, and a once-a-second sample is coarse at 4%. It serves 1,110 pages a second
per MiB: 12x Leptos (91), which peaks lower (16.4 MiB against 21.1) but serves a sixteenth of
the pages, and 22x ASP.NET Core (50). From the item API, fetching and parsing the 55 KB item
costs Corten about 240 µs a page, six times the page itself
([#166 Corten's item-API path spends six times as long fetching and parsing the item as rendering it, and the single-process item API could soon cap it](https://github.com/todd-skelton/corten/issues/166)).

In r6 Corten's core was 93% busy at 64 users for the page from memory and 88% for the JSON, the
only app below 97% (Leptos's JSON was at 96.9%): four load-generator cores could not keep it
busy, so its requests a second from memory are below what it can serve, while its requests per
CPU-second are not affected. From r7 the load generator also gets the item API's two cores, which
are idle with items in memory, and each phase's CPU time is read from the container's cgroup
counter rather than from once-a-second samples (`cpu-time.mjs`; `cpuCounted` in
`summary.json`).

### Bytes the item page loads (raw / gzip, KB)

| App | HTML | JavaScript (files) | WASM | CSS |
|---|---|---|---|---|
| astro | 9.3 / 2.6 | 24.9 / 9.2 (3) | 0 | 0.0 / 0.0 |
| **corten** | 11.2 / 2.3 | 24.9 / 9.6 (4) | 0 | 4.6 / 1.6 |
| django | 7.3 / 1.8 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| aspnet | 7.3 / 1.9 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| spring | 7.2 / 1.8 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| sveltekit | 16.3 / 3.6 | 96.3 / 37.0 (11) | 0 | 1.9 / 0.8 |
| rails | 8.5 / 2.2 | 152.2 / 40.6 (7) | 0 | 2.3 / 0.9 |
| phoenix | 10.0 / 2.4 | 149.2 / 46.2 (1) | 0 | 2.3 / 0.9 |
| nuxt | 17.3 / 4.5 | 197.3 / 73.7 (4) | 0 | 1.9 / 0.8 |
| laravel | 10.8 / 2.8 | 255.8 / 84.3 (1) | 0 | 2.3 / 0.9 |
| react-router | 19.5 / 4.9 | 309.7 / 98.6 (4) | 0 | 1.9 / 0.8 |
| tanstack-start | 17.4 / 4.3 | 342.6 / 110.9 (5) | 0 | 1.9 / 0.8 |
| nextjs | 25.3 / 4.8 | 456.2 / 136.1 (7) | 0 | 1.9 / 0.8 |
| leptos | 17.8 / 3.5 | 24.8 / 7.0 (1) | 719.9 / 286.7 | 2.0 / 0.8 |

### Browser, median of 10 (p90)

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| django | 45 (48) ms | 68 ms | 0 ms | 76 ms | 21 (29) ms | 1.5 MB |
| aspnet | 45 (47) ms | 64 ms | 0 ms | 78 ms | 16 (19) ms | 1.5 MB |
| astro | 54 (56) ms | 60 ms | 0 ms | 76 ms | 18 (19) ms | 1.4 MB |
| rails | 57 (63) ms | 64 ms | 0 ms | 91 ms | 26 (33) ms | 1.9 MB |
| **corten** | 58 (62) ms | 64 ms | 0 ms | 75 ms | 13 (15) ms | 1.4 MB |
| spring | 61 (75) ms | 76 ms | 0 ms | 77 ms | 32 (61) ms | 1.5 MB |
| laravel | 71 (74) ms | 64 ms | 0 ms | 77 ms | 24 (24) ms | 2 MB |
| nuxt | 97 (99) ms | 72 ms | 0 ms | 73 ms | 24 (26) ms | 2.7 MB |
| sveltekit | 99 (102) ms | 76 ms | 0 ms | 74 ms | 21 (23) ms | 2.1 MB |
| phoenix | 99 (113) ms | 84 ms | 0 ms | 82 ms | 16 (17) ms | 2.1 MB |
| leptos | 107 (120) ms | 88 ms | 0 ms | 75 ms | 24 (24) ms | 1.7 MB |
| tanstack-start | 115 (119) ms | 68 ms | 0 ms | 73 ms | 28 (29) ms | 2.9 MB |
| nextjs | 148 (151) ms | 72 ms | 0 ms | 74 ms | 137 (163) ms | 3.2 MB |
| react-router | 159 (181) ms | 104 ms | 0 ms | 84 ms | 29 (33) ms | 2.5 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| aspnet | 427 (431) ms | 400 ms | 2 ms | 295 ms | 243 (247) ms | 1.5 MB |
| django | 437 (438) ms | 408 ms | 5 ms | 295 ms | 244 (248) ms | 1.5 MB |
| **corten** | 449 (454) ms | 444 ms | 0 ms | 286 ms | 197 (201) ms | 1.4 MB |
| spring | 608 (613) ms | 420 ms | 2 ms | 301 ms | 245 (247) ms | 1.5 MB |
| astro | 621 (626) ms | 272 ms | 0 ms | 313 ms | 242 (245) ms | 1.4 MB |
| sveltekit | 845 (851) ms | 464 ms | 4 ms | 285 ms | 270 (273) ms | 2.1 MB |
| laravel | 876 (882) ms | 416 ms | 21 ms | 333 ms | 217 (221) ms | 2 MB |
| phoenix | 930 (949) ms | 400 ms | 49 ms | 152 ms | 134 (136) ms | 2.1 MB |
| react-router | 1,055 (1,058) ms | 432 ms | 31 ms | 350 ms | 262 (266) ms | 2.6 MB |
| rails | 1,147 (1,153) ms | 416 ms | 2 ms | 316 ms | 253 (256) ms | 1.9 MB |
| nextjs | 1,398 (1,410) ms | 400 ms | 150 ms | 310 ms | 448 (455) ms | 3.1 MB |
| nuxt | 1,489 (1,497) ms | 432 ms | 59 ms | 287 ms | 280 (284) ms | 2.7 MB |
| leptos | 1,839 (1,844) ms | 416 ms | 72 ms | 301 ms | 253 (256) ms | 1.7 MB |
| tanstack-start | 2,323 (2,327) ms | 424 ms | 64 ms | 296 ms | 344 (349) ms | 2.9 MB |

### What the numbers say

1. Rendering from memory: Corten serves 23,416 item pages a second at 64 users, 5.4x ASP.NET
   Core (4,298), the next fastest, and 16x Leptos (1,487). A page costs it 95,038 user-mode
   instructions; ASP.NET Core 1.03 million, Leptos 2.7 million, Next.js 17 million.
2. JSON from memory: Corten leads with 27,625 a second, then Leptos (21,360) and ASP.NET Core
   (14,541). Corten's JSON costs 69,444 user-mode instructions, a third of Leptos's (203,756)
   and less than its own HTML page.
3. With the item API every request fetches and parses about 55 KB of JSON, and that dominates.
   Corten parses only the fields the page reads (see below) and serves 3,615 pages a second,
   3.1x ASP.NET Core (1,171) and 4.6x Leptos (787); the rest serve 183-484. As JSON from the item
   API: Corten 3,753, ASP.NET Core 1,695, Leptos 1,194. At 150 a second Corten's page used 7% of
   its core, ASP.NET Core's 35%.
4. Failed: Next.js could not serve 300 pages a second from memory (151, p99 57.2 s) or 150 from
   the item API (79). Laravel did not start for the in-memory runs: each Octane worker loads the
   508 items into PHP arrays, which PHP's default `memory_limit` (128 MB) does not allow; with the
   item API it served 103 of 150 pages a second and kept up with the JSON. On this faster CPU
   Nuxt passed the in-memory page test it failed in r4 and r5 (291 of 300, 1.5% never sent) at
   100% CPU and a median of 717 ms, and Django passed it at 85% CPU.
5. Cold start: Corten and Leptos answer 6.0 ms and 6.9 ms after their process starts and hold
   1.1 MiB and 1.5 MiB after the first request. The Node frameworks take 178-707 ms and
   24-51 MiB; Spring Boot 8.5 s and 147 MiB.
6. Browser: on desktop the server-rendered apps with small scripts (Django, ASP.NET Core, Astro,
   Rails, Corten, Spring) are interactive in 45-61 ms. On the throttled phone Corten is
   interactive in 449 ms, with the htmx apps (427-437 ms), against 1,398 ms for Next.js,
   1,839 ms for Leptos (WASM) and 2,323 ms for TanStack Start. Corten's client navigation is the
   fastest on desktop (13 ms); on the phone Phoenix's is (134 ms against 197 ms), over its
   already-open LiveView socket. Corten's item page loads 9.6 KB of JavaScript (gzip) in four
   files, against 7.3 KB in r5, now more than Astro's 9.2 KB.

r6 ran on a Platinum 8280, faster than r5's Platinum 8168, and every app but Corten was
unchanged: their 64-user requests a second rose by a median of 14% for the page from memory, 18%
for the JSON from memory, 17% for the item-API page and 27% for the item-API JSON. Corten's rose
less in each: 8% (21,735 to 23,416), 10% (25,200 to 27,625), 12% (3,240 to 3,615), and its
item-API JSON not at all (3,787 to 3,753); at 150 a second that route used 6.5% of its core, as
in r5 (6.6%), while ASP.NET Core's fell from 30% to 22.5%. Instruction counts, which do not
depend on the droplet, fell 8.5% for Corten's page (103,877 to 95,038) and held for its JSON
(69,291 to 69,444) and for Leptos's page (2,724,688 against 2,731,557). Counts compare across
rounds, and requests a second only within one.

### Browser pass after the many-rows change (round browser-3, 2026-10-23)

`droplet.mjs browser-3 --suite browser` reran parity, bytes and the browser timing for all
fourteen apps on a c-8 droplet (Xeon Platinum 8280), with Corten at 77002fe (the runtime that
finds ref names in many new rows with one query per name); the other apps are unchanged. Every
browser run worked and parity passed on 16 pages for every app. Corten's item page loads 6.9 KB of
JavaScript (gzip, four files; browser-1 6.8 KB). All rows are in
[results/browser-3](results/browser-3); the README's browser table came from this round until r5. A
selection, median (p90):

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| corten | 50 (60) ms | 60 ms | 0 ms | 75 ms | 12 (13) ms | 1.4 MB |
| aspnet | 42 (43) ms | 60 ms | 0 ms | 76 ms | 15 (16) ms | 1.5 MB |
| django | 43 (48) ms | 64 ms | 0 ms | 77 ms | 19 (22) ms | 1.5 MB |
| astro | 48 (50) ms | 56 ms | 0 ms | 76 ms | 17 (18) ms | 1.4 MB |
| nextjs | 138 (141) ms | 68 ms | 0 ms | 74 ms | 105 (124) ms | 3.2 MB |
| leptos | 91 (95) ms | 64 ms | 0 ms | 75 ms | 23 (24) ms | 1.7 MB |
| tanstack-start | 105 (113) ms | 64 ms | 0 ms | 71 ms | 26 (28) ms | 2.9 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| corten | 441 (449) ms | 444 ms | 0 ms | 269 ms | 191 (196) ms | 1.4 MB |
| aspnet | 428 (434) ms | 400 ms | 0 ms | 287 ms | 240 (243) ms | 1.5 MB |
| django | 435 (436) ms | 396 ms | 0 ms | 288 ms | 239 (243) ms | 1.5 MB |
| astro | 615 (618) ms | 256 ms | 0 ms | 298 ms | 242 (246) ms | 1.4 MB |
| nextjs | 1360 (1379) ms | 400 ms | 133 ms | 303 ms | 431 (440) ms | 3.1 MB |
| leptos | 1819 (1825) ms | 408 ms | 54 ms | 283 ms | 239 (245) ms | 1.7 MB |
| tanstack-start | 2289 (2308) ms | 408 ms | 52 ms | 289 ms | 336 (341) ms | 2.9 MB |

Every app's desktop interactive times are 4-22% lower than in browser-1 on the same CPU model,
Corten's included (50 ms against 54); phone times moved by up to 4%. One run each, so the
difference between droplets is about as large as anything the change did to this page, which has
no list a script fills.

### Browser pass on the views runtime (round browser-1, 2026-10-23)

`droplet.mjs browser-1 --suite browser` reran parity, bytes and the browser timing for all
fourteen apps on a fresh c-8 droplet (Xeon Platinum 8280), with Corten at the commit that loads
handler modules as module scripts and serves views; the other apps are unchanged from r3. Every
browser run worked. Corten's item page now loads 6.8 KB of
JavaScript (gzip, four files) against 6.2 KB in r3; the rest are within 0.1 KB of r3. All rows
are in [results/browser-1](results/browser-1); the README's browser table came from this round until browser-3.
A selection, median (p90):

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| corten | 54 (63) ms | 64 ms | 0 ms | 75 ms | 13 (13) ms | 1.4 MB |
| aspnet | 45 (52) ms | 72 ms | 0 ms | 77 ms | 16 (20) ms | 1.5 MB |
| django | 47 (51) ms | 68 ms | 0 ms | 76 ms | 21 (21) ms | 1.5 MB |
| astro | 52 (57) ms | 60 ms | 0 ms | 75 ms | 18 (20) ms | 1.4 MB |
| nextjs | 150 (160) ms | 72 ms | 0 ms | 85 ms | 89 (144) ms | 3.2 MB |
| leptos | 109 (127) ms | 100 ms | 0 ms | 74 ms | 25 (25) ms | 1.7 MB |
| tanstack-start | 118 (123) ms | 68 ms | 0 ms | 84 ms | 28 (30) ms | 2.9 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| corten | 454 (458) ms | 452 ms | 6 ms | 290 ms | 200 (202) ms | 1.4 MB |
| aspnet | 430 (436) ms | 404 ms | 4 ms | 301 ms | 245 (248) ms | 1.5 MB |
| django | 439 (443) ms | 408 ms | 3 ms | 302 ms | 244 (252) ms | 1.5 MB |
| astro | 604 (622) ms | 264 ms | 0 ms | 326 ms | 244 (246) ms | 1.4 MB |
| nextjs | 1413 (1443) ms | 404 ms | 169 ms | 312 ms | 453 (462) ms | 3.1 MB |
| leptos | 1848 (1858) ms | 416 ms | 89 ms | 307 ms | 254 (258) ms | 1.7 MB |
| tanstack-start | 2332 (2346) ms | 420 ms | 69 ms | 302 ms | 351 (357) ms | 2.9 MB |

Against r3, which ran on the same CPU model (r4 ran on a Xeon Platinum 8168), one run each:
Corten's desktop interactive 54 ms (r3: 58), phone 454 ms (r3: 442), client navigation 13 and
200 ms (r3: 13 and 194). Parity passed on 16 pages for every app.

### Corten's item API path

Corten's item-API page fell from 1,290 a second in r2 to 1,175 in r3 as the unchanged apps fell
5-22% (ASP.NET Core 1,278 to 1,209, Leptos 915 to 812): r3's droplet, not Corten. What remained
was Corten's own cost. Every request parsed the 55 KB response into a `serde_json::Value` and
then read the view from it, 3.47 million instructions an item in process (callgrind), against
about 124,000 to render the page. ASP.NET Core deserializes into typed records and skips the
members it does not use. `examples/card-market` now deserializes into `ApiItem`, structs of only
the fields the page reads (1.08 million instructions an item), and the same views come out for
all 508 fixture items. Leptos's app, as it is written, still parses into a `Value`.

Before and after on one c-8 droplet (Xeon Platinum 8280), the build before (`1ee12d4`) and after
one at a time, alternating, two rounds, items from the item API (`results/ab-typed-parse`):

| | Before | After | ASP.NET Core, same droplet |
|---|---|---|---|
| Page, 64 users: req/s | 1,249-1,270 | 3,128-3,176 | 1,344-1,373 |
| Page, 150/s: CPU, p99 | 13.3%, 1.6-1.7 ms | 6.6-6.7%, 1.1-1.3 ms | 25-27%, 6.1-6.2 ms |
| JSON, 64 users: req/s | 1,337-1,359 | 3,349-3,372 | |
| Page, user instructions a request | 4,395,372 | 1,385,381 | 5.4-9.2 million (JIT and GC vary) |
| JSON, user instructions a request | 4,387,294 | 1,398,989 | |

Instructions are `count.mjs ... api`, median of 3 x 5,000 requests. r6, on a
Platinum 8280 like this A/B's, serves 3,615 item-API pages a second against ASP.NET Core's 1,171
(3.1x; 2.3x in the A/B and in r4, 3.2x in r5), at 7% of its core at 150 a second against 35%.

### Corten before and after its type and performance pass (Docker Desktop, 2026-10-21)

`ab.mjs` runs the image Corten's first r1 rows were measured with (`cb-corten`) and the current build
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

## Method

1. Machine: a DigitalOcean CPU-optimized droplet (`c-8`: 8 dedicated vCPUs, Intel Xeon Platinum
   8280 at 2.7 GHz in r6, 16 GB), Ubuntu 24.04, kernel 6.8, Docker 29.9, Node 24, created for the run
   and destroyed after it (`droplet.mjs`). Droplets of one size are not all the same CPU (r1's,
   r4's and r5's were a Platinum 8168, r2's, r3's and r6's a Platinum 8280), so times are compared within a
   round, never across rounds.
2. Cores: the app under test pinned to core 1 with 1 GiB of memory, the item API to cores 2-3,
   the load generator (k6) to cores 4-7, the system and Docker on core 0. One app runs at a time.
   From r7, with items in memory, k6 also gets cores 2-3, which the item API does not use then.
3. Cold start (`cold-start.mjs`): the container runs a shell that writes the time and execs the
   server; a probe in the same network namespace polls for the first 200 of an item page (items
   from the item API). Docker's own container setup is not in the number. 10 runs each.
4. Server rendering (`run-ssr.mjs`): a 30 s warmup with 32 users (long enough for the JVM's and
   .NET's JITs), then 60 s at a fixed rate (300/s from memory, 150/s from the API), 60 s with 64
   closed-loop users, and 60 s with 256. Items 1 to 500, skewed toward low ids. Responses
   uncompressed. CPU and memory are sampled from `docker stats` each second; from r7 each
   phase's CPU time is also read from the container's cgroup counter before and after it
   (`cpu-time.mjs`, `<app>-<phase>-cpu.json`).
   - Items in memory: a 27.7 MB fixture of 508 items loaded at start, so only the framework is
     measured.
   - Item API: every request fetches the item (about 55 KB of JSON) over HTTP from
     `item-api/server.mjs`, which serves the fixture's responses the way the production API does
     with a warm cache, and parses it. From r7 it runs one Node process per core it has (two), so
     it can serve about twice what r6's single process could; its CPU next to each item-API phase
     (`apiCpuPct`) is a percentage of one core, up to 200.
   - JSON: `GET /api/items/{id}` answers the view model the page renders (about 9 KB), written by
     the framework's usual JSON path. Same lookup as the page; only the output differs.
5. Instructions per request (`count.mjs`): `perf stat` counts the app container's user and
   kernel instructions while 4 users make 5,000 requests (items in memory) after a 30 s warmup;
   median of 3. A count does not depend on what else the machine is doing, so it is steadier than
   time; it does not see waiting (I/O, locks) or how well the instructions run.
   `node count.mjs <round> 3 <apps> page,json api` counts with items from the item API instead,
   so the count includes fetching and parsing each 55 KB response (keys `<app>/<target>/api`).
6. Browser (`browser.mjs`): every app running at once on the app core, items from the item API,
   one app measured at a time; Playwright
   Chromium, cache disabled, 10 runs per app and profile, each cold load starting on a page
   from another origin. Desktop is unthrottled; mobile is 4x CPU slowdown, 150 ms RTT, 1.6 Mbps down.
   "Interactive" is when the page's own code marks itself ready (hydration finished, islands
   connected, or the framework's load event). Action is timed only on an item with listings (an
   alert on one with none is refused by an app that checks it, Corten since round r7, and set by
   others, so the work differs; [#259 Benchmark round r7 has no browser numbers: Corten refuses the price alert on item_12345, which has no listings, and browser.mjs times out waiting for 'Alert set'](https://github.com/todd-skelton/corten/issues/259)). Action is fill, submit, and wait for the
   confirmation. Navigation is home page to item page.
7. Bytes (`bytes.mjs`): every script, WASM and stylesheet the item page loads, fetched raw and
   gzipped here at level 6, so server compression settings do not change the comparison.
8. Efficiency (`summarize.mjs`): requests per CPU-second are a phase's requests over the app's
   CPU time in it (the cgroup counter; before r7, the 64-user rate over the sampled average),
   requests a second per MiB are the 64-user rate over the phase's peak memory, and the item API's
   sampled CPU is kept next to each item-API phase (`apiCpuPct`).
9. k6 runs with every local port and reuses ports in TIME_WAIT: gunicorn's sync workers close
   each connection, and at a few thousand requests a second k6 otherwise ran out of ports
   (r2's first attempt stopped there).

## Caveats

1. One run per phase on one droplet. Differences under about 10% are noise.
2. The data is synthetic (generated catalog items), sized like a production marketplace's item
   pages. The item API is a stand-in that serves the same responses as the production API (a
   Rust service with Postgres and Redis, not in this repository).
3. JavaScript dependencies are pinned in each `package.json` without lockfiles, so transitive
   versions can drift between builds; the other apps have lockfiles.
4. The page is small. Apps with large client-side state (a seller grid of live rows) are a
   different test; see [bench/live](../live/README.md) for pushing updates.
5. Each framework runs its production defaults with the worker count its docs give for one CPU,
   in one CPU and 1 GiB. Nothing is tuned to get an app through a test: one that cannot start or
   cannot serve the fixed rate (more than 5% of it never sent, or more than 1% errors) is marked
   failed. Each could be tuned further (caching, streaming, more workers). None of the apps cache
   rendered pages.
6. The images include the live benchmark's routes (`/live/...`), which the item page does not
   touch.
7. Laravel ran with PHP's default `memory_limit` throughout r3 to r6. (r2's main run had raised it
   to 512 MB and measured Laravel again with the default afterwards; `results/r2/laravel-rerun.log`.)
8. r4's droplet had a slower CPU than r3's (Platinum 8168 against 8280). Django and Nuxt, which
   passed r3's in-memory page test near their limit, failed it on r4's; their settings did not
   change. r3's numbers stay in `results/r3/`. In r5, on the same CPU model as r4, Django passed
   it again (4.1% never sent) and Nuxt did not; in r6, on a Platinum 8280, both passed.
9. r6 measured the tree at `cf8ac0f` (`results/r6/commit.txt`). Changes merged to `main` after
   that are not in these numbers.
10. r6's efficiency numbers divide by CPU sampled once a second, which is coarse for an app using
    a few percent of its core (Corten's 4.1% at the fixed rate); from r7 they divide by the
    counted CPU time. Requests per CPU-second, like requests a second, compare only within a round.

## Running it

```sh
# On a fresh droplet, created and destroyed for the run (needs doctl and an SSH key there):
node bench/fullstack/droplet.mjs r7 --ssh-key <id> [--region sfo2]
# Or by hand, from the repository root: the images, the fixture, then everything.
docker build -f examples/card-market/Dockerfile -t cb-corten .
for app in nextjs tanstack-start sveltekit react-router leptos django rails laravel aspnet spring phoenix astro nuxt; do
  docker build -t cb-$app bench/fullstack/apps/$app
done
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400   # or any {"item_1": {...}} file
cd bench/fullstack && node run-all.mjs r7 10
node check-app.mjs <app>    # before adding an app: parity, JSON and 404s against corten
```
