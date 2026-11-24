//! The HTTP API through the whole stack: the realm's authentication (real signed tokens), the same-origin check, the gate
//! and the handlers, over real Postgres.

use crate::support::*;
use mailroom_core::admin::{
    AdminMailbox, AuditEntry, Delivery, Page, Settings, SettingsInput, Stats,
};
use mailroom_core::{
    Change, Compose, Contact, Folder, Me, MessageList, MessageView, SendRequest, Sent, Upload,
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
        (Method::GET, "/api/v1/folders"),
        (Method::GET, "/api/v1/messages"),
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
    let machine = api.token("gatehouse-mailroom", "service").await;
    let stranger = api.token("sam", "sage:write").await;

    assert_eq!(
        api.get("/api/v1/folders", &reader).await.status,
        StatusCode::OK
    );
    for token in [&stranger, &sender, &machine] {
        assert_eq!(
            api.get("/api/v1/folders", token).await.status,
            StatusCode::FORBIDDEN
        );
    }
    // A reader reads; writing is for a writer.
    let mail = Compose {
        to: vec!["walt".into()],
        subject: "s".into(),
        text: "t".into(),
        ..Compose::default()
    };
    api.get("/api/v1/me", &writer).await;
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/outgoing", &reader, &mail)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/outgoing", &admin, &mail)
            .await
            .status,
        StatusCode::CREATED
    );
    // The panel is for administrators only.
    for token in [&reader, &writer, &stranger, &sender, &machine] {
        for path in [
            "/api/v1/admin/stats",
            "/api/v1/admin/settings",
            "/api/v1/admin/mailboxes",
            "/api/v1/admin/deliveries",
            "/api/v1/admin/audit",
        ] {
            assert_eq!(
                api.get(path, token).await.status,
                StatusCode::FORBIDDEN,
                "{path}"
            );
        }
    }
    assert_eq!(
        api.get("/api/v1/admin/stats", &admin).await.status,
        StatusCode::OK
    );
    // The services' endpoint is for machines holding `send`; people, administrators included, do not.
    let request = SendRequest {
        to: vec!["walt".into()],
        subject: "s".into(),
        text: "t".into(),
        ..SendRequest::default()
    };
    for token in [&reader, &writer, &admin, &stranger] {
        assert_eq!(
            api.send_json(Method::POST, "/api/v1/send", token, &request)
                .await
                .status,
            StatusCode::FORBIDDEN
        );
    }
    for token in [&sender, &machine] {
        assert_eq!(
            api.send_json(Method::POST, "/api/v1/send", token, &request)
                .await
                .status,
            StatusCode::CREATED
        );
    }
}

