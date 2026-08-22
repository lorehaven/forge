use base64::Engine as _;
use reqwest::blocking::Client;
use riveter::image_updates::{
    ImageOccurrence, ImageRef, RegistryAuth, RegistryCredentials, ScanMessage, UpdateCandidate,
    apply_updates, discover_images, docker_config_credentials, encode_repository_path,
    newest_compatible_tag, normalize_registry_key, parse_image_ref, parse_registry_auth_env,
    parse_registry_auth_item, parse_username_password, print_results, print_rows, registry_v2_tags,
    version_key,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `docker_config_credentials` reads `DOCKER_CONFIG`, a fixed env var
/// name every test that sets it must serialize around - cargo runs
/// tests in this binary in parallel by default.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// --- parse_image_ref -----------------------------------------------

#[test]
fn parse_image_ref_rejects_digest_pinned_images() {
    assert_eq!(
        parse_image_ref("redis@sha256:abcd"),
        Err("digest-pinned images are not supported")
    );
}

#[test]
fn parse_image_ref_rejects_a_missing_tag() {
    assert_eq!(parse_image_ref("redis"), Err("image has no explicit tag"));
}

#[test]
fn parse_image_ref_bare_name_is_docker_hub_library() {
    let image = parse_image_ref("redis:7.2").unwrap();
    assert_eq!(image.registry, "registry-1.docker.io");
    assert_eq!(image.repository, "library/redis");
    assert_eq!(image.tag, "7.2");
    assert_eq!(image.original, "redis:7.2");
}

#[test]
fn parse_image_ref_namespaced_docker_hub_repo_keeps_the_namespace() {
    let image = parse_image_ref("bitnami/postgresql:16").unwrap();
    assert_eq!(image.registry, "registry-1.docker.io");
    assert_eq!(image.repository, "bitnami/postgresql");
}

#[test]
fn parse_image_ref_explicit_docker_io_normalizes_to_hub_host() {
    let image = parse_image_ref("docker.io/library/redis:7").unwrap();
    assert_eq!(image.registry, "registry-1.docker.io");
    assert_eq!(image.repository, "library/redis");
}

#[test]
fn parse_image_ref_private_registry_with_dot() {
    let image = parse_image_ref("ennor.ddns.net/forge/conveyor:1.2.3").unwrap();
    assert_eq!(image.registry, "ennor.ddns.net");
    assert_eq!(image.repository, "forge/conveyor");
    assert_eq!(image.tag, "1.2.3");
}

#[test]
fn parse_image_ref_private_registry_with_port() {
    let image = parse_image_ref("registry.local:8443/forge/conveyor:1.2.3").unwrap();
    assert_eq!(image.registry, "registry.local:8443");
    assert_eq!(image.repository, "forge/conveyor");
}

#[test]
fn parse_image_ref_localhost_registry() {
    let image = parse_image_ref("localhost/myimage:dev").unwrap();
    assert_eq!(image.registry, "localhost");
    assert_eq!(image.repository, "myimage");
}

#[test]
fn parse_image_ref_rejects_an_empty_repository() {
    assert_eq!(
        parse_image_ref("ennor.ddns.net/:1.0"),
        Err("image repository is empty")
    );
}

// --- ImageRef::display_repository / with_tag ------------------------

#[test]
fn display_repository_omits_the_host_for_docker_hub() {
    let image = parse_image_ref("redis:7.2").unwrap();
    assert_eq!(image.display_repository(), "library/redis");
    assert_eq!(image.with_tag("7.4"), "library/redis:7.4");
}

#[test]
fn display_repository_includes_the_host_for_a_private_registry() {
    let image = parse_image_ref("ennor.ddns.net/forge/conveyor:1.2.3").unwrap();
    assert_eq!(image.display_repository(), "ennor.ddns.net/forge/conveyor");
    assert_eq!(
        image.with_tag("1.3.0"),
        "ennor.ddns.net/forge/conveyor:1.3.0"
    );
}

// --- version_key / newest_compatible_tag -----------------------------

#[test]
fn version_key_splits_prefix_numbers_and_suffix() {
    assert_eq!(
        version_key("v1.2.3-alpine"),
        Some(("v".to_string(), vec![1, 2, 3], "-alpine".to_string()))
    );
}

#[test]
fn version_key_is_none_for_a_non_versioned_tag() {
    assert_eq!(version_key("latest"), None);
    assert_eq!(version_key("stable"), None);
}

#[test]
fn newest_compatible_tag_is_none_for_a_floating_current_tag() {
    assert_eq!(
        newest_compatible_tag("latest", &["1.0.0".to_string()]),
        None
    );
}

#[test]
fn newest_compatible_tag_picks_the_highest_numeric_version() {
    let tags = ["1.0.0", "1.2.0", "1.10.0", "1.9.0"]
        .map(String::from)
        .to_vec();
    assert_eq!(
        newest_compatible_tag("1.0.0", &tags),
        Some("1.10.0".to_string())
    );
}

#[test]
fn newest_compatible_tag_never_suggests_a_downgrade() {
    let tags = ["1.0.0".to_string()];
    assert_eq!(newest_compatible_tag("2.0.0", &tags), None);
}

#[test]
fn newest_compatible_tag_ignores_a_different_prefix_or_suffix() {
    let tags = ["v2.0.0".to_string(), "2.0.0-alpine".to_string()];
    assert_eq!(newest_compatible_tag("1.0.0", &tags), None);
}

#[test]
fn newest_compatible_tag_refuses_to_cross_a_three_digit_epoch_boundary() {
    let tags = ["100.0.0".to_string()];
    assert_eq!(newest_compatible_tag("9.0.0", &tags), None);
}

#[test]
fn newest_compatible_tag_rejects_a_tag_with_fewer_components() {
    let tags = ["2".to_string()];
    assert_eq!(newest_compatible_tag("1.0.0", &tags), None);
}

#[test]
fn newest_compatible_tag_rejects_a_tag_with_far_more_components() {
    // current has 1 component, so anything above max(1, 3) = 3 is refused.
    let tags = ["1.0.0.0.0".to_string()];
    assert_eq!(newest_compatible_tag("1", &tags), None);
}

// --- registry auth parsing -------------------------------------------

#[test]
fn parse_username_password_splits_on_the_first_colon() {
    let creds = parse_username_password("alice:s3cr3t:with:colons").unwrap();
    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "s3cr3t:with:colons");
}

