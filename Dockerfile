# =================================================================
# Estágio 1: Builder Multi-Arquitetura
# =================================================================
FROM rust:1.81-slim AS builder

# TARGETPLATFORM é preenchido automaticamente pelo 'docker buildx'
ARG TARGETPLATFORM

WORKDIR /app

# Instala o cross
RUN cargo install cross --git https://github.com/cross-rs/cross

# Instala o Docker para o cross funcionar
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        docker.io \
        ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Configura o Docker para usar o socket do host (DinD)
ENV DOCKER_HOST=tcp://docker:2376
ENV DOCKER_TLS_VERIFY=1
ENV DOCKER_CERT_PATH=/certs/client

# Copia os certificados Docker (serão montados via volume no CI)
RUN mkdir -p /certs/client

# Instala dependências específicas por arquitetura
RUN apt-get update && \
    case ${TARGETPLATFORM} in \
        "linux/arm64") \
            # Dependências para ARM64
            dpkg --add-architecture arm64 && \
            apt-get update && \
            apt-get install -y --no-install-recommends \
                pkg-config \
                gcc-aarch64-linux-gnu \
                libc6-dev-arm64-cross \
                libssl-dev:arm64 \
                libstdc++-12-dev:arm64 && \
            rustup target add aarch64-unknown-linux-gnu \
        ;; \
        "linux/amd64") \
            # Dependências para AMD64
            apt-get install -y --no-install-recommends \
                pkg-config \
                g++ \
                libssl-dev \
                libstdc++-12-dev \
        ;; \
    esac && \
    rm -rf /var/lib/apt/lists/*

COPY . .

# Compila usando cross para ARM64 e cargo normal para AMD64
RUN case ${TARGETPLATFORM} in \
        "linux/arm64") \
            # Usa cross para ARM64
            cross build --release --target aarch64-unknown-linux-gnu && \
            cp ./target/aarch64-unknown-linux-gnu/release/owlfacerec ./owlfacerec \
        ;; \
        "linux/amd64") \
            # Usa cargo normal para AMD64
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
COPY --from=builder /app/owlfacerec /app/

CMD ["./owlfacerec"]