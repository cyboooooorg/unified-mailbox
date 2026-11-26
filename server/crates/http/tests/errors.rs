mod common;

use common::{harness, json, send};

#[tokio::test]
async fn health_without_token() {
    let h = harness();
    let res = send(&h.router, "GET", "/api/v1/health").await;
    assert_eq!(res.status(), 200);
    let body = json(res).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn unknown_route_404() {
    let h = harness();
    let res = send(&h.router, "GET", "/api/v1/nope").await;
    assert_eq!(res.status(), 404);
    assert_eq!(json(res).await["error"]["code"], "not_found");
}

#[tokio::test]
async fn wrong_method_405() {
    let h = harness();
    let res = send(&h.router, "DELETE", "/api/v1/health").await;
    assert_eq!(res.status(), 405);
    assert_eq!(json(res).await["error"]["code"], "method_not_allowed");
}

#[tokio::test]
async fn security_headers_on_success_and_error() {
    let h = harness();
    for (method, uri) in [("GET", "/api/v1/health"), ("GET", "/api/v1/nope")] {
        let res = send(&h.router, method, uri).await;
        assert_eq!(res.headers()["cache-control"], "no-store", "{uri}");
        assert_eq!(res.headers()["x-content-type-options"], "nosniff", "{uri}");
    }
}
