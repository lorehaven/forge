//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use gantry_app::{Gantry, Msg, Request, Setup, gantry};
use gantry_core::{
    Busy, DeploymentView, Group, Info, Operation, PackageVersion, Plan, ResourceRef, ResourceYaml,
    Row, StoredPlan, Summary, TargetView, Unit,
};
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;

type Asked = Rc<RefCell<Vec<Request>>>;

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

fn count(r: &TestRenderer, attr: &str) -> usize {
    r.document_order()
        .into_iter()
        .filter(|n| r.attr(*n, attr).is_some())
        .count()
}

struct Live {
    r: Rc<TestRenderer>,
    asked: Asked,
    deliver: Rc<dyn Fn(Msg)>,
    history: Rc<MemoryHistory>,
    _mount: quench_ui::Mount<TestRenderer>,
}

impl Live {
    fn click(&self, tag: &str, id: &str) {
        self.r.fire(find(&self.r, tag, ("id", id)), &Event::Click);
    }

    fn type_into(&self, tag: &str, id: &str, text: &str) {
        self.r
            .fire(find(&self.r, tag, ("id", id)), &Event::Input(text.into()));
    }

    fn submit(&self, id: &str) {
        self.r
            .fire(find(&self.r, "form", ("id", id)), &Event::Submit);
    }

    fn asked(&self) -> Vec<Request> {
        self.asked.borrow().clone()
    }

    fn last(&self) -> Request {
        self.asked().last().cloned().unwrap()
    }

    fn send(&self, m: Msg) {
        (self.deliver)(m);
    }

    fn text(&self) -> String {
        self.r.text()
    }
}

