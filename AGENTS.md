# Working on Corten

Read `README.md`, the guide in `docs/guide/` for the area you are changing, `docs/design.md`
(choices, rules, limits) and `docs/architecture.md` (surfaces) before changing anything. Keep the framework small: a change that adds a dependency or a new concept needs a
reason that a smaller change cannot meet.

## Values

Every change follows the values in `README.md` (Values). When they conflict, this order decides:

1. Correctness and reliability.
2. Performance at the user's end.
3. Type safety and one way to do things.
4. Developer convenience.

For example, the deadline timer that cost 640 ns per request was reworked, not accepted for
convenience. And a nicer API that slows every page loses.

## Where things go

1. Wire format and client-side feed logic: `crates/corten-protocol`. No async or web
   dependencies there; it must build for native and agent clients.
2. Server feed (replay, live streams, followers): `crates/corten-feed`.
3. Pages, request modes, patches, handlers, the runtime, `App` defaults: `crates/corten`.
   Markup is `html!` and `#[component]`, links are `#[derive(Route)]` structs, behavior is a
   `#[component(js)]` (plain `export function`s with JSDoc in the script next to it, called from
   its markup: `let save = refs();`, `ref={save}`, `on:click={js::enable(save)}`) and text is
   `messages!` (`crates/corten-macros`, no dependencies: keep it on `proc_macro` alone). Never
   write a URL, `data-ref`, `data-on`, `data-scope` or message key as a string in a view, and give
   an element a patch replaces or removes a `#[derive(Id)]` id.
   Behavior has one pattern, a function the markup calls; do not add built-in client behaviors
   or a second component model. Cleanup is the `signal` keyword alone (an `AbortSignal`): runtime APIs that start lasting work take
   `{ signal }` and return nothing, and nothing hands back a stop function. On the server, work
   that outlives the handler's future takes `corten::Signal`.
   Styles are a CSS file per component (`#[component(css)]`, scoped at compile time, `class={Name}`)
   and tokens from `tokens!`; never write one of a component's classes as a string, and keep
   inline `style=` out (the CSP blocks it).
   A compile-time guarantee gets a `compile_fail` doctest next to the feature. A change to what `html!` generates needs the item page's
   `render_cost` test before and after (`CORTEN_FIXTURE=bench/fullstack/fixtures/items.json`,
   release build) so rendering does not get slower unnoticed.
4. The `corten` command: `crates/corten-cli`, standard library only. `template/` is the app
   `corten new` writes; it must pass `corten check` with rustfmt's default settings.
5. Edit the runtime in `crates/corten/js/corten.js` and `solid.js`; the crate embeds their
   minified builds in `js/dist/`, which only `npm run build:js` writes (`npm run check` fails
   when they are stale).
6. Every behavior the runtime has is covered by `scripts/smoke.mjs` against `examples/hello`
   (live reload under `corten dev` by `scripts/smoke-dev.mjs`), and production modules (security, assets, PWA, head tags, compression) by
   `examples/item-page/smoke.mjs`. Add the check with the behavior.
7. Types across seams: never hand-write a type the server already defines. Handler calls are
   checked against each handler's JSDoc when the crate compiles; feed rows and typed events
   (`#[event]`) against the declarations `corten::types` generates (`examples/*/types/app.d.ts`,
   rewritten by the `script_types_match_the_server` test); a typing guarantee gets an
   `@ts-expect-error` line in `scripts/typecheck-fixture.js` or a `compile_fail` doctest.
8. Widgets the browser lacks go in `crates/corten-ui`: headless `#[component(js)]`s (no CSS; parts are
   styled by role and ARIA state), labels from the app, links as typed routes (`Link`), working as
   plain HTML before the script loads. Add one only when an app needs it and the platform has no
   element for it; remove it when the platform does. Each is tested in a browser by
   `examples/ui-gallery/smoke.mjs`.
9. `bench/` is not part of the workspace. Its apps are built only by `bench/fullstack` scripts.

## Checks (all must pass before you report done)

Run every command in [CONTRIBUTING.md](CONTRIBUTING.md#checks), each separately, and remove the
containers afterwards. Update the guide that describes a changed behavior, and add a line to
`CHANGELOG.md` under Unreleased.
