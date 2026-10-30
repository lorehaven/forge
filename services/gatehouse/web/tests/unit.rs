#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use gatehouse_app::{Msg, Request};
use gatehouse_core::MfaEnrollRequest;
use gatehouse_web::client::{Boxed, Client, HttpRequest, HttpResponse, Transport};
use quench_core::future::block_on;

type Handler = Box<dyn Fn(&HttpRequest) -> (u16, String)>;
type Seen = Rc<RefCell<Vec<HttpRequest>>>;

/// A service that answers from a function and remembers what it was asked.
struct Standin {
    seen: Seen,
    handler: Handler,
}

impl Transport for Standin {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
        let (status, body) = (self.handler)(&request);
        self.seen.borrow_mut().push(request);
        Box::pin(async move { Ok(HttpResponse { status, body }) })
    }
}

fn client(handler: impl Fn(&HttpRequest) -> (u16, String) + 'static) -> (Client<Standin>, Seen) {
    let seen: Seen = Rc::default();
    (
        Client::new(Standin {
            seen: seen.clone(),
            handler: Box::new(handler),
        }),
        seen,
    )
}

fn answer(status: u16, body: &str) -> impl Fn(&HttpRequest) -> (u16, String) + 'static {
    let body = body.to_string();
    move |_| (status, body.clone())
}

const DASHBOARD: &str = r#"{"admin":true,"services":[{"url":"https://x.test/sage","title_key":"ui_service_sage_title","desc_key":"ui_service_sage_desc","label":"Sage","description":null,"card_class":"home-card-sage"}]}"#;
const ACCOUNT: &str = r#"{"username":"ola","has_avatar":false,"email_verified":false,"mfa_enabled":false,"has_notice_address":false,"timezones":[],"locales":["en-US"],"subscriptions":[]}"#;
const USERS: &str = r#"{"actor":"root","admin":true,"can_create":true,"users":[]}"#;
const USER: &str = r#"{"username":"ola k","role":"user","wildcard":false,"held":[],"email_verified":false,"disabled":false,"locked":false,"mfa_enabled":false,"created_at":"x","is_self":false,"viewer_admin":true,"can_edit":true,"can_delete":true,"can_manage_permissions":true,"services":[],"templates":[]}"#;

#[quench_test::test]
fn the_reads_go_to_the_page_api_and_come_back_as_messages() {
    for (request, path, body, expected) in [
        (
            Request::Dashboard,
            "/api/v1/ui/dashboard",
            DASHBOARD,
            "Dashboard",
        ),
        (Request::Account, "/api/v1/ui/account", ACCOUNT, "Account"),
        (
            Request::AdminUsers,
            "/api/v1/ui/admin/users",
            USERS,
            "AdminUsers",
        ),
        (
            Request::AdminUser("ola k".into()),
            "/api/v1/ui/admin/users/ola%20k",
            USER,
            "AdminUser",
        ),
        (
            Request::MfaSecret,
            "/api/v1/ui/mfa-enroll",
            r#"{"secret":"S","uri":"otpauth://x","qr":null}"#,
            "MfaSecret",
        ),
    ] {
        let (c, seen) = client(answer(200, body));
        let msgs = block_on(c.execute(request));
        assert_eq!(msgs.len(), 1, "{path}");
        assert!(
            format!("{:?}", msgs[0]).starts_with(expected),
            "{path}: {:?}",
            msgs[0]
        );
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!((seen[0].method, seen[0].path.as_str()), ("GET", path));
        assert_eq!(seen[0].body, None);
    }
}

#[quench_test::test]
fn the_dashboard_keeps_its_cards() {
    let (c, _) = client(answer(200, DASHBOARD));
    let msgs = block_on(c.execute(Request::Dashboard));
    let Msg::Dashboard(d) = &msgs[0] else {
        panic!("{:?}", msgs[0]);
    };
    assert!(d.admin);
    assert_eq!(d.services[0].url, "https://x.test/sage");
}