fn live(path: &str) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(path));
    let Gantry { view, deliver } = gantry(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/gantry".into(),
            language: "en-US".into(),
            login_url: "/gantry/ui/login".into(),
            history: history.clone(),
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

fn row(kind: &str, name: &str, state: &str, editable: bool) -> Row {
    Row {
        api_version: Some("v1".into()),
        kind: kind.into(),
        name: name.into(),
        namespace: Some("ml".into()),
        state: state.into(),
        ready: (kind == "Deployment" && state != "missing").then(|| "1/1".into()),
        workload: kind == "Deployment",
        editable,
    }
}

fn group(package: &str, sync: &str, can: &[&str]) -> Group {
    let rows = vec![
        row("Deployment", "sage", "synced", false),
        row("ConfigMap", "sage-config", "edited", true),
        row("Deployment", "trainer", "missing", false),
        row("Secret", "sage-secret", "hidden", false),
    ];
    Group {
        package: package.into(),
        description: Some(format!("About {package}")),
        installed: Some("1.0.0".into()),
        offered: Some("1.1.0".into()),
        status: "update_available".into(),
        inventoried: true,
        summary: Summary {
            sync: sync.into(),
            total: rows.len(),
            synced: 2,
            edited: 1,
            missing: 1,
            extra: 0,
            workloads: 2,
            not_ready: 0,
            update_to: Some("1.1.0".into()),
        },
        rows,
        can: can.iter().map(ToString::to_string).collect(),
    }
}

fn plain(package: &str, sync: &str) -> Group {
    Group {
        rows: vec![],
        summary: Summary {
            sync: sync.into(),
            ..Summary::default()
        },
        installed: None,
        offered: None,
        ..group(package, sync, &["read"])
    }
}

fn deployment(name: &str, observed: &str, conflicts: &[&str]) -> DeploymentView {
    DeploymentView {
        name: name.into(),
        declared: true,
        target: "ml".into(),
        description: None,
        observed: observed.into(),
        conflicts_with: conflicts.iter().map(ToString::to_string).collect(),
    }
}

fn operation(id: &str, state: &str) -> Operation {
    Operation {
        id: id.into(),
        kind: "upgrade".into(),
        title: "Upgrade ml → 1.1.0".into(),
        state: state.into(),
        requested_by: "ana".into(),
        error: (state == "failed").then(|| "the rollout timed out".into()),
        log: Some("kept log".into()),
        live_log: Some("runner output".into()),
        created_at: "2026-10-29T10:00:05Z".into(),
        plan: Plan::default(),
        step_lines: vec!["download ml 1.1.0".into(), "install ml 1.1.0".into()],
    }
}

fn everything(l: &Live) {
    l.send(Msg::Info(Info {
        actor: "ana".into(),
        version: "0.1".into(),
        can: vec!["read".into(), "deploy".into()],
    }));
    l.send(Msg::Groups(vec![
        group("ml", "missing", &["read", "deploy", "scale"]),
        plain("media", "synced"),
        plain("forge", "not_installed"),
    ]));
}

#[quench_test::test]
fn the_app_starts_by_asking_who_and_which_packages() {
    let l = live("/gantry/ui/home");
    assert_eq!(l.asked()[..2], [Request::Startup, Request::Packages]);
    assert!(present(&l.r, "waiting"));
}

#[quench_test::test]
fn the_sidebar_has_the_packages_and_the_history_and_marks_the_open_one() {
    let l = live("/gantry/ui/home");
    let packages = find(&l.r, "a", ("id", "nav-packages"));
    assert_eq!(
        l.r.attr(packages, "href").as_deref(),
        Some("/gantry/ui/home")
    );
    assert!(l.r.attr(packages, "class").unwrap().contains("gt-current"));
    let history = find(&l.r, "a", ("id", "nav-history"));
    assert!(!l.r.attr(history, "class").unwrap().contains("gt-current"));
    l.r.fire(history, &Event::Click);
    assert_eq!(l.history.location().path, "/gantry/ui/operations");
    let history = find(&l.r, "a", ("id", "nav-history"));
    assert!(l.r.attr(history, "class").unwrap().contains("gt-current"));
    let packages = find(&l.r, "a", ("id", "nav-packages"));
    assert!(!l.r.attr(packages, "class").unwrap().contains("gt-current"));
}

#[quench_test::test]
fn home_shows_a_card_per_package_those_needing_attention_first() {
    let l = live("/gantry/ui/home");
    everything(&l);
    let order: Vec<String> =
        l.r.document_order()
            .into_iter()
            .filter_map(|n| l.r.attr(n, "data-package"))
            .collect();
    assert_eq!(order, ["forge", "ml", "media"]);
    let card = find(&l.r, "a", ("data-package", "ml"));
    assert_eq!(
        l.r.attr(card, "href").as_deref(),
        Some("/gantry/ui/apps/ml")
    );
    let text = l.r.text_of(card);
    assert!(
        text.contains("1.0.0") && text.contains("→ 1.1.0") && text.contains("About ml"),
        "{text}"
    );
    assert!(text.contains("4 resources"), "{text}");
    assert!(
        text.contains("1 missing") && text.contains("1 edited"),
        "{text}"
    );
    assert!(text.contains("missing"), "{text}");

    l.send(Msg::Groups(vec![]));
    assert!(present(&l.r, "no-packages"));
    assert_eq!(count(&l.r, "data-package"), 0);
}

#[quench_test::test]
fn home_filters_by_sync_state_and_by_name() {
    let l = live("/gantry/ui/home");
    everything(&l);
    // A chip per state that has packages.
    assert!(present(&l.r, "chip-all") && present(&l.r, "chip-synced"));
    assert!(present(&l.r, "chip-missing") && present(&l.r, "chip-not_installed"));
    assert!(!present(&l.r, "chip-unlisted"));

    l.click("button", "chip-not_installed");
    assert_eq!(count(&l.r, "data-package"), 1);
    find(&l.r, "a", ("data-package", "forge"));
    let chip = find(&l.r, "button", ("id", "chip-not_installed"));
    assert!(l.r.attr(chip, "class").unwrap().contains("gt-chip-active"));
    l.click("button", "chip-all");
    assert_eq!(count(&l.r, "data-package"), 3);

    l.type_into("input", "home-q", "MED");
    assert_eq!(count(&l.r, "data-package"), 1);
    find(&l.r, "a", ("data-package", "media"));
    l.type_into("input", "home-q", "nothing");
    assert!(present(&l.r, "no-match"));
    assert_eq!(count(&l.r, "data-package"), 0);
}

#[quench_test::test]
fn home_names_the_change_in_progress() {
    let l = live("/gantry/ui/home");
    everything(&l);
    assert!(!present(&l.r, "busy"));
    l.send(Msg::Busy(Some(Busy {
        id: "o9".into(),
        title: "Upgrade ml".into(),
    })));
    let busy = find(&l.r, "p", ("id", "busy"));
    assert!(l.r.text_of(busy).contains("Upgrade ml …"));
    let link = find(&l.r, "a", ("href", "/gantry/ui/operations/o9"));
    assert!(l.r.text_of(link).contains("Upgrade ml"));
}

#[quench_test::test]
fn a_cluster_that_cannot_be_read_is_said_not_hidden() {
    let l = live("/gantry/ui/home");
    l.send(Msg::Failed("cluster: connection refused".into()));
    assert!(l.text().contains("cluster: connection refused"));
    assert!(!present(&l.r, "waiting") && !present(&l.r, "no-packages"));
    everything(&l);
    assert!(!present(&l.r, "problem"));
}

#[quench_test::test]
fn a_signed_out_visitor_gets_a_way_to_sign_in() {
    let l = live("/gantry/ui/home");
    l.send(Msg::SignedOut);
    let link = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(l.r.attr(link, "href").as_deref(), Some("/gantry/ui/login"));
}

fn open_ml(l: &Live) {
    everything(l);
    l.send(Msg::Deployments(vec![
        deployment("inference", "running", &["training"]),
        deployment("training", "stopped", &[]),
    ]));
}

#[quench_test::test]
fn the_package_page_shows_its_state_its_counts_and_its_resources() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    assert_eq!(l.last(), Request::Package("ml".into()));
    let text = l.text();
    assert!(
        text.contains("ml") && text.contains("1.0.0 → 1.1.0") && text.contains("About ml"),
        "{text}"
    );
    for id in ["count-synced", "count-edited", "count-missing"] {
        assert!(present(&l.r, id), "{id}");
    }
    assert!(!present(&l.r, "count-extra"));
    assert_eq!(count(&l.r, "data-resource"), 4);
    assert!(l.text().contains("4 / 4"));
    let trainer = find(&l.r, "tr", ("data-resource", "Deployment-trainer"));
    assert!(l.r.text_of(trainer).contains("missing"));
    let sage = find(&l.r, "tr", ("data-resource", "Deployment-sage"));
    assert!(l.r.text_of(sage).contains("1/1"));
}

