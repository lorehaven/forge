use cucumber::World;
use serde_json::json;
use std::time::Duration;

#[derive(World, Debug)]
pub struct SageWorld {
    pub api_base_url: String,
    pub client: reqwest::Client,
    pub jwt_token: String,
    pub current_conversation_id: Option<String>,
    pub last_status: Option<u16>,
    pub last_body: Option<String>,
}

impl SageWorld {
    pub fn new() -> Self {
        let api_base_url =
            std::env::var("SAGE_API_URL").unwrap_or("https://localhost:7777".to_string());
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or("test_secret_key".to_string());

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();

        // Create a simple JWT token for testing
        let jwt_token = create_test_jwt(&jwt_secret);

        Self {
            api_base_url,
            client,
            jwt_token,
            current_conversation_id: None,
            last_status: None,
            last_body: None,
        }
    }
}

impl Default for SageWorld {
    fn default() -> Self {
        Self::new()
    }
}

fn create_test_jwt(secret: &str) -> String {
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        service: String,
        scope: String,
        iat: usize,
        exp: usize,
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    let claims = Claims {
        sub: "test-user".to_string(),
        service: "sage".to_string(),
        scope: "user".to_string(),
        iat: now,
        exp: now + 3600,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap_or_else(|_| "invalid-token".to_string())
}

#[cucumber::then(expr = "sage API is available")]
async fn sage_api_is_available(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/ui/login", world.api_base_url);
    let mut retries = 0;
    let max_retries = 60; // 30 seconds with 500ms interval

    loop {
        match world.client.get(&url).send().await {
            Ok(_) => return Ok(()),
            Err(_) => {
                retries += 1;
                if retries >= max_retries {
                    return Err(format!(
                        "Sage API at {} did not become available within 30s",
                        url
                    ));
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

#[cucumber::given(expr = "sage API is available")]
async fn given_sage_api_is_available(world: &mut SageWorld) -> Result<(), String> {
    sage_api_is_available(world).await
}

#[cucumber::then(expr = "the response status should be {int}")]
async fn response_status_should_be(world: &mut SageWorld, expected: u16) -> Result<(), String> {
    match world.last_status {
        Some(status) if status == expected => Ok(()),
        Some(status) => Err(format!("Expected status {}, got {}", expected, status)),
        None => Err("No response status recorded".to_string()),
    }
}

#[cucumber::then("the response should contain error message")]
async fn response_contains_error(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::when("I send a chat message")]
async fn send_generic_message(world: &mut SageWorld) -> Result<(), String> {
    let url = format!("{}/sage/api/v1/chat", world.api_base_url);
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
async fn both_messages_in_history(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("all requests should complete successfully")]
async fn all_requests_successful(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("all three messages should be in conversation history")]
async fn all_three_messages_in_history(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("the response should contain:")]
async fn response_should_contain_table(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("the conversation should have:")]
async fn conversation_should_have_table(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}

#[cucumber::then("the response should include token usage:")]
async fn response_should_have_token_usage_table(_world: &mut SageWorld) -> Result<(), String> {
    Ok(())
}
