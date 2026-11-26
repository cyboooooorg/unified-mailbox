mod common;

use axum::body::Body;
use common::{harness, json, new_code, pair, post_pairing, send_auth, send_with, text};
use serde_json::json;

fn is_code(code: &str) -> bool {
    const ALPHABET: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let bytes = code.as_bytes();
    bytes.len() == 9
        && bytes[4] == b'-'
        && code
            .chars()
            .enumerate()
            .all(|(i, c)| i == 4 || ALPHABET.contains(c))
}

#[tokio::test]
async fn pair_flow() {
    let h = harness();
    let code = new_code(&h).await.to_lowercase();
    let res = post_pairing(&h.router, &json!({"code": code, "name": " Phone "})).await;
    assert_eq!(res.status(), 201);
    let body = json(res).await;
    let token = body["token"].as_str().unwrap();
    assert!(token.starts_with("um_"), "{token}");
    let device_id = body["device_id"].as_i64().unwrap();

    let res = send_auth(&h.router, "GET", "/api/v1/devices", token).await;
    assert_eq!(res.status(), 200);
    let list = json(res).await;
    let devices = list.as_array().unwrap();
    assert_eq!(devices.len(), 1);
    let device = &devices[0];
    assert_eq!(device["id"], device_id);
    assert_eq!(device["name"], "Phone");
    assert_eq!(device["current"], true);
    assert_eq!(device["created_at"], "2026-01-01T00:00:00Z");
    assert!(device["created_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(device["last_seen_at"], "2026-01-01T00:00:00Z");
}

#[tokio::test]
async fn create_code_from_device() {
    let h = harness();
    let (first_id, first_token) = pair(&h, "Phone").await;

    let res = send_auth(
        &h.router,
        "POST",
        "/api/v1/devices/pairing-codes",
        &first_token,
    )
    .await;
    assert_eq!(res.status(), 201);
    let body = json(res).await;
    let code = body["code"].as_str().unwrap();
    assert!(is_code(code), "{code}");
    assert_eq!(body["expires_at"], "2026-01-01T00:10:00Z");

    let res = post_pairing(&h.router, &json!({"code": code, "name": "Tablet"})).await;
    assert_eq!(res.status(), 201);
    let second_id = json(res).await["device_id"].as_i64().unwrap();

    let list = json(send_auth(&h.router, "GET", "/api/v1/devices", &first_token).await).await;
    let current: Vec<(i64, bool)> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (d["id"].as_i64().unwrap(), d["current"].as_bool().unwrap()))
        .collect();
    assert_eq!(current, vec![(first_id, true), (second_id, false)]);
}

#[tokio::test]
async fn pair_invalid_code_400() {
    let h = harness();
    new_code(&h).await;
    let res = post_pairing(&h.router, &json!({"code": "0000-0000", "name": "Phone"})).await;
    assert_eq!(res.status(), 400);
    let body = json(res).await;
    assert_eq!(body["error"]["code"], "invalid_pairing_code");
    assert!(!body["error"]["message"].as_str().unwrap().contains("0000"));

    let code = new_code(&h).await;
    let first = post_pairing(&h.router, &json!({"code": code, "name": "Phone"})).await;
    assert_eq!(first.status(), 201);
    let res = post_pairing(&h.router, &json!({"code": code, "name": "Tablet"})).await;
    assert_eq!(res.status(), 400);
    let body = text(res).await;
    assert!(body.contains("invalid_pairing_code"), "{body}");
    assert!(!body.contains(&code), "{body}");
}

#[tokio::test]
async fn pair_invalid_name_422() {
    let h = harness();
    let code = new_code(&h).await;
    let res = post_pairing(&h.router, &json!({"code": code, "name": "a".repeat(65)})).await;
    assert_eq!(res.status(), 422);
    let body = json(res).await;
    assert_eq!(body["error"]["code"], "invalid_request");
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.contains("name"), "{message}");
    assert!(message.contains("64"), "{message}");

    // The code was not consumed by the failed attempt.
    let res = post_pairing(&h.router, &json!({"code": code, "name": "a".repeat(64)})).await;
    assert_eq!(res.status(), 201);
}

