FROM rustlang/rust:nightly AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y \
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
    git \
    wget \
    && rm -rf /var/lib/apt/lists/*

ARG PORT
ARG LOG_LEVEL
ARG POSTGRES_HOST
ARG POSTGRES_DB
ARG POSTGRES_PORT
ARG POSTGRES_USER
ARG POSTGRES_PASSWORD
ARG RUSTFACE_MIN_FACE_SIZE
ARG RUSTFACE_SCORE_THRESH
ARG RUSTFACE_PYRAMID_SCALE_FACTOR
ARG RUSTFACE_SLIDE_WINDOW_STEP_X
ARG RUSTFACE_SLIDE_WINDOW_STEP_Y
ARG MINIO_ENDPOINT
ARG MINIO_BUCKET
ARG MINIO_ACCESS_KEY
ARG MINIO_SECRET_KEY
ARG MODEL_ARCFACERESNET100_8_DOWNLOAD_LINK
ARG MODEL_SEETA_FD_FRONTAL_DOWNLOAD_LINK

RUN echo "PORT=$PORT" >> .env && \
    echo "LOG_LEVEL=$LOG_LEVEL" >> .env && \
    echo "POSTGRES_HOST=$POSTGRES_HOST" >> .env && \
    echo "POSTGRES_DB=$POSTGRES_DB" >> .env && \
    echo "POSTGRES_PORT=$POSTGRES_PORT" >> .env && \
    echo "POSTGRES_USER=$POSTGRES_USER" >> .env && \
    echo "POSTGRES_PASSWORD=$POSTGRES_PASSWORD" >> .env && \
    echo "RUSTFACE_MIN_FACE_SIZE=$RUSTFACE_MIN_FACE_SIZE" >> .env && \
    echo "RUSTFACE_SCORE_THRESH=$RUSTFACE_SCORE_THRESH" >> .env && \
    echo "RUSTFACE_PYRAMID_SCALE_FACTOR=$RUSTFACE_PYRAMID_SCALE_FACTOR" >> .env && \
    echo "RUSTFACE_SLIDE_WINDOW_STEP_X=$RUSTFACE_SLIDE_WINDOW_STEP_X" >> .env && \
    echo "RUSTFACE_SLIDE_WINDOW_STEP_Y=$RUSTFACE_SLIDE_WINDOW_STEP_Y" >> .env && \
    echo "MINIO_ENDPOINT=$MINIO_ENDPOINT" >> .env && \
    echo "MINIO_BUCKET=$MINIO_BUCKET" >> .env && \
    echo "MINIO_ACCESS_KEY=$MINIO_ACCESS_KEY" >> .env && \
    echo "MINIO_SECRET_KEY=$MINIO_SECRET_KEY" >> .env && \
    echo "MODEL_ARCFACERESNET100_8_DOWNLOAD_LINK=$MODEL_ARCFACERESNET100_8_DOWNLOAD_LINK" >> .env && \
    echo "MODEL_SEETA_FD_FRONTAL_DOWNLOAD_LINK=$MODEL_SEETA_FD_FRONTAL_DOWNLOAD_LINK" >> .env && \
    sed -i '/=$/d' .env

COPY --from=builder /app/models /app/models
COPY --from=builder /app/target/release/owl-face-rec /usr/local/bin/owl-face-rec

CMD ["/usr/local/bin/owl-face-rec"]