#[quench_test::test]
async fn me_says_what_the_person_may_do() {
    db!();
    let api = api().await;
    let me: Me = api
        .get("/api/v1/me", &api.token("ann", "mailroom:write").await)
        .await
        .json();
    assert_eq!(
        (
            me.username.as_str(),
            me.address.as_str(),
            me.can_write,
            me.admin
        ),
        ("ann", "ann@home.example", true, false)
    );
    let me: Me = api
        .get("/api/v1/me", &api.token("rita", "mailroom:read").await)
        .await
        .json();
    assert!(!me.can_write && !me.admin);
    let me: Me = api
        .get("/api/v1/me", &api.token("adam", "mailroom:admin").await)
        .await
        .json();
    assert!(me.admin && me.can_write);
    // Gatehouse's user names are case-sensitive, so `Eve` and `eve` are two people: a name with capitals has no mailbox here
    // (it would otherwise be somebody else's). Nor does the name of a service.
    for name in ["Eve", "gatehouse"] {
        let r = api
            .get("/api/v1/me", &api.token(name, "mailroom:read").await)
            .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{name}");
    }
    // A name that cannot be an address has no mailbox.
    assert_eq!(
        api.get("/api/v1/me", &api.token("a@b", "mailroom:read").await)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
}

#[quench_test::test]
async fn two_people_write_to_each_other_end_to_end() {
    db!();
    let api = api().await;
    let ann = api.token("ann", "mailroom:write").await;
    let bob = api.token("bob", "mailroom:write").await;
    api.get("/api/v1/me", &bob).await;
    let sent: Sent = api
        .send_json(
            Method::POST,
            "/api/v1/outgoing",
            &ann,
            &Compose {
                to: vec!["bob@home.example".into()],
                subject: "Hello Bob".into(),
                text: "How are you?".into(),
                ..Compose::default()
            },
        )
        .await
        .json();
    assert_eq!((sent.status.as_str(), sent.delivered), ("delivered", 1));
    let folders: Vec<Folder> = api.get("/api/v1/folders", &bob).await.json();
    assert_eq!(
        (folders[0].id.as_str(), folders[0].unread, folders[0].total),
        ("inbox", 1, 1)
    );
    let list: MessageList = api.get("/api/v1/messages?folder=inbox", &bob).await.json();
    assert_eq!(list.items[0].subject, "Hello Bob");
    let id = list.items[0].id.clone();
    let view: MessageView = api
        .get(&format!("/api/v1/messages/{id}"), &bob)
        .await
        .json();
    assert_eq!(view.text, "How are you?");
    // Mark read, star, and trash.
    let read = Change {
        ids: vec![id.clone()],
        seen: Some(true),
        flagged: Some(true),
        ..Change::default()
    };
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/message-changes", &bob, &read)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let list: MessageList = api
        .get("/api/v1/messages?folder=inbox&flagged=1", &bob)
        .await
        .json();
    assert!(list.items[0].seen && list.items[0].flagged);
    let sent_list: MessageList = api
        .get("/api/v1/messages?folder=sent&q=hello", &ann)
        .await
        .json();
    assert_eq!(sent_list.total, 1);
    // Bob cannot see Ann's mail by guessing, and the other way round.
    let theirs = sent_list.items[0].id.clone();
    assert_eq!(
        api.get(&format!("/api/v1/messages/{theirs}"), &bob)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // Addresses complete.
    let found: Vec<Contact> = api.get("/api/v1/people?q=bo", &ann).await.json();
    assert_eq!(
        found.iter().map(|c| c.address.as_str()).collect::<Vec<_>>(),
        ["bob@home.example"]
    );
    // Writing outside the estate is refused with a sentence.
    let outside = api
        .send_json(
            Method::POST,
            "/api/v1/outgoing",
            &ann,
            &Compose {
                to: vec!["x@gmail.com".into()],
                subject: "s".into(),
                text: "t".into(),
                ..Compose::default()
            },
        )
        .await;
    assert_eq!(outside.status, StatusCode::BAD_REQUEST);
    assert!(
        outside.text().contains("stays inside this estate"),
        "{}",
        outside.text()
    );
}

#[quench_test::test]
async fn the_html_of_a_message_is_served_only_inside_a_cage() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:read").await;
    api.env
        .service_mail("gatehouse", &["ann"], "html", "plain")
        .await
        .unwrap();
    mailroom_service::outgoing::send_service(
        &api.env.app,
        "gatehouse",
        &SendRequest {
            to: vec!["ann".into()],
            subject: "html2".into(),
            text: "plain".into(),
            html: Some(
                "<p>hi</p><script>alert(1)</script><img src=\"https://t.example/p.gif\">".into(),
            ),
            ..SendRequest::default()
        },
    )
    .await
    .unwrap();
    let id = api
        .env
        .folder("ann", "inbox")
        .await
        .into_iter()
        .find(|m| m.subject == "html2")
        .unwrap()
        .id;
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

/// A `multipart/form-data` body with one file field, the way the page's upload control sends it.
fn multipart(name: &str, data: &[u8]) -> (Vec<u8>, String) {
    let boundary = "----mailroomtest";
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: text/plain\r\n\r\n").into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (body, format!("multipart/form-data; boundary={boundary}"))
}

#[quench_test::test]
async fn files_are_uploaded_attached_and_always_downloaded_never_rendered() {
    db!();
    let api = api().await;
    let ann = api.token("ann", "mailroom:write").await;
    let bob = api.token("bob", "mailroom:write").await;
    api.get("/api/v1/me", &bob).await;
    let (body, content_type) = multipart("../notes.html", b"<script>alert(1)</script>");
    let reply = api
        .call(
            Method::POST,
            "/api/v1/uploads",
            Some(&ann),
            Some(body),
            &[("content-type", &content_type)],
        )
        .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.text());
    let upload: Upload = reply.json();
    assert_eq!((upload.filename.as_str(), upload.size), ("notes.html", 25));
    // Someone else cannot remove it; its owner can.
    let remove = |token: &str, id: &str| {
        let path = format!("/api/v1/uploads/{id}");
        let token = token.to_string();
        let api = &api;
        async move {
            api.call(Method::DELETE, &path, Some(&token), None, &[])
                .await
                .status
        }
    };
    assert_eq!(remove(&bob, &upload.id).await, StatusCode::NOT_FOUND);
    // Send it.
    let mail = Compose {
        to: vec!["bob".into()],
        subject: "file".into(),
        text: "see".into(),
        attachments: vec![upload.id.clone()],
        ..Compose::default()
    };
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/outgoing", &ann, &mail)
            .await
            .status,
        StatusCode::CREATED
    );
    assert_eq!(
        remove(&ann, &upload.id).await,
        StatusCode::NOT_FOUND,
        "sending used it up"
    );
    let id = api.env.folder("bob", "inbox").await[0].id.clone();
    let download = api
        .get(&format!("/api/v1/messages/{id}/attachments/0"), &bob)
        .await;
    assert_eq!(download.status, StatusCode::OK);
    assert_eq!(download.header("content-type"), "application/octet-stream");
    assert!(
        download
            .header("content-disposition")
            .starts_with("attachment; filename=\"notes.html\"")
    );
    assert_eq!(download.header("x-content-type-options"), "nosniff");
    assert!(
        download
            .header("content-security-policy")
            .contains("sandbox")
    );
    assert_eq!(download.body, b"<script>alert(1)</script>");
    assert_eq!(
        api.get(&format!("/api/v1/messages/{id}/attachments/9"), &bob)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.get(&format!("/api/v1/messages/{id}/attachments/-1"), &bob)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    // An empty file, no file field, and a body that is not an upload are refused.
    let (empty, ct) = multipart("e.txt", b"");
    assert_eq!(
        api.call(
            Method::POST,
            "/api/v1/uploads",
            Some(&ann),
            Some(empty),
            &[("content-type", &ct)]
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let none =
        b"--b\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nx\r\n--b--\r\n".to_vec();
    assert_eq!(
        api.call(
            Method::POST,
            "/api/v1/uploads",
            Some(&ann),
            Some(none),
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
            Some(&ann),
            Some(b"plain".to_vec()),
            &[("content-type", "text/plain")]
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
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
    let path = "/api/v1/uploads/anything";
    let attempt = |extra: Vec<(&'static str, &'static str)>| {
        let api = &api;
        let cookie = cookie.clone();
        async move {
            let mut headers: Vec<(&str, &str)> = vec![("host", "ennor.ddns.net")];
            headers.extend(extra);
            let cookie_ref = cookie.as_str();
            headers.push(("cookie", cookie_ref));
            api.call(Method::DELETE, path, None, None, &headers)
                .await
                .status
        }
    };
    assert_eq!(
        attempt(vec![("sec-fetch-site", "cross-site")]).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        attempt(vec![("origin", "https://evil.example")]).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        attempt(vec![("sec-fetch-site", "same-origin")]).await,
        StatusCode::NOT_FOUND,
        "let through to the handler"
    );
    let read = api
        .call(
            Method::GET,
            "/api/v1/folders",
            None,
            None,
            &[("cookie", &cookie), ("sec-fetch-site", "cross-site")],
        )
        .await;
    assert_eq!(
        read.status,
        StatusCode::OK,
        "reading is the browser's SameSite business"
    );
}

#[quench_test::test]
async fn every_api_answer_is_private() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:read").await;
    let r = api.get("/api/v1/folders", &token).await;
    assert_eq!(r.header("cache-control"), "no-store");
    assert_eq!(r.header("referrer-policy"), "no-referrer");
    assert_eq!(r.header("x-content-type-options"), "nosniff");
    let denied = api
        .call(Method::GET, "/api/v1/folders", None, None, &[])
        .await;
    assert_eq!(denied.header("cache-control"), "no-store");
}

#[quench_test::test]
async fn the_panel_works_over_http() {
    db!();
    let api = api().await;
    let admin = api.token("adam", "mailroom:admin").await;
    let ann = api.token("ann", "mailroom:write").await;
    api.get("/api/v1/me", &ann).await;
    api.env
        .service_mail("gatehouse", &["ann"], "Welcome", "hi")
        .await
        .unwrap();
    let settings: Settings = api.get("/api/v1/admin/settings", &admin).await.json();
    assert_eq!(settings.domain, "home.example");
    let changed: Settings = api
        .send_json(
            Method::PUT,
            "/api/v1/admin/settings",
            &admin,
            &SettingsInput {
                quota_mb: Some(40),
                ..SettingsInput::default()
            },
        )
        .await
        .json();
    assert_eq!(changed.quota_mb, 40);
    let bad = api
        .send_json(
            Method::PUT,
            "/api/v1/admin/settings",
            &admin,
            &SettingsInput {
                quota_mb: Some(0),
                ..SettingsInput::default()
            },
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    let stats: Stats = api.get("/api/v1/admin/stats", &admin).await.json();
    assert_eq!((stats.mailboxes, stats.delivered_24h), (1, 1));
    let boxes: Page<AdminMailbox> = api
        .get("/api/v1/admin/mailboxes?limit=10", &admin)
        .await
        .json();
    assert_eq!(boxes.total, 1);
    let deliveries: Page<Delivery> = api.get("/api/v1/admin/deliveries", &admin).await.json();
    assert_eq!(deliveries.items[0].subject, "Welcome");
    // Switch Ann off and on again, then delete her.
    let post = |path: String| {
        let api = &api;
        let admin = admin.clone();
        async move {
            api.call(Method::POST, &path, Some(&admin), None, &[])
                .await
                .status
        }
    };
    assert_eq!(
        post("/api/v1/admin/mailboxes/ann/disable".into()).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.get("/api/v1/folders", &ann).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post("/api/v1/admin/mailboxes/ann/enable".into()).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.get("/api/v1/folders", &ann).await.status,
        StatusCode::OK
    );
    assert_eq!(
        post("/api/v1/admin/mailboxes/ghost/disable".into()).await,
        StatusCode::NOT_FOUND
    );
    let gone = api
        .call(
            Method::DELETE,
            "/api/v1/admin/mailboxes/ann",
            Some(&admin),
            None,
            &[],
        )
        .await;
    assert_eq!(gone.status, StatusCode::NO_CONTENT);
    let audit: Page<AuditEntry> = api.get("/api/v1/admin/audit", &admin).await.json();
    assert_eq!(
        audit
            .items
            .iter()
            .map(|a| a.action.as_str())
            .collect::<Vec<_>>(),
        ["delete-mailbox", "enable", "disable", "settings"]
    );
}

#[quench_test::test]
async fn the_service_endpoint_answers_for_retries_and_refusals() {
    db!();
    let api = api().await;
    let sender = api.token("gatehouse-mailroom", "service").await;
    let mut mail = SendRequest {
        to: vec!["ann".into()],
        subject: "s".into(),
        text: "t".into(),
        idempotency_key: Some("k1".into()),
        ..SendRequest::default()
    };
    let first = api
        .send_json(Method::POST, "/api/v1/send", &sender, &mail)
        .await;
    assert_eq!(first.status, StatusCode::CREATED);
    let again = api
        .send_json(Method::POST, "/api/v1/send", &sender, &mail)
        .await;
    assert_eq!(again.status, StatusCode::OK);
    assert_eq!(again.json::<Sent>().status, "duplicate");
    mail.idempotency_key = None;
    mail.to = vec!["someone@elsewhere.example".into()];
    assert_eq!(
        api.send_json(Method::POST, "/api/v1/send", &sender, &mail)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    // The mail came from the service, by name.
    let inbox = api.env.folder("ann", "inbox").await;
    assert_eq!(
        (
            inbox.len(),
            inbox[0].from.address.as_str(),
            inbox[0].automated
        ),
        (1, "gatehouse@home.example", true)
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
            "/api/v1/outgoing",
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
    assert_eq!(
        api.get("/api/v1/messages?folder=spam", &token).await.status,
        StatusCode::BAD_REQUEST
    );
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
        let r = api
            .get(&format!("/api/v1/messages?q={hostile}"), &token)
            .await;
        assert!(
            r.status == StatusCode::OK || r.status == StatusCode::BAD_REQUEST,
            "{hostile}: {}",
            r.status
        );
        let r = api
            .get(&format!("/api/v1/people?q={hostile}"), &token)
            .await;
        assert!(
            r.status == StatusCode::OK || r.status == StatusCode::BAD_REQUEST,
            "{hostile}: {}",
            r.status
        );
    }
}

#[quench_test::test]
async fn the_pages_need_a_session_and_serve_the_client() {
    db!();
    let api = api().await;
    let token = api.token("ann", "mailroom:read").await;
    let cookie = format!(
        "{}={token}",
        quench_auth::domain::realm::session_cookie_name()
    );
    for path in [
        "/ui",
        "/ui/",
        "/ui/home",
        "/ui/home/",
        "/ui/folder/inbox",
        "/ui/message/abc",
        "/ui/drafts",
        "/ui/muted",
        "/ui/compose",
        "/ui/compose/reply/abc",
        "/ui/compose/draft/abc",
        "/ui/admin",
        "/ui/admin/deliveries",
    ] {
        let anonymous = api.call(Method::GET, path, None, None, &[]).await;
        assert!(
            anonymous.status.is_redirection(),
            "{path}: {}",
            anonymous.status
        );
        let signed_in = api
            .call(Method::GET, path, None, None, &[("cookie", &cookie)])
            .await;
        assert!(
            signed_in.status == StatusCode::OK
                || signed_in.status.is_redirection()
                // the browser client is built by the image (web/app.wasm); without it there is no page to serve
                || signed_in.status == StatusCode::NOT_FOUND,
            "{path}: {}",
            signed_in.status
        );
    }
    let page = api
        .call(Method::GET, "/ui/home", None, None, &[("cookie", &cookie)])
        .await;
    if page.status == StatusCode::OK {
        assert!(page.header("content-type").starts_with("text/html"));
        assert!(
            page.text().contains("quench.js") || page.text().contains("app.wasm"),
            "{}",
            page.text()
        );
    } else {
        assert_eq!(
            page.status,
            StatusCode::NOT_FOUND,
            "web/app.wasm is not built"
        );
    }
    // The files the page loads need no session: they carry no data.
    for (path, kind) in [
        ("/ui/assets/quench.js", "javascript"),
        ("/ui/assets/app.css", "css"),
        ("/ui/assets/shell.js", "javascript"),
        ("/ui/assets/favicon.png", "image/png"),
    ] {
        let asset = api.call(Method::GET, path, None, None, &[]).await;
        assert_eq!(asset.status, StatusCode::OK, "{path}");
        assert!(
            asset.header("content-type").contains(kind),
            "{path}: {}",
            asset.header("content-type")
        );
    }
    let wasm = api
        .call(Method::GET, "/ui/assets/app.wasm", None, None, &[])
        .await;
    assert!(
        wasm.status == StatusCode::OK || wasm.status == StatusCode::NOT_FOUND,
        "{}",
        wasm.status
    );
    // Signing out and the session probe answer without any network.
    let out = api.call(Method::GET, "/ui/logout", None, None, &[]).await;
    // With no Gatehouse configured it has nowhere to send the browser: it says so rather than guess.
    assert!(
        out.status.is_redirection()
            || out.status == StatusCode::OK
            || out.status == StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        out.status
    );
    let status = api
        .call(
            Method::GET,
            "/ui/status",
            None,
            None,
            &[("cookie", &cookie)],
        )
        .await;
    assert!(
        status.status.is_success() || status.status.is_client_error(),
        "{}",
        status.status
    );
}

#[quench_test::test]
async fn startup_refuses_what_cannot_work() {
    use mailroom_service::{boot, config::Config, store::Store};
    let no_database = boot::app(Config::for_tests("x", "  ")).await.unwrap_err();
    assert!(
        matches!(no_database, mailroom_service::error::Error::Internal(m) if m.contains("DATABASE_URL"))
    );
    for bad in ["", "a b", "x;DROP", "ü"] {
        let e = Store::connect("postgres://nobody@127.0.0.1:1/none", bad)
            .await
            .unwrap_err();
        assert!(
            matches!(e, mailroom_service::error::Error::Internal(m) if m.contains("bad schema name")),
            "{bad}"
        );
    }
    db!();
    // A reachable database answers the readiness probe.
    let env = env().await;
    env.app.store.ping().await.unwrap();
}

#[quench_test::test]
async fn the_maintenance_worker_runs_by_itself() {
    db!();
    let env = env().await;
    env.people(&["ann"]).await;
    let up = mailroom_service::outgoing::upload(&env.app, "ann", "u", "a/b", vec![1])
        .await
        .unwrap();
    // Make the upload old, then start the worker: its first pass is immediate.
    env.app
        .store
        .pool
        .execute("UPDATE uploads SET created = 0", &[])
        .await
        .unwrap();
    mailroom_service::worker::spawn(&env.app);
    for _ in 0..50 {
        if env
            .app
            .store
            .uploads_owned("ann", &[up.id.clone()])
            .await
            .unwrap()
            .is_empty()
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the worker did not sweep");
}

#[quench_test::test]
async fn with_authentication_off_everyone_is_a_local_administrator() {
    db!();
    let mut api = api().await;
    api.jwt.auth_enabled = false;
    let container = ContainerBuilder::new()
        .provide(quench_http::extract::BodyLimit(
            mailroom_service::outgoing::MAX_UPLOAD + 1024 * 1024,
        ))
        .provide(api.env.app.clone())
        .provide(api.jwt.clone())
        .build()
        .await
        .unwrap();
    api.container = Arc::new(container);
    let endpoint = mailroom_service::routers::api::wrap_auth(
        quench_starter::http::discover_and_mount("/mailroom"),
        api.jwt.clone(),
        "/mailroom",
        vec![],
    );
    api.endpoint = endpoint;
    let me: Me = api
        .call(Method::GET, "/api/v1/me", None, None, &[])
        .await
        .json();
    assert_eq!(
        (me.username.as_str(), me.can_write, me.admin),
        ("dev", true, true)
    );
    assert_eq!(
        api.call(Method::GET, "/api/v1/admin/stats", None, None, &[])
            .await
            .status,
        StatusCode::OK
    );
    let request = SendRequest {
        to: vec!["dev".into()],
        subject: "s".into(),
        text: "t".into(),
        ..SendRequest::default()
    };
    let sent = api
        .call(
            Method::POST,
            "/api/v1/send",
            None,
            Some(quench_serde::json::to_string(&request).into_bytes()),
            &[],
        )
        .await;
    assert_eq!(sent.status, StatusCode::CREATED);
}
