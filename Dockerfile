FROM rust:1.81-slim AS builder
WORKDIR /app
# Install OpenSSL development dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim AS runtime
WORKDIR /app
# Install OpenSSL runtime libraries
RUN apt-get update && apt-get install -y \
    libssl1.1 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/rust_rest_api /app/
CMD ["./rust_rest_api"]
