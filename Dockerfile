# ---- Estágio de Build (MUSL) ----
ARG RUST_VERSION=1.81
ARG APP_NAME=owl-face-rec
FROM messense/rust-musl-cross:x86_64-musl AS build
ARG APP_NAME
WORKDIR /app

# 1. Instalar dependências adicionais necessárias
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libpq-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# 2. Configurar OpenSSL para MUSL (já pré-instalado na imagem)
ENV OPENSSL_DIR=/usr/include/openssl
ENV OPENSSL_STATIC=1
ENV PKG_CONFIG_ALLOW_CROSS=1
ENV PKG_CONFIG_PATH=/usr/lib/x86_64-linux-musl/pkgconfig

# 3. Cache de dependências
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {println!(\"Dummy\");}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release \
    && rm -f target/x86_64-unknown-linux-musl/release/deps/${APP_NAME}-* \
    && rm src/main.rs

# 4. Compilação final
COPY src ./src
COPY models ./models
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release

# ---- Estágio Final (Alpine) ----
FROM alpine:latest AS final
ARG APP_NAME

# 5. Dependências mínimas no Alpine
RUN apk add --no-cache postgresql-libs ca-certificates

# 6. Configuração de usuário seguro
ARG UID=10001
RUN addgroup -S -g ${UID} appgroup \
    && adduser -S -u ${UID} -G appgroup -h /app -D appuser

WORKDIR /app

# 7. Copiar artefatos de build
COPY --from=build --chown=appuser:appgroup \
    /app/target/x86_64-unknown-linux-musl/release/${APP_NAME} .
COPY --from=build --chown=appuser:appgroup \
    /app/models ./models

# 8. Configuração final
USER appuser
EXPOSE 3000
ENV HOST=0.0.0.0 PORT=3000 RUST_LOG=info
CMD ["/app/owl-face-rec"]