#[quench_test::test]
fn the_buttons_follow_what_the_caller_may_do_and_the_state_of_each_resource() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    // Missing: apply. Present: delete. Edited and editable: edit. A secret: none of the writes.
    assert!(
        present(&l.r, "apply-Deployment-trainer") && !present(&l.r, "delete-Deployment-trainer")
    );
    assert!(present(&l.r, "delete-Deployment-sage") && !present(&l.r, "apply-Deployment-sage"));
    assert!(present(&l.r, "edit-ConfigMap-sage-config") && !present(&l.r, "edit-Deployment-sage"));
    assert!(
        !present(&l.r, "delete-Secret-sage-secret") && !present(&l.r, "apply-Secret-sage-secret")
    );
    let edit = find(&l.r, "a", ("id", "edit-ConfigMap-sage-config"));
    assert_eq!(
        l.r.attr(edit, "href").as_deref(),
        Some(
            "/gantry/ui/resource?package=ml&kind=ConfigMap&name=sage-config&namespace=ml&api_version=v1"
        )
    );
    // The package-wide ones.
    assert!(
        present(&l.r, "upgrade") && present(&l.r, "apply-missing") && present(&l.r, "versions")
    );
    assert!(!present(&l.r, "refresh"));

    // A caller who may only read gets no write buttons.
    l.send(Msg::Groups(vec![group("ml", "missing", &["read"])]));
    for id in [
        "upgrade",
        "apply-missing",
        "apply-Deployment-trainer",
        "delete-Deployment-sage",
        "edit-ConfigMap-sage-config",
        "start-training",
        "stop-inference",
    ] {
        assert!(!present(&l.r, id), "{id}");
    }
    assert!(present(&l.r, "versions"));
}

