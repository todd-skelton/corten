# Full-stack comparison: one marketplace item page in fourteen frameworks

The same item detail page (title, images, sorted listings table, offers with "show all", a
price-alert form that posts and patches a message), and the same item as JSON, built in:

| App | Stack | Directory |
|---|---|---|
| corten | Corten 0.1 (Rust, axum, `html!` views, handlers) | [examples/item-page](../../examples/item-page) |
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

All numbers are measured, from one run (`results/r4/`, 2026-10-23 UTC) on one DigitalOcean c-8
droplet (Intel Xeon Platinum 8168); see [Method](#method). Failed means the app could not do the
test with its usual settings in one CPU and 1 GiB (see caveat 5); its other numbers are kept
where it produced any. r3 (`results/r3/`, a Platinum 8280, Corten before it parsed the item API's
response into typed structs), r2 (`results/r2/`, a Platinum 8280, Corten before handlers replaced
islands and before `corten::Json`) and r1 (`results/r1/`, six frameworks, a Platinum 8168 and
20 s warmups) are kept for the record; times are compared within a round, not across rounds.

### Cold start

| App | Process start to first 200, median (min-max) of 10 | Memory after first request | Image |
|---|---|---|---|
| **corten** | 7.6 ms (6.2 ms-44 ms) | 1.074 MiB | 122 MB |
| leptos | 8.8 ms (7.9 ms-43 ms) | 1.48 MiB | 127 MB |
| sveltekit | 203 ms (189 ms-283 ms) | 23.98 MiB | 333 MB |
| tanstack-start | 244 ms (225 ms-266 ms) | 26.41 MiB | 332 MB |
| react-router | 346 ms (327 ms-472 ms) | 31.84 MiB | 353 MB |
| nuxt | 348 ms (330 ms-425 ms) | 35.29 MiB | 335 MB |
| astro | 409 ms (349 ms-445 ms) | 36.85 MiB | 526 MB |
| nextjs | 793 ms (728 ms-964 ms) | 51.23 MiB | 385 MB |
| aspnet | 860 ms (778 ms-979 ms) | 30.82 MiB | 341 MB |
| laravel | 990 ms (907 ms-1.1 s) | 96.8 MiB | 943 MB |
| django | 1.4 s (1.2 s-1.8 s) | 62.12 MiB | 276 MB |
| rails | 1.4 s (1.2 s-1.9 s) | 63.91 MiB | 431 MB |
| phoenix | 1.6 s (1.5 s-1.8 s) | 125.3 MiB | 262 MB |
| spring | 9.2 s (8.7 s-10.0 s) | 134.6 MiB | 516 MB |

### Item page, items in memory (no database)

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.3 ms / 0.4 ms, 4% | 18,043, 3.2 ms / 12 ms | 17,182, 41 ms, 0% | 20-25 MiB |
| aspnet | 300/s, 0.6 ms / 2.8 ms, 24% | 3,385, 19 ms / 28 ms | 3,577, 116 ms, 0% | 87-90 MiB |
| phoenix | 300/s, 1.1 ms / 1.5 ms, 39% | 1,290, 48 ms / 69 ms | 1,212, 1.9 s, 0% | 280-297 MiB |
| leptos | 300/s, 1.0 ms / 1.3 ms, 26% | 1,282, 49 ms / 75 ms | 1,107, 390 ms, 0% | 16-34 MiB |
| astro | 300/s, 1.0 ms / 34 ms, 32% | 1,097, 52 ms / 132 ms | 996, 355 ms, 0% | 137-138 MiB |
| spring | 300/s, 1.1 ms / 5.1 ms, 44% | 1,050, 45 ms / 362 ms | 1,119, 1.1 s, 0% | 353-373 MiB |
| sveltekit | 300/s, 1.5 ms / 27 ms, 47% | 757, 77 ms / 163 ms | 638, 527 ms, 0% | 144-151 MiB |
| tanstack-start | 300/s, 1.7 ms / 52 ms, 56% | 578, 104 ms / 193 ms | 574, 509 ms, 0% | 172-173 MiB |
| react-router | 300/s, 2.1 ms / 56 ms, 68% | 455, 137 ms / 214 ms | 489, 699 ms, 0% | 137-138 MiB |
| rails | 300/s, 2.6 ms / 57 ms, 81% | 393, 167 ms / 199 ms | 368, 803 ms, 0% | 141-145 MiB |
| nuxt | **failed**: 191/s served, 809 ms / 49.2 s, 100% | 266, 238 ms / 302 ms | 266, 8.3 s, 0.03% | 134-168 MiB |
| django | **failed**: 265/s served, 1.5 s / 4.3 s, 101% | 228, 283 ms / 357 ms | 259, 1.3 s, 0% | 344-344 MiB |
| nextjs | **failed**: 133/s served, 1.5 s / 57.7 s, 100% | 156, 419 ms / 500 ms | 160, 18.4 s, 0% | 217-223 MiB |
| laravel | **failed**: never answered /items/item_1 within a minute | - | - | - |

### The item as JSON, items in memory

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.2 ms / 0.4 ms, 4% | 18,934, 3.0 ms / 12 ms | 19,528, 41 ms, 0% | 20-24 MiB |
| leptos | 300/s, 0.3 ms / 0.4 ms, 5% | 15,952, 3.5 ms / 12 ms | 15,666, 42 ms, 0% | 14-18 MiB |
| aspnet | 300/s, 0.4 ms / 0.6 ms, 16% | 11,715, 5.0 ms / 12 ms | 9,395, 49 ms, 0% | 77-81 MiB |
| spring | 300/s, 0.4 ms / 1.8 ms, 18% | 7,338, 6.1 ms / 78 ms | 7,043, 181 ms, 0% | 358-381 MiB |
| nuxt | 300/s, 0.4 ms / 0.7 ms, 11% | 6,998, 8.1 ms / 36 ms | 6,271, 98 ms, 0% | 96-98 MiB |
| tanstack-start | 300/s, 0.5 ms / 5.4 ms, 13% | 3,416, 15 ms / 51 ms | 3,216, 133 ms, 0% | 151-153 MiB |
| sveltekit | 300/s, 0.5 ms / 4.5 ms, 14% | 3,088, 17 ms / 56 ms | 2,924, 153 ms, 0% | 127-130 MiB |
| react-router | 300/s, 0.7 ms / 9.3 ms, 19% | 2,159, 27 ms / 91 ms | 2,080, 206 ms, 0% | 115-117 MiB |
| phoenix | 300/s, 0.8 ms / 1.2 ms, 29% | 1,839, 33 ms / 56 ms | 1,795, 4.1 s, 0% | 299-325 MiB |
| astro | 300/s, 0.8 ms / 27 ms, 25% | 1,802, 32 ms / 90 ms | 1,810, 212 ms, 0% | 131-132 MiB |
| django | 300/s, 0.9 ms / 1.3 ms, 32% | 1,126, 54 ms / 80 ms | 1,108, 302 ms, 0% | 293-294 MiB |
| nextjs | 300/s, 1.0 ms / 41 ms, 34% | 1,025, 57 ms / 131 ms | 1,042, 334 ms, 0% | 169-170 MiB |
| rails | 300/s, 1.3 ms / 10 ms, 42% | 829, 78 ms / 106 ms | 779, 391 ms, 0% | 130-131 MiB |
| laravel | **failed**: never answered /api/items/item_1 within a minute | - | - | - |

### Item page, item fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 1.0 ms / 1.3 ms, 10% | 2,142, 32 ms / 39 ms | 2,002, 244 ms, 0% | 17-63 MiB |
| aspnet | 150/s, 1.9 ms / 17 ms, 42% | 915, 68 ms / 107 ms | 852, 379 ms, 0% | 71-97 MiB |
| leptos | 150/s, 1.9 ms / 2.7 ms, 25% | 631, 103 ms / 124 ms | 560, 939 ms, 0% | 15-51 MiB |
| spring | 150/s, 3.0 ms / 48 ms, 67% | 433, 146 ms / 268 ms | 344, 1.3 s, 0% | 426-459 MiB |
| astro | 150/s, 2.3 ms / 44 ms, 46% | 418, 156 ms / 193 ms | 418, 729 ms, 0% | 131-147 MiB |
| sveltekit | 150/s, 2.6 ms / 29 ms, 48% | 298, 215 ms / 258 ms | 319, 935 ms, 0% | 127-144 MiB |
| phoenix | 150/s, 3.8 ms / 5.8 ms, 62% | 289, 218 ms / 305 ms | 295, 5.7 s, 0% | 219-313 MiB |
| tanstack-start | 150/s, 3.1 ms / 45 ms, 57% | 283, 226 ms / 284 ms | 271, 4.8 s, 0% | 129-163 MiB |
| rails | 150/s, 4.5 ms / 50 ms, 68% | 260, 249 ms / 301 ms | 231, 1.2 s, 0% | 106-109 MiB |
| react-router | 150/s, 3.8 ms / 146 ms, 67% | 242, 263 ms / 328 ms | 252, 6.0 s, 0% | 135-154 MiB |
| nuxt | 150/s, 13 ms / 482 ms, 83% | 181, 353 ms / 395 ms | 184, 19.8 s, 0% | 135-196 MiB |
| django | 145/s, 1.5 s / 2.1 s, 100% | 149, 426 ms / 529 ms | 146, 2.2 s, 0% | 126-126 MiB |
| nextjs | **failed**: 76/s served, 2.7 s / 54.5 s, 100% | 107, 584 ms / 873 ms | 100, 28.1 s, 0% | 250-256 MiB |
| laravel | **failed**: 73/s served, 13.1 s / 13.8 s, 99% | 73, 873 ms / 1.0 s | 75, 3.8 s, 0% | 124-125 MiB |

### The item as JSON, fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 150/s, 0.9 ms / 1.2 ms, 8% | 2,283, 28 ms / 37 ms | 2,211, 168 ms, 0% | 18-57 MiB |
| aspnet | 150/s, 1.5 ms / 8.3 ms, 30% | 1,205, 53 ms / 83 ms | 1,183, 289 ms, 0% | 58-85 MiB |
| leptos | 150/s, 1.4 ms / 2.1 ms, 17% | 894, 75 ms / 87 ms | 908, 531 ms, 0% | 15-53 MiB |
| spring | 150/s, 1.7 ms / 5.9 ms, 35% | 811, 74 ms / 162 ms | 672, 709 ms, 0% | 415-456 MiB |
| nuxt | 150/s, 1.5 ms / 20 ms, 28% | 715, 89 ms / 143 ms | 628, 598 ms, 0% | 123-140 MiB |
| sveltekit | 150/s, 1.6 ms / 19 ms, 30% | 609, 107 ms / 141 ms | 526, 575 ms, 0% | 126-146 MiB |
| tanstack-start | 150/s, 1.6 ms / 20 ms, 31% | 602, 109 ms / 146 ms | 556, 537 ms, 0% | 120-145 MiB |
| astro | 150/s, 2.0 ms / 33 ms, 40% | 500, 131 ms / 159 ms | 432, 677 ms, 0% | 122-139 MiB |
| react-router | 150/s, 1.9 ms / 32 ms, 38% | 489, 135 ms / 163 ms | 455, 687 ms, 0% | 125-140 MiB |
| phoenix | 150/s, 3.4 ms / 5.5 ms, 55% | 332, 192 ms / 274 ms | 322, 5.6 s, 0% | 218-310 MiB |
| rails | 150/s, 3.0 ms / 33 ms, 50% | 325, 199 ms / 269 ms | 312, 957 ms, 0% | 82-86 MiB |
| nextjs | 150/s, 2.5 ms / 78 ms, 49% | 316, 210 ms / 264 ms | 296, 1.0 s, 0% | 144-183 MiB |
| django | 150/s, 3.6 ms / 5.2 ms, 56% | 250, 257 ms / 295 ms | 259, 1.1 s, 0% | 76-76 MiB |
| laravel | 150/s, 5.4 ms / 32 ms, 75% | 215, 298 ms / 373 ms | 202, 1.4 s, 0% | 101-116 MiB |

### Instructions per request

| App | Item page: user / kernel | JSON: user / kernel |
|---|---|---|
| **corten** | 123,693 / 32,613 | 133,308 / 33,884 |
| aspnet | 1,086,010 / 57,566 | 267,351 / 57,810 |
| astro | 2,049,893 / 81,301 | 1,099,766 / 77,049 |
| phoenix | 2,141,177 / 96,211 | 1,559,797 / 86,141 |
| leptos | 2,725,430 / 36,335 | 204,826 / 32,093 |
| sveltekit | 4,496,151 / 47,414 | 675,318 / 34,015 |
| rails | 5,009,085 / 339,183 | 2,287,167 / 292,264 |
| tanstack-start | 5,320,566 / 51,511 | 514,973 / 33,626 |
| spring | 5,927,435 / 99,222 | 1,271,081 / 97,126 |
| react-router | 6,498,694 / 112,152 | 860,028 / 70,543 |
| nuxt | 9,937,915 / 70,269 | 405,122 / 32,328 |
| django | 12,628,599 / 361,904 | 1,886,028 / 171,493 |
| nextjs | 17,013,925 / 138,743 | 1,718,390 / 59,264 |
| laravel | **failed**: did not start | **failed**: did not start |

### Bytes the item page loads (raw / gzip, KB)

| App | HTML | JavaScript (files) | WASM | CSS |
|---|---|---|---|---|
| **corten** | 11.1 / 2.3 | 15.5 / 6.2 (4) | 0 | 4.6 / 1.6 |
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
| django | 57 (60) ms | 84 ms | 0 ms | 75 ms | 24 (26) ms | 1.5 MB |
| aspnet | 57 (60) ms | 80 ms | 0 ms | 75 ms | 18 (21) ms | 1.5 MB |
| astro | 64 (69) ms | 72 ms | 0 ms | 77 ms | 20 (21) ms | 1.4 MB |
| **corten** | 65 (78) ms | 80 ms | 0 ms | 77 ms | 14 (15) ms | 1.3 MB |
| rails | 72 (74) ms | 80 ms | 0 ms | 91 ms | 30 (55) ms | 1.9 MB |
| spring | 75 (81) ms | 92 ms | 0 ms | 78 ms | 54 (85) ms | 1.5 MB |
| laravel | 86 (97) ms | 76 ms | 0 ms | 89 ms | 27 (29) ms | 2 MB |
| nuxt | 106 (117) ms | 80 ms | 0 ms | 74 ms | 27 (28) ms | 2.7 MB |
| sveltekit | 122 (129) ms | 100 ms | 0 ms | 74 ms | 23 (24) ms | 2.1 MB |
| phoenix | 127 (148) ms | 112 ms | 0 ms | 82 ms | 20 (21) ms | 2.1 MB |
| leptos | 131 (133) ms | 100 ms | 0 ms | 75 ms | 28 (28) ms | 1.7 MB |
| tanstack-start | 137 (145) ms | 84 ms | 0 ms | 82 ms | 31 (33) ms | 2.9 MB |
| nextjs | 166 (181) ms | 88 ms | 1 ms | 87 ms | 110 (135) ms | 3.2 MB |
| react-router | 185 (189) ms | 108 ms | 0 ms | 97 ms | 34 (36) ms | 2.5 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| aspnet | 437 (438) ms | 416 ms | 8 ms | 315 ms | 249 (257) ms | 1.5 MB |
| django | 443 (445) ms | 420 ms | 9 ms | 312 ms | 245 (250) ms | 1.5 MB |
| **corten** | 455 (459) ms | 456 ms | 8 ms | 287 ms | 198 (201) ms | 1.4 MB |
| spring | 615 (616) ms | 432 ms | 10 ms | 303 ms | 246 (257) ms | 1.5 MB |
| astro | 629 (634) ms | 284 ms | 8 ms | 330 ms | 246 (249) ms | 1.4 MB |
| sveltekit | 849 (870) ms | 484 ms | 8 ms | 292 ms | 276 (281) ms | 2.1 MB |
| laravel | 890 (900) ms | 432 ms | 38 ms | 343 ms | 217 (219) ms | 2 MB |
| phoenix | 971 (981) ms | 416 ms | 74 ms | 165 ms | 137 (146) ms | 2.1 MB |
| react-router | 1,076 (1,099) ms | 436 ms | 43 ms | 368 ms | 268 (276) ms | 2.6 MB |
| rails | 1,160 (1,162) ms | 428 ms | 11 ms | 321 ms | 259 (263) ms | 1.9 MB |
| nextjs | 1,435 (1,469) ms | 416 ms | 172 ms | 318 ms | 457 (462) ms | 3.1 MB |
| nuxt | 1,512 (1,525) ms | 444 ms | 81 ms | 290 ms | 292 (299) ms | 2.7 MB |
| leptos | 1,853 (1,863) ms | 428 ms | 85 ms | 304 ms | 261 (264) ms | 1.7 MB |
| tanstack-start | 2,359 (2,392) ms | 432 ms | 81 ms | 308 ms | 352 (357) ms | 2.9 MB |

### What the numbers say

1. Rendering from memory: Corten serves 18,043 item pages a second at 64 users, 5.3x ASP.NET
   Core (3,385), the next fastest, and 14x Leptos (1,282). A page costs it 123,693 user-mode
   instructions; ASP.NET Core 1.09 million, Leptos 2.7 million, Next.js 17 million.
2. JSON from memory: Corten leads with 18,934 a second, then Leptos (15,952) and ASP.NET Core
   (11,715). Corten's JSON costs 133,308 user-mode instructions against Leptos's 204,826, about
   what its HTML page does.
3. With the item API every request fetches and parses about 55 KB of JSON, and that dominates.
   Corten now parses only the fields the page reads (see below) and serves 2,142 pages a second,
   2.3x ASP.NET Core (915) and 3.4x Leptos (631); the rest serve 149-433. As JSON from the item
   API: Corten 2,283, ASP.NET Core 1,205, Leptos 894. At 150 a second Corten's page used 10% of
   its core, ASP.NET Core's 42%.
4. Failed: Next.js could not serve 300 pages a second from memory (133, p99 57.7 s) or 150 from
   the item API (76). Laravel did not start for the in-memory runs: each Octane worker loads the
   508 items into PHP arrays, which PHP's default `memory_limit` (128 MB) does not allow; with the
   item API it served 73 of 150 pages a second (108 in r3) and kept up with the JSON. Django
   (265 of 300, 5.9% never sent) and Nuxt (191, 10.7%) failed the in-memory page test on this
   droplet's slower CPU, where in r3 they passed at 90% and 100% CPU. Django served 145 of 150
   item-API pages a second, inside the rule (3.3% never sent) but at 100% CPU and a median of
   1.5 s.
5. Cold start: Corten and Leptos answer 7.6 ms and 8.8 ms after their process starts and hold
   1.1 MiB and 1.5 MiB after the first request. The Node frameworks take 203-793 ms and
   24-51 MiB; Spring Boot 9.2 s and 135 MiB.
6. Browser: on desktop the server-rendered apps with small scripts (Django, ASP.NET Core, Astro,
   Corten, Rails, Spring) are interactive in 57-75 ms. On the throttled phone Corten is
   interactive in 455 ms, with the htmx apps (437-443 ms), against 1,435 ms for Next.js,
   1,853 ms for Leptos (WASM) and 2,359 ms for TanStack Start. Corten's client navigation is the
   fastest on desktop (14 ms); on the phone Phoenix's is (137 ms against 198 ms), over its
   already-open LiveView socket. Corten's item page loads 6.2 KB of JavaScript (gzip) in four
   files.

r4 ran on a Platinum 8168, slower than r3's and r2's Platinum 8280: every app served fewer
requests a second from memory than in r3 (Corten's page 18,043 against 23,086, ASP.NET Core's
3,385 against 4,519, Leptos's 1,282 against 1,490). Instruction counts did not move with the CPU:
Corten's page 123,693 against 123,665 in r3, Leptos's 2,725,430 against 2,727,190, ASP.NET Core's
1,086,010 against 1,124,519; Spring's JSON 1,271,081 against 1,331,933. So counts compare across
rounds, and requests a second only within one.

### Corten's item API path

Corten's item-API page fell from 1,290 a second in r2 to 1,175 in r3 as the unchanged apps fell
5-22% (ASP.NET Core 1,278 to 1,209, Leptos 915 to 812): r3's droplet, not Corten. What remained
was Corten's own cost. Every request parsed the 55 KB response into a `serde_json::Value` and
then read the view from it, 3.47 million instructions an item in process (callgrind), against
about 124,000 to render the page. ASP.NET Core deserializes into typed records and skips the
members it does not use. `examples/item-page` now deserializes into `ApiItem`, structs of only
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

Instructions are `count.mjs ... api`, median of 3 x 5,000 requests. r4, on its slower droplet,
shows the same ratio: Corten 2,142 item-API pages a second against ASP.NET Core's 915 (2.3x; 2.3x
in the A/B), at 10% of its core at 150 a second against 42%.

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
   8168 at 2.7 GHz in r4, 16 GB), Ubuntu 24.04, kernel 6.8, Docker 29.9, Node 24, created for the run
   and destroyed after it (`droplet.mjs`). Droplets of one size are not all the same CPU (r1's
   and r4's were a Platinum 8168, r2's and r3's a Platinum 8280), so times are compared within a
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
7. Laravel ran with PHP's default `memory_limit` throughout r3 and r4. (r2's main run had raised it
   to 512 MB and measured Laravel again with the default afterwards; `results/r2/laravel-rerun.log`.)
8. r4's droplet had a slower CPU than r3's (Platinum 8168 against 8280). Django and Nuxt, which
   passed r3's in-memory page test near their limit, failed it on r4's; their settings did not
   change. r3's numbers stay in `results/r3/`.
9. r4 measured the tree at `05fcb74`. The `prototype/state` merge (views, handler modules as
   module scripts) and the removal of Solid landed on `main` while it ran and are not in these
   numbers; they change Corten's runtime and the item page's scripts, so its bytes and browser
   rows may differ at the next round.

## Running it

```sh
# On a fresh droplet, created and destroyed for the run (needs doctl and an SSH key there):
node bench/fullstack/droplet.mjs r5 --ssh-key <id> [--region sfo2]
# Or by hand, from the repository root: the images, the fixture, then everything.
docker build -f examples/item-page/Dockerfile -t cb-corten .
for app in nextjs tanstack-start sveltekit react-router leptos django rails laravel aspnet spring phoenix astro nuxt; do
  docker build -t cb-$app bench/fullstack/apps/$app
done
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400   # or any {"item_1": {...}} file
cd bench/fullstack && node run-all.mjs r5 10
node check-app.mjs <app>    # before adding an app: parity, JSON and 404s against corten
```
