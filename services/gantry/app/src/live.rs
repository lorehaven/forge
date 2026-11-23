//! The app against a host.
//!
//! It asks for the data and for every change (`Request`) and is told what came (`Msg`). The host does the talking to
//! the service; the app owns the routes (`{base}/ui/home`, `/apps/{package}`, `/targets/{package}`, `/operations`,
//! `/operations/{id}`, `/plans/{id}`, `/resource?...`), the sidebar, the polling of a running operation and the
//! language.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gantry_core::{
    Busy, ClusterEntry, ClusterKind, ClusterObject, ClusterOverview, ClusterRef, DeploymentView,
    ExecRead, FacadeEntry, Group, Info, Operation, ResourceRef, ResourceYaml, StoredPlan,
    TargetView,
};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};

use crate::screens::{self, Env};

/// How often a running operation is asked for again.
const POLL_MS: u32 = 3000;
/// How often a list of the cluster's objects is asked for again.
const CLUSTER_POLL_MS: u32 = 5000;
/// How often the list of services is asked for again.
const FACADE_POLL_MS: u32 = 10_000;
/// How often an open shell is asked what it printed.
const EXEC_POLL_MS: u32 = 700;

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Who the person is and what they may do anywhere: once, when the app starts.
    Startup,
    /// The packages, and the change in progress if any.
    Packages,
    /// One package: its resources and its deployments.
    Package(String),
    /// One package: what runs, what is published.
    Target(String),
    Operations,
    Operation(String),
    Plan(String),
    /// The live YAML of a resource.
    Yaml(ResourceRef),
    /// The kinds of object the cluster can be browsed by.
    ClusterKinds,
    /// The cluster in a few numbers.
    ClusterOverview,
    /// What the cluster serves, as links.
    Facade,
    /// Every object of a kind, in a namespace (empty: all).
    ClusterList {
        kind: String,
        namespace: String,
    },
    /// One object, with its events.
    ClusterObject(ClusterRef),
    /// Which namespaces a shell may be opened in.
    ExecEnabled,
    /// Open a shell in a pod's container (empty: the first).
    ExecOpen {
        object: ClusterRef,
        container: String,
    },
    /// Type into a shell.
    ExecInput {
        id: String,
        data: String,
    },
    /// What a shell printed since.
    ExecPoll {
        id: String,
        since: u64,
    },
    ExecClose(String),
    /// Delete an object.
    ClusterDelete(ClusterRef),
    /// Set the replicas of a workload.
    ClusterScale {
        object: ClusterRef,
        replicas: u32,
    },
    /// Apply an object's edited YAML.
    ClusterEdit {
        object: ClusterRef,
        yaml: String,
    },
    /// The log of a pod's container.
    ClusterLog {
        namespace: String,
        pod: String,
        container: String,
        previous: bool,
    },
    /// Start or stop a deployment, now.
    Start(String),
    Stop(String),
    /// Put a package at a version (the newest when none), now.
    Sync {
        package: String,
        version: Option<String>,
    },
    /// Apply one resource again from its package.
    ApplyResource(ResourceRef),
    /// Apply everything the package declares that is missing.
    ApplyMissing(String),
    Refresh(String),
    DeleteResource(ResourceRef),
    Edit {
        resource: ResourceRef,
        yaml: String,
    },
    Cancel(String),
    /// Run a stored plan.
    Confirm(String),
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Info(Info),
    Groups(Vec<Group>),
    /// The change in progress, if there is one.
    Busy(Option<Busy>),
    Deployments(Vec<DeploymentView>),
    Target(Box<TargetView>),
    Operations(Vec<Operation>),
    Operation(Box<Operation>),
    Plan(Box<StoredPlan>),
    Yaml(Box<ResourceYaml>),
    ClusterKinds(Vec<ClusterKind>),
    ClusterOverview(Box<ClusterOverview>),
    Facade(Vec<FacadeEntry>),
    /// The namespaces a shell may be opened in.
    ExecEnabled(Vec<String>),
    /// A shell was opened.
    ExecOpened(String),
    ExecOutput(ExecRead),
    ClusterList(Vec<ClusterEntry>),
    ClusterObject(Box<ClusterObject>),
    ClusterLog(String),
    /// A change was queued: the app opens its operation.
    Started(String),
    /// A change went through (the text is for the person).
    Done(String),
    /// The service refused or failed a change; the text is for the person.
    Rejected(String),
    /// There is no such package, plan or operation.
    NotFound,
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/gantry`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
}

impl std::fmt::Debug for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup")
            .field("base", &self.base)
            .field("language", &self.language)
            .field("login_url", &self.login_url)
            .finish_non_exhaustive()
    }
}

/// The app and the function that feeds it.
pub struct Gantry {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Gantry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gantry").finish_non_exhaustive()
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
    NotFound,
}

/// What a change that was refused or went through says, until the person does something else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    pub ok: bool,
    pub text: String,
}

/// A change waiting for the person to say yes: the question names the thing.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pending {
    pub question: String,
    pub request: Request,
}

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub info: Signal<Info>,
    pub groups: Signal<Option<Vec<Group>>>,
    pub busy: Signal<Option<Busy>>,
    pub deployments: Signal<Vec<DeploymentView>>,
    pub target: Signal<Option<TargetView>>,
    pub operations: Signal<Option<Vec<Operation>>>,
    pub operation: Signal<Option<Operation>>,
    pub plan: Signal<Option<StoredPlan>>,
    pub yaml: Signal<Option<ResourceYaml>>,
    pub cluster_kinds: Signal<Vec<ClusterKind>>,
    pub cluster_overview: Signal<Option<ClusterOverview>>,
    pub facade: Signal<Option<Vec<FacadeEntry>>>,
    pub exec_namespaces: Signal<Vec<String>>,
    /// The open shell, what it has printed (as the terminal shows it), and where to ask from next.
    pub exec_id: Signal<Option<String>>,
    pub exec_screen: Signal<String>,
    pub exec_raw: Signal<String>,
    pub exec_next: Signal<u64>,
    pub exec_over: Signal<Option<String>>,
    pub cluster_list: Signal<Option<Vec<ClusterEntry>>>,
    pub cluster_object: Signal<Option<ClusterObject>>,
    pub cluster_log: Signal<Option<String>>,
    pub problem: Signal<Option<Problem>>,
    pub notice: Signal<Option<Notice>>,
    pub pending: Signal<Option<Pending>>,
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn gantry(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Gantry {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Gantry { view, deliver }
}

impl State {
    fn new() -> Self {
        Self {
            info: create_signal(Info::default()),
            groups: create_signal(None),
            busy: create_signal(None),
            deployments: create_signal(Vec::new()),
            target: create_signal(None),
            operations: create_signal(None),
            operation: create_signal(None),
            plan: create_signal(None),
            yaml: create_signal(None),
            cluster_kinds: create_signal(Vec::new()),
            cluster_overview: create_signal(None),
            facade: create_signal(None),
            exec_namespaces: create_signal(Vec::new()),
            exec_id: create_signal(None),
            exec_screen: create_signal(String::new()),
            exec_raw: create_signal(String::new()),
            exec_next: create_signal(0),
            exec_over: create_signal(None),
            cluster_list: create_signal(None),
            cluster_object: create_signal(None),
            cluster_log: create_signal(None),
            problem: create_signal(None),
            notice: create_signal(None),
            pending: create_signal(None),
        }
    }

    /// A screen is opened: nothing of the last one's banners or questions stays.
    fn enter(&self) {
        self.problem.set(None);
        self.notice.set(None);
        self.pending.set(None);
    }
}

fn receive(state: &State, router: &Router, base: &str, m: Msg) {
    match m {
        Msg::Info(info) => state.info.set(info),
        Msg::Groups(list) => {
            state.problem.set(None);
            state.groups.set(Some(list));
        }
        Msg::Busy(busy) => state.busy.set(busy),
        Msg::Deployments(list) => state.deployments.set(list),
        Msg::Target(target) => {
            state.problem.set(None);
            state.target.set(Some(*target));
        }
        Msg::Operations(list) => {
            state.problem.set(None);
            state.operations.set(Some(list));
        }
        Msg::Operation(operation) => {
            state.problem.set(None);
            state.operation.set(Some(*operation));
        }
        Msg::Plan(plan) => {
            state.problem.set(None);
            state.plan.set(Some(*plan));
        }
        Msg::Yaml(yaml) => {
            state.problem.set(None);
            state.yaml.set(Some(*yaml));
        }
        Msg::ClusterKinds(kinds) => state.cluster_kinds.set(kinds),
        Msg::ExecEnabled(namespaces) => state.exec_namespaces.set(namespaces),
        Msg::ExecOpened(id) => {
            state.exec_raw.set(String::new());
            state.exec_screen.set(String::new());
            state.exec_next.set(0);
            state.exec_over.set(None);
            state.exec_id.set(Some(id));
        }
        Msg::ExecOutput(read) => {
            if read.next >= state.exec_next.get_untracked() {
                state.exec_next.set(read.next);
                if !read.data.is_empty() {
                    let mut raw = state.exec_raw.get_untracked();
                    raw.push_str(&read.data);
                    state.exec_screen.set(gantry_core::plain_terminal(&raw));
                    state.exec_raw.set(raw);
                }
            }
            if read.ended {
                state.exec_over.set(Some(read.closed.unwrap_or_default()));
            }
        }
        Msg::Facade(entries) => {
            state.problem.set(None);
            state.facade.set(Some(entries));
        }
        Msg::ClusterOverview(overview) => state.cluster_overview.set(Some(*overview)),
        Msg::ClusterList(list) => {
            state.problem.set(None);
            state.cluster_list.set(Some(list));
        }
        Msg::ClusterObject(object) => {
            state.problem.set(None);
            state.cluster_object.set(Some(*object));
        }
        Msg::ClusterLog(text) => state.cluster_log.set(Some(text)),
        Msg::Started(id) => {
            state.enter();
            router.navigate(&format!("{base}/ui/operations/{id}"));
        }
        Msg::Done(text) => state.notice.set(Some(Notice { ok: true, text })),
        Msg::Rejected(text) => state.notice.set(Some(Notice { ok: false, text })),
        Msg::NotFound => state.problem.set(Some(Problem::NotFound)),
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

/// Asks again every `every_ms` until the screen that asked is gone or `ask` says it is done (returns false).
fn poll(every_ms: u32, ask: Rc<dyn Fn() -> bool>) {
    fn tick(every_ms: u32, ask: Rc<dyn Fn() -> bool>, alive: Rc<Cell<bool>>) {
        let _ = quench_ui::platform::after(every_ms, move || {
            if alive.get() && ask() {
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
    let state = State::new();
    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();

    {
        let (state, router, base) = (state.clone(), router.clone(), base.clone());
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| receive(&state, &router, &base, m)));
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
            state.enter();
            (env.ask)(Request::Packages);
            screens::home(&env, &state)
        }
    };
    let package = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let name = ctx.param("name").unwrap_or_default();
            state.enter();
            state.deployments.set(Vec::new());
            (env.ask)(Request::Package(name.clone()));
            screens::package(&env, &state, &name)
        }
    };
    let target = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let name = ctx.param("name").unwrap_or_default();
            state.enter();
            state.target.set(None);
            (env.ask)(Request::Target(name.clone()));
            screens::target(&env, &state, name)
        }
    };
    let operations = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.enter();
            state.operations.set(None);
            (env.ask)(Request::Operations);
            screens::operations(&env, &state)
        }
    };
    let operation = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            state.enter();
            state.operation.set(None);
            (env.ask)(Request::Operation(id.clone()));
            // A running operation is looked at again until it is over.
            let (again, watched, signal) =
                (Rc::clone(&env.ask), id.clone(), state.operation.clone());
            poll(
                POLL_MS,
                Rc::new(move || {
                    let over =
                        signal.with_untracked(|op| op.as_ref().is_some_and(Operation::is_terminal));
                    if !over {
                        again(Request::Operation(watched.clone()));
                    }
                    !over
                }),
            );
            screens::operation(&env, &state, id)
        }
    };
    let plan = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            state.enter();
            state.plan.set(None);
            (env.ask)(Request::Plan(id.clone()));
            screens::plan(&env, &state, id)
        }
    };
    let resource = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            state.enter();
            state.yaml.set(None);
            let Some(resource) = ResourceRef::from_pairs(&ctx.location().query_pairs()) else {
                state.problem.set(Some(Problem::NotFound));
                return screens::not_found(&env);
            };
            (env.ask)(Request::Yaml(resource.clone()));
            screens::resource(&env, &state, resource)
        }
    };
    let cluster_home = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.enter();
            (env.ask)(Request::ClusterKinds);
            (env.ask)(Request::ClusterOverview);
            let again = Rc::clone(&env.ask);
            poll(
                CLUSTER_POLL_MS,
                Rc::new(move || {
                    again(Request::ClusterOverview);
                    true
                }),
            );
            screens::cluster_home(&env, &state)
        }
    };
    let cluster_list = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let kind = ctx.param("kind").unwrap_or_default();
            state.enter();
            state.cluster_list.set(None);
            (env.ask)(Request::ClusterKinds);
            let namespace = ctx
                .location()
                .query_pairs()
                .into_iter()
                .find(|(key, _)| key == "namespace")
                .map(|(_, value)| value)
                .unwrap_or_default();
            let ask = {
                let (ask, kind) = (Rc::clone(&env.ask), kind.clone());
                move || {
                    ask(Request::ClusterList {
                        kind: kind.clone(),
                        namespace: namespace.clone(),
                    });
                }
            };
            ask();
            // What a cluster is doing changes under the page: look again.
            poll(
                CLUSTER_POLL_MS,
                Rc::new(move || {
                    ask();
                    true
                }),
            );
            screens::cluster_list(&env, &state, &kind)
        }
    };
    let cluster_object = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let kind = ctx.param("kind").unwrap_or_default();
            state.enter();
            state.cluster_object.set(None);
            state.cluster_log.set(None);
            state.exec_id.set(None);
            state.exec_over.set(None);
            (env.ask)(Request::ClusterKinds);
            let Some(object) = ClusterRef::from_pairs(&kind, &ctx.location().query_pairs()) else {
                state.problem.set(Some(Problem::NotFound));
                return screens::not_found(&env);
            };
            (env.ask)(Request::ClusterObject(object.clone()));
            if object.kind == "Pod" {
                (env.ask)(Request::ExecEnabled);
                // While a shell is open and not over, ask what it printed.
                let (ask, state) = (Rc::clone(&env.ask), state.clone());
                poll(
                    EXEC_POLL_MS,
                    Rc::new(move || {
                        if let Some(id) = state.exec_id.get_untracked()
                            && state.exec_over.get_untracked().is_none()
                        {
                            ask(Request::ExecPoll {
                                id,
                                since: state.exec_next.get_untracked(),
                            });
                        }
                        true
                    }),
                );
            }
            screens::cluster_object(&env, &state, object)
        }
    };
    let services = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.enter();
            state.facade.set(None);
            let ask = Rc::clone(&env.ask);
            ask(Request::Facade);
            poll(
                FACADE_POLL_MS,
                Rc::new(move || {
                    ask(Request::Facade);
                    true
                }),
            );
            screens::services(&env, &state)
        }
    };
    let profile = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.enter();
            (env.ask)(Request::Startup);
            screens::profile(&env, &state)
        }
    };
    let screen = Routes::new()
        .route(&format!("{base}/ui/services"), services)
        .route(&format!("{base}/ui/profile"), profile)
        .route(&format!("{base}/ui/cluster"), cluster_home.clone())
        .route(&format!("{base}/ui/cluster/"), cluster_home)
        .route(&format!("{base}/ui/cluster/:kind"), cluster_list)
        .route(&format!("{base}/ui/cluster/:kind/view"), cluster_object)
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home)
        .route(&format!("{base}/ui/apps/:name"), package)
        .route(&format!("{base}/ui/targets/:name"), target)
        .route(&format!("{base}/ui/operations"), operations.clone())
        .route(&format!("{base}/ui/operations/"), operations)
        .route(&format!("{base}/ui/operations/:id"), operation)
        .route(&format!("{base}/ui/plans/:id"), plan)
        .route(&format!("{base}/ui/resource"), resource)
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Gantry",
            icon: "anchor",
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

    div()
        .attr("id", "gantry")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(
            &bar,
            vec![screens::sidebar(&env, &router, &state)],
        ))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(
                    main()
                        .class("gt-main")
                        .child(screens::banner(&env, &state, &setup.login_url))
                        .child(screen.view(&router)),
                ),
        )
        .into_view()
}