#[quench_test::test]
fn a_package_without_an_inventory_can_be_refreshed() {
    let l = live("/gantry/ui/apps/ml");
    let mut g = group("ml", "synced", &["read"]);
    g.inventoried = false;
    l.send(Msg::Groups(vec![g]));
    l.click("button", "refresh");
    assert_eq!(l.last(), Request::Refresh("ml".into()));
}

#[quench_test::test]
fn upgrading_asks_first_naming_the_version_and_the_no_cancels() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    let before = l.asked().len();
    l.click("button", "upgrade");
    assert_eq!(l.asked().len(), before, "nothing is asked yet");
    let question = find(&l.r, "span", ("id", "confirm-question"));
    assert_eq!(l.r.text_of(question), "Upgrade ml to 1.1.0?");
    l.click("button", "confirm-no");
    assert!(!present(&l.r, "confirm"));
    assert_eq!(l.asked().len(), before);

    l.click("button", "upgrade");
    l.click("button", "confirm-yes");
    assert_eq!(
        l.last(),
        Request::Sync {
            package: "ml".into(),
            version: Some("1.1.0".into())
        }
    );
    assert!(!present(&l.r, "confirm"));
}

#[quench_test::test]
fn deleting_asks_first_applying_and_applying_missing_do_not() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    l.click("button", "delete-Deployment-sage");
    let question = find(&l.r, "span", ("id", "confirm-question"));
    assert_eq!(l.r.text_of(question), "Delete Deployment/sage?");
    l.click("button", "confirm-yes");
    let Request::DeleteResource(resource) = l.last() else {
        panic!("{:?}", l.last());
    };
    assert_eq!(
        (
            resource.package.as_str(),
            resource.kind.as_str(),
            resource.name.as_str()
        ),
        ("ml", "Deployment", "sage")
    );

    l.click("button", "apply-Deployment-trainer");
    assert!(matches!(l.last(), Request::ApplyResource(r) if r.name == "trainer"));
    l.click("button", "apply-missing");
    assert_eq!(l.last(), Request::ApplyMissing("ml".into()));
}

#[quench_test::test]
fn deployments_are_started_at_once_and_stopped_after_a_question() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    let panel = find(&l.r, "div", ("id", "deployments"));
    let text = l.r.text_of(panel);
    assert!(
        text.contains("inference") && text.contains("⇄ training"),
        "{text}"
    );
    assert!(present(&l.r, "stop-inference") && !present(&l.r, "start-inference"));
    assert!(present(&l.r, "start-training") && !present(&l.r, "stop-training"));

    l.click("button", "start-training");
    assert_eq!(l.last(), Request::Start("training".into()));
    l.click("button", "stop-inference");
    let question = find(&l.r, "span", ("id", "confirm-question"));
    assert_eq!(
        l.r.text_of(question),
        "Stop inference? Its workloads are deleted."
    );
    l.click("button", "confirm-yes");
    assert_eq!(l.last(), Request::Stop("inference".into()));

    // A partly up one can be stopped and started; one not installed has neither.
    l.send(Msg::Deployments(vec![
        deployment("inference", "partial", &[]),
        deployment("training", "absent", &[]),
    ]));
    assert!(present(&l.r, "stop-inference") && present(&l.r, "start-inference"));
    assert!(!present(&l.r, "stop-training") && !present(&l.r, "start-training"));
}

