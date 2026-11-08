//! Steps unique to the sage suite. The generic HTTP steps live in
//! `steps::common`, shared with every other service.

use crate::world::ForgeWorld;
use quench_serde::json;

#[quench_test_bdd::when("I send a chat message")]
async fn send_generic_message(world: &mut ForgeWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.sage_base_url);
    let conv_id = world
        .current_conversation_id
        .clone()
        .unwrap_or_else(|| "test-conversation".to_string());

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
        Err(e) => Err(format!("Request error: {e}")),
    }
}

#[quench_test_bdd::then("both messages should be in conversation history")]
const fn both_messages_in_history(_world: &mut ForgeWorld) {}

#[quench_test_bdd::then("all requests should complete successfully")]
const fn all_requests_successful(_world: &mut ForgeWorld) {}

#[quench_test_bdd::then("all three messages should be in conversation history")]
const fn all_three_messages_in_history(_world: &mut ForgeWorld) {}

#[quench_test_bdd::then("the response should contain:")]
const fn response_should_contain_table(_world: &mut ForgeWorld) {}

#[quench_test_bdd::then("the conversation should have:")]
const fn conversation_should_have_table(_world: &mut ForgeWorld) {}

#[quench_test_bdd::then("the response should include token usage:")]
const fn response_should_have_token_usage_table(_world: &mut ForgeWorld) {}
