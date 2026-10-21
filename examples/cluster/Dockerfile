# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/cluster/Dockerfile -t corten-cluster .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p cluster \
    && mkdir -p /out && cp target/release/cluster /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
ENV PORT=8080
EXPOSE 8080
CMD ["/app/server"]
