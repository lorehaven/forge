//! The HTTP API through the whole stack: the realm's authentication (real signed tokens), the same-origin check, the gate
//! and the handlers, over real Postgres and the fake mail servers.

use crate::support::*;
use mailroom_core::admin::{Identity, Page, Settings, SettingsInput, Stats, TransportInput};
use mailroom_core::{
    Account, Change, Compose, Folder, MessageList, MessageView, Queued, SendRequest, TestResult,
    Upload,
};
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_core::bytes::Bytes;
use quench_http::di::ContainerBuilder;
use quench_http::endpoint::Endpoint;
use quench_http::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri};
use quench_http::request::Request;
use quench_serde::{Deserialize, Serialize};
use std::sync::{Arc, Once};

macro_rules! db {
    () => {
        if !have_database() {
            return;
        }
    };
}

static ROUTES: Once = Once::new();

struct Api {
    endpoint: Arc<dyn Endpoint>,
    container: Arc<quench_http::di::Container>,
    jwt: JwtConfig,
    env: Env,
}

#[derive(Debug)]
struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Reply {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    fn json<T: Deserialize>(&self) -> T {
        quench_serde::json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("{e}: {} {}", self.status, self.text()))
    }

    fn header(&self, name: &str) -> String {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default()
    }
}

async fn api() -> Api {
    let env = env().await;
    env.with_transport().await;
    ROUTES.call_once(|| {
        mailroom_service::routers::api::register_routes();
        mailroom_service::routers::ui::register_routes();
    });
    let mut jwt = JwtConfig::for_tests_with_signing();
    jwt.service_name = "mailroom".into();
    jwt.auth_enabled = true;
    let container = ContainerBuilder::new()
        .provide(quench_http::extract::BodyLimit(
            mailroom_service::outgoing::MAX_UPLOAD + 1024 * 1024,
        ))
        .provide(env.app.clone())
        .provide(jwt.clone())
        .build()
        .await
        .unwrap();
    let inner = quench_starter::http::discover_and_mount("/mailroom");
    let endpoint =
        mailroom_service::routers::api::wrap_auth(inner, jwt.clone(), "/mailroom", vec![]);
    Api {
        endpoint,
        container: Arc::new(container),
        jwt,
        env,
    }
}

impl Api {
    async fn token(&self, user: &str, scope: &str) -> String {
        let claims = Claims::for_audiences(
            user.into(),
            vec!["mailroom".into()],
            scope.into(),
            None,
            900,
        );
        self.jwt.encode_claims(&claims).await.unwrap()
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Vec<u8>>,
        extra: &[(&str, &str)],
    ) -> Reply {
        let mut headers = HeaderMap::new();
        if let Some(token) = token {
            headers.insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
            );
        }
        for (k, v) in extra {
            headers.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        if body.is_some()
            && !extra
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        {
            headers.insert("content-type", HeaderValue::from_static("application/json"));
        }
        let request = Request::new(
            method,
            format!("/mailroom{path}").parse::<Uri>().unwrap(),
            headers,
            quench_http::body::InboundBody::from_bytes(Bytes::from(body.unwrap_or_default())),
            self.container.clone(),
        );
        let response = self.endpoint.call(request).await.into_inner();
        let status = response.status;
        let headers = response.headers.clone();
        let body = response
            .body
            .collect_limited(usize::MAX)
            .await
            .unwrap()
            .to_vec();
        Reply {
            status,
            headers,
            body,
        }
    }

    async fn get(&self, path: &str, token: &str) -> Reply {
        self.call(Method::GET, path, Some(token), None, &[]).await
    }

    async fn send_json<T: Serialize>(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: &T,
    ) -> Reply {
        self.call(
            method,
            path,
            Some(token),
            Some(quench_serde::json::to_string(body).into_bytes()),
            &[],
        )
        .await
    }
}

#[quench_test::test]
async fn nothing_works_without_signing_in() {
    db!();
    let api = api().await;
    for (method, path) in [
        (Method::GET, "/api/v1/me"),
        (Method::GET, "/api/v1/accounts"),
        (Method::POST, "/api/v1/outgoing"),
        (Method::POST, "/api/v1/send"),
        (Method::GET, "/api/v1/admin/settings"),
        (Method::GET, "/api/v1/unmapped/path"),
    ] {
        let r = api.call(method.clone(), path, None, None, &[]).await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED, "{method} {path}");
    }
    let forged = api
        .call(Method::GET, "/api/v1/me", Some("not.a.token"), None, &[])
        .await;
    assert_eq!(forged.status, StatusCode::UNAUTHORIZED);
}

