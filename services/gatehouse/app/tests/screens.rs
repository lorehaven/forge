//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use gatehouse_app::{Gatehouse, Msg, Request, Setup, gatehouse};
use gatehouse_core::{
    Account, AdminUser, AdminUserRow, AdminUsers, CatalogService, ConfirmEmail, Dashboard, Grant,
    MfaEnrollRequest, MfaEnrollment, ServiceCard, Subscription, UnsubscribeInfo,
};
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;

type Asked = Rc<RefCell<Vec<Request>>>;

fn find(r: &TestRenderer, tag: &str, attr: (&str, &str)) -> NodeId {
    r.document_order()
        .into_iter()
        .find(|n| {
            r.tag(*n).as_deref() == Some(tag) && r.attr(*n, attr.0).as_deref() == Some(attr.1)
        })
        .unwrap_or_else(|| panic!("no <{tag} {}={}>: {}", attr.0, attr.1, r.html()))
}

fn present(r: &TestRenderer, id: &str) -> bool {
    r.document_order()
        .into_iter()
        .any(|n| r.attr(n, "id").as_deref() == Some(id))
}

struct Live {
    r: Rc<TestRenderer>,
    asked: Asked,
    deliver: Rc<dyn Fn(Msg)>,
    history: Rc<MemoryHistory>,
    _mount: quench_ui::Mount<TestRenderer>,
}

impl Live {
    fn type_into(&self, tag: &str, id: &str, text: &str) {
        self.r
            .fire(find(&self.r, tag, ("id", id)), &Event::Input(text.into()));
    }

    fn submit(&self, id: &str) {
        self.r
            .fire(find(&self.r, "form", ("id", id)), &Event::Submit);
    }

    fn click(&self, tag: &str, id: &str) {
        self.r.fire(find(&self.r, tag, ("id", id)), &Event::Click);
    }

    fn asked(&self) -> Vec<Request> {
        self.asked.borrow().clone()
    }

    fn send(&self, m: Msg) {
        (self.deliver)(m);
    }

    fn text(&self) -> String {
        self.r.text()
    }

    fn html(&self) -> String {
        self.r.html()
    }

    fn attr(&self, tag: &str, id: &str, name: &str) -> Option<String> {
        self.r.attr(find(&self.r, tag, ("id", id)), name)
    }
}

fn live_in(path: &str, language: &str) -> Live {
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(path));
    let Gatehouse { view, deliver } = gatehouse(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/gatehouse".into(),
            language: language.into(),
            history: history.clone(),
        },
    );
    let r = Rc::new(TestRenderer::new());
    let mount = mount(&r, &r.root(), view);
    Live {
        r,
        asked,
        deliver,
        history,
        _mount: mount,
    }
}

fn live(path: &str) -> Live {
    live_in(path, "en-US")
}

/// No translation key may reach the person as text.
fn assert_no_raw_keys(l: &Live) {
    let text = l.text();
    for word in text.split_whitespace() {
        assert!(
            !(word.starts_with("ui_") || word.starts_with("header_")),
            "raw key {word}: {text}"
        );
    }
}

// ── Before a session ─────────────────────────────────────────────────────────

