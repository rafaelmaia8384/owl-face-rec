FROM rustlang/rust:nightly AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY . .

RUN cargo build --release

FROM alpine:latest

RUN apk add --no-cache \
    openssl \
    openssl-dev \
    ca-certificates

COPY --from=builder /app/target/release/owl-face-rec /usr/local/bin/owl-face-rec
CMD ["/usr/local/bin/owl-face-rec"]
