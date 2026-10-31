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

All numbers are measured, from one run (`results/r8/`, 2026-10-30 UTC, the tree at `2976f15`) on one
DigitalOcean c-8 droplet (Intel Xeon Platinum 8168); see [Method](#method). Failed means the app could not do the
test with its usual settings in one CPU and 1 GiB (see caveat 5); its other numbers are kept
where it produced any. r8 has no instruction counts, so that table is r7's (caveat 11). r7
(`results/r7/`, a Platinum 8280, Corten at `a9b1163`, 469 commits before r8's, with no browser
pass), r6 (`results/r6/`, a Platinum 8280, Corten at `cf8ac0f`, with CPU sampled once a second
rather than counted and a one-process item API), r5 (`results/r5/`, a Platinum 8168, Corten at `f80d4a1`, before the
read-again, place-keeping and catalog-formatting runtime that r6's item page loads), r4 (`results/r4/`, the same CPU model, Corten at `05fcb74`, before the
JSON derives, the views runtime and the cheaper escape and route parameters), r3 (`results/r3/`, a Platinum 8280, Corten before it parsed the item API's
response into typed structs), r2 (`results/r2/`, a Platinum 8280, Corten before handlers replaced
islands and before `corten::Json`) and r1 (`results/r1/`, six frameworks, a Platinum 8168 and
20 s warmups) are kept for the record; times are compared within a round, not across rounds.

### Cold start

| App | Process start to first 200, median (min-max) of 10 | Memory after first request | Image |
|---|---|---|---|
| **corten** | 6.7 ms (5.9 ms-36 ms) | 1.23 MiB | 123 MB |
| leptos | 7.8 ms (6.7 ms-10 ms) | 1.473 MiB | 127 MB |
| sveltekit | 188 ms (177 ms-246 ms) | 23.73 MiB | 333 MB |
| tanstack-start | 217 ms (210 ms-230 ms) | 26.38 MiB | 332 MB |
| react-router | 315 ms (304 ms-334 ms) | 31.79 MiB | 353 MB |
| nuxt | 322 ms (318 ms-333 ms) | 35.55 MiB | 335 MB |
| astro | 348 ms (323 ms-427 ms) | 36.11 MiB | 528 MB |
| nextjs | 719 ms (692 ms-1.0 s) | 51.55 MiB | 385 MB |
| aspnet | 752 ms (719 ms-834 ms) | 30.56 MiB | 342 MB |
| laravel | 908 ms (870 ms-993 ms) | 95.47 MiB | 947 MB |
| django | 1.1 s (1.1 s-1.2 s) | 63.1 MiB | 276 MB |
| rails | 1.2 s (1.2 s-1.3 s) | 63.9 MiB | 431 MB |
| phoenix | 1.4 s (1.4 s-1.5 s) | 125.6 MiB | 262 MB |
| spring | 8.2 s (8.0 s-8.3 s) | 142.2 MiB | 516 MB |

### Item page, items in memory (no database)

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.3 ms / 0.4 ms, 5% | 15,747, 4.4 ms / 8.2 ms | 14,553, 33 ms, 0% | 16-19 MiB |
| aspnet | 300/s, 0.6 ms / 2.4 ms, 25% | 4,182, 15 ms / 24 ms | 4,092, 103 ms, 0% | 85-90 MiB |
| phoenix | 300/s, 1.0 ms / 1.4 ms, 36% | 1,420, 45 ms / 59 ms | 1,357, 5.0 s, 0% | 281-298 MiB |
| leptos | 300/s, 0.9 ms / 1.2 ms, 24% | 1,408, 46 ms / 67 ms | 1,233, 350 ms, 0% | 15-33 MiB |
| astro | 300/s, 0.9 ms / 31 ms, 30% | 1,222, 47 ms / 116 ms | 1,166, 293 ms, 0% | 137-141 MiB |
| spring | 300/s, 1.0 ms / 4.6 ms, 45% | 1,136, 40 ms / 339 ms | 1,099, 1.2 s, 0% | 356-377 MiB |
| sveltekit | 300/s, 1.1 ms / 21 ms, 40% | 920, 61 ms / 132 ms | 809, 411 ms, 0% | 143-149 MiB |
| tanstack-start | 300/s, 1.4 ms / 34 ms, 49% | 683, 87 ms / 147 ms | 689, 457 ms, 0% | 162-163 MiB |
| react-router | 300/s, 1.8 ms / 45 ms, 61% | 542, 110 ms / 182 ms | 535, 567 ms, 0% | 142-143 MiB |
| rails | 300/s, 2.2 ms / 49 ms, 72% | 457, 139 ms / 176 ms | 426, 703 ms, 0% | 135-140 MiB |
| django | 300/s, 11 ms / 233 ms, 93% | 335, 187 ms / 235 ms | 306, 983 ms, 0% | 344-344 MiB |
| nuxt | 290/s, 708 ms / 25.3 s, 100% | 329, 192 ms / 243 ms | 308, 870 ms, 0% | 161-162 MiB |
| nextjs | **failed**: 158/s served, 1.3 s / 57.2 s, 89% | 186, 358 ms / 410 ms | 188, 10.5 s, 0% | 219-221 MiB |
| laravel | **failed**: never answered /items/item_1 within a minute | - | - | - |

### The item as JSON, items in memory

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.3 ms / 0.4 ms, 4% | 24,652, 2.0 ms / 8.1 ms | 23,937, 33 ms, 0% | 16-19 MiB |
| leptos | 300/s, 0.3 ms / 0.4 ms, 5% | 13,758, 5.3 ms / 8.3 ms | 12,843, 34 ms, 0% | 15-18 MiB |
| aspnet | 300/s, 0.3 ms / 0.5 ms, 15% | 12,347, 4.8 ms / 9.5 ms | 12,024, 36 ms, 0% | 78-82 MiB |
| nuxt | 300/s, 0.4 ms / 0.6 ms, 10% | 6,693, 8.0 ms / 33 ms | 6,162, 87 ms, 0% | 95-97 MiB |
| spring | 300/s, 0.3 ms / 1.2 ms, 21% | 6,620, 6.2 ms / 89 ms | 6,348, 216 ms, 0% | 343-367 MiB |
| tanstack-start | 300/s, 0.5 ms / 4.1 ms, 13% | 3,625, 15 ms / 47 ms | 3,308, 122 ms, 0% | 141-144 MiB |
| sveltekit | 300/s, 0.5 ms / 3.7 ms, 13% | 3,223, 18 ms / 49 ms | 3,184, 137 ms, 0% | 123-126 MiB |
| react-router | 300/s, 0.7 ms / 1.8 ms, 18% | 2,351, 24 ms / 76 ms | 2,247, 189 ms, 0% | 118-119 MiB |
| phoenix | 300/s, 0.7 ms / 1.0 ms, 27% | 2,127, 29 ms / 47 ms | 2,019, 4.0 s, 0% | 300-325 MiB |
| astro | 300/s, 0.7 ms / 23 ms, 22% | 1,971, 28 ms / 78 ms | 1,877, 190 ms, 0% | 137-138 MiB |
| django | 300/s, 0.7 ms / 1.2 ms, 28% | 1,193, 50 ms / 74 ms | 1,186, 276 ms, 0% | 294-294 MiB |
| nextjs | 300/s, 0.9 ms / 30 ms, 31% | 1,141, 51 ms / 118 ms | 1,102, 307 ms, 0% | 166-168 MiB |
| rails | 300/s, 1.2 ms / 6.5 ms, 38% | 934, 68 ms / 95 ms | 927, 354 ms, 0% | 130-131 MiB |
| laravel | **failed**: never answered /api/items/item_1 within a minute | - | - | - |

### Item page, item fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.7 ms / 1.1 ms, 6% | 3,616, 18 ms / 24 ms | 3,219, 122 ms, 0% | 16-58 MiB |
| aspnet | 150/s, 1.8 ms / 8.4 ms, 39% | 1,065, 58 ms / 96 ms | 973, 344 ms, 0% | 71-100 MiB |
| leptos | 150/s, 1.7 ms / 2.4 ms, 21% | 773, 85 ms / 100 ms | 651, 520 ms, 0% | 15-57 MiB |
| spring | 150/s, 2.3 ms / 8.3 ms, 57% | 515, 121 ms / 232 ms | 415, 1.2 s, 0% | 433-463 MiB |
| astro | 150/s, 2.1 ms / 37 ms, 44% | 462, 142 ms / 177 ms | 426, 748 ms, 0% | 125-147 MiB |
| phoenix | 150/s, 3.1 ms / 5.0 ms, 50% | 373, 171 ms / 225 ms | 380, 787 ms, 0% | 230-345 MiB |
| sveltekit | 150/s, 2.2 ms / 23 ms, 42% | 364, 175 ms / 214 ms | 357, 852 ms, 0% | 147-159 MiB |
| rails | 150/s, 3.4 ms / 34 ms, 56% | 334, 189 ms / 238 ms | 297, 1.0 s, 0% | 107-113 MiB |
| tanstack-start | 150/s, 2.6 ms / 35 ms, 50% | 327, 198 ms / 235 ms | 305, 973 ms, 0% | 137-173 MiB |
| react-router | 150/s, 3.1 ms / 68 ms, 57% | 290, 223 ms / 267 ms | 294, 2.3 s, 0% | 137-149 MiB |
| nuxt | 150/s, 4.6 ms / 275 ms, 74% | 211, 301 ms / 383 ms | 204, 14.5 s, 0% | 132-190 MiB |
| django | 150/s, 5.1 ms / 25 ms, 79% | 181, 351 ms / 395 ms | 179, 1.5 s, 0% | 127-128 MiB |
| nextjs | **failed**: 80/s served, 2.5 s / 52.1 s, 77% | 119, 514 ms / 812 ms | 110, 23.8 s, 0% | 262-266 MiB |
| laravel | **failed**: 92/s served, 9.7 s / 11.0 s, 98% | 97, 660 ms / 736 ms | 95, 2.8 s, 0% | 117-126 MiB |

### The item as JSON, fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.7 ms / 0.9 ms, 6% | 3,852, 18 ms / 23 ms | 3,429, 110 ms, 0% | 16-58 MiB |
| aspnet | 150/s, 1.4 ms / 6.6 ms, 30% | 1,483, 41 ms / 69 ms | 1,421, 247 ms, 0% | 60-87 MiB |
| leptos | 150/s, 1.3 ms / 1.9 ms, 15% | 1,075, 58 ms / 79 ms | 1,010, 482 ms, 0% | 15-59 MiB |
| spring | 150/s, 1.5 ms / 5.6 ms, 41% | 912, 65 ms / 147 ms | 723, 688 ms, 0% | 417-445 MiB |
| nuxt | 150/s, 1.4 ms / 16 ms, 26% | 772, 84 ms / 128 ms | 693, 535 ms, 0% | 126-137 MiB |
| sveltekit | 150/s, 1.4 ms / 15 ms, 26% | 716, 91 ms / 120 ms | 619, 500 ms, 0% | 124-143 MiB |
| tanstack-start | 150/s, 1.5 ms / 18 ms, 28% | 676, 98 ms / 126 ms | 589, 548 ms, 0% | 122-145 MiB |
| react-router | 150/s, 1.7 ms / 27 ms, 35% | 554, 117 ms / 143 ms | 519, 608 ms, 0% | 121-138 MiB |
| astro | 150/s, 1.8 ms / 30 ms, 36% | 547, 120 ms / 148 ms | 510, 629 ms, 0% | 123-140 MiB |
| phoenix | 150/s, 2.8 ms / 4.7 ms, 46% | 421, 151 ms / 203 ms | 421, 7.2 s, 0% | 232-312 MiB |
| rails | 150/s, 2.4 ms / 23 ms, 39% | 405, 157 ms / 215 ms | 390, 785 ms, 0% | 83-86 MiB |
| nextjs | 150/s, 2.2 ms / 62 ms, 44% | 354, 191 ms / 227 ms | 334, 1.5 s, 0% | 156-195 MiB |
| django | 150/s, 2.9 ms / 4.5 ms, 45% | 335, 190 ms / 224 ms | 336, 864 ms, 0% | 77-77 MiB |
| laravel | 150/s, 4.3 ms / 14 ms, 60% | 242, 257 ms / 463 ms | 241, 1.6 s, 0% | 100-117 MiB |

### Instructions per request

From round r7 (2026-10-28, a Platinum 8280, the tree at `a9b1163`); r8 has none (caveat 11).

| App | Item page: user / kernel | JSON: user / kernel |
|---|---|---|
| **corten** | 95,949 / 33,294 | 69,572 / 33,319 |
| aspnet | 1,002,980 / 57,454 | 268,439 / 79,440 |
| astro | 2,056,722 / 80,608 | 1,077,906 / 76,507 |
| phoenix | 2,138,850 / 90,650 | 1,551,767 / 78,495 |
| leptos | 2,722,611 / 35,281 | 204,467 / 33,389 |
| sveltekit | 4,454,119 / 44,876 | 693,322 / 33,562 |
| rails | 5,013,326 / 323,636 | 2,282,059 / 284,679 |
| tanstack-start | 5,327,854 / 49,099 | 538,088 / 33,644 |
| spring | 5,896,061 / 100,755 | 1,786,329 / 98,292 |
| react-router | 6,468,009 / 109,395 | 832,242 / 69,700 |
| nuxt | 9,882,751 / 65,038 | 406,199 / 31,953 |
| django | 12,637,728 / 345,580 | 1,886,997 / 170,501 |
| nextjs | 17,099,460 / 132,653 | 1,705,323 / 58,421 |
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
| **corten** | 15,890 / 3,617 | 25,527 / 3,853 | 4.7% | 960 | 16.4 MiB |
| aspnet | 4,184 / 1,065 | 12,359 / 1,484 | 25% | 48 | 87.2 MiB |
| phoenix | 1,420 / 373 | 2,137 / 421 | 36% | 4.9 | 287 MiB |
| leptos | 1,408 / 773 | 13,844 / 1,075 | 24% | 88 | 16.1 MiB |
| astro | 1,222 / 462 | 1,971 / 547 | 30% | 8.7 | 140.4 MiB |
| spring | 1,137 / 514 | 6,629 / 912 | 45% | 3.2 | 357 MiB |
| sveltekit | 920 / 364 | 3,223 / 716 | 40% | 6.4 | 143.5 MiB |
| tanstack-start | 683 / 327 | 3,626 / 676 | 49% | 4.2 | 162.7 MiB |
| react-router | 542 / 290 | 2,352 / 554 | 61% | 3.8 | 142 MiB |
| rails | 457 / 334 | 934 / 405 | 72% | 3.2 | 142.9 MiB |
| django | 335 / 181 | 1,195 / 335 | 93% | 1.0 | 344 MiB |
| nuxt | 329 / 211 | 6,694 / 772 | 100%, 290/s served | 2.0 | 161.2 MiB |
| nextjs | failed (186) / failed (119) | 1,141 / 354 | failed (89%, 158/s served) | failed (0.8) | 219.2 MiB |
| laravel | **failed** / failed (98) | **failed** / 244 | **failed** | **failed** | **failed** |

Corten renders a page from memory with 63 µs of CPU (15,890 per CPU-second), 3.8x ASP.NET Core
(239 µs) and 11x Leptos (710 µs), and its JSON with 39 µs, against Leptos's 72 and ASP.NET Core's
81. At 300 pages a second it used 4.7% of its core, ASP.NET Core 25% and Leptos 24%; that is 6,450
pages per CPU-second, about two fifths of its rate at 64 users, since at a low rate each request
also pays for waking the server. It serves 960 pages a second per MiB: 11x Leptos (88), which peaks
lower (16.1 MiB against 16.4) but serves an eleventh of the pages, and 20x ASP.NET Core (48). From
the item API, fetching and parsing the 55 KB item costs Corten about 210 µs a page, 3.4 times the
page itself
([#166 Corten's item-API path spends six times as long fetching and parsing the item as rendering it, and the single-process item API could soon cap it](https://github.com/todd-skelton/corten/issues/166)).
Corten's pages per CPU-second from memory fell 16.5% from r7, on a Platinum 8280, to r8; the
other apps' median fell 2.0% (from -13.6% to +2.2%), across the same change of CPU
([#260 The item page from memory fell 19.9% between benchmark rounds r6 and r7 (23,416 to 18,758/s) while the unchanged apps fell 1.2% and its instructions held](https://github.com/todd-skelton/corten/issues/260)
tracks the fall from r6 to r7).

In r8 Corten's core was 101% busy at 64 users for the page from memory and 99% for the JSON: the
load generator, which from r7 also gets the item API's two cores (idle with items in memory), kept
it busy. Every one of Corten's phases has its CPU time read from the container's cgroup counter
rather than from once-a-second samples (`cpu-time.mjs`; `cpuCounted` in `summary.json`), and the
item API used 46-51% of one core at 64 users (45-48% at 256), of the 200% its two processes can
use, in Corten's item-API phases.

### Bytes the item page loads (raw / gzip, KB)

| App | HTML | JavaScript (files) | WASM | CSS |
|---|---|---|---|---|
| astro | 9.3 / 2.6 | 24.9 / 9.2 (3) | 0 | 0.0 / 0.0 |
| **corten** | 11.2 / 2.3 | 26.7 / 10.2 (4) | 0 | 8.4 / 2.5 |
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
| django | 51 (52) ms | 72 ms | 0 ms | 72 ms | 21 (28) ms | 1.7 MB |
| aspnet | 51 (56) ms | 72 ms | 0 ms | 73 ms | 16 (19) ms | 1.7 MB |
| astro | 55 (58) ms | 64 ms | 0 ms | 73 ms | 16 (17) ms | 1.6 MB |
| spring | 60 (86) ms | 72 ms | 0 ms | 73 ms | 34 (50) ms | 1.7 MB |
| **corten** | 61 (65) ms | 72 ms | 0 ms | 83 ms | 14 (14) ms | 1.6 MB |
| rails | 63 (71) ms | 68 ms | 0 ms | 88 ms | 24 (46) ms | 2.1 MB |
| laravel | 77 (81) ms | 68 ms | 0 ms | 88 ms | 24 (25) ms | 2.2 MB |
| nuxt | 99 (103) ms | 76 ms | 0 ms | 71 ms | 23 (24) ms | 2.9 MB |
| leptos | 102 (111) ms | 76 ms | 0 ms | 71 ms | 23 (24) ms | 1.9 MB |
| sveltekit | 109 (112) ms | 80 ms | 0 ms | 72 ms | 21 (26) ms | 2.3 MB |
| phoenix | 113 (124) ms | 92 ms | 0 ms | 79 ms | 17 (17) ms | 2.3 MB |
| tanstack-start | 124 (126) ms | 72 ms | 0 ms | 81 ms | 26 (29) ms | 3.1 MB |
| nextjs | 153 (158) ms | 80 ms | 0 ms | 84 ms | 136 (143) ms | 3.4 MB |
| react-router | 170 (177) ms | 112 ms | 0 ms | 86 ms | 30 (31) ms | 2.7 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| aspnet | 432 (437) ms | 412 ms | 6 ms | 280 ms | 243 (247) ms | 1.7 MB |
| django | 437 (442) ms | 412 ms | 6 ms | 284 ms | 241 (246) ms | 1.7 MB |
| **corten** | 461 (474) ms | 460 ms | 14 ms | 291 ms | 201 (203) ms | 1.6 MB |
| spring | 612 (619) ms | 420 ms | 5 ms | 286 ms | 244 (245) ms | 1.7 MB |
| astro | 618 (628) ms | 272 ms | 0 ms | 302 ms | 242 (246) ms | 1.6 MB |
| sveltekit | 841 (847) ms | 476 ms | 7 ms | 275 ms | 270 (273) ms | 2.3 MB |
| laravel | 876 (889) ms | 420 ms | 20 ms | 318 ms | 214 (217) ms | 2.2 MB |
| phoenix | 932 (951) ms | 408 ms | 44 ms | 148 ms | 129 (143) ms | 2.3 MB |
| react-router | 1,057 (1,102) ms | 432 ms | 33 ms | 336 ms | 257 (273) ms | 2.8 MB |
| rails | 1,149 (1,156) ms | 428 ms | 4 ms | 298 ms | 251 (254) ms | 2.1 MB |
| nextjs | 1,390 (1,460) ms | 400 ms | 153 ms | 307 ms | 433 (441) ms | 3.3 MB |
| nuxt | 1,505 (1,515) ms | 436 ms | 68 ms | 272 ms | 272 (279) ms | 2.9 MB |
| leptos | 1,835 (1,846) ms | 416 ms | 69 ms | 285 ms | 246 (250) ms | 1.9 MB |
| tanstack-start | 2,317 (2,337) ms | 420 ms | 69 ms | 284 ms | 344 (352) ms | 3.1 MB |

### What the numbers say

1. Rendering from memory: Corten serves 15,747 item pages a second at 64 users, 3.8x ASP.NET
   Core (4,182), the next fastest, and 11x Leptos (1,408). In r7 a page cost it 95,949 user-mode
   instructions; ASP.NET Core 1.00 million, Leptos 2.7 million, Next.js 17 million.
2. JSON from memory: Corten leads with 24,652 a second, then Leptos (13,758) and ASP.NET Core
   (12,347). In r7 Corten's JSON cost 69,572 user-mode instructions, a third of Leptos's (204,467)
   and less than its own HTML page.
3. With the item API every request fetches and parses about 55 KB of JSON, and that dominates.
   Corten parses only the fields the page reads (see below) and serves 3,616 pages a second,
   3.4x ASP.NET Core (1,065) and 4.7x Leptos (773); the rest serve 181-515. As JSON from the item
   API: Corten 3,852, ASP.NET Core 1,483, Leptos 1,075. At 150 a second Corten's page used 6.4% of
   its core, ASP.NET Core's 39%.
4. Failed: Next.js could not serve 300 pages a second from memory (158, p99 57.2 s) or 150 from
   the item API (80). Laravel did not start for the in-memory runs: each Octane worker loads the
   508 items into PHP arrays, which PHP's default `memory_limit` (128 MB) does not allow; with the
   item API it served 92 of 150 pages a second and kept up with the JSON. On r8's Platinum 8168,
   the CPU model it failed on in r4 and r5, Nuxt passed the in-memory page test (290 of 300, 1.73%
   never sent) at 100% CPU and a median of 708 ms, and Django passed it at 93% CPU.
5. Cold start: Corten and Leptos answer 6.7 ms and 7.8 ms after their process starts and hold
   1.2 MiB and 1.5 MiB after the first request. The Node frameworks take 188-719 ms and
   24-52 MiB; Spring Boot 8.2 s and 142 MiB.
6. Browser: on desktop the server-rendered apps with small scripts (Django, ASP.NET Core, Astro,
   Spring, Corten, Rails) are interactive in 51-63 ms. On the throttled phone Corten is
   interactive in 461 ms, with the htmx apps (432-437 ms), against 1,390 ms for Next.js,
   1,835 ms for Leptos (WASM) and 2,317 ms for TanStack Start. Corten's client navigation is the
   fastest on desktop (14 ms); on the phone Phoenix's is (129 ms against 201 ms), over its
   already-open LiveView socket. Corten's item page loads 10.2 KB of JavaScript (gzip) in four
   files, against 9.3 KB in r7 and 9.6 KB in r6, more than Astro's 9.2 KB.

r8 ran on a Platinum 8168, r7 on a Platinum 8280, and every app but Corten was unchanged. The
other apps' 64-user requests a second moved by a median of -2.0% for the page from memory, -3.8%
for the JSON from memory, -0.6% for the item-API page and -8.3% for the item-API JSON. Corten's
JSON from memory (-1.3%, 24,976 to 24,652), item-API page (+2.1%, 3,540 to 3,616) and item-API JSON
(-8.2%, 4,196 to 3,852) moved with them. Its page from memory did not: it fell 16.1% (18,758 to
15,747), after falling 19.9% from r6 to r7
([#260 The item page from memory fell 19.9% between benchmark rounds r6 and r7 (23,416 to 18,758/s) while the unchanged apps fell 1.2% and its instructions held](https://github.com/todd-skelton/corten/issues/260)),
and its pages per CPU-second fell as much (-16.5%, 19,025 to 15,890), against the other apps'
median of -2.0%. Its core was busy in both rounds (101%), so the load generator is not the cause.
The unchanged apps spread widely across this change of CPU (page from memory -13.6% to +2.2%, JSON
-19.3% to +4.4%), so a single app's move of this size is not proof; r8 has no instruction counts
to show where the work went (caveat 11). The item page's JavaScript grew 8.9% (9,541 to 10,394
bytes gzip), its CSS 7.4% (2,430 to 2,609) and its HTML 1.1% (2,358 to 2,385), and Corten's memory
after the first request rose from 1.211 to 1.23 MiB. Its cold start (7.3 to 6.7 ms, while the
others' median rose 2.1%) and peak memory at 64 users (18.3 to 16.4 MiB) went down. r7 has no
browser numbers, so r8's compare with r6's: Corten's desktop interactive went from 58 to 61 ms and
its phone from 449 to 461 ms, while the other apps moved by a median of +7.8% and 0.0%, and its
phone blocking time went from 0 to 14 ms. Counts compare across rounds, and requests a second only
within one.

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

Instructions are `count.mjs ... api`, median of 3 x 5,000 requests. r8, on a Platinum 8168 like
r5's, serves 3,616 item-API pages a second against ASP.NET Core's 1,065 (3.4x; 2.3x in the A/B and
in r4, 3.2x in r5, 3.1x in r6 and r7), at 6.4% of its core at 150 a second against 39%.

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
   8168 at 2.7 GHz in r8, 16 GB), Ubuntu 24.04, kernel 6.8, Docker 29.9, Node 24, created for the run
   and destroyed after it (`droplet.mjs`). Droplets of one size are not all the same CPU (r1's,
   r4's, r5's and r8's were a Platinum 8168, r2's, r3's, r6's and r7's a Platinum 8280), so times are compared within a
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
   (`cpu-time.mjs`, `<app>-<phase>-cpu.json`). From r8 the tables' CPU at the fixed rate is that
   counted time.
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
   It first writes the running kernel to `count-kernel.txt` and has `perf` count an idle second,
   stopping with the reason when `perf` does not fit that kernel; `droplet.mjs` turns off the
   droplet's automatic package updates before provisioning, so the kernel stays the one the round
   starts on.
6. Browser (`browser.mjs`): every app running at once on the app core, items from the item API,
   one app measured at a time; Playwright
   Chromium, cache disabled, 10 runs per app and profile, each cold load starting on a page
   from another origin. Desktop is unthrottled; mobile is 4x CPU slowdown, 150 ms RTT, 1.6 Mbps down.
   "Interactive" is when the page's own code marks itself ready (hydration finished, islands
   connected, or the framework's load event). Action is fill, submit, and wait for the
   confirmation. Navigation is home page to item page. From round r8, the action is not timed on
   `item_12345`, which has no listings: card-market refuses an alert there and the others set
   one, which is different work, so its median comes from 8 of 10 runs, where earlier rounds'
   came from all 10
   ([#259 Benchmark round r7 has no browser numbers: Corten refuses the price alert on item_12345, which has no listings, and browser.mjs times out waiting for 'Alert set'](https://github.com/todd-skelton/corten/issues/259)).
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
7. Laravel ran with PHP's default `memory_limit` throughout r3 to r8. (r2's main run had raised it
   to 512 MB and measured Laravel again with the default afterwards; `results/r2/laravel-rerun.log`.)
8. r4's droplet had a slower CPU than r3's (Platinum 8168 against 8280). Django and Nuxt, which
   passed r3's in-memory page test near their limit, failed it on r4's; their settings did not
   change. r3's numbers stay in `results/r3/`. In r5, on the same CPU model as r4, Django passed
   it again (4.1% never sent) and Nuxt did not; in r6 and r7, on a Platinum 8280, both passed, and in
   r8, on a Platinum 8168 again, both passed (Nuxt with 1.73% never sent).
9. r8 measured the tree at `2976f15` (`results/r8/commit.txt`). Changes merged to `main` after
   that are not in these numbers.
10. r6's efficiency numbers divide by CPU sampled once a second, which is coarse for an app using
    a few percent of its core (Corten's 4.1% at the fixed rate); from r7 they divide by the
    counted CPU time. Requests per CPU-second, like requests a second, compare only within a round.
11. r8 has no instruction counts. r8's droplet stopped answering both times the round reached
    `count.mjs`, right after its warmup (the first time with a kernel soft lockup); the cause is not
    known, and the round finished without that step
    ([#466 Benchmark round r8 has no instruction counts: its droplet locked up twice when count.mjs started counting](https://github.com/todd-skelton/corten/issues/466)).
    The instructions table and the counts quoted with it are r7's (the tree at `a9b1163`): counts
    do not depend on the droplet, but Corten changed between the two commits.

## Running it

```sh
# On a fresh droplet, created and destroyed for the run (needs doctl and an SSH key there):
node bench/fullstack/droplet.mjs r9 --ssh-key <id> [--region sfo2]
# Or by hand, from the repository root: the images, the fixture, then everything.
docker build -f examples/card-market/Dockerfile -t cb-corten .
for app in nextjs tanstack-start sveltekit react-router leptos django rails laravel aspnet spring phoenix astro nuxt; do
  docker build -t cb-$app bench/fullstack/apps/$app
done
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400   # or any {"item_1": {...}} file
cd bench/fullstack && node run-all.mjs r9 10
node check-app.mjs <app>    # before adding an app: parity, JSON and 404s against corten
```
