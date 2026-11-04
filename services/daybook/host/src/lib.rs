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
use quench_serde::{Value, json};
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

/// A change made while the services could not be reached (or while earlier ones were still waiting), which goes to them
/// as soon as they answer. `local` is the id the screen gave what a create made, until the service names it.
struct Queued {
    op: Op,
    local: Option<String>,
}

struct State {
    /// The changes that wait for the services, oldest first. Kept on the device, so a restart does not lose them.
    queue: Vec<Queued>,
    flushing: bool,
    data: Data,
    settings: Settings,
    /// Counts the changes, so that of the reminder updates set for later only the last one runs.
    generation: u64,
    asked_notifications: bool,
}

fn read_queue(text: &str) -> Vec<Queued> {
    text.lines()
        .filter_map(|line| {
            let v: Value = json::from_str(line).ok()?;
            Some(Queued {
                op: Op::from_value(&v)?,
                local: v.get("local").and_then(Value::as_str).map(str::to_string),
            })
        })
        .collect()
}

/// Whether the services could not be reached (no connection, or a proxy answering for a service that is down or restarting: 502-504), as opposed to
/// answering with a refusal.
#[must_use]
pub fn is_unreachable(error: &NetError) -> bool {
    matches!(error, NetError::Network(_) | NetError::Status(502..=504))
}

/// The controller.
pub struct Controller {
    platform: Rc<dyn Platform>,
    store: Store,
    emit: Rc<dyn Fn(Msg)>,
    state: RefCell<State>,
    loading: Cell<u32>,
    /// The services did not answer, and the next try is waiting.
    offline: Cell<bool>,
    retrying: Cell<bool>,
    attempts: Cell<u32>,
}

impl Controller {
    #[must_use]
    pub fn new(platform: Rc<dyn Platform>, store: Store, emit: Rc<dyn Fn(Msg)>) -> Rc<Self> {
        let settings = store.settings();
        let queue = read_queue(&store.outbox());
        Rc::new(Self {
            platform,
            store,
            emit,
            state: RefCell::new(State {
                queue,
                flushing: false,
                data: Data::default(),
                settings,
                generation: 0,
                asked_notifications: false,
            }),
            loading: Cell::new(0),
            offline: Cell::new(false),
            retrying: Cell::new(false),
            attempts: Cell::new(0),
        })
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.state.borrow().settings.clone()
    }

    /// Reads everything. Call it when the program starts with a stored session.
    pub fn start(self: &Rc<Self>) {
        // What was known last time is on screen at once, and can be changed without the services.
        let cached = self
            .store
            .cache()
            .and_then(|t| json::from_str::<Value>(&t).ok());
        if let Some(cached) = cached {
            self.state.borrow_mut().data = Data::from_value(&cached);
            self.publish();
        }
        self.sync();
    }

