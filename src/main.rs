use crate::openapi::ApiDoc;
use axum::{
    routing::{get, post},
    Router,
};
use minio::s3::ClientBuilder;
use minio::s3::{
    client::Client, creds::StaticProvider, http::BaseUrl, response::BucketExistsResponse,
    types::S3Api,
};
use ort::{init, session::builder::GraphOptimizationLevel, session::Session};
use rayon::prelude::*;
use sqlx::postgres::PgPoolOptions;
use sqlx::Connection;
use sqlx::PgPool;
use sqlx::Row;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

mod handlers;
mod openapi;

#[derive(Clone)]
pub struct EmbeddingEntry {
    pub id: i64,
    pub uuid: Uuid,
    pub origin: String,
    pub embedding: Vec<f32>,
    pub image_key: String,
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        panic!("Vectors with different sizes!");
    }

    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for i in 0..a.len().min(b.len()) {
        dot_product += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}

#[derive(Clone)]
pub struct EmbeddingsStore {
    entries: Vec<EmbeddingEntry>,
}

pub struct SafeDetector {
    inner: Mutex<Box<dyn rustface::Detector>>,
}

impl SafeDetector {
    pub fn new(detector: Box<dyn rustface::Detector>) -> Self {
        Self {
            inner: Mutex::new(detector),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Box<dyn rustface::Detector>> {
        self.inner.lock().unwrap()
    }
}

unsafe impl Send for SafeDetector {}
unsafe impl Sync for SafeDetector {}

impl Default for EmbeddingsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingsStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add(
        &mut self,
        id: i64,
        uuid: Uuid,
        origin: String,
        embedding: Vec<f32>,
        image_key: String,
    ) {
        self.entries.push(EmbeddingEntry {
            id,
            uuid,
            embedding,
            origin,
            image_key,
        });
    }

    pub fn find_similar(
        &self,
        query: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Vec<(i64, Uuid, String, String, f32)> {
        let mut results: Vec<(i64, Uuid, String, String, f32)> = self
            .entries
            .par_iter()
            .map(|entry| {
                let similarity = cosine_similarity(query, &entry.embedding);
                (
                    entry.id,
                    entry.uuid,
                    entry.origin.clone(),
                    entry.image_key.clone(),
                    similarity,
                )
            })
            .filter(|&(_, _, _, _, similarity)| similarity >= threshold)
            .collect();

        results.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub onnx_session: Arc<Session>,
    pub db_pool: PgPool,
    pub embeddings_store: Arc<Mutex<EmbeddingsStore>>,
    pub facedetector: Arc<SafeDetector>,
    pub s3_client: Arc<Client>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables and initialize tracing
    dotenvy::dotenv().ok();

    // Get log level from LOG_LEVEL first, then RUST_LOG, or default to "debug"
    let log_level = std::env::var("LOG_LEVEL")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "debug".into());

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(log_level))
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Testing database connection...");

    // Get database connection parameters from environment variables
    let postgres_user = env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string());
    let postgres_password =
        env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgres".to_string());
    let postgres_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let postgres_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
    let postgres_db = env::var("POSTGRES_DB").unwrap_or_else(|_| "owlfacerec".to_string());