#[quench_test::test]
fn a_read_that_does_not_work_says_why() {
    for (status, expected) in [
        (401, Msg::SignedOut),
        (403, Msg::Forbidden),
        (404, Msg::NotFound),
        (500, Msg::Failed("the service answered 500".into())),
    ] {
        let (c, _) = client(answer(status, r#"{"error":"x"}"#));
        assert_eq!(block_on(c.execute(Request::Account)), [expected]);
    }
    let (c, _) = client(answer(200, "not json"));
    assert!(matches!(
        block_on(c.execute(Request::Account))[0],
        Msg::Failed(_)
    ));
}

#[quench_test::test]
fn the_code_is_sent_as_json_and_a_wrong_one_comes_back_as_its_key() {
    let body = MfaEnrollRequest {
        secret: "SECRET".into(),
        code: "123456".into(),
    };
    let (c, seen) = client(answer(200, r#"{"done":true}"#));
    assert_eq!(
        block_on(c.execute(Request::MfaEnable(body.clone()))),
        [Msg::MfaEnabled]
    );
    let sent = seen.borrow()[0].clone();
    assert_eq!(
        (sent.method, sent.path.as_str()),
        ("POST", "/api/v1/ui/mfa-enroll")
    );
    assert!(!sent.form);
    assert_eq!(
        sent.body.as_deref(),
        Some(r#"{"secret":"SECRET","code":"123456"}"#)
    );

    let (c, _) = client(answer(
        400,
        r#"{"error":"ui_admin_error_mfa_code_invalid"}"#,
    ));
    assert_eq!(
        block_on(c.execute(Request::MfaEnable(body))),
        [Msg::MfaRefused("ui_admin_error_mfa_code_invalid".into())]
    );
}

#[quench_test::test]
fn a_link_of_an_email_that_stands_for_nothing_is_invalid_not_missing() {
    let (c, seen) = client(answer(404, r#"{"error":"ui_not_found"}"#));
    assert_eq!(
        block_on(c.execute(Request::ConfirmEmail("a b".into()))),
        [Msg::LinkInvalid]
    );
    assert_eq!(
        seen.borrow()[0].path,
        "/api/v1/ui/confirm-email?token=a%20b",
        "the token is encoded"
    );
    assert_eq!(
        block_on(c.execute(Request::UnsubscribeInfo("t".into()))),
        [Msg::LinkInvalid]
    );

    let (c, _) = client(answer(200, r#"{"new_email":"new@example.test"}"#));
    assert!(matches!(
        &block_on(c.execute(Request::ConfirmEmail("t".into())))[0],
        Msg::ConfirmEmail(e) if e.new_email == "new@example.test"
    ));
    let (c, _) = client(answer(200, r#"{"label_key":"ui_notification_x"}"#));
    assert!(matches!(
        &block_on(c.execute(Request::UnsubscribeInfo("t".into())))[0],
        Msg::UnsubscribeInfo(i) if i.label_key == "ui_notification_x"
    ));
}

#[quench_test::test]
fn unsubscribing_posts_the_token_as_a_form_the_way_the_button_always_did() {
    let (c, seen) = client(answer(200, r#"{"done":true}"#));
    assert_eq!(
        block_on(c.execute(Request::Unsubscribe("tok en".into()))),
        [Msg::Unsubscribed]
    );
    let sent = seen.borrow()[0].clone();
    assert_eq!(
        (sent.method, sent.path.as_str()),
        ("POST", "/ui/unsubscribe?token=tok%20en")
    );
    assert!(sent.form);
    assert_eq!(sent.body.as_deref(), Some("unsubscribe=1"));

    let (c, _) = client(answer(404, r#"{"error":"ui_unsubscribe_invalid"}"#));
    assert_eq!(
        block_on(c.execute(Request::Unsubscribe("t".into()))),
        [Msg::LinkInvalid]
    );
    let (c, _) = client(answer(500, ""));
    assert!(matches!(
        block_on(c.execute(Request::Unsubscribe("t".into())))[0],
        Msg::Failed(_)
    ));
}

#[quench_test::test]
fn the_language_is_the_browsers_own_business() {
    let (c, seen) = client(answer(200, "{}"));
    assert!(block_on(c.execute(Request::Language("de-DE".into()))).is_empty());
    assert!(seen.borrow().is_empty());
}

#[quench_test::test]
fn the_stylesheet_is_the_apps() {
    assert!(gatehouse_web::stylesheet().contains(".home-card"));
}

quench_test::main!();
