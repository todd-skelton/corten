# card-market

A trading card store: popular cards, categories, and an item page with listings, offers and a
price alert. It has every production piece Corten offers switched on: a security policy, versioned
assets, error pages, health checks, a PWA with push, head tags, French and English, robots and a
sitemap, compression, pages a CDN may keep, and JSON routes for partners' services and AI agents.
Read it after [watchlist](../watchlist) when you are taking an app to production.

## Run it

From this directory:

```sh
corten dev     # http://localhost:3000, the demo catalog built in
```

`corten` is the framework's command (`cargo install corten-cli`).

Or in Docker, from the repository root:

```sh
docker build -f examples/card-market/Dockerfile -t corten-card-market .
docker run --rm -p 3000:8080 -e API_KEYS=dev=dev-key -e AGENT_TOKENS=dev-token corten-card-market
```

| Variable | What it does | Without it |
|---|---|---|
| `API_BASE` | the item API the pages read | the demo catalog built in (`fixtures/demo.json`) |
| `ITEM_SOURCE=fixture`, `FIXTURE_PATH` | items from a JSON file of API responses, loaded at startup (benchmarks): mount one, such as `fixtures/smoke.json` (`-v "$PWD/examples/card-market/fixtures:/fixtures:ro" -e FIXTURE_PATH=/fixtures/smoke.json`) | `fixtures/items.json`, which the benchmark's `fetch.mjs` writes |
| `API_KEYS` | partners that may set alerts through `/api/alerts`, sending the key in `X-Api-Key`: `name=key,name=key` | no partner may |
| `AGENT_TOKENS` | tokens for the MCP endpoint `/mcp`, sent as `Authorization: Bearer <token>`, comma separated | no agent may |
| `AGENT_ORIGINS` | origins whose pages may call `/mcp` from the browser (CORS), comma separated | none |
| `WRITES_PER_MINUTE` | writes one client address may make a minute | 20 |
| `PROXIES` | the load balancers whose `X-Forwarded-For` is trusted: addresses or CIDRs, comma separated | the connection's address |
| `HTTPS` | present (any value) when served over HTTPS: browsers are told to keep using it (HSTS) | not sent |
| `HOSTS` | the host names the app answers; a request for any other gets a 421 | `localhost` |
| `HOST`, `PORT` | the address it listens on | both loopback addresses, port 3000 (the image sets `0.0.0.0:8080`) |

## Where things are

| Piece | File |
|---|---|
| Every URL and which handler serves it (pages, the JSON API, the MCP endpoint) | [`src/main.rs`](src/main.rs) |
| The handlers, and the app: policy, PWA, deadline, readiness, error page, the write limit, the partner key, the MCP tools, robots and sitemap | [`src/app.rs`](src/app.rs) |
| Where items come from (an API, a fixture, the demo catalog) | [`src/source.rs`](src/source.rs) |
| The item and the alert's rules | [`src/model.rs`](src/model.rs) |
| Views, each with its stylesheet and script beside it | [`src/views/`](src/views) |
| Text in English and French | [`locales/`](locales), [`src/text.rs`](src/text.rs) |
| Design tokens | [`tokens.json`](tokens.json), [`src/theme.rs`](src/theme.rs) |
| The stylesheet, icons, the push handler | [`static/`](static) |

The browser test of the production pieces: [`smoke.mjs`](smoke.mjs). The item page is the one
the [full-stack benchmark](../../bench/fullstack) measures.
