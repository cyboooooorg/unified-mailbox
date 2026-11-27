mod common;

use std::time::Duration;

use common::{harness, json, next_chunk, pair, send_auth, send_with};
use serde_json::{Value, json};

const URI: &str = "/api/v1/settings/language";

async fn put(
    h: &common::Harness,
    token: &str,
    body: &str,
) -> axum::http::Response<axum::body::Body> {
    let bearer = format!("Bearer {token}");
    send_with(
        &h.router,
        "PUT",
        URI,
        &[
            ("authorization", &bearer),
            ("content-type", "application/json"),
        ],
        body.to_owned(),
    )
    .await
}

#[tokio::test]
async fn get_is_null_until_set() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = send_auth(&h.router, "GET", URI, &token).await;
    assert_eq!(res.status(), 200);
    assert_eq!(json(res).await, json!({"language": null}));
}

#[tokio::test]
async fn put_then_get() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = put(&h, &token, r#"{"language":"fr"}"#).await;
    assert_eq!(res.status(), 200);
    assert_eq!(json(res).await, json!({"language": "fr"}));
    let res = send_auth(&h.router, "GET", URI, &token).await;
    assert_eq!(json(res).await, json!({"language": "fr"}));
}

#[tokio::test]
async fn put_unknown_language_is_invalid_request() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    for body in [
        r#"{"language":"de"}"#,
        r#"{"language":"FR"}"#,
        r#"{"language":""}"#,
        "{}",
    ] {
        let res = put(&h, &token, body).await;
        assert_eq!(res.status(), 422, "{body}");
        let body: Value = json(res).await;
        assert_eq!(body["error"]["code"], "invalid_request");
    }
    let res = send_auth(&h.router, "GET", URI, &token).await;
    assert_eq!(json(res).await, json!({"language": null}));
}

#[tokio::test]
async fn routes_need_a_token() {
    let h = harness();
    let res = common::send(&h.router, "GET", URI).await;
    assert_eq!(res.status(), 401);
    assert_eq!(json(res).await["error"]["code"], "unauthorized");
    let res = send_with(
        &h.router,
        "PUT",
        URI,
        &[("content-type", "application/json")],
        r#"{"language":"fr"}"#,
    )
    .await;
    assert_eq!(res.status(), 401);
    assert_eq!(json(res).await["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn put_publishes_settings_changed() {
    let h = harness();
    let (_, writer) = pair(&h, "Phone").await;
    let (_, reader) = pair(&h, "Laptop").await;
    let res = send_auth(&h.router, "GET", "/api/v1/events", &reader).await;
    let mut body = res.into_body();
    let within = Duration::from_secs(2);
    assert_eq!(
        next_chunk(&mut body, within).await.unwrap(),
        "retry: 3000\n\n"
    );
    assert_eq!(put(&h, &writer, r#"{"language":"fr"}"#).await.status(), 200);
    let chunk = next_chunk(&mut body, within).await.unwrap();
    assert_eq!(chunk, "data: {\"type\":\"settings_changed\"}\n\n");
}
