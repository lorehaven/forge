//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used)]

use std::cell::RefCell;
use std::rc::Rc;

use forklift_app::live::{Forklift, Msg, Request, Setup, forklift};
use forklift_core::{Engine, Found, Manifest, Settings, Snapshot};
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

type Asked = Rc<RefCell<Vec<Request>>>;

fn setup(has_session: bool, confirmed: bool) -> Setup {
    Setup {
        settings: Settings {
            server: "https://h.example".into(),
            storage: "moto".into(),
            storage_confirmed: confirmed,
            ..Settings::default()
        },
        has_session,
        initial: Snapshot::default(),
        phone: true,
        offset_minutes: 0,
        toast_ms: 0,
        loader_min_ms: 0,
    }
}

type Live = (
    Rc<TestRenderer>,
    Asked,
    Rc<dyn Fn(Msg)>,
    quench_ui::Mount<TestRenderer>,
);

fn live(has_session: bool) -> Live {
    live_with(has_session, true)
}

thread_local! {
    static CLOCK: std::cell::RefCell<Option<Rc<quench_ui::platform::ManualPlatform>>> = const { std::cell::RefCell::new(None) };
}

/// Moves this test's clock on. Every test gets a clock of its own: the platform is per thread and the runner reuses
/// threads, so a clock left by an earlier test would otherwise hold back the timers (animations) of the next.
fn tick(ms: f64) {
    let clock = CLOCK.with(|c| c.borrow().clone());
    clock
        .expect("a test that ticks starts with `live`")
        .advance(ms);
}

fn live_with(has_session: bool, confirmed: bool) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let Forklift { view, deliver } = forklift(
        Rc::new(move |q| log.borrow_mut().push(q)),
        setup(has_session, confirmed),
    );
    let r = Rc::new(TestRenderer::new());
    let mounted = mount(&r, &r.root(), view);
    (r, asked, deliver, mounted)
}

fn shot(files: usize, done: usize) -> Snapshot {
    let mut m = Manifest::new();
    let found: Vec<Found> = (1..=files)
        .map(|i| Found {
            local_id: i.to_string(),
            name: format!("{i}.jpg"),
            uri: String::new(),
            size: 1_000_000,
            modified_ms: i as i64,
        })
        .collect();
    m.scan("photos", &found, false);
    for i in 1..=done {
        m.done("photos", &i.to_string(), "x");
    }
    Snapshot::of(
        &Engine::new(m, 2),
        &Settings::default(),
        false,
        Some(3_600_000 * 7 + 60_000 * 5),
    )
}

fn nav(r: &TestRenderer) -> Vec<NodeId> {
    r.document_order()
        .into_iter()
        .filter(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.contains("q-bottomnav-item"))
        })
        .collect()
}

#[quench_test::test]
fn without_a_session_the_form_checks_then_hands_the_password_over_once_and_the_name_without_stray_spaces()
 {
    let (r, asked, _deliver, _m) = live(false);
    assert!(
        r.text().contains("Log in") && r.text().contains("Set up Forklift"),
        "{}",
        r.text()
    );
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
            password: "secret".into(),
        }]
    );
}

fn storage(name: &str, used: Option<u64>, quota: Option<u64>) -> forklift_core::StorageInfo {
    forklift_core::StorageInfo {
        name: name.into(),
        used,
        quota,
    }
}

