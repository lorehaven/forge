//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use daybook_app::{Daybook, Setup, daybook};
use daybook_core::calendar::{all_day_time, at_local};
use daybook_core::{Data, Label, ListRef, Msg, Project, Recurrence, Request, Task, Unit};
use quench_core::date::Date;
use quench_core::time::Time;
use quench_tz::Zone;
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::test_renderer::NodeId;

fn find(r: &TestRenderer, tag: &str, attr: (&str, &str)) -> NodeId {
    r.document_order()
        .into_iter()
        .find(|n| {
            r.tag(*n).as_deref() == Some(tag) && r.attr(*n, attr.0).as_deref() == Some(attr.1)
        })
        .unwrap_or_else(|| panic!("no <{tag} {}={}>: {}", attr.0, attr.1, r.html()))
}

fn present(r: &TestRenderer, tag: &str, (name, value): (&str, &str)) -> bool {
    r.document_order()
        .into_iter()
        .any(|n| r.tag(n).as_deref() == Some(tag) && r.attr(n, name).as_deref() == Some(value))
}

fn click(r: &TestRenderer, id: &str) {
    let node = find(r, "button", ("id", id));
    r.fire(node, &Event::Click);
}

fn type_into(r: &TestRenderer, id: &str, text: &str) {
    let node = r
        .document_order()
        .into_iter()
        .find(|n| {
            r.attr(*n, "id").as_deref() == Some(id)
                && matches!(r.tag(*n).as_deref(), Some("input" | "textarea"))
        })
        .unwrap_or_else(|| panic!("no input {id}: {}", r.html()));
    r.fire(node, &Event::Input(text.to_string()));
}

type Asked = Rc<RefCell<Vec<Request>>>;

fn zone() -> Zone {
    Zone::named("Europe/Warsaw").unwrap()
}

fn date(y: i32, m: u32, d: u32) -> Date {
    Date::from_ymd(y, m, d).unwrap()
}

fn now() -> Time {
    at_local(date(2026, 10, 28), 10, 0, &zone()).unwrap()
}

thread_local! {
    static CLOCK: RefCell<Option<Rc<quench_ui::platform::ManualPlatform>>> = const { RefCell::new(None) };
}

/// Moves this test's clock on. Every test gets a clock of its own: the platform is per thread and the runner reuses
/// threads, so a clock left by an earlier test would otherwise hold back the timers (animations) of the next.
fn tick(ms: f64) {
    CLOCK
        .with(|c| c.borrow().clone())
        .expect("a test that ticks starts with `live`")
        .advance(ms);
}

type Live = (
    Rc<TestRenderer>,
    Asked,
    Rc<dyn Fn(Msg)>,
    quench_ui::Mount<TestRenderer>,
);

fn live(has_session: bool) -> Live {
    live_on(has_session, true)
}

fn live_on(has_session: bool, ask_server: bool) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let Daybook { view, deliver } = daybook(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            ask_server,
            server: "https://h.example".into(),
            list: ListRef::Inbox,
            has_session,
            zone: zone(),
            clock: Rc::new(now),
            toast_ms: 0,
        },
    );
    let r = Rc::new(TestRenderer::new());
    let mounted = mount(&r, &r.root(), view);
    (r, asked, deliver, mounted)
}

