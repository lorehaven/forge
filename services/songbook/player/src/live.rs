//! The player as one app: it asks its host for the library (`Request`) and is told what came (`Msg`); it commands the audio
//! engine and hears it through the two hidden elements of [`crate::engine`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use quench_ui::prelude::*;
use quench_ui::router::{History, Router, Routes};
use songbook_core::{
    AlbumInfo, AlbumQuery, Home, Overview, PlaylistInfo, RecentItem, Search, SongIds, SongInfo,
    SongList, SongQuery,
};

use crate::engine::{self, Command, Event, Outbox};
use crate::screens;

/// What the player asks of its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Overview,
    /// What to listen to.
    Home,
    /// Albums for the page `key`.
    Albums(AlbumQuery, String),
    /// Songs for the page `key` (`meta`: the songs the engine is playing, for the lock screen).
    Songs(SongQuery, String),
    /// The ids of the songs a query picks, for what to do with them.
    Ids(SongQuery, Intent),
    Search(String),
    Playlists,
    CreatePlaylist(String),
    RenamePlaylist(i64, String),
    DeletePlaylist(i64),
    AddToPlaylist(i64, Vec<i64>),
    SetPlaylist(i64, Vec<i64>),
    /// What the person chose to start playing, newest first (the home page's "Recently played").
    Contexts,
    /// The person chose to start this: the service keeps it.
    Remember(RecentItem),
}

/// What to do with the songs a query picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Play them (all, or shuffled).
    Play { shuffle: bool },
    /// Put them after the song playing, or at the end of the queue.
    Enqueue { next: bool },
    /// Add them to a playlist.
    ToPlaylist(i64),
    /// Keep them on the device.
    Keep,
}

/// What the host tells the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg {
    Overview(Overview),
    Home(Home),
    Albums(String, Vec<AlbumInfo>),
    Songs(String, SongList),
    Ids(Intent, SongIds),
    Found(String, Search),
    Playlists(Vec<PlaylistInfo>),
    /// A playlist was made or changed.
    Playlist(PlaylistInfo),
    PlaylistGone(i64),
    /// What the person chose to start playing, newest first.
    Contexts(Vec<RecentItem>),
    /// The service refused; the text says why.
    Rejected(String),
    /// The service could not be reached.
    Failed(String),
    /// The session ended: sign in again.
    SignedOut,
}

/// What the host knows that the player needs.
#[derive(Clone)]
pub struct Setup {
    /// The service's path prefix (`/songbook`), empty when it has none.
    pub base: String,
    pub history: Rc<dyn History>,
    /// Where signing in starts (a full page load).
    pub login_url: String,
    /// A phone or a tablet: no way to the admin pages.
    pub mobile: bool,
    /// Where the commands for the engine go as JSON text, one each, when the engine is not the page's script (an app that
    /// plays with the system's own player). The engine answers through [`Player::hear`].
    pub engine: Option<Rc<dyn Fn(String)>>,
    /// What *Sign out* does when it is not a link to a page (an app signs out by itself).
    pub sign_out: Option<Rc<dyn Fn()>>,
}

impl std::fmt::Debug for Setup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Setup")
            .field("base", &self.base)
            .field("mobile", &self.mobile)
            .finish_non_exhaustive()
    }
}

/// The player and the function that feeds it.
pub struct Player {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
    /// Hand every event of the engine to this (when `Setup::engine` is set), on the thread of the views.
    pub hear: Rc<dyn Fn(Event)>,
    /// What the Back button of a phone does: closes the box that is open, else goes back a page. False when there is nothing
    /// to go back to (the app then leaves). Only once the view is mounted.
    pub back: Rc<dyn Fn() -> bool>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player").finish_non_exhaustive()
    }
}

/// A box over the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// Choose a playlist for these songs (or make one).
    AddTo(Vec<i64>),
    /// Crossfade and the songs on this device.
    Settings,
    /// What plays next.
    Queue,
    /// The song playing, large (on a phone).
    Playing,
}

/// Why the page is not showing the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    SignedOut,
    Failed(String),
}