#[test]
fn parse_username_password_rejects_an_empty_username() {
    assert!(parse_username_password(":secret").is_none());
}

#[test]
fn parse_username_password_rejects_a_value_with_no_colon() {
    assert!(parse_username_password("nocolon").is_none());
}

#[test]
fn parse_registry_auth_item_parses_registry_equals_creds() {
    // Only the whole item is trimmed up front - whitespace adjacent to
    // the `=` itself is left in place, so keep this input free of it.
    let (registry, creds) = parse_registry_auth_item(" ennor.ddns.net=alice:secret ").unwrap();
    assert_eq!(registry, "ennor.ddns.net");
    assert_eq!(creds.username, "alice");
    assert_eq!(creds.password, "secret");
}

#[test]
fn parse_registry_auth_item_rejects_missing_equals() {
    assert!(parse_registry_auth_item("ennor.ddns.net-alice:secret").is_none());
}

#[test]
fn parse_registry_auth_env_parses_multiple_semicolon_separated_entries() {
    let parsed = parse_registry_auth_env("registry-a=alice:pw1;registry-b=bob:pw2;garbage;");
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed["registry-a"].username, "alice");
    assert_eq!(parsed["registry-b"].username, "bob");
}

#[test]
fn normalize_registry_key_strips_scheme_and_trailing_slash() {
    assert_eq!(
        normalize_registry_key("https://ennor.ddns.net/"),
        "ennor.ddns.net"
    );
}

