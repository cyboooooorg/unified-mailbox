use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthDto {
    pub status: HealthStatus,
    /// Server version.
    pub version: String,
}

/// Liveness probe; needs no token.
#[utoipa::path(get, path = "/health", security(()),
    responses((status = 200, description = "The server is up", body = HealthDto)))]
async fn health() -> Json<HealthDto> {
    Json(HealthDto {
        status: HealthStatus::Ok,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(health))
}
