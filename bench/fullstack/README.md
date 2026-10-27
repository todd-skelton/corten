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

All numbers are measured, from one run (`results/r5/`, 2026-10-25 UTC, the tree at `f80d4a1`) on one
DigitalOcean c-8 droplet (Intel Xeon Platinum 8168); see [Method](#method). Failed means the app could not do the
test with its usual settings in one CPU and 1 GiB (see caveat 5); its other numbers are kept
where it produced any. r4 (`results/r4/`, the same CPU model, Corten at `05fcb74`, before the
JSON derives, the views runtime and the cheaper escape and route parameters), r3 (`results/r3/`, a Platinum 8280, Corten before it parsed the item API's
response into typed structs), r2 (`results/r2/`, a Platinum 8280, Corten before handlers replaced
islands and before `corten::Json`) and r1 (`results/r1/`, six frameworks, a Platinum 8168 and
20 s warmups) are kept for the record; times are compared within a round, not across rounds.

### Cold start

| App | Process start to first 200, median (min-max) of 10 | Memory after first request | Image |
|---|---|---|---|
| **corten** | 6.3 ms (6.1 ms-22 ms) | 1.094 MiB | 122 MB |
| leptos | 8.6 ms (7.7 ms-11 ms) | 1.48 MiB | 127 MB |
| sveltekit | 191 ms (176 ms-200 ms) | 23.85 MiB | 333 MB |
| tanstack-start | 226 ms (211 ms-259 ms) | 26.4 MiB | 332 MB |
| react-router | 323 ms (309 ms-374 ms) | 32.06 MiB | 353 MB |
| nuxt | 335 ms (327 ms-534 ms) | 35.54 MiB | 335 MB |
| astro | 363 ms (330 ms-401 ms) | 36.75 MiB | 526 MB |
| nextjs | 747 ms (722 ms-866 ms) | 51.37 MiB | 385 MB |
| aspnet | 814 ms (746 ms-891 ms) | 30.87 MiB | 341 MB |
| laravel | 998 ms (912 ms-1.1 s) | 96.21 MiB | 943 MB |
| django | 1.2 s (1.1 s-1.6 s) | 62.26 MiB | 276 MB |
| rails | 1.5 s (1.2 s-1.6 s) | 63.88 MiB | 431 MB |
| phoenix | 1.6 s (1.5 s-1.8 s) | 125.7 MiB | 262 MB |
| spring | 8.9 s (8.1 s-9.4 s) | 140.6 MiB | 516 MB |

### Item page, items in memory (no database)

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.3 ms / 0.4 ms, 5% | 21,735, 2.4 ms / 11 ms | 20,026, 40 ms, 0% | 20-25 MiB |
| aspnet | 300/s, 0.6 ms / 2.7 ms, 23% | 3,951, 16 ms / 25 ms | 3,684, 111 ms, 0% | 85-87 MiB |
| leptos | 300/s, 1.0 ms / 1.3 ms, 26% | 1,384, 45 ms / 70 ms | 1,160, 370 ms, 0% | 15-35 MiB |
| phoenix | 300/s, 1.1 ms / 1.4 ms, 38% | 1,346, 47 ms / 65 ms | 1,382, 220 ms, 0% | 282-299 MiB |
| astro | 300/s, 1.0 ms / 32 ms, 32% | 1,225, 46 ms / 120 ms | 1,141, 318 ms, 0% | 140-140 MiB |
| spring | 300/s, 1.1 ms / 6.3 ms, 46% | 981, 44 ms / 455 ms | 1,000, 1.2 s, 0% | 360-381 MiB |
| sveltekit | 300/s, 1.3 ms / 26 ms, 45% | 784, 77 ms / 154 ms | 704, 480 ms, 0% | 149-154 MiB |
| tanstack-start | 300/s, 1.5 ms / 47 ms, 55% | 625, 90 ms / 186 ms | 632, 537 ms, 0% | 163-164 MiB |
| react-router | 300/s, 2.1 ms / 55 ms, 68% | 482, 125 ms / 203 ms | 476, 669 ms, 0% | 139-141 MiB |
| rails | 300/s, 3.9 ms / 83 ms, 85% | 375, 173 ms / 204 ms | 348, 850 ms, 0% | 135-139 MiB |
| django | 271/s, 2.2 s / 3.7 s, 101% | 269, 230 ms / 330 ms | 252, 1.2 s, 0% | 344-345 MiB |
| nuxt | **failed**: 192/s served, 753 ms / 48.9 s, 100% | 304, 210 ms / 255 ms | 284, 2.3 s, 0% | 139-145 MiB |
| nextjs | **failed**: 141/s served, 1.4 s / 57.7 s, 100% | 165, 396 ms / 476 ms | 167, 12.4 s, 0% | 215-221 MiB |
| laravel | **failed**: never answered /items/item_1 within a minute | - | - | - |

### The item as JSON, items in memory

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.2 ms / 0.4 ms, 4% | 25,200, 2.0 ms / 11 ms | 22,488, 37 ms, 0% | 20-24 MiB |
| leptos | 300/s, 0.3 ms / 0.4 ms, 5% | 17,036, 3.2 ms / 11 ms | 19,048, 39 ms, 0% | 14-18 MiB |
| aspnet | 300/s, 0.3 ms / 0.5 ms, 15% | 12,311, 4.8 ms / 12 ms | 11,890, 43 ms, 0% | 77-80 MiB |
| spring | 300/s, 0.3 ms / 1.2 ms, 11% | 7,698, 6.0 ms / 73 ms | 7,645, 268 ms, 0% | 349-374 MiB |
| nuxt | 300/s, 0.4 ms / 0.6 ms, 10% | 6,978, 8.0 ms / 36 ms | 6,757, 92 ms, 0% | 98-100 MiB |
| tanstack-start | 300/s, 0.5 ms / 4.8 ms, 14% | 3,687, 14 ms / 48 ms | 3,369, 128 ms, 0% | 144-146 MiB |
| sveltekit | 300/s, 0.5 ms / 4.3 ms, 14% | 3,381, 15 ms / 51 ms | 3,314, 140 ms, 0% | 125-127 MiB |
| react-router | 300/s, 0.7 ms / 2.3 ms, 19% | 2,364, 24 ms / 88 ms | 2,326, 194 ms, 0% | 113-116 MiB |
| phoenix | 300/s, 0.7 ms / 1.0 ms, 27% | 1,944, 32 ms / 50 ms | 1,874, 3.0 s, 0% | 287-346 MiB |
| astro | 300/s, 0.7 ms / 24 ms, 23% | 1,852, 30 ms / 87 ms | 1,917, 199 ms, 0% | 139-140 MiB |
| django | 300/s, 0.8 ms / 1.2 ms, 30% | 1,125, 54 ms / 76 ms | 1,144, 283 ms, 0% | 294-294 MiB |
| nextjs | 300/s, 0.9 ms / 38 ms, 33% | 1,086, 53 ms / 131 ms | 1,087, 317 ms, 0% | 170-171 MiB |
| rails | 300/s, 1.3 ms / 11 ms, 42% | 877, 72 ms / 101 ms | 852, 383 ms, 0% | 129-130 MiB |
| laravel | **failed**: never answered /api/items/item_1 within a minute | - | - | - |

### Item page, item fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.9 ms / 1.1 ms, 8% | 3,240, 19 ms / 27 ms | 3,072, 126 ms, 0% | 18-59 MiB |
| aspnet | 150/s, 1.9 ms / 17 ms, 41% | 998, 61 ms / 103 ms | 892, 363 ms, 0% | 70-98 MiB |
| leptos | 150/s, 1.9 ms / 2.7 ms, 25% | 670, 96 ms / 121 ms | 590, 766 ms, 0% | 16-56 MiB |
| spring | 150/s, 3.4 ms / 50 ms, 78% | 435, 143 ms / 274 ms | 362, 1.3 s, 0% | 423-457 MiB |
| astro | 150/s, 2.3 ms / 41 ms, 46% | 407, 161 ms / 207 ms | 395, 755 ms, 0% | 127-149 MiB |
| sveltekit | 150/s, 2.5 ms / 27 ms, 48% | 321, 199 ms / 248 ms | 314, 928 ms, 0% | 143-158 MiB |
| phoenix | 150/s, 3.5 ms / 5.8 ms, 60% | 305, 210 ms / 304 ms | 296, 5.8 s, 0% | 212-309 MiB |
| tanstack-start | 150/s, 3.0 ms / 45 ms, 55% | 288, 223 ms / 266 ms | 276, 1.1 s, 0% | 139-169 MiB |
| rails | 150/s, 4.3 ms / 47 ms, 67% | 260, 249 ms / 299 ms | 240, 1.2 s, 0% | 107-113 MiB |
| react-router | 150/s, 3.7 ms / 154 ms, 65% | 244, 262 ms / 323 ms | 257, 4.7 s, 0% | 133-151 MiB |
| nuxt | 150/s, 15 ms / 788 ms, 86% | 187, 340 ms / 386 ms | 184, 19.1 s, 0% | 134-186 MiB |
| django | 146/s, 1.1 s / 1.5 s, 101% | 162, 380 ms / 542 ms | 150, 2.1 s, 0% | 126-126 MiB |
| nextjs | **failed**: 77/s served, 2.6 s / 53.7 s, 100% | 107, 587 ms / 912 ms | 99, 29.1 s, 0% | 253-256 MiB |
| laravel | **failed**: 78/s served, 11.9 s / 13.4 s, 100% | 81, 781 ms / 988 ms | 78, 3.6 s, 0% | 121-126 MiB |

### The item as JSON, fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.8 ms / 1.1 ms, 7% | 3,787, 16 ms / 26 ms | 3,425, 123 ms, 0% | 18-60 MiB |
| aspnet | 150/s, 1.5 ms / 9.5 ms, 30% | 1,317, 47 ms / 80 ms | 1,221, 280 ms, 0% | 58-87 MiB |
| leptos | 150/s, 1.5 ms / 2.1 ms, 17% | 916, 73 ms / 88 ms | 859, 432 ms, 0% | 15-54 MiB |
| spring | 150/s, 1.6 ms / 5.8 ms, 31% | 823, 74 ms / 156 ms | 693, 701 ms, 0% | 417-456 MiB |
| nuxt | 150/s, 1.6 ms / 18 ms, 28% | 746, 86 ms / 134 ms | 678, 559 ms, 0% | 121-138 MiB |
| sveltekit | 150/s, 1.7 ms / 17 ms, 30% | 624, 104 ms / 137 ms | 562, 564 ms, 0% | 123-141 MiB |
| tanstack-start | 150/s, 1.7 ms / 21 ms, 32% | 603, 110 ms / 145 ms | 532, 585 ms, 0% | 122-143 MiB |
| react-router | 150/s, 2.0 ms / 31 ms, 40% | 505, 129 ms / 158 ms | 458, 657 ms, 0% | 121-137 MiB |
| astro | 150/s, 2.1 ms / 33 ms, 40% | 497, 131 ms / 161 ms | 459, 659 ms, 0% | 123-138 MiB |
| rails | 150/s, 3.1 ms / 31 ms, 50% | 336, 188 ms / 268 ms | 352, 898 ms, 0% | 82-84 MiB |
| phoenix | 150/s, 3.1 ms / 5.4 ms, 52% | 330, 190 ms / 272 ms | 341, 5.8 s, 0% | 227-312 MiB |
| nextjs | 150/s, 2.4 ms / 65 ms, 48% | 312, 214 ms / 258 ms | 295, 3.5 s, 0% | 149-189 MiB |
| django | 150/s, 3.5 ms / 5.3 ms, 55% | 265, 245 ms / 295 ms | 272, 1.1 s, 0% | 76-76 MiB |
| laravel | 150/s, 5.0 ms / 23 ms, 71% | 227, 283 ms / 348 ms | 197, 1.4 s, 0% | 100-115 MiB |

### Instructions per request

| App | Item page: user / kernel | JSON: user / kernel |
|---|---|---|
| **corten** | 103,877 / 33,530 | 69,291 / 33,525 |
| aspnet | 1,259,416 / 58,261 | 268,810 / 60,107 |
| astro | 2,057,084 / 80,867 | 1,092,856 / 77,303 |
| phoenix | 2,143,013 / 95,676 | 1,561,003 / 85,759 |
| leptos | 2,724,688 / 36,016 | 204,910 / 33,423 |
| sveltekit | 4,469,276 / 47,144 | 682,489 / 33,662 |
| rails | 5,010,004 / 339,334 | 2,278,145 / 295,479 |
| tanstack-start | 5,312,995 / 50,663 | 537,769 / 33,672 |
| spring | 5,698,927 / 99,663 | 1,212,492 / 96,484 |
| react-router | 6,440,693 / 110,824 | 865,133 / 70,622 |
| nuxt | 9,810,514 / 67,459 | 425,232 / 32,319 |
| django | 12,641,283 / 356,389 | 1,896,608 / 172,384 |
| nextjs | 17,085,250 / 135,451 | 1,696,172 / 58,526 |
| laravel | **failed**: did not start | **failed**: did not start |

### Bytes the item page loads (raw / gzip, KB)

| App | HTML | JavaScript (files) | WASM | CSS |
|---|---|---|---|---|
| **corten** | 11.1 / 2.3 | 18.4 / 7.3 (4) | 0 | 4.6 / 1.6 |
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
| tanstack-start | 17.4 / 4.3 | 342.6 / 110.9 (5) | 0 | 1.9 / 0.8 |
| nextjs | 25.3 / 4.8 | 456.2 / 136.1 (7) | 0 | 1.9 / 0.8 |
| leptos | 17.8 / 3.5 | 24.8 / 7.0 (1) | 719.9 / 286.7 | 2.0 / 0.8 |

### Browser, median of 10 (p90)

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| aspnet | 55 (57) ms | 80 ms | 0 ms | 76 ms | 18 (20) ms | 1.5 MB |
| django | 57 (59) ms | 80 ms | 0 ms | 75 ms | 22 (27) ms | 1.5 MB |
| astro | 63 (69) ms | 72 ms | 0 ms | 75 ms | 19 (21) ms | 1.4 MB |
| **corten** | 65 (69) ms | 76 ms | 0 ms | 75 ms | 13 (14) ms | 1.4 MB |
| rails | 69 (72) ms | 76 ms | 0 ms | 89 ms | 29 (38) ms | 1.9 MB |
| spring | 69 (70) ms | 80 ms | 0 ms | 75 ms | 36 (80) ms | 1.5 MB |
| laravel | 83 (89) ms | 72 ms | 0 ms | 90 ms | 26 (28) ms | 2 MB |
| nuxt | 101 (116) ms | 76 ms | 0 ms | 72 ms | 25 (26) ms | 2.7 MB |
| sveltekit | 110 (116) ms | 84 ms | 0 ms | 75 ms | 22 (24) ms | 2.1 MB |
| leptos | 115 (136) ms | 92 ms | 0 ms | 73 ms | 26 (28) ms | 1.7 MB |
| phoenix | 120 (125) ms | 96 ms | 0 ms | 82 ms | 19 (20) ms | 2.1 MB |
| tanstack-start | 127 (135) ms | 80 ms | 0 ms | 82 ms | 29 (31) ms | 2.9 MB |
| nextjs | 162 (169) ms | 80 ms | 1 ms | 75 ms | 127 (155) ms | 3.2 MB |
| react-router | 167 (171) ms | 116 ms | 0 ms | 97 ms | 32 (35) ms | 2.5 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| aspnet | 434 (435) ms | 412 ms | 10 ms | 301 ms | 243 (254) ms | 1.5 MB |
| django | 443 (446) ms | 412 ms | 2 ms | 295 ms | 244 (248) ms | 1.5 MB |
| **corten** | 450 (465) ms | 456 ms | 4 ms | 284 ms | 198 (201) ms | 1.4 MB |
| spring | 612 (616) ms | 428 ms | 5 ms | 293 ms | 242 (244) ms | 1.5 MB |
| astro | 625 (631) ms | 276 ms | 0 ms | 314 ms | 247 (251) ms | 1.4 MB |
| sveltekit | 845 (863) ms | 480 ms | 4 ms | 288 ms | 269 (273) ms | 2.1 MB |
| laravel | 877 (880) ms | 420 ms | 16 ms | 348 ms | 217 (219) ms | 2 MB |
| phoenix | 948 (973) ms | 412 ms | 60 ms | 147 ms | 132 (137) ms | 2.1 MB |
| react-router | 1,061 (1,080) ms | 436 ms | 37 ms | 354 ms | 255 (271) ms | 2.6 MB |
| rails | 1,151 (1,158) ms | 424 ms | 6 ms | 311 ms | 254 (259) ms | 1.9 MB |
| nextjs | 1,412 (1,424) ms | 408 ms | 165 ms | 301 ms | 448 (456) ms | 3.1 MB |
| nuxt | 1,482 (1,528) ms | 440 ms | 54 ms | 281 ms | 282 (292) ms | 2.7 MB |
| leptos | 1,849 (1,861) ms | 424 ms | 81 ms | 301 ms | 249 (253) ms | 1.7 MB |
| tanstack-start | 2,324 (2,327) ms | 420 ms | 63 ms | 287 ms | 345 (354) ms | 2.9 MB |

### What the numbers say

1. Rendering from memory: Corten serves 21,735 item pages a second at 64 users, 5.5x ASP.NET
   Core (3,951), the next fastest, and 16x Leptos (1,384). A page costs it 103,877 user-mode
   instructions; ASP.NET Core 1.26 million, Leptos 2.7 million, Next.js 17 million.
2. JSON from memory: Corten leads with 25,200 a second, then Leptos (17,036) and ASP.NET Core
   (12,311). Corten's JSON costs 69,291 user-mode instructions, a third of Leptos's (204,910)
   and less than its own HTML page.
3. With the item API every request fetches and parses about 55 KB of JSON, and that dominates.
   Corten parses only the fields the page reads (see below) and serves 3,240 pages a second,
   3.2x ASP.NET Core (998) and 4.8x Leptos (670); the rest serve 162-435. As JSON from the item
   API: Corten 3,787, ASP.NET Core 1,317, Leptos 916. At 150 a second Corten's page used 8% of
   its core, ASP.NET Core's 41%.
4. Failed: Next.js could not serve 300 pages a second from memory (141, p99 57.7 s) or 150 from
   the item API (77). Laravel did not start for the in-memory runs: each Octane worker loads the
   508 items into PHP arrays, which PHP's default `memory_limit` (128 MB) does not allow; with the
   item API it served 78 of 150 pages a second and kept up with the JSON. Nuxt failed the
   in-memory page test again (192 of 300, 8.4% never sent). Django served 271 of 300 in-memory
   pages a second, inside the rule (4.1% never sent, r4 5.9%) but at 100% CPU and a median of
   2.2 s, and 146 of 150 item-API pages a second at 100% CPU.
5. Cold start: Corten and Leptos answer 6.3 ms and 8.6 ms after their process starts and hold
   1.1 MiB and 1.5 MiB after the first request. The Node frameworks take 191-747 ms and
   24-51 MiB; Spring Boot 8.9 s and 141 MiB.
6. Browser: on desktop the server-rendered apps with small scripts (ASP.NET Core, Django, Astro,
   Corten, Rails, Spring) are interactive in 55-69 ms. On the throttled phone Corten is
   interactive in 450 ms, with the htmx apps (434-443 ms), against 1,412 ms for Next.js,
   1,849 ms for Leptos (WASM) and 2,324 ms for TanStack Start. Corten's client navigation is the
   fastest on desktop (13 ms); on the phone Phoenix's is (132 ms against 198 ms), over its
   already-open LiveView socket. Corten's item page loads 7.3 KB of JavaScript (gzip) in four
   files, against 6.2 KB in r4: the views runtime landed between the two rounds.

r5 ran on the same CPU model as r4 (Platinum 8168), with every app but Corten unchanged, and
the unchanged apps still moved: ASP.NET Core's page from memory 3,951 a second against 3,385
(+17%), Leptos's 1,384 against 1,282 (+8%), Spring's 981 against 1,050 (-7%). So part of Corten's
change is the droplet: its page from memory rose 20% (18,043 to 21,735), about as much as ASP.NET
Core's. Its other rows rose more than any unchanged app's: JSON from memory 33% (18,934 to
25,200), the item-API page 51% (2,142 to 3,240) and its JSON 66% (2,283 to 3,787). Instruction
counts, which do not depend on the droplet, fell 16% for the page (123,693 to 103,877) and 48% for
the JSON (133,308 to 69,291); Leptos's page held (2,724,688 against 2,725,430). Counts compare
across rounds, and requests a second only within one.

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

Instructions are `count.mjs ... api`, median of 3 x 5,000 requests. r5, on a slower
droplet and with Corten's JSON derives, serves 3,240 item-API pages a second against ASP.NET Core's
998 (3.2x; 2.3x in the A/B and in r4), at 8% of its core at 150 a second against 41%.

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
   8168 at 2.7 GHz in r4 and r5, 16 GB), Ubuntu 24.04, kernel 6.8, Docker 29.9, Node 24, created for the run
   and destroyed after it (`droplet.mjs`). Droplets of one size are not all the same CPU (r1's,
   r4's and r5's were a Platinum 8168, r2's and r3's a Platinum 8280), so times are compared within a
   round, never across rounds.
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
   `node count.mjs <round> 3 <apps> page,json api` counts with items from the item API instead,
   so the count includes fetching and parsing each 55 KB response (keys `<app>/<target>/api`).
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
6. The images include the live benchmark's routes (`/live/...`), which the item page does not
   touch.
