//! The app against a host.
//!
//! It asks for the data and for every change (`Request`) and is told what came (`Msg`). The host does the talking to the
//! service; the app owns the routes (`{base}/ui/home`, `/ui/folder/:id`, `/ui/message/:id`, `/ui/compose`, `/ui/accounts`,
//! `/ui/admin/...`), the sidebar and the language.

use std::cell::RefCell;
use std::rc::Rc;

use mailroom_core::admin::{
    AdminAccount, AuditEntry, OutboxItem, Page, Settings, SettingsInput, Stats,
};
use mailroom_core::{
    Account, AccountInput, Change, Compose, Folder, Me, MessageList, MessageView, Preset,
    TestResult, Upload,
};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};

use crate::screens::{self, Env};

/// What a message list is asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListQuery {
    /// One folder...
    pub folder: Option<String>,
    /// ...or every folder with this role across the person's mailboxes.
    pub role: Option<String>,
    pub text: String,
    pub unread: bool,
    pub offset: u32,
}

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Who the person is, their mailboxes and the provider suggestions: once, when the app starts.
    Startup,
    Accounts,
    Folders(String),
    List(ListQuery),
    Open(String),
    Change(Change),
    Send(Compose),
    /// Look at a mailbox's server now.
    Refresh(String),
    AddAccount(AccountInput),
    UpdateAccount {
        id: String,
        input: AccountInput,
    },
    TestAccount(AccountInput),
    RemoveAccount(String),
    RemoveUpload(String),
    // The panel.
    Stats,
    Settings,
    SaveSettings(SettingsInput),
    TestTransport,
    AdminAccounts {
        offset: u32,
    },
    SetDisabled {
        id: String,
        disabled: bool,
    },
    AdminRemoveAccount(String),
    Outbox {
        status: String,
        offset: u32,
    },
    Retry(String),
    RemoveQueued(String),
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
    Presets(Vec<Preset>),
    Accounts(Vec<Account>),
    Folders {
        account: String,
        folders: Vec<Folder>,
    },
    Messages(MessageList),
    Message(Box<MessageView>),
    /// A change was made (read, starred): the app asks for the list, and for the message when one is open.
    Changed,
    /// Messages were moved or deleted: they are not where they were.
    Moved,
    Sent,
    AccountSaved(Box<Account>),
    AccountRemoved(String),
    Tested(Box<TestResult>),
    Uploaded(Upload),
    Stats(Stats),
    Settings(Box<Settings>),
    SettingsSaved(Box<Settings>),
    TransportTested(Box<TestResult>),
    AdminAccounts(Page<AdminAccount>),
    Outbox(Page<OutboxItem>),
    Audit(Page<AuditEntry>),
    /// The service refused or failed a change; the text is for the person.
    Rejected(String),
    /// There is no such message or mailbox.
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
    pub presets: Signal<Vec<Preset>>,
    pub accounts: Signal<Option<Vec<Account>>>,
    /// Folders by mailbox, as they arrive.
    pub folders: Signal<Vec<(String, Vec<Folder>)>>,
    pub list: Signal<Option<MessageList>>,
    /// What the list on screen was asked for: a change asks for it again.
    pub asked: Signal<Option<ListQuery>>,
    pub message: Signal<Option<MessageView>>,
    pub tested: Signal<Option<TestResult>>,
    pub uploads: Signal<Vec<Upload>>,
    pub stats: Signal<Option<Stats>>,
    pub settings: Signal<Option<Settings>>,
    pub transport_test: Signal<Option<TestResult>>,
    pub admin_accounts: Signal<Option<Page<AdminAccount>>>,
    pub outbox: Signal<Option<Page<OutboxItem>>>,
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
        Msg::Presets(list) => state.presets.set(list),
        Msg::Accounts(list) => {
            for account in &list {
                requests(Request::Folders(account.id.clone()));
            }
            state.accounts.set(Some(list));
        }
        Msg::Folders { account, folders } => state.folders.update(|all| {
            all.retain(|(id, _)| *id != account);
            all.push((account, folders));
        }),
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
            requests(Request::Accounts);
        }
        Msg::Moved => {
            state.notice.set(None);
            state.message.set(None);
            requests(Request::Accounts);
            router.navigate(&format!("{base}/ui/home"));
        }
        Msg::Sent => {
            state.notice.set(None);
            state.uploads.set(Vec::new());
            // The page it goes to starts clean, so what is said is said after the move.
            router.navigate(&format!("{base}/ui/home"));
            state.done.set(Some(i18n.t("mr_compose_sent")));
        }
        Msg::AccountSaved(account) => {
            state.notice.set(None);
            state.tested.set(None);
            state.done.set(Some(i18n.t("mr_account_saved")));
            state.accounts.update(|list| {
                let list = list.get_or_insert_with(Vec::new);
                list.retain(|a| a.id != account.id);
                list.push(*account);
            });
            requests(Request::Accounts);
        }
        Msg::AccountRemoved(id) => {
            state.accounts.update(|list| {
                if let Some(list) = list {
                    list.retain(|a| a.id != id);
                }
            });
            state.folders.update(|all| all.retain(|(a, _)| *a != id));
        }
        Msg::Tested(result) => state.tested.set(Some(*result)),
        Msg::Uploaded(upload) => state.uploads.update(|list| list.push(upload)),
        Msg::Stats(stats) => state.stats.set(Some(stats)),
        Msg::Settings(settings) => state.settings.set(Some(*settings)),
        Msg::SettingsSaved(settings) => {
            state.notice.set(None);
            state.done.set(Some(i18n.t("mr_admin_saved")));
            state.settings.set(Some(*settings));
        }
        Msg::TransportTested(result) => state.transport_test.set(Some(*result)),
        Msg::AdminAccounts(page) => state.admin_accounts.set(Some(page)),
        Msg::Outbox(page) => state.outbox.set(Some(page)),
        Msg::Audit(page) => state.audit.set(Some(page)),
        Msg::Rejected(why) => state.notice.set(Some(why)),
        Msg::NotFound => state.problem.set(Some(Problem::NotFound)),
        Msg::Failed(why) => state.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => state.problem.set(Some(Problem::SignedOut)),
    }
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let i18n = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let state = State {
        me: create_signal(None),
        presets: create_signal(Vec::new()),
        accounts: create_signal(None),
        folders: create_signal(Vec::new()),
        list: create_signal(None),
        asked: create_signal(None),
        message: create_signal(None),
        tested: create_signal(None),
        uploads: create_signal(Vec::new()),
        stats: create_signal(None),
        settings: create_signal(None),
        transport_test: create_signal(None),
        admin_accounts: create_signal(None),
        outbox: create_signal(None),
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

    let home = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            fresh(&state);
            let query = ListQuery {
                role: Some("inbox".into()),
                ..ListQuery::default()
            };
            state.list.set(None);
            state.asked.set(Some(query.clone()));
            (env.ask)(Request::List(query));
            screens::inbox::page(&env, &state, None)
        }
    };
    let folder = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            fresh(&state);
            let query = ListQuery {
                folder: Some(id.clone()),
                ..ListQuery::default()
            };
            state.list.set(None);
            state.asked.set(Some(query.clone()));
            (env.ask)(Request::List(query));
            screens::inbox::page(&env, &state, Some(id))
        }
    };
    let message = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let id = ctx.param("id").unwrap_or_default();
            fresh(&state);
            state.message.set(None);
            (env.ask)(Request::Open(id.clone()));
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
            if mode != "new" {
                state.message.set(None);
                (env.ask)(Request::Open(id.clone()));
            }
            screens::compose::page(&env, &state, &mode, id)
        }
    };
    let accounts = {
        let (env, state) = (env.clone(), state.clone());
        move |_: &RouteContext| -> View {
            fresh(&state);
            state.tested.set(None);
            (env.ask)(Request::Accounts);
            screens::accounts::page(&env, &state)
        }
    };
    let admin = {
        let (env, state) = (env.clone(), state.clone());
        move |ctx: &RouteContext| -> View {
            let section = ctx.param("section").unwrap_or_else(|| "overview".into());
            fresh(&state);
            screens::admin::page(&env, &state, &section)
        }
    };
    let pages = Routes::new()
        .route(&format!("{base}/ui/home"), home.clone())
        .route(&format!("{base}/ui/home/"), home.clone())
        .route(&format!("{base}/ui"), home.clone())
        .route(&format!("{base}/ui/"), home)
        .route(&format!("{base}/ui/folder/:id"), folder)
        .route(&format!("{base}/ui/message/:id"), message)
        .route(&format!("{base}/ui/compose"), compose.clone())
        .route(&format!("{base}/ui/compose/:mode/:id"), compose)
        .route(&format!("{base}/ui/accounts"), accounts)
        .route(&format!("{base}/ui/admin"), admin.clone())
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

    div()
        .attr("id", "mailroom")
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
                        .class("mr-main")
                        .child(banner)
                        .child(pages.view(&router)),
                ),
        )
        .into_view()
}
