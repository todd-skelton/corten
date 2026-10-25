# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
RUN rustup target add wasm32-unknown-unknown
RUN curl --proto '=https' --tlsv1.2 -LsSf https://github.com/leptos-rs/cargo-leptos/releases/download/v0.3.11/cargo-leptos-installer.sh | sh
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache: one at a time (`locked`), and with the
# sources touched first, since cargo goes by file times and a checkout older than the last build
# would get that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo leptos build --release && mkdir -p /out && cp target/release/item-page-leptos /out/server && cp -r target/site /out/site

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
COPY --from=build /out/site /app/site
ENV LEPTOS_OUTPUT_NAME=item_page \
    LEPTOS_SITE_ROOT=site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:8080 \
    LEPTOS_RELOAD_PORT=3001 \
    LEPTOS_ENV=PROD
EXPOSE 8080
CMD ["/app/server"]