#[quench_test::test]
fn after_the_sign_in_the_account_s_storages_are_listed_and_one_is_chosen_before_anything_is_sent() {
    let (r, asked, deliver, _m) = live_with(false, false);
    deliver(Msg::LoggedIn);
    assert_eq!(*asked.borrow(), [Request::ListStorages]);
    assert!(
        r.text().contains("Where should this device"),
        "{}",
        r.text()
    );
    assert!(r.text().contains("Asking the server"));
    assert!(
        !present(&r, "div", ("id", "dashboard")),
        "no dashboard before the choice"
    );
    assert!(nav(&r).is_empty(), "no tabs while choosing");
    deliver(Msg::Storages(Box::new(Ok(vec![
        storage("moto", Some(2_000_000_000), Some(10_000_000_000)),
        storage("moto_edge_60_pro", Some(48_900_000_000), None),
    ]))));
    let text = r.text();
    assert!(
        text.contains("moto_edge_60_pro")
            && text.contains("suggested for this device")
            && text.contains("2.0 GB of 10.0 GB"),
        "{text}"
    );
    assert!(text.contains("48.9 GB used"), "{text}");
    assert!(
        !present(&r, "button", ("id", "storage-cancel")),
        "no way out of the first choice"
    );
    r.fire(
        find(&r, "button", ("id", "storage-moto_edge_60_pro")),
        &Event::Click,
    );
    assert_eq!(
        asked.borrow().last(),
        Some(&Request::ChooseStorage("moto_edge_60_pro".into()))
    );
    assert!(present(&r, "div", ("id", "dashboard")), "{}", r.text());
}

#[quench_test::test]
fn a_session_without_a_chosen_storage_asks_at_start_and_an_empty_or_failed_list_says_so() {
    let (r, asked, deliver, _m) = live_with(true, false);
    assert_eq!(*asked.borrow(), [Request::ListStorages]);
    deliver(Msg::Storages(Box::new(Ok(Vec::new()))));
    assert!(r.text().contains("no storage it can use"), "{}", r.text());
    r.fire(find(&r, "button", ("id", "storage-retry")), &Event::Click);
    assert_eq!(asked.borrow().last(), Some(&Request::ListStorages));
    deliver(Msg::Storages(Box::new(Err(
        "cannot reach the server: timed out".into(),
    ))));
    assert!(
        r.text().contains("cannot reach the server: timed out"),
        "{}",
        r.text()
    );
}

#[quench_test::test]
fn the_storage_can_be_changed_later_from_the_settings_and_the_change_can_be_abandoned() {
    let (r, asked, deliver, _m) = live(true);
    open_settings(&r);
    assert!(r.text().contains("moto"), "the current one is shown");
    r.fire(find(&r, "button", ("id", "storage")), &Event::Click);
    assert_eq!(*asked.borrow(), [Request::ListStorages]);
    deliver(Msg::Storages(Box::new(Ok(vec![
        storage("moto", None, None),
        storage("other", None, None),
    ]))));
    let text = r.text();
    assert!(
        text.contains("in use now") && text.contains("sends everything to it again"),
        "{text}"
    );
    r.fire(find(&r, "button", ("id", "storage-cancel")), &Event::Click);
    assert!(present(&r, "div", ("id", "settings")), "{}", r.text());
    assert_eq!(asked.borrow().len(), 1, "nothing was chosen");
}

#[quench_test::test]
fn a_failed_login_is_said_and_a_good_one_opens_the_dashboard() {
    let (r, _asked, deliver, _m) = live(false);
    deliver(Msg::LoginFailed(
        "the user name or password is wrong".into(),
    ));
    assert!(r.text().contains("is wrong") && present(&r, "form", ("id", "login-form")));
    deliver(Msg::LoggedIn);
    assert!(present(&r, "div", ("id", "dashboard")), "{}", r.text());
}

#[quench_test::test]
fn the_dashboard_follows_the_snapshots() {
    let (r, _asked, deliver, _m) = live(true);
    assert!(r.text().contains("Nothing to back up yet"), "{}", r.text());
    deliver(Msg::Snapshot(Box::new(shot(4, 1))));
    let text = r.text();
    assert!(text.contains("Backing up: 1 of 4 files"), "{text}");
    assert!(
        text.contains("3.0 MB to go") && text.contains("Last checked 07:05"),
        "{text}"
    );
    assert!(
        text.contains("Photos") && text.contains("1 backed up") && text.contains("3 waiting"),
        "{text}"
    );
    assert_eq!(
        r.attr(find(&r, "div", ("class", "forklift-bar-fill")), "style")
            .as_deref(),
        Some("width:25%")
    );
    deliver(Msg::Snapshot(Box::new(shot(4, 4))));
    assert!(r.text().contains("All 4 files are backed up"));
}

