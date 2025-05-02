use axum::{extract::State, http::StatusCode, Json};
use base64::{engine::general_purpose, Engine as _};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgb};
use ndarray::{Array, Ix4};
use ort::{inputs, session::Session, session::SessionOutputs, value::Value};
use serde::{Deserialize, Serialize};
use sqlx;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState; // Import AppState from main.rs

// --- Helper function for Image Processing and Embedding Extraction ---

async fn get_embedding_from_base64(
    image_base64: &str,
    onnx_session: &Arc<Session>,
) -> Result<Vec<f32>, StatusCode> {
    // 1. Decode Base64
    let image_bytes = general_purpose::STANDARD
        .decode(image_base64)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to decode base64 image");
            StatusCode::BAD_REQUEST
        })?;
    tracing::debug!(image_size = image_bytes.len(), "Base64 decoded");

    // 2. Load Image from bytes
    let img: DynamicImage = image::load_from_memory(&image_bytes).map_err(|e| {
        tracing::error!(error = %e, "Failed to load image from bytes");
        StatusCode::BAD_REQUEST
    })?;
    tracing::debug!(dims = ?img.dimensions(), "Image loaded");

    // 3. Preprocess Image
    let input_array: Array<f32, Ix4> = preprocess_image(img, 112, 112).map_err(|e| {
        tracing::error!(error = %e, "Failed to preprocess image");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    tracing::debug!(shape = ?input_array.shape(), "Image preprocessed");

    // 4. Prepare ONNX Input Value
    let shape: Vec<usize> = input_array.shape().to_vec();
    let raw_vec = input_array.into_raw_vec();
    let input_value = Value::from_array((shape, raw_vec)).map_err(|e| {
        tracing::error!(error = %e, "Failed to create input value from array");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 5. Prepare session inputs and run ONNX Inference
    let session_inputs = inputs![input_value].map_err(|e| {
        tracing::error!(error = %e, "Failed to create session inputs");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // NOTE: Consider if session.run() needs to be blocking or if it's already async-friendly.
    // If it's blocking, might need tokio::task::spawn_blocking for CPU-bound work.
    let outputs: SessionOutputs = onnx_session.run(session_inputs).map_err(|e| {
        tracing::error!(error = %e, "ONNX inference failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 6. Process Output (Get Embedding)
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
}

// --- Struct Definitions ---

// Define a response payload struct
#[derive(Serialize)]
pub struct ResponsePayload {
    reply: String,
}

// Define the request payload for /register/
#[derive(Deserialize)]
pub struct RegisterPayload {
    target_uuid: Uuid,
    image_base64: String,
    origin: String,
}

// Define the request payload for /search/
#[derive(Deserialize)]
pub struct SearchPayload {
    image_base64: String,
    threshold: Option<f32>,
    limit: Option<usize>,
}

// Define the response for /search/
#[derive(Serialize)]
pub struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Serialize)]
pub struct SearchResult {
    target_uuid: String,
    similarity: f32,
    origin: String,
}

// --- Handlers ---

// Handler for GET / route, returns 200 OK
pub async fn health_check() -> axum::http::StatusCode {
    axum::http::StatusCode::OK
}

// Handler for POST /register/
pub async fn register(
    State(state): State<AppState>, // Extract state
    Json(payload): Json<RegisterPayload>,
) -> Result<StatusCode, StatusCode> {
    let target_uuid = payload.target_uuid;
    let origin = payload.origin.clone();
    tracing::debug!(%target_uuid, %origin, "Received registration request");

    // Get embedding using the helper function
    let embedding_vec =
        match get_embedding_from_base64(&payload.image_base64, &state.onnx_session).await {
            Ok(vec) => vec,
            Err(status) => return Err(status),
        };
    tracing::info!(%target_uuid, "Embedding calculated (first 5 values): {:?}", &embedding_vec[..5.min(embedding_vec.len())]);

    // Store the embedding in the database
    tracing::info!(%target_uuid, %origin, "Storing embedding in the database...");
    match sqlx::query("INSERT INTO targets (uuid, embeddings, origin) VALUES ($1, $2, $3)")
        .bind(target_uuid)
        .bind(&embedding_vec[..])
        .bind(&origin)
        .execute(&state.db_pool)
        .await
    {
        Ok(_) => {
            tracing::info!(%target_uuid, "Successfully stored embedding in the database.");

            // Add the embedding to in-memory storage
            tracing::info!(%target_uuid, %origin, "Adding embedding to in-memory store...");
            let mut embeddings_store = match state.embeddings_store.lock() {
                Ok(store) => store,
                Err(e) => {
                    tracing::error!(%target_uuid, error = %e, "Failed to lock embeddings store");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };
            embeddings_store.add(target_uuid, origin.clone(), embedding_vec.clone());
            tracing::info!(%target_uuid, "Successfully added embedding to in-memory store");
            tracing::info!(%target_uuid, "Total embeddings in memory: {}", embeddings_store.len());

            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            tracing::error!(%target_uuid, error = %e, "Failed to store embedding in database");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Handler for POST /search/
pub async fn search(
    State(state): State<AppState>,
    Json(payload): Json<SearchPayload>,
) -> Result<Json<SearchResponse>, StatusCode> {
    tracing::debug!("Received search request");

    // Get query embedding using the helper function
    let embedding_vec =
        match get_embedding_from_base64(&payload.image_base64, &state.onnx_session).await {
            Ok(vec) => vec,
            Err(status) => return Err(status),
        };
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
        .map(|(uuid, origin, similarity)| SearchResult {
            target_uuid: uuid.to_string(),
            similarity,
            origin,
        })
        .collect();

    Ok(Json(SearchResponse { results }))
}

// --- Image Preprocessing Helper (moved here for locality) ---

fn preprocess_image(
    img: DynamicImage,
    target_width: u32,
    target_height: u32,
) -> Result<Array<f32, Ix4>, Box<dyn std::error::Error>> {
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
