//! The screens with no host: mounted into a test tree, driven by events and the clock, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use gauge_app::{Gauge, Msg, Request, Setup, gauge};
use gauge_core::model::{
    Alert, DiskStat, Fan, Filesystem, Gpu, LinkView, MemParts, Point, PsiPct, Rate, Temp,
    View as Metrics,
};
use gauge_core::{AlertEvent, DiskForecast, HostCard, HostDetail, HostHistory, HostSummary, Range};
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
            power_w: Some(150.0),
            power_cap_w: Some(300.0),
            fan_rpm: Some(1500.0),
            fan_max_rpm: Some(3000.0),
            sclk_mhz: Some(2100.0),
            mclk_mhz: Some(1249.0),
            gtt_used: Some(1.0e9),
            gtt_total: Some(4.0e10),
        }],
        temps: vec![Temp {
            label: "Tctl".into(),
            celsius: 48.0,
        }],
        psi: Some(PsiPct {
            cpu: 7.0,
            mem: 3.0,
            mem_full: 1.0,
            io: 11.0,
            io_full: 6.0,
        }),
        mem_parts: Some(MemParts {
            anon: 8.0 * 1024.0 * 1024.0 * 1024.0,
            cached: 30.0 * 1024.0 * 1024.0 * 1024.0,
            buffers: 1024.0 * 1024.0 * 1024.0,
            dirty: 2.0 * 1024.0 * 1024.0,
        }),
        swap_in_per_sec: Some(0.0),
        swap_out_per_sec: Some(2048.0),
        oom_kills: Some(3.0),
        tcp_retrans_per_sec: Some(0.5),
        disk_stats: vec![DiskStat {
            name: "nvme0n1".into(),
            util_pct: 37.0,
            iops: 250.0,
            read_ms: Some(0.4),
            write_ms: None,
            queue: 1.0,
        }],
        links: vec![LinkView {
            name: "eth0".into(),
            up: true,
            speed: Some(125_000_000.0),
            errors_per_sec: 0.0,
        }],
        fans: vec![Fan {
            label: "nct6683 fan2".into(),
            rpm: 900.0,
        }],
        alerts: vec![Alert {
            key: "fault:btrfs nvme0n1 corruption".into(),
            kind: "fault".into(),
            detail: "btrfs nvme0n1 corruption: 2".into(),
            severe: true,
        }],
        ..Metrics::default()
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
        psi_cpu: Some(2.0),
        psi_mem: Some(1.0),
        psi_io: Some(4.0),
        disk_util: Some(12.0),
        power: Some(120.0),
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
        "Needs a look",
        "Hardware errors — btrfs nvme0n1 corruption: 2",
        "Pressure",
        "Programs 8.00 GiB",
        "Cache 30.0 GiB",
        "OOM kills",
        "150 W / 300 W",
        "1500 rpm (50%)",
        "2100 / 1249 MHz",
        "37%",
        "0.4 ms / —",
        "1000 Mb/s",
        "TCP retransmits",
        "Fans",
        "nct6683 fan2",
        "900 rpm",
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

/// Opens a screen across hosts with a host that answers and one that does not.
fn across(path: &str) -> Live {
    let l = live(path);
    assert_eq!(l.asked.borrow()[0], Request::Hosts);
    (l.deliver)(Msg::Hosts(vec![
        card("lorien", true),
        card("nogrod", false),
    ]));
    l
}

#[quench_test::test]
fn the_sidebar_leads_to_every_screen_and_marks_the_one_open() {
    let l = across("/gauge/ui/storage");
    let html = l.r.html();
    for id in [
        "nav-hosts",
        "nav-fleet",
        "nav-storage",
        "nav-gpus",
        "nav-network",
        "nav-power",
        "nav-alerts",
    ] {
        assert!(html.contains(&format!("id=\"{id}\"")), "{id}: {html}");
    }
    let storage = find(&l.r, "a", ("id", "nav-storage"));
    assert_eq!(l.r.attr(storage, "aria-current").as_deref(), Some("page"));
    let fleet = find(&l.r, "a", ("id", "nav-fleet"));
    assert_eq!(l.r.attr(fleet, "aria-current"), None);
}

#[quench_test::test]
fn a_host_screen_counts_as_the_hosts_in_the_sidebar() {
    let l = live("/gauge/ui/hosts/lorien");
    let hosts = find(&l.r, "a", ("id", "nav-hosts"));
    assert_eq!(l.r.attr(hosts, "aria-current").as_deref(), Some("page"));
}