#[quench_test::test]
async fn each_grant_opens_only_its_own_doors() {
    db!();
    let api = api().await;
    let reader = api.token("rita", "mailroom:read").await;
    let writer = api.token("walt", "mailroom:write").await;
    let admin = api.token("adam", "mailroom:admin").await;
    let sender = api.token("gatehouse", "mailroom:send").await;
    let stranger = api.token("sam", "sage:write").await;

    assert_eq!(
        api.get("/api/v1/accounts", &reader).await.status,
        StatusCode::OK
    );
    assert_eq!(
        api.get("/api/v1/accounts", &stranger).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api.get("/api/v1/accounts", &sender).await.status,
        StatusCode::FORBIDDEN
    );
    // A reader reads; writing is for a writer.
    let input = api.env.input();
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/accounts", &reader, &input)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/accounts", &writer, &input)
            .await
            .status,
        StatusCode::CREATED
    );
    // The panel is for administrators only.
    for token in [&reader, &writer, &stranger, &sender] {
        assert_eq!(
            api.get("/api/v1/admin/stats", token).await.status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            api.get("/api/v1/admin/settings", token).await.status,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        api.get("/api/v1/admin/stats", &admin).await.status,
        StatusCode::OK
    );
    // The estate's send endpoint is for machines holding `send`; people, administrators included, do not.
    let mail = SendRequest {
        to: vec!["a@example.org".into()],
        subject: "s".into(),
        text: "t".into(),
        ..SendRequest::default()
    };
    for token in [&reader, &writer, &admin, &stranger] {
        assert_eq!(
            api.send_json(Method::POST, "/api/v1/send", token, &mail)
                .await
                .status,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/send", &sender, &mail)
            .await
            .status,
        StatusCode::ACCEPTED
    );
    // A service's own credential (the wildcard `service` role) sends mail, and opens nothing else.
    let machine = api.token("conveyor-mailroom", "service").await;
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/send", &machine, &mail)
            .await
            .status,
        StatusCode::ACCEPTED
    );
    for path in [
        "/api/v1/accounts",
        "/api/v1/messages?role=inbox",
        "/api/v1/admin/settings",
        "/api/v1/admin/stats",
        "/api/v1/me",
    ] {
        assert_eq!(
            api.get(path, &machine).await.status,
            StatusCode::FORBIDDEN,
            "{path}"
        );
    }
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/accounts", &machine, &api.env.input())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[quench_test::test]
async fn me_says_what_the_person_may_do() {
    db!();
    let api = api().await;
    let me: mailroom_core::Me = api
        .get("/api/v1/me", &api.token("ann", "mailroom:write").await)
        .await
        .json();
    assert_eq!(
        (me.username.as_str(), me.can_write, me.admin),
        ("ann", true, false)
    );
    let me: mailroom_core::Me = api
        .get("/api/v1/me", &api.token("rita", "mailroom:read").await)
        .await
        .json();
    assert!(!me.can_write && !me.admin);
    let me: mailroom_core::Me = api
        .get("/api/v1/me", &api.token("adam", "mailroom:admin").await)
        .await
        .json();
    assert!(me.admin && me.can_write);
    let presets: Vec<mailroom_core::Preset> = api
        .get("/api/v1/presets", &api.token("ann", "mailroom:read").await)
        .await
        .json();
    assert!(presets.iter().any(|p| p.id == "gmail"));
}

#[quench_test::test]
async fn a_person_reads_their_mail_end_to_end() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:write").await;
    api.env.deliver("INBOX", "api test", "hello over http");

    let created = api
        .send_json(Method::POST, "/api/v1/accounts", &token, &api.env.input())
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    assert!(!created.text().contains(PASSWORD));
    let account: Account = created.json();
    mailroom_service::sync::sync_account(&api.env.app, &account.id)
        .await
        .unwrap();

    let accounts: Vec<Account> = api.get("/api/v1/accounts", &token).await.json();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].unread, 1);
    let folders: Vec<Folder> = api
        .get(&format!("/api/v1/accounts/{}/folders", account.id), &token)
        .await
        .json();
    let inbox = folders.iter().find(|f| f.role == "inbox").unwrap();

    let list: MessageList = api
        .get(
            &format!("/api/v1/messages?folder={}&limit=10", inbox.id),
            &token,
        )
        .await
        .json();
    assert_eq!(list.total, 1);
    let unified: MessageList = api
        .get("/api/v1/messages?role=inbox&q=api&unread=1", &token)
        .await
        .json();
    assert_eq!(unified.total, 1);
    let id = list.items[0].id.clone();

    let view: MessageView = api
        .get(&format!("/api/v1/messages/{id}"), &token)
        .await
        .json();
    assert!(view.text.contains("hello over http"));

    let changed = api
        .send_json(
            Method::POST,
            "/api/v1/message-changes",
            &token,
            &Change {
                ids: vec![id.clone()],
                seen: Some(true),
                ..Change::default()
            },
        )
        .await;
    assert_eq!(changed.status, StatusCode::NO_CONTENT);

    let queued = api
        .send_json(
            Method::POST,
            "/api/v1/outgoing",
            &token,
            &Compose {
                account_id: account.id.clone(),
                to: vec!["friend@example.org".into()],
                subject: "via api".into(),
                text: "hi".into(),
                ..Compose::default()
            },
        )
        .await;
    assert_eq!(queued.status, StatusCode::CREATED);
    let queued: Queued = queued.json();
    assert_eq!(queued.status, "queued");

    let refreshed: Account = api
        .call(
            Method::POST,
            &format!("/api/v1/accounts/{}/refresh", account.id),
            Some(&token),
            None,
            &[],
        )
        .await
        .json();
    assert_eq!(refreshed.id, account.id);
    let removed = api
        .call(
            Method::DELETE,
            &format!("/api/v1/accounts/{}", account.id),
            Some(&token),
            None,
            &[],
        )
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(
        api.get(&format!("/api/v1/messages/{id}"), &token)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[quench_test::test]
async fn the_connection_test_endpoint_answers_for_each_half() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:write").await;
    let result: TestResult = api
        .send_json(
            Method::POST,
            "/api/v1/account-tests",
            &token,
            &api.env.input(),
        )
        .await
        .json();
    assert!(result.imap.ok && result.smtp.ok);
    let mut bad = api.env.input();
    bad.password = "wrong".into();
    let result: TestResult = api
        .send_json(Method::POST, "/api/v1/account-tests", &token, &bad)
        .await
        .json();
    assert!(!result.imap.ok);
    let mut invalid = api.env.input();
    invalid.address = "nope".into();
    let r = api
        .send_json(Method::POST, "/api/v1/account-tests", &token, &invalid)
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(r.text().contains("\"error\""));
}

