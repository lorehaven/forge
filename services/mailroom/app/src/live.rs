//! The app against a host.
//!
//! It asks for the data and for every change (`Request`) and is told what came (`Msg`). The host does the talking to the
//! service; the app owns the routes (`{base}/ui/home`, `/ui/folder/:id`, `/ui/message/:id`, `/ui/compose`, `/ui/accounts`,
//! `/ui/admin/...`), the sidebar and the language.

use std::cell::RefCell;
use std::rc::Rc;

use mailroom_core::admin::{
    AdminMailbox, AuditEntry, Delivery, Page, Settings, SettingsInput, Stats,
};
use mailroom_core::{
    Change, Compose, Contact, Draft, DraftInput, Folder, Me, MessageList, MessageSummary,
    MessageView, Upload,
};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};

use crate::screens::{self, Env};

/// What a message list is asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListQuery {
    /// `inbox`, `sent` or `trash`.
    pub folder: String,
    pub text: String,
    pub unread: bool,
    pub flagged: bool,
    pub offset: u32,
}

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Who the person is and their folders: once, when the app starts.
    Startup,
    Folders,
    List(ListQuery),
    Open(String),
    /// The rest of the conversation a message belongs to.
    Thread(String),
    Change(Change),
    Send(Compose),
    RemoveUpload(String),
    /// Addresses that start with these letters, to complete a recipient.
    People(String),
    // Drafts and muted senders.
    Drafts,
    OpenDraft(String),
    /// Keep what was typed: a new draft with `None`, else the draft of that id.
    SaveDraft {
        id: Option<String>,
        input: DraftInput,
    },
    DeleteDraft(String),
    Mutes,
    Mute(String),
    Unmute(String),
    SaveSignature(String),
    // The panel.
    Stats,
    Settings,
    SaveSettings(SettingsInput),
    Mailboxes {
        offset: u32,
    },
    SetDisabled {
        owner: String,
        disabled: bool,
    },
    RemoveMailbox(String),
    Deliveries {
        offset: u32,
    },
    Audit {
        offset: u32,
    },
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Me(Me),
    Folders(Vec<Folder>),
    Messages(MessageList),
    Message(Box<MessageView>),
    Thread(Vec<MessageSummary>),
    /// A change was made (read, starred): the app asks for the list, and for the message when one is open.
    Changed,
    /// Messages were moved or deleted: they are not where they were.
    Moved,
    Sent,
    Uploaded(Upload),
    People(Vec<Contact>),
    Drafts(Vec<Draft>),
    /// A draft opened for writing on.
    Draft(Draft),
    /// What was typed is kept (with the id it has now).
    DraftSaved(Draft),
    DraftDeleted,
    Mutes(Vec<String>),
    SignatureSaved(String),
    /// A sender was muted or let back in.
    MuteChanged {
        address: String,
        muted: bool,
    },
    Stats(Stats),
    Settings(Box<Settings>),
    SettingsSaved(Box<Settings>),
    Mailboxes(Page<AdminMailbox>),
    Deliveries(Page<Delivery>),
    Audit(Page<AuditEntry>),
    /// The service refused or failed a change; the text is for the person.
    Rejected(String),
    /// There is no such message.
    NotFound,
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/mailroom`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `de-DE`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    /// Minutes east of UTC, for the times shown.
    pub utc_offset_minutes: i32,
    /// The current Unix time (the list shows today's mail by its time and older mail by its date).
    pub now: i64,
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
pub struct Mailroom {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Mailroom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mailroom").finish_non_exhaustive()
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
    NotFound,
}

/// What the screens read; the host's messages write it.
#[derive(Clone)]
pub(crate) struct State {
    pub me: Signal<Option<Me>>,
    pub folders: Signal<Vec<Folder>>,
    pub list: Signal<Option<MessageList>>,
    /// What the list on screen was asked for: a change asks for it again.
    pub asked: Signal<Option<ListQuery>>,
    pub message: Signal<Option<MessageView>>,
    /// The other messages of the open message's conversation, oldest first.
    pub thread: Signal<Vec<MessageSummary>>,
    pub uploads: Signal<Vec<Upload>>,
    pub people: Signal<Vec<Contact>>,
    pub drafts: Signal<Option<Vec<Draft>>>,
    /// The draft opened to write on.
    pub draft: Signal<Option<Draft>>,
    /// The last draft the service kept (the page writing it reads the id from here).
    pub saved: Signal<Option<Draft>>,
    pub mutes: Signal<Option<Vec<String>>>,
    pub stats: Signal<Option<Stats>>,
    pub settings: Signal<Option<Settings>>,
    pub mailboxes: Signal<Option<Page<AdminMailbox>>>,
    pub deliveries: Signal<Option<Page<Delivery>>>,
    pub audit: Signal<Option<Page<AuditEntry>>>,
    pub problem: Signal<Option<Problem>>,
    /// A refused change, until the person does something else.
    pub notice: Signal<Option<String>>,
    /// Something that worked, said once.
    pub done: Signal<Option<String>>,
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn mailroom(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Mailroom {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Mailroom { view, deliver }
}

fn receive(
    state: &State,
    router: &Router,
    requests: &Rc<dyn Fn(Request)>,
    base: &str,
    i18n: &I18n,
    m: Msg,
) {
    match m {
        Msg::Me(me) => state.me.set(Some(me)),
        Msg::Folders(list) => state.folders.set(list),
        Msg::Thread(list) => state.thread.set(list),
        Msg::Messages(list) => {
            state.problem.set(None);
            state.list.set(Some(list));
        }
        Msg::Message(view) => {
            state.problem.set(None);
            state.notice.set(None);
            state.message.set(Some(*view));
        }
        Msg::Changed => {
            state.notice.set(None);
            if let Some(query) = state.asked.get_untracked() {
                requests(Request::List(query));
            }
            if let Some(open) = state.message.get_untracked() {
                requests(Request::Open(open.summary.id));
            }
            requests(Request::Folders);
        }
        Msg::Moved => {
            state.notice.set(None);
            requests(Request::Folders);
            // From a message: back to the folder it was in. From a list: the list again, without what left it.
            if let Some(open) = state.message.get_untracked() {
                state.message.set(None);
                router.navigate(&format!("{base}/ui/folder/{}", open.summary.folder));
            } else if let Some(query) = state.asked.get_untracked() {
                requests(Request::List(query));
            }
        }
        Msg::Sent => {
            state.notice.set(None);
            state.uploads.set(Vec::new());
            state.people.set(Vec::new());
            requests(Request::Folders);
            // The page it goes to starts clean, so what is said is said after the move.
            router.navigate(&format!("{base}/ui/folder/sent"));
            state.done.set(Some(i18n.t("mr_compose_sent")));
        }
        Msg::Uploaded(upload) => state.uploads.update(|list| list.push(upload)),
        Msg::People(list) => state.people.set(list),
        Msg::Drafts(list) => state.drafts.set(Some(list)),
        Msg::Draft(draft) => state.draft.set(Some(draft)),
        Msg::DraftSaved(draft) => {
            state.notice.set(None);
            state.saved.set(Some(draft));
            requests(Request::Folders);
        }
        Msg::DraftDeleted => {
            requests(Request::Folders);
            if router.location().path == format!("{base}/ui/drafts") {
                requests(Request::Drafts);
            }
        }
        Msg::Mutes(list) => state.mutes.set(Some(list)),
        Msg::SignatureSaved(text) => {
            state.notice.set(None);
            state.me.update(|me| {
                if let Some(me) = me {
                    me.signature = text;
                }
            });
            state.done.set(Some(i18n.t("mr_signature_saved")));
        }
        Msg::MuteChanged { address, muted } => {
            state.notice.set(None);
            state.done.set(Some(
                i18n.t(if muted {
                    "mr_muted_done"
                } else {
                    "mr_unmuted_done"
                })
                .replace("ADDRESS", &address),
            ));
            requests(Request::Mutes);
        }
        Msg::Stats(stats) => state.stats.set(Some(stats)),
        Msg::Settings(settings) => state.settings.set(Some(*settings)),
        Msg::SettingsSaved(settings) => {
            state.notice.set(None);
            state.done.set(Some(i18n.t("mr_admin_saved")));
            state.settings.set(Some(*settings));
        }
        Msg::Mailboxes(page) => state.mailboxes.set(Some(page)),
        Msg::Deliveries(page) => state.deliveries.set(Some(page)),
        Msg::Audit(page) => state.audit.set(Some(page)),
        Msg::Rejected(why) => state.notice.set(Some(why)),
        Msg::NotFound => state.problem.set(Some(Problem::NotFound)),
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

/// How often the page asks again for what may have arrived.
pub const REFRESH_MS: u32 = 30_000;

/// Looks again every [`REFRESH_MS`]: the folder counts always, and the list on screen while the person is on a list page
/// (not while reading, writing or administering, where a refresh would be in the way).
fn refresh(requests: &Rc<dyn Fn(Request)>, state: &State, router: &Router, base: String) {
    fn tick(requests: Rc<dyn Fn(Request)>, state: State, router: Router, base: String) {
        let _ = quench_ui::platform::after(REFRESH_MS, move || {
            if state.problem.get_untracked() != Some(Problem::SignedOut) {
                requests(Request::Folders);
                let path = router.location().path;
                let browsing = path.starts_with(&format!("{base}/ui/folder/"))
                    || path == format!("{base}/ui/home")
                    || path == format!("{base}/ui/");
                if browsing && let Some(query) = state.asked.get_untracked() {
                    requests(Request::List(query));
                }
                tick(requests, state, router, base);
            }
        });
    }
    // A page built with no host (a server render, a test) has no clock, and would look again at once, for ever.
    if quench_ui::platform::installed() {
        tick(Rc::clone(requests), state.clone(), router.clone(), base);
    }
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let state = State {
        me: create_signal(None),
        folders: create_signal(Vec::new()),
        list: create_signal(None),
        asked: create_signal(None),
        message: create_signal(None),
        thread: create_signal(Vec::new()),
        uploads: create_signal(Vec::new()),
        people: create_signal(Vec::new()),
        drafts: create_signal(None),
        draft: create_signal(None),
        saved: create_signal(None),
        mutes: create_signal(None),
        stats: create_signal(None),
        settings: create_signal(None),
        mailboxes: create_signal(None),
        deliveries: create_signal(None),
        audit: create_signal(None),
        problem: create_signal(None),
        notice: create_signal(None),
        done: create_signal(None),
    };
    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();

    {
        let (state, router, requests, base, i18n) = (
            state.clone(),
            router.clone(),
            Rc::clone(requests),
            base.clone(),
            Rc::clone(&i18n),
        );
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| {
            receive(&state, &router, &requests, &base, &i18n, m);
        }));
    }
    requests(Request::Startup);

    let env = Env {
        t: Rc::clone(&i18n),
        base: base.clone(),
        ask: Rc::clone(requests),
        now: setup.now,
        offset: setup.utc_offset_minutes,
    };

    // A page starts clean: nothing from the last one is left on the screen.
    let fresh = |state: &State| {
        state.problem.set(None);
        state.notice.set(None);
        state.done.set(None);
    };

    // A list page: the folder `id` (the inbox for `/ui/home`).
    let list_page = {
        let (env, state) = (env.clone(), state.clone());
        move |id: String| -> View {
            fresh(&state);
            let query = ListQuery {
                folder: id.clone(),
                ..ListQuery::default()
            };
            state.list.set(None);
            state.message.set(None);
            state.asked.set(Some(query.clone()));
            (env.ask)(Request::List(query));
            screens::inbox::page(&env, &state, id)
        }
    };
    let home = {
        let list_page = list_page.clone();
        move |_: &RouteContext| -> View { list_page("inbox".into()) }
    };
    let folder = {
        let list_page = list_page.clone();
        move |ctx: &RouteContext| -> View {
            list_page(ctx.param("id").unwrap_or_else(|| "inbox".into()))
        }
    };
    let message = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            fresh(&state);
            state.message.set(None);
            state.thread.set(Vec::new());
            (env.ask)(Request::Open(id.clone()));
            (env.ask)(Request::Thread(id.clone()));
            screens::reader::page(&env, &state, id)
        }
    };
    let compose = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let mode = ctx.param("mode").unwrap_or_else(|| "new".into());
            let id = ctx.param("id").unwrap_or_default();
            fresh(&state);
            state.uploads.set(Vec::new());
            state.draft.set(None);
            state.saved.set(None);
            if mode == "draft" {
                (env.ask)(Request::OpenDraft(id.clone()));
            } else if mode != "new" {
                state.message.set(None);
                (env.ask)(Request::Open(id.clone()));
            }
            screens::compose::page(&env, &state, &mode, id)
        }
    };
    let drafts = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            fresh(&state);
            state.drafts.set(None);
            (env.ask)(Request::Drafts);
            screens::drafts::drafts_page(&env, &state)
        }
    };
    let muted = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            fresh(&state);
            state.mutes.set(None);
            (env.ask)(Request::Mutes);
            screens::drafts::muted_page(&env, &state)
        }
    };
    // An admin section nobody links to: the overview.
    let admin = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let section = ctx.param("section").unwrap_or_else(|| "overview".into());
            fresh(&state);
            screens::admin::page(&env, &state, &section)
        }
    };
    // The router keeps a page on screen when only a parameter of the same route changed (it expects the page to follow it by
    // reading `ctx.param` inside its views). These pages ask for their data when they are built, so each folder and each tab
    // is a route of its own: moving between them then builds the page again. The `:id` / `:section` routes after them only
    // catch names nobody links to.
    let mut pages = Routes::new()
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui/"), home.clone())
        .route(&format!("{base}/ui"), home);
    for id in mailroom_core::FOLDERS {
        let list_page = list_page.clone();
        pages = pages.route(
            &format!("{base}/ui/folder/{id}"),
            move |_: &RouteContext| -> View { list_page(id.to_string()) },
        );
    }
    for (section, _) in screens::admin::SECTIONS {
        let (env, state) = (env.clone(), state.clone());
        let path = if section == "overview" {
            format!("{base}/ui/admin")
        } else {
            format!("{base}/ui/admin/{section}")
        };
        pages = pages.route(&path, move |_: &RouteContext| -> View {
            fresh(&state);
            screens::admin::page(&env, &state, section)
        });
    }
    let pages = pages
        .route(&format!("{base}/ui/folder/:id"), folder)
        .route(&format!("{base}/ui/message/:id"), message)
        .route(&format!("{base}/ui/drafts"), drafts)
        .route(&format!("{base}/ui/muted"), muted)
        .route(&format!("{base}/ui/compose"), compose.clone())
        .route(&format!("{base}/ui/compose/:mode/:id"), compose)
        .route(&format!("{base}/ui/admin/:section"), admin)
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Mailroom",
            icon: "envelope",
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
        let (problem, notice, done, login) = (
            state.problem.clone(),
            state.notice.clone(),
            state.done.clone(),
            setup.login_url.clone(),
        );
        let t = Rc::clone(&i18n);
        View::dynamic(move || match (problem.get(), notice.get(), done.get()) {
            (Some(Problem::Failed(why)), _, _) => p()
                .attr("id", "problem")
                .class("fui-notice fui-notice-error")
                .text(why)
                .into_view(),
            (Some(Problem::SignedOut), _, _) => p()
                .attr("id", "problem")
                .class("fui-notice fui-notice-error")
                .child(span().text(format!("{} ", t.t("mr_session_ended"))))
                .child(
                    a().attr("id", "sign-in")
                        .attr("href", login.clone())
                        .text(t.t("mr_sign_in")),
                )
                .into_view(),
            (_, Some(why), _) => p()
                .attr("id", "notice")
                .class("fui-notice fui-notice-error")
                .text(why)
                .into_view(),
            (_, _, Some(ok)) => p()
                .attr("id", "done")
                .class("fui-notice fui-notice-ok")
                .text(ok)
                .into_view(),
            _ => View::Empty,
        })
    };

    // The tab says how much is waiting (the shell's script reads this), and the page looks again now and then.
    let waiting_badge = {
        let (state, i18n) = (state.clone(), Rc::clone(&i18n));
        View::dynamic(move || {
            let folders = state.folders.get();
            // Until the counts are known there is nothing to say (a count of nothing is not "no mail": the first real count must
            // not look like mail that arrived).
            let Some(inbox) = folders.iter().find(|f| f.id == "inbox") else {
                return span()
                    .attr("id", "unread-badge")
                    .attr("hidden", "")
                    .into_view();
            };
            let unread = inbox.unread;
            span()
                .attr("id", "unread-badge")
                .attr("data-fui-badge", unread.to_string())
                .attr(
                    "data-fui-badge-text",
                    format!("{unread} {}", i18n.t("mr_notify_unread")),
                )
                .attr("hidden", "")
                .into_view()
        })
    };
    refresh(requests, &state, &router, base.clone());

    div()
        .attr("id", "mailroom")
        .class("fui-app fui-client")
        .child(waiting_badge)
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
                        .class("mr-main")
                        .child(banner)
                        .child(pages.view(&router)),
                ),
        )
        .into_view()
}
