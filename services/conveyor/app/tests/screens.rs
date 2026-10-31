//! The screens with no host: mounted into a test tree, driven by events and the clock, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use conveyor_app::{Conveyor, Follow, Msg, Request, Setup, conveyor};
use conveyor_core::{
    Access, Artifact, ArtifactView, CheckChip, CheckDetail, Credential, Finding, Job, KnownStage,
    Project, Provider, Repo, RepoChecks, Run, RunView, RunsPage, ScanFinding, ScanFindings,
    ScanReport, Status, Subscription, Trigger,
};
use quench_core::time::Time;
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;

type Asked = Rc<RefCell<Vec<Request>>>;

fn at(text: &str) -> Time {
    Time::parse_rfc3339(text).unwrap()
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

thread_local! {
    static CLOCK: RefCell<Option<Rc<quench_ui::platform::ManualPlatform>>> = const { RefCell::new(None) };
}

/// Moves this test's clock on. Every test gets a clock of its own: the platform is per thread and the runner reuses threads.
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

    fn send(&self, m: Msg) {
        (self.deliver)(m);
    }

    fn text(&self) -> String {
        self.r.text()
    }

    fn path(&self) -> String {
        self.history.location().path
    }
}

fn live(url: &str) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(url));
    let Conveyor { view, deliver } = conveyor(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/conveyor".into(),
            language: "en-US".into(),
            login_url: "/conveyor/ui/login".into(),
            history: history.clone(),
            now: Rc::new(|| at("2026-10-29T12:00:00Z")),
            home_refresh_ms: 5_000,
            run_refresh_ms: 2_000,
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

fn project(id: &str, name: &str, parent: Option<&str>) -> Project {
    Project {
        id: id.into(),
        name: name.into(),
        parent_id: parent.map(Into::into),
        created_at: at("2026-10-29T10:00:00Z"),
        updated_at: at("2026-10-29T10:00:00Z"),
    }
}

fn repo(id: &str, name: &str, project_id: &str, enabled: bool) -> Repo {
    Repo {
        id: id.into(),
        provider: Provider::GitHub,
        owner: "lorehaven".into(),
        name: name.into(),
        clone_url: format!("https://example.test/{name}.git"),
        default_branch: "master".into(),
        registered_by: "admin".into(),
        project_id: project_id.into(),
        enabled,
        created_at: at("2026-10-29T10:00:00Z"),
        updated_at: at("2026-10-29T10:00:00Z"),
    }
}

fn run(id: &str, repo_id: &str, status: Status) -> Run {
    Run {
        id: id.into(),
        repo_id: repo_id.into(),
        trigger: Trigger::Push,
        git_ref: "refs/heads/master".into(),
        sha: "1234567890abcdef1234567890abcdef12345678".into(),
        message: Some("Fix the thing".into()),
        delivery_id: None,
        status,
        queued_at: at("2026-10-29T11:30:00Z"),
        started_at: Some(at("2026-10-29T11:31:00Z")),
        finished_at: None,
        claimed_by: None,
        claimed_at: None,
        attempt: 1,
        error: None,
        resumed_from: None,
        stages: None,
    }
}

fn job(id: &str, stage: &str, name: &str, needs: &[&str], status: Status) -> Job {
    Job {
        id: id.into(),
        run_id: "run-1".into(),
        stage: stage.into(),
        name: name.into(),
        needs: needs.iter().map(ToString::to_string).collect(),
        status,
        exit_code: (status == Status::Failed).then_some(101),
        started_at: Some(at("2026-10-29T11:31:00Z")),
        finished_at: Some(at("2026-10-29T11:33:00Z")),
        error: (status == Status::Skipped).then(|| "excluded by a `when` clause".to_string()),
        reused_from_run: None,
    }
}

fn access(write: &[&str]) -> Access {
    Access {
        username: "ola".into(),
        read: vec!["p-forge".into(), "p-quench".into()],
        write: write.iter().map(ToString::to_string).collect(),
    }
}

fn projects() -> Vec<Project> {
    vec![
        project("p-root", "lorehaven", None),
        project("p-forge", "forge", Some("p-root")),
        project("p-quench", "quench", Some("p-root")),
    ]
}

fn repos() -> Vec<Repo> {
    vec![
        repo("r1", "conveyor", "p-forge", true),
        repo("r2", "retired", "p-quench", false),
    ]
}

fn open_home() -> Live {
    let l = live("/conveyor/ui/home");
    l.send(Msg::Projects(projects()));
    l.send(Msg::Repos(repos()));
    l.send(Msg::Recent {
        scope: None,
        runs: vec![run("run-1", "r1", Status::Failed)],
    });
    l
}

// ── Home ─────────────────────────────────────────────────────────────────────

#[quench_test::test]
fn the_front_page_asks_for_its_data_and_shows_runs_and_the_tree() {
    let l = open_home();
    assert_eq!(l.asked()[0], Request::Home { scope: None });
    let text = l.text();
    assert!(text.contains("Pipelines"), "{text}");
    assert!(text.contains("lorehaven/conveyor"), "{text}");
    assert!(text.contains("30m ago"), "{text}");
    assert!(text.contains("1234567"), "{text}");
    // The tree: the repository's project, and the disabled one is marked.
    assert!(
        text.contains("lorehaven") && text.contains("Disabled"),
        "{text}"
    );
    // A disabled repository has no run button; an enabled one has.
    assert!(present(&l.r, "run-r1"));
    assert!(!present(&l.r, "run-r2"));
}

#[quench_test::test]
fn the_runs_are_asked_for_again_every_few_seconds() {
    let l = open_home();
    let before = l.asked().len();
    tick(5_000.0);
    assert!(l.asked().len() > before);
    assert_eq!(l.asked().last(), Some(&Request::Recent { scope: None }));
}

#[quench_test::test]
fn run_now_asks_the_host_and_the_chips_follow_the_scan() {
    let l = open_home();
    l.send(Msg::Chips(vec![RepoChecks {
        repo_id: "r1".into(),
        checks: vec![CheckChip {
            kind: "lint".into(),
            passed: true,
            findings: 2,
            metric: None,
        }],
    }]));
    let chip = find(&l.r, "span", ("data-check", "lint"));
    assert!(l.r.attr(chip, "class").unwrap().contains("chip-warning"));
    assert_eq!(l.r.text_of(chip), "L 2");
    l.click("button", "run-r1");
    assert_eq!(
        l.asked().last(),
        Some(&Request::RunNow {
            repo: "r1".into(),
            scope: None
        })
    );
}

#[quench_test::test]
fn a_project_page_has_a_breadcrumb_and_asks_for_its_scope() {
    let l = live("/conveyor/ui/projects/p-forge");
    assert_eq!(
        l.asked()[0],
        Request::Home {
            scope: Some("p-forge".into())
        }
    );
    l.send(Msg::Projects(projects()));
    l.send(Msg::Repos(repos()));
    l.send(Msg::Recent {
        scope: Some("p-forge".into()),
        runs: vec![],
    });
    let text = l.text();
    assert!(
        text.contains("lorehaven") && text.contains("forge"),
        "{text}"
    );
    assert!(text.contains("Nothing has run yet"), "{text}");
    // Runs for another scope are not this page's.
    l.send(Msg::Recent {
        scope: None,
        runs: vec![run("x", "r1", Status::Success)],
    });
    assert!(l.text().contains("Nothing has run yet"));
}

#[quench_test::test]
fn an_unknown_project_says_so() {
    let l = live("/conveyor/ui/projects/nope");
    l.send(Msg::Projects(projects()));
    assert!(l.text().contains("No such project"), "{}", l.text());
}

#[quench_test::test]
fn moving_between_projects_asks_again() {
    let l = live("/conveyor/ui/projects/p-forge");
    l.send(Msg::Projects(projects()));
    // The breadcrumb's first link goes up to the parent: the same route, another parameter.
    let up =
        l.r.document_order()
            .into_iter()
            .find(|n| {
                l.r.tag(*n).as_deref() == Some("a")
                    && l.r.attr(*n, "href").as_deref() == Some("/conveyor/ui/projects/p-root")
            })
            .unwrap();
    l.r.fire(up, &Event::Click);
    let asked = l.asked();
    assert!(
        asked.contains(&Request::Home {
            scope: Some("p-root".into())
        }),
        "{asked:?}"
    );
}

// ── History ──────────────────────────────────────────────────────────────────

#[quench_test::test]
fn the_history_asks_for_the_page_in_the_address_and_pages_through() {
    let l = live("/conveyor/ui/runs?project=p-forge&page=2");
    assert_eq!(
        l.asked()[0],
        Request::History {
            scope: Some("p-forge".into()),
            page: 2
        }
    );
    l.send(Msg::Projects(projects()));
    l.send(Msg::Repos(repos()));
    l.send(Msg::History(RunsPage {
        runs: vec![run("run-1", "r1", Status::Success)],
        page: 2,
        pages: 3,
        total: 60,
    }));
    let text = l.text();
    assert!(text.contains("Page 2 of 3"), "{text}");
    assert!(text.contains("lorehaven/conveyor"), "{text}");
    l.click("a", "next");
    assert_eq!(
        l.path(),
        "/conveyor/ui/runs",
        "{}",
        l.history.location().url()
    );
    assert_eq!(
        l.asked().last(),
        Some(&Request::History {
            scope: Some("p-forge".into()),
            page: 3
        })
    );
}

#[quench_test::test]
fn the_history_and_the_repositories_are_asked_for_again_so_new_runs_appear() {
    let l = live("/conveyor/ui/runs?page=2");
    let before = l.asked().len();
    tick(5_000.0);
    assert!(l.asked().len() > before);
    assert_eq!(
        l.asked().last(),
        Some(&Request::History {
            scope: None,
            page: 2
        })
    );

    let l = live("/conveyor/ui/repos");
    let before = l.asked().len();
    tick(5_000.0);
    assert!(l.asked().len() > before);
    assert_eq!(l.asked().last(), Some(&Request::Repos));
}

#[quench_test::test]
fn the_ends_of_the_pager_do_not_link() {
    let l = live("/conveyor/ui/runs");
    l.send(Msg::History(RunsPage {
        runs: vec![],
        page: 1,
        pages: 1,
        total: 0,
    }));
    for id in ["prev", "next"] {
        let node = find(&l.r, "span", ("id", id));
        assert!(l.r.attr(node, "class").unwrap().contains("disabled"));
    }
    assert!(l.text().contains("Page 1 of 1"));
}

// ── A run ────────────────────────────────────────────────────────────────────

fn run_view(status: Status) -> RunView {
    let mut r = run("run-1", "r1", status);
    r.error = (status == Status::Failed).then(|| "stage build failed".to_string());
    RunView {
        run: r,
        jobs: vec![
            job("j1", "check", "lint", &[], Status::Success),
            job("j2", "build", "cargo", &["check"], Status::Failed),
            job("j3", "security", "scan", &[], Status::Skipped),
        ],
        artifacts: vec![ArtifactView {
            artifact: Artifact {
                id: "a1".into(),
                run_id: "run-1".into(),
                job_id: "j1".into(),
                kind: "image".into(),
                name: "forge/conveyor".into(),
                version: None,
                uri: "http://in-cluster/x".into(),
                digest: Some("sha256:0123456789abcdef0123".into()),
                created_at: at("2026-10-29T11:33:00Z"),
            },
            href: "https://public/x".into(),
        }],
        repo: Some(repo("r1", "conveyor", "p-forge", true)),
    }
}

#[quench_test::test]
fn a_failed_run_shows_why_its_jobs_and_a_restart() {
    let l = live("/conveyor/ui/runs/run-1");
    assert_eq!(l.asked()[0], Request::Run("run-1".into()));
    assert!(l.text().contains("Loading"));
    l.send(Msg::Run(Box::new(run_view(Status::Failed))));
    let text = l.text();
    assert!(text.contains("lorehaven/conveyor"), "{text}");
    assert!(text.contains("stage build failed"), "{text}");
    assert!(
        text.contains("check/lint") && text.contains("build/cargo"),
        "{text}"
    );
    assert!(text.contains("exit 101"), "{text}");
    assert!(text.contains("excluded by a `when` clause"), "{text}");
    assert!(text.contains("sha256:0123456789ab…"), "{text}");
    let link = find(&l.r, "a", ("data-artifact", "a1"));
    assert_eq!(l.r.attr(link, "href").as_deref(), Some("https://public/x"));
    l.click("button", "restart");
    assert_eq!(l.asked().last(), Some(&Request::Restart("run-1".into())));
}

#[quench_test::test]
fn a_run_that_passed_has_no_restart_and_a_missing_one_says_so() {
    let l = live("/conveyor/ui/runs/run-1");
    l.send(Msg::Run(Box::new(run_view(Status::Success))));
    assert!(!present(&l.r, "restart"));
    let gone = live("/conveyor/ui/runs/nope");
    gone.send(Msg::NotFound);
    assert!(gone.text().contains("No such run"), "{}", gone.text());
}

#[quench_test::test]
fn a_run_that_has_not_finished_is_asked_for_again_until_it_rests() {
    let l = live("/conveyor/ui/runs/run-1");
    l.send(Msg::Run(Box::new(run_view(Status::Running))));
    let before = l.asked().len();
    tick(2_000.0);
    assert!(l.asked().len() > before);
    assert_eq!(l.asked().last(), Some(&Request::Run("run-1".into())));
    l.send(Msg::Run(Box::new(run_view(Status::Success))));
    let settled = l.asked().len();
    tick(2_000.0);
    tick(2_000.0);
    assert_eq!(l.asked().len(), settled, "a resting run is left alone");
}

#[quench_test::test]
fn opening_a_job_streams_its_log_once_and_lines_arrive_in_order() {
    let l = live("/conveyor/ui/runs/run-1");
    l.send(Msg::Run(Box::new(run_view(Status::Failed))));
    let summary =
        l.r.document_order()
            .into_iter()
            .find(|n| {
                l.r.tag(*n).as_deref() == Some("summary") && l.r.text_of(*n).contains("build/cargo")
            })
            .unwrap();
    l.r.fire(summary, &Event::Click);
    l.r.fire(summary, &Event::Click);
    let opened: Vec<_> = l
        .asked()
        .into_iter()
        .filter(|q| matches!(q, Request::OpenLog(_)))
        .collect();
    assert_eq!(opened, vec![Request::OpenLog("j2".into())]);
    for (stderr, line) in [(false, "Compiling x"), (true, "error: boom")] {
        l.send(Msg::LogLine {
            job: "j2".into(),
            stderr,
            line: line.into(),
        });
    }
    l.send(Msg::LogLines {
        job: "j2".into(),
        lines: vec![
            (false, "Finished dev".into()),
            (false, "Running tests".into()),
        ],
    });
    let log = find(&l.r, "div", ("id", "log-j2"));
    let text = l.r.text_of(log);
    assert!(
        text.find("Compiling x") < text.find("error: boom"),
        "{text}"
    );
    assert!(
        text.find("error: boom") < text.find("Finished dev")
            && text.find("Finished dev") < text.find("Running tests"),
        "a batch lands after what came before it, in order: {text}"
    );
    let spans = l.r.find_by_attr("class", "cv-log-line cv-log-stderr");
    assert_eq!(spans.len(), 1);
    // Raw and copy are there.
    assert!(present(&l.r, "raw-j2") && present(&l.r, "copy-j2"));
    let copy = find(&l.r, "button", ("id", "copy-j2"));
    assert_eq!(
        l.r.attr(copy, "data-copy-target").as_deref(),
        Some("log-j2")
    );
}

// ── Repositories ─────────────────────────────────────────────────────────────

fn open_repos(write: &[&str]) -> Live {
    let l = live("/conveyor/ui/repos");
    l.send(Msg::Access(access(write)));
    l.send(Msg::Projects(projects()));
    l.send(Msg::Repos(repos()));
    l.send(Msg::Subscriptions(vec![Subscription {
        id: "s1".into(),
        repo_id: Some("r1".into()),
        project_id: None,
        created_at: at("2026-10-29T10:00:00Z"),
    }]));
    l
}

#[quench_test::test]
fn the_list_offers_edit_only_where_writable_and_shows_follows() {
    let l = open_repos(&["p-forge"]);
    assert_eq!(l.asked()[0], Request::Repos);
    assert!(present(&l.r, "edit-r1"));
    assert!(!present(&l.r, "edit-r2"));
    let followed = find(&l.r, "button", ("id", "follow-r1"));
    assert!(l.r.text_of(followed).contains("Following"));
    l.click("button", "follow-r1");
    assert_eq!(
        l.asked().last(),
        Some(&Request::Follow {
            target: Follow::Repo("r1".into()),
            on: false
        })
    );
    l.click("button", "follow-p-forge");
    assert_eq!(
        l.asked().last(),
        Some(&Request::Follow {
            target: Follow::Project("p-forge".into()),
            on: true
        })
    );
}

#[quench_test::test]
fn the_add_form_checks_then_sends_and_is_absent_without_a_writable_project() {
    let none = open_repos(&[]);
    assert!(!present(&none.r, "new-repo"));

    let l = open_repos(&["p-forge"]);
    l.submit("new-repo");
    assert!(
        l.text().contains("Owner and name are required"),
        "{}",
        l.text()
    );
    assert!(
        !l.asked()
            .iter()
            .any(|q| matches!(q, Request::CreateRepo(_)))
    );
    l.type_into("input", "new-owner", "lorehaven");
    l.type_into("input", "new-name", "widget");
    l.type_into("input", "new-clone-url", "https://example.test/w.git");
    l.submit("new-repo");
    let Some(Request::CreateRepo(body)) = l.asked().last().cloned() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(body.owner, "lorehaven");
    assert_eq!(body.project_id, "p-forge");
    assert_eq!(body.provider.as_deref(), Some("github"));
    l.send(Msg::Done("ui_repos_ok_created".into()));
    assert!(l.text().contains("Repository registered"), "{}", l.text());
}

#[quench_test::test]
fn the_edit_screen_saves_and_deletes_and_is_read_only_without_write_access() {
    let l = live("/conveyor/ui/repos/lorehaven/conveyor/edit");
    l.send(Msg::Access(access(&["p-forge"])));
    l.send(Msg::Projects(projects()));
    l.send(Msg::Repos(repos()));
    l.type_into("input", "default_branch", "main");
    l.submit("edit-repo");
    let Some(Request::SaveRepo { id, body }) = l.asked().last().cloned() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(id, "r1");
    assert_eq!(body.default_branch.as_deref(), Some("main"));
    assert_eq!(body.enabled, Some(true));
    l.click("button", "delete-repo");
    assert_eq!(l.asked().last(), Some(&Request::DeleteRepo("r1".into())));
    l.send(Msg::Deleted);
    assert_eq!(l.path(), "/conveyor/ui/repos");
    assert!(l.text().contains("Repository deleted"), "{}", l.text());

    let view = live("/conveyor/ui/repos/lorehaven/retired/edit");
    view.send(Msg::Access(access(&["p-forge"])));
    view.send(Msg::Projects(projects()));
    view.send(Msg::Repos(repos()));
    assert!(present(&view.r, "view-only"));
    assert!(!present(&view.r, "save-repo") && !present(&view.r, "delete-repo"));
    let gone = live("/conveyor/ui/repos/lorehaven/nope/edit");
    gone.send(Msg::Access(access(&[])));
    gone.send(Msg::Projects(projects()));
    gone.send(Msg::Repos(repos()));
    assert!(gone.text().contains("No such repository"));
}

#[quench_test::test]
fn a_saved_rename_moves_to_the_new_address() {
    let l = live("/conveyor/ui/repos/lorehaven/conveyor/edit");
    l.send(Msg::Saved {
        owner: "lorehaven".into(),
        name: "conveyor2".into(),
    });
    assert_eq!(l.path(), "/conveyor/ui/repos/lorehaven/conveyor2/edit");
    assert!(l.text().contains("Changes saved"));
}

// ── Run with parameters ──────────────────────────────────────────────────────

fn open_launch(write: &[&str], stages: Vec<KnownStage>) -> Live {
    let l = live("/conveyor/ui/repos/lorehaven/conveyor/run");
    assert_eq!(
        l.asked()[0],
        Request::Launch {
            owner: "lorehaven".into(),
            name: "conveyor".into()
        }
    );
    l.send(Msg::Access(access(write)));
    l.send(Msg::Repos(repos()));
    l.send(Msg::Stages {
        repo: "r1".into(),
        stages,
    });
    l
}

#[quench_test::test]
fn the_launch_form_starts_with_the_ordinary_stages_ticked_and_sends_the_selection() {
    let l = open_launch(
        &["p-forge"],
        vec![
            KnownStage {
                name: "check".into(),
                on_by_default: true,
            },
            KnownStage {
                name: "security".into(),
                on_by_default: false,
            },
        ],
    );
    assert!(
        l.text().contains("normally left out of a run"),
        "{}",
        l.text()
    );
    l.type_into("input", "git_ref", "main");
    l.type_into("input", "extra", "lint");
    l.click("input", "stage-1");
    l.submit("launch-form");
    let Some(Request::RunStages { repo, body }) = l.asked().last().cloned() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(repo, "r1");
    assert_eq!(body.git_ref.as_deref(), Some("refs/heads/main"));
    assert_eq!(
        body.stages,
        Some(vec!["check".into(), "security".into(), "lint".into()])
    );
    l.send(Msg::Opened("run-9".into()));
    assert_eq!(l.path(), "/conveyor/ui/runs/run-9");
}

#[quench_test::test]
fn nothing_selected_is_refused_and_without_access_the_form_cannot_be_sent() {
    let l = open_launch(&["p-forge"], vec![]);
    assert!(present(&l.r, "none-known"));
    l.submit("launch-form");
    assert!(
        l.text().contains("Select at least one stage"),
        "{}",
        l.text()
    );
    assert!(
        !l.asked()
            .iter()
            .any(|q| matches!(q, Request::RunStages { .. }))
    );

    let no = open_launch(&[], vec![]);
    assert!(present(&no.r, "no-access"));
    let submit = find(&no.r, "button", ("id", "launch-submit"));
    assert!(no.r.attr(submit, "disabled").is_some());
}

// ── Scans ────────────────────────────────────────────────────────────────────

fn report() -> ScanReport {
    ScanReport {
        run: Some(run("run-1", "r1", Status::Success)),
        checks: vec![
            CheckDetail {
                kind: "lint".into(),
                job_name: "check/lint".into(),
                passed: true,
                headline: "1 warning".into(),
                findings: vec![Finding {
                    title: "unused variable".into(),
                    severity: Some("warning".into()),
                    location: Some("src/main.rs:1:2".into()),
                    ..Finding::default()
                }],
                metric: None,
                has_report: false,
                origin: Some(run("run-1", "r1", Status::Success)),
            },
            CheckDetail {
                kind: "security".into(),
                job_name: "security/scan".into(),
                passed: false,
                headline: "2 failed".into(),
                findings: vec![],
                metric: None,
                has_report: true,
                origin: None,
            },
        ],
    }
}

#[quench_test::test]
fn the_overview_shows_the_run_and_a_card_per_check() {
    let l = live("/conveyor/ui/repos/lorehaven/conveyor/scan");
    assert_eq!(
        l.asked()[0],
        Request::Scan {
            owner: "lorehaven".into(),
            name: "conveyor".into()
        }
    );
    l.send(Msg::Repos(repos()));
    l.send(Msg::Scan {
        repo: "r1".into(),
        report: Box::new(report()),
    });
    let text = l.text();
    assert!(text.contains("lorehaven/conveyor"), "{text}");
    assert!(
        text.contains("1 warning") && text.contains("2 failed"),
        "{text}"
    );
    assert!(text.contains("1234567 · push · 30m ago"), "{text}");
    let card = find(&l.r, "a", ("data-check", "lint"));
    assert_eq!(
        l.r.attr(card, "href").as_deref(),
        Some("/conveyor/ui/repos/lorehaven/conveyor/scan/lint")
    );
    assert!(present(&l.r, "launch-link"));
}

#[quench_test::test]
fn a_repository_that_never_ran_says_so() {
    let l = live("/conveyor/ui/repos/lorehaven/conveyor/scan");
    l.send(Msg::Repos(repos()));
    l.send(Msg::Scan {
        repo: "r1".into(),
        report: Box::default(),
    });
    assert!(l.text().contains("has not run yet"), "{}", l.text());
}

#[quench_test::test]
fn a_check_page_lists_its_findings_and_the_report_is_sandboxed() {
    let l = live("/conveyor/ui/repos/lorehaven/conveyor/scan/lint");
    assert!(matches!(l.asked()[0], Request::Check { .. }));
    l.send(Msg::Repos(repos()));
    l.send(Msg::Scan {
        repo: "r1".into(),
        report: Box::new(report()),
    });
    let text = l.text();
    assert!(
        text.contains("unused variable") && text.contains("src/main.rs:1:2"),
        "{text}"
    );
    assert!(text.contains("from check/lint"), "{text}");
    assert!(!present(&l.r, "report"));

    let s = live("/conveyor/ui/repos/lorehaven/conveyor/scan/security");
    s.send(Msg::Repos(repos()));
    s.send(Msg::Scan {
        repo: "r1".into(),
        report: Box::new(report()),
    });
    let frame = find(&s.r, "iframe", ("id", "report-frame"));
    assert_eq!(s.r.attr(frame, "sandbox").as_deref(), Some("allow-scripts"));
    assert_eq!(
        s.r.attr(frame, "src").as_deref(),
        Some("/conveyor/api/v1/repos/r1/scan/security/report")
    );
    assert!(s.text().contains("Nothing found"));
}

#[quench_test::test]
fn the_scanner_findings_show_every_level_until_a_filter_is_applied() {
    let l = live("/conveyor/ui/repos/lorehaven/conveyor/scan/security");
    l.send(Msg::Repos(repos()));
    l.send(Msg::Scan {
        repo: "r1".into(),
        report: Box::new(report()),
    });
    let finding = |severity: &str, service: &str| ScanFinding {
        severity: severity.into(),
        check: "authz".into(),
        service: service.into(),
        method: "GET".into(),
        path: "/x".into(),
        detail: "d".into(),
    };
    l.send(Msg::Findings {
        repo: "r1".into(),
        found: Some(Box::new(ScanFindings {
            findings: vec![finding("high", "alpha"), finding("low", "beta")],
            run_id: "run-1".into(),
            created_at: at("2026-10-29T10:20:00Z"),
        })),
    });
    let text = l.text();
    assert!(
        text.contains("high (1)") && text.contains("low (1)"),
        "{text}"
    );
    assert!(
        text.contains("alpha GET /x") && text.contains("beta GET /x"),
        "{text}"
    );
    assert!(text.contains("2 / 2 - 2026-10-29 10:20 UTC"), "{text}");
    // Untick "low" and apply.
    l.click("input", "level-low");
    assert!(
        l.text().contains("beta GET /x"),
        "ticking alone changes nothing"
    );
    l.submit("finding-filter");
    let text = l.text();
    assert!(
        text.contains("alpha GET /x") && !text.contains("beta GET /x"),
        "{text}"
    );
    assert!(text.contains("1 / 2"), "{text}");
}

// ── Credentials and the shell ────────────────────────────────────────────────

#[quench_test::test]
fn credentials_show_their_scope_and_never_a_token() {
    let l = live("/conveyor/ui/credentials");
    assert_eq!(l.asked()[0], Request::Credentials);
    l.send(Msg::Projects(projects()));
    l.send(Msg::Repos(repos()));
    l.send(Msg::Credentials(vec![
        Credential {
            id: "c1".into(),
            project_id: Some("p-forge".into()),
            repo_id: None,
            name: "deploy".into(),
            kind: "http_token".into(),
            username: "git".into(),
            preview: "••••…9f2a".into(),
            created_by: "admin".into(),
            created_at: at("2026-10-29T11:00:00Z"),
            updated_at: at("2026-10-29T11:00:00Z"),
        },
        Credential {
            id: "c2".into(),
            project_id: None,
            repo_id: Some("r1".into()),
            name: "mirror".into(),
            kind: "http_token".into(),
            username: "git".into(),
            preview: "••••…1111".into(),
            created_by: "admin".into(),
            created_at: at("2026-10-29T11:00:00Z"),
            updated_at: at("2026-10-29T11:00:00Z"),
        },
    ]));
    let text = l.text();
    assert!(text.contains("project: lorehaven/forge"), "{text}");
    assert!(text.contains("repo: lorehaven/conveyor"), "{text}");
    assert!(
        text.contains("••••…9f2a") && text.contains("60m ago"),
        "{text}"
    );
    let empty = live("/conveyor/ui/credentials");
    empty.send(Msg::Credentials(vec![]));
    assert!(empty.text().contains("No credentials visible to you"));
}

#[quench_test::test]
fn the_sidebar_navigates_and_a_lost_session_offers_sign_in() {
    let l = open_home();
    l.click("a", "nav-credentials");
    assert_eq!(l.path(), "/conveyor/ui/credentials");
    l.send(Msg::SignedOut);
    let link = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(
        l.r.attr(link, "href").as_deref(),
        Some("/conveyor/ui/login")
    );
    l.send(Msg::Rejected("no write access".into()));
    l.send(Msg::Failed("cannot reach the service: down".into()));
    assert!(l.text().contains("cannot reach the service"));
}

#[quench_test::test]
fn an_unknown_address_is_not_found() {
    let l = live("/conveyor/ui/nowhere");
    assert!(l.text().contains("There is nothing here"), "{}", l.text());
}

quench_test::main!();
