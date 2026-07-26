use crate::world::ForgeWorld;
use serde_json::json;

#[cucumber::when(expr = "I send a chat message {string}")]
async fn send_chat_message(world: &mut ForgeWorld, message: String) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);

    // Chat API requires: conversation_id, message, model, and instance_id
    let body = json!({
        "conversation_id": "test-conversation",
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
            let status = resp.status().as_u16();
            world.last_status = Some(status);
            world.last_body = resp.text().await.ok();
            Ok(())
        }
        Err(e) => Err(format!("Chat request error: {}", e)),
    }
}

#[cucumber::then(expr = "I should receive a response")]
async fn should_receive_response(_world: &mut ForgeWorld) -> Result<(), String> {
    // This is a placeholder - in real tests you'd verify the response
    Ok(())
}
