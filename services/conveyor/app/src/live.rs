//! The app against a host: it asks for the data and for every change (`Request`) and is told what came (`Msg`). The host does
//! the talking to the service; the app owns the routes (`{base}/ui/home`, `/ui/projects/{id}`, `/ui/runs`, `/ui/runs/{id}`,
//! `/ui/repos`, `/ui/repos/{owner}/{name}/{edit,run,scan,scan/{check}}`, `/ui/credentials`), the sidebar, the polling and the
//! language.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use conveyor_core::{
    Access, Credential, KnownStage, Project, RegisterRepo, Repo, RepoChecks, Run, RunView,
    RunsPage, ScanFindings, ScanReport, Subscription, TriggerRun, UpdateRepo,
};
use quench_core::time::Time;
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes, link};

use crate::screens::{self, Env};

/// What can be followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Follow {
    Repo(String),
    Project(String),
}

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// The front page (or a project's branch of it): projects, repositories, the runs, the checks.
    Home {
        scope: Option<String>,
    },
    /// Only the runs of the front page and the checks of the repositories, again (the poll).
    Recent {
        scope: Option<String>,
    },
    /// A page of the full history.
    History {
        scope: Option<String>,
        page: u32,
    },
    /// A run with its jobs, artifacts and repository (opening it, and the poll).
    Run(String),
    Restart(String),
    /// Start a run of the repository now, then read the front page's runs again.
    RunNow {
        repo: String,
        scope: Option<String>,
    },
    /// Start streaming a job's log / stop it.
    OpenLog(String),
    CloseLog(String),
    /// What the repositories pages show: access, projects, repositories, follows.
    Repos,
    CreateRepo(RegisterRepo),
    SaveRepo {
        id: String,
        body: UpdateRepo,
    },
    DeleteRepo(String),
    Follow {
        target: Follow,
        on: bool,
    },
    /// The "run with parameters" form of a repository: access, repositories, stages.
    Launch {
        owner: String,
        name: String,
    },
    RunStages {
        repo: String,
        body: TriggerRun,
    },
    /// A repository's scan overview.
    Scan {
        owner: String,
        name: String,
    },
    /// One check of the scan (`security` adds the scanner's findings).
    Check {
        owner: String,
        name: String,
        kind: String,
    },
    Credentials,
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Access(Access),
    Projects(Vec<Project>),
    Repos(Vec<Repo>),
    Subscriptions(Vec<Subscription>),
    Recent {
        scope: Option<String>,
        runs: Vec<Run>,
    },
    History(RunsPage),
    Chips(Vec<RepoChecks>),
    Run(Box<RunView>),
    Credentials(Vec<Credential>),
    Stages {
        repo: String,
        stages: Vec<KnownStage>,
    },
    Scan {
        repo: String,
        report: Box<ScanReport>,
    },
    Findings {
        repo: String,
        found: Option<Box<ScanFindings>>,
    },
    /// A line of a job's log.
    LogLine {
        job: String,
        stderr: bool,
        line: String,
    },
    /// The log is complete.
    LogDone {
        job: String,
    },
    /// A change went through: the i18n key of what to say.
    Done(String),
    /// A repository was saved, as it is called now: the app stays on (or moves to) its edit screen.
    Saved {
        owner: String,
        name: String,
    },
    /// A run was started: the app opens it.
    Opened(String),
    /// The repository is gone: the app goes to the list.
    Deleted,
    /// The service refused or failed a change; the text is for the person.
    Rejected(String),
    /// There is no such project or run.
    NotFound,
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/conveyor`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
    /// The time now, for "3m ago".
    pub now: Rc<dyn Fn() -> Time>,
    /// How often the front page's runs are asked for again.
    pub home_refresh_ms: u32,
    /// How often a run that has not finished is asked for again.
    pub run_refresh_ms: u32,
}

/// The app and the function that feeds it.
pub struct Conveyor {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Conveyor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conveyor").finish_non_exhaustive()
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

/// What a banner says after a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    /// An i18n key of something that worked.
    Ok(String),
    /// An i18n key of something that was wrong with the form.
    ErrKey(String),
    /// The service's own words.
    Err(String),
}

/// A job's log as far as it arrived.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct JobLog {
    pub lines: Vec<(bool, String)>,
    pub done: bool,
}

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub access: Signal<Option<Access>>,
    pub projects: Signal<Option<Vec<Project>>>,
    pub repos: Signal<Option<Vec<Repo>>>,
    pub subs: Signal<Vec<Subscription>>,
    /// The project the front page is rooted at.
    pub scope: Signal<Option<String>>,
    pub recent: Signal<Option<Vec<Run>>>,
    pub chips: Signal<Vec<RepoChecks>>,
    pub history: Signal<Option<RunsPage>>,
    pub run: Signal<Option<RunView>>,
    /// The jobs whose log was asked for.
    pub opened: Signal<HashSet<String>>,
    pub logs: Signal<HashMap<String, JobLog>>,
    pub creds: Signal<Option<Vec<Credential>>>,
    pub stages: Signal<Option<(String, Vec<KnownStage>)>>,
    pub scan: Signal<Option<(String, ScanReport)>>,
    pub findings: Signal<Option<(String, Option<ScanFindings>)>>,
    pub problem: Signal<Option<Problem>>,
    pub notice: Signal<Option<Notice>>,
}

impl State {
    fn new() -> Self {
        Self {
            access: create_signal(None),
            projects: create_signal(None),
            repos: create_signal(None),
            subs: create_signal(Vec::new()),
            scope: create_signal(None),
            recent: create_signal(None),
            chips: create_signal(Vec::new()),
            history: create_signal(None),
            run: create_signal(None),
            opened: create_signal(HashSet::new()),
            logs: create_signal(HashMap::new()),
            creds: create_signal(None),
            stages: create_signal(None),
            scan: create_signal(None),
            findings: create_signal(None),
            problem: create_signal(None),
            notice: create_signal(None),
        }
    }

    /// A screen opens: what the last one said is over.
    pub(crate) fn enter(&self) {
        self.problem.set(None);
        self.notice.set(None);
    }
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn conveyor(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Conveyor {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Conveyor { view, deliver }
}

fn receive(state: &State, router: &Router, base: &str, m: Msg) {
    match m {
        Msg::Access(access) => state.access.set(Some(access)),
        Msg::Projects(list) => state.projects.set(Some(list)),
        Msg::Repos(list) => state.repos.set(Some(list)),
        Msg::Subscriptions(list) => state.subs.set(list),
        Msg::Recent { scope, runs } => {
            if scope == state.scope.get_untracked() {
                state.problem.set(None);
                state.recent.set(Some(runs));
            }
        }
        Msg::History(page) => {
            state.problem.set(None);
            state.history.set(Some(page));
        }
        Msg::Chips(list) => state.chips.set(list),
        Msg::Run(view) => {
            state.problem.set(None);
            state.run.set(Some(*view));
        }
        Msg::Credentials(list) => state.creds.set(Some(list)),
        Msg::Stages { repo, stages } => state.stages.set(Some((repo, stages))),
        Msg::Scan { repo, report } => {
            state.problem.set(None);
            state.scan.set(Some((repo, *report)));
        }
        Msg::Findings { repo, found } => state.findings.set(Some((repo, found.map(|f| *f)))),
        Msg::LogLine { job, stderr, line } => state.logs.update(|logs| {
            logs.entry(job).or_default().lines.push((stderr, line));
        }),
        Msg::LogDone { job } => state.logs.update(|logs| {
            logs.entry(job).or_default().done = true;
        }),
        Msg::Done(key) => state.notice.set(Some(Notice::Ok(key))),
        Msg::Saved { owner, name } => {
            let target = format!(
                "{base}/ui/repos/{}/{}/edit",
                conveyor_core::format::segment(&owner),
                conveyor_core::format::segment(&name)
            );
            if router.location().path != target {
                router.navigate(&target);
            }
            state
                .notice
                .set(Some(Notice::Ok("ui_repos_ok_saved".to_string())));
        }
        Msg::Opened(id) => router.navigate(&format!("{base}/ui/runs/{id}")),
        Msg::Deleted => {
            // The list opens clean; what was said is said after it.
            router.navigate(&format!("{base}/ui/repos"));
            state
                .notice
                .set(Some(Notice::Ok("ui_repos_ok_deleted".to_string())));
        }
        Msg::Rejected(why) => state.notice.set(Some(Notice::Err(why))),
        Msg::NotFound => state.problem.set(Some(Problem::NotFound)),
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

/// Asks again every `every_ms` while `again` says to, and until the screen that asked is gone.
fn poll(every_ms: u32, again: Rc<dyn Fn() -> bool>) {
    fn tick(every_ms: u32, again: Rc<dyn Fn() -> bool>, alive: Rc<std::cell::Cell<bool>>) {
        let _ = quench_ui::platform::after(every_ms, move || {
            if alive.get() && again() {
                tick(every_ms, again, alive);
            }
        });
    }
    let alive = Rc::new(std::cell::Cell::new(true));
    let gone = Rc::clone(&alive);
    on_cleanup(move || gone.set(false));
    tick(every_ms, again, alive);
}

/// A route whose page is built again when only its parameters change (`/runs/a` to `/runs/b`), which the router does not do by
/// itself: the page asks for its data and starts its polling when it is built.
fn again(
    page: impl Fn(&RouteContext) -> View + 'static,
) -> impl Fn(&RouteContext) -> View + 'static {
    let page = Rc::new(page);
    move |ctx: &RouteContext| {
        let (page, ctx) = (Rc::clone(&page), ctx.clone());
        View::dynamic(move || {
            // Reading the parameters here is what follows them.
            let _ = ctx.param("id");
            let _ = ctx.param("owner");
            let _ = ctx.param("name");
            let _ = ctx.param("category");
            page(&ctx)
        })
    }
}

/// A number from the address, or `fallback`.
fn page_of(text: Option<String>) -> u32 {
    text.and_then(|t| t.parse().ok())
        .filter(|p| *p >= 1)
        .unwrap_or(1)
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

    let env = Env {
        t: Rc::clone(&i18n),
        base: base.clone(),
        ask: Rc::clone(requests),
        now: Rc::clone(&setup.now),
        login: setup.login_url.clone(),
    };
    let (home_ms, run_ms) = (setup.home_refresh_ms, setup.run_refresh_ms);

    // The front page, or a project's branch of it (`:id`).
    let home = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let scope = ctx.param("id");
            state.enter();
            state.scope.set(scope.clone());
            state.recent.set(None);
            (env.ask)(Request::Home {
                scope: scope.clone(),
            });
            let (ask, again_scope, current) =
                (Rc::clone(&env.ask), scope.clone(), state.scope.clone());
            poll(
                home_ms,
                Rc::new(move || {
                    if current.get_untracked() == again_scope {
                        ask(Request::Recent {
                            scope: again_scope.clone(),
                        });
                    }
                    true
                }),
            );
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::home(&env, &state, scope.clone()))
        }
    };
    // The full history: the project and the page are in the address's query.
    let runs = {
        let (env, state, router) = (env.clone(), state.clone(), router.clone());
        move |_: &RouteContext| -> View {
            state.enter();
            state.history.set(None);
            let (ask, router_now, path) = (
                Rc::clone(&env.ask),
                router.clone(),
                format!("{}/ui/runs", env.base),
            );
            create_effect(move || {
                let at = router_now.location();
                // The effect outlives the route for a moment: it asks only while the address is still this screen's.
                if at.path.trim_end_matches('/') != path {
                    return;
                }
                let scope = at.query_get("project");
                let page = page_of(at.query_get("page"));
                untrack(|| ask(Request::History { scope, page }));
            });
            // New runs show up, and a running one changes its status, without a reload.
            let (ask, router_now, path) = (
                Rc::clone(&env.ask),
                router.clone(),
                format!("{}/ui/runs", env.base),
            );
            poll(
                home_ms,
                Rc::new(move || {
                    let at = router_now.location();
                    if at.path.trim_end_matches('/') == path {
                        ask(Request::History {
                            scope: at.query_get("project"),
                            page: page_of(at.query_get("page")),
                        });
                    }
                    true
                }),
            );
            let (env, state, router) = (env.clone(), state.clone(), router.clone());
            View::dynamic(move || screens::history(&env, &state, &router))
        }
    };
    let run = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            state.enter();
            state.run.set(None);
            state.opened.set(HashSet::new());
            state.logs.set(HashMap::new());
            (env.ask)(Request::Run(id.clone()));
            let (ask, run, again_id) = (Rc::clone(&env.ask), state.run.clone(), id.clone());
            // Stops once the run rests, so a page left open overnight costs nothing.
            poll(
                run_ms,
                Rc::new(move || {
                    let resting = run
                        .with_untracked(|r| r.as_ref().is_some_and(|v| v.run.status.is_terminal()));
                    if resting {
                        return false;
                    }
                    ask(Request::Run(again_id.clone()));
                    true
                }),
            );
            // The streams belong to the screen.
            let (ask, opened) = (Rc::clone(&env.ask), state.opened.clone());
            on_cleanup(move || {
                for job in opened.get_untracked() {
                    ask(Request::CloseLog(job));
                }
            });
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::run(&env, &state, id.clone()))
        }
    };
    let credentials = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.enter();
            state.creds.set(None);
            (env.ask)(Request::Credentials);
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::credentials(&env, &state))
        }
    };
    let repos = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            state.enter();
            (env.ask)(Request::Repos);
            let ask = Rc::clone(&env.ask);
            poll(
                home_ms,
                Rc::new(move || {
                    ask(Request::Repos);
                    true
                }),
            );
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::repos(&env, &state))
        }
    };
    let edit = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let (owner, name) = (
                ctx.param("owner").unwrap_or_default(),
                ctx.param("name").unwrap_or_default(),
            );
            state.enter();
            (env.ask)(Request::Repos);
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::edit(&env, &state, &owner, &name))
        }
    };
    let launch = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let (owner, name) = (
                ctx.param("owner").unwrap_or_default(),
                ctx.param("name").unwrap_or_default(),
            );
            state.enter();
            state.stages.set(None);
            (env.ask)(Request::Launch {
                owner: owner.clone(),
                name: name.clone(),
            });
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::launch(&env, &state, &owner, &name))
        }
    };
    let scan = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let (owner, name) = (
                ctx.param("owner").unwrap_or_default(),
                ctx.param("name").unwrap_or_default(),
            );
            state.enter();
            state.scan.set(None);
            (env.ask)(Request::Scan {
                owner: owner.clone(),
                name: name.clone(),
            });
            let (ask, again_owner, again_name) = (Rc::clone(&env.ask), owner.clone(), name.clone());
            poll(
                home_ms,
                Rc::new(move || {
                    ask(Request::Scan {
                        owner: again_owner.clone(),
                        name: again_name.clone(),
                    });
                    true
                }),
            );
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::scan(&env, &state, &owner, &name))
        }
    };
    let check = {
        let (env, state, router) = (env.clone(), state.clone(), router.clone());
        move |ctx: &RouteContext| -> View {
            let (owner, name, kind) = (
                ctx.param("owner").unwrap_or_default(),
                ctx.param("name").unwrap_or_default(),
                ctx.param("category").unwrap_or_default(),
            );
            state.enter();
            state.scan.set(None);
            state.findings.set(None);
            (env.ask)(Request::Check {
                owner: owner.clone(),
                name: name.clone(),
                kind: kind.clone(),
            });
            let (env, state, router) = (env.clone(), state.clone(), router.clone());
            View::dynamic(move || screens::check(&env, &state, &router, &owner, &name, &kind))
        }
    };

    let routes = Routes::new()
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home.clone())
        .route(&format!("{base}/ui/projects/:id"), again(home))
        .route(&format!("{base}/ui/runs"), runs)
        .route(&format!("{base}/ui/runs/:id"), again(run))
        .route(&format!("{base}/ui/credentials"), credentials)
        .route(&format!("{base}/ui/repos"), repos)
        .route(&format!("{base}/ui/repos/:owner/:name/edit"), again(edit))
        .route(&format!("{base}/ui/repos/:owner/:name/run"), again(launch))
        .route(&format!("{base}/ui/repos/:owner/:name/scan"), again(scan))
        .route(
            &format!("{base}/ui/repos/:owner/:name/scan/:category"),
            again(check),
        )
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Conveyor",
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
    let nav = {
        let (env, router) = (env.clone(), router.clone());
        View::dynamic(move || {
            let here = router.location().path;
            let item = |path: &str, key: &str, id: &str| {
                let href = format!("{}/ui{path}", env.base);
                let mut item = link(&href)
                    .attr("id", id.to_string())
                    .class("fui-nav-item")
                    .child(forge_shell::icon(match id {
                        "nav-credentials" => "key",
                        _ => "activity",
                    }))
                    .child(span().text(env.t.t(key)));
                if here == href || (path == "/home" && here == format!("{}/ui", env.base)) {
                    item = item.class("cv-current");
                }
                item
            };
            div()
                .child(item("/home", "ui_nav_pipelines", "nav-pipelines"))
                .child(item(
                    "/credentials",
                    "ui_nav_credentials",
                    "nav-credentials",
                ))
                .into_view()
        })
    };

    let banner = {
        let (problem, notice, login, t) = (
            state.problem.clone(),
            state.notice.clone(),
            setup.login_url.clone(),
            Rc::clone(&i18n),
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
                .child(span().text(format!("{} ", t.t("ui_session_ended"))))
                .child(
                    a().attr("id", "sign-in")
                        .attr("href", login.clone())
                        .text(t.t("ui_sign_in")),
                )
                .into_view(),
            (_, Some(Notice::Ok(key))) => p()
                .attr("id", "notice")
                .class("fui-notice fui-notice-ok")
                .text(t.t(&key))
                .into_view(),
            (_, Some(Notice::ErrKey(key))) => p()
                .attr("id", "notice")
                .class("fui-notice fui-notice-error")
                .text(t.t(&key))
                .into_view(),
            (_, Some(Notice::Err(why))) => p()
                .attr("id", "notice")
                .class("fui-notice fui-notice-error")
                .text(why)
                .into_view(),
            _ => View::Empty,
        })
    };

    div()
        .attr("id", "conveyor")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(&bar, vec![nav]))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(
                    main()
                        .class("cv-main")
                        .child(banner)
                        .child(routes.view(&router)),
                ),
        )
        .into_view()
}
