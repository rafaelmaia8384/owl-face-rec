# ---- Estágio de Build ----
ARG RUST_VERSION=1.78
ARG APP_NAME=owl-face-rec
FROM rust:${RUST_VERSION}-slim-bullseye AS build
ARG APP_NAME
WORKDIR /app

# Instalar dependências do sistema para compilação (ex: para sqlx)
RUN apt-get update && \
    apt-get install -y --no-install-recommends libpq-dev build-essential pkg-config cmake && \
    rm -rf /var/lib/apt/lists/*

# Copiar dependências e compilar cache
COPY Cargo.toml Cargo.lock ./
# Criar um dummy src/main.rs para compilar apenas as dependências
RUN mkdir src && echo "fn main() {println!(\"Dummy\");}" > src/main.rs
# --mount=type=cache... requer BuildKit (habilite DOCKER_BUILDKIT=1)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    rm -f target/release/deps/${APP_NAME}-* && \
    rm src/main.rs

# Copiar o restante do código fonte
COPY src ./src
COPY models ./models

# Compilar a aplicação real
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release

# ---- Estágio Final ----
FROM debian:bullseye-slim AS final
ARG APP_NAME

# Instalar dependências de tempo de execução (libpq5 para Postgres)
# Adicionar ca-certificates para conexões TLS se necessário
# Adicionar libonnxruntime (se não for baixado pela crate ort - verificar documentação ort)
RUN apt-get update && \
    apt-get install -y --no-install-recommends libpq5 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Criar usuário não-root
ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser

WORKDIR /app

# Copiar binário compilado, modelos e configuração
COPY --from=build /app/target/release/${APP_NAME} .
COPY --from=build /app/models ./models
# Não copie .env diretamente se contiver segredos - use env vars do docker-compose
# COPY .env .

# Definir permissões e usuário
RUN chown -R appuser:appuser /app
USER appuser

# Expor a porta da aplicação (padrão 3000)
EXPOSE 3000

# Comando para rodar a aplicação
# Use HOST=0.0.0.0 para escutar em todas as interfaces dentro do container
ENV HOST=0.0.0.0
ENV PORT=3000
# Defina outras variáveis de ambiente via docker-compose (ex: POSTGRES_USER, etc)

CMD ["/app/owl-face-rec"] 