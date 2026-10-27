# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/watchlist/Dockerfile -t corten-watchlist .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache: one at a time (`locked`), and with the
# sources touched first, since cargo goes by file times and a checkout older than the last build
# would get that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo test --release --workspace --all-features && cargo build --release -p watchlist \
    && mkdir -p /out && cp target/release/watchlist /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
COPY examples/watchlist/static /app/static
ENV STATIC_DIR=/app/static HOST=0.0.0.0 PORT=8080
EXPOSE 8080
CMD ["/app/server"]
