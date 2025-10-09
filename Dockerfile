# =================================================================
# Estágio 1: Builder Multi-Arquitetura
# =================================================================
FROM rust:1.81-slim AS builder

RUN rustup install nightly
RUN rustup default nightly

# TARGETPLATFORM é preenchido automaticamente pelo 'docker buildx'
# Ex: 'linux/amd64' ou 'linux/arm64'
ARG TARGETPLATFORM

WORKDIR /app

# ENV CARGO_BUILD_JOBS=1
RUN cargo install cross --git https://github.com/cross-rs/cross

# Instala as dependências de build com base na arquitetura de destino
RUN apt-get update && \
    case ${TARGETPLATFORM} in \
        "linux/arm64") \
            # Habilita a multi-arquitetura do Debian e instala as ferramentas de cross-compilação para ARM64
            dpkg --add-architecture arm64 && \
            apt-get update && \
            apt-get install -y --no-install-recommends \
                pkg-config \
                gcc-aarch64-linux-gnu \
                libc6-dev-arm64-cross \
                libssl-dev:arm64 \
                libstdc++-12-dev:arm64 && \
            # Adiciona o target do Rust para ARM64
            rustup target add aarch64-unknown-linux-gnu \
            ;; \
        "linux/amd64") \
            # Instala as dependências nativas para AMD64
            apt-get install -y --no-install-recommends \
                pkg-config \
                g++ \
                libssl-dev \
                libstdc++-12-dev \
            ;; \
    esac && \
    rm -rf /var/lib/apt/lists/*

COPY . .

# Compila o projeto com base na arquitetura de destino
RUN case ${TARGETPLATFORM} in \
        "linux/arm64") \
            # Executa a cross-compilação para ARM64
            cross build --release --target aarch64-unknown-linux-gnu && \
            # Copia o binário para um local padrão para facilitar a próxima etapa
            cp ./target/aarch64-unknown-linux-gnu/release/owlfacerec ./owlfacerec \
            ;; \
        "linux/amd64") \
            # Executa a compilação nativa para AMD64
            cargo build --release && \
            cp ./target/release/owlfacerec ./owlfacerec \
            ;; \
    esac

# =================================================================
# Estágio 2: Runtime Multi-Arquitetura
# =================================================================
FROM debian:bookworm-slim AS runtime

ARG TARGETPLATFORM

WORKDIR /app

# Instala as dependências de runtime com base na arquitetura de destino
RUN apt-get update && \
    case ${TARGETPLATFORM} in \
        "linux/arm64") \
            dpkg --add-architecture arm64 && \
            apt-get update && \
            apt-get install -y --no-install-recommends \
                libssl3:arm64 \
                libstdc++6:arm64 \
            ;; \
        "linux/amd64") \
            apt-get install -y --no-install-recommends \
                openssl \
                libstdc++6 \
            ;; \
    esac && \
    rm -rf /var/lib/apt/lists/*

# Copia o binário compilado do estágio de builder
# O nome do arquivo agora é o mesmo para ambas as arquiteturas
COPY --from=builder /app/owlfacerec /app/

CMD ["./owlfacerec"]