#[test]
fn normalize_registry_key_maps_docker_hub_aliases_to_the_canonical_host() {
    assert_eq!(normalize_registry_key("docker.io"), "registry-1.docker.io");
    assert_eq!(
        normalize_registry_key("index.docker.io"),
        "registry-1.docker.io"
    );
    // NOTE: the scheme is stripped by `without_scheme` *before* this
    // match runs, so the `"https://index.docker.io/v1"` arm in
    // `normalize_registry_key`'s `matches!` can never actually match -
    // it's dead code, and this input falls through to the `else` branch
    // unchanged instead of normalizing. Documenting the real behavior
    // here rather than silently asserting something the code doesn't do;
    // flagged in the coverage-push report as a likely pre-existing bug,
    // not fixed here.
    assert_eq!(
        normalize_registry_key("https://index.docker.io/v1"),
        "index.docker.io/v1"
    );
}

#[test]
fn normalize_registry_key_leaves_the_wildcard_alone() {
    assert_eq!(normalize_registry_key("*"), "*");
}

#[test]
fn registry_auth_from_sources_cli_flag_overrides_env() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set("DOCKER_CONFIG", "/does/not/exist");
    envmnt::remove("RIVETER_REGISTRY_AUTH");
    envmnt::set("RIVETER_REGISTRY_USERNAME", "env-user");
    envmnt::set("RIVETER_REGISTRY_PASSWORD", "env-pass");

    let auth = RegistryAuth::from_sources(&["myregistry.example=cli-user:cli-pass".to_string()]);

    assert_eq!(
        auth.basic_header("myregistry.example"),
        Some(
            RegistryCredentials {
                username: "cli-user".to_string(),
                password: "cli-pass".to_string(),
            }
            .basic_auth_header()
        )
    );
    // The wildcard entry from RIVETER_REGISTRY_USERNAME/PASSWORD still
    // covers a registry no explicit source names.
    assert_eq!(
        auth.basic_header("unrelated.example"),
        Some(
            RegistryCredentials {
                username: "env-user".to_string(),
                password: "env-pass".to_string(),
            }
            .basic_auth_header()
        )
    );

    envmnt::remove("DOCKER_CONFIG");
    envmnt::remove("RIVETER_REGISTRY_USERNAME");
    envmnt::remove("RIVETER_REGISTRY_PASSWORD");
}

#[test]
fn registry_auth_from_sources_reads_the_registry_auth_env_var() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set("DOCKER_CONFIG", "/does/not/exist");
    envmnt::remove("RIVETER_REGISTRY_USERNAME");
    envmnt::remove("RIVETER_REGISTRY_PASSWORD");
    envmnt::set(
        "RIVETER_REGISTRY_AUTH",
        "registry.example=env-user:env-pass;other.example=o:p",
    );

    let auth = RegistryAuth::from_sources(&[]);

    assert_eq!(
        auth.basic_header("registry.example"),
        Some(
            RegistryCredentials {
                username: "env-user".to_string(),
                password: "env-pass".to_string(),
            }
            .basic_auth_header()
        )
    );
    assert!(auth.basic_header("other.example").is_some());
    assert!(auth.basic_header("unrelated.example").is_none());

    envmnt::remove("DOCKER_CONFIG");
    envmnt::remove("RIVETER_REGISTRY_AUTH");
}

#[test]
fn registry_auth_get_falls_back_to_wildcard() {
    let auth = RegistryAuth {
        credentials: BTreeMap::from([(
            "*".to_string(),
            RegistryCredentials {
                username: "u".to_string(),
                password: "p".to_string(),
            },
        )]),
    };
    assert!(auth.get("anything.example").is_some());
    assert!(auth.basic_header("anything.example").is_some());
}

// --- docker_config_credentials ---------------------------------------

