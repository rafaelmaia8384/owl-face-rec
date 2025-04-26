# ---- Estágio de Build (MUSL) ----
ARG RUST_VERSION=1.81 # Mantenha ou atualize a versão do Rust
ARG APP_NAME=owl-face-rec
# Usar uma imagem base que tenha as ferramentas necessárias para musl
FROM rust:${RUST_VERSION}-bullseye AS build
ARG APP_NAME
WORKDIR /app

# Instalar dependências para compilação MUSL e libpq
RUN apt-get update && \
    apt-get install -y --no-install-recommends musl-tools libpq-dev build-essential pkg-config && \
    rm -rf /var/lib/apt/lists/*

# Adicionar o target musl
RUN rustup target add x86_64-unknown-linux-musl

# Copiar dependências e compilar cache (target musl)
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {println!(\"Dummy\");}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --target x86_64-unknown-linux-musl && \
    rm -f target/x86_64-unknown-linux-musl/release/deps/${APP_NAME}-* && \
    rm src/main.rs

# Copiar o restante do código fonte
COPY src ./src
COPY models ./models

# Compilar a aplicação real (target musl)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --target x86_64-unknown-linux-musl

# ---- Estágio Final (Alpine) ----
FROM alpine:latest AS final
ARG APP_NAME

# Instalar dependências de tempo de execução mínimas para Alpine
# libpq (postgresql-libs) e ca-certificates para TLS
RUN apk add --no-cache postgresql-libs ca-certificates

# Criar usuário não-root no Alpine
ARG UID=10001
RUN addgroup -S -g ${UID} appgroup && \
    adduser -S -u ${UID} -G appgroup -h /app appuser

WORKDIR /app

# Copiar binário compilado (do target musl) e modelos
COPY --from=build /app/target/x86_64-unknown-linux-musl/release/${APP_NAME} .
COPY --from=build /app/models ./models

# Definir permissões e usuário
# O diretório já pertence ao appuser devido ao -h /app no adduser
# RUN chown -R appuser:appgroup /app
USER appuser

# Expor a porta da aplicação (padrão 3000)
EXPOSE 3000

# Comando para rodar a aplicação
ENV HOST=0.0.0.0
ENV PORT=3000
# Outras variáveis de ambiente via docker-compose

CMD ["/app/owl-face-rec"] 