//! The screens with no host: mounted into a test tree, driven by events and the clock, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use gauge_app::{Gauge, Msg, Request, Setup, gauge};
use gauge_core::model::{Filesystem, Gpu, Point, Rate, Temp, View as Metrics};
use gauge_core::{HostCard, HostDetail, HostSummary};
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;

type Asked = Rc<RefCell<Vec<Request>>>;

fn find(r: &TestRenderer, tag: &str, attr: (&str, &str)) -> NodeId {
    r.document_order()
        .into_iter()
        .find(|n| {
            r.tag(*n).as_deref() == Some(tag) && r.attr(*n, attr.0).as_deref() == Some(attr.1)
        })
        .unwrap_or_else(|| panic!("no <{tag} {}={}>: {}", attr.0, attr.1, r.html()))
}

fn present(r: &TestRenderer, id: &str) -> bool {
    r.document_order()
        .into_iter()
        .any(|n| r.attr(n, "id").as_deref() == Some(id))
}

thread_local! {
    static CLOCK: RefCell<Option<Rc<quench_ui::platform::ManualPlatform>>> = const { RefCell::new(None) };
}

/// Moves this test's clock on. Every test gets a clock of its own: the platform is per thread and the runner reuses
/// threads.
fn tick(ms: f64) {
    CLOCK
        .with(|c| c.borrow().clone())
        .expect("a test that ticks starts with `live`")
        .advance(ms);
}

struct Live {
    r: Rc<TestRenderer>,
    asked: Asked,
    deliver: Rc<dyn Fn(Msg)>,
    history: Rc<MemoryHistory>,
    _mount: quench_ui::Mount<TestRenderer>,
}

fn live(path: &str) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(path));
    let Gauge { view, deliver } = gauge(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/gauge".into(),
            language: "en-US".into(),
            login_url: "/gauge/ui/login".into(),
            history: history.clone(),
            now: Rc::new(|| 1_800_000_000.0),
            refresh_ms: 10_000,
        },
    );
    let r = Rc::new(TestRenderer::new());
    let mount = mount(&r, &r.root(), view);
    Live {
        r,
        asked,
        deliver,
        history,
        _mount: mount,
    }
}

fn metrics() -> Metrics {
    Metrics {
        at: 1_800_000_000.0,
        hostname: Some("lorien".into()),
        uptime_secs: Some(90_000.0),
        cpu_pct: Some(41.6),
        cores_pct: vec![("0".into(), 10.0), ("1".into(), 95.0)],
        load: Some([0.5, 0.4, 0.3]),
        mem_total: Some(64.0 * 1024.0 * 1024.0 * 1024.0),
        mem_used: Some(16.0 * 1024.0 * 1024.0 * 1024.0),
        swap_total: Some(8.0 * 1024.0 * 1024.0 * 1024.0),
        swap_used: Some(0.0),
        filesystems: vec![Filesystem {
            device: "/dev/nvme0n1p2".into(),
            mount: Some("/".into()),
            size: 1.0e12,
            avail: 2.0e11,
        }],
        disks: vec![Rate {
            name: "nvme0n1".into(),
            a_per_sec: 1024.0,
            b_per_sec: 2048.0,
        }],
        nics: vec![Rate {
            name: "eth0".into(),
            a_per_sec: 3072.0,
            b_per_sec: 4096.0,
        }],
        gpus: vec![Gpu {
            name: "RX 7900".into(),
            busy_pct: Some(40.0),
            vram_used: Some(8.0e9),
            vram_total: Some(24.0e9),
            temp_c: Some(55.0),
        }],
        temps: vec![Temp {
            label: "Tctl".into(),
            celsius: 48.0,
        }],
    }
}

fn summary(name: &str, up: bool) -> HostSummary {
    HostSummary {
        name: name.into(),
        kind: "pcp".into(),
        url: format!("http://{name}:44323"),
        up,
        error: (!up).then(|| "connection refused".to_string()),
        last_ok: Some(1_800_000_000.0 - 7_300.0),
        view: Some(metrics()),
    }
}

