//! The device's side of the controller in a page: the calls go to the services one after another (a refresh token is
//! spent when it is used, so two calls must not renew it at once), there are no scheduled reminders (a page cannot wake
//! when it is closed), and the clock and the zone are the browser's.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use daybook_core::Reminder;
use daybook_host::{Op, Platform, Reply};
use daybook_net::NetError;
use quench_core::time::Time;
use quench_tz::Zone;

use crate::client::{Boxed, Client, Keep, Transport};

type Then = Box<dyn FnOnce(Result<Reply, NetError>)>;

/// How a background task is started (the browser's executor, or a test's).
pub type Spawn = Rc<dyn Fn(Boxed<'static, ()>)>;

/// How a call is delayed.
pub type Later = Rc<dyn Fn(u64, Box<dyn FnOnce()>)>;

struct Inner<T: Transport, K: Keep> {
    client: Client<T, K>,
    queue: RefCell<VecDeque<(Op, Then)>>,
    running: Cell<bool>,
    spawn: Spawn,
}

/// The platform of a page.
pub struct Web<T: Transport + 'static, K: Keep + 'static> {
    inner: Rc<Inner<T, K>>,
    now: Rc<dyn Fn() -> Time>,
    zone: Zone,
    later: Later,
}

impl<T: Transport + 'static, K: Keep + 'static> Web<T, K> {
    pub fn new(
        client: Client<T, K>,
        spawn: Spawn,
        now: Rc<dyn Fn() -> Time>,
        zone: Zone,
        later: Later,
    ) -> Self {
        Self {
            inner: Rc::new(Inner {
                client,
                queue: RefCell::default(),
                running: Cell::new(false),
                spawn,
            }),
            now,
            zone,
            later,
        }
    }

    #[must_use]
    pub fn signed_in(&self) -> bool {
        self.inner.client.signed_in()
    }
}

fn pump<T: Transport + 'static, K: Keep + 'static>(inner: &Rc<Inner<T, K>>) {
    if inner.running.get() {
        return;
    }
    let Some((op, then)) = inner.queue.borrow_mut().pop_front() else {
        return;
    };
    inner.running.set(true);
    let again = Rc::clone(inner);
    (inner.spawn)(Box::pin(async move {
        let result = again.client.execute(op).await;
        again.running.set(false);
        then(result);
        pump(&again);
    }));
}

impl<T: Transport + 'static, K: Keep + 'static> Platform for Web<T, K> {
    fn call(&self, op: Op, then: Box<dyn FnOnce(Result<Reply, NetError>)>) {
        self.inner.queue.borrow_mut().push_back((op, then));
        pump(&self.inner);
    }

    fn replace_reminders(&self, _reminders: &[Reminder]) {}

    fn now(&self) -> Time {
        (self.now)()
    }

    fn zone(&self) -> Zone {
        self.zone.clone()
    }

    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        (self.later)(ms, f);
    }
}
