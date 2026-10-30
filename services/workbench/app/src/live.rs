//! The app against a host: it asks for the data and for every change (`Request`) and is told what came (`Msg`). The host
//! does the talking to the service; the app owns the routes (`{base}/ui/home`, `{base}/ui/projects/{id}/board`,
//! `{base}/ui/issues/{id}`), the sidebar and the language.

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};
use workbench_core::{
    Board, CreateIssue, CreateProject, Issue, IssueView, Project, UpdateIssue, User,
};

use crate::screens::{self, Env};

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Who the person is, who can be assigned, and the projects: once, when the app starts.
    Startup,
    Projects,
    /// A project with its issues.
    Board(String),
    /// An issue with its project, comments and links.
    Issue(String),
    CreateProject(CreateProject),
    CreateIssue {
        project: String,
        body: CreateIssue,
    },
    /// Move a card to a status (the screen already shows it moved).
    Move {
        issue: String,
        status: String,
    },
    /// Replace the issue's fields; `status` is set when the form changed it (a transition of its own).
    Save {
        issue: String,
        body: UpdateIssue,
        status: Option<String>,
    },
    Comment {
        issue: String,
        body: String,
    },
    /// Link to the issue with this key (`WB-4`); the host looks it up.
    AddLink {
        issue: String,
        key: String,
        kind: String,
    },
    RemoveLink {
        issue: String,
        link: String,
    },
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Me(String),
    Users(Vec<User>),
    Projects(Vec<Project>),
    Board(Box<Board>),
    /// The issue's page, after it was opened or changed.
    Issue(Box<IssueView>),
    /// A project was created: the app opens its board.
    ProjectCreated(Box<Project>),
    IssueCreated(Box<Issue>),
    /// The service refused or failed a change; the text is for the person.
    Rejected(String),
    /// There is no such project or issue.
    NotFound,
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/workbench`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
}

/// The app and the function that feeds it.
pub struct Workbench {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Workbench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workbench").finish_non_exhaustive()
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

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub projects: Signal<Option<Vec<Project>>>,
    pub me: Signal<String>,
    pub users: Signal<Vec<User>>,
    pub board: Signal<Option<Board>>,
    pub issue: Signal<Option<IssueView>>,
    pub problem: Signal<Option<Problem>>,
    /// A refused change, until the person does something else.
    pub notice: Signal<Option<String>>,
    /// The project whose board is open: a refused move asks for it again.
    pub watching: Signal<Option<String>>,
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn workbench(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Workbench {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Workbench { view, deliver }
}

fn receive(state: &State, router: &Router, requests: &Rc<dyn Fn(Request)>, base: &str, m: Msg) {
    match m {
        Msg::Me(name) => state.me.set(name),
        Msg::Users(users) => state.users.set(users),
        Msg::Projects(list) => state.projects.set(Some(list)),
        Msg::Board(board) => {
            state.problem.set(None);
            state.board.set(Some(*board));
        }
        Msg::Issue(view) => {
            state.problem.set(None);
            state.notice.set(None);
            state.issue.set(Some(*view));
        }
        Msg::ProjectCreated(project) => {
            state.notice.set(None);
            state.projects.update(|list| {
                let list = list.get_or_insert_with(Vec::new);
                list.push((*project).clone());
                list.sort_by(|a, b| a.key.cmp(&b.key));
            });
            router.navigate(&format!("{base}/ui/projects/{}/board", project.id));
        }
        Msg::IssueCreated(issue) => {
            state.notice.set(None);
            state.board.update(|board| {
                if let Some(board) = board.as_mut().filter(|b| b.project.id == issue.project_id) {
                    board.issues.push(*issue);
                    board.issues.sort_by_key(|i| i.seq);
                }
            });
        }
        Msg::Rejected(why) => {
            state.notice.set(Some(why));
            // The screen may show a change the service refused: ask for what is true.
            if let Some(id) = state.watching.get_untracked() {
                requests(Request::Board(id));
            }
        }
        Msg::NotFound => state.problem.set(Some(Problem::NotFound)),
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let state = State {
        projects: create_signal(None),
        me: create_signal(String::new()),
        users: create_signal(Vec::new()),
        board: create_signal(None),
        issue: create_signal(None),
        problem: create_signal(None),
        notice: create_signal(None),
        watching: create_signal(None),
    };
    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();

    {
        let (state, router, requests, base) = (
            state.clone(),
            router.clone(),
            Rc::clone(requests),
            base.clone(),
        );
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| {
            receive(&state, &router, &requests, &base, m);
        }));
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
            state.problem.set(None);
            state.notice.set(None);
            (env.ask)(Request::Projects);
            screens::home(&env, &state)
        }
    };
    let board = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            state.problem.set(None);
            state.notice.set(None);
            state.board.set(None);
            state.watching.set(Some(id.clone()));
            let watching = state.watching.clone();
            on_cleanup(move || watching.set(None));
            (env.ask)(Request::Board(id.clone()));
            screens::board(&env, &state, id)
        }
    };
    let issue = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            state.problem.set(None);
            state.notice.set(None);
            state.issue.set(None);
            (env.ask)(Request::Issue(id.clone()));
            screens::issue(&env, &state, id)
        }
    };
    let routes = Routes::new()
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home)
        .route(&format!("{base}/ui/projects/:id/board"), board)
        .route(&format!("{base}/ui/issues/:id"), issue)
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Workbench",
            icon: "list-check",
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
        let (problem, notice, login) = (
            state.problem.clone(),
            state.notice.clone(),
            setup.login_url.clone(),
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
                .child(span().text("Your session ended. "))
                .child(
                    a().attr("id", "sign-in")
                        .attr("href", login.clone())
                        .text("Sign in"),
                )
                .into_view(),
            (_, Some(why)) => p()
                .attr("id", "notice")
                .class("fui-notice fui-notice-error")
                .text(why)
                .into_view(),
            _ => View::Empty,
        })
    };

    div()
        .attr("id", "workbench")
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
                        .class("wb-main")
                        .child(banner)
                        .child(routes.view(&router)),
                ),
        )
        .into_view()
}