#[quench_test::test]
fn the_resource_filter_narrows_by_name_kind_and_state_and_clears() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    assert!(!present(&l.r, "clear"));
    l.type_into("input", "app-q", "SAGE");
    assert_eq!(count(&l.r, "data-resource"), 3);
    assert!(l.text().contains("3 / 4"));
    l.type_into("select", "app-kind", "Secret");
    assert_eq!(count(&l.r, "data-resource"), 1);
    find(&l.r, "tr", ("data-resource", "Secret-sage-secret"));
    l.type_into("select", "app-kind", "ConfigMap");
    l.type_into("select", "app-state", "missing");
    assert_eq!(count(&l.r, "data-resource"), 0);
    assert!(present(&l.r, "no-match"));
    l.click("button", "clear");
    assert_eq!(count(&l.r, "data-resource"), 4);
    assert!(!present(&l.r, "clear") && !present(&l.r, "no-match"));

    // The counts are the quickest filters.
    l.click("button", "count-edited");
    assert_eq!(count(&l.r, "data-resource"), 1);
    find(&l.r, "tr", ("data-resource", "ConfigMap-sage-config"));
}

#[quench_test::test]
fn an_unknown_package_and_an_empty_one_say_so() {
    let l = live("/gantry/ui/apps/nope");
    everything(&l);
    assert!(present(&l.r, "waiting"));
    l.send(Msg::NotFound);
    assert!(present(&l.r, "not-found"));

    let l = live("/gantry/ui/apps/forge");
    everything(&l);
    assert!(present(&l.r, "no-units"));
}

fn target_view() -> TargetView {
    let unit = |name: &str, ready: i32| Unit {
        kind: "deployment".into(),
        name: name.into(),
        namespace: "ml".into(),
        version: Some("1.0.0".into()),
        desired: 1,
        ready,
    };
    TargetView {
        name: "ml".into(),
        description: Some("Models".into()),
        installed: Some("1.0.0".into()),
        offered: Some("1.1.0".into()),
        status: "update_available".into(),
        units: vec![unit("sage", 1), unit("trainer", 0)],
        versions: vec![
            PackageVersion {
                name: "ml".into(),
                version: "1.1.0".into(),
                description: Some("new".into()),
                yanked: false,
            },
            PackageVersion {
                name: "ml".into(),
                version: "1.0.5".into(),
                description: None,
                yanked: true,
            },
            PackageVersion {
                name: "ml".into(),
                version: "1.0.0".into(),
                description: None,
                yanked: false,
            },
        ],
    }
}

#[quench_test::test]
fn the_target_page_shows_what_runs_what_is_published_and_syncs_to_a_chosen_version() {
    let l = live("/gantry/ui/targets/ml");
    assert_eq!(l.last(), Request::Target("ml".into()));
    l.send(Msg::Target(Box::new(target_view())));
    let line = find(&l.r, "p", ("id", "versions-line"));
    assert_eq!(l.r.text_of(line), "1.0.0 → 1.1.0");
    assert!(l.text().contains("Models") && l.text().contains("update available"));
    find(&l.r, "tr", ("data-unit", "ml/sage"));
    assert!(
        l.r.text_of(find(&l.r, "tr", ("data-unit", "ml/trainer")))
            .contains("0/1")
    );
    let yanked = find(&l.r, "tr", ("data-version", "1.0.5"));
    assert!(l.r.text_of(yanked).contains("yanked"));
    assert!(l.text().contains("Versions (3)"));

    // The newest by default; a yanked version cannot be picked.
    l.submit("sync-form");
    assert_eq!(
        l.last(),
        Request::Sync {
            package: "ml".into(),
            version: None
        }
    );
    l.type_into("select", "gt-version", "1.0.0");
    l.submit("sync-form");
    assert_eq!(
        l.last(),
        Request::Sync {
            package: "ml".into(),
            version: Some("1.0.0".into())
        }
    );
    let options: Vec<(String, bool)> =
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.tag(*n).as_deref() == Some("option"))
            .skip(5) // the languages in the top bar
            .map(|n| {
                (
                    l.r.attr(n, "value").unwrap(),
                    l.r.attr(n, "disabled").is_some(),
                )
            })
            .collect();
    assert_eq!(
        options,
        [
            (String::new(), false),
            ("1.1.0".into(), false),
            ("1.0.5".into(), true),
            ("1.0.0".into(), false)
        ]
    );
}

