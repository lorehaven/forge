#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use daybook_core::calendar::{all_day_time, at_local};
use daybook_core::{Data, Draft, ListRef, Msg, Reminder, Request, Task, Unit};
use daybook_host::{Controller, Op, Platform, Reply};
use daybook_net::{NetError, Store};
use quench_core::date::Date;
use quench_core::time::Time;
use quench_tz::Zone;

type Then<T> = Box<dyn FnOnce(T)>;
type Call = (Op, Then<Result<Reply, NetError>>);
type Later = (u64, Box<dyn FnOnce()>);

#[derive(Default)]
struct Fake {
    calls: RefCell<Vec<Call>>,
    later: RefCell<Vec<Later>>,
    reminders: RefCell<Vec<Vec<Reminder>>>,
    signed_out: RefCell<u32>,
    allow: RefCell<bool>,
}

impl Platform for Fake {
    fn call(&self, op: Op, then: Then<Result<Reply, NetError>>) {
        self.calls.borrow_mut().push((op, then));
    }
    fn replace_reminders(&self, reminders: &[Reminder]) {
        self.reminders.borrow_mut().push(reminders.to_vec());
    }
    fn ask_notifications(&self, then: Then<bool>) {
        then(*self.allow.borrow());
    }
    fn signed_out(&self) {
        *self.signed_out.borrow_mut() += 1;
    }
    fn now(&self) -> Time {
        at_local(Date::from_ymd(2026, 10, 28).unwrap(), 10, 0, &self.zone()).unwrap()
    }
    fn zone(&self) -> Zone {
        Zone::named("Europe/Warsaw").unwrap()
    }
    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        self.later.borrow_mut().push((ms, f));
    }
}

impl Fake {
    /// Answers the oldest call, and tells what it was.
    fn answer(&self, result: Result<Reply, NetError>) -> Op {
        let (op, then) = self.calls.borrow_mut().remove(0);
        then(result);
        op
    }
    fn fire_later(&self) {
        let all: Vec<_> = self.later.borrow_mut().drain(..).collect();
        for (_, f) in all {
            f();
        }
    }
}

fn task(id: &str) -> Task {
    Task {
        id: id.into(),
        project_id: None,
        content: id.into(),
        description: None,
        is_completed: false,
        sort_order: 0,
        due_at: None,
        due_all_day: false,
        remind_at: None,
        label_ids: vec![],
        recurrence: None,
        created_at: Time::from_unix(1),
        updated_at: Time::from_unix(1),
    }
}

