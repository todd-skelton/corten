# Full-stack comparison: one marketplace item page in fourteen frameworks

The same item detail page (title, images, sorted listings table, offers with "show all", a
price-alert form that posts and patches a message), and the same item as JSON, built in:

| App | Stack | Directory |
|---|---|---|
| corten | Corten 0.1 (Rust, axum, `html!` views, custom-element islands; measured before handlers replaced them) | [examples/item-page](../../examples/item-page) |
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

All numbers are measured, from one run (`results/r2/`, 2026-10-22) on one DigitalOcean c-8
droplet (Intel Xeon Platinum 8280); see [Method](#method). Failed means the app could not do the
test with its usual settings in one CPU and 1 GiB (see caveat 5); its other numbers are kept
where it produced any. r1 (`results/r1/`, six frameworks, a Platinum 8168 and 20 s warmups) is
kept for the record and is not comparable.

### Cold start

| App | Process start to first 200, median (min-max) of 10 | Memory after first request | Image |
|---|---|---|---|
| **corten** | 6.6 ms (6.1 ms-10 ms) | 1.477 MiB | 122 MB |
| leptos | 7.4 ms (6.1 ms-8.1 ms) | 1.473 MiB | 127 MB |
| sveltekit | 159 ms (156 ms-176 ms) | 23.38 MiB | 333 MB |
| tanstack-start | 195 ms (190 ms-225 ms) | 26.51 MiB | 332 MB |
| react-router | 275 ms (266 ms-344 ms) | 32.01 MiB | 353 MB |
| nuxt | 294 ms (284 ms-316 ms) | 35.28 MiB | 335 MB |
| astro | 311 ms (295 ms-347 ms) | 36.78 MiB | 526 MB |
| nextjs | 627 ms (618 ms-682 ms) | 51.45 MiB | 385 MB |
| aspnet | 702 ms (675 ms-729 ms) | 30.56 MiB | 341 MB |
| laravel | 757 ms (731 ms-838 ms) | 95.41 MiB | 943 MB |
| django | 1.0 s (973 ms-1.6 s) | 63.31 MiB | 276 MB |
| rails | 1.0 s (983 ms-1.1 s) | 63.87 MiB | 431 MB |
| phoenix | 1.2 s (1.2 s-1.9 s) | 126 MiB | 262 MB |
| spring | 7.9 s (7.6 s-8.3 s) | 144.4 MiB | 516 MB |

### Item page, items in memory (no database)

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.3 ms / 0.4 ms, 4% | 23,034, 2.4 ms / 11 ms | 20,948, 37 ms, 0% | 16-20 MiB |
| aspnet | 300/s, 0.5 ms / 1.9 ms, 15% | 4,915, 13 ms / 20 ms | 4,744, 84 ms, 0% | 85-89 MiB |
| phoenix | 300/s, 0.9 ms / 1.1 ms, 29% | 1,986, 32 ms / 45 ms | 1,747, 4.2 s, 0% | 281-298 MiB |
| leptos | 300/s, 0.8 ms / 0.9 ms, 20% | 1,790, 34 ms / 52 ms | 1,471, 283 ms, 0% | 15-34 MiB |
| astro | 300/s, 0.8 ms / 25 ms, 26% | 1,441, 39 ms / 99 ms | 1,375, 257 ms, 0% | 145-148 MiB |
| spring | 300/s, 0.9 ms / 3.6 ms, 32% | 1,289, 35 ms / 298 ms | 1,328, 853 ms, 0% | 357-378 MiB |
| sveltekit | 300/s, 0.9 ms / 14 ms, 32% | 1,104, 51 ms / 106 ms | 980, 290 ms, 0% | 147-151 MiB |
| tanstack-start | 300/s, 1.3 ms / 25 ms, 43% | 755, 76 ms / 142 ms | 760, 385 ms, 0% | 165-166 MiB |
| react-router | 300/s, 1.6 ms / 37 ms, 54% | 602, 98 ms / 170 ms | 612, 497 ms, 0% | 150-152 MiB |
| rails | 300/s, 1.6 ms / 32 ms, 57% | 573, 111 ms / 131 ms | 521, 567 ms, 0% | 135-140 MiB |
| django | 300/s, 2.4 ms / 4.7 ms, 77% | 394, 162 ms / 175 ms | 393, 691 ms, 0% | 344-344 MiB |
| nuxt | 300/s, 19 ms / 97 ms, 89% | 343, 185 ms / 224 ms | 329, 1.5 s, 0% | 134-136 MiB |
| nextjs | **failed**: 163/s served, 1.2 s / 55.9 s, 100% | 197, 337 ms / 384 ms | 196, 1.3 s, 0% | 220-229 MiB |
| laravel | **failed**: never answered /items/item_1 within a minute | - | - | - |

### The item as JSON, items in memory

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| leptos | 300/s, 0.2 ms / 0.4 ms, 4% | 22,847, 2.4 ms / 9.6 ms | 22,914, 35 ms, 0% | 14-18 MiB |
| aspnet | 300/s, 0.3 ms / 0.5 ms, 9% | 16,810, 3.5 ms / 9.2 ms | 15,738, 38 ms, 0% | 78-81 MiB |
| **corten** | 300/s, 0.3 ms / 0.4 ms, 4% | 16,014, 4.1 ms / 11 ms | 16,892, 38 ms, 0% | 15-19 MiB |
| spring | 300/s, 0.3 ms / 0.8 ms, 10% | 10,072, 5.1 ms / 51 ms | 9,780, 247 ms, 0% | 349-375 MiB |
| nuxt | 300/s, 0.4 ms / 0.6 ms, 10% | 8,561, 6.7 ms / 31 ms | 7,954, 79 ms, 0% | 98-100 MiB |
| tanstack-start | 300/s, 0.4 ms / 3.0 ms, 11% | 4,269, 13 ms / 41 ms | 3,896, 111 ms, 0% | 139-143 MiB |
| sveltekit | 300/s, 0.4 ms / 2.7 ms, 12% | 3,873, 13 ms / 44 ms | 3,657, 119 ms, 0% | 124-127 MiB |
| phoenix | 300/s, 0.5 ms / 0.8 ms, 19% | 3,225, 20 ms / 29 ms | 3,097, 2.9 s, 0% | 283-325 MiB |
| react-router | 300/s, 0.6 ms / 3.4 ms, 16% | 2,530, 23 ms / 65 ms | 2,628, 157 ms, 0% | 119-121 MiB |
| astro | 300/s, 0.6 ms / 17 ms, 19% | 2,227, 24 ms / 67 ms | 2,137, 168 ms, 0% | 137-138 MiB |
| django | 300/s, 0.6 ms / 0.9 ms, 24% | 1,514, 41 ms / 53 ms | 1,531, 202 ms, 0% | 294-294 MiB |
| nextjs | 300/s, 0.8 ms / 27 ms, 29% | 1,221, 48 ms / 106 ms | 1,213, 286 ms, 0% | 162-164 MiB |
| rails | 300/s, 0.9 ms / 4.3 ms, 30% | 1,171, 54 ms / 70 ms | 1,192, 266 ms, 0% | 130-131 MiB |
| laravel | **failed**: never answered /api/items/item_1 within a minute | - | - | - |

### Item page, item fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 1.4 ms / 1.9 ms, 15% | 1,290, 49 ms / 60 ms | 1,200, 288 ms, 0% | 18-62 MiB |
| aspnet | 150/s, 1.3 ms / 6.8 ms, 28% | 1,278, 50 ms / 73 ms | 1,156, 271 ms, 0% | 70-97 MiB |
| leptos | 150/s, 1.4 ms / 1.7 ms, 17% | 915, 72 ms / 84 ms | 769, 678 ms, 0% | 15-59 MiB |
| spring | 150/s, 1.8 ms / 6.0 ms, 41% | 610, 103 ms / 191 ms | 480, 943 ms, 0% | 422-455 MiB |
| astro | 150/s, 1.8 ms / 32 ms, 37% | 492, 135 ms / 157 ms | 512, 569 ms, 0% | 129-148 MiB |
| phoenix | 150/s, 2.4 ms / 3.0 ms, 38% | 481, 132 ms / 172 ms | 471, 6.4 s, 0% | 192-301 MiB |
| sveltekit | 150/s, 1.9 ms / 19 ms, 36% | 442, 145 ms / 173 ms | 432, 646 ms, 0% | 144-155 MiB |
| rails | 150/s, 2.6 ms / 24 ms, 43% | 440, 144 ms / 165 ms | 378, 747 ms, 0% | 108-114 MiB |
| tanstack-start | 150/s, 2.3 ms / 29 ms, 43% | 374, 177 ms / 207 ms | 332, 846 ms, 0% | 140-165 MiB |
| react-router | 150/s, 2.7 ms / 34 ms, 51% | 327, 201 ms / 224 ms | 336, 877 ms, 0% | 144-156 MiB |
| nuxt | 150/s, 3.3 ms / 63 ms, 65% | 224, 284 ms / 331 ms | 221, 1.5 s, 0% | 138-191 MiB |
| django | 150/s, 4.4 ms / 6.1 ms, 67% | 221, 287 ms / 332 ms | 202, 1.4 s, 0% | 127-128 MiB |
| nextjs | **failed**: 106/s served, 2.4 s / 46.0 s, 100% | 135, 459 ms / 573 ms | 122, 15.4 s, 0% | 281-286 MiB |
| laravel | **failed**: 124/s served, 5.2 s / 6.8 s, 100% | 125, 481 ms / 780 ms | 127, 2.8 s, 0% | 112-126 MiB |

### The item as JSON, fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| aspnet | 150/s, 1.1 ms / 5.1 ms, 20% | 1,922, 33 ms / 49 ms | 1,800, 187 ms, 0% | 60-85 MiB |
| leptos | 150/s, 1.2 ms / 1.6 ms, 13% | 1,342, 48 ms / 57 ms | 1,263, 298 ms, 0% | 16-56 MiB |
| **corten** | 150/s, 1.1 ms / 1.4 ms, 13% | 1,291, 50 ms / 59 ms | 1,202, 298 ms, 0% | 17-65 MiB |
| spring | 150/s, 1.2 ms / 4.8 ms, 26% | 1,038, 57 ms / 129 ms | 837, 528 ms, 0% | 425-453 MiB |
| nuxt | 150/s, 1.2 ms / 14 ms, 23% | 895, 71 ms / 108 ms | 794, 467 ms, 0% | 122-145 MiB |
| sveltekit | 150/s, 1.3 ms / 13 ms, 24% | 760, 87 ms / 108 ms | 662, 456 ms, 0% | 121-142 MiB |
| tanstack-start | 150/s, 1.3 ms / 16 ms, 25% | 750, 90 ms / 112 ms | 658, 484 ms, 0% | 126-148 MiB |
| react-router | 150/s, 1.5 ms / 24 ms, 31% | 661, 101 ms / 121 ms | 562, 505 ms, 0% | 129-142 MiB |
| astro | 150/s, 1.6 ms / 26 ms, 32% | 625, 105 ms / 127 ms | 578, 501 ms, 0% | 126-150 MiB |
| rails | 150/s, 1.9 ms / 15 ms, 31% | 545, 111 ms / 155 ms | 516, 560 ms, 0% | 83-86 MiB |
| phoenix | 150/s, 2.2 ms / 2.8 ms, 34% | 543, 117 ms / 158 ms | 542, 1.2 s, 0% | 223-360 MiB |
| nextjs | 150/s, 1.8 ms / 45 ms, 37% | 396, 169 ms / 201 ms | 362, 845 ms, 0% | 151-188 MiB |
| django | 150/s, 2.5 ms / 3.2 ms, 38% | 379, 169 ms / 194 ms | 394, 740 ms, 0% | 77-77 MiB |
| laravel | 150/s, 3.2 ms / 8.2 ms, 50% | 309, 181 ms / 326 ms | 307, 1.3 s, 0% | 102-120 MiB |

### Instructions per request

| App | Item page: user / kernel | JSON: user / kernel |
|---|---|---|
| **corten** | 163,908 / 33,522 | 212,201 / 32,931 |
| aspnet | 1,058,000 / 56,670 | 269,111 / 76,574 |
| astro | 2,039,650 / 79,752 | 1,093,725 / 78,031 |
| phoenix | 2,142,641 / 88,450 | 1,550,827 / 77,767 |
| leptos | 2,725,108 / 35,087 | 204,099 / 31,739 |
| sveltekit | 4,512,477 / 44,206 | 676,566 / 33,265 |
| rails | 4,857,062 / 318,584 | 2,283,091 / 281,378 |
| tanstack-start | 5,321,881 / 48,454 | 539,485 / 33,405 |
| spring | 6,030,656 / 97,429 | 1,801,784 / 99,170 |
| react-router | 6,489,483 / 108,479 | 848,339 / 69,738 |
| nuxt | 9,840,704 / 64,298 | 406,861 / 32,366 |
| django | 13,809,615 / 348,281 | 1,997,757 / 171,724 |
| nextjs | 16,989,693 / 127,905 | 1,694,767 / 57,543 |
| laravel | **failed**: did not start | **failed**: did not start |

### Bytes the item page loads (raw / gzip, KB)

| App | HTML | JavaScript (files) | WASM | CSS |
|---|---|---|---|---|
| **corten** | 11.1 / 2.3 | 10.3 / 4.4 (4) | 0 | 4.6 / 1.6 |
| astro | 9.3 / 2.6 | 24.9 / 9.2 (3) | 0 | 0.0 / 0.0 |
| django | 7.3 / 1.8 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| aspnet | 7.3 / 1.9 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| spring | 7.2 / 1.8 | 51.7 / 17.0 (2) | 0 | 2.3 / 0.9 |
| sveltekit | 16.3 / 3.6 | 96.3 / 37.0 (11) | 0 | 1.9 / 0.8 |
| rails | 8.5 / 2.2 | 152.2 / 40.6 (7) | 0 | 2.3 / 0.9 |
| phoenix | 10.0 / 2.4 | 149.2 / 46.2 (1) | 0 | 2.3 / 0.9 |
| nuxt | 17.3 / 4.5 | 197.3 / 73.7 (4) | 0 | 1.9 / 0.8 |
| laravel | 10.8 / 2.8 | 255.8 / 84.3 (1) | 0 | 2.3 / 0.9 |
| react-router | 19.5 / 4.9 | 309.7 / 98.6 (4) | 0 | 1.9 / 0.8 |
| tanstack-start | 17.4 / 4.3 | 342.5 / 110.9 (5) | 0 | 1.9 / 0.8 |
| nextjs | 25.3 / 4.8 | 456.2 / 136.1 (7) | 0 | 1.9 / 0.8 |
| leptos | 17.8 / 3.5 | 24.8 / 7.0 (1) | 719.9 / 286.7 | 2.0 / 0.8 |

### Browser, median of 10 (p90)

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| django | 39 (43) ms | 60 ms | 0 ms | 77 ms | 18 (24) ms | 1.5 MB |
| aspnet | 40 (43) ms | 60 ms | 0 ms | 77 ms | 14 (16) ms | 1.5 MB |
| astro | 45 (48) ms | 52 ms | 0 ms | 77 ms | 16 (17) ms | 1.4 MB |
| **corten** | 49 (56) ms | 56 ms | 0 ms | 76 ms | 8 (9) ms | 1.4 MB |
| rails | 50 (53) ms | 56 ms | 0 ms | 92 ms | 23 (37) ms | 1.9 MB |
| spring | 52 (63) ms | 60 ms | 0 ms | 77 ms | 32 (42) ms | 1.5 MB |
| laravel | 61 (64) ms | 52 ms | 0 ms | 76 ms | 21 (22) ms | 2 MB |
| phoenix | 80 (100) ms | 72 ms | 0 ms | 80 ms | 14 (15) ms | 2.1 MB |
| nuxt | 82 (88) ms | 60 ms | 0 ms | 74 ms | 21 (22) ms | 2.7 MB |
| sveltekit | 89 (96) ms | 68 ms | 0 ms | 75 ms | 19 (19) ms | 2.1 MB |
| leptos | 93 (107) ms | 64 ms | 0 ms | 76 ms | 21 (22) ms | 1.7 MB |
| tanstack-start | 104 (107) ms | 60 ms | 0 ms | 74 ms | 24 (25) ms | 2.9 MB |
| nextjs | 127 (131) ms | 60 ms | 0 ms | 75 ms | 88 (112) ms | 3.2 MB |
| react-router | 131 (143) ms | 88 ms | 0 ms | 74 ms | 26 (29) ms | 2.5 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| aspnet | 427 (428) ms | 396 ms | 0 ms | 286 ms | 236 (243) ms | 1.5 MB |
| django | 433 (440) ms | 396 ms | 0 ms | 271 ms | 237 (241) ms | 1.5 MB |
| **corten** | 441 (448) ms | 432 ms | 0 ms | 285 ms | 184 (187) ms | 1.4 MB |
| spring | 606 (613) ms | 408 ms | 0 ms | 278 ms | 235 (239) ms | 1.5 MB |
| astro | 610 (618) ms | 248 ms | 0 ms | 293 ms | 238 (240) ms | 1.4 MB |
| sveltekit | 827 (830) ms | 464 ms | 0 ms | 279 ms | 264 (265) ms | 2.1 MB |
| laravel | 857 (873) ms | 408 ms | 6 ms | 317 ms | 209 (213) ms | 2 MB |
| phoenix | 924 (941) ms | 400 ms | 27 ms | 133 ms | 115 (124) ms | 2.1 MB |
| react-router | 1,021 (1,029) ms | 412 ms | 20 ms | 330 ms | 246 (254) ms | 2.6 MB |
| rails | 1,144 (1,149) ms | 420 ms | 0 ms | 296 ms | 248 (249) ms | 1.9 MB |
| nextjs | 1,334 (1,354) ms | 396 ms | 122 ms | 296 ms | 427 (434) ms | 3.1 MB |
| nuxt | 1,461 (1,473) ms | 424 ms | 38 ms | 274 ms | 267 (272) ms | 2.7 MB |
| leptos | 1,818 (1,824) ms | 408 ms | 52 ms | 282 ms | 241 (244) ms | 1.7 MB |
| tanstack-start | 2,280 (2,298) ms | 408 ms | 47 ms | 288 ms | 331 (337) ms | 2.9 MB |

### What the numbers say

1. Rendering from memory: Corten serves 23,034 item pages a second at 64 users, 4.7x ASP.NET
   Core (4,915), the next fastest, and 13x Leptos (1,790). A page costs it 164,000 user-mode
   instructions; ASP.NET Core 1.06 million, Leptos 2.7 million, Next.js 17 million.
2. JSON from memory: Leptos (22,847 a second), ASP.NET Core (16,810) and Corten (16,014) lead.
   Corten's JSON costs about the instructions Leptos's does (212,000 against 204,000 user-mode)
   but serves 30% fewer requests with the CPU full in both, and costs more than Corten's own HTML
   page. Why has not been investigated.
3. With the item API every request fetches and parses about 55 KB of JSON, and that dominates:
   Corten (1,290 pages a second) and ASP.NET Core (1,278) are level, Leptos does 915, the rest
   125-610.
4. Failed: Next.js could not serve 300 pages a second from memory (163, p99 56 s) or 150 from
   the item API (106). Laravel did not start for the in-memory runs: each Octane worker loads the
   508 items into PHP arrays, which PHP's default `memory_limit` (128 MB) does not allow; with the
   item API it served 124 of 150 pages a second and kept up with the JSON (150 a second).
5. Cold start: Corten and Leptos answer 6.6 and 7.4 ms after their process starts and hold
   1.5 MiB after the first request. The Node frameworks take 159-627 ms and 23-51 MiB; Spring
   Boot 7.9 s and 144 MiB.
6. Browser: on desktop the server-rendered apps with small scripts (Django, ASP.NET Core, Astro,
   Corten, Rails, Spring) are interactive in 39-52 ms, within noise of each other. On the
   throttled phone Corten is interactive in 441 ms, with the htmx apps (427-433 ms), against
   1,334 ms for Next.js, 1,818 ms for Leptos (WASM) and 2,280 ms for TanStack Start. Corten's
   client navigation is the fastest on desktop (8 ms); on the phone Phoenix's is (115 ms against
   184 ms), over its already-open LiveView socket.

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
   8280 at 2.7 GHz, 16 GB), Ubuntu 24.04, kernel 6.8, Docker 29.9, Node 24, created for the run
   and destroyed after it (`droplet.mjs`). Droplets of one size are not all the same CPU (r1's
   was a Platinum 8168), so numbers are compared within a round, never across rounds.