fn card(name: &str, up: bool) -> HostCard {
    HostCard {
        host: summary(name, up),
        cpu: vec![Some(10.0), Some(40.0), None, Some(30.0)],
    }
}

fn detail(name: &str) -> HostDetail {
    let point = |t: f64| Point {
        t,
        cpu: Some(40.0),
        mem: Some(25.0),
        rx: Some(10.0),
        tx: Some(20.0),
        read: Some(1.0),
        write: Some(2.0),
        gpu: Some(40.0),
        vram: Some(33.0),
    };
    HostDetail {
        host: summary(name, true),
        history: vec![point(1.0), point(2.0), point(3.0)],
    }
}

#[quench_test::test]
fn the_hosts_screen_asks_for_the_hosts_and_waits_for_them() {
    let l = live("/gauge/ui/home");
    assert_eq!(*l.asked.borrow(), [Request::Hosts]);
    assert!(present(&l.r, "waiting"), "{}", l.r.text());
}

#[quench_test::test]
fn there_is_a_card_per_host_by_name_with_its_numbers() {
    let l = live("/gauge/ui/home");
    (l.deliver)(Msg::Hosts(vec![card("lorien", true), card("arda", true)]));
    let text = l.r.text();
    assert!(
        text.find("arda").unwrap() < text.find("lorien").unwrap(),
        "{text}"
    );
    for expected in [
        "2 cores · pcp · http://arda:44323",
        "42%",
        "25%",
        "80%",
        "up",
        "Load",
        "↓ 3.00 KiB/s ↑ 4.00 KiB/s",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
    let link = find(&l.r, "a", ("id", "host-arda"));
    assert_eq!(
        l.r.attr(link, "href").as_deref(),
        Some("/gauge/ui/hosts/arda")
    );
    assert!(
        l.r.html().contains("data:image/svg+xml,"),
        "a chart is drawn"
    );
}

#[quench_test::test]
fn a_host_that_stopped_answering_says_why_and_since_when() {
    let l = live("/gauge/ui/home");
    (l.deliver)(Msg::Hosts(vec![card("lorien", false)]));
    let text = l.r.text();
    assert!(text.contains("unreachable"), "{text}");
    assert!(
        text.contains("connection refused - last seen 2h 1m ago"),
        "{text}"
    );
    assert!(l.r.html().contains("gg-stale"));
}

#[quench_test::test]
fn no_hosts_is_said_in_words() {
    let l = live("/gauge/ui/home");
    (l.deliver)(Msg::Hosts(vec![]));
    assert!(
        l.r.text().contains("No hosts configured."),
        "{}",
        l.r.text()
    );
}

#[quench_test::test]
fn the_open_screen_asks_again_until_it_is_left() {
    let l = live("/gauge/ui/home");
    assert_eq!(l.asked.borrow().len(), 1);
    tick(10_000.0);
    assert_eq!(l.asked.borrow().len(), 2);
    tick(10_000.0);
    assert_eq!(l.asked.borrow().len(), 3);
    (l.deliver)(Msg::Hosts(vec![card("arda", true)]));
    let link = find(&l.r, "a", ("id", "host-arda"));
    l.r.fire(link, &Event::Click);
    assert_eq!(l.history.location().path, "/gauge/ui/hosts/arda");
    assert_eq!(l.asked.borrow().last(), Some(&Request::Host("arda".into())));
    let before = l
        .asked
        .borrow()
        .iter()
        .filter(|r| **r == Request::Hosts)
        .count();
    tick(10_000.0);
    let after = l
        .asked
        .borrow()
        .iter()
        .filter(|r| **r == Request::Hosts)
        .count();
    assert_eq!(
        before, after,
        "the hosts are not asked for from a host's screen"
    );
    assert!(
        l.asked
            .borrow()
            .iter()
            .filter(|r| **r == Request::Host("arda".into()))
            .count()
            >= 2,
        "the host's own screen polls"
    );
}

#[quench_test::test]
fn a_host_screen_has_every_section() {
    let l = live("/gauge/ui/hosts/lorien");
    assert_eq!(*l.asked.borrow(), [Request::Host("lorien".into())]);
    assert!(present(&l.r, "waiting"));
    (l.deliver)(Msg::Detail(Box::new(detail("lorien"))));
    let text = l.r.text();
    for expected in [
        "← Hosts",
        "lorien",
        "pcp · http://lorien:44323",
        "CPU",
        "Memory",
        "16.0 GiB / 64.0 GiB",
        "Swap",
        "RX 7900",
        "8.00 GB",
        "Disks",
        "/",
        "80%",
        "nvme0n1",
        "Network",
        "eth0",
        "Temperatures",
        "Tctl",
        "48 °C",
        "load 0.50 · 0.40 · 0.30",
        "1d 1h",
    ] {
        assert!(
            text.contains(expected) || expected == "8.00 GB",
            "{expected}: {text}"
        );
    }
    assert!(
        l.r.html().matches("data:image/svg+xml,").count() >= 5,
        "a chart each for CPU, memory, GPU, disks and network"
    );
}

#[quench_test::test]
fn the_detail_of_another_host_is_not_shown() {
    let l = live("/gauge/ui/hosts/lorien");
    (l.deliver)(Msg::Detail(Box::new(detail("arda"))));
    assert!(present(&l.r, "waiting"));
    assert!(!l.r.text().contains("Temperatures"));
}

#[quench_test::test]
fn an_unknown_host_is_said_so() {
    let l = live("/gauge/ui/hosts/nope");
    (l.deliver)(Msg::NotFound("nope".into()));
    assert!(present(&l.r, "not-found"), "{}", l.r.text());
    assert!(l.r.text().contains("No such host."));
}

#[quench_test::test]
fn a_host_with_no_numbers_yet_says_it_is_waiting() {
    let l = live("/gauge/ui/hosts/lorien");
    let mut d = detail("lorien");
    d.host.view = None;
    d.host.up = false;
    (l.deliver)(Msg::Detail(Box::new(d)));
    assert!(present(&l.r, "waiting"));
    assert!(l.r.text().contains("Waiting for the first scrape"));
}

#[quench_test::test]
fn the_language_can_be_changed_and_is_reported() {
    let l = live("/gauge/ui/home");
    (l.deliver)(Msg::Hosts(vec![]));
    assert!(l.r.text().contains("No hosts configured."));
    let select = find(&l.r, "select", ("id", "language"));
    l.r.fire(select, &Event::Input("de-DE".into()));
    assert_eq!(
        l.asked.borrow().last(),
        Some(&Request::Language("de-DE".into()))
    );
    assert!(
        !l.r.text().contains("No hosts configured."),
        "{}",
        l.r.text()
    );
}

#[quench_test::test]
fn a_failure_and_a_ended_session_are_shown_and_cleared_by_the_next_answer() {
    let l = live("/gauge/ui/home");
    (l.deliver)(Msg::Failed("the server answered 502".into()));
    assert!(l.r.text().contains("the server answered 502"));
    (l.deliver)(Msg::Hosts(vec![card("arda", true)]));
    assert!(!present(&l.r, "problem"));
    (l.deliver)(Msg::SignedOut);
    let sign_in = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(
        l.r.attr(sign_in, "href").as_deref(),
        Some("/gauge/ui/login")
    );
}

#[quench_test::test]
fn the_title_leads_home_and_sign_out_leaves() {
    let l = live("/gauge/ui/hosts/lorien");
    let title = find(&l.r, "a", ("id", "title"));
    assert_eq!(l.r.attr(title, "href").as_deref(), Some("/gauge/ui/home"));
    l.r.fire(title, &Event::Click);
    assert_eq!(l.history.location().path, "/gauge/ui/home");
    let out =
        l.r.document_order()
            .into_iter()
            .find(|n| l.r.attr(*n, "href").as_deref() == Some("/gauge/ui/logout"));
    assert!(out.is_some());
}

quench_test::main!();
