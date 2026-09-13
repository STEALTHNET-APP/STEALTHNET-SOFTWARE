# ── сборка ──────────────────────────────────────────────────────────
# Отдельный слой для зависимостей: они меняются редко, а код — каждый раз.
FROM rust:1-slim AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Сначала только манифесты — тогда пересборка кода не тянет заново crates.io.
COPY Cargo.toml Cargo.lock ./
COPY crates/core/Cargo.toml      crates/core/
COPY crates/payments/Cargo.toml  crates/payments/
COPY crates/api/Cargo.toml       crates/api/
COPY crates/sub/Cargo.toml       crates/sub/
COPY crates/bot/Cargo.toml       crates/bot/
COPY crates/node/Cargo.toml      crates/node/
COPY crates/worker/Cargo.toml    crates/worker/
RUN mkdir -p crates/core/src crates/payments/src crates/api/src/bin crates/sub/src \
             crates/bot/src crates/node/src crates/worker/src \
 && echo "" > crates/core/src/lib.rs \
 && echo "" > crates/payments/src/lib.rs \
 && for c in api sub bot node worker; do echo "fn main(){}" > crates/$c/src/main.rs; done \
 && echo "fn main(){}" > crates/api/src/bin/admin.rs \
 && cargo build --release 2>/dev/null || true

COPY crates crates
# Кабинет, Mini App и шрифт подписки встраиваются в бинарники при сборке.
COPY web web
# Трогаем исходники, иначе cargo посчитает их неизменёнными с шага выше.
RUN find crates -name "*.rs" -exec touch {} + && cargo build --release --locked

# ── исполнение ──────────────────────────────────────────────────────
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 -m app

COPY --from=builder /build/target/release/sn-api    /usr/local/bin/
COPY --from=builder /build/target/release/sn-sub    /usr/local/bin/
COPY --from=builder /build/target/release/sn-bot    /usr/local/bin/
COPY --from=builder /build/target/release/sn-worker /usr/local/bin/
COPY --from=builder /build/target/release/sn-node   /usr/local/bin/
COPY --from=builder /build/target/release/sn-admin  /usr/local/bin/
COPY --from=builder /build/target/release/sn-cabinet /usr/local/bin/
COPY web /app/web
COPY db  /app/db

USER app
WORKDIR /app
ENV WEB_ROOT=/app/web
EXPOSE 8080 8081
CMD ["sn-api"]