#[quench_test::test]
fn back_up_now_asks_the_host() {
    let (r, asked, _deliver, _m) = live(true);
    r.fire(find(&r, "button", ("id", "back-up-now")), &Event::Click);
    assert_eq!(*asked.borrow(), [Request::BackUpNow]);
}

#[quench_test::test]
fn a_setting_is_kept_a_moment_after_it_changes_and_nothing_is_sent_at_the_start() {
    let (r, asked, _deliver, _m) = live(true);
    tick(1000.0);
    assert!(asked.borrow().is_empty());
    r.fire(nav(&r)[1], &Event::Click);
    assert!(r.text().contains("Back up automatically"), "{}", r.text());
    r.fire(
        find(&r, "div", ("id", "auto")).into_switch(&r),
        &Event::Click,
    );
    tick(700.0);
    let asked = asked.borrow();
    let [Request::Settings(s)] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    assert!(!s.auto && s.photos && s.storage == "moto", "{s:?}");
}

trait IntoSwitch {
    fn into_switch(self, r: &TestRenderer) -> NodeId;
}

impl IntoSwitch for NodeId {
    /// The switch inside a settings line.
    fn into_switch(self, r: &TestRenderer) -> NodeId {
        let mut stack = vec![self];
        while let Some(n) = stack.pop() {
            if r.attr(n, "role").as_deref() == Some("switch") {
                return n;
            }
            stack.extend(r.children(n));
        }
        panic!("no switch in {}", r.outer_html(self));
    }
}

#[quench_test::test]
fn a_folder_the_host_adds_is_listed_and_can_be_removed() {
    let (r, asked, deliver, _m) = live(true);
    r.fire(nav(&r)[1], &Event::Click);
    assert!(r.text().contains("No folders yet"));
    r.fire(find(&r, "button", ("id", "add-folder")), &Event::Click);
    assert_eq!(*asked.borrow(), [Request::AddFolder]);
    let mut with = Settings {
        server: "https://h.example".into(),
        storage: "moto".into(),
        ..Settings::default()
    };
    with.folders.push(forklift_core::Folder {
        slug: "docs".into(),
        name: "Documents".into(),
        uri: "u".into(),
        enabled: true,
        wifi_only: false,
    });
    deliver(Msg::Settings(Box::new(with)));
    assert!(r.text().contains("Documents"), "{}", r.text());
    // Taking it away is a setting change like the others.
    let remove = r
        .document_order()
        .into_iter()
        .find(|n| r.attr(*n, "aria-label").as_deref() == Some("Remove folder"))
        .unwrap();
    r.fire(remove, &Event::Click);
    tick(700.0);
    assert!(!r.text().contains("Documents"));
    let asked = asked.borrow();
    let Some(Request::Settings(s)) = asked.last() else {
        panic!("{asked:?}")
    };
    assert!(s.folders.is_empty());
}

#[quench_test::test]
fn signing_out_asks_first_and_the_end_of_the_session_brings_the_form_back() {
    let (r, asked, deliver, _m) = live(true);
    r.fire(nav(&r)[1], &Event::Click);
    r.fire(find(&r, "button", ("id", "logout")), &Event::Click);
    assert!(r.text().contains("Sign out?"));
    assert!(asked.borrow().is_empty(), "asking first");
    r.fire(find(&r, "button", ("id", "confirm-logout")), &Event::Click);
    assert_eq!(*asked.borrow(), [Request::Logout]);
    deliver(Msg::SignedOut);
    assert!(
        present(&r, "form", ("id", "login-form")) && r.text().contains("Sign in again"),
        "{}",
        r.text()
    );
}

fn open_settings(r: &TestRenderer) {
    r.fire(nav(r)[1], &Event::Click);
}

