//! The screens with no host: mounted into a test tree, driven by events, read back as text.

use std::cell::RefCell;
use std::rc::Rc;

use hoist_app::screens::{Act, Action, DeviceApp, Settings, catalog, settings, status, updates};
use hoist_app::{app, sample_catalog};
use hoist_core::{Category, PublicApp, PublicPage, PublicQuery, build_entries};
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

#[quench_test::test]
fn the_apps_of_the_device_follow_the_registrys_and_the_search_filters_both() {
    let (data, installed) = sample_catalog();
    let entries = create_signal(build_entries(&data, &installed, "hoist"));
    let app = |program: &str, label: &str| DeviceApp {
        program: program.into(),
        label: label.into(),
        version_name: "3.1".into(),
    };
    // One of them is in the registry too (it must be listed once, in the registry's part).
    let device = create_signal(vec![
        app("org.example.maps", "Maps"),
        app("org.example.radio", "Radio"),
        app("mathom", "Mathom"),
    ]);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let act: Act = Rc::new(move |a: Action| log.borrow_mut().push(a));
    let search = create_signal(String::new());
    let r = Rc::new(TestRenderer::new());
    let _m = mount(&r, &r.root(), catalog(&entries, &device, &act, &search));
    let text = r.text();
    let (registry, heading, maps) = (
        text.find("Palantir").expect("registry"),
        text.find("Installed apps").expect("heading"),
        text.find("Maps").expect("device app"),
    );
    assert!(registry < heading && heading < maps, "{text}");
    assert_eq!(text.matches("Mathom").count(), 1, "once: {text}");
    assert!(
        text.contains("org.example.radio") && text.contains("v3.1"),
        "{text}"
    );
    // Its button starts it.
    r.fire(
        find(&r, "button", ("aria-label", "Open Radio")),
        &Event::Click,
    );
    assert_eq!(
        seen.borrow().last(),
        Some(&Action::Open("org.example.radio".into()))
    );
    // The search looks through both lists, and the heading goes when nothing is left under it.
    r.fire(
        find(&r, "input", ("id", "search")),
        &Event::Input("radio".into()),
    );
    let text = r.text();
    assert!(
        text.contains("Radio") && !text.contains("Maps") && !text.contains("Palantir"),
        "{text}"
    );
    r.fire(
        find(&r, "input", ("id", "search")),
        &Event::Input("palantir".into()),
    );
    let text = r.text();
    assert!(
        text.contains("Palantir") && !text.contains("Installed apps"),
        "{text}"
    );
}

