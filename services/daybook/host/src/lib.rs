//! Daybook's brain on a device: loads the person's tasks, makes their changes at once on screen and undoes them if
//! the service refuses, schedules the reminders, and tells the app. It touches the device only through [`Platform`], so
//! it runs the same under a test.
//!
//! One thread owns it (the program's main loop); the slow part (a call to the services) happens elsewhere and comes
//! back as a callback.

pub mod ops;
#[cfg(feature = "native")]
pub mod remote;
#[cfg(feature = "native")]
pub mod worker;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use daybook_core::calendar::format_due;
use daybook_core::{Data, ListRef, Msg, Request, Task, pending_reminders};
use daybook_net::{NetError, Settings, Store};
use quench_core::time::Time;
use quench_tz::Zone;

use daybook_core::Reminder;
pub use ops::{Op, Reply};
#[cfg(feature = "native")]
pub use remote::Remote;

type Then<T> = Box<dyn FnOnce(T)>;

/// How long after the last change the reminders are scheduled again.
pub const REMINDER_DELAY_MS: u64 = 2000;

/// The device's side of the app.
pub trait Platform {
    /// Does one call to the services and gives the answer back on the program's thread.
    fn call(&self, op: Op, then: Then<Result<Reply, NetError>>);
    /// Makes the scheduled notifications exactly `reminders` (everything else is cancelled).
    fn replace_reminders(&self, reminders: &[Reminder]);
    /// Asks the person to allow notifications if that is needed; tells whether they are allowed.
    fn ask_notifications(&self, then: Then<bool>) {
        then(true);
    }
    /// The tasks changed: a host with a widget redraws it.
    fn data_changed(&self, _data: &Data, _now: Time, _zone: &Zone) {}
    /// The person signed out: a host with a widget empties it.
    fn signed_out(&self) {}
    fn now(&self) -> Time;
    fn zone(&self) -> Zone;
    /// Runs `f` after `ms` milliseconds.
    fn later(&self, ms: u64, f: Box<dyn FnOnce()>);
}

struct State {
    data: Data,
    settings: Settings,
    /// Counts the changes, so that of the reminder updates set for later only the last one runs.
    generation: u64,
    asked_notifications: bool,
}

/// The controller.
pub struct Controller {
    platform: Rc<dyn Platform>,
    store: Store,
    emit: Rc<dyn Fn(Msg)>,
    state: RefCell<State>,
    loading: Cell<u32>,
}

impl Controller {
    #[must_use]
    pub fn new(platform: Rc<dyn Platform>, store: Store, emit: Rc<dyn Fn(Msg)>) -> Rc<Self> {
        let settings = store.settings();
        Rc::new(Self {
            platform,
            store,
            emit,
            state: RefCell::new(State {
                data: Data::default(),
                settings,
                generation: 0,
                asked_notifications: false,
            }),
            loading: Cell::new(0),
        })
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.state.borrow().settings.clone()
    }

    /// Reads everything. Call it when the program starts with a stored session.
    pub fn start(self: &Rc<Self>) {
        self.load();
    }

    fn busy(&self, on: bool) {
        let n = if on {
            self.loading.get() + 1
        } else {
            self.loading.get().saturating_sub(1)
        };
        self.loading.set(n);
        if (on && n == 1) || (!on && n == 0) {
            (self.emit)(Msg::Loading(on));
        }
    }

    fn publish(&self) {
        let data = self.state.borrow().data.clone();
        (self.emit)(Msg::Data(Box::new(data)));
    }

    fn notice(&self, text: impl Into<String>) {
        (self.emit)(Msg::Notice(text.into()));
    }

    /// An answer that says the session is over ends it; anything else is told to the person.
    fn failed(&self, what: &str, error: &NetError) {
        if *error == NetError::Unauthorized {
            self.sign_out();
        } else {
            self.notice(format!("{what}: {error}"));
        }
    }

    fn sign_out(&self) {
        self.store.clear_token();
        self.platform.call(Op::Logout, Box::new(|_| {}));
        {
            let mut s = self.state.borrow_mut();
            s.data = Data::default();
            s.generation += 1;
        }
        self.platform.replace_reminders(&[]);
        self.platform.signed_out();
        (self.emit)(Msg::SignedOut);
    }

    fn load(self: &Rc<Self>) {
        self.busy(true);
        let me = Rc::clone(self);
        self.platform.call(
            Op::Load,
            Box::new(move |result| {
                me.busy(false);
                match result {
                    Ok(Reply::Loaded(data)) => {
                        me.state.borrow_mut().data = *data;
                        me.publish();
                        me.reschedule_now();
                    }
                    Ok(_) => {}
                    Err(e) => me.failed("Could not load", &e),
                }
            }),
        );
    }

