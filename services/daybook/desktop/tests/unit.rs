#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use daybook_core::Reminder;
use daybook_desktop::{Alarms, Later};
use quench_core::time::Time;

type Timers = Rc<RefCell<Vec<(u64, Box<dyn FnOnce()>)>>>;

fn reminder(id: &str, at: i64) -> Reminder {
    Reminder {
        id: 1,
        task_id: id.into(),
        at: Time::from_unix(at),
        title: id.into(),
        body: "Due".into(),
    }
}

struct Rig {
    timers: Timers,
    clock: Rc<RefCell<i64>>,
    shown: Rc<RefCell<Vec<String>>>,
    alarms: Rc<Alarms>,
}

fn rig() -> Rig {
    let timers: Timers = Rc::default();
    let clock = Rc::new(RefCell::new(1000_i64));
    let shown: Rc<RefCell<Vec<String>>> = Rc::default();
    let later: Later = {
        let timers = Rc::clone(&timers);
        Rc::new(move |ms, f| timers.borrow_mut().push((ms, f)))
    };
    let (c, s) = (Rc::clone(&clock), Rc::clone(&shown));
    let alarms = Alarms::new(
        later,
        Rc::new(move || Time::from_unix(*c.borrow())),
        Rc::new(move |t, _| s.borrow_mut().push(t.to_string())),
    );
    Rig {
        timers,
        clock,
        shown,
        alarms,
    }
}

impl Rig {
    /// Lets the oldest timer go off now.
    fn go(&self) {
        let (_, f) = self.timers.borrow_mut().remove(0);
        f();
    }
}

#[quench_test::test]
fn the_soonest_reminder_gets_a_timer_for_its_time() {
    let r = rig();
    r.alarms.set(&[reminder("b", 1200), reminder("a", 1030)]);
    assert_eq!(r.timers.borrow()[0].0, 30_000);
}

#[quench_test::test]
fn a_reminder_is_shown_once_when_its_time_comes() {
    let r = rig();
    r.alarms.set(&[reminder("a", 1030), reminder("b", 1100)]);
    *r.clock.borrow_mut() = 1031;
    r.go();
    assert_eq!(*r.shown.borrow(), ["a"]);
    assert_eq!(r.alarms.waiting(), 1);
    assert_eq!(
        r.timers.borrow()[0].0,
        60_000,
        "waits for b, looking again within a minute"
    );
    *r.clock.borrow_mut() = 1100;
    r.go();
    assert_eq!(*r.shown.borrow(), ["a", "b"]);
    assert!(r.timers.borrow().is_empty());
}

#[quench_test::test]
fn new_reminders_replace_the_old_and_stale_timers_do_nothing() {
    let r = rig();
    r.alarms.set(&[reminder("old", 1010)]);
    r.alarms.set(&[reminder("new", 1500)]);
    *r.clock.borrow_mut() = 1011;
    r.go();
    assert!(r.shown.borrow().is_empty(), "the replaced timer is ignored");
    assert_eq!(r.alarms.waiting(), 1);
    r.alarms.set(&[]);
    assert_eq!(r.alarms.waiting(), 0);
}

quench_test::main!();
