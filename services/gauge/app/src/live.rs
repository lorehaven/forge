//! The app against a host: it asks for the numbers (`Request`) and is told what came (`Msg`). The host does the talking to
//! the service; the app owns the routes (`{base}/ui/home`, `{base}/ui/hosts/{name}`), the polling and the language.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gauge_core::{HostCard, HostDetail};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};

use crate::screens;

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The list of hosts, with their CPU history.
    Hosts,
    /// One host, with its history.
    Host(String),
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Hosts(Vec<HostCard>),
    Detail(Box<HostDetail>),
    /// There is no such host.
    NotFound(String),
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/gauge`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
    /// Unix seconds now, for "last seen ... ago".
    pub now: Rc<dyn Fn() -> f64>,
    /// How often the open screen asks again.
    pub refresh_ms: u32,
}

/// The app and the function that feeds it.
pub struct Gauge {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Gauge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gauge").finish_non_exhaustive()
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    Failed(String),
    SignedOut,
    NotFound(String),
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn gauge(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Gauge {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Gauge { view, deliver }
}

/// Asks again every `every_ms` until the screen that asked is gone.
fn poll(every_ms: u32, ask: Rc<dyn Fn()>) {
    fn tick(every_ms: u32, ask: Rc<dyn Fn()>, alive: Rc<Cell<bool>>) {
        let _ = quench_ui::platform::after(every_ms, move || {
            if alive.get() {
                ask();
                tick(every_ms, ask, alive);
            }
        });
    }
    let alive = Rc::new(Cell::new(true));
    let gone = Rc::clone(&alive);
    on_cleanup(move || gone.set(false));
    tick(every_ms, ask, alive);
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let hosts: Signal<Option<Vec<HostCard>>> = create_signal(None);
    let detail: Signal<Option<HostDetail>> = create_signal(None);
    let problem: Signal<Option<Problem>> = create_signal(None);

    {
        let (hosts, detail, problem) = (hosts.clone(), detail.clone(), problem.clone());
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| match m {
            Msg::Hosts(list) => {
                problem.set(None);
                hosts.set(Some(list));
            }
            Msg::Detail(d) => {
                problem.set(None);
                detail.set(Some(*d));
            }
            Msg::NotFound(name) => problem.set(Some(Problem::NotFound(name))),
            Msg::Failed(why) => problem.set(Some(Problem::Failed(why))),
            Msg::SignedOut => problem.set(Some(Problem::SignedOut)),
        }));
    }

    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();
    let refresh = setup.refresh_ms;

    let home = {
        let (i18n, hosts, requests, base, now) = (
            Rc::clone(&i18n),
            hosts.clone(),
            Rc::clone(requests),
            base.clone(),
            Rc::clone(&setup.now),
        );
        move |_: &RouteContext| -> View {
            requests(Request::Hosts);
            let again = Rc::clone(&requests);
            poll(refresh, Rc::new(move || again(Request::Hosts)));
            let (i18n, hosts, base, now) = (
                Rc::clone(&i18n),
                hosts.clone(),
                base.clone(),
                Rc::clone(&now),
            );
            View::dynamic(move || screens::hosts(&i18n, hosts.get().as_deref(), &base, now()))
        }
    };
    let host = {
        let (i18n, detail, problem, requests, base, now) = (
            Rc::clone(&i18n),
            detail.clone(),
            problem.clone(),
            Rc::clone(requests),
            base.clone(),
            Rc::clone(&setup.now),
        );
        move |ctx: &RouteContext| -> View {
            let name = ctx.param("name").unwrap_or_default();
            requests(Request::Host(name.clone()));
            let (again, who) = (Rc::clone(&requests), name.clone());
            poll(refresh, Rc::new(move || again(Request::Host(who.clone()))));
            let (i18n, detail, problem, base, now) = (
                Rc::clone(&i18n),
                detail.clone(),
                problem.clone(),
                base.clone(),
                Rc::clone(&now),
            );
            View::dynamic(move || {
                if problem.get() == Some(Problem::NotFound(name.clone())) {
                    return screens::not_found(&i18n, &base);
                }
                match detail.get() {
                    Some(d) if d.host.name == name => screens::host(&i18n, &d, &base, now()),
                    _ => p()
                        .attr("id", "waiting")
                        .class("fui-empty")
                        .text(i18n.t("ui_waiting"))
                        .into_view(),
                }
            })
        }
    };
    let routes = Routes::new()
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home)
        .route(&format!("{base}/ui/hosts/:name"), host)
        .not_found({
            let (i18n, base) = (Rc::clone(&i18n), base.clone());
            move |_| screens::not_found(&i18n, &base)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Gauge",
            icon: "eye",
        },
        image: Some(format!("{base}/ui/assets/favicon.png")),
        home: format!("{base}/ui/home"),
        spa: true,
        account: true,
        brand_in_bar: false,
        logout: format!("{base}/ui/logout"),
        on_logout: None,
        dashboard: true,
        languages: &CODES,
        on_language: {
            let requests = Rc::clone(requests);
            Rc::new(move |code| requests(Request::Language(code.to_string())))
        },
    };
    let top = {
        let (bar, i18n) = (bar.clone(), Rc::clone(&i18n));
        move || -> View { forge_shell::topbar(&bar, &i18n).into_view() }
    };

    let banner = {
        let (problem, login) = (problem.clone(), setup.login_url.clone());
        View::dynamic(move || match problem.get() {
            Some(Problem::Failed(why)) => p()
                .attr("id", "problem")
                .class("fui-notice fui-notice-error")
                .text(why)
                .into_view(),
            Some(Problem::SignedOut) => p()
                .attr("id", "problem")
                .class("fui-notice fui-notice-error")
                .child(span().text("Your session ended. "))
                .child(
                    a().attr("id", "sign-in")
                        .attr("href", login.clone())
                        .text("Sign in"),
                )
                .into_view(),
            _ => View::Empty,
        })
    };

    div()
        .attr("id", "gauge")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(&bar, vec![]))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(
                    main()
                        .class("gg-main")
                        .child(banner)
                        .child(routes.view(&router)),
                ),
        )
        .into_view()
}
