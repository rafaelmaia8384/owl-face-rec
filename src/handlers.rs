use axum::{
    body::Bytes,
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use base64::{engine::general_purpose, Engine as _};
use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, Rgb};
use md5;
use minio::s3::segmented_bytes::SegmentedBytes;
use minio::s3::types::S3Api;
use ndarray::{Array, Ix4};
use ort::{inputs, session::Session, session::SessionOutputs, value::Value};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx;
use sqlx::Row;
use std::env;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;
use tokio::task;
use utoipa::{schema, ToSchema};
use uuid::Uuid;

use crate::AppState;
use crate::SafeDetector;

// Define the request payload for /register/
#[derive(Deserialize, ToSchema)]
pub struct RegisterPayload {
    #[schema(value_type = String, format = "uuid")]
    target_uuid: Uuid,
    image_base64: String,
    origin: String,
    extra: Option<serde_json::Value>,
}

// Define the request payload for /search/
#[derive(Deserialize, ToSchema)]
pub struct SearchPayload {
    image_base64: String,
    threshold: Option<f32>,
    limit: Option<usize>,
}

// Define the response for /search/
#[derive(Serialize, ToSchema)]
pub struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Serialize, ToSchema)]
pub struct SearchResult {
    id: i64,
    target_uuid: String,
    similarity: f32,
    origin: String,
    image_key: String,
}

// Function to decode base64 and return the image (synchronous for performance)
fn decode_base64_to_image(image_base64: &str) -> Result<DynamicImage, StatusCode> {
    let start = Instant::now(); // Record start time
                                // Decode Base64
    let image_bytes = general_purpose::STANDARD
        .decode(image_base64)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to decode base64 image");
            StatusCode::BAD_REQUEST
        })?;
    tracing::debug!(image_size = image_bytes.len(), "Base64 decoded");

    // Load Image from bytes
    let img: DynamicImage = image::load_from_memory(&image_bytes).map_err(|e| {
        tracing::error!(error = %e, "Failed to load image from bytes");
        StatusCode::BAD_REQUEST
    })?;
    tracing::debug!(dims = ?img.dimensions(), "Image loaded");
    let duration = start.elapsed(); // Calculate duration
    tracing::info!(duration = ?duration, "decode_base64_to_image"); // Log duration
    Ok(img)
}

// Function to get cropped face image
pub async fn crop_face(
    detector_arc: Arc<SafeDetector>,
    img: DynamicImage,
) -> Result<DynamicImage, StatusCode> {
    let start = Instant::now(); // Record start time
    let res = task::spawn_blocking(move || {
        let mut detector = detector_arc.lock();
        let gray = img.to_luma8();
        let (width, height) = (gray.width(), gray.height());
        let image_data = rustface::ImageData::new(gray.as_raw(), width, height);
        let faces = detector.detect(&image_data);

        if faces.is_empty() {
            tracing::error!("No face detected in the image");
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }

        if faces.len() > 1 {
            tracing::error!(
                count = faces.len(),
                "More than one face detected in the image"
            );
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }

        let face = &faces[0];
        let bbox = face.bbox();

        let x_factor: f32 = bbox.width() as f32 * 0.05;
        let y_factor: f32 = bbox.height() as f32 * 0.05;

        let x = (bbox.x() as f32 - x_factor) as u32;
        let y = (bbox.y() as f32 - y_factor) as u32;
        let w = (bbox.width() as f32 + x_factor) as u32;
        let h = (bbox.height() as f32 + (y_factor * 3.0)) as u32;

        let cropped = img.crop_imm(x, y, w, h);

        Ok(cropped)
    })
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Error executing face detection task");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let duration = start.elapsed(); // Calculate duration
    tracing::info!(duration = ?duration, "crop_face"); // Log duration
    res
}

