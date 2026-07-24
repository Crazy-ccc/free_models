# syntax=docker/dockerfile:1
FROM rust:alpine AS builder

RUN sed -i 's#https\?://dl-cdn.alpinelinux.org/alpine#https://mirrors.tuna.tsinghua.edu.cn/alpine#g' /etc/apk/repositories && \
    apk add --no-cache \
    ca-certificates \
    musl-dev \
    curl

RUN RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup \
    rustup update stable && rustup default stable

WORKDIR /app

RUN mkdir -p $CARGO_HOME && \
    printf '[source.crates-io]\nreplace-with = "tuna"\n\n[source.tuna]\nregistry = "https://mirrors.tuna.tsinghua.edu.cn/git/crates.io-index.git"\n' > $CARGO_HOME/config.toml

COPY Cargo.toml Cargo.lock ./
COPY src src/

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    CARGO_HTTP_MULTIPLEXING=false \
    CARGO_NET_RETRY=5 \
    cargo build --release && \
    cp target/release/free_models_server /tmp/free_models_server && \
    strip /tmp/free_models_server

FROM alpine:3.21

RUN sed -i 's#https\?://dl-cdn.alpinelinux.org/alpine#https://mirrors.tuna.tsinghua.edu.cn/alpine#g' /etc/apk/repositories && \
    apk add --no-cache ca-certificates tzdata

COPY --from=builder --chown=nobody:nobody \
    /tmp/free_models_server /usr/local/bin/free_models_server

USER nobody

EXPOSE 8080

CMD ["free_models_server"]