    /// Handles what the app asks.
    pub fn request(self: &Rc<Self>, request: Request) {
        match request {
            Request::Login {
                server,
                username,
                password,
            } => self.login(server, username, password),
            Request::Reload => self.load(),
            Request::Open(list) => self.remember(|s| s.list = list),
            Request::Save { id, draft } => self.save(id, &draft),
            Request::SetCompleted { id, done } => self.set_completed(&id, done),
            Request::Delete { id } => self.delete(&id),
            Request::CreateProject { name } => {
                let sort_order =
                    i64::try_from(self.state.borrow().data.projects.len()).unwrap_or(0);
                self.simple(
                    Op::CreateProject { name, sort_order },
                    "Could not add the list",
                );
            }
            Request::RenameProject { id, name } => {
                let sort_order = self
                    .state
                    .borrow()
                    .data
                    .projects
                    .iter()
                    .find(|p| p.id == id)
                    .map_or(0, |p| p.sort_order);
                self.simple(
                    Op::RenameProject {
                        id,
                        name,
                        sort_order,
                    },
                    "Could not rename the list",
                );
            }
            Request::DeleteProject { id } => self.delete_project(id),
            Request::CreateLabel { name, color } => {
                self.simple(Op::CreateLabel { name, color }, "Could not add the label")
            }
            Request::UpdateLabel { id, name, color } => self.simple(
                Op::UpdateLabel { id, name, color },
                "Could not change the label",
            ),
            Request::DeleteLabel { id } => self.delete_label(id),
            Request::AskNotifications => self.ask_notifications(),
            Request::Logout => self.sign_out(),
        }
    }

    fn remember(&self, change: impl FnOnce(&mut Settings)) {
        let mut s = self.state.borrow_mut();
        change(&mut s.settings);
        self.store.set_settings(&s.settings);
    }

    fn login(self: &Rc<Self>, server: String, username: String, password: String) {
        let server = server.trim().trim_end_matches('/').to_string();
        let me = Rc::clone(self);
        let (kept_server, kept_user) = (server.clone(), username.clone());
        self.platform.call(
            Op::Login {
                server,
                username,
                password,
            },
            Box::new(move |result| match result {
                Ok(_) => {
                    me.remember(|s| {
                        s.server = kept_server;
                        s.user = kept_user;
                    });
                    (me.emit)(Msg::LoggedIn);
                    me.load();
                }
                Err(e) => (me.emit)(Msg::LoginFailed(e.to_string())),
            }),
        );
    }

    fn save(self: &Rc<Self>, id: Option<String>, draft: &daybook_core::Draft) {
        let zone = self.platform.zone();
        let existing = id
            .as_deref()
            .and_then(|i| self.state.borrow().data.task(i).cloned());
        let body = draft.body(
            &zone,
            existing.as_ref().map(|t| t.is_completed),
            existing.as_ref().map_or(0, |t| t.sort_order),
        );
        let op = match &id {
            Some(id) => Op::UpdateTask(id.clone(), body),
            None => Op::CreateTask(body),
        };
        let me = Rc::clone(self);
        self.platform.call(
            op,
            Box::new(move |result| match result {
                Ok(Reply::Task(task)) => {
                    me.state.borrow_mut().data.put_task(*task);
                    me.publish();
                    me.changed();
                    (me.emit)(Msg::Saved(Ok(())));
                }
                Ok(_) => {}
                Err(NetError::Unauthorized) => me.sign_out(),
                Err(e) => {
                    if e == NetError::NotFound {
                        me.load();
                    }
                    (me.emit)(Msg::Saved(Err(e.to_string())));
                }
            }),
        );
    }