// Function to obtain embeddings from the image (async for blocking ONNX inference)
async fn get_embedding_from_image(
    img: DynamicImage,
    onnx_session: &Arc<Session>,
) -> Result<Vec<f32>, StatusCode> {
    let start = Instant::now(); // Record start time
                                // Clone the Arc to make it owned and 'static
    let onnx_session = onnx_session.clone();

    // Wrap CPU-bound ONNX inference in spawn_blocking for better async performance
    let res = task::spawn_blocking(move || {
        // Preprocess Image
        let input_array: Array<f32, Ix4> = preprocess_image(img, 112, 112).map_err(|e| {
            tracing::error!(error = %e, "Failed to preprocess image");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        tracing::debug!(shape = ?input_array.shape(), "Image preprocessed");

        // Prepare ONNX Input Value
        let shape: Vec<usize> = input_array.shape().to_vec();
        let raw_vec = input_array.into_raw_vec();
        let input_value = Value::from_array((shape, raw_vec)).map_err(|e| {
            tracing::error!(error = %e, "Failed to create input value from array");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Prepare session inputs and run ONNX Inference
        let session_inputs = inputs![input_value].map_err(|e| {
            tracing::error!(error = %e, "Failed to create session inputs");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let outputs: SessionOutputs = onnx_session.run(session_inputs).map_err(|e| {
            tracing::error!(error = %e, "ONNX inference failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        // Process Output (Get Embedding)
        if outputs.len() == 0 {
            tracing::error!("ONNX output is empty");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        let embedding_value: &Value = &outputs[0];
        let embedding_tensor = embedding_value.try_extract_tensor::<f32>().map_err(|e| {
            tracing::error!(error = %e, "Failed to extract tensor from ONNX output");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let embedding_vec: Vec<f32> = embedding_tensor.view().iter().cloned().collect();
        Ok(embedding_vec)
    })
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to spawn blocking task for inference");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let duration = start.elapsed(); // Calculate duration
    tracing::info!(duration = ?duration, "get_embedding_from_image"); // Log duration
    Ok(res?)
}

// --- Handlers ---

// Handler for GET / route, returns 200 OK
#[utoipa::path( 
    get,
    path = "/health/",
    responses(
        (status = 200, description = "Health check OK")
    )
)]
pub async fn health_check() -> axum::http::StatusCode {
    axum::http::StatusCode::OK
}

// Handler for POST /register/
#[utoipa::path(
    post,
    path = "/register/",
    request_body = RegisterPayload,  // Infere do Json<RegisterPayload>
    responses(
        (status = 201, description = "Registro criado com sucesso"),
        (status = 400, description = "Payload inválido ou erro de registro")
    )
)]
pub async fn register(
    State(state): State<AppState>, // Extract state
    Json(payload): Json<RegisterPayload>,
) -> Result<StatusCode, StatusCode> {
    let start = Instant::now(); // Record start time

    // --- Payload Validation ---
    if payload.target_uuid == Uuid::nil() {
        // Check if UUID is nil (optional, but good practice)
        tracing::warn!("Received registration request with nil UUID");
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload.origin.trim().is_empty() {
        tracing::warn!("Received registration request with empty origin");
        return Err(StatusCode::BAD_REQUEST);
    }
    if payload.image_base64.trim().is_empty() {
        tracing::warn!("Received registration request with empty image_base64");
        return Err(StatusCode::BAD_REQUEST);
    }
    // --- End Validation ---

    let target_uuid = payload.target_uuid;
    let origin = payload.origin.clone();
    let extra = payload.extra.clone().unwrap_or(serde_json::Value::Null);
    tracing::debug!(%target_uuid, %origin, "Received registration request");

    // Get embedding using separated functions
    let img = decode_base64_to_image(&payload.image_base64)?;
    let cropped_img = crop_face(state.facedetector, img.clone()).await?;
    let embedding_vec = get_embedding_from_image(cropped_img, &state.onnx_session).await?;
    let minio_bucket = env::var("MINIO_BUCKET").expect("MINIO_BUCKET must be set");
    let mut buffer = Cursor::new(Vec::new());

    img.write_to(&mut buffer, ImageFormat::WebP).map_err(|e| {
        tracing::error!(error = %e, "Failed to write image to buffer");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let image_bytes = buffer.into_inner();
    let image_key = format!("{:x}.webp", md5::compute(&image_bytes));
    let segmented_bytes = SegmentedBytes::from(Bytes::from(image_bytes));

    tracing::info!(%target_uuid, %origin, "Sending image do bucket...");
    state
        .s3_client
        .put_object(minio_bucket, &image_key, segmented_bytes)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to upload image to MinIO");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tracing::info!(%target_uuid, "Image successfully sent");

    // Store the embedding in the database
    tracing::info!(%target_uuid, %origin, "Storing embedding in the database...");
    match sqlx::query(
        r#"
        INSERT INTO targets (uuid, embeddings, image_key, origin, extra)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(target_uuid)
    .bind(&embedding_vec[..])
    .bind(&image_key)
    .bind(&origin)
    .bind(extra)
    // .execute(&state.db_pool)
    .fetch_one(&state.db_pool)
    .await
    {
        Ok(record) => {
            let id: i64 = record.try_get("id").map_err(|e| {
                tracing::error!(error = %e, "Failed to get id from record");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            tracing::info!(%target_uuid, "Successfully stored embedding in the database");

            // Add the embedding to in-memory storage
            tracing::info!(%target_uuid, %origin, "Adding embedding to in-memory store...");
            let mut embeddings_store = match state.embeddings_store.lock() {
                Ok(store) => store,
                Err(e) => {
                    tracing::error!(%target_uuid, error = %e, "Failed to lock embeddings store");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };
            embeddings_store.add(
                id,
                target_uuid,
                origin.clone(),
                embedding_vec.clone(),
                image_key.clone(),
            );
            tracing::info!(%target_uuid, "Successfully added embedding to in-memory store");
            tracing::info!(%target_uuid, "Total embeddings in memory: {}", embeddings_store.len());

            let duration = start.elapsed(); // Calculate duration
            tracing::info!(%target_uuid, duration = ?duration, "Registration successful"); // Log duration

            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            tracing::error!(%target_uuid, error = %e, "Failed to store embedding in database");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Handler for POST /search/
#[utoipa::path(
    post,
    path = "/search/",
    request_body = SearchPayload,
    responses(
        (status = 200, description = "Busca realizada", body = SearchResponse),
        (status = 400, description = "Payload inválido ou erro na busca")
    )
)]
pub async fn search(
    State(state): State<AppState>,
    Json(payload): Json<SearchPayload>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let start = Instant::now();

    if payload.image_base64.trim().is_empty() {
        tracing::warn!("Received search request with empty image_base64");
        return Err(StatusCode::BAD_REQUEST);
    }

    tracing::debug!("Received search request");

    // Get query embedding using separated functions
    let img = decode_base64_to_image(&payload.image_base64)?;
    let cropped_img = crop_face(state.facedetector, img).await?;
    let embedding_vec = get_embedding_from_image(cropped_img, &state.onnx_session).await?;
    tracing::info!(
        "Query embedding calculated (first 5 values): {:?}",
        &embedding_vec[..5.min(embedding_vec.len())]
    );

    // Search for similar embeddings in memory
    let threshold = payload.threshold.unwrap_or(0.7); // Default threshold
    let limit = payload.limit.unwrap_or(10); // Default limit

    tracing::info!(
        "Searching for similar embeddings with threshold={} and limit={}",
        threshold,
        limit
    );

    let embeddings_store = match state.embeddings_store.lock() {
        Ok(store) => store,
        Err(e) => {
            tracing::error!(error = %e, "Failed to lock embeddings store");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let similar_embeddings = embeddings_store.find_similar(&embedding_vec, threshold, limit);
    tracing::info!("Found {} similar embeddings", similar_embeddings.len());

    // Format results
    let results: Vec<SearchResult> = similar_embeddings
        .into_iter()
        .map(|(id, uuid, origin, image_key, similarity)| SearchResult {
            id,
            target_uuid: uuid.to_string(),
            similarity,
            origin,
            image_key,
        })
        .collect();

    let duration = start.elapsed();
    tracing::info!(duration = ?duration, results_count = results.len(), "Search successful"); // Log duration

    Ok(Json(SearchResponse { results }))
}

// Handler for GET /details/
#[utoipa::path(
    get,
    path = "/details/{id}/",
    params(
        ("id" = i64, Path, description = "ID do item para detalhes")
    ),
    responses(
        (status = 200, description = "Detalhes encontrados", body = Value),
        (status = 404, description = "Item não encontrado")
    )
)]
pub async fn details(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // consulta no banco
    let row =
        sqlx::query(r#"SELECT id, uuid, origin, image_key, extra FROM targets WHERE id = $1"#)
            .bind(id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Database query failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    match row {
        Some(record) => {
            let id: i64 = record.try_get("id").unwrap_or_default();
            let uuid: uuid::Uuid = record.try_get("uuid").unwrap();
            let origin: String = record.try_get("origin").unwrap_or_default();
            let image_key: String = record.try_get("image_key").unwrap_or_default();
            let extra: Option<serde_json::Value> = record.try_get("extra").unwrap_or(None);

            Ok(Json(json!({
                "id": id,
                "uuid": uuid,
                "origin": origin,
                "image_key": image_key,
                "extra": extra
            })))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

fn preprocess_image(
    img: DynamicImage,
    target_width: u32,
    target_height: u32,
) -> Result<Array<f32, Ix4>, StatusCode> {
    let resized_img = img.resize_exact(
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    );
    let rgb_img: ImageBuffer<Rgb<u8>, Vec<u8>> = resized_img.to_rgb8();

    let mut input_tensor = Array::zeros((1, 3, target_height as usize, target_width as usize));

    for (x, y, pixel) in rgb_img.enumerate_pixels() {
        let r = pixel[0] as f32;
        let g = pixel[1] as f32;
        let b = pixel[2] as f32;

        input_tensor[[0, 0, y as usize, x as usize]] = (b - 127.5) / 128.0;
        input_tensor[[0, 1, y as usize, x as usize]] = (g - 127.5) / 128.0;
        input_tensor[[0, 2, y as usize, x as usize]] = (r - 127.5) / 128.0;
    }

    Ok(input_tensor)
}
