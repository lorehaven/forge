//! The catalogue against Postgres: the store, the refresh and the API. Set `HOIST_TEST_DATABASE_URL` to a throwaway
//! database with the `hoist` module installed (`foreman db` makes one) to run them; without it they pass without
//! checking anything.
// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::unwrap_used, clippy::too_many_lines)]

use std::sync::{Arc, OnceLock};

use hoist_service::models::MergedPackage;
use hoist_service::refresh::{Fetched, apply};
use hoist_service::routers;
use hoist_service::store::{self, Filter, Sort};
use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_db::prelude::{Database, Db};
use quench_http::http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::prelude::{Container, ContainerBuilder, Endpoint, Request};
use quench_serde::{Value, json};
use tokio::sync::{Mutex, MutexGuard};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// A connection to the test database with an empty catalogue, and the lock that keeps tests from sharing it.
async fn database() -> Option<(Db, MutexGuard<'static, ()>)> {
    let url = std::env::var("HOIST_TEST_DATABASE_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    let guard = lock().lock().await;
    let db = Db::connect(&url)
        .await
        .expect("connect to the test database");
    db.execute("TRUNCATE hoist.packages").await.expect("clear");
    Some((db, guard))
}

fn package(name: &str, label: &str) -> MergedPackage {
    MergedPackage {
        package_name: name.to_string(),
        label: label.to_string(),
        version_code: 1,
        version_name: "1.0".to_string(),
        size_bytes: 1000,
        download_url: "https://example.test/a.apk".to_string(),
        install_source: "fdroid".to_string(),
        ..Default::default()
    }
}

fn in_category(name: &str, category: Option<&str>, votes: Option<i64>) -> MergedPackage {
    MergedPackage {
        category: category.map(String::from),
        rating_count: votes,
        ..package(name, name)
    }
}

async fn search(db: &Db, filter: &Filter<'_>, sort: Sort) -> (Vec<String>, i64) {
    let (rows, total) = store::search(db, filter, sort, 0, 50).await.unwrap();
    (rows.into_iter().map(|r| r.package_name).collect(), total)
}

#[quench_test::test]
async fn what_is_stored_is_read_back_and_a_second_write_replaces_it() {
    let Some((db, _guard)) = database().await else {
        return;
    };
    let rich = MergedPackage {
        summary: Some("s".into()),
        rating_avg: Some(4.5),
        rating_count: Some(12),
        downloads: Some(99),
        malware_rank: Some("TRUSTED".into()),
        sha256: Some("abc".into()),
        ..package("com.example.app", "Example")
    };
    store::replace_all(&db, &[rich, package("com.example.old", "Old")])
        .await
        .unwrap();
    let got = store::get(&db, "com.example.app").await.unwrap().unwrap();
    assert_eq!(
        (
            got.label.as_str(),
            got.rating_avg,
            got.rating_count,
            got.downloads
        ),
        ("Example", Some(4.5), Some(12), Some(99))
    );
    assert_eq!(got.malware_rank.as_deref(), Some("TRUSTED"));
    assert!(store::get(&db, "missing").await.unwrap().is_none());
    // The whole catalogue is replaced, not added to.
    store::replace_all(&db, &[package("com.example.new", "New")])
        .await
        .unwrap();
    assert!(store::get(&db, "com.example.old").await.unwrap().is_none());
    assert!(store::get(&db, "com.example.new").await.unwrap().is_some());
    // More rows than one insert holds.
    let many: Vec<MergedPackage> = (0..450)
        .map(|i| package(&format!("p.n{i}"), &format!("N{i}")))
        .collect();
    store::replace_all(&db, &many).await.unwrap();
    assert_eq!(search(&db, &Filter::default(), Sort::Alpha).await.1, 450);
}

#[quench_test::test]
async fn search_filters_by_text_category_and_votes_and_sorts_what_has_no_figures_last() {
    let Some((db, _guard)) = database().await else {
        return;
    };
    let mut chess = in_category("org.x.chess", Some("Games"), Some(300));
    chess.label = "Chess Club".into();
    chess.downloads = Some(10);
    chess.rating_avg = Some(4.1);
    let mut go = in_category("org.x.go", Some("Games"), Some(5));
    go.label = "go board".into();
    go.downloads = Some(500);
    go.rating_avg = Some(5.0);
    let mut notes = in_category("org.x.notes", Some("Tools"), None);
    notes.label = "Notes_100%".into();
    store::replace_all(
        &db,
        &[chess, go, notes, in_category("org.x.plain", None, None)],
    )
    .await
    .unwrap();

    let all = Filter::default();
    // Text: any case, in the label or the package; % and _ are letters.
    let q = |text| Filter {
        query: Some(text),
        ..Filter::default()
    };
    assert_eq!(
        search(&db, &q("CHESS"), Sort::Alpha).await.0,
        ["org.x.chess"]
    );
    assert_eq!(
        search(&db, &q("org.x.go"), Sort::Alpha).await.0,
        ["org.x.go"]
    );
    assert_eq!(
        search(&db, &q("100%"), Sort::Alpha).await.0,
        ["org.x.notes"]
    );
    assert_eq!(
        search(&db, &q("_"), Sort::Alpha).await.0,
        ["org.x.notes"],
        "an underscore is not a wildcard"
    );
    // Category and votes.
    let games = Filter {
        category: Some("Games"),
        ..Filter::default()
    };
    assert_eq!(search(&db, &games, Sort::Alpha).await.1, 2);
    let rated = Filter {
        category: Some("Games"),
        min_votes: Some(50),
        ..Filter::default()
    };
    assert_eq!(search(&db, &rated, Sort::Rating).await.0, ["org.x.chess"]);
    // Sorts: what has no figures goes last, and alpha ignores case.
    assert_eq!(
        search(&db, &all, Sort::Downloads).await.0[..2],
        ["org.x.go", "org.x.chess"]
    );
    assert_eq!(
        search(&db, &all, Sort::Rating).await.0[..2],
        ["org.x.go", "org.x.chess"]
    );
    assert_eq!(
        search(&db, &all, Sort::Alpha).await.0[0],
        "org.x.chess",
        "Chess Club, go board, Notes_100%, org.x.plain"
    );
    // A page.
    let (rows, total) = store::search(&db, &all, Sort::Alpha, 3, 10).await.unwrap();
    assert_eq!((rows.len(), total), (1, 4));
    // Categories: counted, biggest first, uncategorised left out.
    assert_eq!(
        store::categories(&db).await.unwrap(),
        [("Games".to_string(), 2), ("Tools".to_string(), 1)]
    );
}

#[quench_test::test]
async fn a_refresh_merges_the_sources_that_answered_and_keeps_the_old_catalogue_when_none_did() {
    let Some((db, _guard)) = database().await else {
        return;
    };
    let listing = |source: &str| hoist_service::models::RawListing {
        source: source.to_string(),
        package_name: "com.example.app".to_string(),
        label: "Example".to_string(),
        version_code: 1,
        version_name: "1.0".to_string(),
        size_bytes: 1,
        download_url: Some("https://example.test/a.apk".to_string()),
        sha256: Some("a".to_string()),
        ..Default::default()
    };
    let fetched: Fetched = vec![
        ("fdroid".into(), Ok(vec![listing("fdroid")])),
        ("aptoide".into(), Err("down".into())),
    ];
    assert_eq!(apply(&db, fetched).await.unwrap(), 1);
    assert_eq!(
        store::get(&db, "com.example.app")
            .await
            .unwrap()
            .unwrap()
            .install_source,
        "fdroid"
    );
    // Every source failed: nothing is written, last time's data stays.
    let none: Fetched = vec![
        ("fdroid".into(), Err("down".into())),
        ("aptoide".into(), Err("down".into())),
    ];
    assert_eq!(apply(&db, none).await.unwrap(), 0);
    assert!(store::get(&db, "com.example.app").await.unwrap().is_some());
}

async fn app(db: Db) -> (Arc<dyn Endpoint>, Arc<Container>) {
    routers::register_routes();
    let container = ContainerBuilder::new()
        .provide(db)
        .provide(JwtConfig::for_tests())
        .build()
        .await
        .expect("container");
    (
        quench_starter::http::discover_and_mount("/"),
        Arc::new(container),
    )
}

async fn get(
    app: &Arc<dyn Endpoint>,
    container: &Arc<Container>,
    path: &str,
) -> (StatusCode, Value) {
    let request = Request::new(
        Method::GET,
        path.parse::<Uri>().expect("valid URI"),
        HeaderMap::new(),
        quench_http::body::InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    );
    let response = app.call(request).await;
    let status = response.status();
    let bytes = response
        .into_inner()
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body");
    (
        status,
        quench_serde::json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[quench_test::test]
async fn the_api_lists_filters_and_describes_the_catalogue() {
    let Some((db, _guard)) = database().await else {
        return;
    };
    let mut chess = in_category("org.x.chess", Some("Games"), Some(300));
    chess.rating_avg = Some(4.5);
    store::replace_all(
        &db,
        &[chess, in_category("org.x.notes", Some("Tools"), None)],
    )
    .await
    .unwrap();
    let (app, container) = app(db).await;

    let (status, body) = get(
        &app,
        &container,
        "/v1/apps?category=Games&sort=rating&min_votes=20&limit=5",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (body["total"].as_i64(), body["limit"].as_i64()),
        (Some(1), Some(5))
    );
    assert_eq!(
        body["items"][0]["package_name"].as_str(),
        Some("org.x.chess")
    );
    assert_eq!(body["items"][0]["rating_avg"].as_f64(), Some(4.5));
    assert!(body["items"][0]["sha256"].is_null());

    let (_, body) = get(&app, &container, "/v1/apps?query=notes").await;
    assert_eq!(body["total"].as_i64(), Some(1));
    let (status, _) = get(&app, &container, "/v1/apps?sort=bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = get(&app, &container, "/v1/categories").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"][0]["name"].as_str(), Some("Games"));
    assert_eq!(body["items"][0]["count"].as_i64(), Some(1));

    let (status, body) = get(&app, &container, "/v1/apps/org.x.chess").await;
    assert_eq!(
        (status, body["label"].as_str()),
        (StatusCode::OK, Some("org.x.chess"))
    );
    let (status, _) = get(&app, &container, "/v1/apps/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = json!({});
}

/// Against the real stores, when asked (`HOIST_LIVE_SOURCES=1`): the three upstreams fetched, merged and stored.
#[quench_test::test]
async fn the_real_stores_can_be_fetched_merged_and_stored() {
    if std::env::var("HOIST_LIVE_SOURCES").is_err() {
        return;
    }
    let Some((db, _guard)) = database().await else {
        return;
    };
    let mut settings = hoist_service::config::Settings::from_env();
    settings.aptoide_pages = 2;
    let fetched =
        hoist_service::refresh::fetch_all(hoist_service::connectors::Source::all(&settings)).await;
    for (name, result) in &fetched {
        match result {
            Ok(listings) => println!("{name}: {} listings", listings.len()),
            Err(why) => println!("{name}: FAILED {why}"),
        }
    }
    let count = apply(&db, fetched).await.unwrap();
    println!("stored {count} packages");
    assert!(count > 1000, "{count}");
    let categories = store::categories(&db).await.unwrap();
    println!("categories: {:?}", &categories[..categories.len().min(8)]);
    assert!(!categories.is_empty());
}

quench_test::main!();