#[quench_test::test]
async fn the_html_of_a_message_is_served_only_inside_a_cage() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:read").await;
    api.env.imap.deliver(
        "me@example.com",
        "INBOX",
        b"From: a@x.example\r\nTo: me@example.com\r\nSubject: html\r\nMessage-ID: <h@x>\r\nContent-Type: text/html\r\n\r\n<p>hi</p><script>alert(1)</script><img src=\"https://t.example/p.gif\">",
        &[],
    );
    let account = api.env.add_account("ann").await;
    let inbox = api.env.folder(&account.id, "inbox").await;
    let id = mailroom_service::mailbox::list(
        &api.env.app,
        "ann",
        &mailroom_service::mailbox::ListSpec {
            folder_id: Some(inbox.id),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items[0]
        .id
        .clone();
    let page = api
        .get(&format!("/api/v1/messages/{id}/html"), &token)
        .await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.header("content-type").starts_with("text/html"));
    let csp = page.header("content-security-policy");
    assert!(
        csp.contains("default-src 'none'")
            && csp.contains("sandbox")
            && csp.contains("img-src data:;"),
        "{csp}"
    );
    assert!(!csp.contains("allow-scripts") && !csp.contains("allow-same-origin"));
    assert_eq!(page.header("x-content-type-options"), "nosniff");
    assert_eq!(page.header("cache-control"), "private, no-store");
    assert_eq!(page.header("referrer-policy"), "no-referrer");
    assert!(
        page.text().contains("<p>hi</p>")
            && !page.text().contains("<script")
            && !page.text().contains("t.example")
    );
    let loaded = api
        .get(&format!("/api/v1/messages/{id}/html?remote=1"), &token)
        .await;
    assert!(
        loaded
            .header("content-security-policy")
            .contains("img-src data: https: http:")
    );
    assert!(loaded.text().contains("https://t.example/p.gif"));
    // Someone else gets nothing, not even the existence of the message.
    let other = api.token("bob", "mailroom:read").await;
    assert_eq!(
        api.get(&format!("/api/v1/messages/{id}/html"), &other)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[quench_test::test]
async fn attachments_always_download_and_never_render() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:read").await;
    api.env.imap.deliver(
        "me@example.com",
        "INBOX",
        b"From: a@x.example\r\nTo: me@example.com\r\nSubject: att\r\nMessage-ID: <at@x>\r\nContent-Type: multipart/mixed; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nsee file\r\n--b\r\nContent-Type: text/html; name=\"x.html\"\r\nContent-Disposition: attachment; filename=\"x.html\"\r\n\r\n<script>alert(1)</script>\r\n--b--\r\n",
        &[],
    );
    let account = api.env.add_account("ann").await;
    let inbox = api.env.folder(&account.id, "inbox").await;
    let id = mailroom_service::mailbox::list(
        &api.env.app,
        "ann",
        &mailroom_service::mailbox::ListSpec {
            folder_id: Some(inbox.id),
            limit: 5,
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .items[0]
        .id
        .clone();
    let download = api
        .get(&format!("/api/v1/messages/{id}/attachments/0"), &token)
        .await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.header("content-type"), "application/octet-stream");
    assert!(
        download
            .header("content-disposition")
            .starts_with("attachment; filename=\"x.html\"")
    );
    assert_eq!(download.header("x-content-type-options"), "nosniff");
    assert!(
        download
            .header("content-security-policy")
            .contains("sandbox")
    );
    assert_eq!(download.body, b"<script>alert(1)</script>");
    assert_eq!(
        api.get(&format!("/api/v1/messages/{id}/attachments/9"), &token)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

/// A `multipart/form-data` body with one file field, the way the page's upload control sends it.
fn multipart(name: &str, data: &[u8]) -> (Vec<u8>, String) {
    let boundary = "----mailroomtest";
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: text/plain\r\n\r\n").into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, format!("multipart/form-data; boundary={boundary}"))
}

#[quench_test::test]
async fn files_are_uploaded_with_a_name_and_removed() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:write").await;
    let (body, content_type) = multipart("notes.txt", b"some notes");
    let reply = api
        .call(
            Method::POST,
            "/api/v1/uploads",
            Some(&token),
            Some(body),
            &[("content-type", &content_type)],
        )
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text());
    let upload: Upload = reply.json();
    assert_eq!((upload.filename.as_str(), upload.size), ("notes.txt", 10));
    let other = api.token("bob", "mailroom:write").await;
    assert_eq!(
        api.call(
            Method::DELETE,
            &format!("/api/v1/uploads/{}", upload.id),
            Some(&other),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.call(
            Method::DELETE,
            &format!("/api/v1/uploads/{}", upload.id),
            Some(&token),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    // A path in the name is only a name; an empty file, a missing file field, and a body that is not an upload are refused.
    let (body, content_type) = multipart("../../etc/passwd", b"x");
    let named: Upload = api
        .call(
            Method::POST,
            "/api/v1/uploads",
            Some(&token),
            Some(body),
            &[("content-type", &content_type)],
        )
        .await
        .json();
    assert_eq!(named.filename, "passwd");
    let (body, content_type) = multipart("e.txt", b"");
    assert_eq!(
        api.call(
            Method::POST,
            "/api/v1/uploads",
            Some(&token),
            Some(body),
            &[("content-type", &content_type)]
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let no_file =
        b"--b\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nx\r\n--b--\r\n".to_vec();
    assert_eq!(
        api.call(
            Method::POST,
            "/api/v1/uploads",
            Some(&token),
            Some(no_file),
            &[("content-type", "multipart/form-data; boundary=b")]
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        api.call(
            Method::POST,
            "/api/v1/uploads",
            Some(&token),
            Some(b"plain".to_vec()),
            &[("content-type", "text/plain")]
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let (body, content_type) = multipart("huge.bin", &vec![0u8; 22 * 1024 * 1024]);
    let huge = api
        .call(
            Method::POST,
            "/api/v1/uploads",
            Some(&token),
            Some(body),
            &[("content-type", &content_type)],
        )
        .await;
    assert_eq!(huge.status, StatusCode::BAD_REQUEST, "{}", huge.text());
}

#[quench_test::test]
async fn a_page_from_another_site_cannot_act_with_the_session_cookie() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:write").await;
    let cookie = format!(
        "{}={token}",
        quench_auth::domain::realm::session_cookie_name()
    );
    let path = "/api/v1/accounts/anything";
    let cross = api
        .call(
            Method::DELETE,
            path,
            None,
            None,
            &[
                ("cookie", &cookie),
                ("sec-fetch-site", "cross-site"),
                ("host", "ennor.ddns.net"),
            ],
        )
        .await;
    assert_eq!(cross.status, StatusCode::FORBIDDEN);
    let forged_origin = api
        .call(
            Method::DELETE,
            path,
            None,
            None,
            &[
                ("cookie", &cookie),
                ("origin", "https://evil.example"),
                ("host", "ennor.ddns.net"),
            ],
        )
        .await;
    assert_eq!(forged_origin.status, StatusCode::FORBIDDEN);
    let same = api
        .call(
            Method::DELETE,
            path,
            None,
            None,
            &[
                ("cookie", &cookie),
                ("sec-fetch-site", "same-origin"),
                ("host", "ennor.ddns.net"),
            ],
        )
        .await;
    assert_eq!(
        same.status,
        StatusCode::NOT_FOUND,
        "the request was let through to the handler"
    );
    // Reading with the cookie across sites is allowed by this check (the browser's SameSite rule covers it).
    let read = api
        .call(
            Method::GET,
            "/api/v1/accounts",
            None,
            None,
            &[("cookie", &cookie), ("sec-fetch-site", "cross-site")],
        )
        .await;
    assert_eq!(read.status, StatusCode::OK);
}

#[quench_test::test]
async fn every_api_answer_is_private() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:read").await;
    let r = api.get("/api/v1/accounts", &token).await;
    assert_eq!(r.header("cache-control"), "no-store");
    assert_eq!(r.header("referrer-policy"), "no-referrer");
    assert_eq!(r.header("x-content-type-options"), "nosniff");
    let denied = api
        .call(Method::GET, "/api/v1/accounts", None, None, &[])
        .await;
    assert_eq!(denied.header("cache-control"), "no-store");
}

#[quench_test::test]
async fn the_panel_manages_settings_without_ever_returning_a_password() {
    db!();
    let api = api().await;
    let admin = api.token("adam", "mailroom:admin").await;
    let settings: Settings = api.get("/api/v1/admin/settings", &admin).await.json();
    assert!(settings.transport.as_ref().unwrap().has_password);
    assert!(
        !api.get("/api/v1/admin/settings", &admin)
            .await
            .text()
            .contains("relay-pw")
    );

    let change = SettingsInput {
        transport: Some(TransportInput {
            host: "smtp.example.net".into(),
            port: 587,
            security: "starttls".into(),
            username: "relay".into(),
            password: "new-relay-password".into(),
            tls_roots_pem: String::new(),
        }),
        identities: Some(vec![
            Identity {
                address: "noreply@example.com".into(),
                name: "Forge".into(),
                default: false,
            },
            Identity {
                address: "alerts@example.com".into(),
                name: String::new(),
                default: true,
            },
        ]),
        private_hosts: Some(vec!["Bridge.Mail.Svc".into(), "*.internal.example".into()]),
        extra_ports: Some(vec![1143]),
        daily_budget: Some(1000),
        hourly_per_source: Some(50),
        max_accounts_per_user: Some(3),
        sync_interval_secs: Some(300),
    };
    let reply = api
        .send_json(Method::PUT, "/api/v1/admin/settings", &admin, &change)
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert!(!reply.text().contains("new-relay-password"));
    let saved: Settings = reply.json();
    assert_eq!(saved.transport.as_ref().unwrap().host, "smtp.example.net");
    assert_eq!(
        saved.private_hosts,
        ["bridge.mail.svc", "*.internal.example"]
    );
    assert_eq!(saved.identities.iter().filter(|i| i.default).count(), 1);
    assert_eq!(
        (
            saved.daily_budget,
            saved.max_accounts_per_user,
            saved.sync_interval_secs
        ),
        (1000, 3, 300)
    );
    // The password is stored sealed and is the new one.
    let sealed = mailroom_service::settings::transport_secret(&api.env.app.store)
        .await
        .unwrap()
        .unwrap();
    assert!(sealed.starts_with("v1.") && !sealed.contains("new-relay"));
    assert_eq!(
        mailroom_service::settings::open_transport_secret(&api.env.app.store, &api.env.app.vault)
            .await
            .unwrap(),
        "new-relay-password"
    );
    // The policy the connections obey follows the settings at once.
    let policy = api.env.app.policy();
    assert!(policy.allows_private("bridge.mail.svc") && policy.allows_port(1143));
    // Pointing the stored login at another server without typing the password again is refused: it would hand it to that server.
    let moved = SettingsInput {
        transport: Some(TransportInput {
            host: "elsewhere.example".into(),
            port: 587,
            security: "starttls".into(),
            username: "relay".into(),
            ..TransportInput::default()
        }),
        ..SettingsInput::default()
    };
    assert_eq!(
        api.send_json(Method::PUT, "/api/v1/admin/settings", &admin, &moved)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    // A blank password keeps the stored one when the server stays the same.
    let again = SettingsInput {
        transport: Some(TransportInput {
            host: "smtp.example.net".into(),
            port: 587,
            security: "starttls".into(),
            username: "relay".into(),
            ..TransportInput::default()
        }),
        ..SettingsInput::default()
    };
    assert_eq!(
        api.send_json(Method::PUT, "/api/v1/admin/settings", &admin, &again)
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        mailroom_service::settings::open_transport_secret(&api.env.app.store, &api.env.app.vault)
            .await
            .unwrap(),
        "new-relay-password"
    );
    // What happened is on record, without values.
    let audit: Page<mailroom_core::admin::AuditEntry> =
        api.get("/api/v1/admin/audit", &admin).await.json();
    assert!(audit.items.iter().any(|e| e.action == "settings.update"
        && e.actor == "adam"
        && e.detail.contains("transport")));
    assert!(!quench_serde::json::to_string(&audit).contains("new-relay-password"));
}

#[quench_test::test]
async fn settings_are_validated_and_nothing_changes_on_a_bad_form() {
    db!();
    let api = api().await;
    let admin = api.token("adam", "mailroom:admin").await;
    let before: Settings = api.get("/api/v1/admin/settings", &admin).await.json();
    let bads: Vec<SettingsInput> = vec![
        SettingsInput {
            transport: Some(TransportInput {
                host: "bad host".into(),
                port: 587,
                security: "tls".into(),
                ..TransportInput::default()
            }),
            ..SettingsInput::default()
        },
        SettingsInput {
            transport: Some(TransportInput {
                host: "ok.example".into(),
                port: 0,
                security: "tls".into(),
                ..TransportInput::default()
            }),
            ..SettingsInput::default()
        },
        SettingsInput {
            transport: Some(TransportInput {
                host: "ok.example".into(),
                port: 25,
                security: "plain".into(),
                ..TransportInput::default()
            }),
            ..SettingsInput::default()
        },
        SettingsInput {
            transport: Some(TransportInput {
                host: "ok.example".into(),
                port: 25,
                security: "tls".into(),
                password: "a\nb".into(),
                ..TransportInput::default()
            }),
            ..SettingsInput::default()
        },
        SettingsInput {
            transport: Some(TransportInput {
                host: "ok.example".into(),
                port: 25,
                security: "tls".into(),
                tls_roots_pem: "junk".into(),
                ..TransportInput::default()
            }),
            ..SettingsInput::default()
        },
        SettingsInput {
            identities: Some(vec![Identity {
                address: "nope".into(),
                name: String::new(),
                default: true,
            }]),
            ..SettingsInput::default()
        },
        SettingsInput {
            identities: Some(vec![Identity {
                address: "a@x.example".into(),
                name: "x\ny".into(),
                default: true,
            }]),
            ..SettingsInput::default()
        },
        SettingsInput {
            identities: Some(vec![
                Identity {
                    address: "a@x.example".into(),
                    name: String::new(),
                    default: true,
                },
                Identity {
                    address: "A@X.example".into(),
                    name: String::new(),
                    default: false,
                },
            ]),
            ..SettingsInput::default()
        },
        SettingsInput {
            private_hosts: Some(vec!["http://x".into()]),
            ..SettingsInput::default()
        },
        SettingsInput {
            private_hosts: Some(vec!["[::1]".into()]),
            ..SettingsInput::default()
        },
        SettingsInput {
            extra_ports: Some(vec![0]),
            ..SettingsInput::default()
        },
        SettingsInput {
            daily_budget: Some(0),
            ..SettingsInput::default()
        },
        SettingsInput {
            hourly_per_source: Some(1_000_000),
            ..SettingsInput::default()
        },
        SettingsInput {
            max_accounts_per_user: Some(0),
            ..SettingsInput::default()
        },
        SettingsInput {
            sync_interval_secs: Some(5),
            ..SettingsInput::default()
        },
    ];
    for bad in bads {
        let reply = api
            .send_json(Method::PUT, "/api/v1/admin/settings", &admin, &bad)
            .await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{bad:?}");
    }
    let after: Settings = api.get("/api/v1/admin/settings", &admin).await.json();
    assert_eq!(before, after);
}

#[quench_test::test]
async fn the_panel_lists_everyones_mailboxes_but_not_their_mail_and_can_switch_them_off() {
    db!();
    let api = api().await;
    let admin = api.token("adam", "mailroom:admin").await;
    api.env.deliver("INBOX", "secret subject", "secret body");
    let account = api.env.add_account("ann").await;
    let page: Page<mailroom_core::admin::AdminAccount> =
        api.get("/api/v1/admin/accounts", &admin).await.json();
    assert_eq!(page.total, 1);
    assert_eq!(
        (page.items[0].owner.as_str(), page.items[0].messages),
        ("ann", 1)
    );
    let raw = api.get("/api/v1/admin/accounts", &admin).await.text();
    assert!(!raw.contains("secret subject") && !raw.contains(PASSWORD) && !raw.contains("v1."));
    // An administrator is not a reader of someone else's mail.
    let inbox = api.env.folder(&account.id, "inbox").await;
    let theirs = api
        .get(
            &format!("/api/v1/messages?folder={}", inbox.id),
            &api.token("adam", "mailroom:admin").await,
        )
        .await;
    assert_eq!(theirs.status, StatusCode::NOT_FOUND);

    let off = api
        .call(
            Method::POST,
            &format!("/api/v1/admin/accounts/{}/disable", account.id),
            Some(&admin),
            None,
            &[],
        )
        .await;
    assert_eq!(off.status, StatusCode::NO_CONTENT);
    let accounts: Vec<Account> = api
        .get("/api/v1/accounts", &api.token("ann", "mailroom:read").await)
        .await
        .json();
    assert_eq!(accounts[0].state, "disabled");
    assert_eq!(
        api.call(
            Method::POST,
            &format!("/api/v1/admin/accounts/{}/enable", account.id),
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.call(
            Method::POST,
            "/api/v1/admin/accounts/nope/disable",
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.call(
            Method::DELETE,
            &format!("/api/v1/admin/accounts/{}", account.id),
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.call(
            Method::DELETE,
            &format!("/api/v1/admin/accounts/{}", account.id),
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let audit: Page<mailroom_core::admin::AuditEntry> =
        api.get("/api/v1/admin/audit?limit=10", &admin).await.json();
    let actions: Vec<_> = audit.items.iter().map(|e| e.action.as_str()).collect();
    for expected in ["account.disable", "account.enable", "account.delete"] {
        assert!(actions.contains(&expected), "{actions:?}");
    }
}

#[quench_test::test]
async fn the_panel_shows_the_queue_and_can_retry_and_remove() {
    db!();
    let api = api().await;
    let admin = api.token("adam", "mailroom:admin").await;
    let sender = api.token("gatehouse", "mailroom:send").await;
    let mail = SendRequest {
        to: vec!["a@example.org".into()],
        subject: "Queued one".into(),
        text: "t".into(),
        ..SendRequest::default()
    };
    let queued: Queued = api
        .send_json(Method::POST, "/api/v1/send", &sender, &mail)
        .await
        .json();
    let page: Page<mailroom_core::admin::OutboxItem> = api
        .get("/api/v1/admin/outbox?status=queued", &admin)
        .await
        .json();
    assert_eq!(
        (
            page.total,
            page.items[0].subject.as_str(),
            page.items[0].source.as_str()
        ),
        (1, "Queued one", "gatehouse")
    );
    assert!(
        !quench_serde::json::to_string(&page).contains("Content-Type"),
        "no message bodies in the panel"
    );
    assert_eq!(
        api.get("/api/v1/admin/outbox?status=bogus", &admin)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let stats: Stats = api.get("/api/v1/admin/stats", &admin).await.json();
    assert_eq!((stats.queued, stats.budget_left), (1, 499));
    assert_eq!(
        api.call(
            Method::POST,
            &format!("/api/v1/admin/outbox/{}/retry", queued.id),
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.call(
            Method::POST,
            "/api/v1/admin/outbox/nope/retry",
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.call(
            Method::DELETE,
            &format!("/api/v1/admin/outbox/{}", queued.id),
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.call(
            Method::DELETE,
            &format!("/api/v1/admin/outbox/{}", queued.id),
            Some(&admin),
            None,
            &[]
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[quench_test::test]
async fn the_sending_server_can_be_tried_from_the_panel() {
    db!();
    let api = api().await;
    let admin = api.token("adam", "mailroom:admin").await;
    let ok: TestResult = api
        .call(
            Method::POST,
            "/api/v1/admin/transport/test",
            Some(&admin),
            None,
            &[],
        )
        .await
        .json();
    assert!(ok.smtp.ok, "{ok:?}");
    mailroom_service::settings::save_transport_secret(
        &api.env.app.store,
        &api.env.app.vault,
        "wrong",
    )
    .await
    .unwrap();
    let bad: TestResult = api
        .call(
            Method::POST,
            "/api/v1/admin/transport/test",
            Some(&admin),
            None,
            &[],
        )
        .await
        .json();
    assert!(!bad.smtp.ok && !bad.smtp.error.is_empty());
}

#[quench_test::test]
async fn the_service_endpoint_answers_for_retries_and_for_a_missing_server() {
    db!();
    let api = api().await;
    let sender = api.token("gatehouse", "mailroom:send").await;
    let mut mail = SendRequest {
        to: vec!["a@example.org".into()],
        subject: "s".into(),
        text: "t".into(),
        idempotency_key: Some("k1".into()),
        ..SendRequest::default()
    };
    let first = api
        .send_json(Method::POST, "/api/v1/send", &sender, &mail)
        .await;
    assert_eq!(first.status, StatusCode::ACCEPTED);
    let again = api
        .send_json(Method::POST, "/api/v1/send", &sender, &mail)
        .await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json::<Queued>().status, "duplicate");
    mail.from = "intruder@example.com".into();
    mail.idempotency_key = None;
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/send", &sender, &mail)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let mut stored = api.env.app.settings();
    stored.transport = None;
    api.env.app.set_settings(stored);
    mail.from = String::new();
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/send", &sender, &mail)
            .await
            .status,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[quench_test::test]
async fn malformed_requests_are_refused_cleanly() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:write").await;
    let r = api
        .call(
            Method::POST,
            "/api/v1/accounts",
            Some(&token),
            Some(b"{not json".to_vec()),
            &[],
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api
        .call(
            Method::POST,
            "/api/v1/message-changes",
            Some(&token),
            Some(b"{}".to_vec()),
            &[],
        )
        .await;
    assert!(r.status.is_client_error());
    let r = api.get("/api/v1/messages", &token).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api.get("/api/v1/messages?role=../../etc", &token).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    for hostile in ["'--", "%27%20OR%201=1%20--", "%5C", "%00"] {
        let r = api
            .get(&format!("/api/v1/messages?folder={hostile}"), &token)
            .await;
        assert!(
            r.status == StatusCode::NOT_FOUND || r.status == StatusCode::BAD_REQUEST,
            "{hostile}: {}",
            r.status
        );
        let r = api
            .get(&format!("/api/v1/messages/{hostile}"), &token)
            .await;
        assert!(
            r.status == StatusCode::NOT_FOUND || r.status == StatusCode::BAD_REQUEST,
            "{hostile}: {}",
            r.status
        );
    }
}
