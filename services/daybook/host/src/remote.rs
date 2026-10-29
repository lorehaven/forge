//! The talk to the services, as plain operations: what the controller asks for, and what comes back. [`Remote`] does
//! them with a real session; a test answers them itself.

use std::sync::{Arc, Mutex, PoisonError};

use daybook_core::Data;
use daybook_net::{NetError, Session, Store, login};

pub use crate::ops::{Op, Reply};

/// The session, if there is one.
pub type Shared = Arc<Mutex<Option<Session>>>;

/// A session that keeps its refresh token in `store` and follows what another program of the app keeps there (the
/// widget's refresh).
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

/// Does the operations against the real services.
pub struct Remote {
    session: Shared,
    store: Store,
}

impl Remote {
    /// With the session an earlier run left, if there is one.
    #[must_use]
    pub fn new(store: Store) -> Arc<Self> {
        let session = stored_session(&store, &store.settings().server);
        Arc::new(Self {
            session: Arc::new(Mutex::new(session)),
            store,
        })
    }

    /// Whether anyone is signed in.
    #[must_use]
    pub fn signed_in(&self) -> bool {
        self.session
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Does one operation.
    ///
    /// # Errors
    /// [`NetError`]; [`NetError::Unauthorized`] also when nobody is signed in.
    pub fn execute(&self, op: Op) -> Result<Reply, NetError> {
        let mut guard = self.session.lock().unwrap_or_else(PoisonError::into_inner);
        match op {
            Op::Login {
                server,
                username,
                password,
            } => {
                let session = login(&server, &username, &password)?;
                self.store.set_token(session.refresh_token());
                let (keep, follow) = (self.store.clone(), self.store.clone());
                *guard = Some(
                    session
                        .keeping_tokens(move |t| keep.set_token(t))
                        .following(move || follow.token()),
                );
                Ok(Reply::Done)
            }
            Op::Logout => {
                *guard = None;
                self.store.clear_token();
                Ok(Reply::Done)
            }
            op => run(guard.as_mut().ok_or(NetError::Unauthorized)?, op),
        }
    }
}

fn run(s: &mut Session, op: Op) -> Result<Reply, NetError> {
    Ok(match op {
        Op::Load => {
            let projects = s.projects()?;
            let labels = s.labels()?;
            let tasks = s.open_tasks()?;
            Reply::Loaded(Box::new(Data {
                projects,
                labels,
                tasks,
            }))
        }
        Op::CreateTask(body) => Reply::Task(Box::new(s.create_task(&body)?)),
        Op::UpdateTask(id, body) => Reply::Task(Box::new(s.update_task(&id, &body)?)),
        Op::DeleteTask(id) => {
            s.delete_task(&id)?;
            Reply::Done
        }
        Op::CreateProject { name, sort_order } => {
            Reply::Project(s.create_project(&name, sort_order)?)
        }
        Op::RenameProject {
            id,
            name,
            sort_order,
        } => Reply::Project(s.rename_project(&id, &name, sort_order)?),
        Op::DeleteProject(id) => {
            s.delete_project(&id)?;
            Reply::Done
        }
        Op::CreateLabel { name, color } => Reply::Label(s.create_label(&name, &color)?),
        Op::UpdateLabel { id, name, color } => Reply::Label(s.update_label(&id, &name, &color)?),
        Op::DeleteLabel(id) => {
            s.delete_label(&id)?;
            Reply::Done
        }
        Op::Login { .. } | Op::Logout => Reply::Done,
    })
}