type Inbox = Rc<RefCell<Option<Rc<dyn Fn(Msg)>>>>;
type Ears = Rc<RefCell<Option<Rc<dyn Fn(Event)>>>>;
type Backs = Rc<RefCell<Option<Rc<dyn Fn() -> bool>>>>;

/// Everything the screens read.
#[derive(Debug)]
pub(crate) struct Model {
    pub overview: Signal<Option<Overview>>,
    pub home: Signal<Option<Home>>,
    /// The songs played last, newest first.
    pub recent: Signal<Vec<i64>>,
    /// What was chosen to play last (a mix, an artist, an album, a playlist, a song), newest first.
    pub contexts: Signal<Vec<RecentItem>>,
    pub playlists: Signal<Vec<PlaylistInfo>>,
    /// The songs and albums of the page being shown, and the key of the page they were asked for.
    pub songs: Signal<(String, SongList)>,
    pub albums: Signal<(String, Vec<AlbumInfo>)>,
    pub found: Signal<Option<(String, Search)>>,
    pub engine: Signal<engine::State>,
    pub queue: Signal<engine::Queue>,
    pub cached: Signal<Vec<i64>>,
    pub saving: Signal<(u32, u32)>,
    pub storage: Signal<(u64, u64)>,
    /// What was played and is kept for next time, and the most that is kept (0: the engine has no such cache).
    pub cache: Signal<(u64, u64)>,
    /// Songs seen, by id; `known` changes when one is added.
    pub library: RefCell<HashMap<i64, SongInfo>>,
    pub known: Signal<u32>,
    pub outbox: RefCell<Outbox>,
    pub cmd: Signal<String>,
    pub toast: Signal<String>,
    pub dialog: Signal<Option<Dialog>>,
    pub problem: Signal<Option<Problem>>,
    /// Songs to put in the playlist that is being made.
    pub pending_add: RefCell<Vec<i64>>,
    /// What was said last, to tell a new toast from the same one again.
    pub toast_gen: std::cell::Cell<u32>,
    pub search_gen: std::cell::Cell<u32>,
}

impl Model {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            overview: create_signal(None),
            home: create_signal(None),
            recent: create_signal(Vec::new()),
            contexts: create_signal(Vec::new()),
            playlists: create_signal(Vec::new()),
            songs: create_signal((String::new(), SongList::default())),
            albums: create_signal((String::new(), Vec::new())),
            found: create_signal(None),
            engine: create_signal(engine::State {
                volume: 1.0,
                index: -1,
                ..engine::State::default()
            }),
            queue: create_signal(engine::Queue::default()),
            cached: create_signal(Vec::new()),
            saving: create_signal((0, 0)),
            storage: create_signal((0, 0)),
            cache: create_signal((0, 0)),
            library: RefCell::default(),
            known: create_signal(0),
            outbox: RefCell::default(),
            cmd: create_signal("[]".to_string()),
            toast: create_signal(String::new()),
            dialog: create_signal(None),
            problem: create_signal(None),
            pending_add: RefCell::default(),
            toast_gen: std::cell::Cell::new(0),
            search_gen: std::cell::Cell::new(0),
        })
    }
}

/// What the screens share: the model, the host, and the few things every screen does.
#[derive(Clone)]
pub(crate) struct Env {
    pub m: Rc<Model>,
    pub ask: Rc<dyn Fn(Request)>,
    pub base: String,
    pub mobile: bool,
    pub login_url: String,
    /// See [`Setup::engine`].
    pub link: Option<Rc<dyn Fn(String)>>,
    /// See [`Setup::sign_out`].
    pub sign_out: Option<Rc<dyn Fn()>>,
}

impl Env {
    /// `{base}/ui{path}`.
    pub fn path(&self, path: &str) -> String {
        format!("{}/ui{path}", self.base)
    }

    /// The address of a picture: `kind` is `album` (`key`: its id), `song` (its id) or `artist` (the name); `width` is 0 for
    /// the saved picture as it is, else about how wide it is shown, in pixels (the service makes 64, 160 and 320).
    pub fn art(&self, kind: &str, key: &str, width: u32) -> String {
        let w = if width == 0 {
            String::new()
        } else {
            format!("w={width}")
        };
        let base = format!("{}/api/v1/art/{kind}", self.base);
        match (kind, w.is_empty()) {
            ("artist", true) => format!("{base}?name={}", crate::screens::enc(key)),
            ("artist", false) => format!("{base}?name={}&{w}", crate::screens::enc(key)),
            (_, true) => format!("{base}/{key}"),
            (_, false) => format!("{base}/{key}?{w}"),
        }
    }

