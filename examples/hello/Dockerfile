# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/hello/Dockerfile -t corten-hello .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache, and cargo goes by file times: touching
# the sources first keeps a checkout older than the last build from getting that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo test --release --workspace --all-features && cargo build --release -p hello \
    && mkdir -p /out && cp target/release/hello /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
COPY examples/hello/static /app/static
ENV STATIC_DIR=/app/static HOST=0.0.0.0 PORT=8080
EXPOSE 8080
CMD ["/app/server"]