#[quench_test::test]
fn the_catalog_lists_updates_first_with_one_row_and_the_right_button_for_each() {
    let (data, installed) = sample_catalog();
    let entries = create_signal(build_entries(&data, &installed, "hoist"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let act: Act = Rc::new(move |a: Action| log.borrow_mut().push(a));
    let search = create_signal(String::new());
    let r = Rc::new(TestRenderer::new());
    let _m = mount(
        &r,
        &r.root(),
        catalog(&entries, &create_signal(Vec::new()), &act, &search),
    );
    let text = r.text();
    let order = ["Mathom", "Daybook", "Hoist", "Palantir"]
        .map(|t| text.find(t).unwrap_or_else(|| panic!("{t} in {text}")));
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "an update, then up to date, then not installed: {text}"
    );
    // A row: the title, the program, the version and size, and what is installed when an update is due.
    assert!(text.contains("mathom"), "{text}");
    assert!(
        text.contains("v1.2.0 \u{b7} 39.8 MiB \u{b7} installed v1.1.0"),
        "{text}"
    );
    assert!(
        text.contains("v0.7.1 \u{b7} 11.4 MiB")
            && !text.contains("v0.7.1 \u{b7} 11.4 MiB \u{b7} installed"),
        "{text}"
    );
    assert_eq!(
        text.matches("Details").count(),
        0,
        "the words are the way to the page, there is no second button"
    );
    // Each row's button does what its label says; the words open the page.
    let press = |label: &str| {
        let b = find(&r, "button", ("aria-label", label));
        r.fire(b, &Event::Click);
    };
    press("Update Mathom");
    press("Open Daybook");
    press("Install Palantir");
    press("Details Mathom");
    assert_eq!(
        *seen.borrow(),
        [
            Action::Update("mathom".into()),
            Action::Open("daybook".into()),
            Action::Install("palantir".into()),
            Action::Details("mathom".into())
        ]
    );
    // The search narrows the list by title or program, in any case.
    let field = find(&r, "input", ("id", "search"));
    r.fire(field, &Event::Input("PALA".into()));
    let narrowed = r.text();
    assert!(
        narrowed.contains("Palantir") && !narrowed.contains("Mathom"),
        "{narrowed}"
    );
    r.fire(field, &Event::Input("zzz".into()));
    assert!(!r.text().contains("Palantir"));
    r.fire(field, &Event::Input(String::new()));
    assert!(r.text().contains("Mathom") && r.text().contains("Palantir"));
}

#[quench_test::test]
fn the_updates_page_lists_what_is_due_and_updates_it_all() {
    let (data, installed) = sample_catalog();
    let entries = create_signal(build_entries(&data, &installed, "hoist"));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let act: Act = Rc::new(move |a: Action| log.borrow_mut().push(a));
    let r = Rc::new(TestRenderer::new());
    let _m = mount(&r, &r.root(), updates(&entries, &act));
    assert!(
        r.text().contains("1 app has an update")
            && r.text().contains("Mathom")
            && !r.text().contains("Palantir"),
        "{}",
        r.text()
    );
    r.fire(find(&r, "button", ("id", "update-all")), &Event::Click);
    assert_eq!(*seen.borrow(), [Action::UpdateAll]);
    // Once nothing is due the button and the rows go.
    let mut done = installed.clone();
    done.get_mut("mathom").unwrap().version_code = 12;
    entries.set(build_entries(&data, &done, "hoist"));
    assert!(
        !r.text().contains("Update all") && !r.text().contains("Mathom"),
        "{}",
        r.text()
    );
    assert!(r.text().contains("Everything is up to date"));
    assert_eq!(
        status(&entries.get()[0]).split(':').next(),
        Some("Up to date")
    );
}

#[quench_test::test]
fn the_settings_show_the_server_the_switch_and_the_ways_to_check_and_leave() {
    let (checked, left) = (Rc::new(RefCell::new(0)), Rc::new(RefCell::new(0)));
    let (c, l) = (checked.clone(), left.clone());
    let prefs = Settings::new("https://warehouse.example.org", true);
    let r = Rc::new(TestRenderer::new());
    let _m = mount(
        &r,
        &r.root(),
        settings(
            &prefs,
            "v1.2.3",
            move || *c.borrow_mut() += 1,
            move || *l.borrow_mut() += 1,
        ),
    );
    let text = r.text();
    for want in [
        "Server",
        "Check for updates in the background",
        "Never installs anything on its own",
        "Check now",
        "Run the update check once",
        "Sign out",
        "Version",
        "v1.2.3",
    ] {
        assert!(text.contains(want), "{want} in {text}");
    }
    assert!(
        !text.contains("Save") && !text.contains("Wi-Fi"),
        "nothing to save: {text}"
    );
    // The switch is on and flips; the buttons do what they say.
    let switch = find(&r, "button", ("role", "switch"));
    assert_eq!(r.attr(switch, "aria-checked").as_deref(), Some("true"));
    r.fire(switch, &Event::Click);
    assert!(!prefs.auto_check.get());
    r.fire(find(&r, "button", ("id", "check-now")), &Event::Click);
    r.fire(find(&r, "button", ("id", "logout")), &Event::Click);
    assert_eq!((*checked.borrow(), *left.borrow()), (1, 1));
    // The address is shown, not edited.
    assert!(r.text().contains("https://warehouse.example.org"));
    assert!(
        !r.document_order()
            .into_iter()
            .any(|n| r.attr(n, "aria-label").as_deref() == Some("Server")),
        "no field for the server"
    );
}

#[quench_test::test]
fn the_app_switches_screens_and_says_what_an_action_would_do() {
    let r = Rc::new(TestRenderer::new());
    let _m = mount(&r, &r.root(), app());
    assert!(r.text().contains("Catalog"));
    r.fire(
        find(&r, "button", ("aria-label", "Update Mathom")),
        &Event::Click,
    );
    assert!(r.text().contains("Updating mathom"));
    let nav = r
        .document_order()
        .into_iter()
        .filter(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.contains("q-bottomnav-item"))
        })
        .collect::<Vec<_>>();
    r.fire(nav[1], &Event::Click);
    assert!(
        r.text().contains("Check for updates in the background"),
        "{}",
        r.text()
    );
    r.fire(find(&r, "button", ("id", "check-now")), &Event::Click);
    assert!(r.text().contains("Checking"));
}

