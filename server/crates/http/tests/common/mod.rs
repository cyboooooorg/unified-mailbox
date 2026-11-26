#![allow(dead_code)]

use std::sync::Arc;

use application::Deps;
use application::testing::{FixedClock, InMemoryDevices, InMemoryJobs, SeqSecrets};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response};
use chrono::{TimeZone, Utc};
use events_memory::MemoryBus;
use http_api::AppState;
use tower::ServiceExt;

pub struct Harness {
    pub router: Router,
    pub clock: Arc<FixedClock>,
    pub state: AppState,
    pub jobs: Arc<InMemoryJobs>,
    pub bus: Arc<MemoryBus>,
    pub shutdown: tokio::sync::watch::Sender<bool>,
}

pub fn harness() -> Harness {
    harness_with().0
}

/// A harness and its repository, to make the storage fail.
pub fn harness_with() -> (Harness, Arc<InMemoryDevices>) {
    let repo = Arc::new(InMemoryDevices::new());
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    ));
    let jobs = Arc::new(InMemoryJobs::new());
    let bus = Arc::new(MemoryBus::new(64));
    let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
    let state = AppState::new(
        Deps {
            repo: repo.clone(),
            clock: clock.clone(),
            secrets: Arc::new(SeqSecrets::new()),
            jobs: jobs.clone(),
            events: bus.clone(),
        },
        shutdown_rx,
    );
    let harness = Harness {
        router: http_api::router(state.clone()),
        clock,
        state,
        jobs,
        bus,
        shutdown,
    };
    (harness, repo)
}

pub async fn send(router: &Router, method: &str, uri: &str) -> Response<Body> {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

pub async fn json(response: Response<Body>) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Sends a request with the given headers and body.
pub async fn send_with(
    router: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: impl Into<Body>,
) -> Response<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    router
        .clone()
        .oneshot(builder.body(body.into()).unwrap())
        .await
        .unwrap()
}

/// Sends a request authenticated with `token`.
pub async fn send_auth(router: &Router, method: &str, uri: &str, token: &str) -> Response<Body> {
    let bearer = format!("Bearer {token}");
    send_with(
        router,
        method,
        uri,
        &[("authorization", &bearer)],
        Body::empty(),
    )
    .await
}

/// `POST /api/v1/pairing` with a JSON body.
pub async fn post_pairing(router: &Router, body: &serde_json::Value) -> Response<Body> {
    send_with(
        router,
        "POST",
        "/api/v1/pairing",
        &[("content-type", "application/json")],
        body.to_string(),
    )
    .await
}

/// A fresh pairing code, as created by the server's subcommand, in its
/// display form (`XXXX-XXXX`).
pub async fn new_code(h: &Harness) -> String {
    h.state
        .create_pairing_code
        .execute(None)
        .await
        .unwrap()
        .code
        .to_string()
}

/// Pairs a device through the API; returns its id and token.
pub async fn pair(h: &Harness, name: &str) -> (i64, String) {
    let code = new_code(h).await;
    let res = post_pairing(&h.router, &serde_json::json!({"code": code, "name": name})).await;
    assert_eq!(res.status(), 201);
    let body = json(res).await;
    (
        body["device_id"].as_i64().unwrap(),
        body["token"].as_str().unwrap().to_owned(),
    )
}

pub async fn text(response: Response<Body>) -> String {
    let bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}
