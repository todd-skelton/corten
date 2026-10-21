# Working on {{name}}

A Corten app: Rust renders every page with `html!`; the browser gets HTML, a small runtime and
custom-element islands. Run `corten dev` while working and `corten check` before you finish.

1. Pages are handlers that return `req.page(title, view)`. Markup lives in `html!` in Rust, never
   in strings. A piece of markup used twice is a `#[component]`.
2. A form that updates part of a page has `data-patch`; its handler answers `req.patch(view)`
   when `req.mode == Mode::Action` and the whole page otherwise, so it works without JavaScript.
   Each patched element has an `id`, and the page renders the same component.
3. Field names come from `#[derive(Fields)]` (`Form::FIELDS.name`), never typed as strings.
4. Islands are `static/islands/<tag-name>.js`, one per file, default export extends `Island`.
   They add behavior to server-rendered markup; they do not build HTML. User-facing text stays in
   the Rust views.
5. Everything works without JavaScript: links are links and forms post.