    /// Tells the engine to do something.
    pub fn engine(&self, command: &Command) {
        let text = self.m.outbox.borrow_mut().push(command);
        if let Some(link) = &self.link {
            if let Some(one) = self.m.outbox.borrow().last() {
                link(one);
            }
        } else {
            self.m.cmd.set(text);
        }
    }

    /// Says something for a few seconds.
    pub fn say(&self, text: impl Into<String>) {
        let generation = self.m.toast_gen.get().wrapping_add(1);
        self.m.toast_gen.set(generation);
        self.m.toast.set(text.into());
        let env = self.clone();
        quench_ui::platform::after(3500, move || {
            if env.m.toast_gen.get() == generation {
                env.m.toast.set(String::new());
            }
        });
    }

    /// Remembers songs the service described.
    pub fn remember(&self, songs: &[SongInfo]) {
        if songs.is_empty() {
            return;
        }
        {
            let mut library = self.m.library.borrow_mut();
            for song in songs {
                library.insert(song.id, song.clone());
            }
        }
        self.m.known.update(|n| *n = n.wrapping_add(1));
    }

    /// A song seen before. Reading it follows `known`, so a view that shows it fills in when it arrives.
    pub fn song(&self, id: i64) -> Option<SongInfo> {
        let _ = self.m.known.get();
        self.m.library.borrow().get(&id).cloned()
    }

    /// Asks for the songs among `ids` that are not known yet.
    pub fn want(&self, ids: &[i64]) {
        let missing: Vec<String> = {
            let library = self.m.library.borrow();
            ids.iter()
                .filter(|id| !library.contains_key(id))
                .take(100)
                .map(ToString::to_string)
                .collect()
        };
        if !missing.is_empty() {
            (self.ask)(Request::Songs(
                SongQuery {
                    ids: missing.join(","),
                    limit: 100,
                    ..SongQuery::default()
                },
                "meta".into(),
            ));
        }
    }

    /// Plays `ids` from `index`.
    pub fn play(&self, ids: Vec<i64>, index: i64, shuffle: bool) {
        if ids.is_empty() {
            return;
        }
        self.engine(&Command::Load {
            queue: ids,
            index,
            shuffle,
        });
    }

    /// The person chose to start `item`: shown first in "Recently played" at once, and kept by the service.
    pub fn chose(&self, item: RecentItem) {
        let key = item.key();
        self.m.contexts.update(|list| {
            list.retain(|i| i.key() != key);
            list.insert(0, item.clone());
            list.truncate(30);
        });
        (self.ask)(Request::Remember(item));
    }

    /// Starts what `query` picks, and remembers that it was chosen (`name` and `sub` are what the list will say).
    pub fn start(&self, query: SongQuery, shuffle: bool, name: &str, sub: &str) {
        self.chose(RecentItem::of(&query, shuffle, name, sub));
        (self.ask)(Request::Ids(query, Intent::Play { shuffle }));
    }

    pub fn open(&self, dialog: Dialog) {
        self.m.dialog.set(Some(dialog));
    }

    pub fn close(&self) {
        self.m.dialog.set(None);
    }
}

