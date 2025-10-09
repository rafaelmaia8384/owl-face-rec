use utoipa::OpenApi;

// Importe as structs necessárias
use crate::handlers;

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::health_check,
        handlers::register,
        handlers::search,
        handlers::details
    ),
    components(
        schemas(
            handlers::RegisterPayload,
            handlers::SearchPayload,
            handlers::SearchResponse,
            handlers::SearchResult
        )
    ),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "registration", description = "User registration endpoints"),
        (name = "search", description = "Face search endpoints"),
        (name = "details", description = "User details endpoints")
    ),
    info(
        title = "OwlFaceRec API",
        description = "API para reconhecimento facial usando ArcFace",
        version = "1.0.0"
    )
)]
pub struct ApiDoc;
