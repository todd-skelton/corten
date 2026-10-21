# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/hello/Dockerfile -t corten-hello .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo test --release --workspace --all-features && cargo build --release -p hello \
    && mkdir -p /out && cp target/release/hello /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
COPY examples/hello/static /app/static
ENV STATIC_DIR=/app/static PORT=8080
EXPOSE 8080
CMD ["/app/server"]