fn task(id: &str, content: &str) -> Task {
    Task {
        id: id.into(),
        project_id: None,
        content: content.into(),
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

fn signed_in(tasks: Vec<Task>) -> Live {
    let l = live(true);
    (l.2)(Msg::Data(Box::new(Data {
        projects: vec![Project {
            id: "p1".into(),
            name: "Home".into(),
            sort_order: 0,
        }],
        labels: vec![Label {
            id: "l1".into(),
            name: "urgent".into(),
            color: "#ef4444".into(),
        }],
        tasks,
    })));
    l
}

#[quench_test::test]
fn without_a_session_the_form_checks_then_hands_the_password_over_once() {
    let (r, asked, deliver, _m) = live(false);
    assert!(r.text().contains("Sign in to Daybook"), "{}", r.text());
    r.fire(find(&r, "form", ("id", "login-form")), &Event::Submit);
    assert!(asked.borrow().is_empty(), "an empty form is not sent");
    let inputs: Vec<_> = r
        .document_order()
        .into_iter()
        .filter(|n| r.tag(*n).as_deref() == Some("input"))
        .collect();
    for (input, text) in inputs.iter().zip(["https://h.example/", " me ", "secret"]) {
        r.fire(*input, &Event::Input(text.to_string()));
    }
    r.fire(find(&r, "form", ("id", "login-form")), &Event::Submit);
    assert_eq!(
        *asked.borrow(),
        [Request::Login {
            server: "https://h.example".into(),
            username: "me".into(),
            password: "secret".into()
        }]
    );
    deliver(Msg::LoginFailed(
        "the user name or password is wrong".into(),
    ));
    assert!(
        r.text().contains("the user name or password is wrong"),
        "{}",
        r.text()
    );
    deliver(Msg::LoggedIn);
    assert!(!present(&r, "form", ("id", "login-form")));
}

#[quench_test::test]
fn a_page_does_not_ask_for_a_server() {
    let (r, asked, _deliver, _m) = live_on(false, false);
    assert!(!r.text().contains("Server URL"), "{}", r.text());
    let inputs: Vec<_> = r
        .document_order()
        .into_iter()
        .filter(|n| r.tag(*n).as_deref() == Some("input"))
        .collect();
    assert_eq!(inputs.len(), 2, "user name and password only");
    for (input, text) in inputs.iter().zip(["me", "secret"]) {
        r.fire(*input, &Event::Input(text.to_string()));
    }
    r.fire(find(&r, "form", ("id", "login-form")), &Event::Submit);
    assert!(
        matches!(asked.borrow().as_slice(), [Request::Login { server, .. }] if server.is_empty())
    );
}

#[quench_test::test]
fn the_inbox_shows_its_tasks_by_due_date_with_the_due_text() {
    let mut later = task("b", "Later task");
    later.due_at = Some(all_day_time(date(2026, 10, 30)));
    later.due_all_day = true;
    let mut today = task("a", "Today task");
    today.due_at = Some(at_local(date(2026, 10, 28), 14, 30, &zone()).unwrap());
    let mut elsewhere = task("c", "In the list");
    elsewhere.project_id = Some("p1".into());
    let (r, _asked, _d, _m) = signed_in(vec![later, task("z", "No date"), elsewhere, today]);
    let text = r.text();
    let (a, b, z) = (
        text.find("Today task").unwrap(),
        text.find("Later task").unwrap(),
        text.find("No date").unwrap(),
    );
    assert!(a < b && b < z, "{text}");
    assert!(
        text.contains("Today 14:30") && text.contains("Fri 30 Oct"),
        "{text}"
    );
    assert!(!text.contains("In the list"), "that one is in another list");
}

#[quench_test::test]
fn completed_tasks_are_not_listed_and_there_is_no_way_to_show_them() {
    let mut done = task("d", "Done long ago");
    done.is_completed = true;
    let (r, _asked, deliver, _m) = signed_in(vec![task("a", "Milk"), done]);
    assert!(r.text().contains("Milk"));
    assert!(!r.text().contains("Done long ago"), "{}", r.text());
    assert!(!r.text().contains("Show completed"), "{}", r.text());
    // A task checked off at once leaves the list: the host sends the data with it marked.
    let mut checked = task("a", "Milk");
    checked.is_completed = true;
    deliver(Msg::Data(Box::new(Data {
        tasks: vec![checked],
        ..Data::default()
    })));
    assert!(!r.text().contains("Milk"), "{}", r.text());
}

#[quench_test::test]
fn checking_a_task_and_opening_a_list_are_asked_of_the_host() {
    let (r, asked, _d, _m) = signed_in(vec![task("a", "Milk")]);
    click(&r, "check-a");
    assert_eq!(
        *asked.borrow(),
        [Request::SetCompleted {
            id: "a".into(),
            done: true
        }]
    );
    click(&r, "nav-list-p1");
    assert_eq!(
        asked.borrow().last(),
        Some(&Request::Open(ListRef::Project("p1".into())))
    );
    assert!(r.text().contains("Home"), "{}", r.text());
    assert!(r.text().contains("Nothing here"), "{}", r.text());
}

#[quench_test::test]
fn a_new_task_needs_a_title_and_is_sent_as_the_form_has_it() {
    let (r, asked, deliver, _m) = signed_in(vec![]);
    click(&r, "add-task");
    tick(300.0);
    click(&r, "save-task");
    assert!(r.text().contains("Enter a title"), "{}", r.text());
    assert!(asked.borrow().is_empty());
    type_into(&r, "task-title", "Buy milk");
    click(&r, "due");
    tick(300.0);
    click(&r, "day-2026-10-30");
    click(&r, "date-ok");
    click(&r, "all-day");
    assert!(r.text().contains("Fri 30 Oct"), "{}", r.text());
    click(&r, "reminder");
    click(&r, "remind-0");
    assert!(r.text().contains("On the day, 09:00"), "{}", r.text());
    click(&r, "save-task");
    let sent = asked.borrow().clone();
    let [Request::Save { id: None, draft }] = sent.as_slice() else {
        panic!("{sent:?}")
    };
    assert_eq!(draft.content, "Buy milk");
    assert_eq!(draft.due().unwrap().date, date(2026, 10, 30));
    assert!(draft.due().unwrap().all_day());
    deliver(Msg::Saved(Ok(())));
    tick(300.0);
    assert!(
        !present(&r, "div", ("id", "task-form")),
        "the form closes when the save is done"
    );
}

#[quench_test::test]
fn a_date_written_in_the_title_becomes_the_due_date() {
    let (r, asked, _deliver, _m) = signed_in(vec![]);
    click(&r, "add-task");
    tick(300.0);
    type_into(&r, "task-title", "Pay rent nov 6 5pm");
    assert!(present(&r, "p", ("id", "due-hint")));
    assert!(r.text().contains("Due Fri 6 Nov"), "{}", r.text());
    click(&r, "save-task");
    let sent = asked.borrow().clone();
    let [Request::Save { id: None, draft }] = sent.as_slice() else {
        panic!("{sent:?}")
    };
    assert_eq!(draft.content, "Pay rent");
    let due = draft.due().unwrap();
    assert_eq!(due.date, date(2026, 11, 6));
    assert_eq!(due.clock, Some((17, 0)));
}

#[quench_test::test]
fn a_long_list_is_shown_a_page_at_a_time() {
    let tasks: Vec<Task> = (0..60)
        .map(|i| task(&format!("t{i}"), &format!("Task {i}")))
        .collect();
    let (r, _asked, _deliver, _m) = signed_in(tasks);
    assert!(r.text().contains("Show more (40 left)"), "{}", r.text());
    assert!(!r.text().contains("Task 30"));
    click(&r, "show-more");
    assert!(r.text().contains("Task 30"), "{}", r.text());
    assert!(r.text().contains("Show more (20 left)"), "{}", r.text());
}

#[quench_test::test]
fn a_refused_save_keeps_the_form_open_with_the_reason() {
    let (r, _asked, deliver, _m) = signed_in(vec![]);
    click(&r, "add-task");
    tick(300.0);
    type_into(&r, "task-title", "x");
    click(&r, "save-task");
    deliver(Msg::Saved(Err("the server did not accept that".into())));
    assert!(present(&r, "div", ("id", "task-form")));
    assert!(
        r.text().contains("the server did not accept that"),
        "{}",
        r.text()
    );
}

#[quench_test::test]
fn the_time_picker_gives_a_timed_due_date() {
    let (r, asked, _d, _m) = signed_in(vec![]);
    click(&r, "add-task");
    tick(300.0);
    type_into(&r, "task-title", "Call");
    click(&r, "due");
    tick(300.0);
    click(&r, "day-2026-10-29");
    click(&r, "date-ok");
    click(&r, "choose-time");
    click(&r, "usual-1800");
    click(&r, "minute-up");
    click(&r, "time-ok");
    assert!(r.text().contains("Tomorrow 18:05"), "{}", r.text());
    click(&r, "save-task");
    let sent = asked.borrow().clone();
    let [Request::Save { draft, .. }] = sent.as_slice() else {
        panic!("{sent:?}")
    };
    assert_eq!(draft.due().unwrap().clock, Some((18, 5)));
}

#[quench_test::test]
fn a_repeat_needs_a_due_date_and_a_custom_one_is_built_from_the_pieces() {
    let (r, asked, _d, _m) = signed_in(vec![]);
    click(&r, "add-task");
    tick(300.0);
    type_into(&r, "task-title", "Gym");
    click(&r, "repeat");
    assert!(
        !present(&r, "button", ("id", "repeat-week")),
        "no menu without a due date"
    );
    click(&r, "due");
    tick(300.0);
    click(&r, "day-2026-10-29");
    click(&r, "date-ok");
    click(&r, "all-day");
    click(&r, "repeat");
    click(&r, "repeat-custom");
    click(&r, "unit-week");
    click(&r, "interval-up");
    click(&r, "weekday-1");
    click(&r, "weekday-4");
    click(&r, "repeat-ok");
    assert!(
        r.text().contains("Every 2 weeks on Mon, Thu"),
        "{}",
        r.text()
    );
    click(&r, "save-task");
    let sent = asked.borrow().clone();
    let [Request::Save { draft, .. }] = sent.as_slice() else {
        panic!("{sent:?}")
    };
    let rule = draft.recurrence().unwrap();
    assert_eq!(
        (rule.unit, rule.interval, rule.weekdays.clone()),
        (Unit::Week, 2, vec![1, 4])
    );
}

#[quench_test::test]
fn a_repeating_task_offers_to_mark_one_occurrence_done() {
    let mut t = task("a", "Water plants");
    t.due_at = Some(all_day_time(date(2026, 10, 28)));
    t.due_all_day = true;
    t.recurrence = Some(Recurrence::every(Unit::Day, "Europe/Warsaw"));
    let (r, asked, _d, _m) = signed_in(vec![t]);
    assert!(r.text().contains("Every day"), "{}", r.text());
    click(&r, "task-a");
    tick(300.0);
    click(&r, "occurrence-done");
    assert_eq!(
        *asked.borrow(),
        [Request::SetCompleted {
            id: "a".into(),
            done: true
        }]
    );
}

#[quench_test::test]
fn deleting_a_task_asks_first() {
    let (r, asked, _d, _m) = signed_in(vec![task("a", "Milk")]);
    let swipe = r
        .document_order()
        .into_iter()
        .find(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.contains("q-swipe-action"))
        })
        .unwrap();
    r.fire(swipe, &Event::Click);
    tick(300.0);
    assert!(r.text().contains("Delete “Milk”?"), "{}", r.text());
    assert!(asked.borrow().is_empty());
    click(&r, "confirm-yes");
    assert_eq!(*asked.borrow(), [Request::Delete { id: "a".into() }]);
}