#[quench_test::test]
fn a_package_with_nothing_running_says_so() {
    let l = live("/gantry/ui/targets/ml");
    let mut view = target_view();
    view.units.clear();
    view.installed = None;
    view.status = "not_installed".into();
    l.send(Msg::Target(Box::new(view)));
    assert!(present(&l.r, "no-units"));
    let line = find(&l.r, "p", ("id", "versions-line"));
    assert_eq!(l.r.text_of(line), "— → 1.1.0");
}

#[quench_test::test]
fn the_history_lists_operations_and_says_when_there_are_none() {
    let l = live("/gantry/ui/operations");
    assert_eq!(l.last(), Request::Operations);
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Operations(vec![]));
    assert!(present(&l.r, "no-operations"));
    l.send(Msg::Operations(vec![
        operation("o2", "running"),
        operation("o1", "succeeded"),
    ]));
    assert!(!present(&l.r, "no-operations"));
    let first = find(&l.r, "tr", ("data-operation", "o2"));
    let text = l.r.text_of(first);
    assert!(
        text.contains("Upgrade ml → 1.1.0") && text.contains("running") && text.contains("ana"),
        "{text}"
    );
    assert!(text.contains("2026-10-29 10:00:05"), "{text}");
    assert!(
        l.r.text_of(find(&l.r, "tr", ("data-operation", "o1")))
            .contains("done")
    );
    find(&l.r, "a", ("href", "/gantry/ui/operations/o2"));
}

#[quench_test::test]
fn a_running_operation_shows_the_live_log_and_can_be_cancelled() {
    let l = live("/gantry/ui/operations/o2");
    assert_eq!(l.last(), Request::Operation("o2".into()));
    l.send(Msg::Info(Info {
        actor: "ana".into(),
        version: String::new(),
        can: vec!["read".into(), "deploy".into()],
    }));
    l.send(Msg::Operation(Box::new(operation("o2", "running"))));
    assert_eq!(
        l.r.text_of(find(&l.r, "pre", ("id", "log"))),
        "runner output"
    );
    assert!(l.text().contains("Steps (2)") && l.text().contains("install ml 1.1.0"));
    l.click("button", "cancel");
    assert_eq!(l.last(), Request::Cancel("o2".into()));

    l.send(Msg::Done("cancelling".into()));
    assert!(
        l.r.text_of(find(&l.r, "p", ("id", "notice")))
            .contains("cancelling")
    );

    // Over: the kept log, the error, no cancel.
    l.send(Msg::Operation(Box::new(operation("o2", "failed"))));
    assert_eq!(l.r.text_of(find(&l.r, "pre", ("id", "log"))), "kept log");
    assert!(
        l.r.text_of(find(&l.r, "p", ("id", "operation-error")))
            .contains("rollout timed out")
    );
    assert!(!present(&l.r, "cancel"));
}

#[quench_test::test]
fn cancelling_needs_deploy_and_an_empty_log_shows_a_dash() {
    let l = live("/gantry/ui/operations/o2");
    l.send(Msg::Info(Info {
        actor: "ana".into(),
        version: String::new(),
        can: vec!["read".into()],
    }));
    let mut op = operation("o2", "queued");
    op.live_log = None;
    l.send(Msg::Operation(Box::new(op)));
    assert!(!present(&l.r, "cancel"));
    assert_eq!(l.r.text_of(find(&l.r, "pre", ("id", "log"))), "—");
}

