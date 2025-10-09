FROM rustlang/rust:nightly AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY . .

RUN cargo build --release

FROM debian:bookworm-slim

COPY --from=builder /app/target/release/owl-face-rec /usr/local/bin/owl-face-rec
CMD ["/usr/local/bin/owl-face-rec"]
