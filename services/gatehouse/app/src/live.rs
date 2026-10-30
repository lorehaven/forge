//! The app against a host: it asks for the data (`Request`) and is told what came (`Msg`). The host does the talking to the
//! service; the app owns the routes (`{base}/ui/...`), the sidebar and the language.

use std::cell::RefCell;
use std::rc::Rc;

use gatehouse_core::{
    Account, AdminUser, AdminUsers, ConfirmEmail, Dashboard, MfaEnrollRequest, MfaEnrollment,
    UnsubscribeInfo,
};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};

use crate::screens::{self, Env};

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// The cards of the services dashboard.
    Dashboard,
    /// The person's own account.
    Account,
    /// A fresh secret to enrol an authenticator with.
    MfaSecret,
    /// Check the first code and turn MFA on.
    MfaEnable(MfaEnrollRequest),
    /// The user list.
    AdminUsers,
    /// One user, with the catalog.
    AdminUser(String),
    /// The address the confirmation link (this token) would set.
    ConfirmEmail(String),
    /// The kind of notification this unsubscribe link (token) ends.
    UnsubscribeInfo(String),
    /// End that subscription.
    Unsubscribe(String),
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Dashboard(Box<Dashboard>),
    Account(Box<Account>),
    MfaSecret(Box<MfaEnrollment>),
    /// MFA is on: the app goes back to the account page.
    MfaEnabled,
    /// The code was refused; the text is a translation key.
    MfaRefused(String),
    AdminUsers(Box<AdminUsers>),
    AdminUser(Box<AdminUser>),
    ConfirmEmail(ConfirmEmail),
    UnsubscribeInfo(UnsubscribeInfo),
    /// The subscription is ended.
    Unsubscribed,
    /// The link stands for nothing (it was used, or is not genuine).
    LinkInvalid,
    /// There is no such user.
    NotFound,
    /// The person may not see this page.
    Forbidden,
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/gatehouse`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    pub history: Rc<dyn History>,
}

/// The app and the function that feeds it.
pub struct Gatehouse {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Gatehouse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gatehouse").finish_non_exhaustive()
    }
}

/// The languages, each with the name it is chosen by. The first is the fallback.
pub const LANGUAGES: [(&str, &str); 5] = [
    ("en-US", "English"),
    ("de-DE", "Deutsch"),
    ("es-ES", "Español"),
    ("fr-FR", "Français"),
    ("pl-PL", "Polski"),
];

/// The codes alone, for the language picker.
const CODES: [&str; 5] = ["en-US", "de-DE", "es-ES", "fr-FR", "pl-PL"];

const LOCALES: [(&str, &str); 5] = [
    ("en-US", include_str!("../../service/i18n/en-US.ftl")),
    ("de-DE", include_str!("../../service/i18n/de-DE.ftl")),
    ("es-ES", include_str!("../../service/i18n/es-ES.ftl")),
    ("fr-FR", include_str!("../../service/i18n/fr-FR.ftl")),
    ("pl-PL", include_str!("../../service/i18n/pl-PL.ftl")),
];

/// The screens that need no session, by the first segment after `/ui/`.
const PUBLIC: [&str; 8] = [
    "login",
    "register",
    "forgot-password",
    "reset-password",
    "resend-verification",
    "accept-invite",
    "confirm-email",
    "unsubscribe",
];

/// Whether `path` is a screen that needs no session.
#[must_use]
pub fn is_public(base: &str, path: &str) -> bool {
    let rest = path
        .strip_prefix(base)
        .unwrap_or(path)
        .strip_prefix("/ui/")
        .unwrap_or_default();
    let first = rest.split('/').next().unwrap_or_default();
    PUBLIC.contains(&first)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Problem {
    Failed(String),
    SignedOut,
    NotFound,
    Forbidden,
}

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub dashboard: Signal<Option<Dashboard>>,
    pub account: Signal<Option<Account>>,
    pub enrollment: Signal<Option<MfaEnrollment>>,
    pub admin_users: Signal<Option<AdminUsers>>,
    pub admin_user: Signal<Option<AdminUser>>,
    pub confirm: Signal<Option<ConfirmEmail>>,
    pub unsubscribe: Signal<Option<UnsubscribeInfo>>,
    pub unsubscribed: Signal<bool>,
    /// The link of an email stands for nothing.
    pub invalid_link: Signal<bool>,
    pub problem: Signal<Option<Problem>>,
    /// The translation key of why a code was refused.
    pub refused: Signal<Option<String>>,
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn gatehouse(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Gatehouse {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Gatehouse { view, deliver }
}

fn receive(state: &State, router: &Router, base: &str, m: Msg) {
    match m {
        Msg::Dashboard(dashboard) => {
            state.problem.set(None);
            state.dashboard.set(Some(*dashboard));
        }
        Msg::Account(account) => {
            state.problem.set(None);
            state.account.set(Some(*account));
        }
        Msg::MfaSecret(enrollment) => {
            state.problem.set(None);
            state.enrollment.set(Some(*enrollment));
        }
        Msg::MfaEnabled => router.navigate(&format!("{base}/ui/account?ok=mfa_enabled")),
        Msg::MfaRefused(key) => state.refused.set(Some(key)),
        Msg::AdminUsers(users) => {
            state.problem.set(None);
            state.admin_users.set(Some(*users));
        }
        Msg::AdminUser(user) => {
            state.problem.set(None);
            state.admin_user.set(Some(*user));
        }
        Msg::ConfirmEmail(confirm) => state.confirm.set(Some(confirm)),
        Msg::UnsubscribeInfo(info) => state.unsubscribe.set(Some(info)),
        Msg::Unsubscribed => state.unsubscribed.set(true),
        Msg::LinkInvalid => state.invalid_link.set(true),
        Msg::NotFound => state.problem.set(Some(Problem::NotFound)),
        Msg::Forbidden => state.problem.set(Some(Problem::Forbidden)),
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let state = State {
        dashboard: create_signal(None),
        account: create_signal(None),
        enrollment: create_signal(None),
        admin_users: create_signal(None),
        admin_user: create_signal(None),
        confirm: create_signal(None),
        unsubscribe: create_signal(None),
        unsubscribed: create_signal(false),
        invalid_link: create_signal(false),
        problem: create_signal(None),
        refused: create_signal(None),
    };
    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();

    {
        let (state, router, base) = (state.clone(), router.clone(), base.clone());
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| receive(&state, &router, &base, m)));
    }

    let env = Env {
        t: Rc::clone(&i18n),
        base: base.clone(),
        ask: Rc::clone(requests),
    };

    // The screens are drawn in the language of the moment: a change of language draws the open one again.
    let routes = {
        let (env, state, router, i18n, base) = (
            env.clone(),
            state.clone(),
            router.clone(),
            Rc::clone(&i18n),
            base.clone(),
        );
        move || -> View {
            let _ = i18n.language();
            let screen = |draw: fn(&Env, &State, &RouteContext) -> View| {
                let (env, state) = (env.clone(), state.clone());
                move |ctx: &RouteContext| -> View {
                    // A screen starts clean: what the last one left is not its business.
                    state.problem.set(None);
                    state.refused.set(None);
                    draw(&env, &state, ctx)
                }
            };
            let home = screen(screens::home);
            let ui = |rest: &str| format!("{base}/ui{rest}");
            Routes::new()
                .route(&ui("/home"), home.clone())
                .route(&ui("/home/"), home.clone())
                .route(&ui(""), home.clone())
                .route(&ui("/"), home)
                .route(&ui("/login"), screen(screens::login))
                .route(&ui("/login/"), screen(screens::login))
                .route(&ui("/login/mfa"), screen(screens::login_mfa))
                .route(&ui("/register"), screen(screens::register))
                .route(&ui("/register/"), screen(screens::register))
                .route(&ui("/forgot-password"), screen(screens::forgot_password))
                .route(&ui("/forgot-password/"), screen(screens::forgot_password))
                .route(&ui("/reset-password"), screen(screens::reset_password))
                .route(&ui("/resend-verification"), screen(screens::resend))
                .route(&ui("/resend-verification/"), screen(screens::resend))
                .route(&ui("/accept-invite"), screen(screens::accept_invite))
                .route(&ui("/confirm-email"), screen(screens::confirm_email))
                .route(&ui("/unsubscribe"), screen(screens::unsubscribe))
                .route(&ui("/account"), screen(screens::account))
                .route(&ui("/account/mfa/enroll"), screen(screens::mfa_enroll))
                .route(&ui("/admin/users"), screen(screens::admin_users))
                .route(&ui("/admin/users/"), screen(screens::admin_users))
                .route(&ui("/admin/users/:username"), screen(screens::admin_user))
                .not_found({
                    let env = env.clone();
                    move |_| screens::not_found(&env)
                })
                .view(&router)
        }
    };

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Gatehouse",
            icon: "shield",
        },
        image: Some(format!("{base}/ui/assets/favicon.png")),
        home: format!("{base}/ui/home"),
        spa: true,
        account: true,
        brand_in_bar: false,
        logout: format!("{base}/ui/logout"),
        on_logout: None,
        dashboard: false,
        languages: &CODES,
        on_language: {
            let requests = Rc::clone(requests);
            Rc::new(move |code| requests(Request::Language(code.to_string())))
        },
    };
    // Before a session there is no account menu to open.
    let bare = forge_shell::Bar {
        account: false,
        ..bar.clone()
    };
    let public = {
        let (router, base) = (router.clone(), base.clone());
        move || is_public(&base, &router.location().path)
    };

    let top = {
        let (bar, bare, i18n, public) = (bar.clone(), bare, Rc::clone(&i18n), public.clone());
        move || -> View {
            let is_public = public();
            div()
                .class(if is_public {
                    "gh-top gh-public"
                } else {
                    "gh-top"
                })
                .child(forge_shell::topbar(
                    if is_public { &bare } else { &bar },
                    &i18n,
                ))
                .into_view()
        }
    };
    let side = {
        let (bar, env, router, public) = (bar.clone(), env.clone(), router.clone(), public);
        move || -> View {
            if public() {
                return View::Empty;
            }
            forge_shell::sidebar(&bar, vec![screens::sidebar(&env, &router)]).into_view()
        }
    };

    let banner = {
        let (problem, env) = (state.problem.clone(), env.clone());
        View::dynamic(move || screens::banner(&env, problem.get()))
    };

    div()
        .attr("id", "gatehouse")
        .class("fui-app fui-client")
        .child(View::dynamic(side))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(
                    main()
                        .class("gh-main")
                        .child(banner)
                        .child(View::dynamic(routes)),
                ),
        )
        .into_view()
}