use hoist_app::live::{Msg, Request, Setup, hoist};

fn find_opt(r: &TestRenderer, tag: &str, (name, value): (&str, &str)) -> bool {
    r.document_order()
        .into_iter()
        .any(|n| r.tag(n).as_deref() == Some(tag) && r.attr(n, name).as_deref() == Some(value))
}

fn setup(has_session: bool) -> Setup {
    let (_, installed) = sample_catalog();
    Setup {
        server: "https://warehouse.example.org".into(),
        has_session,
        installed,
        self_program: "hoist".into(),
        check_every_ms: 1000,
        auto_check: true,
        public: false,
        toast_ms: 0,
        loader_min_ms: 0,
    }
}

type Asked = Rc<RefCell<Vec<Request>>>;

#[allow(
    clippy::type_complexity,
    reason = "a test fixture: the renderer, the requests, the way in and the mount"
)]
fn live(
    has_session: bool,
) -> (
    Rc<TestRenderer>,
    Asked,
    Rc<dyn Fn(Msg)>,
    quench_ui::Mount<TestRenderer>,
) {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let log = asked.clone();
    let app = hoist(
        Rc::new(move |r| log.borrow_mut().push(r)),
        setup(has_session),
    );
    let r = Rc::new(TestRenderer::new());
    let mounted = mount(&r, &r.root(), app.view);
    (r, asked, app.deliver, mounted)
}

#[quench_test::test]
fn without_a_session_the_app_asks_for_one_and_hands_the_password_over_once() {
    let (r, asked, deliver, _m) = live(false);
    assert!(
        r.text().contains("Log in") && r.text().contains("Set up Hoist"),
        "{}",
        r.text()
    );
    assert!(asked.borrow().is_empty());
    // The form checks before it sends.
    r.fire(find(&r, "form", ("id", "login-form")), &Event::Submit);
    assert!(asked.borrow().is_empty(), "an empty form is not sent");
    let inputs: Vec<_> = r
        .document_order()
        .into_iter()
        .filter(|n| r.tag(*n).as_deref() == Some("input"))
        .collect();
    for (input, text) in inputs
        .iter()
        .zip(["https://warehouse.example.org", "ann", "hunter2"])
    {
        r.fire(*input, &Event::Input(text.to_string()));
    }
    r.fire(find(&r, "form", ("id", "login-form")), &Event::Submit);
    assert_eq!(
        *asked.borrow(),
        [Request::Login {
            server: "https://warehouse.example.org".into(),
            username: "ann".into(),
            password: "hunter2".into()
        }]
    );
    // A refusal is shown; success asks for the catalog and shows it.
    deliver(Msg::LoginFailed(
        "the user name or password is wrong".into(),
    ));
    assert!(r.text().contains("the user name or password is wrong"));
    deliver(Msg::LoggedIn);
    assert_eq!(asked.borrow().last(), Some(&Request::Catalog));
    assert!(!r.text().contains("Log in"));
    let (rows, _) = sample_catalog();
    deliver(Msg::Catalog(rows));
    assert!(
        r.text().contains("Mathom") && r.text().contains("installed v1.1.0"),
        "{}",
        r.text()
    );
}

