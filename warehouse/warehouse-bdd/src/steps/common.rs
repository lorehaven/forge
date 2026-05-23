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
    pub last_headers: reqwest::header::HeaderMap,
    pub last_response_headers: reqwest::header::HeaderMap,
    pub docker_digest: Option<String>,
    pub token: Option<String>,
    pub username: String,
    pub password: String,
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
            last_headers: reqwest::header::HeaderMap::new(),
            last_response_headers: reqwest::header::HeaderMap::new(),
            docker_digest: None,
            token: None,
            username,
            password,
        }
    }

    pub async fn record_response(&mut self, res: Response) {
        self.last_status = Some(res.status().as_u16());
        self.last_headers = res.headers().clone();
        self.last_response_headers = res.headers().clone();
        if res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.contains("application/json"))
            .unwrap_or(false)
        {
            self.last_json = res.json().await.ok();
        } else {
            self.last_json = None;
        }
    }
}

#[cucumber::given("warehouse API is available")]
async fn api_available(world: &mut WarehouseWorld) {
    let res = world
        .client
        .get(format!("{}/health", world.api_url))
        .send()
        .await;
    assert!(
        res.is_ok(),
        "API is not available at {}: {:?}",
        world.api_url,
        res.err()
    );
}

#[cucumber::then(expr = "response status should be {int}")]
async fn check_status(world: &mut WarehouseWorld, status: u16) {
    assert_eq!(world.last_status.expect("No response available"), status);
}

#[cucumber::when(expr = "GET request is sent to {string}")]
async fn send_get_request(world: &mut WarehouseWorld, path: String) {
    let url = if path.starts_with("/api/v1/crates") {
        format!("{}{}", world.api_url, path)
    } else {
        format!("{}{}", world.base_url, path)
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
