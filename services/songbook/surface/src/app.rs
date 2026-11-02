//! The app: the player's screens, or the sign-in when nobody is signed in.
//!
//! The calls to the service run on a few threads, the pictures come through the renderer's loader, and the playback commands
//! go to the host's `player` plugin.
//!
//! One thread owns the views (the program's main loop); everything slow happens on the pool and comes back as an [`Answer`].

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use quench_android_surface::Bridge;
use quench_serde::{Value, json};
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui_native::NativeRenderer;
use songbook_native::store::Store;
use songbook_native::{Auth, Failure};
use songbook_player::protocol::{call_for, interpret, unreachable};
use songbook_player::{Event, Msg, Player, Request, Setup, player};

use crate::pictures::{self, Wanted};
use crate::pool::Pool;
use crate::signin;

/// The service's path prefix.
pub const BASE: &str = "/songbook";

/// What the pool brings back.
enum Answer {
    Msg(Msg),
    Picture(String, Option<Vec<u8>>),
    SignedIn(Result<(), String>),
}

type Slot<T> = Rc<RefCell<Option<Rc<dyn Fn(T)>>>>;
/// What Back does, once the player exists.
type BackSlot = Rc<RefCell<Option<Back>>>;

fn say<T: 'static>(slot: &Slot<T>, value: T) {
    let f = slot.borrow().clone();
    if let Some(f) = f {
        f(value);
    }
}

/// The whole app as a view.
pub struct App {
    pub view: View,
    back: BackSlot,
    scope: Scope,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("App { .. }")
    }
}

/// What Back does: the app's answer to the phone's button, shared so that the program can hold it apart from the view.
pub type Back = Rc<dyn Fn() -> bool>;

impl App {
    /// What Back does: closes a box, goes back a page; false when there is nothing to go back to (the program then leaves).
    #[must_use]
    pub fn back(&self) -> Back {
        let slot = Rc::clone(&self.back);
        Rc::new(move || {
            let go = slot.borrow().clone();
            go.is_some_and(|go| go())
        })
    }

    /// Keeps the app's reactive state alive as long as the caller holds this.
    #[must_use]
    pub const fn scope(&self) -> &Scope {
        &self.scope
    }
}