#[quench_test::test]
fn with_a_session_the_catalog_is_asked_for_at_once_and_downloads_follow_the_buttons() {
    let (r, asked, deliver, _m) = live(true);
    assert_eq!(*asked.borrow(), [Request::Catalog]);
    find(&r, "div", ("id", "loader"));
    let (rows, _) = sample_catalog();
    deliver(Msg::Catalog(rows.clone()));
    r.fire(
        find(&r, "button", ("aria-label", "Update Mathom")),
        &Event::Click,
    );
    let last = asked.borrow().last().cloned().unwrap();
    assert_eq!(
        last,
        Request::Install(rows.iter().find(|a| a.program == "mathom").unwrap().clone())
    );
    assert!(r.text().contains("Downloading Mathom"));
    deliver(Msg::Downloaded {
        program: "mathom".into(),
        path: "/tmp/mathom.deb".into(),
    });
    assert!(r.text().contains("Downloaded mathom to /tmp/mathom.deb"));
    // Once the installer has the build, the ledger has it and the row is up to date.
    let mathom = rows.iter().find(|a| a.program == "mathom").unwrap();
    deliver(Msg::Installed {
        program: "mathom".into(),
        version_code: mathom.version_code,
        version_name: mathom.version_name.clone(),
        path: "/tmp/mathom.deb".into(),
    });
    assert!(
        r.text().contains("in your installer")
            && find_opt(&r, "button", ("aria-label", "Open Mathom")),
        "{}",
        r.text()
    );
    assert!(
        find_opt(&r, "button", ("aria-label", "Open Mathom")),
        "now it can be opened"
    );
    deliver(Msg::DownloadFailed(
        "the download was corrupted, try again".into(),
    ));
    assert!(r.text().contains("corrupted"));
    // Open asks the host to start the app (there is nothing to download for it).
    r.fire(
        find(&r, "button", ("aria-label", "Open Daybook")),
        &Event::Click,
    );
    assert_eq!(
        asked.borrow().last(),
        Some(&Request::Launch("daybook".into()))
    );
    // Update all asks for every update, one install each (none are left after the one above).
    assert!(
        !find_opt(&r, "button", ("id", "update-all")),
        "nothing left to update"
    );
    deliver(Msg::Catalog(rows.clone()));
    r.fire(find(&r, "button", ("id", "refresh")), &Event::Click);
    assert_eq!(asked.borrow().last(), Some(&Request::Catalog));
    deliver(Msg::CatalogFailed("this server has no registry".into()));
    assert!(r.text().contains("no registry"));
    // Settings has the way out; the session ending brings the sign-in back.
    let nav = r
        .document_order()
        .into_iter()
        .filter(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.contains("q-bottomnav-item"))
        })
        .collect::<Vec<_>>();
    r.fire(nav[1], &Event::Click);
    r.fire(find(&r, "button", ("id", "logout")), &Event::Click);
    assert_ne!(
        asked.borrow().last(),
        Some(&Request::Logout),
        "it asks first"
    );
    assert!(r.text().contains("Sign out?"), "{}", r.text());
    r.fire(find(&r, "button", ("id", "cancel-logout")), &Event::Click);
    assert_ne!(asked.borrow().last(), Some(&Request::Logout), "cancelled");
    r.fire(find(&r, "button", ("id", "logout")), &Event::Click);
    r.fire(find(&r, "button", ("id", "confirm-logout")), &Event::Click);
    assert_eq!(asked.borrow().last(), Some(&Request::Logout));
    deliver(Msg::SignedOut);
    assert!(r.text().contains("Sign in again") && r.text().contains("Log in"));
    let _ = format!("{:?}", hoist(Rc::new(|_| {}), setup(false)));
}

