use crate::steps::common::SageWorld;
use serde_json::json;

#[cucumber::when("I send a chat message without authentication")]
async fn send_without_auth(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    let body = json!({
        "conversation_id": "test-conv",
        "message": "Hello",
        "model": "test-model",
        "instance_id": "mock-1782724283792"
    });

    match world.client.post(&url).json(&body).send().await {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send a chat message with invalid token")]
async fn send_with_invalid_token(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    let body = json!({
        "conversation_id": "test-conv",
        "message": "Hello",
        "model": "test-model",
        "instance_id": "mock-1782724283792"
    });

    match world
        .client
        .post(&url)
        .header("Authorization", "Bearer invalid-token")
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send a chat message with expired token")]
async fn send_with_expired_token(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    let body = json!({
        "conversation_id": "test-conv",
        "message": "Hello",
        "model": "test-model",
        "instance_id": "mock-1782724283792"
    });

    // Token with past expiration
    let expired_token =
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0IiwiZXhwIjowfQ.invalid";

    match world
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", expired_token))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send a chat message with valid token")]
async fn send_with_valid_token(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    let body = json!({
        "conversation_id": "test-conv",
        "message": "Hello",
        "model": "test-model",
        "instance_id": "mock-1782724283792"
    });

    match world
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", world.jwt_token))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status().as_u16();
            world.last_status = Some(status);
            let body_text = resp.text().await.unwrap_or_default();
            if status >= 400 {
                eprintln!("DEBUG TEST: status={}, body={}", status, body_text);
            }
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send a chat message with token missing scope")]
async fn send_with_missing_scope(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    // Token without scope claim
    let token_without_scope =
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0IiwiZXhwIjoxMDAwMDAwMDAwMH0.invalid";

    let body = serde_json::json!({
        "conversation_id": "test-conv",
        "message": "Hello",
        "model": "test-model",
        "instance_id": "mock-1782724283792"
    });

    match world
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token_without_scope))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when(expr = "I send a chat message with token for service {string}")]
async fn send_with_wrong_service(world: &mut SageWorld, _service: String) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    let body = serde_json::json!({
        "conversation_id": "test-conv",
        "message": "Hello",
        "model": "test-model",
        "instance_id": "mock-1782724283792"
    });

    // Token for wrong service
    let wrong_service_token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0Iiwic2VydmljZSI6Im90aGVyIiwic2NvcGUiOiJ1c2VyIiwiZXhwIjoxMDAwMDAwMDAwMH0.invalid";

    match world
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", wrong_service_token))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}
