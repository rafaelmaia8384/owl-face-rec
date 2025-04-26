FROM rust:1.81-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y \
    pkg-config \
    g++ \
    libssl-dev \
    libstdc++-12-dev \
    && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app
RUN apt-get update && apt-get install -y \
    openssl \
    libstdc++6 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/owl-face-rec /app/
CMD ["./owl-face-rec"]