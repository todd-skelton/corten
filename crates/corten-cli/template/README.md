# {{name}}

A [Corten](https://github.com/todd-skelton/corten) app.

```sh
corten dev      # http://localhost:3000, rebuilds on save and reloads the page
corten check    # format, lint, tests
corten build    # release binary in target/release
docker build -t {{name}} . && docker run -p 8080:8080 {{name}}
```

| Path | What |
|---|---|
| `src/main.rs` | routes and handlers |
| `src/views.rs` | the shell and pages, in `html!` |
| `static/` | CSS, images; served at versioned URLs |
| `static/islands/` | one custom element per file, loaded only where used |
