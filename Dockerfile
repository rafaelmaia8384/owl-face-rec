ARG RUST_VERSION=1.70
ARG TARGET=x86_64-unknown-linux-musl

FROM rust:${RUST_VERSION} as builder

RUN apt-get update && apt-get install -y \
    musl-tools \
    && rustup target add x86_64-unknown-linux-musl \
    && rustup target add aarch64-unknown-linux-musl \
    && cargo install cross

WORKDIR /app
COPY . .

ARG TARGET
RUN cross build --target $TARGET --release

FROM alpine:latest
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/$TARGET/release/owl-face-rec /usr/local/bin/owl-face-rec
CMD ["./owl-face-rec"]