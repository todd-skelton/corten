# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/cluster/Dockerfile -t corten-cluster .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache, and cargo goes by file times: touching
# the sources first keeps a checkout older than the last build from getting that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo build --release -p cluster \
    && mkdir -p /out && cp target/release/cluster /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
ENV PORT=8080
EXPOSE 8080
CMD ["/app/server"]
