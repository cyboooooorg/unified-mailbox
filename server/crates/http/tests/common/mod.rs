#![allow(dead_code)]

use std::sync::Arc;

use application::Deps;
use application::testing::{FixedClock, InMemoryDevices, SeqSecrets};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response};
use chrono::{TimeZone, Utc};
use http_api::AppState;
use tower::ServiceExt;

pub struct Harness {
    pub router: Router,
    pub clock: Arc<FixedClock>,
    pub state: AppState,
}

pub fn harness() -> Harness {
    let clock = Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    ));
    let state = AppState::new(Deps {
        repo: Arc::new(InMemoryDevices::new()),
        clock: clock.clone(),
        secrets: Arc::new(SeqSecrets::new()),
    });
    Harness {
        router: http_api::router(state.clone()),
        clock,
        state,
    }
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