#[quench_test::test]
fn a_new_list_is_named_and_asked_for() {
    let (r, asked, _d, _m) = signed_in(vec![]);
    click(&r, "new-list");
    tick(300.0);
    type_into(&r, "name", "Garden");
    click(&r, "name-ok");
    assert_eq!(
        *asked.borrow(),
        [Request::CreateProject {
            name: "Garden".into()
        }]
    );
}

#[quench_test::test]
fn the_widget_can_open_the_form_for_a_new_task_in_a_list_and_a_task_waits_for_its_data() {
    let (r, _asked, deliver, _m) = live(true);
    deliver(Msg::Show {
        list: ListRef::Project("p1".into()),
        add: true,
        task: None,
    });
    tick(300.0);
    assert!(present(&r, "div", ("id", "task-form")), "{}", r.text());
    click(&r, "cancel-task");
    tick(300.0);
    deliver(Msg::Show {
        list: ListRef::Inbox,
        add: false,
        task: Some("a".into()),
    });
    assert!(!present(&r, "div", ("id", "task-form")), "no data yet");
    deliver(Msg::Data(Box::new(Data {
        tasks: vec![task("a", "Milk")],
        ..Data::default()
    })));
    tick(300.0);
    assert!(present(&r, "div", ("id", "task-form")), "{}", r.text());
}

