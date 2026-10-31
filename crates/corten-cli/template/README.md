# {{name}}

A [Corten](https://github.com/todd-skelton/corten) app.

```sh
corten dev      # http://localhost:3000, rebuilds on save and reloads the page
corten check    # format, lint, script types (with tsc installed), tests
corten build    # release binary in target/release
docker build -t {{name}} . && docker run -p 8080:8080 {{name}}
```

| Path | What |
|---|---|
| `src/lib.rs` | the app: routes, handlers, `app()` and its tests; declare modules here |
| `src/main.rs` | `main`, which serves `app()`; declares no modules |
| `src/views.rs` | the layout (the whole document), the error page and the pages, in `html!` |
| `src/click_counter.js`, `.css` | the `ClickCounter` component's handlers and stylesheet, next to its component |
| `static/` | the app's own CSS (element defaults, the layout), images; compiled into the program (`views::Static`, a constant per file) and served at versioned URLs |
