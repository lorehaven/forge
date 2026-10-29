//! The session for a host that cannot block: the session lives behind a mutex, a worker thread does the talking to the
//! servers, and the answer comes back as a message for the program's own thread to give to the waiting callback.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use forklift_core::Remote;
use forklift_net::{Change, NetError, Session, Store};

/// The session, if there is one.
pub type Shared = Arc<Mutex<Option<Session>>>;

/// What the worker thread sends back: the request's number and its answer, to be given to [`Worker::complete`].
pub type Answer = Box<dyn Any + Send>;

type Callback = Box<dyn FnOnce(Answer)>;
type Reply = Arc<dyn Fn(u64, Answer) + Send + Sync>;

/// A session that keeps its refresh token in `store` and follows what another program of the app keeps there.
#[must_use]
pub fn session_for(store: &Store, server: &str, token: &str) -> Session {
    let (keep, follow) = (store.clone(), store.clone());
    Session::resume(server, token)
        .keeping_tokens(move |t| keep.set_token(t))
        .following(move || follow.token())
}

/// The session stored by an earlier run, if the server and a token are known.
#[must_use]
pub fn stored_session(store: &Store, server: &str) -> Option<Session> {
    if server.is_empty() {
        return None;
    }
    store.token().map(|t| session_for(store, server, &t))
}

/// Does the session's work on another thread.
pub struct Worker {
    session: Shared,
    answers: Reply,
    waiting: RefCell<HashMap<u64, Callback>>,
    next: Cell<u64>,
}

impl Worker {
    #[must_use]
    pub fn new(session: Shared, answers: impl Fn(u64, Answer) + Send + Sync + 'static) -> Rc<Self> {
        Rc::new(Self {
            session,
            answers: Arc::new(answers),
            waiting: RefCell::default(),
            next: Cell::new(0),
        })
    }

    /// Runs `work` with the session (`None` when nobody is signed in) on the worker thread and gives its result to
    /// `then` on the program's thread, once [`complete`](Self::complete) is called with the answer.
    pub fn run<T: Send + 'static>(
        &self,
        work: impl FnOnce(Option<&mut Session>) -> T + Send + 'static,
        then: impl FnOnce(T) + 'static,
    ) {
        self.next.set(self.next.get() + 1);
        let id = self.next.get();
        self.waiting.borrow_mut().insert(
            id,
            Box::new(move |answer: Answer| {
                if let Ok(result) = answer.downcast::<T>() {
                    then(*result);
                }
            }),
        );
        let (session, answers) = (Arc::clone(&self.session), Arc::clone(&self.answers));
        std::thread::spawn(move || {
            let result = work(session.lock().unwrap().as_mut());
            answers(id, Box::new(result));
        });
    }

    /// Runs `work` on another thread without the session (a dialog the person takes their time over must not hold the
    /// session up) and gives its result to `then` on the program's thread, as [`run`](Self::run) does.
    pub fn spawn<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(T) + 'static,
    ) {
        self.next.set(self.next.get() + 1);
        let id = self.next.get();
        self.waiting.borrow_mut().insert(
            id,
            Box::new(move |answer: Answer| {
                if let Ok(result) = answer.downcast::<T>() {
                    then(*result);
                }
            }),
        );
        let answers = Arc::clone(&self.answers);
        std::thread::spawn(move || answers(id, Box::new(work())));
    }

    /// An access token (`force`: a new one whatever the old one says).
    pub fn token(&self, force: bool, then: impl FnOnce(Result<String, NetError>) + 'static) {
        self.run(
            move |session| match session {
                None => Err(NetError::Unauthorized),
                Some(s) => {
                    (if force { s.refresh() } else { Ok(()) }).and_then(|()| s.fresh_token())
                }
            },
            then,
        );
    }

    /// What changed in `storage` since change number `since`.
    pub fn changes(
        &self,
        storage: &str,
        since: i64,
        then: impl FnOnce(Result<Vec<Change>, NetError>) + 'static,
    ) {
        let storage = storage.to_string();
        self.run(
            move |session| match session {
                None => Err(NetError::Unauthorized),
                Some(s) => s.changes(&storage, since),
            },
            then,
        );
    }

    /// Every file stored in `storage`.
    pub fn listing(
        &self,
        storage: &str,
        then: impl FnOnce(Result<Vec<Remote>, NetError>) + 'static,
    ) {
        let storage = storage.to_string();
        self.run(
            move |session| match session {
                None => Err(NetError::Unauthorized),
                Some(s) => s.list_all(&storage, "").map(|all| {
                    all.into_iter()
                        .map(|e| Remote {
                            path: e.path,
                            size: e.size.unwrap_or(0),
                        })
                        .collect()
                }),
            },
            then,
        );
    }

    /// `(used, quota)` of `storage`, when the server says (any failure is no answer).
    pub fn quota(&self, storage: &str, then: impl FnOnce(Option<(u64, u64)>) + 'static) {
        let storage = storage.to_string();
        self.run(
            move |session| {
                let found = session?
                    .storages()
                    .ok()?
                    .into_iter()
                    .find(|s| s.name == storage)?;
                Some((found.used_bytes?, found.quota_bytes?))
            },
            then,
        );
    }

    /// The storages this account may use.
    pub fn storages(
        &self,
        then: impl FnOnce(Result<Vec<forklift_net::Storage>, NetError>) + 'static,
    ) {
        self.run(
            move |session| match session {
                None => Err(NetError::Unauthorized),
                Some(s) => s.storages(),
            },
            then,
        );
    }

    /// The worker's answer to request `id`.
    pub fn complete(&self, id: u64, answer: Answer) {
        let then = self.waiting.borrow_mut().remove(&id);
        if let Some(then) = then {
            then(answer);
        }
    }
}