struct Rig {
    fake: Rc<Fake>,
    out: Rc<RefCell<Vec<Msg>>>,
    c: Rc<Controller>,
    dir: std::path::PathBuf,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn rig(name: &str) -> Rig {
    let dir = std::env::temp_dir().join(format!("daybook-host-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let fake = Rc::new(Fake {
        allow: RefCell::new(true),
        ..Fake::default()
    });
    let out: Rc<RefCell<Vec<Msg>>> = Rc::default();
    let sink = Rc::clone(&out);
    let c = Controller::new(
        fake.clone(),
        Store::new(&dir),
        Rc::new(move |m| sink.borrow_mut().push(m)),
    );
    Rig { fake, out, c, dir }
}

impl Rig {
    fn data(&self) -> Data {
        self.out
            .borrow()
            .iter()
            .rev()
            .find_map(|m| {
                if let Msg::Data(d) = m {
                    Some((**d).clone())
                } else {
                    None
                }
            })
            .expect("data sent")
    }
    fn notices(&self) -> Vec<String> {
        self.out
            .borrow()
            .iter()
            .filter_map(|m| {
                if let Msg::Notice(t) = m {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    fn loaded(&self, tasks: Vec<Task>) {
        self.c.start();
        self.fake.answer(Ok(Reply::Loaded(Box::new(Data {
            tasks,
            ..Data::default()
        }))));
    }
}

#[quench_test::test]
fn start_loads_everything_and_schedules_reminders() {
    let r = rig("start");
    let mut t = task("a");
    t.remind_at = Some(r.fake.now() + quench_core::time::Duration::hours(2));
    r.c.start();
    assert!(matches!(r.fake.calls.borrow()[0].0, Op::Load));
    assert_eq!(r.out.borrow()[0], Msg::Loading(true));
    r.fake.answer(Ok(Reply::Loaded(Box::new(Data {
        tasks: vec![t],
        ..Data::default()
    }))));
    assert_eq!(r.data().tasks.len(), 1);
    assert!(r.out.borrow().contains(&Msg::Loading(false)));
    let sets = r.fake.reminders.borrow();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0][0].task_id, "a");
}

#[quench_test::test]
fn checking_a_task_off_shows_at_once_and_comes_back_if_refused() {
    let r = rig("toggle");
    r.loaded(vec![task("a")]);
    r.c.request(Request::SetCompleted {
        id: "a".into(),
        done: true,
    });
    assert!(r.data().task("a").unwrap().is_completed, "optimistic");
    r.fake.answer(Err(NetError::Status(500)));
    assert!(!r.data().task("a").unwrap().is_completed, "rolled back");
    assert!(r.notices()[0].starts_with("Could not update the task"));
}

#[quench_test::test]
fn a_repeating_task_waits_for_the_service_and_says_when_it_is_next() {
    let r = rig("repeat");
    let mut t = task("a");
    t.due_at = Some(all_day_time(Date::from_ymd(2026, 10, 28).unwrap()));
    t.due_all_day = true;
    t.recurrence = Some(daybook_core::Recurrence::every(Unit::Day, "Europe/Warsaw"));
    r.loaded(vec![t.clone()]);
    r.c.request(Request::SetCompleted {
        id: "a".into(),
        done: true,
    });
    assert!(
        !r.data().task("a").unwrap().is_completed,
        "not closed before the service says"
    );
    let mut next = t;
    next.due_at = Some(all_day_time(Date::from_ymd(2026, 10, 29).unwrap()));
    r.fake.answer(Ok(Reply::Task(Box::new(next))));
    assert_eq!(r.notices(), ["Done — next: Tomorrow"]);
    assert!(!r.data().task("a").unwrap().is_completed);
}

#[quench_test::test]
fn a_delete_is_at_once_and_undone_if_refused() {
    let r = rig("delete");
    r.loaded(vec![task("a"), task("b")]);
    r.c.request(Request::Delete { id: "a".into() });
    assert_eq!(r.data().tasks.len(), 1);
    r.fake.answer(Err(NetError::Network("down".into())));
    assert_eq!(r.data().tasks.len(), 2);
    r.c.request(Request::Delete { id: "a".into() });
    r.fake.answer(Err(NetError::NotFound));
    assert_eq!(r.data().tasks.len(), 1, "already gone is fine");
}

#[quench_test::test]
fn only_the_last_change_schedules_the_reminders() {
    let r = rig("debounce");
    r.loaded(vec![task("a"), task("b")]);
    let before = r.fake.reminders.borrow().len();
    r.c.request(Request::Delete { id: "a".into() });
    r.c.request(Request::Delete { id: "b".into() });
    assert_eq!(r.fake.later.borrow().len(), 2);
    assert!(r.fake.later.borrow().iter().all(|(ms, _)| *ms == 2000));
    r.fake.fire_later();
    assert_eq!(r.fake.reminders.borrow().len(), before + 1);
}

#[quench_test::test]
fn a_session_that_ended_signs_out_and_cancels_the_reminders() {
    let r = rig("expired");
    r.loaded(vec![task("a")]);
    r.c.request(Request::SetCompleted {
        id: "a".into(),
        done: true,
    });
    r.fake.answer(Err(NetError::Unauthorized));
    assert!(r.out.borrow().contains(&Msg::SignedOut));
    assert_eq!(*r.fake.signed_out.borrow(), 1);
    assert!(r.fake.reminders.borrow().last().unwrap().is_empty());
    assert!(matches!(
        r.fake.calls.borrow().last().unwrap().0,
        Op::Logout
    ));
}

#[quench_test::test]
fn signing_in_keeps_the_server_and_loads() {
    let r = rig("login");
    r.c.request(Request::Login {
        server: " https://x.example/ ".into(),
        username: "me".into(),
        password: "secret".into(),
    });
    let op = r.fake.answer(Ok(Reply::Done));
    assert!(matches!(op, Op::Login { ref server, .. } if server == "https://x.example"));
    assert!(r.out.borrow().contains(&Msg::LoggedIn));
    assert_eq!(r.c.settings().server, "https://x.example");
    assert!(matches!(r.fake.calls.borrow()[0].0, Op::Load));
    r.c.request(Request::Login {
        server: "x".into(),
        username: "me".into(),
        password: "bad".into(),
    });
    let _ = r.fake.calls.borrow_mut().remove(0);
    r.fake.answer(Err(NetError::Unauthorized));
    assert!(
        r.out
            .borrow()
            .iter()
            .any(|m| matches!(m, Msg::LoginFailed(_)))
    );
}

#[quench_test::test]
fn saving_sends_the_form_and_reports_back() {
    let r = rig("save");
    r.loaded(vec![]);
    let mut draft = Draft::new(&ListRef::Inbox);
    draft.content = "Milk".into();
    r.c.request(Request::Save {
        id: None,
        draft: Box::new(draft.clone()),
    });
    let (op, then) = r.fake.calls.borrow_mut().remove(0);
    let Op::CreateTask(body) = op else {
        panic!("expected a create")
    };
    assert_eq!(body.get("content").and_then(|v| v.as_str()), Some("Milk"));
    then(Ok(Reply::Task(Box::new(task("new")))));
    assert!(r.out.borrow().contains(&Msg::Saved(Ok(()))));
    assert_eq!(r.data().tasks.len(), 1);
    r.c.request(Request::Save {
        id: Some("new".into()),
        draft: Box::new(draft),
    });
    let op = r.fake.answer(Err(NetError::Invalid));
    assert!(matches!(op, Op::UpdateTask(..)));
    assert!(
        r.out
            .borrow()
            .iter()
            .any(|m| matches!(m, Msg::Saved(Err(_))))
    );
}

#[quench_test::test]
fn deleting_the_open_list_goes_back_to_the_inbox() {
    let r = rig("project");
    r.loaded(vec![]);
    r.c.request(Request::Open(ListRef::Project("p".into())));
    assert_eq!(r.c.settings().list, ListRef::Project("p".into()));
    r.c.request(Request::DeleteProject { id: "p".into() });
    r.fake.answer(Ok(Reply::Done));
    assert_eq!(r.c.settings().list, ListRef::Inbox);
}

#[quench_test::test]
fn a_blocked_notification_permission_is_told() {
    let r = rig("perm");
    *r.fake.allow.borrow_mut() = false;
    let mut t = task("a");
    t.remind_at = Some(r.fake.now() + quench_core::time::Duration::hours(2));
    r.loaded(vec![t]);
    assert!(r.out.borrow().contains(&Msg::NotificationsBlocked(true)));
}

quench_test::main!();
