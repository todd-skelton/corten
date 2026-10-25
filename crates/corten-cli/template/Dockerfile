# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache: one at a time (`locked`), and with the
# sources touched first, since cargo goes by file times and a checkout older than the last build
# would get that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo build --release && cp target/release/{{name}} /server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /server /app/server
COPY static /app/static
ENV HOST=0.0.0.0 PORT=8080 STATIC_DIR=/app/static
EXPOSE 8080
CMD ["/app/server"]