#[quench_test::test]
fn the_login_form_posts_to_the_service_with_the_names_it_reads() {
    let l = live("/gatehouse/ui/login");
    let form = find(&l.r, "form", ("id", "login-form"));
    assert_eq!(
        l.r.attr(form, "action").as_deref(),
        Some("/gatehouse/ui/login")
    );
    assert_eq!(l.r.attr(form, "method").as_deref(), Some("post"));
    for (id, kind) in [("username", "text"), ("password", "password")] {
        assert_eq!(l.attr("input", id, "name").as_deref(), Some(id));
        assert_eq!(l.attr("input", id, "type").as_deref(), Some(kind));
        assert!(l.attr("input", id, "required").is_some());
    }
    let text = l.text();
    assert!(
        text.contains("Sign in") && text.contains("Username"),
        "{text}"
    );
    for (id, href) in [
        ("forgot-link", "/gatehouse/ui/forgot-password"),
        ("register-link", "/gatehouse/ui/register"),
    ] {
        assert_eq!(l.attr("a", id, "href").as_deref(), Some(href));
    }
    assert!(!present(&l.r, "resend-link"));
    assert!(!present(&l.r, "notice"));
    assert!(l.asked().is_empty(), "nothing is asked of the service");
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn where_the_person_was_going_is_carried_through_the_form() {
    let l = live("/gatehouse/ui/login?redirect=%2Fworkbench%2Fui%2Fhome");
    let html = l.html();
    assert!(
        html.contains("name=\"redirect\"") && html.contains("value=\"/workbench/ui/home\""),
        "{html}"
    );
    let l = live("/gatehouse/ui/login");
    assert!(!l.html().contains("name=\"redirect\""));
}

#[quench_test::test]
fn the_login_says_why_the_person_is_back() {
    let l = live("/gatehouse/ui/login?err=1");
    assert!(l.text().contains("Invalid credentials"), "{}", l.text());
    assert!(l.attr("p", "notice", "class").unwrap().contains("error"));

    for (query, said) in [
        ("err=ui_login_account_locked", "Too many failed attempts"),
        ("err=ui_login_rate_limited", "Too many requests"),
        ("err=ui_login_account_disabled", "has been disabled"),
    ] {
        let l = live(&format!("/gatehouse/ui/login?{query}"));
        assert!(l.text().contains(said), "{query}: {}", l.text());
        assert_no_raw_keys(&l);
    }

    let l = live("/gatehouse/ui/login?err=ui_login_email_unverified");
    assert!(present(&l.r, "resend-link"), "a lost mail is likely");
    let l = live("/gatehouse/ui/login?registered=1");
    assert!(l.attr("p", "notice", "class").unwrap().contains("ok"));
    assert!(present(&l.r, "resend-link"));
    let l = live("/gatehouse/ui/login?err=<script>");
    assert!(!present(&l.r, "notice"), "an unknown error shows nothing");
}

#[quench_test::test]
fn the_second_step_of_signing_in_asks_for_the_code() {
    let l = live("/gatehouse/ui/login/mfa?pending=tok&redirect=%2Fa&err=1");
    let form = find(&l.r, "form", ("id", "mfa-form"));
    assert_eq!(
        l.r.attr(form, "action").as_deref(),
        Some("/gatehouse/ui/login/mfa")
    );
    let html = l.html();
    assert!(
        html.contains("name=\"pending\"") && html.contains("value=\"tok\""),
        "{html}"
    );
    assert!(html.contains("name=\"redirect\""));
    assert_eq!(l.attr("input", "code", "name").as_deref(), Some("code"));
    assert!(l.text().contains("code"), "{}", l.text());
    assert!(present(&l.r, "notice"));
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn registration_and_the_password_pages_post_where_the_service_listens() {
    for (path, form, action, fields) in [
        (
            "register",
            "register-form",
            "/gatehouse/ui/register",
            &["username", "email", "password"][..],
        ),
        (
            "forgot-password",
            "forgot-form",
            "/gatehouse/ui/forgot-password",
            &["username"][..],
        ),
        (
            "reset-password?token=abc",
            "reset-form",
            "/gatehouse/ui/reset-password",
            &["password"][..],
        ),
        (
            "resend-verification",
            "resend-form",
            "/gatehouse/ui/resend-verification",
            &["username"][..],
        ),
        (
            "accept-invite?token=abc",
            "invite-form",
            "/gatehouse/ui/accept-invite",
            &["password"][..],
        ),
    ] {
        let l = live(&format!("/gatehouse/ui/{path}"));
        let f = find(&l.r, "form", ("id", form));
        assert_eq!(l.r.attr(f, "action").as_deref(), Some(action), "{path}");
        for field in fields {
            assert_eq!(
                l.attr("input", field, "name").as_deref(),
                Some(*field),
                "{path}"
            );
        }
        if path.contains("token=") {
            assert!(
                l.html().contains("value=\"abc\""),
                "{path}: the token goes back"
            );
        }
        assert_no_raw_keys(&l);
    }
    let l = live("/gatehouse/ui/register?err=ui_admin_error_exists");
    assert!(l.attr("p", "notice", "class").unwrap().contains("error"));
    let l = live("/gatehouse/ui/reset-password?token=abc&err=ui_reset_error_password_empty");
    assert!(present(&l.r, "notice"));
}

#[quench_test::test]
fn a_page_before_a_session_has_no_side_panel_and_no_account_menu() {
    let l = live("/gatehouse/ui/login");
    let html = l.html();
    assert!(!html.contains("fui-sidebar"), "{html}");
    assert!(html.contains("gh-public"));
    assert!(
        !html.contains("data-gh-href=\"/ui/account\""),
        "no menu to open"
    );
    // The language and theme are still there.
    assert!(html.contains("id=\"language\"") && html.contains("data-fui-mode"));
}

#[quench_test::test]
fn the_link_for_a_new_address_shows_it_and_asks_before_changing_it() {
    let l = live("/gatehouse/ui/confirm-email?token=tok");
    assert_eq!(l.asked(), [Request::ConfirmEmail("tok".into())]);
    assert!(present(&l.r, "waiting"));
    l.send(Msg::ConfirmEmail(ConfirmEmail {
        new_email: "new@example.test".into(),
    }));
    assert_eq!(l.text().matches("new@example.test").count(), 1);
    let f = find(&l.r, "form", ("id", "confirm-form"));
    assert_eq!(
        l.r.attr(f, "action").as_deref(),
        Some("/gatehouse/ui/confirm-email")
    );
    assert!(l.html().contains("value=\"tok\""));
    assert_no_raw_keys(&l);

    let l = live("/gatehouse/ui/confirm-email?token=tok");
    l.send(Msg::LinkInvalid);
    assert!(present(&l.r, "invalid-link"));
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn the_notification_link_asks_then_unsubscribes_on_the_button() {
    let l = live("/gatehouse/ui/unsubscribe?token=tok");
    assert_eq!(l.asked(), [Request::UnsubscribeInfo("tok".into())]);
    l.send(Msg::UnsubscribeInfo(UnsubscribeInfo {
        label_key: "ui_notification_conveyor_run_failed".into(),
    }));
    assert!(
        !l.text().contains("ui_notification"),
        "the kind is named in words"
    );
    assert_eq!(l.asked().len(), 1, "opening the link changes nothing");
    l.click("button", "unsubscribe");
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::Unsubscribe("tok".into())
    );
    l.send(Msg::Unsubscribed);
    assert!(present(&l.r, "done"));
    assert_no_raw_keys(&l);

    let l = live("/gatehouse/ui/unsubscribe?token=bad");
    l.send(Msg::LinkInvalid);
    assert!(present(&l.r, "invalid-link"));
}

// ── The dashboard ────────────────────────────────────────────────────────────

fn card(name: &str, url: &str, with_translation: bool) -> ServiceCard {
    let key = name.to_lowercase();
    ServiceCard {
        url: url.into(),
        title_key: if with_translation {
            format!("ui_service_{key}_title")
        } else {
            "ui_service_unheard_title".into()
        },
        desc_key: if with_translation {
            format!("ui_service_{key}_desc")
        } else {
            "ui_service_unheard_desc".into()
        },
        label: name.into(),
        description: Some(format!("About {name}")),
        card_class: format!("home-card-{key}"),
    }
}

#[quench_test::test]
fn the_dashboard_has_a_card_per_service_in_the_order_given() {
    let l = live("/gatehouse/ui/home");
    assert_eq!(l.asked(), [Request::Dashboard]);
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Dashboard(Box::new(Dashboard {
        admin: false,
        services: vec![
            card("Sage", "https://x.test/sage", true),
            card("Workbench", "https://x.test/workbench", true),
            card("Gauge", "https://x.test/gauge", true),
            card("Newcomer", "https://x.test/new", false),
        ],
    })));
    let cards: Vec<_> =
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.attr(*n, "data-service").is_some())
            .collect();
    let names: Vec<_> = cards
        .iter()
        .map(|n| l.r.attr(*n, "data-service").unwrap())
        .collect();
    assert_eq!(names, ["Sage", "Workbench", "Gauge", "Newcomer"]);
    let hrefs: Vec<_> = cards
        .iter()
        .map(|n| l.r.attr(*n, "href").unwrap())
        .collect();
    assert_eq!(
        hrefs,
        [
            "https://x.test/sage",
            "https://x.test/workbench",
            "https://x.test/gauge",
            "https://x.test/new"
        ]
    );
    let text = l.text();
    assert!(
        text.contains("Projects, issues and boards."),
        "the translation: {text}"
    );
    assert!(
        text.contains("About Newcomer"),
        "else the service's own words: {text}"
    );
    assert!(!text.contains("ui_service_"), "{text}");
    assert!(!present(&l.r, "realm-users"), "only for an administrator");
    assert_no_raw_keys(&l);
    assert!(l.html().contains("fui-sidebar"), "the side panel is there");
}

