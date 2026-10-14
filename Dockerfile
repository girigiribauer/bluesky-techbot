# syntax=docker/dockerfile:1

FROM rust:1.99.0-slim-bookworm AS chef
WORKDIR /build
RUN cargo install cargo-chef --version 0.1.78 --locked
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev perl make && \
    rm -rf /var/lib/apt/lists/*

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM chef AS builder
COPY . .
COPY --from=cacher /build/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
RUN cargo build --release --bin bluesky-techbot

FROM debian:trixie-slim AS runtime

RUN apt-get update && \
    apt-get install -y ca-certificates sqlite3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /build/target/release/bluesky-techbot /usr/local/bin/app
COPY config.toml /app/config.toml

ENV TECHBOT_DB_PATH=/data/techbot.db

EXPOSE 8080

CMD ["/usr/local/bin/app"]
