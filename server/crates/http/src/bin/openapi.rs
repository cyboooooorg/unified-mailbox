//! Prints the OpenAPI document; `mise run openapi` stores it in
//! `api/openapi.json`.

fn main() {
    println!(
        "{}",
        http_api::openapi()
            .to_pretty_json()
            .expect("the OpenAPI document serializes")
    );
}