#[quench_test::test]
fn an_administrator_also_gets_the_realm_section_and_an_empty_estate_says_so() {
    let l = live("/gatehouse/ui/home");
    l.send(Msg::Dashboard(Box::new(Dashboard {
        admin: true,
        services: vec![],
    })));
    assert!(present(&l.r, "no-services"));
    let realm = find(&l.r, "a", ("id", "realm-users"));
    assert_eq!(
        l.r.attr(realm, "href").as_deref(),
        Some("/gatehouse/ui/admin/users")
    );
    assert_no_raw_keys(&l);
}

// ── The account ──────────────────────────────────────────────────────────────

fn account() -> Account {
    Account {
        username: "ola".into(),
        display_name: Some("Ola N.".into()),
        avatar_url: Some("https://pics.test/ola.png".into()),
        has_avatar: true,
        title: Some("Engineer".into()),
        timezone: Some("Mars/Olympus".into()),
        preferred_locale: Some("pl-PL".into()),
        email: Some("ola@example.test".into()),
        email_verified: true,
        mfa_enabled: false,
        has_notice_address: true,
        timezones: vec!["Europe/Warsaw".into(), "UTC".into()],
        locales: vec!["en-US".into(), "pl-PL".into()],
        subscriptions: vec![
            Subscription {
                id: "conveyor.run.failed".into(),
                service: "conveyor".into(),
                label_key: "ui_notification_conveyor_run_failed".into(),
                wanted: true,
            },
            Subscription {
                id: "conveyor.run.succeeded".into(),
                service: "conveyor".into(),
                label_key: "ui_notification_conveyor_run_succeeded".into(),
                wanted: false,
            },
        ],
    }
}

