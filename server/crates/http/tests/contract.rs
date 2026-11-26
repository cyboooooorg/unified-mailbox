use http_api::openapi;
use serde_json::{Value, json};

#[test]
fn contract_is_current() {
    let committed: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../api/openapi.json"
        ))
        .expect("api/openapi.json exists; run: mise run openapi"),
    )
    .expect("api/openapi.json is JSON");
    assert_eq!(
        committed,
        serde_json::to_value(openapi()).unwrap(),
        "api/openapi.json is stale; run: mise run openapi"
    );
}

#[test]
fn public_routes_declare_no_security() {
    let doc = serde_json::to_value(openapi()).unwrap();
    assert_eq!(doc["security"], json!([{"bearer": []}]));
    assert_eq!(
        doc["components"]["securitySchemes"]["bearer"]["scheme"],
        "bearer"
    );
    assert_eq!(
        doc["paths"]["/api/v1/health"]["get"]["security"],
        json!([{}])
    );
}
