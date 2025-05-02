use axum::{
    routing::{get, post},
    Router,
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
use uuid::Uuid;

mod handlers;

// Estructura para associar uuid com embeddings
#[derive(Clone)]
pub struct EmbeddingEntry {
    pub uuid: Uuid,
    pub origin: String,
    pub embedding: Vec<f32>,
}

// Implementação de funções de similaridade para embeddings
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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

// Armazenamento e função de busca para embeddings
#[derive(Clone)]
pub struct EmbeddingsStore {
    entries: Vec<EmbeddingEntry>,
}

impl EmbeddingsStore {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn add(&mut self, uuid: Uuid, origin: String, embedding: Vec<f32>) {
        self.entries.push(EmbeddingEntry {
            uuid,
            embedding,
            origin,
        });
    }

    pub fn find_similar(
        &self,
        query: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Vec<(Uuid, String, f32)> {
        let mut results: Vec<(Uuid, String, f32)> = self
            .entries
            .par_iter()
            .map(|entry| {
                let similarity = cosine_similarity(query, &entry.embedding);
                (entry.uuid, entry.origin.clone(), similarity)
            })
            .filter(|&(_, _, similarity)| similarity >= threshold)
            .collect();

        // Ordenar por similaridade (maior primeiro)
        results.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Limitar o número de resultados
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
    onnx_session: Arc<Session>,
    db_pool: PgPool,
    embeddings_store: Arc<Mutex<EmbeddingsStore>>,
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

    // 4. Connect to the target database for the application
    let target_db_url = format!(
        "postgres://{}:{}@{}:{}/{}", // Connect to the target db
        postgres_user, postgres_password, postgres_host, postgres_port, postgres_db
    );

    tracing::info!("Checking target database '{}'...", postgres_db);
    let pool = PgPoolOptions::new()
        .max_connections(5) // Increased pool size
        .connect(&target_db_url)
        .await?;

    // 5. Ping the database to verify connection
    pool.acquire().await?.ping().await?;
    tracing::info!(
        "Connection to target database '{}' successful.",
        postgres_db
    );

    // 6. Create 'targets' table if it doesn't exist
    tracing::info!("Ensuring 'targets' table exists...");
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS targets (
            uuid UUID NOT NULL,
            origin VARCHAR(64) NOT NULL DEFAULT 'unknown',
            embeddings REAL[] NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await?;
    tracing::info!("'targets' table is ready.");

    // Initialize ONNX Runtime environment globally
    init().with_name("ArcFaceApp").commit()?;
    tracing::info!("ONNX Runtime environment initialized.");

    tracing::info!("Loading ArcFace ONNX model...");
    // Build session with absolute path to ONNX model
    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("arcfaceresnet100-8.onnx");
    tracing::info!(model_path = ?model_path, "Using ONNX model file");
    let onnx_session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .commit_from_file(model_path.clone())?;

    tracing::info!(model_path = ?model_path, "ONNX model loaded successfully.");

    // Inicializar o armazenamento de embeddings
    tracing::info!("Initializing embeddings store...");
    let mut embeddings_store = EmbeddingsStore::new();

    // Carregar todos os embeddings existentes do banco de dados
    tracing::info!("Loading existing embeddings from database into memory...");
    let all_embeddings = sqlx::query("SELECT uuid, embeddings, origin FROM targets")
        .fetch_all(&pool)
        .await?;

    if !all_embeddings.is_empty() {
        for record in &all_embeddings {
            let uuid: Uuid = record.try_get("uuid")?;
            let origin: String = record.try_get("origin").unwrap_or_else(|_| "".to_string());
            let embeddings: Vec<f32> = record.try_get("embeddings")?;

            embeddings_store.add(uuid, origin, embeddings);
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
    };

    // build our application with multiple routes and state
    let app = Router::new()
        .route("/", get(handlers::health_check))
        .route("/health/", get(handlers::health_check))
        .route("/register/", post(handlers::register))
        .route("/search/", post(handlers::search))
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
