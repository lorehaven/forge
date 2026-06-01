use cucumber::World;
use reqwest::Response;
use serde_json::Value;
use std::env;

#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct WarehouseWorld {
    pub base_url: String,
    pub api_url: String,
    pub client: reqwest::Client,
    pub last_status: Option<u16>,
    pub last_json: Option<Value>,
    pub last_body: Option<String>,
    pub last_headers: reqwest::header::HeaderMap,
    pub last_response_headers: reqwest::header::HeaderMap,
    pub docker_digest: Option<String>,
    pub token: Option<String>,
    pub username: String,
    pub password: String,
    pub current_crate_name: Option<String>,
    pub current_crate_version: Option<String>,
}

impl WarehouseWorld {
    pub async fn new() -> Self {
        let base_url =
            env::var("WAREHOUSE_API_URL").unwrap_or_else(|_| "http://localhost:8443".to_string());
        let base_path = env::var("BASE_PATH").unwrap_or_else(|_| "/warehouse".to_string());
        let username = env::var("SERVICE_USERNAME").unwrap_or_else(|_| "admin".to_string());
        let password = env::var("SERVICE_PASSWORD").unwrap_or_else(|_| "password".to_string());

        let api_url = format!("{}{}", base_url, base_path);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            api_url,
            client,
            last_status: None,
            last_json: None,
            last_body: None,
            last_headers: reqwest::header::HeaderMap::new(),
            last_response_headers: reqwest::header::HeaderMap::new(),
            docker_digest: None,
            token: None,
            username,
            password,
            current_crate_name: None,
            current_crate_version: None,
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
            self.last_json = serde_json::from_str(&json_text).ok();
            self.last_body = Some(json_text);
        } else {
            self.last_json = None;
            self.last_body = res.text().await.ok();
        }
    }
}

#[cucumber::given("warehouse API is available")]
async fn api_available(world: &mut WarehouseWorld) {
    let url = format!("{}/health", world.api_url);
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
        "Warehouse API at {} did not become available within {:?}",
        url, timeout
    );
}

#[cucumber::then(expr = "response status should be {int}")]
async fn check_status(world: &mut WarehouseWorld, status: u16) {
    assert_eq!(world.last_status.expect("No response available"), status);
}

#[cucumber::when(expr = "GET request is sent to {string}")]
async fn send_get_request(world: &mut WarehouseWorld, path: String) {
    let mut resolved_path = path;
    if let Some(name) = &world.current_crate_name {
        resolved_path = resolved_path.replace("test-crate-random-unique-xyz-789", name);
        resolved_path = resolved_path.replace("test-owners-crate", name);
        resolved_path = resolved_path.replace("test-index-crate", name);
        resolved_path = resolved_path.replace("test-crate-index-123", name);
    }

    let url = if resolved_path.starts_with("/api/v1/crates") || resolved_path.starts_with("/index")
    {
        format!("{}{}", world.api_url, resolved_path)
    } else {
        format!("{}{}", world.base_url, resolved_path)
    };
    let res = world
        .client
        .get(&url)
        .send()
        .await
        .expect("Failed to send GET request");
    world.last_response_headers = res.headers().clone();
    world.record_response(res).await;
}

#[cucumber::then(expr = "response content type should be {string}")]
async fn check_content_type(world: &mut WarehouseWorld, expected: String) {
    let actual = world
        .last_response_headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .expect("No Content-Type header");
    assert!(
        actual.contains(&expected),
        "Content-Type '{}' does not contain '{}'",
        actual,
        expected
    );
}

#[cucumber::then(expr = "response should contain {string}")]
async fn check_response_contains(world: &mut WarehouseWorld, expected: String) {
    let mut resolved_expected = expected;
    if let Some(name) = &world.current_crate_name {
        resolved_expected = resolved_expected.replace("test-crate-random-unique-xyz-789", name);
        resolved_expected = resolved_expected.replace("test-owners-crate", name);
        resolved_expected = resolved_expected.replace("test-index-crate", name);
        resolved_expected = resolved_expected.replace("test-crate-index-123", name);
    }
    let body = world
        .last_body
        .as_ref()
        .expect("No response body available");
    assert!(
        body.contains(&resolved_expected),
        "Response body does not contain '{}'",
        resolved_expected
    );
}