    // Connect to the target database for the application using a pool
    let target_db_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        postgres_user, postgres_password, postgres_host, postgres_port, postgres_db
    );

    tracing::info!("Initiating connection pool at {}...", target_db_url);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&target_db_url)
        .await?;

    // Ping the database to verify connection
    pool.acquire().await?.ping().await?;
    tracing::info!("Connection to target database '{}' successful", postgres_db);

    // Create 'targets' table if it doesn't exist
    tracing::info!("Ensuring 'targets' table exists...");
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targets (
            id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
            uuid UUID NOT NULL,
            origin VARCHAR(64) NOT NULL DEFAULT 'unknown',
            embeddings REAL[] NOT NULL,
            image_key VARCHAR(64) NOT NULL,
            extra JSONB
        );
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_targets_uuid ON targets (uuid);
        "#,
    )
    .execute(&pool)
    .await?;

    tracing::info!("'targets' table is ready");

    // Initialize ONNX Runtime environment globally
    init().with_name("ArcFaceApp").commit()?;
    tracing::info!("ONNX Runtime environment initialized");

    tracing::info!("Loading ArcFace ONNX model...");
    // Build session with absolute path to ONNX model
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("arcfaceresnet100-8.onnx");
    tracing::info!(model_path = ?model_path, "Using ONNX model file");
    let onnx_session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .commit_from_file(model_path.clone())?;

    tracing::info!(model_path = ?model_path, "ONNX model loaded successfully");

    // Carregue o detector de rostos aqui, similar ao carregamento do modelo ONNX
    tracing::info!("Loading face detection model...");
    let facedetect_model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("seeta_fd_frontal_v1.0.bin");
    tracing::info!(model_path = ?facedetect_model_path, "ONNX face detection model loaded successfully");

    let mut detector = rustface::create_detector(facedetect_model_path.to_str().unwrap())
        .expect("Face detector model load failed");

    let rustface_min_face_size: u32 = env::var("RUSTFACE_MIN_FACE_SIZE")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(50);
    let rustface_score_thresh: f64 = env::var("RUSTFACE_SCORE_THRESH")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(2.5);
    let rustface_pyramid_scale_factor: f32 = env::var("RUSTFACE_PYRAMID_SCALE_FACTOR")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.9);
    let rustface_slide_window_step_x: u32 = env::var("RUSTFACE_SLIDE_WINDOW_STEP_X")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(10);
    let rustface_slide_window_step_y: u32 = env::var("RUSTFACE_SLIDE_WINDOW_STEP_Y")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(10);

    detector.set_min_face_size(rustface_min_face_size);
    detector.set_score_thresh(rustface_score_thresh);
    detector.set_pyramid_scale_factor(rustface_pyramid_scale_factor);
    detector.set_slide_window_step(rustface_slide_window_step_x, rustface_slide_window_step_y);

    let safe_detector = SafeDetector::new(detector);

    tracing::info!("Face detection model loaded and configured successfully");

    // Minio client
    tracing::info!("Configuring Minio client...");
    let minio_bucket = env::var("MINIO_BUCKET").expect("MINIO_BUCKET must be set");
    let minio_endpoint = env::var("MINIO_ENDPOINT").expect("MINIO_ENDPOINT must be set");
    let minio_access_key = env::var("MINIO_ACCESS_KEY").expect("MINIO_ACCESS_KEY must be set");
    let minio_secret_key = env::var("MINIO_SECRET_KEY").expect("MINIO_SECRET_KEY must be set");

    let static_provider = StaticProvider::new(&minio_access_key, &minio_secret_key, None);
    let s3_client = ClientBuilder::new(minio_endpoint.parse::<BaseUrl>()?)
        .provider(Some(Box::new(static_provider)))
        .build()?;

    let resp: BucketExistsResponse = s3_client.bucket_exists(minio_bucket.clone()).send().await?;

    // Make 'bucket_name' bucket if not exist.
    if !resp.exists {
        tracing::info!("Creating bucket: {}", &minio_bucket);
        s3_client.create_bucket(&minio_bucket).send().await.unwrap();
    };

    tracing::info!(endpoint = %minio_endpoint, "Minio client configured successfully");

    // Inicializar o armazenamento de embeddings
    tracing::info!("Initializing embeddings store...");
    let mut embeddings_store = EmbeddingsStore::new();

    // Carregar todos os embeddings existentes do banco de dados
    tracing::info!("Loading existing embeddings from database into memory...");

    let all_embeddings = sqlx::query("SELECT id, uuid, embeddings, origin, image_key FROM targets")
        .fetch_all(&pool)
        .await?;

    if !all_embeddings.is_empty() {
        for record in &all_embeddings {
            let id: i64 = record.try_get("id")?;
            let uuid: Uuid = record.try_get("uuid")?;
            let origin: String = record.try_get("origin").unwrap_or_else(|_| "".to_string());
            let embeddings: Vec<f32> = record.try_get("embeddings")?;
            let image_key: String = record
                .try_get("image_key")
                .unwrap_or_else(|_| "".to_string());

            embeddings_store.add(id, uuid, origin, embeddings, image_key);
        }
        tracing::info!("Loaded {} embeddings into memory", embeddings_store.len());
    } else {
        tracing::info!("No existing embeddings found in database");
    }

    // Create the application state
    let app_state = AppState {
        onnx_session: Arc::new(onnx_session),
        db_pool: pool.clone(),
        embeddings_store: Arc::new(Mutex::new(embeddings_store)),
        facedetector: Arc::new(safe_detector),
        s3_client: Arc::new(s3_client),
    };

    let swagger_ui = SwaggerUi::new("/swagger").url("/api-docs/openapi.json", ApiDoc::openapi());

    let app = Router::new()
        .merge(swagger_ui)
        .route("/", get(handlers::health_check))
        .route("/health/", get(handlers::health_check))
        .route("/register/", post(handlers::register))
        .route("/search/", post(handlers::search))
        .route("/details/:id/", get(handlers::details))
        .with_state(app_state);

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr_str = format!("{}:{}", host, port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid address format");

    tracing::info!(address = %addr, "listening on address");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
