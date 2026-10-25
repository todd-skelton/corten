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
| `src/main.rs` | routes and handlers |
| `src/views.rs` | the layout (the whole document), the error page and the pages, in `html!` |
| `src/click_counter.js`, `.css` | the `ClickCounter` component's handlers and stylesheet, next to its component |
| `static/` | the app's own CSS (element defaults, the layout), images; served at versioned URLs |
