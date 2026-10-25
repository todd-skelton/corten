# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache, and cargo goes by file times: touching
# the sources first keeps a checkout older than the last build from getting that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo build --release && cp target/release/{{name}} /server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /server /app/server
COPY static /app/static
ENV HOST=0.0.0.0 PORT=8080 STATIC_DIR=/app/static
EXPOSE 8080
CMD ["/app/server"]
