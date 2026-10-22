# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f bench/live/apps/corten/Dockerfile -t cb-live-corten .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/bench/live/apps/corten/target \
    cd bench/live/apps/corten && test -f Cargo.toml && cargo build --release && mkdir -p /out && cp target/release/live-corten /out/server

FROM debian:bookworm-slim
COPY --from=build /out/server /app/server
ENV HOST=0.0.0.0 PORT=8080
EXPOSE 8080
CMD ["/app/server"]