#[quench_test::test]
fn a_session_that_ended_shows_the_sign_in_again() {
    let (r, _asked, deliver, _m) = signed_in(vec![task("a", "Milk")]);
    deliver(Msg::SignedOut);
    assert!(present(&r, "form", ("id", "login-form")));
    assert!(r.text().contains("Sign in again"), "{}", r.text());
    assert!(!r.text().contains("Milk"));
}

/// One task changing must not rebuild the list: with hundreds of tasks that made every tap slow and the screen jump.
#[quench_test::test]
fn changing_one_task_leaves_the_other_rows_alone() {
    let tasks: Vec<Task> = (0..300)
        .map(|i| task(&format!("t{i:03}"), &format!("Task {i}")))
        .collect();
    let (r, _asked, deliver, _mounted) = signed_in(tasks.clone());
    let untouched = find(&r, "button", ("id", "task-t010"));
    let build = r.ops().len();
    r.clear_ops();

    let mut changed = tasks;
    changed[5].is_completed = true;
    deliver(Msg::Data(Box::new(Data {
        projects: vec![],
        labels: vec![],
        tasks: changed,
    })));
    assert_eq!(
        find(&r, "button", ("id", "task-t010")),
        untouched,
        "the row of an untouched task is the same element"
    );
    let after = r.ops().len();
    assert!(
        // The list is one page of rows, and a task checked off brings the next one into the page: that row is built.
        after * 8 < build,
        "one change took {after} operations against {build} to build the list"
    );
}

