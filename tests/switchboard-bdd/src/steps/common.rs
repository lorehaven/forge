use cucumber::World;
use reqwest::Response;
use serde_json::Value;
use std::env;

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct SwitchboardWorld {
    pub api_url: String,
    pub client: reqwest::Client,
    pub last_status: Option<u16>,
    pub last_json: Option<Value>,
    pub last_body: Option<String>,
    pub last_headers: reqwest::header::HeaderMap,
    pub last_response_headers: reqwest::header::HeaderMap,
    pub last_id: Option<String>,
}

impl SwitchboardWorld {
    pub async fn new() -> Self {
        let base_url =
            env::var("SWITCHBOARD_API_URL").unwrap_or_else(|_| "http://localhost:8554".to_string());
        let base_path = env::var("BASE_PATH").unwrap_or_else(|_| "".to_string());

        let api_url = format!("{}{}", base_url, base_path);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            api_url,
            client,
            last_status: None,
            last_json: None,
            last_body: None,
            last_headers: reqwest::header::HeaderMap::new(),
            last_response_headers: reqwest::header::HeaderMap::new(),
            last_id: None,
        }
    }

    pub async fn record_response(&mut self, res: Response) {
        self.last_status = Some(res.status().as_u16());
        self.last_headers = res.headers().clone();
        self.last_response_headers = res.headers().clone();

        let content_type = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");

        if content_type.contains("text/event-stream") {
            self.last_json = None;
            self.last_body = Some("SSE stream started".to_string());
            return;
        }

        if content_type.contains("application/json") {
            let json_text = res.text().await.ok().unwrap_or_default();
            let json: Option<Value> = serde_json::from_str(&json_text).ok();
            if let Some(json) = &json
                && let Some(id) = json.get("id").and_then(|v| v.as_str())
            {
                self.last_id = Some(id.to_string());
            }
            self.last_json = json;
            self.last_body = Some(json_text);
        } else {
            self.last_json = None;
            self.last_body = res.text().await.ok();
        }
    }
}

#[cucumber::given("switchboard API is available")]
async fn api_available(world: &mut SwitchboardWorld) {
    let url = format!("{}/ui/login", world.api_url);
    let mut success = false;
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(30);

    while start.elapsed() < timeout {
        let res = world.client.get(&url).send().await;
        if let Ok(res) = res
            && res.status().is_success()
        {
            success = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    assert!(
        success,
        "Switchboard API at {} did not become available within {:?}",
        url, timeout
    );
}

#[cucumber::then(expr = "response status should be {int}")]
async fn check_status(world: &mut SwitchboardWorld, status: u16) {
    assert_eq!(world.last_status.expect("No response available"), status);
}

#[cucumber::when(expr = "GET request is sent to {string}")]
async fn send_get_request(world: &mut SwitchboardWorld, path: String) {
    let url = format!("{}{}", world.api_url, path);
    let res = world
        .client
        .get(&url)
        .send()
        .await
        .expect("Failed to send GET request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[cucumber::then(expr = "response should contain {string}")]
async fn check_response_contains(world: &mut SwitchboardWorld, expected: String) {
    let body = world
        .last_body
        .as_ref()
        .expect("No response body available");
    assert!(
        body.contains(&expected),
        "Response body does not contain '{}'",
        expected
    );
}

#[cucumber::then(expr = "response should not contain {string}")]
async fn check_response_not_contains(world: &mut SwitchboardWorld, expected: String) {
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

#[cucumber::when(expr = "DELETE request is sent to {string}")]
async fn send_delete_request(world: &mut SwitchboardWorld, path: String) {
    let mut resolved_path = path;
    if let Some(id) = &world.last_id {
        resolved_path = resolved_path.replace("{last_id}", id);
    }
    let url = format!("{}{}", world.api_url, resolved_path);
    let res = world
        .client
        .delete(&url)
        .send()
        .await
        .expect("Failed to send DELETE request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}