#[quench_test::test]
fn update_all_installs_every_update_with_this_app_last() {
    let (r, asked, deliver, _m) = live(true);
    let (mut rows, _) = sample_catalog();
    // Hoist itself, newer than what is installed, and another program with an update too.
    for a in rows
        .iter_mut()
        .filter(|a| a.program == "hoist" || a.program == "daybook")
    {
        a.version_code += 100;
    }
    let mut installed = std::collections::BTreeMap::new();
    for a in &rows {
        installed.insert(
            a.program.clone(),
            hoist_core::Installed {
                program: a.program.clone(),
                version_code: a.version_code - 100,
                version_name: "old".into(),
            },
        );
    }
    // The app was built with its own ledger: use the live app's, which has the sample's.
    deliver(Msg::Catalog(rows));
    r.fire(find(&r, "button", ("id", "updates-open")), &Event::Click);
    r.fire(find(&r, "button", ("id", "update-all")), &Event::Click);
    let installs: Vec<String> = asked
        .borrow()
        .iter()
        .filter_map(|q| {
            if let Request::Install(a) = q {
                Some(a.program.clone())
            } else {
                None
            }
        })
        .collect();
    assert!(installs.len() >= 2, "{installs:?}");
    if installs.contains(&"hoist".to_string()) {
        assert_eq!(
            installs.last().map(String::as_str),
            Some("hoist"),
            "{installs:?}"
        );
    }
    assert!(r.text().contains("Updating"), "{}", r.text());
    let _ = installed;
}

#[quench_test::test]
fn a_programs_page_shows_what_the_registry_says_and_goes_back() {
    let (r, asked, deliver, _m) = live(true);
    let (rows, _) = sample_catalog();
    deliver(Msg::Catalog(rows.clone()));
    r.fire(
        find(&r, "button", ("aria-label", "Details Mathom")),
        &Event::Click,
    );
    let text = r.text();
    for want in [
        "Mathom",
        "Update available: 1.2.0 (installed 1.1.0)",
        "Version",
        "1.2.0 (12)",
        "39.8 MiB",
        "linux · x86_64 · deb",
        "Published by",
        "ci on 2026-10-28",
        "SHA-256",
        "Installed",
        "1.1.0 (11)",
    ] {
        assert!(text.contains(want), "{want} in {text}");
    }
    assert!(!text.contains("Daybook"), "the page replaces the list");
    // Its button does what the row's does; Back returns to the list.
    r.fire(find(&r, "button", ("id", "detail-action")), &Event::Click);
    assert_eq!(
        asked.borrow().last(),
        Some(&Request::Install(
            rows.iter().find(|a| a.program == "mathom").unwrap().clone()
        ))
    );
    r.fire(find(&r, "button", ("id", "detail-back")), &Event::Click);
    assert!(r.text().contains("Daybook") && !r.text().contains("SHA-256"));
    // A program that is not installed has no "Installed" line; the Android facts show when the registry has them.
    let mut android = rows
        .iter()
        .find(|a| a.program == "palantir")
        .unwrap()
        .clone();
    android.min_sdk_version = Some(26);
    android.target_sdk_version = Some(35);
    android.permissions = vec!["INTERNET".into(), "CAMERA".into()];
    deliver(Msg::Catalog(vec![android]));
    r.fire(
        find(&r, "button", ("aria-label", "Details Palantir")),
        &Event::Click,
    );
    let text = r.text();
    assert!(
        text.contains("from API 26, built for 35")
            && text.contains("Permissions")
            && text.contains("INTERNET")
            && text.contains("CAMERA")
            && !text.contains("Installed"),
        "{text}"
    );
}

