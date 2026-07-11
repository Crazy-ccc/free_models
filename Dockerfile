# syntax=docker/dockerfile:1
#
# 构建前请先一次性构建预构建基础镜像（含系统库 + Rust 工具链，显著加速）：
#   docker build -f docker/Dockerfile.rust-builder -t rust-builder:alpine .
# 之后本项目直接 FROM 该镜像，跳过 apk/rustup 安装步骤。
# 只有 Rust 工具链或依赖变更时才需重建 rust-builder 基础镜像。

FROM rust-builder:alpine AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src src/

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    CARGO_HTTP_MULTIPLEXING=false \
    CARGO_NET_RETRY=5 \
    cargo build --release && \
    cp target/release/free_models /tmp/free_models && \
    strip /tmp/free_models

FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder --chown=nobody:nobody \
    /tmp/free_models /usr/local/bin/free_models

USER nobody

EXPOSE 8080

CMD ["free_models"]