#[quench_test::test]
fn an_operation_that_was_queued_is_opened() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    l.send(Msg::Started("o7".into()));
    assert_eq!(l.history.location().path, "/gantry/ui/operations/o7");
    assert_eq!(l.last(), Request::Operation("o7".into()));
}

fn stored(operation_id: Option<&str>, can_confirm: bool, deployments: &[&str]) -> StoredPlan {
    StoredPlan {
        id: "p1".into(),
        target: "ml".into(),
        action: "upgrade".into(),
        version: Some("1.1.0".into()),
        plan: Plan {
            title: "Upgrade ml".into(),
            summary: vec!["ml 1.0.0 → 1.1.0".into(), "sage restarts".into()],
            touches: vec!["ml".into()],
            deployments: deployments.iter().map(ToString::to_string).collect(),
        },
        operation_id: operation_id.map(str::to_string),
        step_lines: vec!["download ml 1.1.0".into()],
        can_confirm,
    }
}

#[quench_test::test]
fn a_plan_says_what_would_happen_and_runs_on_confirmation() {
    let l = live("/gantry/ui/plans/p1");
    assert_eq!(l.last(), Request::Plan("p1".into()));
    l.send(Msg::Plan(Box::new(stored(None, true, &[]))));
    assert_eq!(
        l.r.text_of(find(&l.r, "h3", ("id", "plan-action"))),
        "Upgrade"
    );
    let text = l.text();
    assert!(
        text.contains("ml 1.0.0 → 1.1.0")
            && text.contains("sage restarts")
            && text.contains("1.1.0"),
        "{text}"
    );
    assert!(text.contains("Steps (1)") && text.contains("download ml 1.1.0"));
    // A package change goes back to the package.
    find(&l.r, "a", ("href", "/gantry/ui/targets/ml"));
    l.click("button", "confirm-plan");
    assert_eq!(l.last(), Request::Confirm("p1".into()));
}

#[quench_test::test]
fn a_plan_that_cannot_be_run_or_was_run_says_so() {
    let l = live("/gantry/ui/plans/p1");
    l.send(Msg::Plan(Box::new(stored(None, false, &["inference"]))));
    assert!(present(&l.r, "not-allowed") && !present(&l.r, "confirm-plan"));
    // A start or a swap goes back to the packages.
    find(&l.r, "a", ("href", "/gantry/ui/home"));

    l.send(Msg::Plan(Box::new(stored(Some("claiming"), true, &[]))));
    assert!(
        present(&l.r, "confirm-plan"),
        "a claim in progress is not an operation"
    );
    l.send(Msg::Plan(Box::new(stored(Some("o3"), true, &[]))));
    assert!(!present(&l.r, "confirm-plan"));
    let link = find(&l.r, "a", ("id", "confirmed"));
    assert_eq!(
        l.r.attr(link, "href").as_deref(),
        Some("/gantry/ui/operations/o3")
    );
}

fn resource() -> ResourceRef {
    ResourceRef {
        package: "ml".into(),
        api_version: "v1".into(),
        kind: "ConfigMap".into(),
        name: "sage-config".into(),
        namespace: "ml".into(),
    }
}

const ADDRESS: &str =
    "/gantry/ui/resource?package=ml&kind=ConfigMap&name=sage-config&namespace=ml&api_version=v1";

