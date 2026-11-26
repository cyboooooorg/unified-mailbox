mod common;

use axum::body::Body;
use common::{harness, json, pair, send, send_with};
use domain::DeviceId;
use http_api::openapi;

const PUBLIC: [&str; 2] = ["/api/v1/health", "/api/v1/pairing"];

/// Fills every `{param}` of an OpenAPI path with `1`.
fn fill_params(path: &str) -> String {
    let mut out = String::new();
    let mut in_param = false;
    for c in path.chars() {
        match c {
            '{' => {
                in_param = true;
                out.push('1');
            }
            '}' => in_param = false,
            _ if in_param => {}
            _ => out.push(c),
        }
    }
    out
}

#[tokio::test]
async fn every_protected_route_requires_a_token() {
    let h = harness();
    let doc = serde_json::to_value(openapi()).unwrap();
    let paths = doc["paths"].as_object().unwrap();
    let mut checked = 0;
    for (path, item) in paths {
        if PUBLIC.contains(&path.as_str()) {
            continue;
        }
        for method in item.as_object().unwrap().keys() {
            let method = method.to_uppercase();
            let uri = fill_params(path);
            let res = send(&h.router, &method, &uri).await;
            assert_eq!(res.status(), 401, "{method} {uri}");
            assert_eq!(
                res.headers()["www-authenticate"],
                "Bearer",
                "{method} {uri}"
            );
            let body = json(res).await;
            assert_eq!(body["error"]["code"], "unauthorized", "{method} {uri}");
            checked += 1;
        }
    }
    assert!(checked >= 3, "only {checked} protected operations found");
}

async fn status_with(h: &common::Harness, authorization: &str) -> u16 {
    send_with(
        &h.router,
        "GET",
        "/api/v1/devices",
        &[("authorization", authorization)],
        Body::empty(),
    )
    .await
    .status()
    .as_u16()
}

#[tokio::test]
async fn malformed_authorization_headers() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    let truncated = &token[..token.len() - 1];
    for header in [
        "Basic abc".to_owned(),
        "Bearer".to_owned(),
        "Bearer ".to_owned(),
        format!("Token {token}"),
        token.clone(),
        format!("Bearer {truncated}"),
        format!("Bearer{token}"),
        format!("Bearer\t{token}"),
    ] {
        let res = send_with(
            &h.router,
            "GET",
            "/api/v1/devices",
            &[("authorization", &header)],
            Body::empty(),
        )
        .await;
        assert_eq!(res.status(), 401, "{header:?}");
        assert_eq!(res.headers()["www-authenticate"], "Bearer");
        let body = json(res).await;
        assert_eq!(body["error"]["code"], "unauthorized");
        assert_eq!(body["error"]["message"], "missing or invalid bearer token");
    }
    // The well-formed header works, so the 401s above come from the header.
    assert_eq!(status_with(&h, &format!("Bearer {token}")).await, 200);
}

#[tokio::test]
async fn lowercase_scheme_and_extra_spaces_accepted() {
    let h = harness();
    let (_, token) = pair(&h, "Phone").await;
    for header in [
        format!("bearer {token}"),
        format!("BEARER {token}"),
        format!("Bearer   {token}"),
    ] {
        assert_eq!(status_with(&h, &header).await, 200, "{header:?}");
    }
}

#[tokio::test]
async fn revoked_token_401() {
    let h = harness();
    let (id, token) = pair(&h, "Phone").await;
    assert_eq!(status_with(&h, &format!("Bearer {token}")).await, 200);
    assert!(
        h.state
            .revoke_device
            .execute(DeviceId::new(id))
            .await
            .unwrap()
    );
    assert_eq!(status_with(&h, &format!("Bearer {token}")).await, 401);
}
