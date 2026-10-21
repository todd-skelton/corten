# Working on {{name}}

A Corten app: Rust renders every page with `html!`; the browser gets HTML, a small runtime and
custom-element islands. Run `corten dev` while working and `corten check` before you finish.

1. Pages are handlers that return `req.page(title, view)`. Markup lives in `html!` in Rust, never
   in strings. A piece of markup used twice is a `#[component]`.
2. A form that updates part of a page has `data-patch`; its handler answers `req.patch(view)`
   when `req.mode == Mode::Action` and the whole page otherwise, so it works without JavaScript.
   Each patched element has an `id`, and the page renders the same component.
3. Field names come from `#[derive(Fields)]` (`Form::FIELDS.name`), never typed as strings.
4. Behavior in the browser is an `#[island]` component: its markup names elements with
   `ref={Name}` and events with `on:click={Action}`, and the script next to it (`snake_name.js`,
   `export default island("kebab-name", { actions: { action(ctx) {} } })`) implements them.
   Scripts add behavior to server-rendered markup; they do not build HTML, and user-facing text
   stays in the Rust views.
5. A component's styles are `snake_name.css` next to it, with `#[component(css)]` (or
   `#[island(css)]`): every rule starts at one of the file's classes, and the markup names them
   as constants, `class={Title}`, never as strings. `static/style.css` is only element defaults
   and the shell. List every island and styled component once in `App::components`.
6. Everything works without JavaScript: links are links and forms post.
