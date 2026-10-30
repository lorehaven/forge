//! The JSON the browser client reads and writes (`/ui/api`): the session check, the reads and the changes.

use crate::support;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::time::Time;
use quench_db::InMemoryDb;
use quench_db::prelude::{Crud, Db};
use quench_http::endpoint::Endpoint;
use quench_http::http::{Method, StatusCode};
use quench_serde::{Value, json};
use std::sync::Arc;
use warehouse_core::{ArtifactEntry, CrateVersion, DockerTag, Overview, RivetEntry, Storages};
use warehouse_service::domain::artifact::{ArtifactMetadata, ArtifactVersion};
use warehouse_service::domain::rivet::RivetPackage;
use warehouse_service::routers::ui::api::Session;

type App = (Arc<dyn Endpoint>, Arc<quench_http::di::Container>);

async fn app(auth_enabled: bool, db: Db) -> App {
    warehouse_service::routers::ui::register_routes();
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = auth_enabled;
    let container = support::container_builder()
        .provide(config)
        .provide(db)
        .build()
        .await
        .unwrap();
    support::app(container)
}

async fn open_app() -> App {
    app(false, Db::InMemory(InMemoryDb::new())).await
}

async fn get(app: &App, path: &str) -> (StatusCode, Value) {
    let resp = app.0.call(support::req(Method::GET, path, &app.1)).await;
    let status = resp.status();
    let text = support::body_text(resp).await;
    (status, json::from_str(&text).unwrap_or(Value::Null))
}

async fn post(app: &App, path: &str, body: &Value) -> (StatusCode, Value) {
    let resp = app
        .0
        .call(support::json_req(Method::POST, path, body, &app.1))
        .await;
    let status = resp.status();
    let text = support::body_text(resp).await;
    (status, json::from_str(&text).unwrap_or(Value::Null))
}

