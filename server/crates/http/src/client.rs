//! Static serving of the built web client, outside `/api`.

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, Request, Response, StatusCode, header};
use axum::response::IntoResponse;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

use crate::error::{ApiError, ErrorCode};

const IMMUTABLE_PREFIX: &str = "/_app/immutable/";
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// What the server alone can say; the script hashes come from the
/// `<meta>` tag the client build writes into `index.html`.
const INDEX_CSP: &str = "frame-ancestors 'none'; object-src 'none'; base-uri 'none'";

/// The directory holding the built web client. It is validated at startup
/// (a directory with an `index.html`): building one does no I/O.
#[derive(Clone, Debug)]
pub struct ClientDir(Arc<PathBuf>);

impl ClientDir {
    pub fn new(path: PathBuf) -> ClientDir {
        ClientDir(Arc::new(path))
    }
}

/// Serves everything the API routes did not match.
pub(crate) async fn fallback(client: Option<ClientDir>, request: Request<Body>) -> Response<Body> {
    let path = request.uri().path().to_owned();
    if path == "/api" || path.starts_with("/api/") {
        return ApiError::new(ErrorCode::NotFound, "not found").into_response();
    }
    let Some(client) = client else {
        return ApiError::new(ErrorCode::NotFound, "not found").into_response();
    };
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return ApiError::new(ErrorCode::MethodNotAllowed, "method not allowed").into_response();
    }
    serve(&client, request, &path).await
}

async fn serve(client: &ClientDir, request: Request<Body>, path: &str) -> Response<Body> {
    let (parts, _) = request.into_parts();
    let again = |parts: &axum::http::request::Parts| {
        let mut builder = Request::builder()
            .method(parts.method.clone())
            .uri(parts.uri.clone());
        for (name, value) in &parts.headers {
            builder = builder.header(name, value);
        }
        builder
            .body(Body::empty())
            .expect("request rebuilt from valid parts")
    };
    let dir = ServeDir::new(client.0.as_path());
    let Ok(response) = dir.oneshot(again(&parts)).await;
    let mut response = response.map(Body::new);
    let mut is_index = path == "/" || path == "/index.html";
    if response.status() == StatusCode::NOT_FOUND && is_spa_route(path) {
        let index = ServeFile::new(client.0.join("index.html"));
        let Ok(served) = index.oneshot(again(&parts)).await;
        response = served.map(Body::new);
        is_index = true;
    }
    decorate(&mut response, path, is_index);
    response
}

/// A path whose last segment has no extension is a page of the app.
fn is_spa_route(path: &str) -> bool {
    !path.rsplit('/').next().unwrap_or_default().contains('.')
}

fn decorate(response: &mut Response<Body>, path: &str, is_index: bool) {
    let headers = response.headers_mut();
    let cache = if path.starts_with(IMMUTABLE_PREFIX) && !is_index {
        IMMUTABLE
    } else {
        "no-cache"
    };
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    if is_index {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(INDEX_CSP),
        );
    }
}
