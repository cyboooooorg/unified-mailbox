mod common;

use std::time::Duration;

use axum::body::Body;
use chrono::{TimeZone, Utc};
use common::{harness, harness_with_bus_capacity, next_chunk, pair, send_auth};
use domain::{DeviceId, Event, Job, JobId, JobKind, JobProgress, JobStatus};
use ports::EventBus;
use serde_json::{Value, json};

const WITHIN: Duration = Duration::from_secs(2);

fn job(id: i64) -> Job {
    Job {
        id: JobId::new(id),
        kind: JobKind::new("relabel").unwrap(),
        status: JobStatus::Running,
        progress: JobProgress {
            done: 3,
            failed: 0,
            total: Some(10),
        },
        error: None,
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        finished_at: None,
    }
}

/// Opens the stream for `token` and reads its first frame, the `retry`
/// one: the subscription exists once it has arrived.
async fn open(router: &axum::Router, token: &str) -> Body {
    let res = send_auth(router, "GET", "/api/v1/events", token).await;
    assert_eq!(res.status(), 200);
    let mut body = res.into_body();
    let first = next_chunk(&mut body, WITHIN).await.expect("a first frame");
    assert_eq!(first, "retry: 3000\n\n");
    body
}

/// The JSON of a `data:` frame.
fn data(chunk: &str) -> Value {
    let line = chunk
        .strip_prefix("data: ")
        .and_then(|rest| rest.strip_suffix("\n\n"))
        .unwrap_or_else(|| panic!("not a data frame: {chunk:?}"));
    serde_json::from_str(line).unwrap()
}

#[tokio::test]
async fn stream_headers() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = send_auth(&h.router, "GET", "/api/v1/events", &token).await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    assert_eq!(res.headers()["x-accel-buffering"], "no");
    assert_eq!(res.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn stream_starts_with_retry() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    // `open` checks the first frame.
    let _body = open(&h.router, &token).await;
}

#[tokio::test]
async fn published_job_arrives_as_json() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    h.bus.publish(Event::Job(job(7)));
    let chunk = next_chunk(&mut body, WITHIN).await.unwrap();
    assert_eq!(
        data(&chunk),
        json!({
            "type": "job",
            "job": {
                "id": 7,
                "kind": "relabel",
                "status": "running",
                "done": 3,
                "failed": 0,
                "total": 10,
                "error": null,
                "created_at": "2026-01-01T00:00:00Z",
                "finished_at": null,
            }
        })
    );
}

#[tokio::test]
async fn lag_gives_resync() {
    let h = harness_with_bus_capacity(1);
    let (_, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    h.bus.publish(Event::Job(job(1)));
    h.bus.publish(Event::Job(job(2)));
    let chunk = next_chunk(&mut body, WITHIN).await.unwrap();
    assert_eq!(data(&chunk), json!({"type": "resync"}));
    // The stream goes on with the newest event.
    let chunk = next_chunk(&mut body, WITHIN).await.unwrap();
    let event = data(&chunk);
    assert_eq!(event["type"], "job");
    assert_eq!(event["job"]["id"], 2);
}

#[tokio::test]
async fn ping_comment_arrives() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let router = http_api::router(
        h.state
            .clone()
            .with_ping_interval(Duration::from_millis(50)),
    );
    let mut body = open(&router, &token).await;
    let chunk = next_chunk(&mut body, WITHIN).await.unwrap();
    assert_eq!(chunk, ": ping\n\n");
}

#[tokio::test]
async fn own_revocation_ends_stream() {
    let h = harness();
    let (id, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    let res = send_auth(
        &h.router,
        "DELETE",
        &format!("/api/v1/devices/{id}"),
        &token,
    )
    .await;
    assert_eq!(res.status(), 204);
    assert_eq!(next_chunk(&mut body, WITHIN).await, None);
}

#[tokio::test]
async fn other_device_revocation_is_forwarded() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let (other, _) = pair(&h, "Tablet").await;
    let mut body = open(&h.router, &token).await;
    h.state
        .revoke_device
        .execute(DeviceId::new(other))
        .await
        .unwrap();
    let chunk = next_chunk(&mut body, WITHIN).await.unwrap();
    assert_eq!(
        data(&chunk),
        json!({"type": "device_revoked", "device_id": other})
    );
}

#[tokio::test]
async fn shutdown_ends_stream() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    h.shutdown.send(true).unwrap();
    assert_eq!(next_chunk(&mut body, WITHIN).await, None);
}