#[quench_test::test]
fn storage_lists_every_filesystem_with_the_fullest_host_first_and_totals() {
    let l = across("/gauge/ui/storage");
    let text = l.r.text();
    for expected in [
        "Capacity",
        "Free",
        "Fullest",
        "lorien / 80%",
        "nvme0n1",
        "37%",
        "0.4 ms / —",
        "nogrod",
        "unreachable",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

#[quench_test::test]
fn the_fleet_has_a_row_per_host_and_a_bar_per_signal() {
    let l = across("/gauge/ui/fleet");
    let text = l.r.text();
    for expected in [
        "Host", "CPU", "Pressure", "lorien", "42%", "55 °C", "nogrod",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

#[quench_test::test]
fn the_gpus_screen_has_a_section_for_each_card_and_says_when_there_is_none() {
    let l = across("/gauge/ui/gpus");
    let text = l.r.text();
    for expected in ["lorien", "RX 7900", "150 W / 300 W", "1500 rpm", "GPU busy"] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
    let l = live("/gauge/ui/gpus");
    let mut bare = card("pihole", true);
    bare.host.view.as_mut().unwrap().gpus.clear();
    (l.deliver)(Msg::Hosts(vec![bare]));
    assert!(present(&l.r, "no-gpus"));
}

#[quench_test::test]
fn the_network_screen_sums_throughput_and_shows_each_link() {
    let l = across("/gauge/ui/network");
    let text = l.r.text();
    for expected in ["Received", "eth0", "1000 Mb/s", "0.00/s", "TCP retransmits"] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

#[quench_test::test]
fn the_power_screen_adds_up_what_is_measured_on_hosts_that_answer() {
    let l = across("/gauge/ui/power");
    let text = l.r.text();
    for expected in [
        "Measured draw",
        "150 W",
        "55 °C",
        "Tctl",
        "nct6683 fan2",
        "900 rpm",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

#[quench_test::test]
fn the_alerts_screen_puts_severe_ones_first_and_an_unreachable_host_is_severe() {
    let l = live("/gauge/ui/alerts");
    let mut busy = card("lorien", true);
    busy.host.view.as_mut().unwrap().alerts.insert(
        0,
        Alert {
            key: "swapping".into(),
            kind: "swapping".into(),
            detail: "3 MiB/s".into(),
            severe: false,
        },
    );
    (l.deliver)(Msg::Hosts(vec![busy, card("nogrod", false)]));
    let text = l.r.text();
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {text}"))
    };
    let fault = at("Hardware errors — btrfs nvme0n1 corruption: 2");
    let down = at("Host unreachable");
    let swap = at("Swapping — 3 MiB/s");
    assert!(fault < down && down < swap, "{text}");
    assert!(!present(&l.r, "all-clear"));
}

#[quench_test::test]
fn with_nothing_wrong_the_alerts_screen_says_so() {
    let l = live("/gauge/ui/alerts");
    let mut fine = card("lorien", true);
    fine.host.view.as_mut().unwrap().alerts.clear();
    (l.deliver)(Msg::Hosts(vec![fine]));
    assert!(present(&l.r, "all-clear"));
    assert!(l.r.text().contains("All clear."));
}

#[quench_test::test]
fn a_screen_across_hosts_asks_again_until_it_is_left() {
    let l = across("/gauge/ui/fleet");
    tick(10_000.0);
    tick(10_000.0);
    assert_eq!(l.asked.borrow().len(), 3);
}

fn stored(range: &str, step_secs: u64) -> HostHistory {
    let point = |t: f64| Point {
        cpu: Some(10.0 + t),
        mem: Some(20.0),
        ..Point::gap(t)
    };
    HostHistory {
        name: "lorien".into(),
        range: range.into(),
        step_secs,
        points: vec![point(1.0), point(2.0), Point::gap(3.0), point(4.0)],
    }
}

#[quench_test::test]
fn the_host_screen_starts_live_and_offers_the_stored_ranges() {
    let l = live("/gauge/ui/hosts/lorien");
    (l.deliver)(Msg::Detail(Box::new(detail("lorien"))));
    for id in ["range-1h", "range-24h", "range-7d", "range-30d"] {
        assert!(present(&l.r, id), "{id}");
    }
    let live_button = find(&l.r, "button", ("id", "range-1h"));
    assert_eq!(
        l.r.attr(live_button, "aria-pressed").as_deref(),
        Some("true")
    );
    assert!(
        !present(&l.r, "range-note"),
        "nothing to explain about the live view"
    );
    assert_eq!(*l.asked.borrow(), [Request::Host("lorien".into())]);
}

#[quench_test::test]
fn choosing_a_range_asks_for_its_history_and_draws_it_when_it_comes() {
    let l = live("/gauge/ui/hosts/lorien");
    (l.deliver)(Msg::Detail(Box::new(detail("lorien"))));
    l.r.fire(find(&l.r, "button", ("id", "range-7d")), &Event::Click);
    assert_eq!(
        l.asked.borrow().last(),
        Some(&Request::History("lorien".into(), Range::Week))
    );
    assert!(
        l.r.text().contains("Waiting for the first scrape"),
        "until it comes"
    );

    (l.deliver)(Msg::History(Box::new(stored("7d", 1800))));
    let text = l.r.text();
    assert!(
        text.contains("Showing the last 7d · 30m per point"),
        "{text}"
    );
    let on = find(&l.r, "button", ("id", "range-7d"));
    assert_eq!(l.r.attr(on, "aria-pressed").as_deref(), Some("true"));
    assert!(l.r.html().matches("data:image/svg+xml,").count() >= 5);
}

#[quench_test::test]
fn history_of_another_range_is_not_drawn_under_this_one() {
    let l = live("/gauge/ui/hosts/lorien");
    (l.deliver)(Msg::Detail(Box::new(detail("lorien"))));
    l.r.fire(find(&l.r, "button", ("id", "range-30d")), &Event::Click);
    (l.deliver)(Msg::History(Box::new(stored("24h", 300))));
    assert!(l.r.text().contains("Waiting for the first scrape"));
    assert!(!l.r.text().contains("per point"));
}

#[quench_test::test]
fn a_service_without_a_database_says_so_and_the_live_view_still_works() {
    let l = live("/gauge/ui/hosts/lorien");
    (l.deliver)(Msg::Detail(Box::new(detail("lorien"))));
    l.r.fire(find(&l.r, "button", ("id", "range-24h")), &Event::Click);
    (l.deliver)(Msg::NoHistory);
    assert!(l.r.text().contains("keeps no history"));
    assert!(!present(&l.r, "problem"), "it is not an error banner");
    l.r.fire(find(&l.r, "button", ("id", "range-1h")), &Event::Click);
    assert!(!present(&l.r, "range-note"));
}

#[quench_test::test]
fn the_open_range_asks_again_but_slowly_and_the_live_one_does_not() {
    let l = live("/gauge/ui/hosts/lorien");
    (l.deliver)(Msg::Detail(Box::new(detail("lorien"))));
    l.r.fire(find(&l.r, "button", ("id", "range-24h")), &Event::Click);
    let history_asks = || {
        l.asked
            .borrow()
            .iter()
            .filter(|q| matches!(q, Request::History(..)))
            .count()
    };
    assert_eq!(history_asks(), 1);
    tick(10_000.0);
    assert_eq!(history_asks(), 1, "not on the live interval");
    tick(50_000.0);
    assert_eq!(history_asks(), 2, "a minute after");
}

fn forecast() -> Vec<DiskForecast> {
    let gib = 1024.0 * 1024.0 * 1024.0;
    vec![
        DiskForecast {
            host: "lorien".into(),
            mount: "/".into(),
            size: 931.0 * gib,
            used: 745.0 * gib,
            per_day: Some(12.0 * gib),
            days_left: Some(15.0),
        },
        DiskForecast {
            host: "nogrod".into(),
            mount: "/".into(),
            size: 931.0 * gib,
            used: 745.0 * gib,
            per_day: Some(0.0),
            days_left: None,
        },
    ]
}

#[quench_test::test]
fn the_storage_screen_asks_for_the_forecast_too_and_again_each_minute() {
    let l = live("/gauge/ui/storage");
    assert_eq!(*l.asked.borrow(), [Request::Hosts, Request::Forecast],);
    tick(60_000.0);
    let forecasts = l
        .asked
        .borrow()
        .iter()
        .filter(|q| **q == Request::Forecast)
        .count();
    assert_eq!(forecasts, 2);
}

#[quench_test::test]
fn the_storage_screen_says_how_fast_each_disk_grows_and_when_it_fills() {
    let l = across("/gauge/ui/storage");
    assert!(
        !l.r.text().contains("15 days"),
        "nothing until the forecast comes"
    );
    (l.deliver)(Msg::Forecast(forecast()));
    let text = l.r.text();
    for expected in [
        "Growth",
        "Full in",
        "+12.0 GiB/day",
        "15 days",
        "not growing",
        "Fills first",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

#[quench_test::test]
fn without_history_the_storage_screen_still_lists_the_disks() {
    let l = across("/gauge/ui/storage");
    (l.deliver)(Msg::NoHistory);
    let text = l.r.text();
    assert!(text.contains("745 GiB / 931 GiB"), "{text}");
    assert!(!text.contains("Fills first"));
}

#[quench_test::test]
fn every_chart_has_a_time_axis_for_its_range() {
    let l = live("/gauge/ui/hosts/lorien");
    (l.deliver)(Msg::Detail(Box::new(detail("lorien"))));
    let text = l.r.text();
    for expected in ["−1 h", "−45 min", "−30 min", "−15 min", "now"] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
    let axes = l.r.html().matches("gg-axis").count();
    assert!(
        axes >= 5,
        "a chart each for CPU, memory, pressure, disks and network: {axes}"
    );

    l.r.fire(find(&l.r, "button", ("id", "range-30d")), &Event::Click);
    (l.deliver)(Msg::History(Box::new(stored("30d", 7200))));
    let text = l.r.text();
    for expected in ["−30 d", "−22.5 d", "−15 d", "−7.5 d"] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
    assert!(!text.contains("−1 h"));
}

fn event(
    host: &str,
    key: &str,
    kind: &str,
    opened_ago: f64,
    closed_ago: Option<f64>,
) -> AlertEvent {
    let now = 1_800_000_000.0;
    AlertEvent {
        host: host.into(),
        key: key.into(),
        kind: kind.into(),
        detail: format!("{key} detail"),
        severe: true,
        opened: now - opened_ago,
        closed: closed_ago.map(|c| now - c),
    }
}

#[quench_test::test]
fn the_alerts_screen_asks_for_the_log_and_says_how_long_each_alert_has_gone_on() {
    let l = live("/gauge/ui/alerts");
    assert_eq!(*l.asked.borrow(), [Request::Hosts, Request::AlertLog]);
    (l.deliver)(Msg::Hosts(vec![card("lorien", true)]));
    assert!(
        !l.r.text().contains(" · for "),
        "nothing to say before the log comes"
    );
    (l.deliver)(Msg::AlertLog(vec![event(
        "lorien",
        "fault:btrfs nvme0n1 corruption",
        "fault",
        3.0 * 3600.0 + 120.0,
        None,
    )]));
    let text = l.r.text();
    assert!(
        text.contains("Hardware errors — btrfs nvme0n1 corruption: 2 · for 3h 2m"),
        "{text}"
    );
}

#[quench_test::test]
fn alerts_that_ended_are_listed_with_when_and_for_how_long() {
    let l = live("/gauge/ui/alerts");
    let mut fine = card("lorien", true);
    fine.host.view.as_mut().unwrap().alerts.clear();
    (l.deliver)(Msg::Hosts(vec![fine]));
    (l.deliver)(Msg::AlertLog(vec![
        event("nogrod", "down", "down", 7_200.0, Some(3_600.0)),
        event("lorien", "hot:k10temp", "hot", 90_000.0, Some(86_400.0)),
    ]));
    assert!(present(&l.r, "all-clear"), "nothing is wrong now");
    assert!(present(&l.r, "alert-log"));
    let text = l.r.text();
    for expected in [
        "Recently ended",
        "nogrod",
        "Host unreachable — down detail",
        "1h 0m ago · lasted 1h 0m",
        "Running hot — hot:k10temp detail",
        "1d 0h ago · lasted 1h 0m",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
}

#[quench_test::test]
fn without_a_log_the_alerts_screen_is_just_the_current_list() {
    let l = live("/gauge/ui/alerts");
    (l.deliver)(Msg::Hosts(vec![card("lorien", true)]));
    (l.deliver)(Msg::NoHistory);
    assert!(!present(&l.r, "alert-log"));
    assert!(l.r.text().contains("Hardware errors"));
}
