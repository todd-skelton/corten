# Working on Corten

Read `README.md` (rules, request modes) and `docs/architecture.md` (surfaces) before changing
anything. Keep the framework small: a change that adds a dependency or a new concept needs a
reason that a smaller change cannot meet.

## Where things go

1. Wire format and client-side feed logic: `crates/corten-protocol`. No async or web
   dependencies there; it must build for native and agent clients.
2. Server feed (replay, live streams, followers): `crates/corten-feed`.
3. Pages, request modes, patches, islands, the runtime: `crates/corten`.
4. Never edit `crates/corten/js/dist/solid.js` by hand; run `npm run build:solid`.
5. Every behavior the runtime has is covered by `scripts/smoke.mjs` against `examples/hello`.
   Add the check with the behavior.

## Checks (all must pass before you report done)

```powershell
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo fmt --all --check && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo test --workspace --all-features"
npm run check
docker build -f examples/hello/Dockerfile -t corten-hello .
docker run -d --name corten-hello -p 18480:8080 corten-hello
npm run smoke -- http://localhost:18480
docker rm -f corten-hello
```

Rust tests run in Docker because the Windows host cannot link `windows-sys`. Run each command
separately.
