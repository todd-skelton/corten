# Security policy

## Supported versions

Corten is 0.1 and is installed from this repository. Fixes land on `main` only; there are no
maintained release branches yet.

| Version | Supported |
|---|---|
| `main` (0.1) | yes |
| anything older | no |

## Reporting a vulnerability

Report it privately through GitHub: open the repository's **Security** tab and choose **Report a
vulnerability** ([direct link](https://github.com/todd-skelton/corten/security/advisories/new)).
Do not open a public issue, pull request or discussion for it.

Include the affected crate and commit, what an attacker can do, and the smallest app or request
that shows it. You will get an answer within seven days. Once a fix is on `main`, the advisory is
published with credit to you unless you ask otherwise.

## Scope

In scope is anything Corten does on an app's behalf:

1. Escaping in `html!`: text and attribute values, and anything that reaches the page without
   going through `raw`.
2. The Content-Security-Policy and its per-request nonce, and the other headers
   `corten::security::Policy` sets.
3. The same-origin check on writes (CSRF), the CORS answers of `App::api` routes, and the
   session cookie helpers' defaults.
4. Request handling: the request modes, patches, redirects (`Href` only accepts typed routes),
   static files and versioned asset URLs, and error pages that could leak internals outside dev
   mode.
5. The browser runtime (`corten.js`), handlers, `<corten-live>` and feeds, including resuming a
   feed from a client-supplied cursor.
6. `corten-cluster` (outbox, relay, followers) and `corten::agent` (MCP tools).

Out of scope: the app's own handlers and HTML passed through `raw`, settings an app chose to
loosen (`allow_style_attributes`, `allow_origin` on a `Policy` or an `Api`, `SetCookie::insecure`, `script_readable`),
dev mode (`CORTEN_DEV`, `corten dev`), which is not meant to face the internet, and
vulnerabilities in dependencies that Corten does not make reachable. Report those upstream.