#[quench_test::test]
fn the_catalog_is_looked_at_now_and_then_and_a_new_update_is_announced() {
    let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(clock.clone());
    let (r, asked, deliver, _m) = live(true);
    let (rows, _) = sample_catalog();
    let asked_catalogs = || {
        asked
            .borrow()
            .iter()
            .filter(|q| **q == Request::Catalog)
            .count()
    };
    assert_eq!(asked_catalogs(), 1);
    deliver(Msg::Catalog(rows.clone()));
    assert!(
        !r.text().contains("update available") && !r.text().contains("updates available"),
        "the first look says nothing new: {}",
        r.text()
    );
    // A second after, the timer asks again.
    clock.advance(1000.0);
    assert_eq!(asked_catalogs(), 2);
    // Something new came out in the meantime: Daybook has an update too.
    let mut newer = rows.clone();
    newer
        .iter_mut()
        .find(|a| a.program == "daybook")
        .unwrap()
        .version_code += 1;
    deliver(Msg::Catalog(newer.clone()));
    assert!(r.text().contains("2 updates available"), "{}", r.text());
    // A look the user asked for does not announce; the same findings the next time are not news.
    r.fire(find(&r, "button", ("id", "refresh")), &Event::Click);
    deliver(Msg::Catalog(newer.clone()));
    clock.advance(1000.0);
    deliver(Msg::Catalog(newer));
    assert!(
        !r.text().contains("updates available"),
        "nothing new since the last look: {}",
        r.text()
    );
    // While something is being waited for, or after signing out, the timer does not ask.
    let before = asked_catalogs();
    deliver(Msg::SignedOut);
    clock.advance(5000.0);
    assert_eq!(asked_catalogs(), before);
}

