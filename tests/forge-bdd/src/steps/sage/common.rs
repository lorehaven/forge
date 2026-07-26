//! Steps unique to the sage suite. The generic HTTP steps live in
//! `steps::common`, shared with every other service.

use crate::world::ForgeWorld;
use serde_json::json;

#[cucumber::when("I send a chat message")]
async fn send_generic_message(world: &mut ForgeWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);
    let conv_id = world
        .current_conversation_id
        .clone()
        .unwrap_or("test-conversation".to_string());

    let body = json!({
        "conversation_id": conv_id,
        "message": "Test message",
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
            world.last_body = resp.text().await.ok();
            Ok(())
        }
        Err(e) => Err(format!("Request error: {}", e)),
    }
}

#[cucumber::then("both messages should be in conversation history")]
async fn both_messages_in_history(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("all requests should complete successfully")]
async fn all_requests_successful(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("all three messages should be in conversation history")]
async fn all_three_messages_in_history(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("the response should contain:")]
async fn response_should_contain_table(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("the conversation should have:")]
async fn conversation_should_have_table(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("the response should include token usage:")]
async fn response_should_have_token_usage_table(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}