2. Cores: the app under test pinned to core 1 with 1 GiB of memory, the item API to cores 2-3,
   the load generator (k6) to cores 4-7, the system and Docker on core 0. One app runs at a time.
3. Cold start (`cold-start.mjs`): the container runs a shell that writes the time and execs the
   server; a probe in the same network namespace polls for the first 200 of an item page (items
   from the item API). Docker's own container setup is not in the number. 10 runs each.
4. Server rendering (`run-ssr.mjs`): a 30 s warmup with 32 users (long enough for the JVM's and
   .NET's JITs), then 60 s at a fixed rate (300/s from memory, 150/s from the API), 60 s with 64
   closed-loop users, and 60 s with 256. Items 1 to 500, skewed toward low ids. Responses
   uncompressed. CPU and memory are sampled from `docker stats` each second.
   - Items in memory: a 27.7 MB fixture of 508 items loaded at start, so only the framework is
     measured.
   - Item API: every request fetches the item (about 55 KB of JSON) over HTTP from
     `item-api/server.mjs`, which serves the fixture's responses the way the production API does
     with a warm cache, and parses it.
   - JSON: `GET /api/items/{id}` answers the view model the page renders (about 9 KB), written by
     the framework's usual JSON path. Same lookup as the page; only the output differs.
5. Instructions per request (`count.mjs`): `perf stat` counts the app container's user and
   kernel instructions while 4 users make 5,000 requests (items in memory) after a 30 s warmup;
   median of 3. A count does not depend on what else the machine is doing, so it is steadier than
   time; it does not see waiting (I/O, locks) or how well the instructions run.
6. Browser (`browser.mjs`): every app running at once on the app core, items from the item API,
   one app measured at a time; Playwright
   Chromium, cache disabled, 10 runs per app and profile, each cold load starting on a page
   from another origin. Desktop is unthrottled; mobile is 4x CPU slowdown, 150 ms RTT, 1.6 Mbps down.
   "Interactive" is when the page's own code marks itself ready (hydration finished, islands
   connected, or the framework's load event). Action is fill, submit, and wait for the
   confirmation. Navigation is home page to item page.
7. Bytes (`bytes.mjs`): every script, WASM and stylesheet the item page loads, fetched raw and
   gzipped here at level 6, so server compression settings do not change the comparison.
8. k6 runs with every local port and reuses ports in TIME_WAIT: gunicorn's sync workers close
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
6. r2's images include the live benchmark's routes (`/live/...`), which the item page does not
   touch.
7. Laravel's image in the main r2 run raised PHP's `memory_limit` to 512 MB. Its cold start, load
   and instruction rows were measured again on the same droplet right after the round with the
   default (`results/r2/laravel-rerun.log`). Its parity, bytes and browser rows are from the main
   run; the limit does not change what it serves.

## Running it

```sh
# On a fresh droplet, created and destroyed for the run (needs doctl and an SSH key there):
node bench/fullstack/droplet.mjs r3 --ssh-key <id> [--region sfo2]
# Or by hand, from the repository root: the images, the fixture, then everything.
docker build -f examples/item-page/Dockerfile -t cb-corten .
for app in nextjs tanstack-start sveltekit react-router leptos django rails laravel aspnet spring phoenix astro nuxt; do
  docker build -t cb-$app bench/fullstack/apps/$app
done
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400   # or any {"item_1": {...}} file
cd bench/fullstack && node run-all.mjs r3 10
node check-app.mjs <app>    # before adding an app: parity, JSON and 404s against corten
```
