# syntax=docker/dockerfile:1
# Build from the repository root: docker build -f examples/auction-room/Dockerfile -t corten-auction-room .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
# Every build on this machine shares the target cache: one at a time (`locked`), and with the
# sources touched first, since cargo goes by file times and a checkout older than the last build
# would get that build's crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    find . -name target -prune -o -type f -exec touch {} + \
    && cargo build --release -p auction-room \
    && mkdir -p /out && cp target/release/auction-room /out/server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=build /out/server /app/server
RUN mkdir -p /app/data
# The database is a file in /app/data: mount a volume there to keep it.
ENV HOST=0.0.0.0 PORT=8080 DATABASE_URL=sqlite:/app/data/auction.db?mode=rwc
EXPOSE 8080
CMD ["/app/server"]