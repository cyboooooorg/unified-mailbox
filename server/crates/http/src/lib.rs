//! Inbound HTTP adapter: axum routes under `/api/v1`, the error format and
//! the OpenAPI contract (`api/openapi.json`, `mise run openapi`).

mod auth;
mod devices;
mod error;
mod extract;
mod health;
mod jobs;
mod state;

use axum::Router;
use axum::http::{HeaderName, HeaderValue, header};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;
use utoipa::openapi::OpenApi;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityRequirement, SecurityScheme};
use utoipa::{Modify, OpenApi as _};
use utoipa_axum::router::OpenApiRouter;

pub use auth::CurrentDevice;
pub use error::{ApiError, ErrorBody, ErrorCode, ErrorDetail};
pub use extract::{ApiJson, ApiPath};
pub use state::AppState;

#[derive(utoipa::OpenApi)]
#[openapi(
    info(
        title = "Unified Mailbox API",
        description = "API of the Unified Mailbox server. Every route but /health and /pairing requires a per-device bearer token."
    ),
    modifiers(&Bearer),
    components(schemas(ErrorBody, ErrorDetail, ErrorCode, jobs::JobDto, jobs::JobStatusDto))
)]
struct ApiDoc;

/// Declares the `bearer` scheme and requires it everywhere by default.
struct Bearer;

impl Modify for Bearer {
    fn modify(&self, openapi: &mut OpenApi) {
        openapi.info.version = env!("CARGO_PKG_VERSION").to_owned();
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("Per-device token obtained by pairing"))
                    .build(),
            ),
        );
        openapi.security = Some(vec![SecurityRequirement::new(
            "bearer",
            Vec::<String>::new(),
        )]);
    }
}

fn api() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi()).nest(
        "/api/v1",
        health::routes()
            .merge(devices::routes())
            .merge(jobs::routes()),
    )
}

/// The whole application, layers included.
pub fn router(state: AppState) -> Router {
    let (router, _) = api().split_for_parts();
    router
        .fallback(|| async { ApiError::new(ErrorCode::NotFound, "not found") })
        .method_not_allowed_fallback(|| async {
            ApiError::new(ErrorCode::MethodNotAllowed, "method not allowed")
        })
        .with_state(state)
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(
            TraceLayer::new_for_http()
                // The path only: the query string and the headers may carry
                // a secret.
                .make_span_with(|request: &axum::http::Request<_>| {
                    tracing::info_span!(
                        "request",
                        method = %request.method(),
                        path = request.uri().path(),
                    )
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
}

/// The OpenAPI document, built from the same routes as [`router`].
pub fn openapi() -> OpenApi {
    api().split_for_parts().1
}
