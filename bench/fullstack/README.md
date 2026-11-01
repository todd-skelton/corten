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
writes its counts as an English page does, digits grouped and the word singular for one ("2,866
listings", "1 listing"), as Corten's plural messages do. Parity compares the grouping (the
fixture's counts are all 3 or more); the singular form is checked by reading each app. And every app
answers `GET /api/items/{id}` with the same JSON (`check-app.mjs` checks both before an app joins
a round). Each has the same interactions: a client-side sort, a form action that updates part of
the page, and client-side navigation from the home page, each done the framework's usual way
(htmx for Razor Pages, Spring and Django; Turbo for Rails; Livewire for Laravel; LiveView for
Phoenix). The Corten app also runs everything a production deployment would (CSP, CSRF check,
compression, versioned assets, service worker, error pages, health, MCP tools); the others run
their framework defaults in production mode.

The same apps hold the live-updates comparison in [bench/live](../live/README.md).

## Results

All numbers are measured, from one run (`results/r10/`, 2026-11-01 UTC, the tree at `f5c76a0`) on one
DigitalOcean c-8 droplet (Intel Xeon Platinum 8280); see [Method](#method). Failed means the app could not do the
test with its usual settings in one CPU and 1 GiB (see caveat 5); its other numbers are kept
where it produced any. r9 measured nothing (its launcher hung before the upload). r8
(`results/r8/`, a Platinum 8168, Corten at `2976f15`, 530 commits before r10's, before plural
messages, typed custom properties and comment-free component scripts, with no instruction counts),
r7 (`results/r7/`, a Platinum 8280, Corten at `a9b1163`, with no browser
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
| **corten** | 5.9 ms (5.4 ms-27 ms) | 1.223 MiB | 124 MB |
| leptos | 6.8 ms (6.0 ms-24 ms) | 1.488 MiB | 127 MB |
| sveltekit | 177 ms (172 ms-244 ms) | 25.93 MiB | 333 MB |
| tanstack-start | 216 ms (206 ms-264 ms) | 27.36 MiB | 332 MB |
| react-router | 301 ms (292 ms-362 ms) | 33.25 MiB | 353 MB |
| astro | 304 ms (294 ms-343 ms) | 36.45 MiB | 528 MB |
| nuxt | 312 ms (299 ms-360 ms) | 37.36 MiB | 335 MB |
| nextjs | 682 ms (663 ms-792 ms) | 52.47 MiB | 385 MB |
| aspnet | 699 ms (669 ms-757 ms) | 30.58 MiB | 342 MB |
| laravel | 786 ms (765 ms-850 ms) | 95.11 MiB | 947 MB |
| rails | 1.0 s (984 ms-1.1 s) | 63.93 MiB | 431 MB |
| django | 1.1 s (1.0 s-1.2 s) | 65.43 MiB | 276 MB |
| phoenix | 1.3 s (1.2 s-1.3 s) | 125.7 MiB | 262 MB |
| spring | 7.7 s (7.3 s-7.9 s) | 137.7 MiB | 516 MB |

### Item page, items in memory (no database)

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.2 ms / 0.5 ms, 4% | 23,898, 2.2 ms / 7.7 ms | 24,413, 32 ms, 0% | 16-19 MiB |
| aspnet | 300/s, 0.5 ms / 2.1 ms, 19% | 4,644, 14 ms / 21 ms | 4,351, 97 ms, 0% | 85-89 MiB |
| phoenix | 300/s, 0.9 ms / 1.3 ms, 31% | 1,765, 36 ms / 50 ms | 1,618, 932 ms, 0% | 295-296 MiB |
| leptos | 300/s, 0.8 ms / 1.2 ms, 21% | 1,607, 40 ms / 57 ms | 1,372, 301 ms, 0% | 16-34 MiB |
| astro | 300/s, 0.8 ms / 27 ms, 28% | 1,311, 44 ms / 108 ms | 1,265, 268 ms, 0% | 143-144 MiB |
| spring | 300/s, 1.0 ms / 5.0 ms, 46% | 1,161, 40 ms / 320 ms | 1,179, 961 ms, 0% | 358-379 MiB |
| sveltekit | 300/s, 0.9 ms / 16 ms, 33% | 1,055, 53 ms / 111 ms | 928, 311 ms, 0% | 151-155 MiB |
| tanstack-start | 300/s, 1.3 ms / 27 ms, 44% | 736, 79 ms / 144 ms | 744, 412 ms, 0% | 165-166 MiB |
| react-router | 300/s, 1.7 ms / 40 ms, 56% | 587, 101 ms / 171 ms | 576, 516 ms, 0% | 145-147 MiB |
| rails | 300/s, 1.8 ms / 33 ms, 60% | 559, 114 ms / 131 ms | 472, 609 ms, 0% | 135-137 MiB |
| django | 300/s, 2.5 ms / 8.9 ms, 80% | 385, 165 ms / 191 ms | 383, 736 ms, 0% | 346-347 MiB |
| nuxt | 300/s, 29 ms / 469 ms, 91% | 336, 189 ms / 238 ms | 314, 2.2 s, 0% | 161-162 MiB |
| nextjs | **failed**: 159/s served, 1.2 s / 56.6 s, 87% | 195, 342 ms / 398 ms | 192, 12.0 s, 0% | 216-218 MiB |
| laravel | **failed**: never answered /items/item_1 within a minute | - | - | - |

### The item as JSON, items in memory

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.2 ms / 0.5 ms, 4% | 32,155, 1.6 ms / 7.5 ms | 25,401, 30 ms, 0% | 15-19 MiB |
| leptos | 300/s, 0.2 ms / 0.5 ms, 4% | 19,343, 2.9 ms / 7.3 ms | 16,596, 30 ms, 0% | 15-18 MiB |
| aspnet | 300/s, 0.3 ms / 0.7 ms, 10% | 14,945, 4.0 ms / 7.9 ms | 14,259, 33 ms, 0% | 79-83 MiB |
| nuxt | 300/s, 0.3 ms / 0.7 ms, 9% | 8,000, 6.8 ms / 29 ms | 7,092, 74 ms, 0% | 104-107 MiB |
| spring | 300/s, 0.3 ms / 0.9 ms, 19% | 6,967, 6.0 ms / 86 ms | 7,167, 429 ms, 0% | 357-381 MiB |
| tanstack-start | 300/s, 0.4 ms / 3.4 ms, 12% | 3,783, 14 ms / 44 ms | 3,379, 113 ms, 0% | 140-142 MiB |
| sveltekit | 300/s, 0.4 ms / 3.3 ms, 12% | 3,517, 15 ms / 48 ms | 3,342, 125 ms, 0% | 125-127 MiB |
| phoenix | 300/s, 0.6 ms / 1.0 ms, 22% | 2,783, 23 ms / 34 ms | 2,491, 128 ms, 0% | 280-334 MiB |
| react-router | 300/s, 0.6 ms / 3.5 ms, 17% | 2,630, 22 ms / 65 ms | 2,466, 168 ms, 0% | 121-122 MiB |
| astro | 300/s, 0.6 ms / 21 ms, 20% | 2,094, 27 ms / 70 ms | 2,071, 175 ms, 0% | 139-141 MiB |
| django | 300/s, 0.6 ms / 1.0 ms, 24% | 1,443, 43 ms / 56 ms | 1,437, 214 ms, 0% | 293-294 MiB |
| nextjs | 300/s, 0.8 ms / 25 ms, 28% | 1,187, 49 ms / 112 ms | 1,184, 284 ms, 0% | 175-176 MiB |
| rails | 300/s, 0.9 ms / 7.1 ms, 30% | 1,097, 58 ms / 74 ms | 1,153, 270 ms, 0% | 125-134 MiB |
| laravel | **failed**: never answered /api/items/item_1 within a minute | - | - | - |

### Item page, item fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.6 ms / 1.0 ms, 5% | 4,283, 14 ms / 22 ms | 3,433, 108 ms, 0% | 16-54 MiB |
| aspnet | 150/s, 1.4 ms / 6.6 ms, 30% | 1,255, 50 ms / 74 ms | 1,105, 279 ms, 0% | 69-99 MiB |
| leptos | 150/s, 1.5 ms / 2.1 ms, 19% | 842, 78 ms / 92 ms | 718, 470 ms, 0% | 16-57 MiB |
| spring | 150/s, 2.2 ms / 9.2 ms, 57% | 508, 124 ms / 226 ms | 408, 1.2 s, 0% | 425-456 MiB |
| astro | 150/s, 1.8 ms / 35 ms, 39% | 477, 137 ms / 169 ms | 440, 704 ms, 0% | 129-149 MiB |
| phoenix | 150/s, 2.7 ms / 3.5 ms, 42% | 434, 146 ms / 199 ms | 431, 5.8 s, 0% | 211-298 MiB |
| sveltekit | 150/s, 2.0 ms / 21 ms, 38% | 412, 156 ms / 193 ms | 405, 710 ms, 0% | 150-163 MiB |
| rails | 150/s, 2.8 ms / 27 ms, 45% | 406, 156 ms / 177 ms | 352, 802 ms, 0% | 108-111 MiB |
| tanstack-start | 150/s, 2.4 ms / 31 ms, 45% | 353, 185 ms / 220 ms | 330, 879 ms, 0% | 140-167 MiB |
| react-router | 150/s, 2.8 ms / 39 ms, 52% | 306, 212 ms / 250 ms | 316, 943 ms, 0% | 133-149 MiB |
| django | 150/s, 4.4 ms / 6.4 ms, 68% | 214, 295 ms / 359 ms | 213, 1.3 s, 0% | 126-127 MiB |
| nuxt | 150/s, 4.0 ms / 198 ms, 71% | 211, 304 ms / 345 ms | 209, 2.4 s, 0% | 137-194 MiB |
| nextjs | **failed**: 85/s served, 2.5 s / 48.8 s, 77% | 123, 510 ms / 645 ms | 111, 23.6 s, 0% | 219-259 MiB |
| laravel | **failed**: 116/s served, 6.5 s / 8.5 s, 99% | 116, 531 ms / 910 ms | 117, 3.1 s, 0% | 114-126 MiB |

### The item as JSON, fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.6 ms / 1.0 ms, 5% | 4,191, 15 ms / 21 ms | 3,710, 103 ms, 0% | 17-57 MiB |
| aspnet | 150/s, 1.1 ms / 5.3 ms, 23% | 1,769, 36 ms / 51 ms | 1,652, 200 ms, 0% | 57-85 MiB |
| leptos | 150/s, 1.1 ms / 1.5 ms, 13% | 1,234, 52 ms / 62 ms | 1,144, 304 ms, 0% | 15-57 MiB |
| spring | 150/s, 1.4 ms / 5.0 ms, 35% | 868, 69 ms / 151 ms | 733, 672 ms, 0% | 411-448 MiB |
| nuxt | 150/s, 1.3 ms / 15 ms, 24% | 795, 80 ms / 125 ms | 721, 519 ms, 0% | 127-142 MiB |
| sveltekit | 150/s, 1.3 ms / 15 ms, 25% | 727, 90 ms / 116 ms | 636, 476 ms, 0% | 122-141 MiB |
| tanstack-start | 150/s, 1.3 ms / 17 ms, 26% | 703, 95 ms / 120 ms | 599, 516 ms, 0% | 126-151 MiB |
| react-router | 150/s, 1.6 ms / 25 ms, 32% | 605, 110 ms / 137 ms | 547, 571 ms, 0% | 122-143 MiB |
| astro | 150/s, 1.6 ms / 28 ms, 34% | 568, 115 ms / 135 ms | 540, 572 ms, 0% | 129-148 MiB |
| rails | 150/s, 1.9 ms / 18 ms, 32% | 503, 121 ms / 168 ms | 494, 583 ms, 0% | 83-86 MiB |
| phoenix | 150/s, 2.4 ms / 3.1 ms, 37% | 500, 127 ms / 167 ms | 487, 622 ms, 0% | 226-350 MiB |
| django | 150/s, 2.6 ms / 3.6 ms, 40% | 384, 164 ms / 204 ms | 382, 766 ms, 0% | 75-75 MiB |
| nextjs | 150/s, 1.9 ms / 49 ms, 39% | 365, 185 ms / 218 ms | 345, 1.0 s, 0% | 151-194 MiB |
| laravel | 150/s, 3.4 ms / 16 ms, 50% | 285, 197 ms / 422 ms | 279, 1.7 s, 0% | 102-117 MiB |

### Instructions per request

| App | Item page: user / kernel | JSON: user / kernel |
|---|---|---|
| **corten** | 110,172 / 32,989 | 69,968 / 33,225 |
| aspnet | 1,162,953 / 57,414 | 267,635 / 55,881 |
| astro | 2,013,228 / 79,869 | 1,049,850 / 76,468 |
| phoenix | 2,145,724 / 90,744 | 1,549,593 / 80,678 |
| leptos | 2,727,486 / 35,147 | 205,205 / 33,459 |
| sveltekit | 4,463,599 / 45,026 | 676,323 / 33,584 |
| rails | 5,209,289 / 323,646 | 2,275,916 / 282,847 |
| tanstack-start | 5,237,740 / 49,230 | 537,960 / 33,593 |
| spring | 6,185,933 / 98,492 | 1,411,254 / 99,063 |
| react-router | 6,459,058 / 108,783 | 830,574 / 69,746 |
| nuxt | 9,809,095 / 64,529 | 408,706 / 32,077 |
| django | 12,724,171 / 346,851 | 1,887,796 / 171,131 |
| nextjs | 17,203,430 / 132,366 | 1,699,707 / 58,394 |
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
| **corten** | 24,459 / 4,284 | 34,058 / 4,191 | 4.0% | 1,457 | 16.4 MiB |
| aspnet | 4,648 / 1,256 | 14,965 / 1,774 | 19% | 54 | 85.4 MiB |
| phoenix | 1,768 / 434 | 2,788 / 500 | 31% | 6.0 | 295.6 MiB |
| leptos | 1,607 / 842 | 19,559 / 1,235 | 21% | 100 | 16.1 MiB |
| astro | 1,311 / 477 | 2,094 / 568 | 28% | 9.1 | 143.3 MiB |
| spring | 1,162 / 507 | 6,956 / 865 | 46% | 3.2 | 358.5 MiB |
| sveltekit | 1,055 / 412 | 3,517 / 727 | 33% | 6.9 | 151.9 MiB |
| tanstack-start | 736 / 353 | 3,781 / 704 | 44% | 4.4 | 165.7 MiB |
| react-router | 587 / 306 | 2,630 / 605 | 56% | 4.0 | 145.7 MiB |
| rails | 559 / 406 | 1,097 / 503 | 60% | 3.9 | 143.3 MiB |
| django | 385 / 214 | 1,447 / 385 | 80% | 1.1 | 346.3 MiB |
| nuxt | 336 / 211 | 8,001 / 795 | 91% | 2.1 | 161.3 MiB |
| nextjs | failed (195) / failed (123) | 1,187 / 365 | failed (87%, 159/s served) | failed (0.9) | 216.2 MiB |
| laravel | **failed** / failed (117) | **failed** / 288 | **failed** | **failed** | **failed** |

Corten renders a page from memory with 41 µs of CPU (24,459 per CPU-second), 5.3x ASP.NET Core
(215 µs) and 15x Leptos (622 µs), and its JSON with 29 µs, against Leptos's 51 and ASP.NET Core's
67. At 300 pages a second it used 4.0% of its core, ASP.NET Core 19% and Leptos 21%; that is 7,434
pages per CPU-second, about three tenths of its rate at 64 users, since at a low rate each request
also pays for waking the server. It serves 1,457 pages a second per MiB: 15x Leptos (100), which
peaks lower (16.1 MiB against 16.4) but serves a fifteenth of the pages, and 27x ASP.NET Core (54).
From the item API, fetching and parsing the 55 KB item costs Corten about 190 µs a page, 4.7 times
the page itself
([#166 Corten's item-API path spends six times as long fetching and parsing the item as rendering it, and the single-process item API could soon cap it](https://github.com/todd-skelton/corten/issues/166)).
Corten's pages per CPU-second from memory rose 53.9% from r8, on a Platinum 8168, to r10; the
other apps' median rose 11.1% (from +2.1% to +24.5%), across the same change of CPU.

In r10 Corten's core was 99% busy at 64 users for the page from memory and 95% for the JSON: the
load generator, which from r7 also gets the item API's two cores (idle with items in memory), kept
it busy. Every one of Corten's phases has its CPU time read from the container's cgroup counter
rather than from once-a-second samples (`cpu-time.mjs`; `cpuCounted` in `summary.json`), and the
item API used 47-48% of one core at 64 users (40-49% at 256), of the 200% its two processes can
use, in Corten's item-API phases.

### Bytes the item page loads (raw / gzip, KB)

| App | HTML | JavaScript (files) | WASM | CSS |
|---|---|---|---|---|
| astro | 9.3 / 2.6 | 24.9 / 9.2 (3) | 0 | 0.0 / 0.0 |
| **corten** | 11.2 / 2.3 | 28.3 / 10.6 (4) | 0 | 8.9 / 2.7 |
| django | 7.3 / 1.8 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| aspnet | 7.3 / 1.9 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| spring | 7.2 / 1.8 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| sveltekit | 16.3 / 3.6 | 96.4 / 37.1 (11) | 0 | 1.9 / 0.8 |
| rails | 8.5 / 2.2 | 152.2 / 40.6 (7) | 0 | 2.3 / 0.9 |
| phoenix | 10.0 / 2.4 | 149.2 / 46.2 (1) | 0 | 2.3 / 0.9 |
| nuxt | 17.3 / 4.5 | 197.4 / 73.8 (4) | 0 | 1.9 / 0.8 |
| laravel | 10.8 / 2.8 | 255.8 / 84.3 (1) | 0 | 2.3 / 0.9 |
| react-router | 19.5 / 4.9 | 309.8 / 98.7 (4) | 0 | 1.9 / 0.8 |
| tanstack-start | 17.2 / 4.2 | 342.3 / 109.7 (3) | 0 | 1.9 / 0.8 |
| nextjs | 25.3 / 4.8 | 456.2 / 136.1 (7) | 0 | 1.9 / 0.8 |
| leptos | 18.1 / 3.6 | 25.1 / 7.0 (1) | 723.1 / 287.6 | 2.0 / 0.8 |

### Browser, median of 10 (p90)

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| aspnet | 41 (44) ms | 60 ms | 0 ms | 73 ms | 15 (15) ms | 1.7 MB |
| django | 42 (44) ms | 60 ms | 0 ms | 74 ms | 19 (20) ms | 1.7 MB |
| astro | 49 (49) ms | 56 ms | 0 ms | 74 ms | 15 (16) ms | 1.6 MB |
| rails | 56 (60) ms | 64 ms | 0 ms | 91 ms | 24 (26) ms | 2.1 MB |
| **corten** | 59 (65) ms | 68 ms | 0 ms | 73 ms | 13 (13) ms | 1.6 MB |
| spring | 60 (67) ms | 72 ms | 0 ms | 74 ms | 37 (42) ms | 1.7 MB |
| laravel | 66 (68) ms | 56 ms | 0 ms | 75 ms | 22 (23) ms | 2.2 MB |
| leptos | 95 (114) ms | 64 ms | 0 ms | 72 ms | 22 (23) ms | 1.9 MB |
| nuxt | 95 (99) ms | 56 ms | 0 ms | 71 ms | 23 (25) ms | 2.9 MB |
| phoenix | 98 (110) ms | 76 ms | 0 ms | 79 ms | 15 (16) ms | 2.3 MB |
| sveltekit | 106 (111) ms | 68 ms | 0 ms | 72 ms | 30 (32) ms | 2.3 MB |
| tanstack-start | 121 (127) ms | 60 ms | 0 ms | 70 ms | 26 (28) ms | 3.1 MB |
| nextjs | 152 (157) ms | 68 ms | 0 ms | 70 ms | 122 (150) ms | 3.4 MB |
| react-router | 171 (190) ms | 92 ms | 0 ms | 69 ms | 37 (40) ms | 2.7 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| aspnet | 423 (426) ms | 404 ms | 1 ms | 274 ms | 239 (242) ms | 1.7 MB |
| django | 432 (434) ms | 396 ms | 0 ms | 279 ms | 242 (243) ms | 1.7 MB |
| **corten** | 459 (468) ms | 456 ms | 11 ms | 278 ms | 196 (197) ms | 1.6 MB |
| astro | 594 (615) ms | 252 ms | 0 ms | 280 ms | 239 (241) ms | 1.6 MB |
| spring | 607 (608) ms | 412 ms | 1 ms | 270 ms | 236 (240) ms | 1.7 MB |
| laravel | 862 (874) ms | 416 ms | 8 ms | 314 ms | 212 (214) ms | 2.2 MB |
| sveltekit | 888 (897) ms | 464 ms | 8 ms | 267 ms | 292 (296) ms | 2.3 MB |
| phoenix | 918 (929) ms | 404 ms | 30 ms | 138 ms | 117 (132) ms | 2.3 MB |
| react-router | 1,081 (1,103) ms | 408 ms | 34 ms | 325 ms | 298 (303) ms | 2.8 MB |
| rails | 1,145 (1,151) ms | 428 ms | 0 ms | 296 ms | 249 (252) ms | 2.1 MB |
| nextjs | 1,423 (1,432) ms | 396 ms | 143 ms | 294 ms | 506 (511) ms | 3.3 MB |
| nuxt | 1,523 (1,539) ms | 424 ms | 102 ms | 270 ms | 277 (324) ms | 2.9 MB |
| leptos | 1,823 (1,831) ms | 412 ms | 56 ms | 278 ms | 241 (243) ms | 1.9 MB |
| tanstack-start | 2,346 (2,348) ms | 412 ms | 111 ms | 277 ms | 347 (349) ms | 3.1 MB |

### What the numbers say

1. Rendering from memory: Corten serves 23,898 item pages a second at 64 users, 5.1x ASP.NET
   Core (4,644), the next fastest, and 15x Leptos (1,607). A page costs it 110,172 user-mode
   instructions; ASP.NET Core 1.16 million, Leptos 2.7 million, Next.js 17 million.
2. JSON from memory: Corten leads with 32,155 a second, then Leptos (19,343) and ASP.NET Core
   (14,945). Corten's JSON costs 69,968 user-mode instructions, a third of Leptos's (205,205) and
   less than its own HTML page.
3. With the item API every request fetches and parses about 55 KB of JSON, and that dominates.
   Corten parses only the fields the page reads (see below) and serves 4,283 pages a second,
   3.4x ASP.NET Core (1,255) and 5.1x Leptos (842); the rest serve 211-508. As JSON from the item
   API: Corten 4,191, ASP.NET Core 1,769, Leptos 1,234. At 150 a second Corten's page used 5.3% of
   its core, ASP.NET Core's 30%.
4. Failed: Next.js could not serve 300 pages a second from memory (159, p99 56.6 s) or 150 from
   the item API (85). Laravel did not start for the in-memory runs: each Octane worker loads the
   508 items into PHP arrays, which PHP's default `memory_limit` (128 MB) does not allow; with the
   item API it served 116 of 150 pages a second and kept up with the JSON. On r10's Platinum 8280
   Nuxt passed the in-memory page test at 91% CPU (p50 29 ms, p99 469 ms), and Django at 80%.
5. Cold start: Corten and Leptos answer 5.9 ms and 6.8 ms after their process starts and hold
   1.2 MiB and 1.5 MiB after the first request. The Node frameworks take 177-682 ms and
   26-52 MiB; Spring Boot 7.7 s and 138 MiB.
6. Browser: on desktop the server-rendered apps with small scripts (ASP.NET Core, Django, Astro,
   Rails, Corten, Spring) are interactive in 41-60 ms. On the throttled phone Corten is
   interactive in 459 ms, with the htmx apps (423-432 ms), against 1,423 ms for Next.js,
   1,823 ms for Leptos (WASM) and 2,346 ms for TanStack Start. Corten's client navigation is the
   fastest on desktop (13 ms); on the phone Phoenix's is (117 ms against 196 ms), over its
   already-open LiveView socket. Corten's item page loads 10.6 KB of JavaScript (gzip) in four
   files, against 10.2 KB in r8, more than Astro's 9.2 KB.

r10 ran on a Platinum 8280, r8 on a Platinum 8168. Between them Corten moved 530 commits, and the
other apps changed only to write counts as Corten's plural messages do (digits grouped, the
singular for one; `600050a`). The other apps' 64-user requests a second rose by a median of 11.0%
for the page from memory, 14.7% for the JSON from memory, 8.9% for the item-API page and 9.2% for
the item-API JSON. Corten's rose more from memory, the page 51.8% (15,747 to 23,898) and the JSON
30.4% (24,652 to 32,155), and with the others from the item API: the page 18.4% (3,616 to 4,283)
and the JSON 8.8% (3,852 to 4,191). Its pages per CPU-second from memory rose 53.9% (15,890 to
24,459; the other apps' median +11.1%). Against r7, on the same CPU model, its page from memory
is 27.4% faster (18,758 to 23,898) while the other apps' median rose 8.1%, and above r6's 23,416,
after the falls from r6 to r7 and r7 to r8
([#260 The item page from memory fell 19.9% between benchmark rounds r6 and r7 (23,416 to 18,758/s) while the unchanged apps fell 1.2% and its instructions held](https://github.com/todd-skelton/corten/issues/260)).
Corten got worse in two places. Its item page costs 14.8% more user-mode instructions than in r7,
the last round that counted (95,949 to 110,172; kernel 33,294 to 32,989), while the other apps'
page counts moved by a median of +0.3% (ASP.NET Core +15.9%, Spring +4.9% and Rails +3.9% after
the change to their counts); its JSON held (69,572 to 69,968, +0.6%). Each page now runs more
instructions in less CPU time, and one run of each cannot say why. Its item page's CSS grew 5.1%
(2,609 to 2,742 bytes gzip) and its JavaScript 4.3% (10,394 to 10,844); the HTML held (2,385 to
2,387). Its cold start (6.7 to 5.9 ms, while the others' median fell 6.4%), memory after the first
request (1.23 to 1.223 MiB) and peak memory at 64 users (16.4 MiB in both) held or went down. In
the browser Corten's desktop interactive went from 61 to 59 ms and its phone from 461 to 459 ms,
while the other apps moved by a median of -6.9% and -0.7%, and its phone blocking time went from
14 to 11 ms. Counts compare across rounds, and requests a second only within one.

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

Instructions are `count.mjs ... api`, median of 3 x 5,000 requests. r10, on a Platinum 8280 like
r6's and r7's, serves 4,283 item-API pages a second against ASP.NET Core's 1,255 (3.4x; 2.3x in the
A/B and in r4, 3.2x in r5, 3.1x in r6 and r7, 3.4x in r8), at 5.3% of its core at 150 a second
against 30%.

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
   8280 at 2.7 GHz in r10, 16 GB), Ubuntu 24.04, kernel 6.8, Docker 29.9, Node 24, created for the run
   and destroyed after it (`droplet.mjs`). The round runs on the droplet by itself, as a systemd
   unit (`corten-bench-<round>`), so it does not need the machine that started it:
   `droplet.mjs --detach` starts it and leaves, and `droplet.mjs <round> --collect` later
   downloads the results and destroys the droplet once the round has ended (or says where it is).
   On the droplet, `run-all.mjs --unattended` gives each call a time limit (at least twice the
   longest it took in r5-r8) and stops one that runs past it or prints nothing for an hour. A kernel
   lockup (r8's) panics the kernel, which reboots the droplet; the unit starts again at boot and
   the round goes on from the next step. A droplet that does not answer for 15 minutes while
   `droplet.mjs` waits or collects is power-cycled, with the same effect (the full comparison
   only; `--collect` gives up, leaving the droplet, only when DigitalOcean cannot be reached
   either). Either way the step it
   was in is skipped (the full comparison only: `--suite live` and `--suite browser` start no
   further), named in `run.log` and in `summary.json` (`skipped`), and its results are partly or
   wholly missing from the round. A round that stops short keeps what it measured, and the
   droplet is destroyed only after its results are downloaded. Droplets of one size are
   not all the same CPU (r1's, r4's, r5's and r8's were a Platinum 8168, r2's, r3's, r6's,
   r7's and r10's a Platinum 8280), so times are compared within a round, never across rounds.
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
   It first writes the running kernel (and any package waiting for a reboot) to
   `count-kernel.txt` and has `perf` count an idle second, stopping with the reason when `perf`
   does not fit that kernel or cannot count; `droplet.mjs` turns off the droplet's automatic
   package updates before provisioning, so a reboot comes back on the kernel `perf` was installed
   for.
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
7. Laravel ran with PHP's default `memory_limit` throughout r3 to r10. (r2's main run had raised it
   to 512 MB and measured Laravel again with the default afterwards; `results/r2/laravel-rerun.log`.)
8. r4's droplet had a slower CPU than r3's (Platinum 8168 against 8280). Django and Nuxt, which
   passed r3's in-memory page test near their limit, failed it on r4's; their settings did not
   change. r3's numbers stay in `results/r3/`. In r5, on the same CPU model as r4, Django passed
   it again (4.1% never sent) and Nuxt did not; in r6 and r7, on a Platinum 8280, both passed, and in
   r8, on a Platinum 8168 again, both passed (Nuxt with 1.73% never sent), as they did in r10, on
   a Platinum 8280.
9. r10 measured the tree at `f5c76a0` (`results/r10/commit.txt`). Changes merged to `main` after
   that are not in these numbers.
10. r6's efficiency numbers divide by CPU sampled once a second, which is coarse for an app using
    a few percent of its core (Corten's 4.1% at the fixed rate); from r7 they divide by the
    counted CPU time. Requests per CPU-second, like requests a second, compare only within a round.
11. r8 has no instruction counts. r8's droplet stopped answering both times the round reached
    `count.mjs`, right after its warmup (the first time with a kernel soft lockup); the cause is not
    known, and the round finished without that step
    ([#466 Benchmark round r8 has no instruction counts: its droplet locked up twice when count.mjs started counting](https://github.com/todd-skelton/corten/issues/466)).
    r9 measured nothing (its launcher hung before the upload), so r10's counts are compared with
    r7's (the tree at `a9b1163`), the last round before it that counted: counts do not depend on
    the droplet, but Corten changed between the two commits.

## Running it

```sh
# On a fresh droplet, created and destroyed for the run (needs doctl and an SSH key there):
node bench/fullstack/droplet.mjs r11 --ssh-key <id> [--region sfo2]   # waits for the round
node bench/fullstack/droplet.mjs r11 --ssh-key <id> --detach          # or start it and leave,
node bench/fullstack/droplet.mjs r11 --collect                        # then collect it later
# Or by hand, from the repository root: the images, the fixture, then everything.
docker build -f examples/card-market/Dockerfile -t cb-corten .
for app in nextjs tanstack-start sveltekit react-router leptos django rails laravel aspnet spring phoenix astro nuxt; do
  docker build -t cb-$app bench/fullstack/apps/$app
done
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400   # or any {"item_1": {...}} file
cd bench/fullstack && node run-all.mjs r11 10
node check-app.mjs <app>    # before adding an app: parity, JSON and 404s against corten
```
