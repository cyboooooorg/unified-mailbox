mod common;

use chrono::{TimeZone, Utc};
use common::{harness, json, pair, send_auth};
use domain::{Job, JobId, JobKind, JobProgress, JobStatus};

fn job(id: i64, status: JobStatus, total: Option<u64>) -> Job {
    let finished = status != JobStatus::Running;
    Job {
        id: JobId::new(id),
        kind: JobKind::new("relabel").unwrap(),
        status,
        progress: JobProgress {
            done: 2,
            failed: 1,
            total,
        },
        error: None,
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, id as u32, 0).unwrap(),
        finished_at: finished.then(|| Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0).unwrap()),
    }
}

#[tokio::test]
async fn list_jobs_returns_running_oldest_first() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    h.jobs.insert(job(2, JobStatus::Running, Some(10)));
    h.jobs.insert(job(1, JobStatus::Running, None));
    h.jobs.insert(job(3, JobStatus::Succeeded, None));
    let res = send_auth(&h.router, "GET", "/api/v1/jobs", &token).await;
    assert_eq!(res.status(), 200);
    let body = json(res).await;
    let ids: Vec<i64> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [1, 2]);
    assert_eq!(body[0]["status"], "running");
    assert_eq!(body[1]["total"], 10);
}

#[tokio::test]
async fn get_job_returns_dto_with_null_total() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    h.jobs.insert(job(1, JobStatus::Succeeded, None));
    let res = send_auth(&h.router, "GET", "/api/v1/jobs/1", &token).await;
    assert_eq!(res.status(), 200);
    let body = json(res).await;
    assert_eq!(body["kind"], "relabel");
    assert_eq!(body["status"], "succeeded");
    assert_eq!(body["done"], 2);
    assert_eq!(body["failed"], 1);
    assert_eq!(body["total"], serde_json::Value::Null);
    assert!(body.as_object().unwrap().contains_key("total"));
    assert_eq!(body["error"], serde_json::Value::Null);
    assert_eq!(body["created_at"], "2026-01-01T00:01:00Z");
    assert_eq!(body["finished_at"], "2026-01-01T01:00:00Z");
}

#[tokio::test]
async fn get_unknown_job_404() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = send_auth(&h.router, "GET", "/api/v1/jobs/42", &token).await;
    assert_eq!(res.status(), 404);
    assert_eq!(json(res).await["error"]["code"], "not_found");
}

#[tokio::test]
async fn get_job_non_numeric_id_404() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = send_auth(&h.router, "GET", "/api/v1/jobs/abc", &token).await;
    assert_eq!(res.status(), 404);
    assert_eq!(json(res).await["error"]["code"], "not_found");
}

#[test]
fn job_dto_fields_required_in_contract() {
    let doc = serde_json::to_value(http_api::openapi()).unwrap();
    let schema = &doc["components"]["schemas"]["JobDto"];
    let required = schema["required"].as_array().unwrap();
    for field in ["total", "error", "finished_at"] {
        assert!(required.iter().any(|f| f == field), "{field} not required");
        let ty = &schema["properties"][field];
        assert!(
            ty.to_string().contains("null"),
            "{field} not nullable: {ty}"
        );
    }
}