#[quench_test::test]
fn the_restore_dialog_asks_before_it_fetches_and_shows_how_it_goes() {
    use forklift_core::{PlanSummary, Progress, RestoreView};
    let (r, asked, deliver, _m) = live(true);
    open_settings(&r);
    assert!(
        !present(&r, "p", ("id", "restore-words")),
        "no dialog until asked"
    );
    r.fire(find(&r, "button", ("id", "restore")), &Event::Click);
    assert_eq!(*asked.borrow(), [Request::PlanRestore]);
    deliver(Msg::Restore(Box::new(RestoreView::Planning)));
    assert!(
        r.text().contains("Looking at what is stored"),
        "{}",
        r.text()
    );
    deliver(Msg::Restore(Box::new(RestoreView::Plan(PlanSummary {
        files: 12,
        bytes: 3_400_000_000,
        present: 40,
        no_target: 2,
    }))));
    let text = r.text();
    assert!(
        text.contains("Restore 12 files (3.4 GB)? 40 already on this device."),
        "{text}"
    );
    assert!(text.contains("2 are left out"), "{text}");
    deliver(Msg::Restore(Box::new(RestoreView::Plan(PlanSummary {
        files: 1,
        bytes: 5_400,
        present: 8,
        no_target: 0,
    }))));
    assert!(
        r.text()
            .contains("Restore 1 file (5.4 KB)? 8 already on this device."),
        "{}",
        r.text()
    );
    deliver(Msg::Restore(Box::new(RestoreView::Plan(PlanSummary {
        files: 12,
        bytes: 3_400_000_000,
        present: 40,
        no_target: 2,
    }))));
    // Cancel closes it and asks nothing of the host; Restore asks.
    r.fire(find(&r, "button", ("id", "restore-cancel")), &Event::Click);
    tick(500.0);
    assert!(!present(&r, "p", ("id", "restore-words")));
    deliver(Msg::Restore(Box::new(RestoreView::Plan(PlanSummary {
        files: 1,
        bytes: 5,
        present: 0,
        no_target: 0,
    }))));
    r.fire(find(&r, "button", ("id", "restore-go")), &Event::Click);
    assert_eq!(asked.borrow().last(), Some(&Request::StartRestore));
    deliver(Msg::Restore(Box::new(RestoreView::Running(Progress {
        total: 10,
        done: 3,
        failed: 1,
    }))));
    assert!(
        r.text()
            .contains("Restoring: 4 of 10 files, 1 could not be restored"),
        "{}",
        r.text()
    );
    r.fire(find(&r, "button", ("id", "restore-stop")), &Event::Click);
    assert_eq!(asked.borrow().last(), Some(&Request::CancelRestore));
    deliver(Msg::Restore(Box::new(RestoreView::Done(Progress {
        total: 10,
        done: 8,
        failed: 2,
    }))));
    assert!(
        r.text()
            .contains("Restored 8 of 10 files. 2 could not be restored."),
        "{}",
        r.text()
    );
    r.fire(find(&r, "button", ("id", "restore-ok")), &Event::Click);
    tick(500.0);
    assert!(!present(&r, "p", ("id", "restore-words")));
}

#[quench_test::test]
fn a_restore_that_could_not_start_says_why() {
    use forklift_core::RestoreView;
    let (r, _asked, deliver, _m) = live(true);
    open_settings(&r);
    deliver(Msg::Restore(Box::new(RestoreView::Failed(
        "cannot reach the server: timed out".into(),
    ))));
    assert!(r.text().contains("cannot reach the server: timed out"));
}

#[quench_test::test]
fn a_folder_can_be_kept_for_wifi_from_the_settings() {
    let (r, asked, deliver, _m) = live(true);
    open_settings(&r);
    let mut with = Settings {
        server: "https://h.example".into(),
        storage: "moto".into(),
        ..Settings::default()
    };
    with.folders.push(forklift_core::Folder {
        slug: "docs".into(),
        name: "Documents".into(),
        uri: "u".into(),
        enabled: true,
        wifi_only: false,
    });
    deliver(Msg::Settings(Box::new(with)));
    assert!(r.text().contains("Any network"), "{}", r.text());
    r.fire(find(&r, "button", ("id", "wifi-docs")), &Event::Click);
    assert!(r.text().contains("Wi-Fi only"));
    tick(700.0);
    let asked = asked.borrow();
    let Some(Request::Settings(s)) = asked.last() else {
        panic!("{asked:?}")
    };
    assert!(s.folders[0].wifi_only);
}