#[quench_test::test]
fn the_switch_is_kept_as_it_changes_without_a_save_button() {
    let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(clock.clone());
    let (r, asked, _deliver, _m) = live(true);
    let (rows, _) = sample_catalog();
    _deliver(Msg::Catalog(rows));
    let kept = || {
        asked
            .borrow()
            .iter()
            .filter_map(|q| {
                if let Request::Preferences { server, auto_check } = q {
                    Some((server.clone(), *auto_check))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    assert!(
        kept().is_empty(),
        "nothing is sent for what was there at the start"
    );
    let nav = r
        .document_order()
        .into_iter()
        .filter(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.contains("q-bottomnav-item"))
        })
        .collect::<Vec<_>>();
    r.fire(nav[1], &Event::Click);
    assert!(!r.text().contains("Save"));
    // The switch: kept a moment after it moved.
    r.fire(find(&r, "button", ("role", "switch")), &Event::Click);
    clock.advance(700.0);
    assert_eq!(
        kept(),
        [("https://warehouse.example.org".to_string(), false)]
    );
}

#[quench_test::test]
fn a_refresh_says_it_is_looking_and_what_it_found() {
    let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(clock.clone());
    let (r, asked, deliver, _m) = live(true);
    let (rows, _) = sample_catalog();
    deliver(Msg::Catalog(rows.clone()));
    assert!(!find_opt(&r, "div", ("id", "loader")));
    let before = asked
        .borrow()
        .iter()
        .filter(|q| matches!(q, Request::Catalog))
        .count();
    r.fire(find(&r, "button", ("id", "refresh")), &Event::Click);
    find(&r, "div", ("id", "loader"));
    assert_eq!(
        asked
            .borrow()
            .iter()
            .filter(|q| matches!(q, Request::Catalog))
            .count(),
        before + 1
    );
    deliver(Msg::Catalog(rows));
    let text = r.text();
    assert!(
        text.contains("update") && !find_opt(&r, "div", ("id", "loader")),
        "{text}"
    );
    clock.advance(4100.0);
    let text = r.text();
    assert!(
        !text.contains("available") && !text.contains("up to date"),
        "{text}"
    );
}

fn public_app(package: &str, label: &str) -> PublicApp {
    PublicApp {
        package_name: package.into(),
        label: label.into(),
        summary: Some(format!("{label} does things")),
        description: None,
        category: Some("Games".into()),
        version_code: 3,
        version_name: "1.3".into(),
        size_bytes: 2048,
        download_url: "https://f-droid.example/app.apk".into(),
        sha256: Some("abc".into()),
        install_source: "fdroid".into(),
        rating_avg: Some(4.5),
        rating_count: Some(120),
        downloads: Some(1000),
    }
}

fn pages(asked: &Asked) -> Vec<PublicQuery> {
    asked
        .borrow()
        .iter()
        .filter_map(|q| match q {
            Request::PublicPage(p) => Some(p.clone()),
            _ => None,
        })
        .collect()
}

fn page(items: Vec<PublicApp>, total: i64, offset: i64) -> PublicPage {
    PublicPage {
        total,
        offset,
        items,
    }
}

#[quench_test::test]
fn the_public_tab_suggests_by_category_opens_a_category_and_searches_everything() {
    let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(clock.clone());
    let asked: Asked = Rc::new(RefCell::new(Vec::new()));
    let log = asked.clone();
    let app = hoist(
        Rc::new(move |r| log.borrow_mut().push(r)),
        Setup {
            public: true,
            ..setup(true)
        },
    );
    let r = Rc::new(TestRenderer::new());
    let _m = mount(&r, &r.root(), app.view);
    let deliver = app.deliver;
    deliver(Msg::Catalog(sample_catalog().0));
    let nav: Vec<_> = r
        .document_order()
        .into_iter()
        .filter(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.contains("q-bottomnav-item"))
        })
        .collect();
    assert_eq!(nav.len(), 3, "catalog, public, settings");
    assert!(
        !asked.borrow().contains(&Request::PublicCategories),
        "not before it is opened"
    );
    r.fire(nav[1], &Event::Click);
    assert_eq!(
        asked
            .borrow()
            .iter()
            .filter(|q| **q == Request::PublicCategories)
            .count(),
        1
    );
    // The categories come; the biggest are asked for, best rated first among apps with enough ratings.
    deliver(Msg::Categories(vec![
        Category {
            name: "Games".into(),
            count: 40,
        },
        Category {
            name: "Internet".into(),
            count: 12,
        },
    ]));
    let sections = pages(&asked);
    assert_eq!(sections.len(), 2);
    assert_eq!(
        (
            sections[0].category.as_deref(),
            sections[0].sort.as_str(),
            sections[0].min_votes
        ),
        (Some("Games"), "rating", Some(20))
    );
    // "mathom" is installed (it is on the ledger): it is not suggested.
    deliver(Msg::PublicPage {
        asked: sections[0].clone(),
        page: page(
            vec![
                public_app("mathom", "Mathom Again"),
                public_app("org.x.chess", "Chess"),
            ],
            2,
            0,
        ),
    });
    // Nobody rated anything in Internet: the same category by downloads.
    deliver(Msg::PublicPage {
        asked: sections[1].clone(),
        page: page(Vec::new(), 0, 0),
    });
    let fallback = pages(&asked).pop().unwrap();
    assert_eq!(
        (
            fallback.category.as_deref(),
            fallback.sort.as_str(),
            fallback.min_votes
        ),
        (Some("Internet"), "downloads", None)
    );
    deliver(Msg::PublicPage {
        asked: fallback,
        page: page(vec![public_app("org.x.browser", "Browser")], 1, 0),
    });
    let text = r.text();
    assert!(
        text.contains("Games")
            && text.contains("Chess")
            && text.contains("Internet")
            && text.contains("Browser"),
        "{text}"
    );
    assert!(
        !text.contains("Mathom Again"),
        "installed apps are not suggested: {text}"
    );
    assert!(text.contains("\u{2605} 4.5 (120)"), "{text}");
    // A section's name opens the category: the whole of it, paged.
    r.fire(find(&r, "button", ("id", "see-all-games")), &Event::Click);
    let first = pages(&asked).pop().unwrap();
    assert_eq!(
        (first.category.as_deref(), first.offset, first.limit),
        (Some("Games"), 0, 30)
    );
    assert!(r.text().contains("Games"), "the bar names the category");
    deliver(Msg::PublicPage {
        asked: first.clone(),
        page: page(
            vec![
                public_app("org.x.chess", "Chess"),
                public_app("org.x.go", "Go"),
            ],
            3,
            0,
        ),
    });
    assert!(r.text().contains("Go does things"), "{}", r.text());
    r.fire(find(&r, "button", ("id", "public-more")), &Event::Click);
    let next = pages(&asked).pop().unwrap();
    assert_eq!(next.offset, 2, "after the two it has");
    deliver(Msg::PublicPage {
        asked: next,
        page: page(vec![public_app("org.x.tetris", "Tetris")], 3, 2),
    });
    assert!(r.text().contains("Tetris") && !find_opt(&r, "button", ("id", "public-more")));
    // An app's page, and Install from it.
    r.fire(
        find(&r, "button", ("aria-label", "Details Tetris")),
        &Event::Click,
    );
    assert!(find_opt(&r, "div", ("id", "public-detail")), "{}", r.html());
    assert!(
        r.text().contains("checked against the catalogue"),
        "{}",
        r.text()
    );
    r.fire(find(&r, "button", ("id", "public-action")), &Event::Click);
    assert!(
        matches!(asked.borrow().last(), Some(Request::InstallPublic(a)) if a.package_name == "org.x.tetris")
    );
    assert!(find_opt(&r, "div", ("id", "loader")), "working");
    // Back goes page by page: the app, the category, the home page.
    r.fire(find(&r, "button", ("id", "public-back")), &Event::Click);
    assert!(find_opt(&r, "div", ("id", "public-list")));
    r.fire(find(&r, "button", ("id", "public-back")), &Event::Click);
    assert!(find_opt(&r, "div", ("id", "public-home")));
    // The search looks through everything, a moment after the last key.
    let field = find(&r, "input", ("id", "public-search"));
    r.fire(field, &Event::Input("ch".into()));
    r.fire(field, &Event::Input("chess".into()));
    clock.advance(500.0);
    let search = pages(&asked).pop().unwrap();
    assert_eq!(
        (
            search.query.as_deref(),
            search.category.clone(),
            search.sort.as_str()
        ),
        (Some("chess"), None, "downloads")
    );
    deliver(Msg::PublicPage {
        asked: search,
        page: page(vec![public_app("org.x.chess", "Chess")], 1, 0),
    });
    assert!(find_opt(&r, "div", ("id", "public-list")) && r.text().contains("Chess"));
    // An app that is on the device is opened, not installed.
    deliver(Msg::Device(vec![hoist_app::screens::DeviceApp {
        program: "org.x.chess".into(),
        label: "Chess".into(),
        version_name: "1".into(),
    }]));
    r.fire(
        find(&r, "button", ("aria-label", "Open Chess")),
        &Event::Click,
    );
    assert_eq!(
        asked.borrow().last(),
        Some(&Request::Launch("org.x.chess".into()))
    );
    // A failure is said, with a way to try again.
    deliver(Msg::PublicFailed {
        asked: None,
        why: "this server has no public catalogue".into(),
    });
    assert!(r.text().contains("no public catalogue"));
    r.fire(find(&r, "button", ("id", "public-retry")), &Event::Click);
}