#[quench_test::test]
fn the_account_has_a_form_for_each_thing_posted_where_the_service_listens() {
    let l = live("/gatehouse/ui/account");
    assert_eq!(l.asked(), [Request::Account]);
    l.send(Msg::Account(Box::new(account())));
    for (id, action) in [
        ("profile-form", "/gatehouse/ui/account"),
        ("email-form", "/gatehouse/ui/account/email"),
        ("notifications-form", "/gatehouse/ui/account/notifications"),
        ("password-form", "/gatehouse/ui/account/password"),
    ] {
        let f = find(&l.r, "form", ("id", id));
        assert_eq!(l.r.attr(f, "action").as_deref(), Some(action), "{id}");
        assert_eq!(l.r.attr(f, "method").as_deref(), Some("post"));
    }
    let profile = find(&l.r, "form", ("id", "profile-form"));
    assert_eq!(
        l.r.attr(profile, "enctype").as_deref(),
        Some("multipart/form-data"),
        "the picture is a file"
    );
    for name in [
        "display_name",
        "avatar_url",
        "avatar_file",
        "avatar_remove",
        "title",
        "timezone",
        "preferred_locale",
        "email",
        "email_current_password",
        "current_password",
        "new_password",
        "confirm_password",
    ] {
        assert!(l.html().contains(&format!("name=\"{name}\"")), "{name}");
    }
    assert_eq!(
        l.attr("input", "avatar_file", "type").as_deref(),
        Some("file")
    );
    assert!(l.html().contains("value=\"Ola N.\""));
    assert!(l.html().contains("value=\"https://pics.test/ola.png\""));
    assert_eq!(
        l.attr("img", "avatar", "src").as_deref(),
        Some("/gatehouse/ui/account/avatar")
    );
    assert!(l.text().contains("ola@example.test"));
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn a_stored_zone_that_is_not_in_the_list_stays_selectable() {
    let l = live("/gatehouse/ui/account");
    l.send(Msg::Account(Box::new(account())));
    let zone = find(&l.r, "select", ("id", "timezone"));
    let options = l.r.text_of(zone);
    assert!(
        options.contains("Mars/Olympus") && options.contains("Europe/Warsaw"),
        "{options}"
    );
    let selected = l
        .r
        .document_order()
        .into_iter()
        .filter(|n| l.r.tag(*n).as_deref() == Some("option") && l.r.attr(*n, "selected").is_some())
        .map(|n| l.r.attr(n, "value").unwrap())
        .collect::<Vec<_>>();
    assert!(
        selected.contains(&"Mars/Olympus".to_string()),
        "{selected:?}"
    );
    assert!(selected.contains(&"pl-PL".to_string()), "{selected:?}");
}

#[quench_test::test]
fn the_notifications_are_a_checkbox_each_checked_as_chosen() {
    let l = live("/gatehouse/ui/account");
    l.send(Msg::Account(Box::new(account())));
    let on = find(&l.r, "input", ("name", "sub_conveyor.run.failed"));
    assert!(l.r.attr(on, "checked").is_some());
    let off = find(&l.r, "input", ("name", "sub_conveyor.run.succeeded"));
    assert!(l.r.attr(off, "checked").is_none());
    assert!(
        l.text().contains("Conveyor"),
        "grouped by service: {}",
        l.text()
    );
    assert!(!l.text().contains("ui_account_notifications_no_address"));
}

#[quench_test::test]
fn without_a_picture_there_is_nothing_to_remove_and_without_an_address_the_page_says_why() {
    let l = live("/gatehouse/ui/account");
    let mut bare = account();
    bare.has_avatar = false;
    bare.avatar_url = None;
    bare.email = None;
    bare.email_verified = false;
    bare.has_notice_address = false;
    l.send(Msg::Account(Box::new(bare)));
    assert!(!l.html().contains("name=\"avatar_remove\""));
    assert!(!present(&l.r, "avatar"));
    let text = l.text();
    assert!(text.contains("No email address on file"), "{text}");
    assert!(text.contains("which you do not have yet"), "{text}");
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn mfa_is_turned_on_through_its_own_page_and_off_with_a_button() {
    let l = live("/gatehouse/ui/account");
    l.send(Msg::Account(Box::new(account())));
    let enable = find(&l.r, "a", ("id", "mfa-enable"));
    assert_eq!(
        l.r.attr(enable, "href").as_deref(),
        Some("/gatehouse/ui/account/mfa/enroll")
    );
    assert!(!present(&l.r, "mfa-disable-form"));

    let mut on = account();
    on.mfa_enabled = true;
    let l = live("/gatehouse/ui/account");
    l.send(Msg::Account(Box::new(on)));
    let f = find(&l.r, "form", ("id", "mfa-disable-form"));
    assert_eq!(
        l.r.attr(f, "action").as_deref(),
        Some("/gatehouse/ui/account/mfa/disable")
    );
}

#[quench_test::test]
fn the_account_shows_what_the_last_post_came_back_with() {
    let l = live("/gatehouse/ui/account?ok=saved");
    l.send(Msg::Account(Box::new(account())));
    assert!(l.attr("p", "notice", "class").unwrap().contains("ok"));
    assert_no_raw_keys(&l);
    for (query, error) in [
        ("err=ui_account_error_current_password", true),
        ("err=ui_account_error_password_mismatch", true),
        ("err=ui_account_error_avatar_size", true),
        ("err=ui_account_error_rate_limited", true),
        ("ok=password_changed", false),
        ("ok=mfa_enabled", false),
        ("ok=email_change_sent", false),
        ("ok=notifications_saved", false),
    ] {
        let l = live(&format!("/gatehouse/ui/account?{query}"));
        l.send(Msg::Account(Box::new(account())));
        let class = l
            .attr("p", "notice", "class")
            .unwrap_or_else(|| panic!("{query}"));
        assert_eq!(class.contains("error"), error, "{query}");
        assert_no_raw_keys(&l);
    }
}

#[quench_test::test]
fn enrolling_checks_the_code_through_the_api_and_keeps_the_secret_on_a_wrong_one() {
    let l = live("/gatehouse/ui/account/mfa/enroll");
    assert_eq!(l.asked(), [Request::MfaSecret]);
    l.send(Msg::MfaSecret(Box::new(MfaEnrollment {
        secret: "JBSWY3DP".into(),
        uri: "otpauth://totp/Forge:ola?secret=JBSWY3DP".into(),
        qr: Some("data:image/png;base64,AAAA".into()),
    })));
    assert!(l.text().contains("JBSWY3DP"));
    assert_eq!(
        l.attr("img", "qr", "src").as_deref(),
        Some("data:image/png;base64,AAAA")
    );

    // A blank code is not sent.
    l.submit("mfa-enroll-form");
    assert_eq!(l.asked().len(), 1);
    assert!(present(&l.r, "mfa-error"));

    l.type_into("input", "code", " 123 456 ");
    l.submit("mfa-enroll-form");
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::MfaEnable(MfaEnrollRequest {
            secret: "JBSWY3DP".into(),
            code: "123456".into()
        })
    );
    l.send(Msg::MfaRefused("ui_admin_error_mfa_code_invalid".into()));
    assert!(l.text().contains("JBSWY3DP"), "the same secret stays");
    assert!(present(&l.r, "mfa-error"));
    assert_no_raw_keys(&l);

    l.send(Msg::MfaEnabled);
    assert_eq!(
        l.history.location().url(),
        "/gatehouse/ui/account?ok=mfa_enabled"
    );
    assert_eq!(l.asked().last().unwrap(), &Request::Account);
}

// ── The administration ───────────────────────────────────────────────────────

fn users(can_create: bool, admin: bool) -> AdminUsers {
    AdminUsers {
        actor: "root".into(),
        admin,
        can_create,
        users: vec![
            AdminUserRow {
                username: "root".into(),
                roles: vec!["admin".into()],
                wildcard: true,
                grants: vec![],
            },
            AdminUserRow {
                username: "ola k".into(),
                roles: vec!["user".into()],
                wildcard: false,
                grants: vec![Grant {
                    service: "sage".into(),
                    actions: vec!["read".into(), "write".into()],
                }],
            },
            AdminUserRow {
                username: "nobody".into(),
                roles: vec!["user".into()],
                wildcard: false,
                grants: vec![],
            },
        ],
    }
}

#[quench_test::test]
fn the_user_list_shows_roles_and_grants_and_links_to_each_editor() {
    let l = live("/gatehouse/ui/admin/users");
    assert_eq!(l.asked(), [Request::AdminUsers]);
    l.send(Msg::AdminUsers(Box::new(users(true, true))));
    let text = l.text();
    assert!(text.contains("sage: read+write"), "{text}");
    assert!(text.contains("root") && text.contains("admin"), "{text}");
    assert_eq!(text.matches("you").count(), 1, "only the viewer: {text}");
    let row = find(&l.r, "div", ("data-user", "ola k"));
    assert!(l.r.text_of(row).contains("sage: read+write"));
    let editors =
        l.r.document_order()
            .into_iter()
            .filter_map(|n| l.r.attr(n, "href"))
            .filter(|h| h.starts_with("/gatehouse/ui/admin/users/"))
            .collect::<Vec<_>>();
    assert_eq!(
        editors,
        [
            "/gatehouse/ui/admin/users/root",
            "/gatehouse/ui/admin/users/ola%20k",
            "/gatehouse/ui/admin/users/nobody"
        ],
        "an editor link per user, percent-encoded"
    );
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn the_create_form_is_only_for_those_who_may_create_and_only_an_admin_picks_roles() {
    let l = live("/gatehouse/ui/admin/users");
    l.send(Msg::AdminUsers(Box::new(users(false, false))));
    assert!(!present(&l.r, "create-form"));

    let l = live("/gatehouse/ui/admin/users");
    l.send(Msg::AdminUsers(Box::new(users(true, false))));
    let f = find(&l.r, "form", ("id", "create-form"));
    assert_eq!(
        l.r.attr(f, "action").as_deref(),
        Some("/gatehouse/ui/admin/users")
    );
    for name in ["username", "password", "email"] {
        assert!(l.html().contains(&format!("name=\"{name}\"")), "{name}");
    }
    assert!(!present(&l.r, "new-role"));
    assert!(
        l.attr("input", "new-password", "required").is_none(),
        "an invited person picks their own"
    );

    let l = live("/gatehouse/ui/admin/users");
    l.send(Msg::AdminUsers(Box::new(users(true, true))));
    assert!(present(&l.r, "new-role"));
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn what_a_redirect_says_is_shown_on_the_list() {
    let l = live("/gatehouse/ui/admin/users?ok=deleted");
    l.send(Msg::AdminUsers(Box::new(users(true, true))));
    assert!(l.attr("p", "notice", "class").unwrap().contains("ok"));
    let l = live("/gatehouse/ui/admin/users?err=ui_admin_error_exists");
    l.send(Msg::AdminUsers(Box::new(users(true, true))));
    assert!(l.attr("p", "notice", "class").unwrap().contains("error"));
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn a_refused_read_says_so_in_place_of_the_page() {
    let l = live("/gatehouse/ui/admin/users");
    l.send(Msg::Forbidden);
    assert!(present(&l.r, "forbidden"));
    assert_no_raw_keys(&l);
}

fn user(name: &str) -> AdminUser {
    AdminUser {
        username: name.into(),
        role: "user".into(),
        wildcard: false,
        held: vec![Grant {
            service: "sage".into(),
            actions: vec!["read".into()],
        }],
        email: Some("ola@example.test".into()),
        email_verified: false,
        disabled: false,
        locked: true,
        mfa_enabled: true,
        created_at: "2026-10-01 10:00 UTC".into(),
        last_login_at: None,
        is_self: false,
        viewer_admin: true,
        can_edit: true,
        can_delete: true,
        can_manage_permissions: true,
        services: vec![
            CatalogService {
                name: "sage".into(),
                label: "Sage".into(),
                actions: vec!["read".into(), "write".into()],
            },
            CatalogService {
                name: "gantry".into(),
                label: "Gantry".into(),
                actions: vec!["read".into()],
            },
        ],
        templates: vec!["viewer".into(), "editor".into()],
    }
}

#[quench_test::test]
fn the_editor_has_the_role_the_grants_and_every_action_the_viewer_may_take() {
    let l = live("/gatehouse/ui/admin/users/ola%20k");
    assert_eq!(l.asked(), [Request::AdminUser("ola k".into())]);
    l.send(Msg::AdminUser(Box::new(user("ola k"))));

    let f = find(&l.r, "form", ("id", "edit-form"));
    assert_eq!(
        l.r.attr(f, "action").as_deref(),
        Some("/gatehouse/ui/admin/users/ola%20k")
    );
    assert!(
        l.attr("select", "role", "name").is_some(),
        "an admin picks the role"
    );
    let read = find(&l.r, "input", ("name", "perm_sage_read"));
    assert!(l.r.attr(read, "checked").is_some());
    let write = find(&l.r, "input", ("name", "perm_sage_write"));
    assert!(l.r.attr(write, "checked").is_none());
    assert!(l.r.attr(write, "disabled").is_none());
    find(&l.r, "input", ("name", "perm_gantry_read"));
    assert!(l.html().contains("name=\"password\""));

    // The bundles, the state, the actions that undo it, the delete.
    let t = find(&l.r, "form", ("id", "template-form"));
    assert_eq!(
        l.r.attr(t, "action").as_deref(),
        Some("/gatehouse/ui/admin/users/ola%20k/template")
    );
    let html = l.html();
    for suffix in ["disable", "unlock", "mfa/disable", "invite"] {
        assert!(
            html.contains(&format!("/gatehouse/ui/admin/users/ola%20k/{suffix}\"")),
            "{suffix}"
        );
    }
    assert!(!html.contains("users/ola%20k/enable\""));
    let d = find(&l.r, "form", ("id", "delete-form"));
    assert_eq!(
        l.r.attr(d, "action").as_deref(),
        Some("/gatehouse/ui/admin/users/ola%20k/delete")
    );
    assert!(l.text().contains("2026-10-01 10:00 UTC"));
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn a_wildcard_user_shows_every_action_held_and_locked_and_needs_no_bundle() {
    let l = live("/gatehouse/ui/admin/users/root");
    let mut root = user("root");
    root.wildcard = true;
    root.role = "admin".into();
    root.is_self = true;
    root.locked = false;
    root.mfa_enabled = false;
    root.email_verified = true;
    l.send(Msg::AdminUser(Box::new(root)));
    let write = find(&l.r, "input", ("name", "perm_sage_write"));
    assert!(l.r.attr(write, "checked").is_some());
    assert!(l.r.attr(write, "disabled").is_some());
    assert!(
        !present(&l.r, "template-form"),
        "already reaches everything"
    );
    let html = l.html();
    assert!(!html.contains("users/root/disable\""), "no self-disable");
    assert!(!html.contains("users/root/unlock\"") && !html.contains("users/root/invite\""));
    assert!(!present(&l.r, "delete-form"), "no self-delete");
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn a_disabled_account_can_be_enabled_again() {
    let l = live("/gatehouse/ui/admin/users/ola");
    let mut ola = user("ola");
    ola.disabled = true;
    l.send(Msg::AdminUser(Box::new(ola)));
    let html = l.html();
    assert!(html.contains("users/ola/enable\"") && !html.contains("users/ola/disable\""));
}

#[quench_test::test]
fn a_viewer_who_cannot_edit_sees_the_information_and_no_forms() {
    let l = live("/gatehouse/ui/admin/users/ola");
    let mut ola = user("ola");
    ola.can_edit = false;
    ola.can_delete = false;
    ola.can_manage_permissions = false;
    ola.viewer_admin = false;
    l.send(Msg::AdminUser(Box::new(ola)));
    assert!(
        !present(&l.r, "edit-form")
            && !present(&l.r, "template-form")
            && !present(&l.r, "delete-form")
    );
    assert!(present(&l.r, "role"), "the role as plain text");
    find(&l.r, "span", ("id", "role"));
    let read = find(&l.r, "input", ("name", "perm_sage_read"));
    assert!(l.r.attr(read, "disabled").is_some());
    assert!(!l.html().contains("<form"), "{}", l.html());
    assert_no_raw_keys(&l);
}

#[quench_test::test]
fn an_unknown_user_is_not_found() {
    let l = live("/gatehouse/ui/admin/users/ghost");
    l.send(Msg::NotFound);
    assert!(present(&l.r, "not-found"));
}

// ── The frame ────────────────────────────────────────────────────────────────

#[quench_test::test]
fn a_session_that_ended_offers_the_way_back_to_where_the_person_was() {
    let l = live("/gatehouse/ui/account");
    l.send(Msg::SignedOut);
    let a = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(
        l.r.attr(a, "href").as_deref(),
        Some("/gatehouse/ui/login?redirect=%2Fgatehouse%2Fui%2Faccount")
    );
    l.send(Msg::Failed("cannot reach the service".into()));
    assert!(l.text().contains("cannot reach the service"));
}

#[quench_test::test]
fn the_side_panel_marks_the_open_page_and_the_address_without_a_page_is_not_found() {
    let l = live("/gatehouse/ui/admin/users");
    let users = find(&l.r, "a", ("id", "nav-users"));
    assert_eq!(l.r.attr(users, "aria-current").as_deref(), Some("page"));
    let services = find(&l.r, "a", ("id", "nav-services"));
    assert!(l.r.attr(services, "aria-current").is_none());

    let l = live("/gatehouse/ui/nowhere");
    assert!(present(&l.r, "not-found"));
}

#[quench_test::test]
fn the_languages_follow_the_choice_and_report_it() {
    let l = live_in("/gatehouse/ui/login", "pl-PL");
    assert!(
        l.text().contains("Zaloguj") || l.text().contains("Nazwa użytkownika"),
        "{}",
        l.text()
    );
    assert_no_raw_keys(&l);
    l.r.fire(
        find(&l.r, "select", ("id", "language")),
        &Event::Input("de-DE".into()),
    );
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::Language("de-DE".into())
    );
    assert!(
        l.text().contains("Benutzername") || l.text().contains("Anmelden"),
        "{}",
        l.text()
    );
}

#[quench_test::test]
fn every_screen_reads_in_every_language_without_raw_keys() {
    for language in ["en-US", "pl-PL", "es-ES", "de-DE", "fr-FR"] {
        for path in [
            "login?err=ui_login_account_locked",
            "login/mfa?pending=x&err=1",
            "register?err=ui_register_error_rate_limited",
            "forgot-password",
            "reset-password?token=x",
            "resend-verification",
            "accept-invite?token=x",
        ] {
            let l = live_in(&format!("/gatehouse/ui/{path}"), language);
            assert_no_raw_keys(&l);
        }
        let l = live_in("/gatehouse/ui/home", language);
        l.send(Msg::Dashboard(Box::new(Dashboard {
            admin: true,
            services: vec![card("Gantry", "https://x.test/gantry", true)],
        })));
        assert_no_raw_keys(&l);
        let l = live_in("/gatehouse/ui/account?ok=saved", language);
        l.send(Msg::Account(Box::new(account())));
        assert_no_raw_keys(&l);
        let l = live_in("/gatehouse/ui/admin/users/ola", language);
        l.send(Msg::AdminUser(Box::new(user("ola"))));
        assert_no_raw_keys(&l);
        let l = live_in("/gatehouse/ui/admin/users", language);
        l.send(Msg::AdminUsers(Box::new(users(true, true))));
        assert_no_raw_keys(&l);
    }
}

quench_test::main!();
