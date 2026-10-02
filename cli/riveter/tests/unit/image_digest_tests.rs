use reqwest::blocking::Client;
use riveter::image_updates::{
    FetchError, ImageRef, RegistryAuth, RegistryCredentials, RegistryDigests, is_sha256_digest,
    resolve_digest,
};
use riveter::package::DigestResolver;
use wiremock::matchers::{header, header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn image(registry: String) -> ImageRef {
    ImageRef {
        original: format!("{registry}/forge/sage:0.5.3"),
        registry,
        repository: "forge/sage".to_string(),
        tag: "0.5.3".to_string(),
    }
}

async fn resolve(server: &MockServer, auth: RegistryAuth) -> Result<String, FetchError> {
    let image = image(server.address().to_string());
    tokio::task::spawn_blocking(move || resolve_digest(&Client::new(), &image, &auth, "http"))
        .await
        .unwrap()
}

#[test]
fn recognises_only_well_formed_sha256_digests() {
    assert!(is_sha256_digest(DIGEST));
    assert!(!is_sha256_digest("sha256:abc"));
    assert!(!is_sha256_digest(
        "sha512:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    ));
    assert!(!is_sha256_digest(&DIGEST.replace('0', "g")));
    assert!(!is_sha256_digest(""));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reads_the_digest_header_asking_for_index_and_manifest_types() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .and(path("/v2/forge/sage/manifests/0.5.3"))
        .and(header_regex("accept", "image.index.v1\\+json"))
        .and(header_regex("accept", "manifest.v2\\+json"))
        .respond_with(ResponseTemplate::new(200).append_header("Docker-Content-Digest", DIGEST))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        resolve(&server, RegistryAuth::default()).await.unwrap(),
        DIGEST
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hashes_the_manifest_itself_when_the_registry_sends_no_digest_header() {
    let server = MockServer::start().await;
    let manifest = br#"{"schemaVersion":2}"#;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/forge/sage/manifests/0.5.3"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(manifest.to_vec()))
        .expect(1)
        .mount(&server)
        .await;

    assert_eq!(
        resolve(&server, RegistryAuth::default()).await.unwrap(),
        format!("sha256:{}", rivet_package::sha256_hex(manifest))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follows_a_bearer_challenge_and_retries_with_the_token() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "pull-token"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .and(header("authorization", "Bearer pull-token"))
        .respond_with(ResponseTemplate::new(200).append_header("Docker-Content-Digest", DIGEST))
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(401).append_header(
            "WWW-Authenticate",
            format!(
                "Bearer realm=\"http://{}/token\",service=\"registry\",scope=\"repository:forge/sage:pull\"",
                server.address()
            ),
        ))
        .mount(&server)
        .await;

    assert_eq!(
        resolve(&server, RegistryAuth::default()).await.unwrap(),
        DIGEST
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unauthenticated_challenge_with_no_credentials_is_denied() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = resolve(&server, RegistryAuth::default()).await.unwrap_err();
    assert!(matches!(err, FetchError::Denied(_)), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn falls_back_to_basic_credentials_when_there_is_no_token_endpoint() {
    let server = MockServer::start().await;
    let mut auth = RegistryAuth::default();
    auth.credentials.insert(
        "*".to_string(),
        RegistryCredentials {
            username: "ci".into(),
            password: "pw".into(),
        },
    );
    let basic = auth.basic_header("anything").unwrap();

    Mock::given(method("HEAD"))
        .and(header("authorization", basic.as_str()))
        .respond_with(ResponseTemplate::new(200).append_header("Docker-Content-Digest", DIGEST))
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    assert_eq!(resolve(&server, auth).await.unwrap(), DIGEST);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_tag_is_an_error_not_a_guess() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let err = resolve(&server, RegistryAuth::default()).await.unwrap_err();
    assert!(err.to_string().contains("HTTP 404"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_resolver_falls_back_to_plain_http_for_a_registry_without_tls() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(200).append_header("Docker-Content-Digest", DIGEST))
        .mount(&server)
        .await;

    let image = image(server.address().to_string());
    let digest = tokio::task::spawn_blocking(move || RegistryDigests::new(&[]).digest(&image))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(digest, DIGEST);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_resolver_names_the_image_that_failed() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let image = image(server.address().to_string());
    let original = image.original.clone();
    let err = tokio::task::spawn_blocking(move || RegistryDigests::new(&[]).digest(&image))
        .await
        .unwrap()
        .unwrap_err();
    assert!(err.to_string().starts_with(&original), "{err}");
}
