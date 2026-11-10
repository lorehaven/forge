//! Share links against a real database: the SQL under them, making and withdrawing them through the management API, and
//! opening them without a session. Skipped unless `WAREHOUSE_TEST_DATABASE_URL` points at a database with the warehouse
//! module installed (`foundry-service apply -i warehouse`).

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]

#[path = "support.rs"]
mod support;

use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::tmp::TempDir;
use quench_db::prelude::{Database, Db};
use quench_http::http::{Method, StatusCode};
use quench_serde::{Value, json};
use warehouse_core::ShareView;
use warehouse_service::domain::storage::{self, NewStorage};
use warehouse_service::domain::{share, storage_file};
use warehouse_service::routers::files::dynamic;

type App = (
    std::sync::Arc<dyn quench_http::endpoint::Endpoint>,
    std::sync::Arc<quench_http::di::Container>,
);

/// A response read whole: status, headers, body.
struct Got {
    status: StatusCode,
    headers: quench_http::http::HeaderMap,
    body: Vec<u8>,
}

impl Got {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn header(&self, name: &str) -> String {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    }
}

async fn read(resp: quench_http::response::Response) -> Got {
    let status = resp.status();
    let parts = resp.into_inner();
    let body = parts
        .body
        .collect_limited(usize::MAX)
        .await
        .expect("body")
        .to_vec();
    Got {
        status,
        headers: parts.headers,
        body,
    }
}

async fn get(app: &App, path: &str) -> Got {
    read(app.0.call(support::req(Method::GET, path, &app.1)).await).await
}

async fn post(app: &App, path: &str, body: &Value) -> Got {
    read(
        app.0
            .call(support::json_req(Method::POST, path, body, &app.1))
            .await,
    )
    .await
}

fn claims(sub: &str, scope: &str) -> Claims {
    Claims::for_audiences(
        sub.to_string(),
        vec!["warehouse".to_string()],
        scope.to_string(),
        None,
        3600,
    )
}

/// The request as `who` makes it; `None` is nobody at all.
async fn call_as(app: &App, mut req: quench_http::request::Request, who: Option<Claims>) -> Got {
    if let Some(who) = who {
        req.extensions_mut().insert(who);
    }
    read(app.0.call(req).await).await
}

async fn get_as(app: &App, path: &str, who: Option<Claims>) -> Got {
    call_as(app, support::req(Method::GET, path, &app.1), who).await
}

async fn post_as(app: &App, path: &str, body: &Value, who: Option<Claims>) -> Got {
    call_as(
        app,
        support::json_req(Method::POST, path, body, &app.1),
        who,
    )
    .await
}

fn view(got: &Got) -> ShareView {
    quench_serde::json::from_slice(&got.body).expect("a share")
}

fn sha_of(content: &[u8]) -> String {
    quench_core::hex::encode(quench_crypto::hash::sha256(content))
}

async fn put(
    db: &Db,
    scratch: &TempDir,
    root: &std::path::Path,
    storage: &str,
    path: &str,
    content: &[u8],
) {
    let sha = sha_of(content);
    let blob = dynamic::blob_path(root, &sha);
    std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
    let staging = scratch
        .path()
        .join(format!("staging-{}", sha_of(path.as_bytes())));
    std::fs::write(&staging, content).unwrap();
    storage_file::put_file(
        db,
        storage,
        path,
        &sha,
        i64::try_from(content.len()).unwrap(),
        &staging,
        &blob,
    )
    .await
    .expect("put");
}