    /// Sends the changes that wait (then reads everything again), or just reads.
    fn sync(self: &Rc<Self>) {
        if self.state.borrow().queue.is_empty() {
            self.load();
        } else {
            self.flush();
        }
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

    /// An answer that says the session is over ends it; services that do not answer put the app in its waiting state
    /// (the person stays signed in); anything else is told to the person.
    fn failed(self: &Rc<Self>, what: &str, error: &NetError) {
        if *error == NetError::Unauthorized {
            self.sign_out();
        } else if is_unreachable(error) {
            self.unreachable();
        } else {
            self.notice(format!("{what}: {error}"));
        }
    }

    /// Says the services cannot be reached and tries to read again after a wait that grows (3 s up to 24 s).
    fn unreachable(self: &Rc<Self>) {
        if !self.offline.replace(true) {
            (self.emit)(Msg::Offline(true));
        }
        if self.retrying.replace(true) {
            return;
        }
        let n = self.attempts.get();
        self.attempts.set(n.saturating_add(1));
        let me = Rc::clone(self);
        self.platform.later(
            3000 << n.min(3),
            Box::new(move || {
                me.retrying.set(false);
                me.sync();
            }),
        );
    }

    /// The services answered: the waiting state ends.
    fn reachable(&self) {
        self.attempts.set(0);
        if self.offline.replace(false) {
            (self.emit)(Msg::Offline(false));
        }
    }

    fn sign_out(&self) {
        self.store.clear_token();
        self.state.borrow_mut().queue.clear();
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
                        me.reachable();
                        if me.state.borrow().queue.is_empty() {
                            me.state.borrow_mut().data = *data;
                            me.publish();
                            me.reschedule_now();
                        } else {
                            // Changes are still waiting: what they make is on screen, and the reading comes after.
                            me.flush();
                        }
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
            Request::Reload => self.sync(),
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
        if self.must_queue() {
            self.enqueue(op);
            (self.emit)(Msg::Saved(Ok(())));
            return;
        }
        let (me, queued) = (Rc::clone(self), op.clone());
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
                Err(e) if is_unreachable(&e) => {
                    // Kept for when the services answer; the form closes as for any save.
                    me.enqueue_unreachable(queued);
                    (me.emit)(Msg::Saved(Ok(())));
                }
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
        if self.must_queue() {
            self.enqueue(Op::UpdateTask(id.to_string(), body));
            return;
        }
        let me = Rc::clone(self);
        let queued = Op::UpdateTask(id.to_string(), body.clone());
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
                Err(e) if is_unreachable(&e) => me.enqueue_unreachable(queued),
                Err(e) => {
                    me.state.borrow_mut().data.put_task(before);
                    me.publish();
                    me.failed("Could not update the task", &e);
                }
            }),
        );
    }

    fn delete(self: &Rc<Self>, id: &str) {
        if self.must_queue() {
            if self.state.borrow().data.task(id).is_some() {
                self.enqueue(Op::DeleteTask(id.to_string()));
            }
            return;
        }
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
                    if is_unreachable(&e) {
                        me.enqueue_unreachable(Op::DeleteTask(gone.id));
                    } else if e != NetError::NotFound {
                        // (Already gone on the service is as good as deleted.)
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
        if self.must_queue() {
            self.enqueue(op);
            return;
        }
        let (me, queued) = (Rc::clone(self), op.clone());
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
                Err(e) if is_unreachable(&e) => me.enqueue_unreachable(queued),
                Err(e) => me.failed(what, &e),
            }),
        );
    }

    fn delete_project(self: &Rc<Self>, id: String) {
        let op = Op::DeleteProject(id.clone());
        if self.must_queue() {
            self.enqueue(op);
            return;
        }
        let (me, queued) = (Rc::clone(self), op.clone());
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
                Err(e) if is_unreachable(&e) => me.enqueue_unreachable(queued),
                Err(e) => me.failed("Could not delete the list", &e),
            }),
        );
    }

    fn delete_label(self: &Rc<Self>, id: String) {
        let op = Op::DeleteLabel(id.clone());
        if self.must_queue() {
            self.enqueue(op);
            return;
        }
        let (me, queued) = (Rc::clone(self), op.clone());
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
                Err(e) if is_unreachable(&e) => me.enqueue_unreachable(queued),
                Err(e) => me.failed("Could not delete the label", &e),
            }),
        );
    }

    /// Whether a change has to wait its turn: the services cannot be reached, or earlier changes are still waiting
    /// (and they go first, in the order they were made).
    fn must_queue(&self) -> bool {
        self.offline.get() || !self.state.borrow().queue.is_empty()
    }

    /// Keeps a change for when the services answer. It shows on screen at once, as if it had been done.
    fn enqueue(self: &Rc<Self>, op: Op) {
        let local = self.apply_local(&op);
        self.state.borrow_mut().queue.push(Queued { op, local });
        self.save_queue();
        self.publish();
        self.changed();
        if self.offline.get() {
            self.unreachable();
        } else {
            self.flush();
        }
    }

    /// [`enqueue`](Self::enqueue) for a change the services just failed to answer: they are not asked again at once.
    fn enqueue_unreachable(self: &Rc<Self>, op: Op) {
        self.unreachable();
        self.enqueue(op);
    }

    fn save_queue(&self) {
        let lines: String = self
            .state
            .borrow()
            .queue
            .iter()
            .filter_map(|q| {
                let Some(Value::Object(mut m)) = q.op.to_value() else {
                    return None;
                };
                if let Some(local) = &q.local {
                    m.insert("local", Value::String(local.clone()));
                }
                Some(json::to_string(&Value::Object(m)) + "\n")
            })
            .collect();
        self.store.set_outbox(&lines);
    }

    /// Makes the change in the data on screen. A create gets an id of its own until the service names it.
    fn apply_local(&self, op: &Op) -> Option<String> {
        let now = self.platform.now();
        let local = || format!("local-{}", quench_core::uuid::Uuid::new_v4());
        let mut made = None;
        let mut st = self.state.borrow_mut();
        match op {
            Op::CreateTask(body) | Op::UpdateTask(_, body) => {
                let (id, created) = match op {
                    Op::UpdateTask(id, _) => {
                        (id.clone(), st.data.task(id).map_or(now, |t| t.created_at))
                    }
                    _ => {
                        let id = local();
                        made = Some(id.clone());
                        (id, now)
                    }
                };
                if let Value::Object(mut m) = body.clone() {
                    m.insert("id", Value::String(id));
                    m.insert("created_at", Value::String(created.to_rfc3339()));
                    m.insert("updated_at", Value::String(now.to_rfc3339()));
                    if let Some(task) = Task::from_value(&Value::Object(m)) {
                        st.data.put_task(task);
                    }
                }
            }
            Op::DeleteTask(id) => {
                st.data.take_task(id);
            }
            Op::CreateProject { name, sort_order } => {
                let id = local();
                made = Some(id.clone());
                st.data.put_project(daybook_core::Project {
                    id,
                    name: name.trim().to_string(),
                    sort_order: *sort_order,
                });
            }
            Op::RenameProject {
                id,
                name,
                sort_order,
            } => st.data.put_project(daybook_core::Project {
                id: id.clone(),
                name: name.trim().to_string(),
                sort_order: *sort_order,
            }),
            Op::DeleteProject(id) => {
                st.data.remove_project(id);
                if st.settings.list == ListRef::Project(id.clone()) {
                    st.settings.list = ListRef::Inbox;
                    self.store.set_settings(&st.settings);
                }
            }
            Op::CreateLabel { name, color } => {
                let label = daybook_core::Label {
                    id: local(),
                    name: name.trim().to_string(),
                    color: color.clone(),
                };
                made = Some(label.id.clone());
                st.data.put_label(label.clone());
                drop(st);
                (self.emit)(Msg::LabelCreated(Box::new(label)));
                return made;
            }
            Op::UpdateLabel { id, name, color } => st.data.put_label(daybook_core::Label {
                id: id.clone(),
                name: name.trim().to_string(),
                color: color.clone(),
            }),
            Op::DeleteLabel(id) => {
                st.data.remove_label(id);
                if st.settings.list == ListRef::Label(id.clone()) {
                    st.settings.list = ListRef::Inbox;
                    self.store.set_settings(&st.settings);
                }
            }
            Op::Login { .. } | Op::Load | Op::Logout => {}
        }
        made
    }

    /// Sends the oldest waiting change; when it is done, the next. When none is left, everything is read again.
    fn flush(self: &Rc<Self>) {
        let (op, local) = {
            let mut st = self.state.borrow_mut();
            if st.flushing {
                return;
            }
            let Some(first) = st.queue.first() else {
                return;
            };
            let sent = (first.op.clone(), first.local.clone());
            st.flushing = true;
            sent
        };
        let me = Rc::clone(self);
        self.platform.call(
            op,
            Box::new(move |result| {
                me.state.borrow_mut().flushing = false;
                match result {
                    Err(NetError::Unauthorized) => me.sign_out(),
                    Err(e) if is_unreachable(&e) => me.unreachable(),
                    other => {
                        me.sent(local, other);
                        me.reachable();
                        me.state.borrow_mut().queue.remove(0);
                        me.save_queue();
                        if me.state.borrow().queue.is_empty() {
                            me.load();
                        } else {
                            me.flush();
                        }
                    }
                }
            }),
        );
    }

    /// The service answered the oldest waiting change: what it made gets its real id, and a refusal is told.
    fn sent(&self, local: Option<String>, result: Result<Reply, NetError>) {
        let reply = match result {
            Ok(reply) => reply,
            // Gone on the service already: nothing to apply.
            Err(NetError::NotFound) => return,
            Err(e) => {
                self.notice(format!("A change made offline was not accepted: {e}"));
                return;
            }
        };
        let real = match &reply {
            Reply::Task(t) => Some(t.id.clone()),
            Reply::Project(p) => Some(p.id.clone()),
            Reply::Label(l) => Some(l.id.clone()),
            _ => None,
        };
        if let (Some(local), Some(real)) = (&local, &real) {
            let mut st = self.state.borrow_mut();
            st.data.rename_id(local, real);
            for q in st.queue.iter_mut().skip(1) {
                q.op.rename_id(local, real);
                if q.local.as_deref() == Some(local) {
                    q.local = None;
                }
            }
            let renamed = match &st.settings.list {
                ListRef::Project(id) if id == local => Some(ListRef::Project(real.clone())),
                ListRef::Label(id) if id == local => Some(ListRef::Label(real.clone())),
                _ => None,
            };
            if let Some(list) = renamed {
                st.settings.list = list;
                self.store.set_settings(&st.settings);
            }
        }
        let mut st = self.state.borrow_mut();
        match reply {
            Reply::Task(t) => st.data.put_task(*t),
            Reply::Project(p) => st.data.put_project(p),
            Reply::Label(l) => st.data.put_label(l),
            _ => {}
        }
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
        self.store.set_cache(&json::to_string(&data.to_value()));
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
