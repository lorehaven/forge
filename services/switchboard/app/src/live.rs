//! The app against a host: it asks for the data and for every change (`Request`) and is told what came (`Msg`). The host
//! does the talking to the service; the app owns the routes (`{base}/ui/home`, `{base}/ui/models/dashboard`,
//! `{base}/ui/vllm/manage`), the polling, the sidebar and the language.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};
use switchboard_core::{
    Capabilities, FilterDraft, Gpu, Instance, LaunchBody, LaunchDraft, Model, ModelFilters,
};

use crate::screens::{self, Env};

/// How often the GPU's memory is read again, and the instances.
pub const GPU_EVERY_MS: u32 = 1000;
pub const INSTANCES_EVERY_MS: u32 = 2000;

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// What is switched on and what the person may do: once, when the app starts.
    Startup,
    /// The GPU's memory.
    Gpu,
    /// The models the filters leave.
    Models(ModelFilters),
    /// Every model, for the launch form's list.
    Catalog,
    /// Remove a model from the drive.
    DeleteModel(String),
    Instances,
    Launch(LaunchBody),
    /// Stop the instance with this id.
    Stop(String),
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Capabilities(Capabilities),
    Gpu(Gpu),
    /// The models for these filters.
    Models(ModelFilters, Vec<Model>),
    Catalog(Vec<Model>),
    /// The model at this path is gone from the drive.
    ModelDeleted(String),
    Instances(Vec<Instance>),
    /// An instance was started.
    Launched,
    /// The instance with this id was told to stop.
    Stopped(String),
    /// The service refused or failed a change; the text is for the person (or a translation key).
    Rejected(String),
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/switchboard`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
}

/// The app and the function that feeds it.
pub struct Switchboard {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Switchboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Switchboard").finish_non_exhaustive()
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
pub(crate) enum Problem {
    Failed(String),
    SignedOut,
}

/// The estimates window of one model: which, and its three filters.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Estimates {
    pub model: Model,
    pub fit: String,
    pub context: String,
    pub quant: String,
}

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub caps: Signal<Option<Capabilities>>,
    pub gpu: Signal<Option<Gpu>>,
    pub models: Signal<Option<Vec<Model>>>,
    pub filter: Signal<FilterDraft>,
    pub catalog: Signal<Vec<Model>>,
    pub instances: Signal<Option<Vec<Instance>>>,
    pub problem: Signal<Option<Problem>>,
    /// A refused change, until the person does something else.
    pub notice: Signal<Option<String>>,
    pub estimates: Signal<Option<Estimates>>,
    /// The model to delete: path and name.
    pub deleting: Signal<Option<(String, String)>>,
    /// The instance to stop: id and model.
    pub stopping: Signal<Option<(String, String)>>,
    pub launch_open: Signal<bool>,
    pub draft: Signal<LaunchDraft>,
    /// Why the launch form was not sent, or the service refused it.
    pub launch_error: Signal<Option<String>>,
    pub launching: Signal<bool>,
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn switchboard(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Switchboard {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Switchboard { view, deliver }
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

fn receive(state: &State, requests: &Rc<dyn Fn(Request)>, m: Msg) {
    match m {
        Msg::Capabilities(caps) => state.caps.set(Some(caps)),
        Msg::Gpu(gpu) => {
            state.problem.set(None);
            state.gpu.set(Some(gpu));
        }
        Msg::Models(filters, list) => {
            state.problem.set(None);
            // Answers can arrive out of order while the person is still typing: only the latest filters count.
            if state.filter.with_untracked(FilterDraft::body) == filters {
                state.models.set(Some(list));
            }
        }
        Msg::Catalog(list) => state.catalog.set(list),
        Msg::ModelDeleted(_) => {
            state.notice.set(None);
            state.deleting.set(None);
            requests(Request::Models(
                state.filter.with_untracked(FilterDraft::body),
            ));
            requests(Request::Catalog);
        }
        Msg::Instances(list) => {
            state.problem.set(None);
            state.instances.set(Some(list));
        }
        Msg::Launched => {
            state.notice.set(None);
            state.launching.set(false);
            state.launch_open.set(false);
            requests(Request::Instances);
        }
        Msg::Stopped(_) => {
            state.notice.set(None);
            state.stopping.set(None);
            requests(Request::Instances);
        }
        Msg::Rejected(why) => {
            if state.launching.get_untracked() {
                state.launching.set(false);
                state.launch_error.set(Some(why));
            } else {
                state.deleting.set(None);
                state.stopping.set(None);
                state.notice.set(Some(why));
            }
        }
        Msg::Failed(why) => {
            state.launching.set(false);
            state.problem.set(Some(Problem::Failed(why)));
        }
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

fn new_state() -> State {
    State {
        caps: create_signal(None),
        gpu: create_signal(None),
        models: create_signal(None),
        filter: create_signal(FilterDraft::default()),
        catalog: create_signal(Vec::new()),
        instances: create_signal(None),
        problem: create_signal(None),
        notice: create_signal(None),
        estimates: create_signal(None),
        deleting: create_signal(None),
        stopping: create_signal(None),
        launch_open: create_signal(false),
        draft: create_signal(LaunchDraft::new("")),
        launch_error: create_signal(None),
        launching: create_signal(false),
    }
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let state = new_state();
    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();

    {
        let (state, requests) = (state.clone(), Rc::clone(requests));
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| receive(&state, &requests, m)));
    }
    requests(Request::Startup);

    let env = Env {
        t: Rc::clone(&i18n),
        base: base.clone(),
        ask: Rc::clone(requests),
    };

    let home = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.notice.set(None);
            screens::home(&env, &state)
        }
    };
    let models = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.notice.set(None);
            state.models.set(None);
            state.estimates.set(None);
            state.deleting.set(None);
            let ask = Rc::clone(&env.ask);
            ask(Request::Gpu);
            ask(Request::Models(
                state.filter.with_untracked(FilterDraft::body),
            ));
            let again = Rc::clone(&ask);
            poll(GPU_EVERY_MS, Rc::new(move || again(Request::Gpu)));
            screens::models(&env, &state)
        }
    };
    let vllm = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.notice.set(None);
            state.launch_open.set(false);
            state.stopping.set(None);
            let ask = Rc::clone(&env.ask);
            ask(Request::Gpu);
            ask(Request::Instances);
            ask(Request::Catalog);
            let (gpu_again, instances_again) = (Rc::clone(&ask), Rc::clone(&ask));
            poll(GPU_EVERY_MS, Rc::new(move || gpu_again(Request::Gpu)));
            poll(
                INSTANCES_EVERY_MS,
                Rc::new(move || instances_again(Request::Instances)),
            );
            screens::vllm(&env, &state)
        }
    };
    let routes = Routes::new()
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home)
        .route(&format!("{base}/ui/models/dashboard"), models.clone())
        .route(&format!("{base}/ui/models/dashboard/"), models)
        .route(&format!("{base}/ui/vllm/manage"), vllm.clone())
        .route(&format!("{base}/ui/vllm/manage/"), vllm)
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Switchboard",
            icon: "shuffle",
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
        let (problem, notice, login, i18n) = (
            state.problem.clone(),
            state.notice.clone(),
            setup.login_url.clone(),
            Rc::clone(&i18n),
        );
        View::dynamic(move || match (problem.get(), notice.get()) {
            (Some(Problem::Failed(why)), _) => p()
                .attr("id", "problem")
                .class("sb-notice")
                .text(why)
                .into_view(),
            (Some(Problem::SignedOut), _) => p()
                .attr("id", "problem")
                .class("sb-notice")
                .child(span().text(format!("{} ", i18n.t("ui_session_ended"))))
                .child(
                    a().attr("id", "sign-in")
                        .attr("href", login.clone())
                        .text(i18n.t("ui_login_sign_in")),
                )
                .into_view(),
            (_, Some(why)) => p()
                .attr("id", "notice")
                .class("sb-notice")
                .text(screens::reason(&i18n, &why))
                .into_view(),
            _ => View::Empty,
        })
    };

    div()
        .attr("id", "switchboard")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(
            &bar,
            vec![screens::sidebar(&env, &state, &router)],
        ))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(
                    main()
                        .class("sb-main")
                        .child(banner)
                        .child(routes.view(&router)),
                ),
        )
        .into_view()
}
