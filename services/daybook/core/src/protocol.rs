//! What the app asks its host to do and what the host tells it. Plain data both ways: the app does no I/O, the host
//! decides how and where the work is done.

use crate::data::Data;
use crate::draft::Draft;
use crate::model::{Label, ListRef};

/// What the app asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Login {
        server: String,
        username: String,
        password: String,
    },
    /// Read everything again (pull to refresh).
    Reload,
    /// The view the person opened (kept for the next start).
    Open(ListRef),
    /// Make a task (`id` is `None`) or replace one.
    Save {
        id: Option<String>,
        draft: Box<Draft>,
    },
    /// Check a task off or reopen it. A repeating task comes back open at its next date.
    SetCompleted {
        id: String,
        done: bool,
    },
    Delete {
        id: String,
    },
    CreateProject {
        name: String,
    },
    RenameProject {
        id: String,
        name: String,
    },
    /// Deletes the list and its tasks.
    DeleteProject {
        id: String,
    },
    CreateLabel {
        name: String,
        color: String,
    },
    UpdateLabel {
        id: String,
        name: String,
        color: String,
    },
    DeleteLabel {
        id: String,
    },
    /// The person agreed to be notified (or the app wants to know if they have).
    AskNotifications,
    Logout,
}

/// What the host tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    LoggedIn,
    LoginFailed(String),
    /// Everything, after a load or a change (optimistic ones included).
    Data(Box<Data>),
    /// A save has finished: the form may close, or stay with the reason.
    Saved(Result<(), String>),
    /// A label that was just made, so a form can tick it.
    LabelCreated(Box<Label>),
    /// Something to tell the person.
    Notice(String),
    /// Reading from the service is going on (or has ended).
    Loading(bool),
    /// Reminders cannot be shown until the person allows notifications.
    NotificationsBlocked(bool),
    /// Something outside the app asks for a view (the widget's **+**, a tapped reminder): open `list`, and with `add` the form
    /// for a new task in it, or with `task` the form of that task.
    Show {
        list: ListRef,
        add: bool,
        task: Option<String>,
    },
    /// The session ended: show the sign-in again.
    SignedOut,
}