    fn set_completed(self: &Rc<Self>, id: &str, done: bool) {
        let Some(before) = self.state.borrow().data.task(id).cloned() else {
            return;
        };
        let mut sent = before.clone();
        sent.is_completed = done;
        let body = sent.to_update();
        let me = Rc::clone(self);
        // A repeating task is not closed by completing it: the service moves it to its next date, so wait for that.
        let repeats = done && before.recurrence.is_some();
        if !repeats {
            self.state
                .borrow_mut()
                .data
                .mark(id, done, self.platform.now());
            self.publish();
        }
        self.platform.call(
            Op::UpdateTask(id.to_string(), body),
            Box::new(move |result| match result {
                Ok(Reply::Task(task)) => {
                    let message = (repeats && !task.is_completed).then(|| {
                        let zone = me.platform.zone();
                        task.due_at.map_or_else(
                            || "Done".to_string(),
                            |due| {
                                format!(
                                    "Done — next: {}",
                                    format_due(due, task.due_all_day, me.platform.now(), &zone)
                                )
                            },
                        )
                    });
                    me.state.borrow_mut().data.put_task(*task);
                    me.publish();
                    me.changed();
                    if let Some(m) = message {
                        me.notice(m);
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    me.state.borrow_mut().data.put_task(before);
                    me.publish();
                    me.failed("Could not update the task", &e);
                }
            }),
        );
    }

    fn delete(self: &Rc<Self>, id: &str) {
        let Some(gone) = self.state.borrow_mut().data.take_task(id) else {
            return;
        };
        self.publish();
        self.changed();
        let me = Rc::clone(self);
        self.platform.call(
            Op::DeleteTask(id.to_string()),
            Box::new(move |result| {
                if let Err(e) = result {
                    // Already gone on the service is as good as deleted.
                    if e != NetError::NotFound {
                        me.state.borrow_mut().data.put_task(gone);
                        me.publish();
                        me.changed();
                        me.failed("Could not delete the task", &e);
                    }
                }
            }),
        );
    }

    /// A change to a list or a label: the answer is put in the data.
    fn simple(self: &Rc<Self>, op: Op, what: &'static str) {
        let me = Rc::clone(self);
        self.platform.call(
            op,
            Box::new(move |result| match result {
                Ok(Reply::Project(p)) => {
                    me.state.borrow_mut().data.put_project(p);
                    me.publish();
                }
                Ok(Reply::Label(l)) => {
                    me.state.borrow_mut().data.put_label(l.clone());
                    me.publish();
                    (me.emit)(Msg::LabelCreated(Box::new(l)));
                }
                Ok(_) => {}
                Err(e) => me.failed(what, &e),
            }),
        );
    }

    fn delete_project(self: &Rc<Self>, id: String) {
        let me = Rc::clone(self);
        let op = Op::DeleteProject(id.clone());
        self.platform.call(
            op,
            Box::new(move |result| match result {
                Ok(_) | Err(NetError::NotFound) => {
                    me.state.borrow_mut().data.remove_project(&id);
                    if me.state.borrow().settings.list == ListRef::Project(id) {
                        me.remember(|s| s.list = ListRef::Inbox);
                    }
                    me.publish();
                    me.changed();
                }
                Err(e) => me.failed("Could not delete the list", &e),
            }),
        );
    }

    fn delete_label(self: &Rc<Self>, id: String) {
        let me = Rc::clone(self);
        let op = Op::DeleteLabel(id.clone());
        self.platform.call(
            op,
            Box::new(move |result| match result {
                Ok(_) | Err(NetError::NotFound) => {
                    me.state.borrow_mut().data.remove_label(&id);
                    if me.state.borrow().settings.list == ListRef::Label(id) {
                        me.remember(|s| s.list = ListRef::Inbox);
                    }
                    me.publish();
                    me.changed();
                }
                Err(e) => me.failed("Could not delete the label", &e),
            }),
        );
    }

    fn ask_notifications(self: &Rc<Self>) {
        let me = Rc::clone(self);
        self.platform.ask_notifications(Box::new(move |allowed| {
            (me.emit)(Msg::NotificationsBlocked(!allowed))
        }));
    }

    /// The tasks changed: the widget and the reminders once things have settled. Neither is done at once: both touch the
    /// device (a file, calls to the system) and would hold up the screen at the moment the person tapped.
    fn changed(self: &Rc<Self>) {
        let generation = {
            let mut s = self.state.borrow_mut();
            s.generation += 1;
            s.generation
        };
        let me = Rc::clone(self);
        self.platform.later(
            REMINDER_DELAY_MS,
            Box::new(move || {
                if me.state.borrow().generation == generation {
                    me.reschedule_now();
                }
            }),
        );
    }

    /// Schedules the reminders of the tasks as they are.
    fn reschedule_now(self: &Rc<Self>) {
        let (now, zone) = (self.platform.now(), self.platform.zone());
        let (reminders, data) = {
            let s = self.state.borrow();
            (pending_reminders(&s.data.tasks, now, &zone), s.data.clone())
        };
        self.platform.data_changed(&data, now, &zone);
        if !reminders.is_empty() && !self.state.borrow().asked_notifications {
            self.state.borrow_mut().asked_notifications = true;
            self.ask_notifications();
        }
        self.platform.replace_reminders(&reminders);
    }

    /// The task as the controller has it (for a host that opens one from a notification).
    #[must_use]
    pub fn task(&self, id: &str) -> Option<Task> {
        self.state.borrow().data.task(id).cloned()
    }
}
