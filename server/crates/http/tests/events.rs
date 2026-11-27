mod common;

use std::sync::Arc;
use std::time::Duration;

use application::testing::{InMemoryDevices, InMemorySettings, SeqSecrets};
use application::{Deps, DeviceIsActive};

use axum::body::Body;
use chrono::{TimeZone, Utc};
use common::{Harness, harness, harness_with_bus_capacity, next_chunk, pair, send_auth};
use domain::{DeviceId, Event, Job, JobId, JobKind, JobProgress, JobStatus};
use http_api::AppState;
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
async fn stream_headers_and_retry_frame() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = send_auth(&h.router, "GET", "/api/v1/events", &token).await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    assert_eq!(res.headers()["x-accel-buffering"], "no");
    assert_eq!(res.headers()["cache-control"], "no-store");
    let mut body = res.into_body();
    let first = next_chunk(&mut body, WITHIN).await.expect("a first frame");
    assert_eq!(first, "retry: 3000\n\n");
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
async fn settings_changed_arrives_as_its_type() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    h.bus.publish(Event::SettingsChanged);
    let chunk = next_chunk(&mut body, WITHIN).await.unwrap();
    assert_eq!(data(&chunk), json!({"type": "settings_changed"}));
}

#[tokio::test]
async fn failed_job_carries_its_error_and_end() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    let failed = Job {
        status: JobStatus::Failed,
        error: Some("invented failure".to_owned()),
        finished_at: Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 5, 0).unwrap()),
        ..job(8)
    };
    h.bus.publish(Event::Job(failed));
    let event = data(&next_chunk(&mut body, WITHIN).await.unwrap());
    assert_eq!(event["job"]["status"], "failed");
    assert_eq!(event["job"]["error"], "invented failure");
    assert_eq!(event["job"]["finished_at"], "2026-01-01T00:05:00Z");
}

#[tokio::test]
async fn lag_sends_resync_then_ends_the_stream() {
    let h = harness_with_bus_capacity(1);
    let (_, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    h.bus.publish(Event::Job(job(1)));
    h.bus.publish(Event::Job(job(2)));
    let chunk = next_chunk(&mut body, WITHIN).await.unwrap();
    assert_eq!(data(&chunk), json!({"type": "resync"}));
    // A lost event may be the caller's own revocation: the client
    // reconnects, which authenticates it again.
    assert_eq!(next_chunk(&mut body, WITHIN).await, None);
}

/// A router whose check of the caller after subscribing reads `devices`,
/// while authentication still reads the harness's store.
fn router_checking_in(h: &Harness, devices: Arc<InMemoryDevices>) -> axum::Router {
    let elsewhere = Deps {
        repo: devices,
        clock: h.clock.clone(),
        secrets: Arc::new(SeqSecrets::new()),
        jobs: h.jobs.clone(),
        events: h.bus.clone(),
        settings: Arc::new(InMemorySettings::new()),
    };
    http_api::router(AppState {
        device_is_active: Arc::new(DeviceIsActive::new(elsewhere)),
        ..h.state.clone()
    })
}

#[tokio::test]
async fn stream_of_a_device_found_inactive_after_subscribing_ends() {
    // The revocation happens between authentication and subscription: the
    // router cannot order them, so the check answers from a store where
    // the device is unknown while authentication still finds it.
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let router = router_checking_in(&h, Arc::new(InMemoryDevices::new()));
    let mut body = open(&router, &token).await;
    h.bus.publish(Event::Job(job(1)));
    assert_eq!(next_chunk(&mut body, WITHIN).await, None);
}

#[tokio::test]
async fn failed_check_after_subscribing_is_a_500() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let devices = Arc::new(InMemoryDevices::new());
    devices.fail_reads(true);
    let router = router_checking_in(&h, devices);
    let res = send_auth(&router, "GET", "/api/v1/events", &token).await;
    assert_eq!(res.status(), 500);
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
    let res = send_auth(&h.router, "GET", "/api/v1/events", &token).await;
    assert_eq!(res.status(), 401);
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

#[tokio::test]
async fn dropped_shutdown_sender_ends_stream() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let mut body = open(&h.router, &token).await;
    drop(h.shutdown);
    assert_eq!(next_chunk(&mut body, WITHIN).await, None);
}
