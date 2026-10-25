# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/ui-gallery/Dockerfile -t corten-ui-gallery .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache, and cargo goes by file times: touching
# the sources first keeps a checkout older than the last build from getting that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo build --release -p ui-gallery && mkdir -p /out && cp target/release/ui-gallery /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
COPY examples/ui-gallery/static /app/static
ENV STATIC_DIR=/app/static HOST=0.0.0.0 PORT=8080
EXPOSE 8080
CMD ["/app/server"]
