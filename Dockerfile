FROM rustlang/rust:nightly AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

COPY . .
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12

COPY --from=builder /app/target/release/owl-face-rec /usr/local/bin/owl-face-rec

USER nonroot:nonroot
CMD ["/usr/local/bin/owl-face-rec"]