/// Builds the app over `auth`. Pictures are fetched through the renderer's image loader.
#[must_use]
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)] // one function wires the whole app; the auth is shared by the closures
pub fn build(
    renderer: &Rc<NativeRenderer>,
    bridge: &Bridge,
    auth: Arc<Auth>,
    store: &Store,
) -> App {
    let pool = Pool::new(4);
    let pictures_dir = store.dir().join("art");
    pictures::trim(&pictures_dir, pictures::MOST_BYTES);

    let ((signed, error, busy), scope) = create_root(|| {
        (
            create_signal(auth.signed_in()),
            create_signal(String::new()),
            create_signal(false),
        )
    });
    let deliver: Slot<Msg> = Rc::default();
    let hear: Slot<Event> = Rc::default();
    let wanted = Rc::new(RefCell::new(Wanted::default()));

    let answers = {
        let (to_renderer, deliver, signed, error, busy, wanted, dir) = (
            Rc::clone(renderer),
            Rc::clone(&deliver),
            signed.clone(),
            error.clone(),
            busy.clone(),
            Rc::clone(&wanted),
            pictures_dir.clone(),
        );
        renderer.channel(move |answer: Answer| match answer {
            Answer::Msg(msg) => {
                if msg == Msg::SignedOut {
                    signed.set(false);
                }
                say(&deliver, msg);
            }
            Answer::Picture(src, bytes) => {
                wanted.borrow_mut().done(&src);
                if let Some(bytes) = bytes {
                    pictures::keep(&dir, &src, &bytes);
                    let _ = to_renderer.add_image(&src, &bytes);
                }
            }
            Answer::SignedIn(result) => {
                busy.set(false);
                match result {
                    Ok(()) => {
                        error.set(String::new());
                        signed.set(true);
                    }
                    Err(why) => error.set(why),
                }
            }
        })
    };

    // The pictures: from the folder at once, else fetched and drawn when they come.
    {
        let (auth, pool, tx, dir, wanted) = (
            Arc::clone(&auth),
            pool.clone(),
            answers.clone(),
            pictures_dir,
            Rc::clone(&wanted),
        );
        renderer.set_image_loader(move |src| {
            if let Ok(bytes) = std::fs::read(pictures::file_for(&dir, src)) {
                return Some(bytes);
            }
            if wanted.borrow_mut().first(src) {
                let (auth, tx, src) = (Arc::clone(&auth), tx.clone(), src.to_string());
                pool.run(move || {
                    let bytes = auth.fetch(&src).ok();
                    let _ = tx.send(Answer::Picture(src, bytes));
                });
            }
            None
        });
    }

    // The calls to the service.
    let requests: Rc<dyn Fn(Request)> = {
        let (auth, pool, tx) = (Arc::clone(&auth), pool.clone(), answers.clone());
        Rc::new(move |request: Request| {
            let call = call_for(&request);
            let (auth, tx) = (Arc::clone(&auth), tx.clone());
            pool.run(move || {
                let url = format!("{}{BASE}{}", auth.settings().server, call.path);
                let msg = match auth.exchange(call.method, &url, call.body.as_deref()) {
                    Ok((status, body)) => {
                        interpret(&request, status, &String::from_utf8_lossy(&body))
                    }
                    Err(Failure::SignedOut) => Some(Msg::SignedOut),
                    Err(other) => Some(unreachable(&other.message())),
                };
                if let Some(msg) = msg {
                    let _ = tx.send(Answer::Msg(msg));
                }
            });
        })
    };

    // The sound is the host's: commands go out through the bridge, its events come back.
    {
        let hear = Rc::clone(&hear);
        bridge.on("player", "event", move |data: &Value| {
            if let Some(event) = Event::parse(&quench_serde::json::to_string(data)) {
                say(&hear, event);
            }
        });
    }
    let engine: Rc<dyn Fn(String)> = {
        let bridge = bridge.clone();
        Rc::new(move |text: String| {
            bridge.call("player", "send", json!({ "command": text }), |_| {});
        })
    };

    let history = Rc::new(MemoryHistory::new(&format!("{BASE}/ui")));
    let back: BackSlot = Rc::default();
    let sign_in = {
        let (auth, pool, tx, busy) = (Arc::clone(&auth), pool, answers, busy.clone());
        Rc::new(move |server: String, user: String, password: String| {
            busy.set(true);
            let (auth, tx) = (Arc::clone(&auth), tx.clone());
            pool.run(move || {
                let result = auth
                    .login(&server, &user, &password)
                    .map_err(|e| e.message());
                let _ = tx.send(Answer::SignedIn(result));
            });
        })
    };

    let view = {
        let (history, bridge, auth, back) = (
            Rc::clone(&history),
            bridge.clone(),
            Arc::clone(&auth),
            Rc::clone(&back),
        );
        let (signed, error, busy) = (signed, error, busy);
        View::dynamic(move || {
            if !signed.get() {
                let server = auth.settings().server;
                let server = if server.is_empty() {
                    "https://".to_string()
                } else {
                    server
                };
                let go = Rc::clone(&sign_in);
                return signin::form(
                    &server,
                    error.clone(),
                    &busy,
                    Rc::new(move |s, u, p| go(s, u, p)),
                );
            }
            let h: Rc<dyn History> = history.clone();
            let Player {
                view,
                deliver: to_player,
                hear: to_ears,
                back: to_back,
            } = player(
                Rc::clone(&requests),
                Setup {
                    base: BASE.to_string(),
                    history: h,
                    login_url: String::new(),
                    mobile: true,
                    engine: Some(Rc::clone(&engine)),
                },
            );
            *deliver.borrow_mut() = Some(to_player);
            *hear.borrow_mut() = Some(to_ears);
            *back.borrow_mut() = Some(to_back);
            // The screens just came up: say everything again.
            bridge.call("player", "ask", json!({}), |_| {});
            view
        })
    };
    App { view, back, scope }
}
