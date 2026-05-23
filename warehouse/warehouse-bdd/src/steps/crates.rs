use crate::steps::common::WarehouseWorld;
use base64::Engine;
use cucumber::{given, when};

#[given("valid token is obtained")]
async fn obtain_token(world: &mut WarehouseWorld) {
    let auth = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", world.username, world.password));
    world.token = Some(auth);
}

#[when(expr = "a crate {string} version {string} is published")]
async fn publish_crate(world: &mut WarehouseWorld, name: String, version: String) {
    let url = format!("{}/api/v1/crates/new", world.api_url);
    let mut rb = world.client.put(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }

    let metadata = serde_json::json!({
        "name": name,
        "vers": version,
        "deps": [],
        "features": {},
        "authors": ["test"],
        "description": "test crate"
    });

    let metadata_str = serde_json::to_string(&metadata).unwrap();
    let metadata_len = metadata_str.len() as u32;

    let crate_data = b"mock crate content";
    let crate_len = crate_data.len() as u32;

    let mut body = Vec::new();
    body.extend_from_slice(&metadata_len.to_le_bytes());
    body.extend_from_slice(metadata_str.as_bytes());
    body.extend_from_slice(&crate_len.to_le_bytes());
    body.extend_from_slice(crate_data);

    let res = rb
        .body(body)
        .send()
        .await
        .expect("Failed to send publish request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "DELETE request is sent to {string} with token for crates")]
async fn send_delete_request_with_token(world: &mut WarehouseWorld, path: String) {
    let url = if path.starts_with("/api/v1/crates") {
        format!("{}{}", world.api_url, path)
    } else {
        format!("{}{}", world.base_url, path)
    };

    let mut rb = world.client.delete(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }
    let res = rb.send().await.expect("Failed to send DELETE request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}
