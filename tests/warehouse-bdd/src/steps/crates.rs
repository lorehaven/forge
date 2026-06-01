use crate::steps::common::WarehouseWorld;
use base64::Engine;
use chrono::Utc;
use cucumber::{given, then, when};

#[given("valid token is obtained")]
async fn obtain_token(world: &mut WarehouseWorld) {
    let auth = base64::engine::general_purpose::STANDARD
        .encode(format!("{}:{}", world.username, world.password));
    world.token = Some(auth);
}

#[given(expr = "a crate {string} version {string} is published")]
#[when(expr = "a crate {string} version {string} is published")]
async fn publish_crate(world: &mut WarehouseWorld, name: String, version: String) {
    let unique_name = if name.contains("test-crate")
        || name.contains("test-owners")
        || name.contains("test-index")
    {
        let suffix = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let n = format!("{}-{}", name, suffix);
        // crate names must be <= 64 chars
        if n.len() > 64 {
            format!("{}-{}", &name[..name.len().min(50)], suffix)
        } else {
            n
        }
    } else {
        name.clone()
    };

    world.current_crate_name = Some(unique_name.clone());
    world.current_crate_version = Some(version.clone());

    let url = format!("{}/api/v1/crates/new", world.api_url);
    let mut rb = world.client.put(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }

    let metadata = serde_json::json!({
        "name": unique_name,
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

fn resolve_crate_path(world: &WarehouseWorld, path: String) -> String {
    let mut resolved_path = path;
    if let Some(name) = &world.current_crate_name {
        resolved_path = resolved_path.replace("test-crate-random-unique-xyz-789", name);
        resolved_path = resolved_path.replace("test-owners-crate", name);
        resolved_path = resolved_path.replace("test-index-crate", name);
        resolved_path = resolved_path.replace("test-crate-index-123", name);
    }

    if resolved_path.starts_with("/api/v1/crates") || resolved_path.starts_with("/index") {
        format!("{}{}", world.api_url, resolved_path)
    } else {
        format!("{}{}", world.base_url, resolved_path)
    }
}

#[when(expr = "DELETE request is sent to {string} with token for crates")]
async fn send_delete_request_with_token(world: &mut WarehouseWorld, path: String) {
    let url = resolve_crate_path(world, path);

    let mut rb = world.client.delete(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }
    let res = rb.send().await.expect("Failed to send DELETE request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "PUT request is sent to {string} with token for crates")]
async fn send_put_request_with_token(world: &mut WarehouseWorld, path: String) {
    let url = resolve_crate_path(world, path);

    let mut rb = world.client.put(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }
    let res = rb.send().await.expect("Failed to send PUT request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "PUT request is sent to {string} with token and body:")]
async fn send_put_request_with_token_and_body(
    world: &mut WarehouseWorld,
    path: String,
    step: &cucumber::gherkin::Step,
) {
    let body = step.docstring().expect("Step must have a docstring");
    let url = resolve_crate_path(world, path);

    let mut rb = world.client.put(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }
    let res = rb
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("Failed to send PUT request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "DELETE request is sent to {string} with token and body:")]
async fn send_delete_request_with_token_and_body(
    world: &mut WarehouseWorld,
    path: String,
    step: &cucumber::gherkin::Step,
) {
    let body = step.docstring().expect("Step must have a docstring");
    let url = resolve_crate_path(world, path);

    let mut rb = world.client.delete(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }
    let res = rb
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("Failed to send DELETE request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[when(expr = "a crate {string} version {string} is published without token")]
async fn publish_crate_no_token(world: &mut WarehouseWorld, name: String, version: String) {
    let url = format!("{}/api/v1/crates/new", world.api_url);
    let rb = world.client.put(&url); // No token

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

#[when("a crate is published with invalid metadata:")]
async fn publish_crate_invalid_metadata(
    world: &mut WarehouseWorld,
    step: &cucumber::gherkin::Step,
) {
    let metadata_str = step.docstring().expect("Step must have a docstring");
    let url = format!("{}/api/v1/crates/new", world.api_url);
    let mut rb = world.client.put(&url);
    if let Some(token) = &world.token {
        rb = rb.header("Authorization", token);
    }

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

#[then("response should be a JSON object")]
async fn check_json_object(world: &mut WarehouseWorld) {
    let json = world
        .last_json
        .as_ref()
        .expect("No JSON response available");
    assert!(
        json.is_object(),
        "Response is not a JSON object: {:?}",
        json
    );
}

#[then(expr = "response should not contain {string}")]
async fn check_response_not_contains(world: &mut WarehouseWorld, expected: String) {
    let body = world
        .last_body
        .as_ref()
        .expect("No response body available");
    assert!(
        !body.contains(&expected),
        "Response body contains '{}' but it shouldn't",
        expected
    );
}
