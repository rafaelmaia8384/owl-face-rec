# OwlFaceRec

A high-performance face recognition API built with Rust that processes base64-encoded images and performs facial recognition using ONNX models. The system uses ArcFace ResNet-100 for generating face embeddings and provides both registration and search capabilities.

## Features

- **Face Recognition**: Uses ArcFace ResNet-100 ONNX model for generating high-quality face embeddings
- **Base64 Image Processing**: Accepts images in base64 format for easy integration
- **RESTful API**: Clean REST endpoints for registration and search operations
- **PostgreSQL Integration**: Persistent storage of face embeddings with automatic database setup
- **In-Memory Caching**: Fast similarity search using in-memory storage with parallel processing
- **Docker Support**: Complete containerization with Docker Compose for easy deployment
- **High Performance**: Built with Rust and optimized for speed and memory efficiency

## Architecture

The system consists of:

1. **ONNX Runtime**: Processes images through the ArcFace ResNet-100 model
2. **PostgreSQL Database**: Stores face embeddings persistently
3. **In-Memory Store**: Caches embeddings for fast similarity search
4. **REST API**: Provides HTTP endpoints for registration and search

## API Endpoints

### Health Check
- **GET** `/` or `/health/` - Returns 200 OK if service is running

### Register Face
- **POST** `/register/` - Register a new face embedding
- **Request Body**:
  ```json
  {
    "target_uuid": "550e8400-e29b-41d4-a716-446655440000",
    "image_base64": "iVBORw0KGgoAAAANSUhEUgAA...",
    "origin": "users"
  }
  ```
- **Response**: `201 Created` on success

### Search Faces
- **POST** `/search/` - Search for similar faces
- **Request Body**:
  ```json
  {
    "image_base64": "iVBORw0KGgoAAAANSUhEUgAA...",
    "threshold": 0.7,
    "limit": 10
  }
  ```
- **Response**:
  ```json
  {
    "results": [
      {
        "target_uuid": "550e8400-e29b-41d4-a716-446655440000",
        "similarity": 0.95,
        "origin": "users"
      }
    ]
  }
  ```

## Prerequisites

- Rust 1.81+ (for local development)
- Docker and Docker Compose (for containerized deployment)
- PostgreSQL (handled automatically with Docker Compose)
- ArcFace ResNet-100 ONNX model file

## Setup

### 1. Download the ONNX Model

Download the `arcfaceresnet100-8.onnx` model file and place it in the `models/` directory:

```bash
# The model should be placed at:
models/arcfaceresnet100-8.onnx
```

### 2. Environment Variables

Create a `.env` file in the project root (optional, defaults are provided):

```env
# Application settings
HOST=0.0.0.0
PORT=3000
RUST_LOG=info

# Database settings
POSTGRES_USER=postgres
POSTGRES_PASSWORD=postgres
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_DB=owlfacerec
```

## Running the Application

### Using Docker Compose (Recommended)

1. **Start the services**:
   ```bash
   docker-compose up -d
   ```

2. **View logs**:
   ```bash
   docker-compose logs -f app
   ```

3. **Stop the services**:
   ```bash
   docker-compose down
   ```

### Local Development

1. **Install dependencies**:
   ```bash
   cargo build
   ```

2. **Start PostgreSQL** (if not using Docker):
   ```bash
   # Using Docker for PostgreSQL only
   docker run --name postgres-owlfacerec \
     -e POSTGRES_USER=postgres \
     -e POSTGRES_PASSWORD=postgres \
     -e POSTGRES_DB=owlfacerec \
     -p 5432:5432 \
     -d postgres:15
   ```

3. **Run the application**:
   ```bash
   cargo run
   ```

## Usage Examples

### Register a Face

```bash
curl -X POST http://localhost:3000/register/ \
  -H "Content-Type: application/json" \
  -d '{
    "target_uuid": "550e8400-e29b-41d4-a716-446655440000",
    "image_base64": "iVBORw0KGgoAAAANSUhEUgAA...",
    "origin": "employee_photo"
  }'
```

### Search for Similar Faces

```bash
curl -X POST http://localhost:3000/search/ \
  -H "Content-Type: application/json" \
  -d '{
    "image_base64": "iVBORw0KGgoAAAANSUhEUgAA...",
    "threshold": 0.8,
    "limit": 5
  }'
```

## Technical Details

### Face Recognition Pipeline

1. **Image Decoding**: Base64 string is decoded to raw image bytes
2. **Image Loading**: Raw bytes are loaded into a `DynamicImage` using the `image` crate
3. **Preprocessing**: Image is resized to 112x112 pixels and normalized
4. **ONNX Inference**: Preprocessed image is fed through the ArcFace ResNet-100 model
5. **Embedding Extraction**: 512-dimensional face embedding is extracted from the model output

### Similarity Search

- Uses cosine similarity for comparing face embeddings
- Parallel processing with Rayon for fast similarity calculations
- Configurable threshold and result limits
- Results are sorted by similarity score (highest first)

### Database Schema

```sql
CREATE TABLE targets (
    uuid UUID NOT NULL,
    origin VARCHAR(64) NOT NULL DEFAULT 'unknown',
    embeddings REAL[] NOT NULL
);
```

## Performance

- **Memory Efficient**: In-memory caching with configurable limits
- **Parallel Processing**: Uses Rayon for parallel similarity calculations
- **Optimized ONNX**: Graph optimization level 3 for maximum performance
- **Connection Pooling**: PostgreSQL connection pooling for database operations

## Dependencies

- **axum**: Modern web framework for Rust
- **ort**: ONNX Runtime bindings for Rust
- **sqlx**: Async SQL toolkit with compile-time checked queries
- **image**: Image processing library
- **ndarray**: N-dimensional arrays for tensor operations
- **rayon**: Data parallelism library
- **uuid**: UUID generation and parsing
- **base64**: Base64 encoding/decoding

## Development

### Project Structure

```
owl-face-rec/
├── src/
│   ├── main.rs          # Application entry point and configuration
│   └── handlers.rs      # HTTP request handlers
├── models/
│   └── arcfaceresnet100-8.onnx  # ONNX model file
├── Dockerfile           # Container configuration
├── docker-compose.yml   # Service orchestration
└── Cargo.toml          # Rust dependencies
```

### Building from Source

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Check code
cargo check
```

## License

This project is licensed under the MIT License.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## Support

For issues and questions, please open an issue on the GitHub repository.