#[test]
fn docker_config_credentials_reads_plaintext_username_and_password() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    // Plain "docker.io" (not the "https://index.docker.io/v1" form) -
    // see `normalize_registry_key_maps_docker_hub_aliases_to_the_canonical_host`
    // for why the longer form doesn't actually normalize.
    std::fs::write(
        dir.path().join("config.json"),
        r#"{"auths": {"docker.io": {"username": "alice", "password": "secret"}}}"#,
    )
    .unwrap();
    envmnt::set("DOCKER_CONFIG", dir.path().to_str().unwrap());

    let creds = docker_config_credentials();
    assert_eq!(
        creds
            .get("registry-1.docker.io")
            .map(|c| c.username.as_str()),
        Some("alice")
    );

    envmnt::remove("DOCKER_CONFIG");
}

#[test]
fn docker_config_credentials_decodes_base64_auth_field() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let encoded = base64::engine::general_purpose::STANDARD.encode("bob:hunter2");
    std::fs::write(
        dir.path().join("config.json"),
        format!(r#"{{"auths": {{"ennor.ddns.net": {{"auth": "{encoded}"}}}}}}"#),
    )
    .unwrap();
    envmnt::set("DOCKER_CONFIG", dir.path().to_str().unwrap());

    let creds = docker_config_credentials();
    assert_eq!(
        creds.get("ennor.ddns.net").map(|c| c.username.as_str()),
        Some("bob")
    );

    envmnt::remove("DOCKER_CONFIG");
}

#[test]
fn docker_config_credentials_is_empty_when_the_file_is_missing() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::set("DOCKER_CONFIG", "/definitely/does/not/exist");
    assert!(docker_config_credentials().is_empty());
    envmnt::remove("DOCKER_CONFIG");
}

#[test]
fn docker_config_credentials_is_empty_for_invalid_json() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.json"), "not json").unwrap();
    envmnt::set("DOCKER_CONFIG", dir.path().to_str().unwrap());
    assert!(docker_config_credentials().is_empty());
    envmnt::remove("DOCKER_CONFIG");
}

#[test]
fn docker_config_credentials_falls_back_to_home_docker_config_when_unset() {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    envmnt::remove("DOCKER_CONFIG");
    // Whatever this machine's real `$HOME/.docker/config.json` says (most
    // likely nothing) - just confirm the fallback path (`dirs_home`) reads
    // without panicking rather than requiring `DOCKER_CONFIG` to be set.
    let _ = docker_config_credentials();
}

// --- encode_repository_path -------------------------------------------

#[test]
fn encode_repository_path_leaves_slashes_alone_but_escapes_segments() {
    assert_eq!(encode_repository_path("forge/my image"), "forge/my%20image");
}

// --- discover_images ---------------------------------------------------

#[test]
fn discover_images_finds_image_lines_in_yaml_j2_templates_only() {
    let dir = tempfile::tempdir().unwrap();
    // `IMAGE_LINE_RE` requires the line to be only whitespace before
    // `image:` - a leading `- ` (a YAML sequence marker) would not
    // match, so this uses a bare `image:` mapping key, same as the
    // overlay templates this actually scans.
    std::fs::write(
        dir.path().join("deployment.yaml.j2"),
        "spec:\n  containers:\n  - name: cache\n    image: redis:7.2\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("README.md"), "image: redis:7.2\n").unwrap();

    let occurrences = discover_images(dir.path()).unwrap();
    assert_eq!(occurrences.len(), 1);
    assert_eq!(occurrences[0].image.tag, "7.2");
    assert_eq!(occurrences[0].line_number, 4);
}

#[test]
fn discover_images_skips_a_line_that_does_not_parse_as_an_image() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("deployment.yaml.j2"),
        "image: no-tag-here\n",
    )
    .unwrap();

    let occurrences = discover_images(dir.path()).unwrap();
    assert!(occurrences.is_empty());
}

// --- apply_updates -------------------------------------------------------

#[test]
fn apply_updates_rewrites_only_the_matching_image_line() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("deployment.yaml.j2");
    std::fs::write(
        &template_path,
        "spec:\n  image: redis:7.2\n  other: unchanged\n",
    )
    .unwrap();

    let occurrence = ImageOccurrence {
        path: template_path.clone(),
        line_number: 2,
        image: parse_image_ref("redis:7.2").unwrap(),
    };
    let update = UpdateCandidate {
        occurrence,
        newest_tag: "7.4".to_string(),
    };

    apply_updates(&[update]).unwrap();

    let rewritten = std::fs::read_to_string(&template_path).unwrap();
    assert_eq!(
        rewritten,
        "spec:\n  image: library/redis:7.4\n  other: unchanged\n"
    );
}

