# ---- Estágio de Build (MUSL) ----
ARG RUST_VERSION=1.81
ARG APP_NAME=owl-face-rec
FROM rust:${RUST_VERSION}-bullseye AS build
ARG APP_NAME
WORKDIR /app

# 1. Instalar dependências ESSENCIAIS para MUSL + OpenSSL
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    musl-tools \
    musl-dev \
    libpq-dev \
    build-essential \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# 2. Compilar OpenSSL estático para MUSL
RUN wget https://www.openssl.org/source/openssl-1.1.1w.tar.gz && \
    tar xzf openssl-1.1.1w.tar.gz && \
    cd openssl-1.1.1w && \
    CC="musl-gcc -fPIE -pie" ./Configure no-shared no-zlib -fPIC --prefix=/usr/local/musl linux-x86_64 && \
    make depend && \
    make -j$(nproc) && \
    make install && \
    cd .. && \
    rm -rf openssl-1.1.1w*

# 3. Configurar variáveis de ambiente para OpenSSL MUSL
ENV OPENSSL_DIR=/usr/local/musl
ENV OPENSSL_STATIC=1
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV PKG_CONFIG_PATH=/usr/local/musl/lib/pkgconfig
ENV RUSTFLAGS="-C target-feature=-crt-static"

# 4. Adicionar target MUSL
RUN rustup target add x86_64-unknown-linux-musl

# 5. Cache de dependências
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {println!(\"Dummy\");}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --target x86_64-unknown-linux-musl \
    && rm -f target/x86_64-unknown-linux-musl/release/deps/${APP_NAME}-* \
    && rm src/main.rs

# 6. Compilação final
COPY src ./src
COPY models ./models
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --target x86_64-unknown-linux-musl

# ---- Estágio Final (Alpine) ----
FROM alpine:latest AS final
ARG APP_NAME

# 7. Dependências mínimas no Alpine
RUN apk add --no-cache postgresql-libs ca-certificates

# 8. Configuração de usuário seguro
ARG UID=10001
RUN addgroup -S -g ${UID} appgroup \
    && adduser -S -u ${UID} -G appgroup -h /app -D appuser

WORKDIR /app

# 9. Copiar artefatos de build
COPY --from=build --chown=appuser:appgroup \
    /app/target/x86_64-unknown-linux-musl/release/${APP_NAME} .
COPY --from=build --chown=appuser:appgroup \
    /app/models ./models

# 10. Configuração final
USER appuser
EXPOSE 3000
ENV HOST=0.0.0.0 PORT=3000 RUST_LOG=info
CMD ["/app/owl-face-rec"]