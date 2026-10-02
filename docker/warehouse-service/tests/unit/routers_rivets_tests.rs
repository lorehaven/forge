use crate::support::{self, WithRivets};
use http::{Method, StatusCode};
use quench_db::{Db, InMemoryDb};
use quench_http::di::Container;
use quench_http::endpoint::Endpoint;
use rivet_package::{Manifest, PackageBuilder};
use std::sync::Arc;
use warehouse_service::routers::rivets;

fn rivet(name: &str, version: &str) -> Vec<u8> {
    let mut manifest = Manifest::new(name, version);
    manifest.package.description = Some("a test overlay".to_string());
    manifest.package.namespace = Some("forge".to_string());
    let mut builder = PackageBuilder::new(manifest);
    builder
        .add_file("overlay.yaml", b"env: forge\n".to_vec())
        .unwrap();
    builder
        .add_file("templates/a.yaml.j2", b"kind: A\n".to_vec())
        .unwrap();
    builder.build().unwrap()
}

async fn setup() -> (Arc<dyn Endpoint>, Arc<Container>) {
    rivets::register_routes();
    let container = support::container_builder()
        .provide(Db::InMemory(InMemoryDb::new()))
        .build()
        .await
        .unwrap();
    support::app(container).await
}

async fn publish(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    name: &str,
    version: &str,
    bytes: &[u8],
) -> quench_http::response::Response {
    app.call(support::raw_req(
        Method::PUT,
        &format!("/api/v1/rivets/{name}/{version}"),
        &[],
        bytes,
        container,
    ))
    .await
}

async fn call(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    method: Method,
    path: &str,
) -> quench_http::response::Response {
    app.call(support::req(method, path, container)).await
}

#[tokio::test]
async fn every_route_reports_not_found_when_disabled() {
    let _rivets = WithRivets::disabled();
    let (app, container) = setup().await;

    for (method, path) in [
        (Method::PUT, "/api/v1/rivets/forge/1.0.0"),
        (Method::GET, "/api/v1/rivets"),
        (Method::GET, "/api/v1/rivets/forge"),
        (Method::GET, "/api/v1/rivets/forge/1.0.0"),
        (Method::GET, "/api/v1/rivets/forge/1.0.0/download"),
        (Method::DELETE, "/api/v1/rivets/forge/1.0.0/yank"),
        (Method::PUT, "/api/v1/rivets/forge/1.0.0/unyank"),
    ] {
        let resp = call(&app, &container, method.clone(), path).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{method} {path}");
    }
}

#[tokio::test]
async fn publish_then_read_back() {
    let rivets = WithRivets::new();
    let (app, container) = setup().await;
    let bytes = rivet("forge", "0.4.0");

    let resp = publish(&app, &container, "forge", "0.4.0", &bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = support::json_body(resp).await;
    assert_eq!(body["name"], "forge");
    assert_eq!(body["version"], "0.4.0");
    assert_eq!(body["filename"], "forge-0.4.0.rivet");
    assert_eq!(body["namespace"], "forge");
    assert_eq!(body["size_bytes"], bytes.len());
    assert_eq!(body["sha256"], rivet_package::sha256_hex(&bytes));
    assert_eq!(body["manifest"]["package"]["name"], "forge");
    assert!(
        rivets
            .dir
            .path()
            .join("forge/0.4.0/forge-0.4.0.rivet")
            .is_file()
    );

    let resp = call(&app, &container, Method::GET, "/api/v1/rivets/forge/0.4.0").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(support::json_body(resp).await["sha256"], body["sha256"]);

    let resp = call(
        &app,
        &container,
        Method::GET,
        "/api/v1/rivets/forge/0.4.0/download",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (parts, body) = resp.into_hyper().into_parts();
    let headers = parts.headers;
    assert_eq!(headers["x-rivet-sha256"], rivet_package::sha256_hex(&bytes));
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .contains("forge-0.4.0.rivet")
    );
    let downloaded = http_body_util::BodyExt::collect(body)
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(downloaded.as_ref(), bytes.as_slice());
}

#[tokio::test]
async fn build_metadata_versions_round_trip_through_the_url() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;
    let bytes = rivet("forge", "0.4.0+b123");

    let resp = publish(&app, &container, "forge", "0.4.0+b123", &bytes).await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = call(
        &app,
        &container,
        Method::GET,
        "/api/v1/rivets/forge/0.4.0+b123/download",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(support::body_bytes(resp).await.as_ref(), bytes.as_slice());
}

#[tokio::test]
async fn republishing_a_version_is_a_conflict_and_changes_nothing() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;
    let original = rivet("forge", "0.4.0");

    assert_eq!(
        publish(&app, &container, "forge", "0.4.0", &original)
            .await
            .status(),
        StatusCode::CREATED
    );

    let mut other = Manifest::new("forge", "0.4.0");
    other.package.description = Some("different".to_string());
    let mut b = PackageBuilder::new(other);
    b.add_file("overlay.yaml", b"env: other\n".to_vec())
        .unwrap();
    let resp = publish(&app, &container, "forge", "0.4.0", &b.build().unwrap()).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let resp = call(
        &app,
        &container,
        Method::GET,
        "/api/v1/rivets/forge/0.4.0/download",
    )
    .await;
    assert_eq!(
        support::body_bytes(resp).await.as_ref(),
        original.as_slice()
    );
}

