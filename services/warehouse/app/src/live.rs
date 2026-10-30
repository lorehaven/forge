//! The app against a host: it asks for the data and for every change (`Request`) and is told what came (`Msg`). The host
//! does the talking to the service; the app owns the routes (`{base}/ui/home`, `/ui/docker/catalog`, `/ui/crates/catalog`,
//! `/ui/files/storages`, `/ui/files/browse`, `/ui/artifacts/catalog`, `/ui/rivets/catalog`), the sidebar and the language.

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, Router, Routes};
use warehouse_core::{
    ArtifactEntry, Browse, CrateVersion, DockerTag, NewStorage, Overview, RivetEntry,
    StorageChanges, Storages,
};

use crate::screens::{self, Env};

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Who the person is and which registries exist: once, when the app starts.
    Startup,
    DockerRepos,
    DockerTags(String),
    DeleteImage {
        repository: String,
        digest: String,
    },
    Crates,
    CrateVersions(String),
    SetCrateYank {
        name: String,
        version: String,
        yanked: bool,
    },
    Artifacts,
    SetArtifactYank {
        program: String,
        platform: String,
        version_code: i64,
        yanked: bool,
    },
    Rivets,
    SetRivetYank {
        name: String,
        version: String,
        yanked: bool,
    },
    Storages,
    CreateStorage(NewStorage),
    EditStorage {
        name: String,
        changes: StorageChanges,
    },
    DeleteStorage(String),
    /// A storage's files.
    Browse(String),
    DeleteFile {
        storage: String,
        path: String,
    },
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Overview(Overview),
    DockerRepos(Vec<String>),
    DockerTags {
        repo: String,
        tags: Vec<DockerTag>,
    },
    /// An image was removed: the app leaves its tag (the tags come again).
    ImageDeleted(String),
    Crates(Vec<String>),
    /// A crate's versions, oldest first.
    CrateVersions {
        name: String,
        versions: Vec<CrateVersion>,
    },
    Artifacts(Vec<ArtifactEntry>),
    Rivets(Vec<RivetEntry>),
    Storages(Storages),
    /// A storage was created or changed: the app opens it.
    StorageSaved(String),
    /// A storage was removed: the app leaves it.
    StorageDeleted,
    Browse(Box<Browse>),
    /// A file was removed: the app opens its directory (the files come again).
    FileDeleted {
        storage: String,
        parent: String,
    },
    /// The service refused or failed a change; the text is for the person (an `api_error_*` key is translated).
    Rejected(String),
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/warehouse`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
}

/// The app and the function that feeds it.
pub struct Warehouse {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Warehouse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Warehouse").finish_non_exhaustive()
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

/// A destructive step the person has to confirm.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Confirm {
    /// Translation key of the question.
    pub text: &'static str,
    /// What is about to go.
    pub target: String,
    /// Translation key of the dialog's title.
    pub title: &'static str,
    /// Asked of the host when the person confirms.
    pub action: Request,
}

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub overview: Signal<Option<Overview>>,
    pub repos: Signal<Option<Vec<String>>>,
    pub tags: Signal<Option<(String, Vec<DockerTag>)>>,
    pub crates: Signal<Option<Vec<String>>>,
    pub versions: Signal<Option<(String, Vec<CrateVersion>)>>,
    pub artifacts: Signal<Option<Vec<ArtifactEntry>>>,
    pub rivets: Signal<Option<Vec<RivetEntry>>>,
    pub storages: Signal<Option<Storages>>,
    pub browse: Signal<Option<Browse>>,
    pub problem: Signal<Option<Problem>>,
    /// A refused change, until the person does something else.
    pub notice: Signal<Option<String>>,
    pub confirm: Signal<Option<Confirm>>,
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn warehouse(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Warehouse {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Warehouse { view, deliver }
}

fn receive(state: &State, router: &Router, base: &str, m: Msg) {
    match m {
        Msg::Overview(overview) => state.overview.set(Some(overview)),
        Msg::DockerRepos(list) => {
            state.problem.set(None);
            state.repos.set(Some(list));
        }
        Msg::DockerTags { repo, tags } => {
            state.problem.set(None);
            state.tags.set(Some((repo, tags)));
        }
        Msg::ImageDeleted(repo) => {
            state.notice.set(None);
            router.navigate(&format!(
                "{base}/ui/docker/catalog?repo={}",
                warehouse_core::fmt::encode(&repo)
            ));
        }
        Msg::Crates(list) => {
            state.problem.set(None);
            state.crates.set(Some(list));
        }
        Msg::CrateVersions { name, versions } => {
            state.problem.set(None);
            state.notice.set(None);
            state.versions.set(Some((name, versions)));
        }
        Msg::Artifacts(list) => {
            state.problem.set(None);
            state.notice.set(None);
            state.artifacts.set(Some(list));
        }
        Msg::Rivets(list) => {
            state.problem.set(None);
            state.notice.set(None);
            state.rivets.set(Some(list));
        }
        Msg::Storages(list) => {
            state.problem.set(None);
            state.storages.set(Some(list));
        }
        Msg::StorageSaved(name) => {
            state.notice.set(None);
            router.navigate(&format!(
                "{base}/ui/files/storages?storage={}",
                warehouse_core::fmt::encode(&name)
            ));
        }
        Msg::StorageDeleted => {
            state.notice.set(None);
            router.navigate(&format!("{base}/ui/files/storages"));
        }
        Msg::Browse(browse) => {
            state.problem.set(None);
            state.browse.set(Some(*browse));
        }
        Msg::FileDeleted { storage, parent } => {
            state.notice.set(None);
            router.navigate(&format!(
                "{base}/ui/files/browse?storage={}&path={}",
                warehouse_core::fmt::encode(&storage),
                warehouse_core::fmt::encode(&parent)
            ));
        }
        Msg::Rejected(why) => state.notice.set(Some(why)),
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

fn new_state() -> State {
    State {
        overview: create_signal(None),
        repos: create_signal(None),
        tags: create_signal(None),
        crates: create_signal(None),
        versions: create_signal(None),
        artifacts: create_signal(None),
        rivets: create_signal(None),
        storages: create_signal(None),
        browse: create_signal(None),
        problem: create_signal(None),
        notice: create_signal(None),
        confirm: create_signal(None),
    }
}

/// The routes. Built again when the language changes, so every screen is drawn in the new one.
fn routes(env: &Env, state: &State, base: &str) -> Routes {
    // A page starts clean: no old failure, no old notice, no open dialog.
    let page = |env: &Env, state: &State, build: fn(&Env, &State) -> View| {
        let (env, state) = (env.clone(), state.clone());
        move |_: &quench_ui::router::RouteContext| -> View {
            state.problem.set(None);
            state.notice.set(None);
            state.confirm.set(None);
            build(&env, &state)
        }
    };
    let home = page(env, state, screens::home);
    let mut routes = Routes::new()
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home.clone())
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home);
    for (path, build) in [
        (
            "/ui/docker/catalog",
            screens::docker as fn(&Env, &State) -> View,
        ),
        ("/ui/crates/catalog", screens::crates),
        ("/ui/files/storages", screens::storages),
        ("/ui/files/browse", screens::browse),
        ("/ui/artifacts/catalog", screens::artifacts),
        ("/ui/rivets/catalog", screens::rivets),
    ] {
        let one = page(env, state, build);
        routes = routes
            .route(&format!("{base}{path}"), one.clone())
            .route(&format!("{base}{path}/"), one);
    }
    routes.not_found({
        let env = env.clone();
        move |_| screens::not_found(&env)
    })
}

fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let state = new_state();
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
        router: router.clone(),
    };

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Warehouse",
            icon: "grid",
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

    // Everything under the bar is drawn again when the language changes; the router (and so the address) stays.
    let content = {
        let (env, state, router, base) = (env.clone(), state.clone(), router.clone(), base.clone());
        move || -> View {
            let _ = env.t.language();
            untrack(|| {
                div()
                    .class("wh-content")
                    .child(banner(&env, &state, &base_login(&base)))
                    .child(routes(&env, &state, &base).view(&router))
                    .child(screens::confirm_dialog(&env, &state))
                    .into_view()
            })
        }
    };

    div()
        .attr("id", "warehouse")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(
            &bar,
            vec![screens::sidebar(&env, &state)],
        ))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(main().class("wh-main").child(View::dynamic(content))),
        )
        .into_view()
}

fn base_login(base: &str) -> String {
    format!("{base}/ui/login")
}

/// The line above the screens: a failure, a lost session, or a refused change.
fn banner(env: &Env, state: &State, login: &str) -> View {
    let (env, problem, notice, login) = (
        env.clone(),
        state.problem.clone(),
        state.notice.clone(),
        login.to_string(),
    );
    View::dynamic(move || match (problem.get(), notice.get()) {
        (Some(Problem::Failed(why)), _) => p()
            .attr("id", "problem")
            .class("fui-notice fui-notice-error")
            .text(why)
            .into_view(),
        (Some(Problem::SignedOut), _) => p()
            .attr("id", "problem")
            .class("fui-notice fui-notice-error")
            .child(span().text(format!("{} ", env.t.t("ui_session_ended"))))
            .child(
                a().attr("id", "sign-in")
                    .attr("href", login.clone())
                    .text(env.t.t("ui_login_sign_in")),
            )
            .into_view(),
        (_, Some(why)) => p()
            .attr("id", "notice")
            .class("fui-notice fui-notice-error")
            .text(env.t.t(&why))
            .into_view(),
        _ => View::Empty,
    })
}