quench_test::main!();

/// The rules of the style sheet as `(selector, declarations)`, the ones inside `@media` included.
fn rules() -> Vec<(String, String)> {
    let css: String = daybook_app::STYLE
        .split("/*")
        .enumerate()
        .map(|(i, part)| {
            if i == 0 {
                part
            } else {
                part.split_once("*/").map_or("", |(_, rest)| rest)
            }
        })
        .collect();
    css.split('}')
        .filter_map(|rule| rule.rsplit_once('{'))
        .map(|(selector, body)| {
            (
                selector
                    .rsplit(['{', ';'])
                    .next()
                    .unwrap_or(selector)
                    .trim()
                    .to_string(),
                body.to_string(),
            )
        })
        .filter(|(selector, _)| !selector.is_empty() && !selector.starts_with('@'))
        .collect()
}

#[quench_test::test]
fn the_style_keeps_to_what_a_phone_paints_quickly() {
    // The native renderer draws every frame in software, on the phone's own CPU. A blurred shadow is drawn over the whole
    // element and a border all round a rounded box that fills the screen is as dear: with those on the list card and the
    // inputs a screen of tasks took 100+ ms a frame on a desktop (5x as much as without), and every tap felt half a second
    // late. Only the two small things that float have a shadow; the list's card has no border all round.
    let rules = rules();
    assert!(rules.len() > 50, "the rules were read: {}", rules.len());
    for (selector, body) in &rules {
        if body.contains("box-shadow") {
            assert!(
                selector == ".q-fab" || selector == ".q-dialog",
                "{selector} has a box-shadow: that is drawn on every frame; keep to a border"
            );
        }
        if selector == ".dbk-tasks" {
            let all_round = body
                .split(';')
                .any(|d| d.trim_start().starts_with("border:"));
            assert!(
                !all_round,
                ".dbk-tasks: a border all round a rounded card is dear to paint"
            );
            assert!(
                !body.contains("overflow"),
                ".dbk-tasks: a clip on a screen-sized box is dear to paint"
            );
        }
    }
}
