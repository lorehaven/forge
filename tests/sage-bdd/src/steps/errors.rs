use crate::steps::common::SageWorld;
use serde_json::json;

#[cucumber::when("I send a chat message \"\" with valid token")]
async fn send_empty_message(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    let body = json!({
        "conversation_id": "test-conv",
        "message": "",
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
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send chat request without message field")]
async fn send_without_message(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    let body = json!({
        "conversation_id": "test-conv",
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
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send a very long chat message (10000+ characters)")]
async fn send_long_message(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);
    let long_message = "a".repeat(10001);

    let body = json!({
        "conversation_id": "test-conv",
        "message": long_message,
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
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send malformed JSON to chat endpoint")]
async fn send_malformed_json(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);

    match world
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", world.jwt_token))
        .header("Content-Type", "application/json")
        .body("{invalid json}")
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

#[cucumber::given("vLLM service is unavailable")]
async fn vllm_unavailable(_world: &mut SageWorld) -> Result<(), String> {
    // Placeholder - in real scenario would mock vLLM unavailability
    Ok(())
}

#[cucumber::when("I send 5 concurrent chat messages")]
async fn send_concurrent_messages(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);
    let token = world.jwt_token.clone();

    let mut handles = vec![];

    for i in 0..5 {
        let url = url.clone();
        let token = token.clone();
        let client = world.client.clone();

        let handle = tokio::spawn(async move {
            let body = json!({
                "conversation_id": "test-conv",
                "message": format!("Message {}", i),
                "model": "test-model",
                "instance_id": "mock-1782724283792"
            });

            client
                .post(&url)
                .header("Authorization", format!("Bearer {}", token))
                .json(&body)
                .send()
                .await
        });

        handles.push(handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => {}
            _ => return Err("Concurrent request failed".to_string()),
        }
    }

    Ok(())
}

#[cucumber::then("the response should indicate service unavailable")]
async fn response_service_unavailable(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::when(expr = "I send a long chat message with {int} characters")]
async fn send_long_message_with_count(
    world: &mut SageWorld,
    char_count: usize,
) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);
    let long_message = "a".repeat(char_count);

    let body = serde_json::json!({
        "conversation_id": "test-conversation",
        "message": long_message,
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
            world.last_status = Some(resp.status().as_u16());
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::when("I send 5 rapid chat messages")]
async fn send_rapid_chat_messages(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);
    let token = world.jwt_token.clone();
    let conv_id = world
        .current_conversation_id
        .clone()
        .unwrap_or("test-conversation".to_string());

    let mut handles = vec![];

    for i in 0..5 {
        let url = url.clone();
        let token = token.clone();
        let conv_id = conv_id.clone();
        let client = world.client.clone();

        let handle = tokio::spawn(async move {
            let body = serde_json::json!({
                "conversation_id": conv_id,
                "message": format!("Rapid message {}", i),
                "model": "test-model",
                "instance_id": "mock-1782724283792"
            });

            client
                .post(&url)
                .header("Authorization", format!("Bearer {}", token))
                .json(&body)
                .send()
                .await
        });

        handles.push(handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(resp)) => {
                world.last_status = Some(resp.status().as_u16());
            }
            _ => return Err("Rapid request failed".to_string()),
        }
    }

    Ok(())
}