fn claims(scope: &str) -> Claims {
    Claims::for_audiences(
        "user".to_string(),
        vec!["warehouse".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

// ── The session ──────────────────────────────────────────────────────────────

#[quench_test::test]
async fn every_read_and_change_asks_for_a_session() {
    let app = app(true, Db::InMemory(InMemoryDb::new())).await;
    for path in [
        "/ui/api/overview",
        "/ui/api/docker/repositories",
        "/ui/api/docker/tags?repo=x",
        "/ui/api/crates",
        "/ui/api/crates/x/versions",
        "/ui/api/artifacts",
        "/ui/api/rivets",
        "/ui/api/files/storages",
        "/ui/api/files/browse?storage=x",
    ] {
        let (status, body) = get(&app, path).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        assert!(body["error"].is_string(), "{path}: {body:?}");
    }
    for path in [
        "/ui/api/docker/delete-image",
        "/ui/api/crates/yank",
        "/ui/api/crates/unyank",
        "/ui/api/artifacts/yank",
        "/ui/api/artifacts/unyank",
        "/ui/api/rivets/yank",
        "/ui/api/rivets/unyank",
        "/ui/api/files/storages",
        "/ui/api/files/storages/x/edit",
        "/ui/api/files/storages/x/delete",
        "/ui/api/files/delete-file",
    ] {
        let (status, _) = post(&app, path, &json!({})).await;
        assert!(
            status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST,
            "{path}: {status}"
        );
    }
}

#[quench_test::test]
fn a_change_needs_the_blanket_write_grant() {
    let denied = Session(Some(claims("warehouse:read")))
        .manager()
        .unwrap_err();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let none = Session(None).manager().unwrap_err();
    assert_eq!(none.status(), StatusCode::UNAUTHORIZED);
    assert!(Session(Some(claims("warehouse:write"))).manager().is_ok());
    assert!(Session(Some(claims("admin"))).manager().is_ok());
    // Reading needs only the session.
    assert!(Session(Some(claims("warehouse:read"))).require().is_ok());
}

#[quench_test::test]
async fn the_overview_says_who_asks_and_what_is_on() {
    let _rivets = support::WithRivets::new();
    let app = open_app().await;
    let (status, body) = get(&app, "/ui/api/overview").await;
    assert_eq!(status, StatusCode::OK);
    let overview: Overview = json::from_str(&json::to_string(&body)).unwrap();
    assert!(!overview.username.is_empty());
    assert!(overview.can_manage, "auth off is all access");
    assert!(overview.features.rivets);
}

// ── Docker ───────────────────────────────────────────────────────────────────

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn seed_image(root: &std::path::Path, repo: &str, tags: &[&str], digest: &str) {
    let tags_dir = root.join(repo).join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    for tag in tags {
        std::fs::write(tags_dir.join(tag), digest).unwrap();
    }
    let manifests = root.join("manifests").join("sha256");
    std::fs::create_dir_all(&manifests).unwrap();
    std::fs::write(
        manifests.join(digest.strip_prefix("sha256:").unwrap()),
        r#"{"mediaType":"application/vnd.oci.image.manifest.v1+json"}"#,
    )
    .unwrap();
}

#[quench_test::test]
#[allow(clippy::significant_drop_tightening)]
async fn docker_repositories_and_tags_are_listed() {
    let storage = support::WithDockerStorageRoot::new();
    seed_image(storage.dir.path(), "team/app", &["1.0.0", "1.2.0"], DIGEST);
    let app = open_app().await;

    let (status, body) = get(&app, "/ui/api/docker/repositories").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!(["team/app"]));

    let (status, body) = get(&app, "/ui/api/docker/tags?repo=team%2Fapp").await;
    assert_eq!(status, StatusCode::OK);
    let tags: Vec<DockerTag> = json::from_str(&json::to_string(&body)).unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].tag, "1.2.0", "newest first");
    assert_eq!(tags[0].digest, DIGEST);
    assert_eq!(
        tags[0].media_type.as_deref(),
        Some("application/vnd.oci.image.manifest.v1+json")
    );
    assert!(tags[0].size_bytes.is_some());

    let (status, body) = get(&app, "/ui/api/docker/tags?repo=nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
}

#[quench_test::test]
#[allow(clippy::significant_drop_tightening)]
async fn deleting_an_image_removes_the_manifest_and_every_tag_of_it() {
    let storage = support::WithDockerStorageRoot::new();
    seed_image(storage.dir.path(), "app", &["1", "latest"], DIGEST);
    let other = "sha256:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    std::fs::write(storage.dir.path().join("app/tags/2"), other).unwrap();
    let app = open_app().await;

    let (status, _) = post(
        &app,
        "/ui/api/docker/delete-image",
        &json!({"repository": "app", "digest": DIGEST}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = get(&app, "/ui/api/docker/tags?repo=app").await;
    let tags: Vec<DockerTag> = json::from_str(&json::to_string(&body)).unwrap();
    assert_eq!(
        tags.iter().map(|t| t.tag.as_str()).collect::<Vec<_>>(),
        ["2"],
        "only the other image's tag is left"
    );

    let (status, body) = post(
        &app,
        "/ui/api/docker/delete-image",
        &json!({"repository": "app", "digest": DIGEST}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
    assert_eq!(body["error"], "api_error_manifest_unknown");
}

#[quench_test::test]
async fn deleting_an_image_checks_its_names() {
    let _storage = support::WithDockerStorageRoot::new();
    let app = open_app().await;
    let (status, body) = post(
        &app,
        "/ui/api/docker/delete-image",
        &json!({"repository": "app", "digest": "nope"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "api_error_digest_required");
    let (status, body) = post(
        &app,
        "/ui/api/docker/delete-image",
        &json!({"repository": "../x", "digest": DIGEST}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "api_error_invalid_repository");
}

// ── Crates ───────────────────────────────────────────────────────────────────

fn seed_crate(name: &str, versions: &[(&str, bool)]) {
    let path = warehouse_service::routers::crates::index_file_path(name).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut content = String::new();
    for (vers, yanked) in versions {
        let line = json!({
            "name": name, "vers": *vers, "cksum": "abc", "yanked": *yanked,
            "features": {"serde": ["dep:serde"]}, "features2": {"extra": []},
            "rust_version": "1.80",
            "deps": [
                {"name": "serde", "req": "^1", "features": [], "optional": true,
                 "default_features": true, "target": "cfg(unix)", "kind": "normal"},
                {"name": "tempfile", "req": "^3", "features": [], "optional": false,
                 "default_features": true, "target": null, "kind": "dev", "package": "tmp"}
            ],
            "v": 2
        });
        content.push_str(&json::to_string(&line));
        content.push('\n');
    }
    std::fs::write(path, content).unwrap();
}

#[quench_test::test]
async fn crates_and_their_versions_are_listed_from_the_index() {
    let _storage = support::WithCratesStorageRoot::new();
    seed_crate("zeta", &[("0.1.0", false)]);
    seed_crate("alpha", &[("1.0.0", false), ("1.1.0", true)]);
    let app = open_app().await;

    let (_, body) = get(&app, "/ui/api/crates").await;
    assert_eq!(body, json!(["alpha", "zeta"]));

    let (status, body) = get(&app, "/ui/api/crates/alpha/versions").await;
    assert_eq!(status, StatusCode::OK);
    let versions: Vec<CrateVersion> = json::from_str(&json::to_string(&body)).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].vers, "1.0.0", "oldest first");
    assert!(versions[1].yanked);
    assert_eq!(
        versions[0].features,
        ["extra", "serde"],
        "both tables, sorted"
    );
    assert_eq!(versions[0].rust_version.as_deref(), Some("1.80"));
    assert_eq!(versions[0].deps.len(), 2);
    assert_eq!(
        versions[0].deps[0].line(),
        "serde ^1 [optional] [target: cfg(unix)]"
    );
    assert_eq!(versions[0].deps[1].line(), "tmp (as tempfile) ^3");

    let (status, _) = get(&app, "/ui/api/crates/missing/versions").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn a_crate_version_is_yanked_and_unyanked() {
    let _storage = support::WithCratesStorageRoot::new();
    seed_crate("alpha", &[("1.0.0", false)]);
    let app = open_app().await;
    let body = json!({"name": "alpha", "version": "1.0.0"});

    let (status, _) = post(&app, "/ui/api/crates/yank", &body).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, versions) = get(&app, "/ui/api/crates/alpha/versions").await;
    assert_eq!(versions[0]["yanked"], true);

    let (status, _) = post(&app, "/ui/api/crates/unyank", &body).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, versions) = get(&app, "/ui/api/crates/alpha/versions").await;
    assert_eq!(versions[0]["yanked"], false);

    let (status, body) = post(
        &app,
        "/ui/api/crates/yank",
        &json!({"name": "alpha", "version": "9.9.9"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "api_error_crate_version_not_found");
}

// ── Artifacts ────────────────────────────────────────────────────────────────

#[quench_test::test]
fn an_artifact_row_becomes_the_clients_entry() {
    let version = ArtifactVersion {
        id: "x".into(),
        program: "app".into(),
        platform: "android".into(),
        arch: Some("arm64".into()),
        format: "apk".into(),
        version_code: 7,
        version_name: "1.7".into(),
        filename: "app.apk".into(),
        size_bytes: 4096,
        sha256: "dead".into(),
        label: Some("App".into()),
        metadata: ArtifactMetadata {
            min_sdk_version: Some(26),
            target_sdk_version: Some(35),
            permissions: vec!["INTERNET".into()],
        },
        uploaded_by: "dev".into(),
        yanked: true,
        created_at: Time::now(),
    };
    let entry: ArtifactEntry = warehouse_service::routers::ui::api::artifacts::entry_of(version);
    assert_eq!((entry.program.as_str(), entry.version_code), ("app", 7));
    assert_eq!(entry.min_sdk_version, Some(26));
    assert_eq!(entry.permissions, ["INTERNET"]);
    assert!(entry.yanked);
    assert_eq!(entry.arch.as_deref(), Some("arm64"));
}

#[quench_test::test]
async fn a_registry_that_is_off_lists_nothing_and_changes_nothing() {
    // `FEATURE_ARTIFACTS_ENABLED` is unset in this sandbox, and the flag is read once for the whole test binary.
    if warehouse_service::routers::artifacts_enabled() {
        return;
    }
    let db = Db::InMemory(InMemoryDb::new());
    let app = app(false, db).await;
    let (status, body) = get(&app, "/ui/api/artifacts").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
    let (status, body) = post(
        &app,
        "/ui/api/artifacts/yank",
        &json!({"program": "a", "platform": "linux", "version_code": 1}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "api_error_artifacts_disabled");
}

// ── Rivets ───────────────────────────────────────────────────────────────────

fn rivet(name: &str, version: &str) -> RivetPackage {
    RivetPackage {
        id: RivetPackage::id_for(name, version),
        name: name.into(),
        version: version.into(),
        description: Some("a package".into()),
        namespace: Some("ns".into()),
        filename: format!("{name}-{version}.rivet"),
        size_bytes: 10,
        sha256: "beef".into(),
        manifest: json!({
            "requires": {"riveter": ">=1", "packages": ["base", "other"]},
            "meta": {"team": "infra", "level": 3}
        }),
        uploaded_by: "dev".into(),
        yanked: false,
        created_at: Time::now(),
    }
}

#[quench_test::test]
async fn rivet_packages_are_listed_newest_first_with_their_manifest() {
    let _rivets = support::WithRivets::new();
    let db = Db::InMemory(InMemoryDb::new());
    for (name, version) in [
        ("pkg", "1.0.0"),
        ("pkg", "1.10.0"),
        ("pkg", "1.2.0"),
        ("aaa", "0.1.0"),
    ] {
        db.repository::<RivetPackage>()
            .create(&rivet(name, version))
            .await
            .unwrap();
    }
    let app = app(false, db).await;
    let (status, body) = get(&app, "/ui/api/rivets").await;
    assert_eq!(status, StatusCode::OK);
    let entries: Vec<RivetEntry> = json::from_str(&json::to_string(&body)).unwrap();
    let pkg: Vec<&str> = entries
        .iter()
        .filter(|e| e.name == "pkg")
        .map(|e| e.version.as_str())
        .collect();
    assert_eq!(pkg, ["1.10.0", "1.2.0", "1.0.0"], "by semver, not by text");
    let first = &entries[0];
    assert_eq!(first.requires_riveter.as_deref(), Some(">=1"));
    assert_eq!(first.requires_packages, ["base", "other"]);
    assert_eq!(first.namespace.as_deref(), Some("ns"));
    assert!(first.published.ends_with("UTC"), "{}", first.published);
    let meta: Vec<(&str, &str)> = first
        .meta
        .iter()
        .map(|m| (m.key.as_str(), m.value.as_str()))
        .collect();
    assert!(
        meta.contains(&("team", "infra")) && meta.contains(&("level", "3")),
        "{meta:?}"
    );
}

#[quench_test::test]
async fn a_rivet_version_is_yanked_and_unyanked() {
    let _rivets = support::WithRivets::new();
    let db = Db::InMemory(InMemoryDb::new());
    db.repository::<RivetPackage>()
        .create(&rivet("pkg", "1.0.0"))
        .await
        .unwrap();
    let app = app(false, db).await;
    let body = json!({"name": "pkg", "version": "1.0.0"});

    let (status, _) = post(&app, "/ui/api/rivets/yank", &body).await;
    assert!(status.is_success(), "{status}");
    let (_, list) = get(&app, "/ui/api/rivets").await;
    assert_eq!(list[0]["yanked"], true);

    let (status, _) = post(&app, "/ui/api/rivets/unyank", &body).await;
    assert!(status.is_success(), "{status}");
    let (_, list) = get(&app, "/ui/api/rivets").await;
    assert_eq!(list[0]["yanked"], false);

    let (status, _) = post(
        &app,
        "/ui/api/rivets/yank",
        &json!({"name": "pkg", "version": "9.0.0"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[quench_test::test]
async fn a_rivet_registry_that_is_off_is_empty_and_refuses_changes() {
    let _rivets = support::WithRivets::disabled();
    let app = open_app().await;
    let (_, list) = get(&app, "/ui/api/rivets").await;
    assert_eq!(list, json!([]));
    let (status, body) = post(
        &app,
        "/ui/api/rivets/yank",
        &json!({"name": "pkg", "version": "1.0.0"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "api_error_rivets_disabled");
}

// ── Files ────────────────────────────────────────────────────────────────────

#[quench_test::test]
async fn storages_with_no_database_are_just_the_configured_ones() {
    let app = open_app().await;
    let (status, body) = get(&app, "/ui/api/files/storages").await;
    assert_eq!(status, StatusCode::OK);
    let storages: Storages = json::from_str(&json::to_string(&body)).unwrap();
    assert!(storages.dynamic.is_empty());
}

#[quench_test::test]
async fn browsing_a_storage_that_does_not_exist_says_so() {
    let app = open_app().await;
    let (status, body) = get(&app, "/ui/api/files/browse?storage=nothing-here").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["access"], "missing");
    assert_eq!(body["files"], json!([]));
}

#[quench_test::test]
async fn file_changes_are_refused_while_the_registry_is_off() {
    // The flag is read once for the whole test binary.
    if warehouse_service::routers::files_enabled() {
        return;
    }
    let app = open_app().await;
    for (path, body) in [
        (
            "/ui/api/files/storages",
            json!({"name": "s", "owner": "o", "sync_enabled": false}),
        ),
        (
            "/ui/api/files/storages/s/edit",
            json!({"clear_max_file": false, "sync_enabled": false}),
        ),
        ("/ui/api/files/storages/s/delete", json!({})),
        (
            "/ui/api/files/delete-file",
            json!({"storage": "s", "path": "a.txt"}),
        ),
    ] {
        let (status, body) = post(&app, path, &body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body["error"], "api_error_files_disabled", "{path}");
    }
}
