# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/seller-desk/Dockerfile -t corten-seller-desk .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p seller-desk \
    && mkdir -p /out && cp target/release/seller-desk /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
COPY examples/seller-desk/static /app/static
ENV STATIC_DIR=/app/static HOST=0.0.0.0 PORT=8080
EXPOSE 8080
CMD ["/app/server"]
