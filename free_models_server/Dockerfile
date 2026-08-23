# syntax=docker/dockerfile:1
FROM rust:1.97.1-alpine AS builder

RUN apk add --no-cache \
    ca-certificates \
    musl-dev \
    curl

RUN rustup update stable && rustup default stable

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src src/
COPY migrations migrations/

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    CARGO_HTTP_MULTIPLEXING=false \
    CARGO_NET_RETRY=5 \
    cargo build --release && \
    cp target/release/free_models_server /tmp/free_models_server && \
    strip /tmp/free_models_server

FROM alpine:3.24.1

RUN apk add --no-cache ca-certificates tzdata \
    && mkdir -p /data \
    && chown nobody:nobody /data

COPY --from=builder --chown=nobody:nobody \
    /tmp/free_models_server /usr/local/bin/free_models_server

USER nobody

EXPOSE 8080

CMD ["free_models_server"]
