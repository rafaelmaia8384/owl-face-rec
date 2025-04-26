FROM rust:1.81-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/rust_rest_api /app/
CMD ["./rust_rest_api"]