fn sending(names: &[&str]) -> Snapshot {
    let mut m = Manifest::new();
    let found: Vec<Found> = names
        .iter()
        .enumerate()
        .map(|(i, n)| Found {
            local_id: i.to_string(),
            name: (*n).to_string(),
            uri: String::new(),
            size: 1_000,
            modified_ms: i as i64,
        })
        .collect();
    m.scan("photos", &found, false);
    let mut engine = Engine::new(m, 3);
    engine.pump(0, true);
    Snapshot::of(&engine, &Settings::default(), true, Some(3_600_000))
}

fn sub_lines(r: &TestRenderer) -> Vec<String> {
    r.document_order()
        .into_iter()
        .filter(|n| r.attr(*n, "class").as_deref() == Some("forklift-sub"))
        .map(|n| r.text_of(n))
        .collect()
}

#[quench_test::test]
fn the_card_keeps_its_lines_whatever_the_file_names_are() {
    let (r, _asked, deliver, _m) = live(true);
    let before = sub_lines(&r);
    assert_eq!(
        before.len(),
        3,
        "the lines are there before anything happens: {before:?}"
    );
    let long = "IMG_20260101_123456_with_a_very_long_descriptive_name_that_goes_on.jpg";
    for names in [vec!["a.jpg"], vec![long], vec!["a.jpg", long, "b.jpg"]] {
        deliver(Msg::Snapshot(Box::new(sending(&names))));
        let lines = sub_lines(&r);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(
            lines.iter().all(|l| l.chars().count() < 60),
            "no line is long enough to wrap: {lines:?}"
        );
    }
    deliver(Msg::Snapshot(Box::new(sending(&[long]))));
    let text = r.text();
    assert!(
        text.contains('\u{2026}') && !text.contains(long),
        "a long name is cut: {text}"
    );
    deliver(Msg::Snapshot(Box::new(sending(&[
        "a.jpg", "b.jpg", "c.jpg",
    ]))));
    assert!(
        r.text().contains("Sending a.jpg and 2 more") || r.text().contains("and 2 more"),
        "{}",
        r.text()
    );
}

#[quench_test::test]
fn a_computer_has_no_media_library_or_battery_to_set() {
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let mut setup = setup(true, true);
    setup.phone = false;
    let Forklift { view, deliver: _ } = forklift(Rc::new(move |q| log.borrow_mut().push(q)), setup);
    let r = Rc::new(TestRenderer::new());
    let _m = mount(&r, &r.root(), view);
    open_settings(&r);
    let text = r.text();
    assert!(
        text.contains("Back up automatically") && text.contains("Folders"),
        "{text}"
    );
    assert!(
        !text.contains("Videos on Wi-Fi only")
            && !text.contains("charging")
            && !present(&r, "div", ("id", "photos")),
        "{text}"
    );
    // A phone has them (the other tests).
    let (r2, _a, _d, _m2) = live(true);
    open_settings(&r2);
    assert!(r2.text().contains("Videos on Wi-Fi only"));
}

quench_test::main!();

#[quench_test::test]
fn the_dashboard_has_last_times_numbers_at_the_first_frame_before_the_host_says_anything() {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh);
    let mut setup = setup(true, true);
    setup.initial = shot(40, 40);
    let Forklift { view, .. } = forklift(Rc::new(|_| {}), setup);
    let r = Rc::new(TestRenderer::new());
    let _m = mount(&r, &r.root(), view);
    assert!(
        r.text().contains("All 40 files are backed up"),
        "{}",
        r.text()
    );
    assert!(!r.text().contains("Nothing to back up yet"), "{}", r.text());
}

#[quench_test::test]
fn a_scan_under_way_says_how_many_files_it_has_looked_at() {
    let (r, _asked, deliver, _m) = live(true);
    let mut s = shot(5, 2);
    s.working = true;
    s.looked_at = 1234;
    deliver(Msg::Snapshot(Box::new(s)));
    assert!(
        r.text().contains("Looking at your files \u{b7} 1234"),
        "{}",
        r.text()
    );
}
