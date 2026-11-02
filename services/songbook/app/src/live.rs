//! The app against a host.
//!
//! It asks for the data and for every change (`Request`) and is told what came (`Msg`). The host does the talking to the
//! service; the app owns the routes (`{base}/ui/library`, `/pipeline`, `/discover`, `/activity`), the polling of the job and
//! the language.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{History, RouteContext, Router, Routes};
use songbook_core::{Browse, Filter, Job, JobStart, NewArtist, Schedule, State};

use crate::screens;

/// What the app asks its host for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The followed artists, categories, genres and suggestions.
    State,
    /// How big the library is and how much of it is on disk.
    Stats,
    /// The albums that match a filter.
    Browse(Filter),
    /// Set the link of a song; an empty `url` clears it.
    SaveLink { track_id: i64, url: String },
    /// Skip an album in later syncs and drop it from the index.
    Blacklist { artist: String, album_id: i64 },
    /// Follow these artists.
    AddArtists(Vec<NewArtist>),
    /// Stop following an artist.
    RemoveArtist(String),
    /// Put an artist in a category (empty for none).
    SetCategory { name: String, category: String },
    /// Keep only the albums of an artist that are in these genres (empty: all).
    SetGenres { name: String, genres: String },
    /// Create a category.
    AddCategory(String),
    /// Give a category another name.
    RenameCategory { name: String, new_name: String },
    /// Start a job.
    StartJob(JobStart),
    /// The job's state and its log from character `from`.
    Job { from: u64 },
    /// The nightly schedule.
    Schedule,
    /// Save the nightly schedule.
    SaveSchedule(Schedule),
    /// The person chose a language (the host keeps it for next time).
    Language(String),
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    State(State),
    Stats(songbook_core::Stats),
    Browse(Browse),
    Job(Job),
    Schedule(Schedule),
    ScheduleSaved(Schedule),
    /// Whether the job started (`false`: one is already running).
    Started(bool),
    LinkSaved,
    /// How many tracks left the index.
    Blacklisted(u32),
    /// The names that were new.
    ArtistsAdded(Vec<String>),
    ArtistRemoved,
    /// The categories that were new.
    CategoryAdded(Vec<String>),
    CategorySet,
    CategoryRenamed,
    /// The service refused the change; the text says why.
    Rejected(String),
    /// The service could not be reached, or answered something else.
    Failed(String),
    /// The session ended: the person has to sign in again.
    SignedOut,
}

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/songbook`), empty when it has none.
    pub base: String,
    /// The language to start in (`en-US`, `pl-PL`...).
    pub language: String,
    /// Where signing in starts (a full page load, not a route of the app).
    pub login_url: String,
    pub history: Rc<dyn History>,
    /// How often the job is asked about while it runs.
    pub busy_ms: u32,
    /// How often it is asked about otherwise (for the pill in the top row).
    pub idle_ms: u32,
}

impl std::fmt::Debug for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup")
            .field("base", &self.base)
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

/// The app and the function that feeds it.
pub struct Songbook {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Songbook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Songbook").finish_non_exhaustive()
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

/// Why the page shows a banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Problem {
    Failed(String),
    SignedOut,
}

/// A change in flight, and what to say or do when the service has answered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum Pending {
    #[default]
    Nothing,
    Link {
        cleared: bool,
    },
    Blacklist {
        album: String,
    },
    AddOne {
        name: String,
    },
    /// Artists chosen on the Discover screen; then a sync.
    AddSuggested,
    Remove {
        name: String,
    },
    /// A category being renamed.
    RenameCategory {
        from: String,
        to: String,
    },
    /// A new category, then the artist goes into it.
    NewCategoryThen {
        name: String,
        category: String,
    },
}

/// A dialog over the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Dialog {
    Link(songbook_core::Track),
    Blacklist(songbook_core::Album),
    Remove(String),
    Category(songbook_core::ArtistRow),
    /// Rename this category.
    RenameCategory(String),
    Add,
}

/// Everything the screens share and change.
#[derive(Clone)]
pub(crate) struct Model {
    pub state: Signal<Option<State>>,
    pub stats: Signal<Option<songbook_core::Stats>>,
    pub browse: Signal<Option<Browse>>,
    pub filter: Signal<Filter>,
    /// The job's state; its log is in `log`.
    pub job: Signal<Job>,
    pub log: Signal<String>,
    /// Where the log we hold ends: the next poll asks from here.
    pub offset: Signal<u64>,
    pub singles: Signal<bool>,
    pub retry: Signal<bool>,
    pub limit_links: quench_ui::form::Field,
    pub limit_download: quench_ui::form::Field,
    pub top: quench_ui::form::Field,
    pub scope: quench_ui::form::Field,
    /// The suggestions ticked on the Discover screen, by artist name.
    pub picked: Signal<Vec<String>>,
    /// The categories the sidebar has expanded (the one holding the selected artist always is).
    pub open: Signal<Vec<String>>,
    /// The nightly schedule as the service has it (its status lines), and the form that edits it.
    pub schedule: Signal<Option<Schedule>>,
    pub sched_enabled: Signal<bool>,
    pub sched_sync: Signal<bool>,
    pub sched_links: Signal<bool>,
    pub sched_download: Signal<bool>,
    pub sched_convert: Signal<bool>,
    pub sched_suggestions: Signal<bool>,
    pub sched_fresh: Signal<bool>,
    pub sched_singles: Signal<bool>,
    pub sched_time: quench_ui::form::Field,
    pub sched_zone: quench_ui::form::Field,
    /// Has the form been filled from the service yet? (Later answers update the status lines, not what is being typed.)
    pub sched_filled: Rc<Cell<bool>>,
    pub dialog: Signal<Option<Dialog>>,
    pub snack: Signal<Option<String>>,
    pub problem: Signal<Option<Problem>>,
    pub pending: Rc<RefCell<Pending>>,
}

impl Model {
    fn new() -> Self {
        use quench_ui::form::Field;
        Self {
            state: create_signal(None),
            stats: create_signal(None),
            browse: create_signal(None),
            filter: create_signal(Filter::default()),
            job: create_signal(Job::default()),
            log: create_signal(String::new()),
            offset: create_signal(0),
            singles: create_signal(false),
            retry: create_signal(false),
            limit_links: Field::new("limit-links", ""),
            limit_download: Field::new("limit-download", ""),
            top: Field::new("top", "50"),
            scope: Field::new("scope", ""),
            picked: create_signal(Vec::new()),
            open: create_signal(Vec::new()),
            schedule: create_signal(None),
            sched_enabled: create_signal(false),
            sched_sync: create_signal(true),
            sched_links: create_signal(true),
            sched_download: create_signal(true),
            sched_convert: create_signal(true),
            sched_suggestions: create_signal(true),
            sched_fresh: create_signal(true),
            sched_singles: create_signal(false),
            sched_time: Field::new("sched-time", "03:00"),
            sched_zone: Field::new("sched-zone", ""),
            sched_filled: Rc::default(),
            dialog: create_signal(None),
            snack: create_signal(None),
            problem: create_signal(None),
            pending: Rc::default(),
        }
    }
}

/// What every screen needs: words, a way to ask, the service's prefix and the shared model.
#[derive(Clone)]
pub(crate) struct Env {
    pub t: Rc<I18n>,
    pub ask: Rc<dyn Fn(Request)>,
    pub base: String,
    pub m: Model,
    snack_gen: Rc<Cell<u32>>,
}

impl Env {
    /// `{base}/ui{path}`.
    pub fn path(&self, path: &str) -> String {
        format!("{}/ui{path}", self.base)
    }

    /// Tells the person something for a few seconds.
    pub fn say(&self, text: String) {
        let generation = self.snack_gen.get().wrapping_add(1);
        self.snack_gen.set(generation);
        self.m.snack.set(Some(text));
        let (snack, current) = (self.m.snack.clone(), Rc::clone(&self.snack_gen));
        let _ = quench_ui::platform::after(3500, move || {
            if current.get() == generation {
                snack.set(None);
            }
        });
    }

    /// Asks for the albums the filter selects.
    pub fn load(&self) {
        (self.ask)(Request::Browse(self.m.filter.get_untracked()));
    }

    /// Asks for the state and the albums again, after a change.
    pub fn refresh(&self) {
        (self.ask)(Request::State);
        (self.ask)(Request::Stats);
        self.load();
    }
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn songbook(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Songbook {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Songbook { view, deliver }
}

/// Asks about the job now and again until the app is gone; faster while it runs.
fn watch_job(env: &Env, busy_ms: u32, idle_ms: u32) {
    fn tick(env: Env, busy_ms: u32, idle_ms: u32, alive: Rc<Cell<bool>>) {
        (env.ask)(Request::Job {
            from: env.m.offset.get_untracked(),
        });
        let wait = if env.m.job.get_untracked().running {
            busy_ms
        } else {
            idle_ms
        };
        let _ = quench_ui::platform::after(wait, move || {
            if alive.get() {
                tick(env, busy_ms, idle_ms, alive);
            }
        });
    }
    let alive = Rc::new(Cell::new(true));
    let gone = Rc::clone(&alive);
    on_cleanup(move || gone.set(false));
    tick(env.clone(), busy_ms, idle_ms, alive);
}

/// Puts the schedule into the form.
fn fill_schedule(m: &Model, s: &Schedule) {
    m.sched_enabled.set(s.enabled);
    m.sched_sync.set(s.sync);
    m.sched_links.set(s.links);
    m.sched_download.set(s.download);
    m.sched_convert.set(s.convert);
    m.sched_suggestions.set(s.suggestions);
    m.sched_fresh.set(s.fresh_ratings);
    m.sched_singles.set(s.singles);
    m.sched_time.set(&s.time);
    m.sched_zone.set(&s.zone);
}

/// What a message from the host does to the model.
#[allow(clippy::too_many_lines)]
fn receive(env: &Env, msg: Msg) {
    let m = &env.m;
    let t = &env.t;
    let take = || std::mem::take(&mut *m.pending.borrow_mut());
    match msg {
        Msg::State(state) => {
            m.problem.set(None);
            m.state.set(Some(state));
        }
        Msg::Stats(stats) => m.stats.set(Some(stats)),
        Msg::Schedule(schedule) => {
            m.problem.set(None);
            if !m.sched_filled.replace(true) {
                fill_schedule(m, &schedule);
            }
            m.schedule.set(Some(schedule));
        }
        Msg::ScheduleSaved(schedule) => {
            fill_schedule(m, &schedule);
            m.schedule.set(Some(schedule));
            env.say(t.t("ui_sched_saved"));
        }
        Msg::Browse(browse) => {
            m.problem.set(None);
            m.browse.set(Some(browse));
        }
        Msg::Job(mut job) => {
            m.problem.set(None);
            let was = m.job.get_untracked();
            let from = m.offset.get_untracked();
            if job.offset < from {
                m.log.set(std::mem::take(&mut job.log));
            } else if !job.log.is_empty() {
                let chunk = std::mem::take(&mut job.log);
                m.log.update(|log| log.push_str(&chunk));
            }
            m.offset.set(job.offset);
            let finished = was.running && !job.running;
            let name = job.name.clone().unwrap_or_default();
            let error = job.error.clone();
            m.job.set(job);
            if finished {
                env.refresh();
                (env.ask)(Request::Schedule);
                env.say(match error {
                    Some(why) => t.tr(
                        "ui_snack_failed",
                        &[("name", name.into()), ("why", why.into())],
                    ),
                    None => t.tr("ui_snack_finished", &[("name", name.into())]),
                });
            }
        }
        Msg::Started(true) => {
            m.log.set(String::new());
            m.offset.set(0);
            (env.ask)(Request::Job { from: 0 });
        }
        Msg::Started(false) => env.say(t.t("ui_job_busy")),
        Msg::LinkSaved => {
            m.dialog.set(None);
            if let Pending::Link { cleared } = take() {
                env.say(t.t(if cleared {
                    "ui_link_removed"
                } else {
                    "ui_link_saved"
                }));
            }
            env.refresh();
        }
        Msg::Blacklisted(_) => {
            m.dialog.set(None);
            if let Pending::Blacklist { album } = take() {
                env.say(t.tr("ui_bl_done", &[("album", album.into())]));
            }
            env.refresh();
        }
        Msg::ArtistsAdded(names) => match take() {
            Pending::AddOne { name } => {
                m.dialog.set(None);
                env.say(if names.is_empty() {
                    t.tr("ui_add_exists", &[("name", name.into())])
                } else {
                    t.tr("ui_add_done", &[("name", name.into())])
                });
                (env.ask)(Request::State);
            }
            Pending::AddSuggested => {
                m.picked.set(Vec::new());
                env.say(t.tr("ui_added_n", &[("n", names.len().into())]));
                (env.ask)(Request::State);
                (env.ask)(Request::StartJob(JobStart {
                    name: "sync".into(),
                    singles: m.singles.get_untracked(),
                    ..JobStart::default()
                }));
            }
            _ => (env.ask)(Request::State),
        },
        Msg::ArtistRemoved => {
            if let Pending::Remove { name } = take() {
                m.dialog.set(None);
                if m.filter.get_untracked().artist == name {
                    m.filter.update(|f| f.artist.clear());
                }
                env.say(t.tr("ui_rm_done", &[("name", name.into())]));
            }
            env.refresh();
        }
        Msg::CategoryAdded(_) => {
            if let Pending::NewCategoryThen { name, category } = take() {
                (env.ask)(Request::SetCategory { name, category });
            }
        }
        Msg::CategoryRenamed => {
            if let Pending::RenameCategory { from, to } = take() {
                m.dialog.set(None);
                // The filter and the open list follow the new name.
                m.filter.update(|f| {
                    if f.category == from {
                        f.category.clone_from(&to);
                    }
                });
                m.open.update(|open| {
                    for c in open.iter_mut().filter(|c| **c == from) {
                        c.clone_from(&to);
                    }
                });
                env.say(t.tr("ui_catrename_done", &[("name", to.into())]));
            }
            env.refresh();
        }
        Msg::CategorySet => {
            m.dialog.set(None);
            env.say(t.t("ui_cat_saved"));
            env.refresh();
        }
        Msg::Rejected(why) => {
            take();
            env.say(why);
        }
        Msg::Failed(why) => m.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => m.problem.set(Some(Problem::SignedOut)),
    }
}

fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let t = Rc::new(I18n::new(&LOCALES, &setup.language).expect("the translations parse"));
    let env = Env {
        t: Rc::clone(&t),
        ask: Rc::clone(requests),
        base: setup.base.clone(),
        m: Model::new(),
        snack_gen: Rc::default(),
    };
    {
        let env = env.clone();
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| receive(&env, m)));
    }

    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    let base = setup.base.clone();

    (env.ask)(Request::State);
    watch_job(&env, setup.busy_ms, setup.idle_ms);

    let library = {
        let env = env.clone();
        move |_: &RouteContext| -> View {
            env.load();
            (env.ask)(Request::Stats);
            screens::library(&env)
        }
    };
    let page = |screen: fn(&Env) -> View| {
        let env = env.clone();
        move |_: &RouteContext| -> View { screen(&env) }
    };
    let schedule = {
        let env = env.clone();
        move |_: &RouteContext| -> View {
            env.m.sched_filled.set(false);
            (env.ask)(Request::Schedule);
            screens::schedule(&env)
        }
    };
    let screens_at = Routes::new()
        .route(&format!("{base}/ui/library"), library.clone())
        .route(&format!("{base}/ui/pipeline"), page(screens::pipeline))
        .route(&format!("{base}/ui/discover"), page(screens::discover))
        .route(&format!("{base}/ui/activity"), page(screens::activity))
        .route(&format!("{base}/ui/schedule"), schedule)
        .route(&format!("{base}/ui/home"), library.clone())
        .route(&format!("{base}/ui/home/"), library.clone())
        .route(&format!("{base}/ui"), library.clone())
        .route(&format!("{base}/ui/"), library)
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    let bar = forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Songbook",
            icon: "database",
        },
        image: Some(format!("{base}/ui/assets/favicon.png")),
        home: format!("{base}/ui/library"),
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
        let (bar, t) = (bar.clone(), Rc::clone(&t));
        move || -> View { forge_shell::topbar(&bar, &t).into_view() }
    };

    div()
        .attr("id", "songbook")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(
            &bar,
            vec![
                screens::pages_nav(&env, &router),
                screens::artists_nav(&env),
            ],
        ))
        .child(
            div()
                .class("fui-client-col")
                .child(View::dynamic(top))
                .child(
                    main()
                        .class("sb-main")
                        .child(screens::toolbar(&env))
                        .child(screens::banner(&env, &setup.login_url))
                        .child(screens_at.view(&router)),
                ),
        )
        .child(screens::dialogs(&env))
        .child(screens::snack(&env))
        .into_view()
}
