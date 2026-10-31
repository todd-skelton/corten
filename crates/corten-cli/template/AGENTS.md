# Working on {{name}}

A Corten app, built to be fast for its users: Rust renders the first page with `html!`, and the
handler scripts next to components do what the page can do with what it already has, with no
request. Run `corten dev` while working and `corten check` before you finish.

For recipes (pages, forms with validation, handlers, live data, styles, tests), use the Corten
skill: https://github.com/todd-skelton/corten/tree/main/skills/corten. Install it with your
agent's skill installer, or read it there.

1. Pages are handlers that return `req.page(..)` with the whole document: `Layout` from
   `src/views.rs` (the app's own, to change freely) with the page as its children. Failed requests
   render `error_page`, in the same layout. Markup lives in `html!` in Rust, never in strings. A
   piece of markup used twice is a `#[component]`.
2. Do the work where it is fastest: sorting, filtering, selecting, totals, showing and hiding
   happen in the browser, in a component's script (4); a write, or data the page does not have, goes to the server.
   A form that writes and updates part of a page has `data-patch`; its handler answers
   `req.patch_or(view, || async { req.page(..) }).await`: the patch with JavaScript, the whole page (or a redirect) otherwise, so it works without JavaScript.
   The patch is a component declared `-> impl Patch`, rooted at an element with a `#[derive(Id)]` id,
   and the page renders the same component; one that is not a patch does not compile.
3. URLs are the structs in `mod url` (`#[corten::routes]`): `href={url::Home}`,
   `req.redirect(&url::Home)`, and each names its handlers, `#[route("/", get = home)]`.
   Field names come from `#[derive(Fields)]` (`Form::FIELDS.name`); neither is ever typed as a
   string.
4. Behavior in the browser is a `#[component(js)]`: `snake_name.js` next to it exports plain
   functions with JSDoc types, and the markup calls them with the elements and values they use:
   `let (save, dialog) = refs();`, `ref={save}`, `on:click={js::confirm(event, dialog)}`.
   `event`, `target`, `signal` and `scope` are the browser's. A wrong element type or a missing
   function is a compile error. Scripts clone and change markup the server rendered; they never
   build HTML from strings, and user-facing text stays in the Rust views.
5. A component's styles are `snake_name.css` next to it, with `#[component(css)]`: every rule starts at one of the file's classes, and the markup names them
   as constants, `class={Title}`, never as strings. `static/style.css` is only element defaults
   and the layout. List every component with a script or stylesheet once in `App::components`.
6. Pages and actions work without JavaScript: links are links and forms post.
7. The app is the library, `src/lib.rs`: modules are declared there, and `src/main.rs` only serves
   `app()`. A test target in `tests/` or a tool in `examples/` uses `{{crate}}::app()` and the modules.