fn receive(env: &Env, msg: Msg) {
    let m = &env.m;
    match msg {
        Msg::Overview(o) => m.overview.set(Some(o)),
        Msg::Home(h) => {
            env.remember(&h.popular);
            env.remember(&h.discover);
            m.home.set(Some(h));
        }
        Msg::Albums(key, albums) => m.albums.set((key, albums)),
        Msg::Songs(key, list) => {
            env.remember(&list.songs);
            if key == "meta" {
                send_meta(env);
            } else {
                m.songs.set((key, list));
            }
        }
        Msg::Ids(intent, ids) => act(env, &intent, ids),
        Msg::Found(key, found) => {
            env.remember(&found.songs);
            m.found.set(Some((key, found)));
        }
        Msg::Playlists(list) => m.playlists.set(list),
        Msg::Playlist(info) => {
            m.playlists
                .update(|list| match list.iter_mut().find(|p| p.id == info.id) {
                    Some(slot) => *slot = info.clone(),
                    None => list.push(info.clone()),
                });
            let pending = std::mem::take(&mut *m.pending_add.borrow_mut());
            if pending.is_empty() {
                (env.ask)(Request::Playlists);
            } else {
                (env.ask)(Request::AddToPlaylist(info.id, pending));
                env.say(format!("Added to {}", info.name));
                env.close();
            }
        }
        Msg::Contexts(list) => m.contexts.set(list),
        Msg::PlaylistGone(id) => m.playlists.update(|list| list.retain(|p| p.id != id)),
        Msg::Rejected(why) => env.say(why),
        Msg::Failed(why) => m.problem.set(Some(Problem::Failed(why))),
        Msg::SignedOut => m.problem.set(Some(Problem::SignedOut)),
    }
}

/// What the player does with the songs a query picked.
fn act(env: &Env, intent: &Intent, picked: SongIds) {
    let ids = picked.ids;
    if ids.is_empty() {
        env.say("Nothing to play there");
        return;
    }
    match intent {
        Intent::Play { shuffle } => env.play(ids, -1, *shuffle),
        Intent::Enqueue { next } => {
            let next = *next;
            env.say(if next {
                "Playing next"
            } else {
                "Added to the queue"
            });
            env.engine(&Command::Enqueue { ids, next });
        }
        Intent::ToPlaylist(id) => (env.ask)(Request::AddToPlaylist(*id, ids)),
        Intent::Keep => env.engine(&Command::Keep(ids)),
    }
}

/// Tells the engine about the song playing and the one after, for the lock screen.
fn send_meta(env: &Env) {
    let queue = env.m.queue.get_untracked();
    let current = env.m.engine.get_untracked().id;
    let songs: Vec<_> = std::iter::once(current)
        .chain(queue.ids.iter().copied())
        .filter_map(|id| env.m.library.borrow().get(&id).cloned())
        .map(|s| (s.id, s.title.clone(), s.artists.join(", "), s.album.clone()))
        .collect();
    if !songs.is_empty() {
        env.engine(&Command::Meta(songs));
    }
}

/// An event of the engine.
fn heard(env: &Env, event: Event) {
    let m = &env.m;
    match event {
        Event::State(state) => {
            // A phone's bar shows no clock: a change of the position alone is not worth drawing the page again (the box of the
            // song playing, large, shows it).
            if env.mobile && !matches!(m.dialog.get_untracked(), Some(Dialog::Playing)) {
                let quiet = m.engine.with_untracked(|old| {
                    let (mut a, mut b) = (old.clone(), state.clone());
                    (a.pos, a.dur, b.pos, b.dur) = (0.0, 0.0, 0.0, 0.0);
                    a == b
                });
                if quiet {
                    return;
                }
            }
            let changed = m.engine.with_untracked(|old| old.id != state.id);
            m.engine.set(state.clone());
            if changed && state.id != 0 {
                if env.m.library.borrow().contains_key(&state.id) {
                    send_meta(env);
                } else {
                    env.want(&[state.id]);
                }
            }
        }
        Event::Queue(queue) => {
            env.want(&queue.ids.iter().copied().take(30).collect::<Vec<_>>());
            m.queue.set(queue);
        }
        Event::Recent(ids) => {
            env.want(&ids.iter().copied().take(12).collect::<Vec<_>>());
            m.recent.set(ids);
        }
        Event::Cached(mut ids) => {
            ids.sort_unstable();
            m.cached.set(ids);
        }
        Event::Saving { done, total } => m.saving.set((done, total)),
        Event::Storage { used, quota } => m.storage.set((used, quota)),
        Event::Cache { used, limit } => m.cache.set((used, limit)),
        Event::Error(why) => env.say(why),
    }
}

