//! The app against a host: it asks for the data and for every change (`Request`) and is told what came (`Msg`). The host does the
//! talking to the service; the app owns the routes (`{base}/ui/home?conversation_id=&project_id=`, `/ui/initializing`), the
//! sidebar, the streaming turn's state and the language.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use quench_ui::form::Field;
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};
use sage_core::{
    ConversationItem, ConversationView, Done, FileInfo, MessageNode, ModelsView, NewProject,
    Project, SendBody, Status, StreamEvent,
};

use crate::screens::{self, Env};

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The models: whether the chat can be used, what the composer offers.
    Models,
    /// The projects and the conversations of the sidebar.
    Sidebar,
    /// The files of a project (the sidebar's Files section).
    ProjectFiles(String),
    /// An id for a conversation that is not stored yet.
    NewId,
    /// A conversation, as the person left it.
    Conversation(String),
    /// Starts a turn with the person's message; the host streams the answer back as `Msg::Stream`.
    Send(SendBody),
    /// A new answer to the message before this answer.
    Regenerate(String),
    /// Has Sage answer the conversation if its last message is the person's.
    Resume(String),
    /// Another version of a message of the person's; Sage answers it.
    Edit {
        conversation_id: String,
        message_id: String,
        text: String,
    },
    /// Makes another alternative the current one, then reads the conversation again.
    Switch {
        conversation_id: String,
        target: String,
    },
    DeleteConversation(String),
    DeleteFile(String),
    CreateProject(NewProject),
    /// A file's state (a staged chip that is still being processed).
    FileStatus(String),
    Reprocess(String),
    /// Stop reading a turn's stream: the person left.
    StopStream,
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    Models(ModelsView),
    Projects(Vec<Project>),
    Conversations(Vec<ConversationItem>),
    ProjectFiles {
        project_id: String,
        files: Vec<FileInfo>,
    },
    NewId(String),
    Conversation(Box<ConversationView>),
    /// The conversation asked for is not stored (a fresh one).
    NoConversation(String),
    /// A turn was started and its stream is being read.
    TurnStarted(String),
    /// One event of the turn's stream.
    Stream(StreamEvent),
    /// The stream ended without saying how.
    StreamEnded,
    File(FileInfo),
    FileGone(String),
    ConversationDeleted(String),
    ProjectCreated(Project),
    /// The service refused or failed a change; the text is for the person.
    Rejected(String),
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/sage`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
}

/// The app and the function that feeds it.
pub struct Sage {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Sage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sage").finish_non_exhaustive()
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

/// What the banner says after a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    /// An i18n key (of something that was wrong with the form).
    ErrKey(String),
    /// The service's own words (an `api_error_*` code is translated).
    Err(String),
}

/// The conversation on screen.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Active {
    /// Empty until the service has given a fresh chat its id.
    pub id: String,
    pub project_id: Option<String>,
    pub messages: Vec<MessageNode>,
    /// The conversation was read (or is known to be new).
    pub loaded: bool,
    /// Sage was already asked to answer a trailing message of the person's (or this is the person's own doing).
    pub resumed: bool,
}

/// The turn being answered: what shows below the stored messages while the stream runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Turn {
    /// The person's message, shown at once (it is not read back from the service until the turn ends).
    pub user: Option<(String, Vec<FileInfo>)>,
    /// The answer so far.
    pub answer: String,
    pub status: Option<Status>,
    /// A regeneration shows in place of this answer.
    pub replaces: Option<String>,
    pub error: Option<String>,
    /// The turn is stored; the conversation is being read back.
    pub done: Option<Done>,
}

/// A dialog over the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Dialog {
    DeleteConversation { id: String, title: String },
    DeleteFile { id: String, name: String },
    NewProject,
}

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub models: Signal<Option<ModelsView>>,
    pub projects: Signal<Vec<Project>>,
    pub convs: Signal<Vec<ConversationItem>>,
    pub project_files: Signal<Vec<FileInfo>>,
    pub active: Signal<Active>,
    pub turn: Signal<Option<Turn>>,
    pub staged: Signal<Vec<FileInfo>>,
    pub editing: Signal<Option<String>>,
    pub dialog: Signal<Option<Dialog>>,
    /// The dot of the navigator the person last chose.
    pub dot: Signal<Option<String>>,
    pub model: Field,
    pub provider: Field,
    pub message: Field,
    /// The text of the message being edited.
    pub message_edit: Field,
    pub problem: Signal<Option<Problem>>,
    pub notice: Signal<Option<Notice>>,
}

impl State {
    fn new() -> Self {
        Self {
            models: create_signal(None),
            projects: create_signal(Vec::new()),
            convs: create_signal(Vec::new()),
            project_files: create_signal(Vec::new()),
            active: create_signal(Active::default()),
            turn: create_signal(None),
            staged: create_signal(Vec::new()),
            editing: create_signal(None),
            dialog: create_signal(None),
            dot: create_signal(None),
            model: Field::new("model-select", ""),
            provider: Field::new("provider-select", ""),
            message: Field::new("chat-input", ""),
            message_edit: Field::new("edit-input", ""),
            problem: create_signal(None),
            notice: create_signal(None),
        }
    }
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn sage(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Sage {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Sage { view, deliver }
}

/// The address of the chat for a conversation.
fn chat_url(base: &str, conversation: Option<&str>, project: Option<&str>) -> String {
    let mut query = Vec::new();
    if let Some(c) = conversation {
        query.push(format!("conversation_id={c}"));
    }
    if let Some(p) = project {
        query.push(format!("project_id={p}"));
    }
    if query.is_empty() {
        format!("{base}/ui/home")
    } else {
        format!("{base}/ui/home?{}", query.join("&"))
    }
}

pub(crate) fn conversation_url(base: &str, conv: &ConversationItem) -> String {
    chat_url(base, Some(&conv.id), conv.project_id.as_deref())
}

pub(crate) fn project_url(base: &str, project: &Project) -> String {
    chat_url(base, None, Some(&project.id))
}

pub(crate) fn new_chat_url(base: &str, project: Option<&str>) -> String {
    chat_url(base, None, project)
}

struct Ctx {
    state: State,
    router: Router,
    base: String,
    ask: Rc<dyn Fn(Request)>,
}

fn receive(cx: &Ctx, m: Msg) {
    let state = &cx.state;
    match m {
        Msg::Models(view) => {
            if state.model.value_untracked().is_empty()
                || !view
                    .instances
                    .iter()
                    .any(|i| i.id == state.model.value_untracked())
            {
                state
                    .model
                    .set(view.instances.first().map_or("", |i| &i.id));
            }
            if state.provider.value_untracked().is_empty() {
                state.provider.set(&view.default_search_provider);
            }
            state.problem.set(None);
            state.models.set(Some(view));
        }
        Msg::Projects(list) => {
            state.problem.set(None);
            state.projects.set(list);
        }
        Msg::Conversations(list) => state.convs.set(list),
        Msg::ProjectFiles { project_id, files } => {
            if state
                .active
                .with_untracked(|a| a.project_id.as_deref() == Some(&project_id))
            {
                state.project_files.set(files);
            }
        }
        Msg::NewId(id) => state.active.update(|a| {
            if a.id.is_empty() {
                a.id = id;
                a.loaded = true;
            }
        }),
        Msg::Conversation(view) => conversation_read(cx, *view),
        Msg::NoConversation(id) => state.active.update(|a| {
            if a.id == id {
                a.loaded = true;
            }
        }),
        Msg::TurnStarted(_) => {
            if state.turn.with_untracked(Option::is_none) {
                state.turn.set(Some(Turn::default()));
            }
        }
        Msg::Stream(event) => stream_event(cx, event),
        Msg::StreamEnded => state.turn.update(|turn| {
            if let Some(t) = turn
                && t.done.is_none()
                && t.error.is_none()
            {
                t.error = Some("ui_chat_stream_lost".to_string());
            }
        }),
        Msg::File(file) => {
            let replace = |list: &mut Vec<FileInfo>| {
                if let Some(slot) = list.iter_mut().find(|f| f.id == file.id) {
                    *slot = file.clone();
                }
            };
            state.staged.update(replace);
            state.project_files.update(replace);
        }
        Msg::FileGone(id) => {
            state.staged.update(|l| l.retain(|f| f.id != id));
            state.project_files.update(|l| l.retain(|f| f.id != id));
            state.dialog.set(None);
        }
        Msg::ConversationDeleted(id) => {
            state.dialog.set(None);
            state.convs.update(|l| l.retain(|c| c.id != id));
            if state.active.with_untracked(|a| a.id == id) {
                cx.router.navigate(&new_chat_url(
                    &cx.base,
                    state
                        .active
                        .with_untracked(|a| a.project_id.clone())
                        .as_deref(),
                ));
            }
        }
        Msg::ProjectCreated(project) => {
            state.dialog.set(None);
            state.projects.update(|l| l.insert(0, project.clone()));
            cx.router.navigate(&project_url(&cx.base, &project));
        }
        Msg::Rejected(why) => {
            // A turn that was refused leaves no bubble waiting for an answer.
            state.turn.update(|turn| {
                if turn.as_ref().is_some_and(|t| t.done.is_none()) {
                    *turn = None;
                }
            });
            state.dialog.set(None);
            state.notice.set(Some(Notice::Err(why)));
        }
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

fn stream_event(cx: &Ctx, event: StreamEvent) {
    let state = &cx.state;
    let mut finished = None;
    state.turn.update(|turn| {
        let turn = turn.get_or_insert_with(Turn::default);
        match event {
            StreamEvent::Status(status) => turn.status = Some(status),
            StreamEvent::Delta(text) => {
                turn.status = None;
                turn.answer.push_str(&text);
            }
            StreamEvent::Replace(text) => {
                turn.status = None;
                turn.answer = text;
            }
            StreamEvent::Done(done) => {
                turn.status = None;
                finished = Some(done.conversation_id.clone());
                turn.done = Some(done);
            }
            StreamEvent::Failure(message) => {
                turn.status = None;
                turn.error = Some(message);
            }
        }
    });
    if let Some(conversation) = finished {
        (cx.ask)(Request::Conversation(conversation));
        (cx.ask)(Request::Sidebar);
    }
}

fn conversation_read(cx: &Ctx, view: ConversationView) {
    let state = &cx.state;
    if state.active.with_untracked(|a| a.id != view.id) {
        return;
    }
    state.problem.set(None);
    let last_is_user = view.messages.last().is_some_and(MessageNode::is_user);
    let mut resume = false;
    let mut project_moved = None;
    state.active.update(|a| {
        if a.project_id != view.project_id {
            project_moved = view.project_id.clone();
        }
        a.project_id = view.project_id.clone();
        a.messages = view.messages.clone();
        a.loaded = true;
        if !a.resumed {
            a.resumed = true;
            resume = last_is_user;
        }
    });
    if let Some(project) = project_moved {
        state.project_files.set(Vec::new());
        (cx.ask)(Request::ProjectFiles(project));
    }
    let turn_done = state
        .turn
        .with_untracked(|t| t.as_ref().is_some_and(|t| t.done.is_some()));
    if turn_done {
        state.turn.set(None);
        state.editing.set(None);
        // A fresh chat gets its own address once it is stored, so a reload keeps it.
        let here = cx.router.location();
        if here.query_get("conversation_id").is_none() {
            cx.router.replace(&chat_url(
                &cx.base,
                Some(&view.id),
                view.project_id.as_deref(),
            ));
        }
    } else if resume && state.turn.with_untracked(Option::is_none) {
        (cx.ask)(Request::Resume(view.id.clone()));
    }
}

/// Asks again every `every_ms` while `again` says to, and until the screen that asked is gone.
fn poll(every_ms: u32, again: Rc<dyn Fn() -> bool>) {
    fn tick(every_ms: u32, again: Rc<dyn Fn() -> bool>, alive: Rc<Cell<bool>>) {
        let _ = quench_ui::platform::after(every_ms, move || {
            if alive.get() && again() {
                tick(every_ms, again, alive);
            }
        });
    }
    let alive = Rc::new(Cell::new(true));
    let gone = Rc::clone(&alive);
    on_cleanup(move || gone.set(false));
    tick(every_ms, again, alive);
}

/// What the address says about the chat, applied: another conversation or project resets the screen.
fn open_chat(cx: &Ctx, base_path: &str) {
    let here = cx.router.location();
    if here.path.trim_end_matches('/') != base_path {
        return;
    }
    let conversation = here.query_get("conversation_id");
    let project = here.query_get("project_id");
    let state = &cx.state;
    let current = state.active.get_untracked();
    let same = match &conversation {
        Some(id) => current.id == *id,
        // A new chat that is already on screen stays.
        None => {
            !current.id.is_empty()
                && current.loaded
                && current.messages.is_empty()
                && current.project_id == project
        }
    };
    if same {
        return;
    }
    (cx.ask)(Request::StopStream);
    batch(|| {
        state.turn.set(None);
        state.staged.set(Vec::new());
        state.editing.set(None);
        state.message.reset("");
        state.project_files.set(Vec::new());
        state.active.set(Active {
            id: conversation.clone().unwrap_or_default(),
            project_id: project.clone(),
            messages: Vec::new(),
            loaded: false,
            resumed: false,
        });
    });
    match conversation {
        Some(id) => (cx.ask)(Request::Conversation(id)),
        None => (cx.ask)(Request::NewId),
    }
    if let Some(p) = project {
        (cx.ask)(Request::ProjectFiles(p));
    }
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let state = State::new();
    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();
    let cx = Rc::new(Ctx {
        state: state.clone(),
        router: router.clone(),
        base: base.clone(),
        ask: Rc::clone(requests),
    });

    {
        let cx = Rc::clone(&cx);
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| receive(&cx, m)));
    }

    let env = Env {
        t: Rc::clone(&i18n),
        base: base.clone(),
        ask: Rc::clone(requests),
        login: setup.login_url.clone(),
    };

    (env.ask)(Request::Sidebar);

    let home = {
        let (env, state, cx) = (env.clone(), state.clone(), Rc::clone(&cx));
        move |_: &RouteContext| -> View {
            let path = format!("{}/ui/home", env.base);
            state.notice.set(None);
            (env.ask)(Request::Models);
            // The models gate the chat: until every default model runs, the initializing screen waits for them.
            {
                let (models, router, base) =
                    (state.models.clone(), cx.router.clone(), env.base.clone());
                create_effect(move || {
                    let ready = models.with(|m| m.as_ref().map(|m| m.ready));
                    if ready == Some(false) {
                        untrack(|| router.replace(&format!("{base}/ui/initializing")));
                    }
                });
            }
            {
                let (cx, path) = (Rc::clone(&cx), path.clone());
                create_effect(move || {
                    // Following the address is what opens another conversation.
                    let _ = cx.router.location();
                    untrack(|| open_chat(&cx, &path));
                });
            }
            {
                let ask = Rc::clone(&env.ask);
                on_cleanup(move || ask(Request::StopStream));
            }
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::chat(&env, &state))
        }
    };
    let initializing = {
        let (env, state, cx) = (env.clone(), state.clone(), Rc::clone(&cx));
        move |_: &RouteContext| -> View {
            state.notice.set(None);
            (env.ask)(Request::Models);
            {
                let (ask, models) = (Rc::clone(&env.ask), state.models.clone());
                poll(
                    3000,
                    Rc::new(move || {
                        if models.with_untracked(|m| m.as_ref().is_some_and(|m| m.ready)) {
                            return false;
                        }
                        ask(Request::Models);
                        true
                    }),
                );
            }
            {
                let (models, router, base) =
                    (state.models.clone(), cx.router.clone(), env.base.clone());
                create_effect(move || {
                    if models.with(|m| m.as_ref().is_some_and(|m| m.ready)) {
                        untrack(|| router.replace(&format!("{base}/ui/home")));
                    }
                });
            }
            let (env, state) = (env.clone(), state.clone());
            View::dynamic(move || screens::initializing(&env, &state))
        }
    };

    let routes = Routes::new()
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home)
        .route(&format!("{base}/ui/initializing"), initializing)
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Sage",
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
    let nav = screens::sidebar(&env, &state);

    let banner = screens::banner(&env, &state);
    let dialog = screens::dialogs(&env, &state);

    div()
        .attr("id", "sage")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(&bar, vec![nav]))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(
                    main()
                        .class("sg-main")
                        .child(banner)
                        .child(routes.view(&router)),
                ),
        )
        .child(dialog)
        .into_view()
}
