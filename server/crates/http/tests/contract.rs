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
    assert_eq!(
        doc["paths"]["/api/v1/pairing"]["post"]["security"],
        json!([{}])
    );
}

#[test]
fn error_codes_are_an_enum_in_the_contract() {
    let doc = serde_json::to_value(openapi()).unwrap();
    assert_eq!(
        doc["components"]["schemas"]["ErrorCode"]["enum"],
        json!([
            "invalid_pairing_code",
            "unauthorized",
            "not_found",
            "method_not_allowed",
            "payload_too_large",
            "unsupported_media_type",
            "invalid_request",
            "internal"
        ])
    );
}

#[test]
fn contract_is_user_facing() {
    let doc = serde_json::to_value(openapi()).unwrap();
    let mut ids = Vec::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            ids.push(format!("{method} {path} {}", op["operationId"]));
        }
    }
    ids.sort();
    assert_eq!(
        ids,
        [
            r#"delete /api/v1/devices/{id} "revoke_device""#,
            r#"get /api/v1/devices "list_devices""#,
            r#"get /api/v1/events "stream_events""#,
            r#"get /api/v1/health "get_health""#,
            r#"get /api/v1/jobs "list_jobs""#,
            r#"get /api/v1/jobs/{id} "get_job""#,
            r#"get /api/v1/settings/language "get_language""#,
            r#"post /api/v1/devices/pairing-codes "create_pairing_code""#,
            r#"post /api/v1/pairing "pair_device""#,
            r#"put /api/v1/settings/language "set_language""#,
        ]
    );
    assert!(!doc.to_string().contains("Debug"));
    let required = doc["components"]["schemas"]["DeviceDto"]["required"]
        .as_array()
        .unwrap();
    assert!(required.iter().any(|f| f == "last_seen_at"));
}

#[test]
fn event_schema_is_a_tagged_one_of() {
    let doc = serde_json::to_value(openapi()).unwrap();
    assert_eq!(
        doc["components"]["schemas"]["EventDto"]["discriminator"],
        serde_json::json!({"propertyName": "type"})
    );
    let variants = doc["components"]["schemas"]["EventDto"]["oneOf"]
        .as_array()
        .expect("EventDto is a oneOf");
    let mut types: Vec<&str> = variants
        .iter()
        .map(|v| {
            assert!(
                v["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f == "type"),
                "{v}"
            );
            v["properties"]["type"]["enum"][0].as_str().unwrap()
        })
        .collect();
    types.sort();
    assert_eq!(
        types,
        ["device_revoked", "job", "resync", "settings_changed"]
    );
    let events = &doc["paths"]["/api/v1/events"]["get"]["responses"]["200"]["content"];
    assert!(events.get("text/event-stream").is_some(), "{events}");
}

#[test]
fn nullable_language_keeps_its_description_on_the_property() {
    let doc = serde_json::to_value(openapi()).unwrap();
    let language = &doc["components"]["schemas"]["LanguageSettingDto"]["properties"]["language"];
    let description = language["description"]
        .as_str()
        .unwrap_or_else(|| panic!("no description on the property: {language}"));
    assert!(
        description.contains("`null` until the user chooses one"),
        "{description}"
    );
    let branches = language["oneOf"].as_array().expect("language is a oneOf");
    assert!(branches.iter().any(|b| b["type"] == "null"), "{language}");
    assert!(
        branches.iter().all(|b| b
            .get("description")
            .is_none_or(|d| !d.as_str().unwrap().contains("`null`"))),
        "the property's description is on a oneOf branch: {language}"
    );
}