/// The two elements the engine and the screens talk through.
fn bridge(env: &Env) -> View {
    let cmd = env.m.cmd.clone();
    let hear = env.clone();
    fragment([
        div()
            .attr("id", "sb-engine")
            .attr("hidden", "")
            .attr("data-base", env.base.clone())
            .attr_fn("data-cmd", move || cmd.get())
            .into_view(),
        input()
            .attr("id", "sb-engine-in")
            .attr("type", "text")
            .attr("hidden", "")
            .on_input(move |text| {
                if let Some(event) = Event::parse(text) {
                    heard(&hear, event);
                }
            })
            .into_view(),
    ])
}

/// The player over `requests`, the way to ask the host.
#[must_use]
pub fn player(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Player {
    let inbox: Inbox = Rc::default();
    let ears: Ears = Rc::default();
    let backs: Backs = Rc::default();
    let built = (Rc::clone(&inbox), Rc::clone(&ears), Rc::clone(&backs));
    // Built when mounted, inside the page's reactive scope.
    let view = View::lazy(move || shell(&requests, &setup, &built.0, &built.1, &built.2));
    let deliver: Rc<dyn Fn(Msg)> = Rc::new({
        let inbox = Rc::clone(&inbox);
        move |msg| {
            let receive = inbox.borrow().clone();
            if let Some(receive) = receive {
                receive(msg);
            }
        }
    });
    let hear: Rc<dyn Fn(Event)> = Rc::new(move |event| {
        let listen = ears.borrow().clone();
        if let Some(listen) = listen {
            listen(event);
        }
    });
    let back: Rc<dyn Fn() -> bool> = Rc::new(move || {
        let go = backs.borrow().clone();
        go.is_some_and(|go| go())
    });
    Player {
        view,
        deliver,
        hear,
        back,
    }
}

fn shell(
    requests: &Rc<dyn Fn(Request)>,
    setup: &Setup,
    inbox: &Inbox,
    ears: &Ears,
    backs: &Backs,
) -> View {
    let env = Env {
        m: Model::new(),
        ask: Rc::clone(requests),
        base: setup.base.clone(),
        mobile: setup.mobile,
        login_url: setup.login_url.clone(),
        link: setup.engine.clone(),
        sign_out: setup.sign_out.clone(),
    };
    {
        let env = env.clone();
        *ears.borrow_mut() = Some(Rc::new(move |e: Event| heard(&env, e)));
    }
    {
        let env = env.clone();
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| receive(&env, m)));
    }
    let router = Router::new(Rc::clone(&setup.history));
    router.provide();
    {
        let (env, history) = (env.clone(), Rc::clone(&setup.history));
        *backs.borrow_mut() = Some(Rc::new(move || {
            if env.m.dialog.get_untracked().is_some() {
                env.m.dialog.set(None);
                return true;
            }
            history.back()
        }));
    }

    (env.ask)(Request::Overview);
    (env.ask)(Request::Playlists);

    let page = |screen: fn(&Env, &quench_ui::router::RouteContext) -> View| {
        let env = env.clone();
        move |ctx: &quench_ui::router::RouteContext| -> View { screen(&env, ctx) }
    };
    let ui = format!("{}/ui", setup.base);
    let table = Routes::new()
        .route(&format!("{ui}/home"), page(screens::home))
        .route(&format!("{ui}/search"), page(screens::search))
        .route(&format!("{ui}/artists"), page(screens::artists))
        .route(&format!("{ui}/artist/:name"), page(screens::artist))
        .route(&format!("{ui}/album/:id"), page(screens::album))
        .route(&format!("{ui}/category/:name"), page(screens::category))
        .route(&format!("{ui}/playlists"), page(screens::playlists))
        .route(&format!("{ui}/playlist/:id"), page(screens::playlist))
        .route(&format!("{ui}/saved"), page(screens::saved))
        .not_found({
            let env = env.clone();
            move |_| screens::not_found(&env)
        });

    div()
        .attr("id", "player")
        .class("pl-app")
        .child(screens::side(&env, &router))
        .child(
            main()
                .class("pl-main")
                .child(screens::banner(&env))
                .child(table.view(&router)),
        )
        .child(screens::bar(&env))
        .child(screens::tabs(&env, &router))
        .child(screens::dialogs(&env))
        .child(screens::toast(&env))
        .child(if env.link.is_some() {
            fragment([])
        } else {
            bridge(&env)
        })
        .into_view()
}
