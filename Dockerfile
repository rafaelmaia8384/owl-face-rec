# Etapa 1: build
FROM rust:1.81 AS builder
WORKDIR /app

# Copia manifestos primeiro para cache das dependências
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copia o restante do código e compila o binário final
COPY . .
RUN cargo build --release

# Etapa 2: imagem final mínima
FROM gcr.io/distroless/cc-debian12

# Copia apenas o binário
COPY --from=builder /app/target/release/owl-face-rec /usr/local/bin/owl-face-rec

# Define o ponto de entrada
USER nonroot:nonroot
CMD ["./owl-face-rec"]
