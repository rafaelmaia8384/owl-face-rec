FROM rustlang/rust:nightly AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY . .

COPY models/ /app/models/

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/models /app/models
COPY --from=builder /app/target/release/owl-face-rec /usr/local/bin/owl-face-rec

CMD ["/usr/local/bin/owl-face-rec"]