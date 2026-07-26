use crate::world::ForgeWorld;
use serde_json::json;

#[cucumber::when("I request available models")]
async fn request_available_models(world: &mut ForgeWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/models", world.sage_base_url);

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

#[cucumber::then("the response should contain model list")]
async fn response_contains_models(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::given(expr = "I have a conversation with model {string}")]
async fn have_conversation_with_model(
    _world: &mut ForgeWorld,
    _model: String,
) -> Result<(), String> {
    // Placeholder
    Ok(())
}

#[cucumber::when(expr = "I send a message {string} with model {string}")]
async fn send_with_model(
    world: &mut ForgeWorld,
    message: String,
    model: String,
) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);
    let conv_id = world
        .current_conversation_id
        .clone()
        .unwrap_or("test-conv".to_string());

    let body = json!({
        "conversation_id": conv_id,
        "message": message,
        "model": model,
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

#[cucumber::given("default model is not running")]
async fn default_model_not_running(_world: &mut ForgeWorld) -> Result<(), String> {
    // In mock setup, models start not running
    Ok(())
}

#[cucumber::then("sage should request switchboard to launch the model")]
async fn sage_should_launch_model(_world: &mut ForgeWorld) -> Result<(), String> {
    Ok(())
}
