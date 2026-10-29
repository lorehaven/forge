//! Daybook on Linux: the device's side of the shared controller. Calls to the services run on the worker thread;
//! reminders are timers that live as long as the window does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use daybook_core::Reminder;
use daybook_host::Platform;
use daybook_host::remote::{Op, Remote, Reply};
use daybook_host::worker::{Answer, Worker};
use daybook_net::NetError;
use quench_core::time::Time;
use quench_tz::Zone;

/// The server the sign-in offers until another is typed.
pub const DEFAULT_SERVER: &str = "https://ennor.ddns.net";

/// How a thread tells the program's thread.
pub enum Event {
    /// The answer to a call of the worker.
    Worker(u64, Answer),
}

/// How the platform asks to be called back later.
pub type Later = Rc<dyn Fn(u64, Box<dyn FnOnce()>)>;

/// Shows a reminder: its title and its text.
pub type Show = Rc<dyn Fn(&str, &str)>;

/// How long a timer waits at most before looking at the clock again (a computer sleeps, and its timers with it).
const LOOK_AGAIN_MS: u64 = 60_000;

/// Says something with a desktop notification (`notify-send`), when there is one to say it with.
pub fn notify_desktop(title: &str, body: &str) {
    let _ = std::process::Command::new("notify-send")
        .args(["--app-name=Daybook", title, body])
        .status();
}

/// The reminders waiting for their time, and the timer that shows the next one.
pub struct Alarms {
    list: RefCell<Vec<Reminder>>,
    generation: Cell<u64>,
    later: Later,
    now: Rc<dyn Fn() -> Time>,
    show: Show,
}

impl Alarms {
    #[must_use]
    pub fn new(later: Later, now: Rc<dyn Fn() -> Time>, show: Show) -> Rc<Self> {
        Rc::new(Self {
            list: RefCell::default(),
            generation: Cell::new(0),
            later,
            now,
            show,
        })
    }

    /// Makes the waiting reminders exactly `reminders`.
    pub fn set(self: &Rc<Self>, reminders: &[Reminder]) {
        *self.list.borrow_mut() = reminders.to_vec();
        self.generation.set(self.generation.get() + 1);
        self.arm();
    }

    #[must_use]
    pub fn waiting(&self) -> usize {
        self.list.borrow().len()
    }

    fn arm(self: &Rc<Self>) {
        let Some(next) = self.list.borrow().iter().map(|r| r.at).min() else {
            return;
        };
        let wait =
            u64::try_from((next.timestamp_millis() - (self.now)().timestamp_millis()).max(0))
                .unwrap_or(0);
        let generation = self.generation.get();
        let me = Rc::clone(self);
        (self.later)(
            wait.min(LOOK_AGAIN_MS),
            Box::new(move || {
                if me.generation.get() != generation {
                    return;
                }
                me.fire_due();
                me.arm();
            }),
        );
    }

    fn fire_due(&self) {
        let now = (self.now)();
        let due: Vec<Reminder> = {
            let mut list = self.list.borrow_mut();
            let (due, rest): (Vec<_>, Vec<_>) = list.drain(..).partition(|r| r.at <= now);
            *list = rest;
            due
        };
        for r in due {
            (self.show)(&r.title, &r.body);
        }
    }
}

/// The device's side of the app on Linux.
pub struct Linux {
    worker: Rc<Worker>,
    remote: Arc<Remote>,
    later: Later,
    alarms: Rc<Alarms>,
}

impl Linux {
    #[must_use]
    pub fn new(worker: Rc<Worker>, remote: Arc<Remote>, later: Later) -> Self {
        let alarms = Alarms::new(
            Rc::clone(&later),
            Rc::new(Time::now),
            Rc::new(notify_desktop),
        );
        Self {
            worker,
            remote,
            later,
            alarms,
        }
    }
}

impl Platform for Linux {
    fn call(&self, op: Op, then: Box<dyn FnOnce(Result<Reply, NetError>)>) {
        self.worker.call(&self.remote, op, then);
    }

    fn replace_reminders(&self, reminders: &[Reminder]) {
        self.alarms.set(reminders);
    }

    fn now(&self) -> Time {
        Time::now()
    }

    fn zone(&self) -> Zone {
        Zone::local()
    }

    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        (self.later)(ms, f);
    }
}