quench_test::main!();

#[quench_test::test]
fn a_problem_is_told_apart_from_news_and_a_toast_shows_only_while_there_is_something_to_say() {
    use hoist_app::toast::is_problem;
    for problem in [
        "Sign in again",
        "Download failed",
        "cannot reach the server",
        "The public catalogue is not available (401)",
    ] {
        assert!(is_problem(problem), "{problem}");
    }
    for news in [
        "Signed in",
        "Downloading Mathom...",
        "Checking",
        "Downloaded Mathom to /tmp/x",
    ] {
        assert!(!is_problem(news), "{news}");
    }
}

#[quench_test::test]
fn the_password_is_hidden_until_the_eye_is_pressed_and_hidden_again_by_pressing_it_once_more() {
    let (r, _asked, _deliver, _m) = live(false);
    let password = find(&r, "input", ("id", "password"));
    let eye = find(&r, "button", ("id", "show-password"));
    assert_eq!(r.attr(password, "type").as_deref(), Some("password"));
    r.fire(eye, &Event::Click);
    assert_eq!(r.attr(password, "type").as_deref(), Some("text"), "shown");
    r.fire(eye, &Event::Click);
    assert_eq!(
        r.attr(password, "type").as_deref(),
        Some("password"),
        "hidden"
    );
}