#[tokio::test]
async fn rejects_an_archive_that_disagrees_with_the_url() {
    let rivets = WithRivets::new();
    let (app, container) = setup().await;

    let resp = publish(&app, &container, "other", "0.4.0", &rivet("forge", "0.4.0")).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let resp = publish(&app, &container, "forge", "0.5.0", &rivet("forge", "0.4.0")).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Nothing was stored, staging files included.
    let leftovers: Vec<_> = walk(rivets.dir.path());
    assert!(leftovers.is_empty(), "{leftovers:?}");

    let resp = call(&app, &container, Method::GET, "/api/v1/rivets").await;
    assert_eq!(support::json_body(resp).await, serde_json::json!([]));
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

#[tokio::test]
async fn rejects_garbage_and_a_corrupted_archive() {
    let rivets = WithRivets::new();
    let (app, container) = setup().await;

    let resp = publish(&app, &container, "forge", "0.4.0", b"not an archive").await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Same archive, one overlay byte flipped inside a re-compressed tar, so SHA256SUMS disagrees.
    let package = rivet_package::Package::read(
        std::io::Cursor::new(rivet("forge", "0.4.0")),
        &rivet_package::Limits::default(),
    )
    .unwrap();
    let mut tampered = package.clone();
    tampered
        .files
        .insert("overlay.yaml".to_string(), b"env: evil\n".to_vec());
    let bytes = tampered.to_bytes().unwrap();
    let resp = publish(&app, &container, "forge", "0.4.0", &bytes).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(support::body_text(resp).await.contains("checksum"));

    assert!(walk(rivets.dir.path()).is_empty());
}

#[tokio::test]
async fn rejects_an_invalid_name_or_version_in_the_url() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;
    let bytes = rivet("forge", "0.4.0");

    for path in ["Forge/0.4.0", "forge/latest", "forge/1.0", "forge/v1"] {
        let resp = app
            .call(support::raw_req(
                Method::PUT,
                &format!("/api/v1/rivets/{path}"),
                &[],
                &bytes,
                &container,
            ))
            .await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY, "{path}");
    }
}

#[tokio::test]
async fn rejects_an_upload_over_the_size_limit() {
    let rivets = WithRivets::new();
    envmnt::set("RIVET_MAX_BYTES", "16");
    let (app, container) = setup().await;

    let resp = publish(&app, &container, "forge", "0.4.0", &rivet("forge", "0.4.0")).await;
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(walk(rivets.dir.path()).is_empty());
}