#[quench_test::test]
async fn links_are_made_opened_and_withdrawn() {
    let Ok(url) = std::env::var("WAREHOUSE_TEST_DATABASE_URL") else {
        return;
    };
    let db = Db::connect(&url).await.expect("connect");
    // `dev` owns the storage; `admin` is who the management API takes the caller for with auth switched off, and a share
    // names its creator.
    for user in ["dev", "admin", "eve", "wr", "sc"] {
        db.execute(&format!("INSERT INTO auth.users (username, password, roles) VALUES ('{user}', 'x', '[]'::jsonb) ON CONFLICT DO NOTHING"))
            .await
            .expect("seed the users");
    }

    // Read once for the whole binary, so set before anything asks.
    let blobs = TempDir::new().expect("tempdir");
    let scratch = TempDir::new().expect("tempdir");
    let fixed = TempDir::new().expect("tempdir");
    quench_core::env::set("FEATURE_FILES_ENABLED", "true");
    quench_core::env::set("DYNAMIC_STORAGE_ROOT", blobs.path().to_str().unwrap());
    quench_core::env::set(
        "FILE_STORAGES",
        format!("fixed={}", fixed.path().to_str().unwrap()),
    );
    std::fs::create_dir_all(fixed.path().join("pub/inner")).unwrap();
    std::fs::write(fixed.path().join("pub/note.txt"), b"fixed note").unwrap();
    std::fs::write(fixed.path().join("pub/inner/deep.txt"), b"deep").unwrap();
    std::fs::write(fixed.path().join("private.txt"), b"keep out").unwrap();

    warehouse_service::routers::register_routes();
    let mut config = JwtConfig::for_tests();
    config.auth_enabled = false;
    let container = support::container_builder()
        .provide(config)
        .provide(db.clone())
        .build()
        .await
        .unwrap();
    let app = support::app(container);

    // A dynamic storage with a few levels, a look-alike sibling and names LIKE would trip over.
    let name = format!("t{}", quench_core::uuid::Uuid::new_v4().simple());
    storage::create(
        &db,
        &NewStorage {
            name: name.clone(),
            owner: "dev".into(),
            max_file_bytes: None,
            quota_bytes: 1_000_000,
            sync_enabled: false,
        },
    )
    .await
    .expect("create");
    // Blobs are shared by content, so every run writes content of its own: a repeat run against the same database must not
    // find a blob a deleted temp directory used to hold.
    let run = quench_core::uuid::Uuid::new_v4().simple().to_string();
    let text = |s: &str| format!("{s}-{run}");
    for (path, content) in [
        ("pics/a.png", text("PNG-A")),
        ("pics/2026/b.txt", text("B")),
        ("pics/2026/deep/c.txt", text("see")),
        ("pics_other/y.txt", text("other")),
        ("odd_%name/x.txt", text("odd")),
        ("top.txt", text("top")),
    ] {
        put(&db, &scratch, blobs.path(), &name, path, content.as_bytes()).await;
    }
    let size = |s: &str| i64::try_from(text(s).len()).unwrap();

    // ── The directory aggregate ──
    let names = |children: Vec<storage_file::Child>| -> Vec<(String, bool, i64)> {
        children
            .into_iter()
            .map(|c| (c.name, c.is_dir, c.size))
            .collect()
    };
    assert_eq!(
        names(storage_file::children(&db, &name, "", 100).await.unwrap()),
        [
            ("odd_%name".to_string(), true, 0),
            ("pics".to_string(), true, 0),
            ("pics_other".to_string(), true, 0),
            ("top.txt".to_string(), false, size("top")),
        ]
    );
    assert_eq!(
        names(
            storage_file::children(&db, &name, "pics", 100)
                .await
                .unwrap()
        ),
        [
            ("2026".to_string(), true, 0),
            ("a.png".to_string(), false, size("PNG-A"))
        ]
    );
    assert_eq!(
        names(
            storage_file::children(&db, &name, "pics/2026", 100)
                .await
                .unwrap()
        ),
        [
            ("deep".to_string(), true, 0),
            ("b.txt".to_string(), false, size("B"))
        ]
    );
    // `_` and `%` are names here, and a prefix of a name is not a directory.
    assert_eq!(
        names(
            storage_file::children(&db, &name, "odd_%name", 100)
                .await
                .unwrap()
        ),
        [("x.txt".to_string(), false, size("odd"))]
    );
    assert!(
        storage_file::children(&db, &name, "pic", 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        storage_file::children(&db, &name, "", 2)
            .await
            .unwrap()
            .len(),
        2,
        "limited"
    );

    // ── Making links ──
    let dir = post(
        &app,
        "/ui/api/files/shares",
        &json!({"storage": name, "path": "pics"}),
    )
    .await;
    assert_eq!(dir.status, StatusCode::OK, "{}", dir.text());
    let dir = view(&dir);
    assert!(dir.is_dir && dir.token.len() == 43, "{dir:?}");
    // No address is configured and the test request names no host: the link is the part after the host.
    assert_eq!(dir.url, format!("/share/{}", dir.token));
    // Asking again gives the same link.
    let again = view(
        &post(
            &app,
            "/ui/api/files/shares",
            &json!({"storage": name, "path": "pics/"}),
        )
        .await,
    );
    assert_eq!(
        (again.id.as_str(), again.token.as_str()),
        (dir.id.as_str(), dir.token.as_str())
    );

    let file = view(
        &post(
            &app,
            "/ui/api/files/shares",
            &json!({"storage": name, "path": "top.txt"}),
        )
        .await,
    );
    assert!(!file.is_dir);
    assert_ne!(file.token, dir.token);

    let storage_wide = view(
        &post(
            &app,
            "/ui/api/files/shares",
            &json!({"storage": name, "path": ""}),
        )
        .await,
    );
    assert!(storage_wide.is_dir && storage_wide.path.is_empty());

    for (body, status, key) in [
        (
            json!({"storage": name, "path": "nope"}),
            StatusCode::NOT_FOUND,
            "api_error_file_not_found",
        ),
        (
            json!({"storage": name, "path": "pic"}),
            StatusCode::NOT_FOUND,
            "api_error_file_not_found",
        ),
        (
            json!({"storage": name, "path": "../x"}),
            StatusCode::BAD_REQUEST,
            "api_error_invalid_path",
        ),
        (
            json!({"storage": "no_such", "path": "a"}),
            StatusCode::NOT_FOUND,
            "api_error_share_storage_missing",
        ),
    ] {
        let got = post(&app, "/ui/api/files/shares", &body).await;
        assert_eq!(got.status, status, "{body:?}");
        assert!(got.text().contains(key), "{body:?}: {}", got.text());
    }

    // ── Opening a directory link: no session, everything below ──
    let open = |token: &str, query: &str| format!("/share/{token}{query}");
    let top = get(&app, &open(&dir.token, "")).await;
    assert_eq!(top.status, StatusCode::OK);
    let page = top.text();
    assert!(page.contains("2026/") && page.contains("a.png"), "{page}");
    assert!(
        !page.contains("top.txt") && !page.contains("pics_other"),
        "outside the share: {page}"
    );
    assert!(top.header("content-type").starts_with("text/html"));
    assert!(
        top.header("content-security-policy")
            .contains("default-src 'none'")
    );
    assert_eq!(top.header("x-content-type-options"), "nosniff");
    assert_eq!(top.header("referrer-policy"), "no-referrer");

    let mid = get(&app, &open(&dir.token, "?path=2026")).await;
    assert!(
        mid.text().contains("deep/") && mid.text().contains("b.txt"),
        "{}",
        mid.text()
    );
    let deepest = get(&app, &open(&dir.token, "?path=2026/deep")).await;
    assert!(deepest.text().contains("c.txt"), "{}", deepest.text());

    let b = get(&app, &open(&dir.token, "?path=2026/b.txt")).await;
    assert_eq!((b.status, b.text()), (StatusCode::OK, text("B")));
    assert!(b.header("content-disposition").starts_with("attachment"));
    let c = get(&app, &open(&dir.token, "?path=2026/deep/c.txt")).await;
    assert_eq!(c.text(), text("see"));

    // Viewing in place only for kinds that cannot run script.
    let png = get(&app, &open(&dir.token, "?path=a.png&inline=1")).await;
    assert_eq!(png.header("content-type"), "image/png");
    assert!(png.header("content-disposition").starts_with("inline"));
    let inline_text = get(&app, &open(&dir.token, "?path=2026/b.txt&inline=1")).await;
    assert_eq!(inline_text.header("content-type"), "text/plain");

    // Nothing outside it, nothing that is not there; the answer is the same for all of them.
    let nothing = get(&app, &open("no-such-token", "")).await;
    assert_eq!(nothing.status, StatusCode::NOT_FOUND);
    for query in [
        "?path=../top.txt",
        "?path=../pics_other/y.txt",
        "?path=/etc/passwd",
        "?path=2026/missing",
        "?path=nope",
        "?path=2026/%00",
    ] {
        let got = get(&app, &open(&dir.token, query)).await;
        assert_eq!(got.status, StatusCode::NOT_FOUND, "{query}");
        assert_eq!(got.text(), nothing.text(), "{query}");
    }

    // ── A file link: just the file ──
    let shared_file = get(&app, &open(&file.token, "")).await;
    assert_eq!(
        (shared_file.status, shared_file.text()),
        (StatusCode::OK, text("top"))
    );
    assert_eq!(
        get(&app, &open(&file.token, "?path=x")).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, &open(&file.token, "?path=top.txt")).await.status,
        StatusCode::NOT_FOUND
    );

    // ── The whole storage ──
    let everything = get(&app, &open(&storage_wide.token, "")).await.text();
    assert!(
        everything.contains("pics/")
            && everything.contains("top.txt")
            && everything.contains("odd_%name/"),
        "{everything}"
    );

    // ── A fixed storage ──
    let pub_dir = view(
        &post(
            &app,
            "/ui/api/files/shares",
            &json!({"storage": "fixed", "path": "pub"}),
        )
        .await,
    );
    assert!(pub_dir.is_dir);
    let listing = get(&app, &open(&pub_dir.token, "")).await.text();
    assert!(
        listing.contains("note.txt") && listing.contains("inner/"),
        "{listing}"
    );
    assert!(!listing.contains("private.txt"));
    assert_eq!(
        get(&app, &open(&pub_dir.token, "?path=inner/deep.txt"))
            .await
            .body,
        b"deep"
    );
    assert_eq!(
        get(&app, &open(&pub_dir.token, "?path=../private.txt"))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let fixed_file = view(
        &post(
            &app,
            "/ui/api/files/shares",
            &json!({"storage": "fixed", "path": "pub/note.txt"}),
        )
        .await,
    );
    assert_eq!(
        get(&app, &open(&fixed_file.token, "")).await.body,
        b"fixed note"
    );
    let missing = post(
        &app,
        "/ui/api/files/shares",
        &json!({"storage": "fixed", "path": "nothing"}),
    )
    .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);

    // ── Listing and withdrawing ──
    let listed = get(&app, "/ui/api/files/shares").await;
    let all: Vec<ShareView> = quench_serde::json::from_slice(&listed.body).unwrap();
    assert!(all.iter().any(|s| s.id == dir.id) && all.iter().any(|s| s.id == pub_dir.id));
    let only: Vec<ShareView> = quench_serde::json::from_slice(
        &get(&app, &format!("/ui/api/files/shares?storage={name}"))
            .await
            .body,
    )
    .unwrap();
    assert!(
        only.iter().all(|s| s.storage == name) && only.len() == 3,
        "{only:?}"
    );

    let gone = post(
        &app,
        &format!("/ui/api/files/shares/{}/delete", dir.id),
        &json!({}),
    )
    .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    assert_eq!(
        get(&app, &open(&dir.token, "")).await.status,
        StatusCode::NOT_FOUND,
        "withdrawn at once"
    );
    assert_eq!(
        post(
            &app,
            &format!("/ui/api/files/shares/{}/delete", dir.id),
            &json!({})
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    // Another link of the same storage is untouched.
    assert_eq!(
        get(&app, &open(&file.token, "")).await.status,
        StatusCode::OK
    );

    // ── Removing the storage removes its links ──
    assert!(share::by_token(&db, &file.token).await.unwrap().is_some());
    storage::delete(&db, &name).await.expect("delete storage");
    assert!(share::by_token(&db, &file.token).await.unwrap().is_none());
    assert!(
        share::by_token(&db, &storage_wide.token)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        get(&app, &open(&file.token, "")).await.status,
        StatusCode::NOT_FOUND
    );
    // The fixed storage's links are not touched by that.
    assert_eq!(
        get(&app, &open(&pub_dir.token, "")).await.status,
        StatusCode::OK
    );

    // Tidy up the fixed storage's links this run made.
    for token in [&pub_dir.token, &fixed_file.token] {
        let found = share::by_token(&db, token).await.unwrap().unwrap();
        share::delete(&db, &found.id).await.unwrap();
    }

    // ── Who may do what, with sign-in required ──
    let mut strict = JwtConfig::for_tests();
    strict.auth_enabled = true;
    let container = support::container_builder()
        .provide(strict)
        .provide(db.clone())
        .build()
        .await
        .unwrap();
    let app = support::app(container);
    let mine = format!("t{}", quench_core::uuid::Uuid::new_v4().simple());
    storage::create(
        &db,
        &NewStorage {
            name: mine.clone(),
            owner: "dev".into(),
            max_file_bytes: None,
            quota_bytes: 1_000_000,
            sync_enabled: false,
        },
    )
    .await
    .expect("create");
    put(
        &db,
        &scratch,
        blobs.path(),
        &mine,
        "docs/plan.txt",
        text("plan").as_bytes(),
    )
    .await;

    let owner = || Some(claims("dev", "user"));
    let reader = || Some(claims("eve", "user warehouse:read"));
    let writer = || Some(claims("wr", "user warehouse:write"));
    let scoped_here = || {
        Some(claims(
            "sc",
            &format!("user warehouse:storage:{mine}:write"),
        ))
    };
    let scoped_fixed = || Some(claims("sc", "user warehouse:storage:fixed:write"));
    let scoped_other = || Some(claims("sc", "user warehouse:storage:elsewhere:write"));
    let make = |storage: &str, path: &str| json!({"storage": storage, "path": path});
    let url = "/ui/api/files/shares";

    // No session: nothing but the sign-in answer.
    assert_eq!(
        post_as(&app, url, &make(&mine, "docs"), None).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get_as(&app, url, None).await.status,
        StatusCode::UNAUTHORIZED
    );

    // A database storage: its owner, or a grant for it. The blanket grants do not reach into someone's storage.
    let by_owner = post_as(&app, url, &make(&mine, "docs"), owner()).await;
    assert_eq!(by_owner.status, StatusCode::OK, "{}", by_owner.text());
    let by_owner = view(&by_owner);
    for (who, label) in [
        (reader(), "reader"),
        (writer(), "blanket writer"),
        (scoped_other(), "grant for another storage"),
    ] {
        let got = post_as(&app, url, &make(&mine, "docs"), who).await;
        assert_eq!(got.status, StatusCode::FORBIDDEN, "{label}: {}", got.text());
    }
    let by_grant = post_as(&app, url, &make(&mine, "docs/plan.txt"), scoped_here()).await;
    assert_eq!(by_grant.status, StatusCode::OK, "{}", by_grant.text());
    let by_grant = view(&by_grant);
    assert_eq!(by_grant.created_by, "sc");

    // A fixed storage: the blanket write, or a grant for it; reading is not publishing.
    for (who, label, allowed) in [
        (reader(), "reader", false),
        (writer(), "blanket writer", true),
        (scoped_fixed(), "grant for it", true),
        (scoped_other(), "grant for another", false),
    ] {
        let got = post_as(&app, url, &make("fixed", "pub"), who).await;
        let want = if allowed {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        };
        assert_eq!(got.status, want, "{label}: {}", got.text());
    }

    // Opening a link needs no session at all, with sign-in required everywhere else.
    let open_plain = get(&app, &format!("/share/{}", by_owner.token)).await;
    assert_eq!(open_plain.status, StatusCode::OK);
    assert!(open_plain.text().contains("plan.txt"));

    // Seeing: your own; everyone's for whoever may write everywhere.
    let ids = |got: &Got| -> Vec<String> {
        quench_serde::json::from_slice::<Vec<ShareView>>(&got.body)
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect()
    };
    let eve_sees = ids(&get_as(&app, url, reader()).await);
    assert!(eve_sees.is_empty(), "{eve_sees:?}");
    let owner_sees = ids(&get_as(&app, url, owner()).await);
    assert!(
        owner_sees.contains(&by_owner.id) && !owner_sees.contains(&by_grant.id),
        "{owner_sees:?}"
    );
    let writer_sees = ids(&get_as(&app, url, writer()).await);
    assert!(
        writer_sees.contains(&by_owner.id) && writer_sees.contains(&by_grant.id),
        "{writer_sees:?}"
    );

    // Withdrawing: the creator, or whoever may write everywhere; nobody else.
    let delete_url = |id: &str| format!("{url}/{id}/delete");
    assert_eq!(
        post_as(&app, &delete_url(&by_owner.id), &json!({}), reader())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post_as(&app, &delete_url(&by_owner.id), &json!({}), scoped_here())
            .await
            .status,
        StatusCode::FORBIDDEN,
        "a grant for the storage does not withdraw someone else's link"
    );
    assert_eq!(
        post_as(&app, &delete_url(&by_owner.id), &json!({}), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(&app, &format!("/share/{}", by_owner.token))
            .await
            .status,
        StatusCode::OK,
        "still open"
    );
    assert_eq!(
        post_as(&app, &delete_url(&by_grant.id), &json!({}), scoped_here())
            .await
            .status,
        StatusCode::NO_CONTENT,
        "the creator"
    );
    assert_eq!(
        post_as(&app, &delete_url(&by_owner.id), &json!({}), writer())
            .await
            .status,
        StatusCode::NO_CONTENT,
        "write everywhere"
    );
    assert_eq!(
        get(&app, &format!("/share/{}", by_owner.token))
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // Tidy up: this run's fixed-storage links and storage.
    for found in share::list(&db, None).await.unwrap() {
        if found.storage == "fixed" {
            share::delete(&db, &found.id).await.unwrap();
        }
    }
    storage::delete(&db, &mine).await.expect("delete storage");
}

quench_test::main!();
