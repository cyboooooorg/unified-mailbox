//! Bearer-token authentication: the [`CurrentDevice`] extractor.

use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use domain::DeviceId;

use crate::error::{ApiError, ErrorCode};
use crate::state::AppState;

/// The device that made the request, authenticated by its bearer token.
///
/// Every route but `/health` and `/pairing` takes it: a handler without it
/// is reachable without a token. It must come before any other extractor,
/// so that an unauthenticated request learns nothing else (axum runs
/// extractors in order).
#[derive(Debug, Clone, Copy)]
pub struct CurrentDevice(pub DeviceId);

impl FromRequestParts<AppState> for CurrentDevice {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(bearer_token)
            .ok_or_else(unauthorized)?;
        state
            .authenticate_device
            .execute(token)
            .await?
            .map(CurrentDevice)
            .ok_or_else(unauthorized)
    }
}

/// The token of `Bearer <token>`: the scheme in any case, then one or
/// more spaces. `None` for any other shape.
fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, rest) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = rest.trim_start_matches(' ');
    (!token.is_empty()).then_some(token)
}

/// The same answer for a missing, malformed, unknown or revoked token.
fn unauthorized() -> ApiError {
    ApiError::new(ErrorCode::Unauthorized, "missing or invalid bearer token")
}

#[cfg(test)]
mod tests {
    use super::bearer_token;

    #[test]
    fn parses_the_bearer_scheme() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bEaReR   abc"), Some("abc"));
        assert_eq!(bearer_token("Bearer "), None);
        assert_eq!(bearer_token("Bearer"), None);
        assert_eq!(bearer_token("Bearerabc"), None);
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("abc"), None);
    }
}
