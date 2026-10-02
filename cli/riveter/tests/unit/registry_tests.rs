use riveter::registry::Registry;
use std::sync::{Mutex, OnceLock};
use wiremock::matchers::{body_bytes, body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `Registry::from_env` reads process-global variables.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

const VARS: [&str; 5] = [
    "RIVETER_WAREHOUSE_URL",
    "RIVETER_WAREHOUSE_TOKEN",
    "RIVETER_GATEHOUSE_URL",
    "RIVETER_CLIENT_ID",
    "RIVETER_CLIENT_SECRET",
];

fn clear_env() {
    for var in VARS {
        // SAFETY-free: `envmnt` wraps the call; the lock above serialises these tests.
        envmnt::remove(var);
    }
}

fn record(name: &str, version: &str, bytes: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "version": version,
        "description": "demo",
        "namespace": "demo",
        "filename": format!("{name}-{version}.rivet"),
        "size_bytes": bytes.len(),
        "sha256": rivet_package::sha256_hex(bytes),
        "manifest": {},
        "uploaded_by": "ci",
        "yanked": false,
        "created_at": "2026-10-02T00:00:00Z"
    })
}

/// Runs a blocking registry call off the async runtime, which `reqwest::blocking` insists on.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publishes_the_bytes_with_the_bearer_token() {
    let server = MockServer::start().await;
    let bytes = b"archive-bytes".to_vec();

    Mock::given(method("PUT"))
        .and(path("/warehouse/api/v1/rivets/forge/0.4.0+b1"))
        .and(header("authorization", "Bearer tok"))
        .and(body_bytes(bytes.clone()))
        .respond_with(ResponseTemplate::new(201).set_body_json(record("forge", "0.4.0+b1", &bytes)))
        .expect(1)
        .mount(&server)
        .await;

    let base = format!("{}/warehouse/", server.uri());
    let published = blocking(move || {
        Registry::new(&base, Some("tok".into()))
            .unwrap()
            .publish("forge", "0.4.0+b1", bytes)
            .unwrap()
    })
    .await;

    assert_eq!(published.name, "forge");
    assert_eq!(published.version, "0.4.0+b1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explains_each_way_a_publish_can_be_refused() {
    for (status, expected) in [
        (401, "rejected the credentials"),
        (403, "warehouse:write"),
        (
            409,
            "already published: this package version has already been published",
        ),
        (422, "HTTP 422: the archive declares"),
        (500, "HTTP 500"),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(status).set_body_json(
                serde_json::json!({"error": match status {
                    409 => "this package version has already been published",
                    422 => "the archive declares `x`",
                    _ => "",
                }}),
            ))
            .mount(&server)
            .await;

        let base = server.uri();
        let err = blocking(move || {
            Registry::new(&base, None)
                .unwrap()
                .publish("forge", "1.0.0", vec![1])
                .unwrap_err()
        })
        .await;
        assert!(format!("{err:#}").contains(expected), "{status}: {err:#}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_resolves_latest_then_downloads_that_exact_version() {
    let server = MockServer::start().await;
    let bytes = b"the-archive".to_vec();

    Mock::given(method("GET"))
        .and(path("/api/v1/rivets/forge/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(record("forge", "0.10.0", &bytes)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/rivets/forge/0.10.0/download"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
        .expect(1)
        .mount(&server)
        .await;

    let base = server.uri();
    let fetched = blocking(move || {
        Registry::new(&base, Some("tok".into()))
            .unwrap()
            .fetch("forge", "latest")
            .unwrap()
    })
    .await;

    assert_eq!(fetched.version, "0.10.0");
    assert_eq!(fetched.bytes, bytes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetch_refuses_bytes_that_do_not_match_the_recorded_digest() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/rivets/forge/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(record("forge", "1.0.0", b"genuine")),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/rivets/forge/1.0.0/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"tampered".to_vec()))
        .mount(&server)
        .await;

    let base = server.uri();
    let err = blocking(move || {
        Registry::new(&base, None)
            .unwrap()
            .fetch("forge", "1.0.0")
            .unwrap_err()
    })
    .await;

    assert!(
        format!("{err:#}").contains("but Warehouse records"),
        "{err:#}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fetching_an_unknown_package_says_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"error": "package or version not found"})),
        )
        .mount(&server)
        .await;

    let base = server.uri();
    let err = blocking(move || {
        Registry::new(&base, None)
            .unwrap()
            .fetch("ghost", "latest")
            .unwrap_err()
    })
    .await;
    assert!(
        format!("{err:#}").contains("not found: package or version not found"),
        "{err:#}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lists_versions_and_the_catalog() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/rivets/forge"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            record("forge", "0.10.0", b"a"),
            record("forge", "0.9.0", b"b"),
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/rivets"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([record("forge", "0.10.0", b"a")])),
        )
        .mount(&server)
        .await;

    let base = server.uri();
    let (versions, catalog) = blocking(move || {
        let registry = Registry::new(&base, None).unwrap();
        (
            registry.versions("forge").unwrap(),
            registry.catalog().unwrap(),
        )
    })
    .await;

    assert_eq!(
        versions
            .iter()
            .map(|v| v.version.as_str())
            .collect::<Vec<_>>(),
        ["0.10.0", "0.9.0"]
    );
    assert_eq!(catalog.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_warehouse_is_reported_as_such() {
    let err = blocking(|| {
        Registry::new("http://127.0.0.1:1", None)
            .unwrap()
            .catalog()
            .unwrap_err()
    })
    .await;
    assert!(
        format!("{err:#}").contains("could not reach Warehouse"),
        "{err:#}"
    );
}

#[test]
fn from_env_needs_a_url_and_credentials() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_env();

    let err = Registry::from_env().unwrap_err();
    assert!(err.to_string().contains("RIVETER_WAREHOUSE_URL"), "{err:#}");

    envmnt::set("RIVETER_WAREHOUSE_URL", "http://warehouse.test/warehouse");
    let err = Registry::from_env().unwrap_err();
    assert!(err.to_string().contains("no credentials"), "{err:#}");

    envmnt::set("RIVETER_WAREHOUSE_TOKEN", "tok");
    assert!(Registry::from_env().is_ok());

    clear_env();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn from_env_exchanges_client_credentials_for_a_token() {
    let gatehouse = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gatehouse/api/v1/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("client_id=ci"))
        .and(body_string_contains("client_secret=shh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "minted", "refresh_token": "", "token_type": "Bearer", "expires_in": 60
        })))
        .expect(1)
        .mount(&gatehouse)
        .await;

    let warehouse = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/rivets"))
        .and(header("authorization", "Bearer minted"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&warehouse)
        .await;

    let gatehouse_url = format!("{}/gatehouse", gatehouse.uri());
    let warehouse_url = warehouse.uri();
    let catalog = blocking(move || {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_env();
        envmnt::set("RIVETER_WAREHOUSE_URL", &warehouse_url);
        envmnt::set("RIVETER_GATEHOUSE_URL", &gatehouse_url);
        envmnt::set("RIVETER_CLIENT_ID", "ci");
        envmnt::set("RIVETER_CLIENT_SECRET", "shh");
        let registry = Registry::from_env();
        clear_env();
        registry.unwrap().catalog().unwrap()
    })
    .await;

    assert!(catalog.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn from_env_reports_refused_client_credentials() {
    let gatehouse = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&gatehouse)
        .await;

    let gatehouse_url = gatehouse.uri();
    let err = blocking(move || {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_env();
        envmnt::set("RIVETER_WAREHOUSE_URL", "http://warehouse.test");
        envmnt::set("RIVETER_GATEHOUSE_URL", &gatehouse_url);
        envmnt::set("RIVETER_CLIENT_ID", "ci");
        envmnt::set("RIVETER_CLIENT_SECRET", "wrong");
        let err = Registry::from_env().unwrap_err();
        clear_env();
        err
    })
    .await;

    assert!(format!("{err:#}").contains("Gatehouse refused"), "{err:#}");
}
