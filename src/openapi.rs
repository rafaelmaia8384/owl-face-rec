// src/openapi.rs
use utoipa::OpenApi;

// Importe o módulo handlers para visibilidade (resolve "unresolved module `handlers`")
use crate::handlers;

// Isso traz as funções anotadas com #[utoipa::path] e as structs no escopo
// (O macro gera __path_* internamente, mas use crate::handlers; basta)

#[derive(OpenApi)]
#[openapi(
    paths(  // Agora resolve handlers:: porque o módulo está importado
        handlers::health_check,
        handlers::register,
        handlers::search,
        handlers::details
    ),
    components(
        schemas(  // Schemas agora no escopo via use crate::handlers;
            handlers::RegisterPayload,
            handlers::SearchPayload,
            handlers::SearchResponse,
            handlers::SearchResult
        )
    )
)]
pub struct ApiDoc;
