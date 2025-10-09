# =================================================================
# Estágio 1: Builder Multi-Arquitetura
# =================================================================
FROM rust:1.81-slim AS builder

ARG TARGETPLATFORM
WORKDIR /app

# Instala o cross
RUN cargo install cross --git https://github.com/cross-rs/cross

# Instala dependências específicas por arquitetura
RUN apt-get update && \
    case ${TARGETPLATFORM} in \
        "linux/arm64") \
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
            apt-get install -y --no-install-recommends \
                pkg-config \
                g++ \
                libssl-dev \
                libstdc++-12-dev \
        ;; \
    esac && \
    rm -rf /var/lib/apt/lists/*

COPY . .

# Compila usando cross
RUN case ${TARGETPLATFORM} in \
        "linux/arm64") \
            cross build --release --target aarch64-unknown-linux-gnu && \
            cp ./target/aarch64-unknown-linux-gnu/release/owlfacerec ./owlfacerec \
        ;; \
        "linux/amd64") \
            cargo build --release && \
            cp ./target/release/owlfacerec ./owlfacerec \
        ;; \
    esac

# =================================================================
# Estágio 2: Runtime (mantenha igual)
# =================================================================
FROM debian:bookworm-slim AS runtime
ARG TARGETPLATFORM
WORKDIR /app

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

COPY --from=builder /app/owlfacerec /app/
CMD ["./owlfacerec"]