#[tokio::test]
async fn pair_invalid_name_messages() {
    let h = harness();
    for (name, expected) in [("   ", "name"), ("Pho\u{7}ne", "control")] {
        let code = new_code(&h).await;
        let res = post_pairing(&h.router, &json!({"code": code, "name": name})).await;
        assert_eq!(res.status(), 422, "{name:?}");
        let body = json(res).await;
        assert_eq!(body["error"]["code"], "invalid_request");
        let message = body["error"]["message"].as_str().unwrap();
        assert!(message.contains(expected), "{message}");
    }
}

#[tokio::test]
async fn pair_malformed_body() {
    let h = harness();
    let json_type = [("content-type", "application/json")];

    let res = send_with(
        &h.router,
        "POST",
        "/api/v1/pairing",
        &json_type,
        r#"{"code": 1}"#,
    )
    .await;
    assert_eq!(res.status(), 422);
    assert_eq!(json(res).await["error"]["code"], "invalid_request");

    let res = send_with(&h.router, "POST", "/api/v1/pairing", &json_type, "not json").await;
    assert_eq!(res.status(), 422);
    assert_eq!(json(res).await["error"]["code"], "invalid_request");

    let valid = json!({"code": "0000-0000", "name": "Phone"}).to_string();
    let res = send_with(&h.router, "POST", "/api/v1/pairing", &[], valid).await;
    assert_eq!(res.status(), 415);
    assert_eq!(json(res).await["error"]["code"], "unsupported_media_type");

    let huge = format!(
        r#"{{"code": "0000-0000", "name": "{}"}}"#,
        "a".repeat(2 * 1024 * 1024 + 1)
    );
    let res = send_with(&h.router, "POST", "/api/v1/pairing", &json_type, huge).await;
    assert_eq!(res.status(), 413);
    assert_eq!(json(res).await["error"]["code"], "payload_too_large");
}

#[tokio::test]
async fn revoke_other_and_self() {
    let h = harness();
    let (self_id, token) = pair(&h, "Phone").await;
    let (other_id, _) = pair(&h, "Tablet").await;

    let other = format!("/api/v1/devices/{other_id}");
    let res = send_auth(&h.router, "DELETE", &other, &token).await;
    assert_eq!(res.status(), 204);
    assert!(text(res).await.is_empty());

    let res = send_auth(&h.router, "DELETE", &other, &token).await;
    assert_eq!(res.status(), 404);
    assert_eq!(json(res).await["error"]["code"], "not_found");

    let list = json(send_auth(&h.router, "GET", "/api/v1/devices", &token).await).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let res = send_auth(
        &h.router,
        "DELETE",
        &format!("/api/v1/devices/{self_id}"),
        &token,
    )
    .await;
    assert_eq!(res.status(), 204);
    let res = send_auth(&h.router, "GET", "/api/v1/devices", &token).await;
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn revoke_bad_ids_404() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    for uri in [
        "/api/v1/devices/abc",
        "/api/v1/devices/99999999999999999999",
        "/api/v1/devices/12345",
    ] {
        let res = send_auth(&h.router, "DELETE", uri, &token).await;
        assert_eq!(res.status(), 404, "{uri}");
        assert_eq!(json(res).await["error"]["code"], "not_found", "{uri}");
    }
}

#[tokio::test]
async fn bad_id_without_token_is_401() {
    // Authentication comes before the path: an unauthenticated caller
    // learns nothing about which ids parse.
    let h = harness();
    let res = send_with(
        &h.router,
        "DELETE",
        "/api/v1/devices/abc",
        &[],
        Body::empty(),
    )
    .await;
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn token_never_in_list() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let res = send_auth(&h.router, "GET", "/api/v1/devices", &token).await;
    assert_eq!(res.status(), 200);
    let body = text(res).await;
    assert!(!body.contains(&token), "{body}");
    assert!(!body.contains(&token[3..]), "{body}");
}