#[tokio::test]
async fn latest_follows_semver_and_skips_yanked_versions() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;
    for version in ["0.9.0", "0.10.0", "0.2.0"] {
        let resp = publish(&app, &container, "forge", version, &rivet("forge", version)).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let latest = |app: Arc<dyn Endpoint>, container: Arc<Container>| async move {
        let resp = call(&app, &container, Method::GET, "/api/v1/rivets/forge/latest").await;
        (resp.status(), support::json_body(resp).await)
    };

    let (status, body) = latest(app.clone(), container.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], "0.10.0");

    let resp = call(
        &app,
        &container,
        Method::DELETE,
        "/api/v1/rivets/forge/0.10.0/yank",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let (_, body) = latest(app.clone(), container.clone()).await;
    assert_eq!(body["version"], "0.9.0");

    // A yanked version still downloads by exact version.
    let resp = call(
        &app,
        &container,
        Method::GET,
        "/api/v1/rivets/forge/0.10.0/download",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = call(
        &app,
        &container,
        Method::PUT,
        "/api/v1/rivets/forge/0.10.0/unyank",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (_, body) = latest(app, container).await;
    assert_eq!(body["version"], "0.10.0");
}

#[tokio::test]
async fn latest_download_serves_the_resolved_version() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;
    let newest = rivet("forge", "1.2.0");
    publish(&app, &container, "forge", "1.1.0", &rivet("forge", "1.1.0")).await;
    publish(&app, &container, "forge", "1.2.0", &newest).await;

    let resp = call(
        &app,
        &container,
        Method::GET,
        "/api/v1/rivets/forge/latest/download",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(support::body_bytes(resp).await.as_ref(), newest.as_slice());
}

#[tokio::test]
async fn unknown_packages_and_versions_are_not_found() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;

    for (method, path) in [
        (Method::GET, "/api/v1/rivets/ghost/1.0.0"),
        (Method::GET, "/api/v1/rivets/ghost/latest"),
        (Method::GET, "/api/v1/rivets/ghost/1.0.0/download"),
        (Method::DELETE, "/api/v1/rivets/ghost/1.0.0/yank"),
        (Method::PUT, "/api/v1/rivets/ghost/1.0.0/unyank"),
        (Method::GET, "/api/v1/rivets/Bad_Name/1.0.0"),
        (Method::GET, "/api/v1/rivets/forge/not-semver"),
    ] {
        let resp = call(&app, &container, method.clone(), path).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{method} {path}");
    }
}

#[tokio::test]
async fn lists_versions_newest_first_and_a_catalog_of_latest() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;
    for (name, version) in [
        ("forge", "0.2.0"),
        ("forge", "0.10.0"),
        ("forge", "0.9.0"),
        ("vllm", "1.0.0"),
        ("media", "3.1.0"),
    ] {
        publish(&app, &container, name, version, &rivet(name, version)).await;
    }

    let resp = call(&app, &container, Method::GET, "/api/v1/rivets/forge").await;
    let body = support::json_body(resp).await;
    let versions: Vec<_> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["version"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(versions, ["0.10.0", "0.9.0", "0.2.0"]);

    let resp = call(&app, &container, Method::GET, "/api/v1/rivets").await;
    let body = support::json_body(resp).await;
    let catalog: Vec<_> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            format!(
                "{}@{}",
                v["name"].as_str().unwrap(),
                v["version"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(catalog, ["forge@0.10.0", "media@3.1.0", "vllm@1.0.0"]);

    // A package with every version yanked drops out of the catalog but stays listable.
    call(
        &app,
        &container,
        Method::DELETE,
        "/api/v1/rivets/vllm/1.0.0/yank",
    )
    .await;
    let resp = call(&app, &container, Method::GET, "/api/v1/rivets").await;
    assert_eq!(support::json_body(resp).await.as_array().unwrap().len(), 2);
    let resp = call(&app, &container, Method::GET, "/api/v1/rivets/vllm").await;
    assert_eq!(support::json_body(resp).await.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn an_invalid_name_lists_nothing() {
    let _rivets = WithRivets::new();
    let (app, container) = setup().await;
    let resp = call(&app, &container, Method::GET, "/api/v1/rivets/Bad_Name").await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(support::json_body(resp).await, serde_json::json!([]));
}
