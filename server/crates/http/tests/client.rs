mod common;

use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::http::Response;
use common::{harness, send};
use http_api::{ClientDir, router, router_with_client};
use http_body_util::BodyExt;

const CSP: &str = "frame-ancestors 'none'; object-src 'none'; base-uri 'none'";

fn write_client(root: &Path) {
    std::fs::create_dir_all(root.join("_app/immutable")).unwrap();
    std::fs::write(root.join("index.html"), "<p>app</p>").unwrap();
    std::fs::write(root.join("_app/immutable/app.abc123.js"), "let a = 1;").unwrap();
    std::fs::write(root.join("robots.txt"), "User-agent: *").unwrap();
}

/// A router serving a fresh client directory; the tempdir holds `root/`
/// (the client) and, next to it, whatever a test hides outside of it.
fn with_client() -> (Router, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    write_client(&root);
    let app = router_with_client(harness().state.clone(), Some(ClientDir::new(root)));
    (app, tmp)
}

async fn text(response: Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn header<'a>(response: &'a Response<Body>, name: &str) -> Option<&'a str> {
    response.headers().get(name).map(|v| v.to_str().unwrap())
}

#[tokio::test]
async fn root_serves_index() {
    let (app, _tmp) = with_client();
    for uri in ["/", "/index.html"] {
        let response = send(&app, "GET", uri).await;
        assert_eq!(response.status(), 200, "{uri}");
        assert_eq!(
            header(&response, "cache-control"),
            Some("no-cache"),
            "{uri}"
        );
        assert_eq!(
            header(&response, "content-security-policy"),
            Some(CSP),
            "{uri}"
        );
        assert_eq!(
            header(&response, "referrer-policy"),
            Some("no-referrer"),
            "{uri}"
        );
        assert_eq!(
            header(&response, "x-content-type-options"),
            Some("nosniff"),
            "{uri}"
        );
        assert_eq!(text(response).await, "<p>app</p>", "{uri}");
    }
}

#[tokio::test]
async fn deep_link_serves_index() {
    let (app, _tmp) = with_client();
    for uri in ["/jobs/42", "/jobs?x=1"] {
        let response = send(&app, "GET", uri).await;
        assert_eq!(response.status(), 200, "{uri}");
        assert_eq!(header(&response, "content-security-policy"), Some(CSP));
        assert_eq!(header(&response, "cache-control"), Some("no-cache"));
        assert_eq!(text(response).await, "<p>app</p>", "{uri}");
    }
}

#[tokio::test]
async fn head_deep_link() {
    let (app, _tmp) = with_client();
    let response = send(&app, "HEAD", "/settings").await;
    assert_eq!(response.status(), 200);
    assert_eq!(text(response).await, "");
}

#[tokio::test]
async fn immutable_asset() {
    let (app, _tmp) = with_client();
    let response = send(&app, "GET", "/_app/immutable/app.abc123.js").await;
    assert_eq!(response.status(), 200);
    assert!(
        header(&response, "content-type")
            .unwrap()
            .contains("javascript")
    );
    assert_eq!(
        header(&response, "cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(header(&response, "referrer-policy"), Some("no-referrer"));
    assert_eq!(header(&response, "x-content-type-options"), Some("nosniff"));
    assert_eq!(header(&response, "content-security-policy"), None);
}

#[tokio::test]
async fn other_asset_no_cache() {
    let (app, _tmp) = with_client();
    let response = send(&app, "GET", "/robots.txt").await;
    assert_eq!(response.status(), 200);
    assert_eq!(header(&response, "cache-control"), Some("no-cache"));
    assert_eq!(header(&response, "content-security-policy"), None);
    assert_eq!(header(&response, "referrer-policy"), Some("no-referrer"));
}

#[tokio::test]
async fn missing_asset_is_404() {
    let (app, _tmp) = with_client();
    for uri in ["/_app/immutable/gone.js", "/favicon.ico"] {
        let response = send(&app, "GET", uri).await;
        assert_eq!(response.status(), 404, "{uri}");
        assert_eq!(
            header(&response, "cache-control"),
            Some("no-cache"),
            "{uri}"
        );
        assert_ne!(text(response).await, "<p>app</p>", "{uri}");
    }
}

#[tokio::test]
async fn api_paths_stay_json() {
    let (app, _tmp) = with_client();
    for uri in ["/api", "/api/unknown", "/api/v2/x"] {
        let response = send(&app, "GET", uri).await;
        assert_eq!(response.status(), 404, "{uri}");
        let body: serde_json::Value = serde_json::from_str(&text(response).await).unwrap();
        assert_eq!(body["error"]["code"], "not_found", "{uri}");
    }
    let response = send(&app, "GET", "/api/v1/health").await;
    assert_eq!(response.status(), 200);
    assert_eq!(header(&response, "cache-control"), Some("no-store"));
}

#[tokio::test]
async fn post_root_is_405() {
    let (app, _tmp) = with_client();
    let response = send(&app, "POST", "/").await;
    assert_eq!(response.status(), 405);
    let body: serde_json::Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(body["error"]["code"], "method_not_allowed");
}

#[tokio::test]
async fn no_escape() {
    let (app, tmp) = with_client();
    std::fs::write(tmp.path().join("secret.txt"), "top secret").unwrap();
    for uri in ["/../secret.txt", "/%2e%2e/secret.txt"] {
        let response = send(&app, "GET", uri).await;
        let status = response.status();
        assert_ne!(text(response).await, "top secret", "{uri} ({status})");
    }
}

#[tokio::test]
async fn without_client_dir() {
    let app = router(harness().state.clone());
    let response = send(&app, "GET", "/").await;
    assert_eq!(response.status(), 404);
    let body: serde_json::Value = serde_json::from_str(&text(response).await).unwrap();
    assert_eq!(body["error"]["code"], "not_found");
}