// --- print_rows / print_results: just must not panic -------------------

#[test]
fn print_rows_handles_an_empty_and_a_populated_table_without_panicking() {
    print_rows("Empty", &[]);
    print_rows(
        "Populated",
        &[[
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ]],
    );
}

#[test]
fn print_results_handles_a_mix_of_updates_skips_and_errors() {
    let occurrence = ImageOccurrence {
        path: PathBuf::from("deployment.yaml.j2"),
        line_number: 1,
        image: parse_image_ref("redis:7.2").unwrap(),
    };
    let updates = vec![UpdateCandidate {
        occurrence: occurrence.clone(),
        newest_tag: "7.4".to_string(),
    }];
    let messages = vec![
        ScanMessage::Skip {
            occurrence: occurrence.clone(),
            detail: "uses floating tag".to_string(),
        },
        ScanMessage::Error {
            occurrence,
            detail: "unreachable".to_string(),
        },
    ];
    print_results(&updates, &messages);
}

// --- bearer_token / registry_v2_tags: exercised against a real local
// HTTP server via wiremock, since `registry_v2_tags` takes the registry
// host from `ImageRef::registry`, which a test can point at the mock
// server's address. `reqwest::blocking::Client` cannot run inside an
// already-active tokio runtime (it spins its own), so the blocking call
// is dispatched via `spawn_blocking` from a multi-threaded test runtime,
// which keeps the mock server's listener polled concurrently.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_v2_tags_follows_pagination_and_returns_every_tag() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/forge/conveyor/tags/list"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"tags": ["1.0.0"]}))
                .append_header(
                    "Link",
                    format!(
                        r#"<{}/v2/forge/conveyor/tags/list?n=100&last=1.0.0>; rel="next""#,
                        server.uri()
                    ),
                ),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/forge/conveyor/tags/list"))
        .and(wiremock::matchers::query_param("last", "1.0.0"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"tags": ["1.1.0"]})),
        )
        .mount(&server)
        .await;

    let address = server.address().to_string();
    let tags = tokio::task::spawn_blocking(move || {
        let client = Client::new();
        let image = ImageRef {
            original: String::new(),
            registry: address,
            repository: "forge/conveyor".to_string(),
            tag: "1.0.0".to_string(),
        };
        registry_v2_tags(&client, &image, &RegistryAuth::default(), "http").unwrap()
    })
    .await
    .unwrap();

    assert_eq!(tags, vec!["1.0.0".to_string(), "1.1.0".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_v2_tags_retries_with_a_bearer_token_after_a_401() {
    let registry = MockServer::start().await;
    let auth_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v2/forge/conveyor/tags/list"))
        .respond_with(ResponseTemplate::new(401).append_header(
            "WWW-Authenticate",
            format!(
                r#"Bearer realm="{}/token",service="forge",scope="repository:forge/conveyor:pull""#,
                auth_server.uri()
            ),
        ))
        .up_to_n_times(1)
        .mount(&registry)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/forge/conveyor/tags/list"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"tags": ["2.0.0"]})),
        )
        .mount(&registry)
        .await;
    Mock::given(method("GET"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "test-token"})),
        )
        .mount(&auth_server)
        .await;

    let address = registry.address().to_string();
    let tags = tokio::task::spawn_blocking(move || {
        let client = Client::new();
        let image = ImageRef {
            original: String::new(),
            registry: address,
            repository: "forge/conveyor".to_string(),
            tag: "1.0.0".to_string(),
        };
        registry_v2_tags(&client, &image, &RegistryAuth::default(), "http").unwrap()
    })
    .await
    .unwrap();

    assert_eq!(tags, vec!["2.0.0".to_string()]);
}
