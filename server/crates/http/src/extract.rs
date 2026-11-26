//! Extractors whose rejections use the API error format.

use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{FromRequest, FromRequestParts, Json, Path, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::error::{ApiError, ErrorCode};

/// `axum::Json` with API errors: 413, 415, and 422 for any other failure.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ApiJson(value)),
            Err(rejection) => Err(json_error(rejection)),
        }
    }
}

fn json_error(rejection: JsonRejection) -> ApiError {
    match rejection.status() {
        StatusCode::PAYLOAD_TOO_LARGE => {
            ApiError::new(ErrorCode::PayloadTooLarge, "request body too large")
        }
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ApiError::new(
            ErrorCode::UnsupportedMediaType,
            "content type must be application/json",
        ),
        _ => ApiError::new(ErrorCode::InvalidRequest, "invalid request body"),
    }
}

/// `axum::extract::Path` whose rejection is a `not_found`.
pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| ApiPath(value))
            .map_err(|_: PathRejection| ApiError::new(ErrorCode::NotFound, "not found"))
    }
}