7. Laravel ran with PHP's default `memory_limit` throughout r3, r4 and r5. (r2's main run had raised it
   to 512 MB and measured Laravel again with the default afterwards; `results/r2/laravel-rerun.log`.)
8. r4's droplet had a slower CPU than r3's (Platinum 8168 against 8280). Django and Nuxt, which
   passed r3's in-memory page test near their limit, failed it on r4's; their settings did not
   change. r3's numbers stay in `results/r3/`. In r5, on the same CPU model as r4, Django passed
   it again (4.1% never sent) and Nuxt did not.
9. r5 measured the tree at `f80d4a1` (`results/r5/commit.txt`). Changes merged to `main` after
   that are not in these numbers.

## Running it

```sh
# On a fresh droplet, created and destroyed for the run (needs doctl and an SSH key there):
node bench/fullstack/droplet.mjs r6 --ssh-key <id> [--region sfo2]
# Or by hand, from the repository root: the images, the fixture, then everything.
docker build -f examples/card-market/Dockerfile -t cb-corten .
for app in nextjs tanstack-start sveltekit react-router leptos django rails laravel aspnet spring phoenix astro nuxt; do
  docker build -t cb-$app bench/fullstack/apps/$app
done
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400   # or any {"item_1": {...}} file
cd bench/fullstack && node run-all.mjs r6 10
node check-app.mjs <app>    # before adding an app: parity, JSON and 404s against corten
```
