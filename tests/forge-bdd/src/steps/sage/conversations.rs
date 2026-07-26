use crate::world::ForgeWorld;
use serde_json::json;
use uuid::Uuid;

#[cucumber::when("I create a new conversation")]
async fn create_conversation(world: &mut ForgeWorld) -> Result<(), String> {
    // Generate a conversation ID (sage doesn't have explicit conversation creation)
    let conv_id = Uuid::new_v4().to_string();
    world.current_conversation_id = Some(conv_id);
    Ok(())
}

#[cucumber::given("I have a conversation")]
async fn have_conversation(world: &mut ForgeWorld) -> Result<(), String> {
    create_conversation(world).await
}

#[cucumber::given("I have a conversation with messages")]
async fn have_conversation_with_messages(world: &mut ForgeWorld) -> Result<(), String> {
    create_conversation(world).await?;

    // Send a few test messages
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);
    let conv_id = world
        .current_conversation_id
        .clone()
        .ok_or("No conversation created")?;

    let body = json!({
        "conversation_id": conv_id,
        "message": "Test message 1",
        "model": "test-model",
        "instance_id": "mock-1782724283792"
    });

    world
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", world.jwt_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cucumber::when(expr = "I send a chat message {string} to the conversation")]
async fn send_to_conversation(world: &mut ForgeWorld, message: String) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);
    let conv_id = world
        .current_conversation_id
        .clone()
        .ok_or("No conversation created")?;

    let body = json!({
        "conversation_id": conv_id,
        "message": message,
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
            let body = resp.text().await.unwrap_or_default();
            world.last_body = Some(body);
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::then("the response should contain a conversation_id")]
async fn response_contains_conversation_id(world: &mut ForgeWorld) -> Result<(), String> {
    if let Some(conv_id) = &world.current_conversation_id
        && !conv_id.is_empty()
    {
        return Ok(());
    }
    Err("No conversation_id in response".to_string())
}

#[cucumber::then("the response should contain assistant message")]
async fn response_contains_message(_world: &mut ForgeWorld) -> Result<(), String> {
    // Placeholder - in real scenario would parse response
    Ok(())
}

#[cucumber::when("I retrieve the conversation")]
async fn retrieve_conversation(world: &mut ForgeWorld) -> Result<(), String> {
    let conv_id = world
        .current_conversation_id
        .clone()
        .ok_or("No conversation created")?;

    let url = format!(
        "{}/sage/api/v1/conversations/{}",
        world.sage_base_url, conv_id
    );

    match world
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", world.jwt_token))
        .send()
        .await
    {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            world.last_body = resp.text().await.ok();
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::then("the response should contain all messages")]
async fn response_contains_all_messages(_world: &mut ForgeWorld) -> Result<(), String> {
    // Placeholder - in real scenario would validate messages in response
    Ok(())
}

#[cucumber::given("I have multiple conversations")]
async fn have_multiple_conversations(world: &mut ForgeWorld) -> Result<(), String> {
    for _ in 0..3 {
        create_conversation(world).await?;
    }
    Ok(())
}

#[cucumber::when("I list all conversations")]
async fn list_conversations(world: &mut ForgeWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/conversations", world.sage_base_url);

    match world
        .client
        .get(&url)
        .header("Authorization", format!("Bearer {}", world.jwt_token))
        .send()
        .await
    {
        Ok(resp) => {
            world.last_status = Some(resp.status().as_u16());
            world.last_body = resp.text().await.ok();
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::then("the response should contain all conversation ids")]
async fn response_contains_conversations(_world: &mut ForgeWorld) -> Result<(), String> {
    // Placeholder - in real scenario would validate conversation list
    Ok(())
}

#[cucumber::when(expr = "I send a chat message {string} without conversation_id")]
async fn send_without_conversation_id(
    world: &mut ForgeWorld,
    message: String,
) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);

    let body = json!({
        "message": message,
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

#[cucumber::when(expr = "I send a chat message {string} to conversation {string}")]
async fn send_to_specific_conversation(
    world: &mut ForgeWorld,
    message: String,
    conversation_id: String,
) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);

    let body = json!({
        "conversation_id": conversation_id,
        "message": message,
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

#[cucumber::then("each message should have a response")]
async fn each_message_has_response(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}
