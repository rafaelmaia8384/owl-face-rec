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
        (name = "health", description = "Health check endpoint"),
        (name = "registration", description = "Face registration endpoint"),
        (name = "search", description = "Face search endpoint"),
        (name = "details", description = "Target details endpoint")
    ),
    info(
        title = "OwlFaceRec",
        description = "Desenvolvido pela Diretoria de Inteligência da PMPB",
        version = "1.0.0"
    )
)]
pub struct ApiDoc;