#[quench_test::test]
fn the_editor_asks_for_the_live_yaml_shows_it_and_applies_what_was_typed() {
    let l = live(ADDRESS);
    assert_eq!(l.last(), Request::Yaml(resource()));
    l.send(Msg::Yaml(Box::new(ResourceYaml {
        yaml: "kind: ConfigMap\nLEVEL: info\n".into(),
        editable: true,
    })));
    assert_eq!(
        l.r.text_of(find(&l.r, "h3", ("id", "resource-title"))),
        "ConfigMap/sage-config"
    );
    assert!(l.text().contains("overwrites it"));
    let area = find(&l.r, "textarea", ("id", "yaml"));
    assert_eq!(l.r.attr(area, "readonly"), None);

    l.type_into("textarea", "yaml", "kind: ConfigMap\nLEVEL: debug\n");
    l.submit("edit-form");
    assert_eq!(
        l.last(),
        Request::Edit {
            resource: resource(),
            yaml: "kind: ConfigMap\nLEVEL: debug\n".into()
        }
    );

    // Refused: the reason is shown and the text stays.
    l.send(Msg::Rejected("that describes a different resource".into()));
    assert!(l.text().contains("different resource"));
    assert!(present(&l.r, "save"));
}

#[quench_test::test]
fn blank_yaml_is_not_sent_and_a_reader_gets_a_read_only_editor() {
    let l = live(ADDRESS);
    l.send(Msg::Yaml(Box::new(ResourceYaml {
        yaml: "x: 1\n".into(),
        editable: true,
    })));
    let sent = l.asked().len();
    l.type_into("textarea", "yaml", "  ");
    l.submit("edit-form");
    assert!(l.text().contains("the YAML is empty"));
    assert_eq!(l.asked().len(), sent);

    let l = live(ADDRESS);
    l.send(Msg::Yaml(Box::new(ResourceYaml {
        yaml: "x: 1\n".into(),
        editable: false,
    })));
    let area = find(&l.r, "textarea", ("id", "yaml"));
    assert!(l.r.attr(area, "readonly").is_some());
    assert!(!present(&l.r, "save"));
}

#[quench_test::test]
fn an_address_without_a_resource_is_not_found() {
    let l = live("/gantry/ui/resource?package=ml");
    assert!(present(&l.r, "not-found"));
}

#[quench_test::test]
fn a_refused_change_shows_its_reason_and_the_next_screen_clears_it() {
    let l = live("/gantry/ui/apps/ml");
    open_ml(&l);
    l.send(Msg::Rejected("no scale access to ml".into()));
    assert!(
        l.r.text_of(find(&l.r, "p", ("id", "notice")))
            .contains("no scale access")
    );
    l.r.fire(find(&l.r, "a", ("id", "nav-history")), &Event::Click);
    assert!(!present(&l.r, "notice"));
}

#[quench_test::test]
fn an_unknown_address_is_not_found_with_a_way_home() {
    let l = live("/gantry/ui/nowhere");
    assert!(present(&l.r, "not-found"));
}

#[quench_test::test]
fn a_running_operation_is_asked_for_again_until_it_is_over() {
    let l = live("/gantry/ui/operations/o2");
    let asks = |l: &Live| {
        l.asked()
            .iter()
            .filter(|r| **r == Request::Operation("o2".into()))
            .count()
    };
    assert_eq!(asks(&l), 1);
    l.send(Msg::Operation(Box::new(operation("o2", "running"))));
    tick(3000.0);
    assert_eq!(asks(&l), 2);
    tick(3000.0);
    assert_eq!(asks(&l), 3);

    // Over: no more asking.
    l.send(Msg::Operation(Box::new(operation("o2", "succeeded"))));
    tick(3000.0);
    tick(3000.0);
    assert_eq!(asks(&l), 3);
}

#[quench_test::test]
fn leaving_the_operation_stops_the_asking() {
    let l = live("/gantry/ui/operations/o2");
    l.send(Msg::Operation(Box::new(operation("o2", "running"))));
    l.r.fire(find(&l.r, "a", ("id", "nav-packages")), &Event::Click);
    let before = l.asked().len();
    tick(3000.0);
    tick(3000.0);
    let again = l
        .asked()
        .iter()
        .filter(|r| matches!(r, Request::Operation(_)))
        .count();
    assert_eq!(
        again, 1,
        "no operation is asked for after leaving: {before}"
    );
}

quench_test::main!();
