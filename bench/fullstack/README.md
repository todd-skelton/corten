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

All numbers are measured, from one run (`results/r3/`, 2026-10-23 UTC) on one DigitalOcean c-8
droplet (Intel Xeon Platinum 8280); see [Method](#method). Failed means the app could not do the
test with its usual settings in one CPU and 1 GiB (see caveat 5); its other numbers are kept
where it produced any. r2 (`results/r2/`, the same CPU model, Corten before handlers replaced
islands and before `corten::Json`) and r1 (`results/r1/`, six frameworks, a Platinum 8168 and
20 s warmups) are kept for the record; times are compared within a round, not across rounds.

### Cold start

| App | Process start to first 200, median (min-max) of 10 | Memory after first request | Image |
|---|---|---|---|
| **corten** | 7.5 ms (6.3 ms-11 ms) | 1.484 MiB | 122 MB |
| leptos | 7.5 ms (6.3 ms-8.8 ms) | 1.48 MiB | 127 MB |
| sveltekit | 175 ms (164 ms-230 ms) | 23.78 MiB | 333 MB |
| tanstack-start | 212 ms (207 ms-258 ms) | 26.77 MiB | 332 MB |
| react-router | 301 ms (288 ms-405 ms) | 32.09 MiB | 353 MB |
| nuxt | 313 ms (295 ms-358 ms) | 35.61 MiB | 335 MB |
| astro | 327 ms (314 ms-474 ms) | 37.09 MiB | 526 MB |
| nextjs | 704 ms (683 ms-752 ms) | 51.36 MiB | 385 MB |
| aspnet | 726 ms (705 ms-738 ms) | 30.56 MiB | 341 MB |
| laravel | 845 ms (820 ms-885 ms) | 95.34 MiB | 943 MB |
| rails | 1.1 s (1.1 s-1.1 s) | 63.92 MiB | 431 MB |
| django | 1.1 s (1.1 s-1.1 s) | 63.37 MiB | 276 MB |
| phoenix | 1.3 s (1.3 s-1.4 s) | 125.2 MiB | 262 MB |
| spring | 8.2 s (8.0 s-8.3 s) | 136.1 MiB | 516 MB |

### Item page, items in memory (no database)

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.2 ms / 0.4 ms, 4% | 23,086, 2.2 ms / 11 ms | 20,106, 38 ms, 0% | 16-20 MiB |
| aspnet | 300/s, 0.5 ms / 2.4 ms, 16% | 4,519, 14 ms / 21 ms | 4,290, 93 ms, 0% | 85-88 MiB |
| phoenix | 300/s, 0.9 ms / 1.2 ms, 32% | 1,683, 37 ms / 52 ms | 1,600, 4.2 s, 0% | 289-298 MiB |
| leptos | 300/s, 0.9 ms / 1.1 ms, 23% | 1,490, 42 ms / 62 ms | 1,283, 327 ms, 0% | 15-32 MiB |
| astro | 300/s, 0.9 ms / 30 ms, 29% | 1,303, 43 ms / 111 ms | 1,257, 269 ms, 0% | 148-149 MiB |
| spring | 300/s, 1.0 ms / 5.1 ms, 39% | 1,192, 39 ms / 292 ms | 1,162, 817 ms, 0% | 358-378 MiB |
| sveltekit | 300/s, 1.0 ms / 18 ms, 35% | 1,028, 54 ms / 117 ms | 905, 309 ms, 0% | 142-149 MiB |
| tanstack-start | 300/s, 1.3 ms / 29 ms, 46% | 727, 80 ms / 144 ms | 728, 414 ms, 0% | 160-161 MiB |
| react-router | 300/s, 1.7 ms / 44 ms, 59% | 565, 104 ms / 174 ms | 561, 521 ms, 0% | 146-147 MiB |
| rails | 300/s, 1.8 ms / 32 ms, 62% | 541, 118 ms / 135 ms | 487, 581 ms, 0% | 135-141 MiB |
| django | 300/s, 2.9 ms / 15 ms, 90% | 335, 191 ms / 209 ms | 339, 818 ms, 0% | 343-344 MiB |
| nuxt | 292/s, 724 ms / 14.7 s, 100% | 318, 200 ms / 251 ms | 304, 861 ms, 0% | 158-161 MiB |
| nextjs | **failed**: 155/s served, 1.3 s / 56.9 s, 100% | 197, 340 ms / 384 ms | 192, 1.4 s, 0% | 217-230 MiB |
| laravel | **failed**: never answered /items/item_1 within a minute | - | - | - |

### The item as JSON, items in memory

| App | 300/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| **corten** | 300/s, 0.3 ms / 0.4 ms, 4% | 24,338, 2.0 ms / 11 ms | 21,933, 38 ms, 0% | 15-19 MiB |
| leptos | 300/s, 0.2 ms / 0.4 ms, 4% | 20,887, 2.6 ms / 10 ms | 19,702, 38 ms, 0% | 14-18 MiB |
| aspnet | 300/s, 0.3 ms / 0.5 ms, 10% | 14,624, 4.1 ms / 11 ms | 12,633, 43 ms, 0% | 77-84 MiB |
| spring | 300/s, 0.3 ms / 1.0 ms, 10% | 8,289, 5.8 ms / 68 ms | 8,040, 165 ms, 0% | 344-369 MiB |
| nuxt | 300/s, 0.4 ms / 0.6 ms, 9% | 8,214, 6.9 ms / 33 ms | 7,600, 82 ms, 0% | 97-99 MiB |
| tanstack-start | 300/s, 0.4 ms / 4.1 ms, 13% | 3,588, 14 ms / 47 ms | 3,409, 122 ms, 0% | 139-141 MiB |
| sveltekit | 300/s, 0.4 ms / 3.6 ms, 13% | 3,429, 14 ms / 48 ms | 3,382, 131 ms, 0% | 126-128 MiB |
| phoenix | 300/s, 0.6 ms / 0.8 ms, 22% | 2,669, 24 ms / 35 ms | 2,632, 3.4 s, 0% | 297-330 MiB |
| react-router | 300/s, 0.6 ms / 4.5 ms, 17% | 2,502, 23 ms / 71 ms | 2,478, 164 ms, 0% | 117-119 MiB |
| astro | 300/s, 0.7 ms / 21 ms, 21% | 2,001, 28 ms / 77 ms | 2,014, 180 ms, 0% | 135-137 MiB |
| django | 300/s, 0.7 ms / 1.0 ms, 27% | 1,309, 47 ms / 62 ms | 1,314, 235 ms, 0% | 294-295 MiB |
| rails | 300/s, 1.0 ms / 5.4 ms, 32% | 1,117, 56 ms / 75 ms | 1,074, 284 ms, 0% | 129-131 MiB |
| nextjs | 300/s, 1.0 ms / 35 ms, 34% | 1,103, 53 ms / 121 ms | 1,152, 309 ms, 0% | 171-172 MiB |
| laravel | **failed**: never answered /api/items/item_1 within a minute | - | - | - |

### Item page, item fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| aspnet | 150/s, 1.6 ms / 7.8 ms, 32% | 1,209, 52 ms / 76 ms | 1,063, 285 ms, 0% | 69-99 MiB |
| **corten** | 150/s, 1.3 ms / 1.8 ms, 15% | 1,175, 55 ms / 64 ms | 1,095, 434 ms, 0% | 17-67 MiB |
| leptos | 150/s, 1.7 ms / 2.1 ms, 21% | 812, 80 ms / 95 ms | 690, 490 ms, 0% | 15-54 MiB |
| spring | 150/s, 2.4 ms / 11 ms, 57% | 494, 128 ms / 233 ms | 410, 1.1 s, 0% | 431-456 MiB |
| astro | 150/s, 2.1 ms / 36 ms, 41% | 460, 143 ms / 172 ms | 440, 679 ms, 0% | 128-147 MiB |
| phoenix | 150/s, 2.8 ms / 3.6 ms, 44% | 413, 155 ms / 199 ms | 411, 7.3 s, 0% | 198-294 MiB |
| rails | 150/s, 2.9 ms / 28 ms, 48% | 399, 160 ms / 178 ms | 346, 800 ms, 0% | 108-113 MiB |
| sveltekit | 150/s, 2.6 ms / 26 ms, 47% | 373, 172 ms / 204 ms | 369, 753 ms, 0% | 145-157 MiB |
| react-router | 150/s, 3.1 ms / 41 ms, 57% | 294, 222 ms / 257 ms | 302, 1.1 s, 0% | 140-152 MiB |
| tanstack-start | 150/s, 3.3 ms / 45 ms, 59% | 292, 220 ms / 254 ms | 285, 2.4 s, 0% | 125-159 MiB |
| nuxt | 150/s, 4.5 ms / 241 ms, 74% | 212, 301 ms / 344 ms | 209, 13.9 s, 0% | 131-187 MiB |
| django | 150/s, 5.1 ms / 7.3 ms, 77% | 183, 349 ms / 383 ms | 181, 1.5 s, 0% | 127-128 MiB |
| nextjs | **failed**: 81/s served, 2.5 s / 52.0 s, 100% | 116, 528 ms / 736 ms | 104, 18.6 s, 0% | 207-261 MiB |
| laravel | **failed**: 108/s served, 6.7 s / 9.4 s, 100% | 109, 583 ms / 641 ms | 108, 2.5 s, 0% | 121-126 MiB |

### The item as JSON, fetched from the item API

| App | 150/s offered: served, p50 / p99, CPU | 64 users: req/s, p50 / p99 | 256 users: req/s, p99, errors | Memory under load |
|---|---|---|---|---|
| aspnet | 150/s, 1.1 ms / 5.3 ms, 19% | 1,725, 36 ms / 55 ms | 1,564, 205 ms, 0% | 59-87 MiB |
| **corten** | 150/s, 1.3 ms / 1.7 ms, 15% | 1,227, 52 ms / 64 ms | 1,167, 301 ms, 0% | 18-62 MiB |
| leptos | 150/s, 1.2 ms / 1.6 ms, 14% | 1,223, 52 ms / 61 ms | 1,151, 327 ms, 0% | 15-57 MiB |
| spring | 150/s, 1.6 ms / 5.7 ms, 32% | 870, 69 ms / 151 ms | 760, 621 ms, 0% | 419-444 MiB |
| nuxt | 150/s, 1.3 ms / 15 ms, 24% | 822, 78 ms / 120 ms | 711, 524 ms, 0% | 123-139 MiB |
| tanstack-start | 150/s, 1.5 ms / 18 ms, 28% | 704, 97 ms / 120 ms | 602, 504 ms, 0% | 124-147 MiB |
| sveltekit | 150/s, 1.5 ms / 16 ms, 29% | 661, 100 ms / 120 ms | 609, 488 ms, 0% | 120-138 MiB |
| react-router | 150/s, 1.7 ms / 27 ms, 34% | 598, 111 ms / 134 ms | 520, 562 ms, 0% | 119-135 MiB |
| astro | 150/s, 1.7 ms / 27 ms, 35% | 537, 121 ms / 155 ms | 494, 663 ms, 0% | 121-139 MiB |
| phoenix | 150/s, 2.5 ms / 3.2 ms, 39% | 483, 132 ms / 170 ms | 487, 615 ms, 0% | 215-344 MiB |
| rails | 150/s, 2.1 ms / 21 ms, 35% | 480, 126 ms / 174 ms | 482, 596 ms, 0% | 83-87 MiB |
| django | 150/s, 2.8 ms / 3.7 ms, 43% | 362, 176 ms / 199 ms | 360, 790 ms, 0% | 77-77 MiB |
| nextjs | 150/s, 2.2 ms / 54 ms, 42% | 353, 191 ms / 222 ms | 334, 883 ms, 0% | 150-187 MiB |
| laravel | 150/s, 3.8 ms / 9.3 ms, 54% | 265, 208 ms / 517 ms | 263, 1.8 s, 0% | 101-117 MiB |

### Instructions per request

| App | Item page: user / kernel | JSON: user / kernel |
|---|---|---|
| **corten** | 123,665 / 32,752 | 133,588 / 32,847 |
| aspnet | 1,124,519 / 57,874 | 268,678 / 62,010 |
| astro | 2,046,519 / 79,717 | 1,077,957 / 76,848 |
| phoenix | 2,135,210 / 90,596 | 1,552,304 / 80,095 |
| leptos | 2,727,190 / 35,225 | 205,426 / 33,300 |
| sveltekit | 4,449,579 / 44,759 | 678,390 / 33,309 |
| rails | 5,015,899 / 325,415 | 2,274,046 / 285,128 |
| tanstack-start | 5,314,388 / 49,223 | 524,916 / 33,405 |
| spring | 5,875,145 / 96,633 | 1,331,933 / 99,792 |
| react-router | 6,445,628 / 108,894 | 860,547 / 69,819 |
| nuxt | 9,953,938 / 66,396 | 424,465 / 32,257 |
| django | 13,796,881 / 350,952 | 2,002,354 / 171,909 |
| nextjs | 17,179,237 / 133,348 | 1,700,681 / 58,454 |
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
| tanstack-start | 17.4 / 4.3 | 342.5 / 110.9 (5) | 0 | 1.9 / 0.8 |
| nextjs | 25.3 / 4.8 | 456.2 / 136.1 (7) | 0 | 1.9 / 0.8 |
| leptos | 17.8 / 3.5 | 24.8 / 7.0 (1) | 719.9 / 286.7 | 2.0 / 0.8 |

### Browser, median of 10 (p90)

| App | Interactive | LCP | Blocking time | Form action | Client navigation | JS heap |
|---|---|---|---|---|---|---|
| Desktop | | | | | | |
| django | 44 (47) ms | 68 ms | 0 ms | 76 ms | 20 (25) ms | 1.5 MB |
| aspnet | 44 (48) ms | 68 ms | 0 ms | 76 ms | 16 (18) ms | 1.5 MB |
| astro | 51 (60) ms | 60 ms | 0 ms | 76 ms | 19 (19) ms | 1.4 MB |
| **corten** | 58 (63) ms | 64 ms | 0 ms | 75 ms | 13 (13) ms | 1.3 MB |
| rails | 58 (62) ms | 64 ms | 0 ms | 91 ms | 27 (47) ms | 1.9 MB |
| spring | 59 (65) ms | 68 ms | 0 ms | 77 ms | 37 (46) ms | 1.5 MB |
| laravel | 71 (75) ms | 60 ms | 0 ms | 79 ms | 24 (24) ms | 2 MB |
| nuxt | 92 (99) ms | 72 ms | 0 ms | 72 ms | 24 (25) ms | 2.7 MB |
| sveltekit | 100 (106) ms | 76 ms | 0 ms | 75 ms | 20 (22) ms | 2.1 MB |
| phoenix | 103 (123) ms | 104 ms | 0 ms | 82 ms | 16 (17) ms | 2.1 MB |
| leptos | 104 (116) ms | 68 ms | 0 ms | 75 ms | 23 (24) ms | 1.7 MB |
| tanstack-start | 115 (124) ms | 68 ms | 0 ms | 74 ms | 27 (30) ms | 2.9 MB |
| nextjs | 151 (154) ms | 72 ms | 0 ms | 73 ms | 102 (141) ms | 3.2 MB |
| react-router | 151 (160) ms | 100 ms | 0 ms | 82 ms | 30 (33) ms | 2.5 MB |
| Mobile (4x CPU, 150 ms RTT, 1.6 Mbps) | | | | | | |
| aspnet | 430 (432) ms | 404 ms | 3 ms | 298 ms | 243 (245) ms | 1.5 MB |
| django | 435 (439) ms | 400 ms | 1 ms | 295 ms | 244 (246) ms | 1.5 MB |
| **corten** | 442 (450) ms | 440 ms | 1 ms | 287 ms | 194 (198) ms | 1.4 MB |
| spring | 605 (611) ms | 412 ms | 0 ms | 295 ms | 242 (246) ms | 1.5 MB |
| astro | 612 (623) ms | 264 ms | 0 ms | 308 ms | 241 (243) ms | 1.4 MB |
| sveltekit | 839 (844) ms | 456 ms | 3 ms | 276 ms | 272 (277) ms | 2.1 MB |
| laravel | 876 (884) ms | 420 ms | 17 ms | 336 ms | 214 (216) ms | 2 MB |
| phoenix | 924 (950) ms | 400 ms | 46 ms | 156 ms | 134 (138) ms | 2.1 MB |
| react-router | 1,054 (1,065) ms | 424 ms | 30 ms | 356 ms | 258 (263) ms | 2.6 MB |
| rails | 1,150 (1,154) ms | 416 ms | 0 ms | 309 ms | 251 (254) ms | 1.9 MB |
| nextjs | 1,388 (1,420) ms | 400 ms | 147 ms | 312 ms | 436 (440) ms | 3.1 MB |
| nuxt | 1,482 (1,494) ms | 428 ms | 55 ms | 285 ms | 277 (282) ms | 2.7 MB |
| leptos | 1,832 (1,842) ms | 412 ms | 68 ms | 296 ms | 246 (254) ms | 1.7 MB |
| tanstack-start | 2,314 (2,327) ms | 412 ms | 62 ms | 294 ms | 342 (348) ms | 2.9 MB |

### What the numbers say

1. Rendering from memory: Corten serves 23,086 item pages a second at 64 users, 5.1x ASP.NET
   Core (4,519), the next fastest, and 15x Leptos (1,490). A page costs it 124,000 user-mode
   instructions; ASP.NET Core 1.12 million, Leptos 2.7 million, Next.js 17 million.
2. JSON from memory: Corten leads with 24,338 a second, then Leptos (20,887) and ASP.NET Core
   (14,624). Corten's JSON now costs 134,000 user-mode instructions against Leptos's 205,000, and
   about what its own HTML page does (124,000). In r2 it cost 212,000 and served 16,014 a second:
   axum's `Json` wrote through a growable buffer, which `corten::Json` replaced.
3. With the item API every request fetches and parses about 55 KB of JSON, and that dominates:
   ASP.NET Core (1,209 pages a second) and Corten (1,175) are level, Leptos does 812, the rest
   109-494.
4. Failed: Next.js could not serve 300 pages a second from memory (155, p99 57 s) or 150 from
   the item API (81). Laravel did not start for the in-memory runs: each Octane worker loads the
   508 items into PHP arrays, which PHP's default `memory_limit` (128 MB) does not allow; with the
   item API it served 108 of 150 pages a second and kept up with the JSON. Nuxt passed the
   in-memory page test by the rule (1.4% of the 300 a second never sent) but at its limit: 100%
   CPU, median 724 ms, p99 14.7 s, against 89% and 19 ms in r2.
5. Cold start: Corten and Leptos answer 7.5 ms after their process starts and hold 1.5 MiB after
   the first request. The Node frameworks take 175-704 ms and 24-51 MiB; Spring Boot 8.2 s and
   136 MiB.
6. Browser: on desktop the server-rendered apps with small scripts (Django, ASP.NET Core, Astro,
   Corten, Rails, Spring) are interactive in 44-59 ms. On the throttled phone Corten is
   interactive in 442 ms, with the htmx apps (430-435 ms), against 1,388 ms for Next.js,
   1,832 ms for Leptos (WASM) and 2,314 ms for TanStack Start. Corten's client navigation is the
   fastest on desktop (13 ms); on the phone Phoenix's is (134 ms against 194 ms), over its
   already-open LiveView socket. Corten's item page now loads 6.2 KB of JavaScript (gzip) in four
   files, the handler runtime and the page's scripts; r2 measured 4.4 KB with islands.

Since r2, on the same CPU model: Corten's page went from 163,908 to 123,665 user-mode
instructions and its JSON from 212,201 to 133,588 (the middleware as its own layer, floats from
`zmij`'s digits, faster escaping, `corten::Json`). Counts for the frameworks that did not change
moved by less than 7% between rounds (Leptos's page 2,725,108 and 2,727,190), except Spring's
JSON (26% fewer, not investigated), so they compare across droplets. Requests a second do not:
the unchanged apps served 0-17% fewer pages from memory on r3's droplet than on r2's, while
Corten's page held level (23,034 and 23,086).

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
6. The images include the live benchmark's routes (`/live/...`), which the item page does not
   touch.
7. Laravel ran with PHP's default `memory_limit` throughout r3. (r2's main run had raised it to
   512 MB and measured Laravel again with the default afterwards; `results/r2/laravel-rerun.log`.)
8. r3's first browser pass stopped at Corten's page: the harness still looked for the
   `<listing-sort>` island's markup, which handlers replaced with the component's scope.
   `browser.mjs` now matches both. Parity, bytes and the browser runs were rerun on the same
   droplet right after the round (the end of `results/r3/run.log`); nothing else was rerun.

## Running it

```sh
# On a fresh droplet, created and destroyed for the run (needs doctl and an SSH key there):
node bench/fullstack/droplet.mjs r4 --ssh-key <id> [--region sfo2]
# Or by hand, from the repository root: the images, the fixture, then everything.
docker build -f examples/item-page/Dockerfile -t cb-corten .
for app in nextjs tanstack-start sveltekit react-router leptos django rails laravel aspnet spring phoenix astro nuxt; do
  docker build -t cb-$app bench/fullstack/apps/$app
done
node bench/fullstack/fixtures/fetch.mjs http://localhost:18400   # or any {"item_1": {...}} file
cd bench/fullstack && node run-all.mjs r4 10
node check-app.mjs <app>    # before adding an app: parity, JSON and 404s against corten
```
