//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;
use workbench_app::{Msg, Request, Setup, Workbench, workbench};
use workbench_core::{
    Board, Comment, CreateProject, Issue, IssueView, LinkedIssue, Project, RelatedIssues, User,
};

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
}

fn live(path: &str) -> Live {
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(path));
    let Workbench { view, deliver } = workbench(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/workbench".into(),
            language: "en-US".into(),
            login_url: "/workbench/ui/login".into(),
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

fn project(id: &str, key: &str) -> Project {
    Project {
        id: id.into(),
        key: key.into(),
        name: format!("Project {key}"),
        description: Some(format!("About {key}")),
        created_at: "2026-10-29T10:00:00Z".into(),
        updated_at: "2026-10-29T10:00:00Z".into(),
    }
}

fn issue(id: &str, seq: i32, title: &str, status: &str) -> Issue {
    Issue {
        id: id.into(),
        project_id: "p1".into(),
        parent_id: None,
        seq,
        kind: "task".into(),
        title: title.into(),
        description: Some("Some words".into()),
        status: status.into(),
        priority: "high".into(),
        assignee: None,
        reporter: "dev".into(),
        estimate: Some(3),
        created_at: "2026-10-29T10:00:00Z".into(),
        updated_at: "2026-10-29T10:00:00Z".into(),
    }
}

fn board() -> Board {
    Board {
        project: project("p1", "WB"),
        issues: vec![
            issue("i1", 1, "First", "todo"),
            issue("i2", 2, "Second", "done"),
            issue("i3", 3, "Third", "todo"),
        ],
    }
}

fn view() -> IssueView {
    IssueView {
        issue: issue("i1", 1, "First", "todo"),
        project: project("p1", "WB"),
        comments: vec![Comment {
            id: "c1".into(),
            issue_id: "i1".into(),
            author: "ola".into(),
            body: "Looks fine".into(),
            created_at: "2026-10-29T11:30:00Z".into(),
        }],
        links: RelatedIssues {
            blocks: vec![LinkedIssue {
                link_id: "l1".into(),
                issue_id: "i2".into(),
                project_key: "WB".into(),
                seq: 2,
                title: "Second".into(),
                status: "done".into(),
            }],
            blocked_by: vec![],
            relates_to: vec![],
        },
    }
}

fn start(l: &Live) {
    l.send(Msg::Me("ola".into()));
    l.send(Msg::Users(vec![
        User {
            username: "ola".into(),
            display_name: None,
        },
        User {
            username: "bo".into(),
            display_name: Some("Bo B.".into()),
        },
    ]));
    l.send(Msg::Projects(vec![
        project("p1", "WB"),
        project("p2", "OT"),
    ]));
}

#[quench_test::test]
fn the_app_starts_by_asking_who_and_which_projects() {
    let l = live("/workbench/ui/home");
    assert_eq!(l.asked()[..2], [Request::Startup, Request::Projects]);
    assert!(present(&l.r, "waiting"));
}

#[quench_test::test]
fn home_lists_the_projects_and_the_sidebar_too() {
    let l = live("/workbench/ui/home");
    start(&l);
    let text = l.text();
    assert!(
        text.contains("WB — Project WB") && text.contains("About OT"),
        "{text}"
    );
    let card = find(&l.r, "a", ("data-project", "WB"));
    assert_eq!(
        l.r.attr(card, "href").as_deref(),
        Some("/workbench/ui/projects/p1/board")
    );
    l.send(Msg::Projects(vec![]));
    assert!(present(&l.r, "no-projects"));
}

#[quench_test::test]
fn a_new_project_is_checked_then_asked_for_and_opened() {
    let l = live("/workbench/ui/home");
    start(&l);
    assert!(!present(&l.r, "new-project"));
    l.click("button", "new-project-button");
    l.submit("new-project");
    assert!(l.text().contains("a key and a name are required"));
    assert_eq!(l.asked().len(), 2, "nothing was sent");

    l.type_into("input", "project-key", "NP");
    l.type_into("input", "project-name", "New one");
    l.submit("new-project");
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::CreateProject(CreateProject {
            key: "NP".into(),
            name: "New one".into(),
            description: None
        })
    );

    l.send(Msg::ProjectCreated(Box::new(project("p3", "NP"))));
    assert_eq!(l.history.location().path, "/workbench/ui/projects/p3/board");
    assert_eq!(l.asked().last().unwrap(), &Request::Board("p3".into()));
    let sidebar = find(&l.r, "a", ("data-project", "NP"));
    assert!(
        l.r.attr(sidebar, "class")
            .unwrap()
            .contains("fui-nav-subitem")
    );
}

#[quench_test::test]
fn the_board_has_a_column_per_status() {
    let l = live("/workbench/ui/projects/p1/board");
    start(&l);
    assert!(l.asked().contains(&Request::Board("p1".into())));
    l.send(Msg::Board(Box::new(board())));
    for status in ["blocked", "todo", "in-progress", "done", "rejected"] {
        find(&l.r, "div", ("data-status", status));
    }
    let todo = find(&l.r, "div", ("data-status", "todo"));
    let text = l.r.text_of(todo);
    assert!(
        text.contains("First") && text.contains("Third") && !text.contains("Second"),
        "{text}"
    );
    assert!(text.contains("3 pt") && text.contains("#1"), "{text}");
}

#[quench_test::test]
fn cards_can_be_dragged_to_a_column_through_their_select() {
    let l = live("/workbench/ui/projects/p1/board");
    start(&l);
    l.send(Msg::Board(Box::new(board())));
    let card = find(&l.r, "div", ("data-issue", "i1"));
    assert_eq!(l.r.attr(card, "draggable").as_deref(), Some("true"));
    assert_eq!(l.r.attr(card, "data-drag-id").as_deref(), Some("i1"));
    let select = l.r.attr(card, "data-drag-select").unwrap();
    assert!(present(&l.r, &select), "the select the drop sets is there");
    let column = find(&l.r, "div", ("data-drop-value", "done"));
    assert_eq!(l.r.attr(column, "data-status").as_deref(), Some("done"));
}

#[quench_test::test]
fn moving_a_card_shows_at_once_and_a_refusal_puts_it_back() {
    let l = live("/workbench/ui/projects/p1/board");
    start(&l);
    l.send(Msg::Board(Box::new(board())));
    l.type_into("select", "status-i1", "in-progress");
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::Move {
            issue: "i1".into(),
            status: "in-progress".into()
        }
    );
    let doing = find(&l.r, "div", ("data-status", "in-progress"));
    assert!(l.r.text_of(doing).contains("First"));

    l.send(Msg::Rejected("no write access here".into()));
    assert!(l.text().contains("no write access here"));
    assert_eq!(l.asked().last().unwrap(), &Request::Board("p1".into()));
    l.send(Msg::Board(Box::new(board())));
    let todo = find(&l.r, "div", ("data-status", "todo"));
    assert!(l.r.text_of(todo).contains("First"));
}

#[quench_test::test]
fn a_new_issue_is_checked_sent_and_added() {
    let l = live("/workbench/ui/projects/p1/board");
    start(&l);
    l.send(Msg::Board(Box::new(board())));
    l.click("button", "new-issue-button");
    l.submit("new-issue");
    assert!(l.text().contains("a title is required"));

    l.type_into("input", "issue-title", "  Brand new ");
    l.type_into("select", "issue-priority", "low");
    l.type_into("select", "issue-assignee", "bo");
    l.submit("new-issue");
    let Some(Request::CreateIssue { project, body }) = l.asked().last().cloned() else {
        panic!("{:?}", l.asked());
    };
    assert_eq!(project, "p1");
    assert_eq!(
        (
            body.title.as_str(),
            body.priority.as_str(),
            body.assignee.as_deref()
        ),
        ("Brand new", "low", Some("bo"))
    );
    assert!(!present(&l.r, "new-issue"), "the form closes");

    l.send(Msg::IssueCreated(Box::new(issue(
        "i4",
        4,
        "Brand new",
        "todo",
    ))));
    let todo = find(&l.r, "div", ("data-status", "todo"));
    assert!(l.r.text_of(todo).contains("Brand new"));
}

#[quench_test::test]
fn the_issue_page_shows_its_details_links_and_comments() {
    let l = live("/workbench/ui/issues/i1");
    start(&l);
    assert!(l.asked().contains(&Request::Issue("i1".into())));
    l.send(Msg::Issue(Box::new(view())));
    let text = l.text();
    assert!(
        text.contains("WB-1") && text.contains("Looks fine"),
        "{text}"
    );
    assert!(
        text.contains("WB-2 — Second") && text.contains("2026-10-29 11:30"),
        "{text}"
    );
    let title = find(&l.r, "input", ("id", "title"));
    assert_eq!(l.r.attr(title, "value").as_deref(), Some("First"));
    let blocks = find(&l.r, "div", ("data-section", "ui_issue_blocks"));
    assert!(l.r.text_of(blocks).contains("Second"));
    let by = find(&l.r, "div", ("data-section", "ui_issue_blocked_by"));
    assert!(l.r.text_of(by).contains("None"));
}

#[quench_test::test]
fn saving_sends_every_field_and_a_changed_status() {
    let l = live("/workbench/ui/issues/i1");
    start(&l);
    l.send(Msg::Issue(Box::new(view())));

    l.type_into("input", "estimate", "many");
    l.submit("details");
    assert!(l.text().contains("the estimate must be a whole number"));
    assert!(!l.asked().iter().any(|q| matches!(q, Request::Save { .. })));

    l.type_into("input", "estimate", "8");
    l.type_into("input", "title", "First, renamed");
    l.type_into("select", "status", "done");
    l.click("button", "assign-me");
    l.submit("details");
    let Some(Request::Save {
        issue,
        body,
        status,
    }) = l.asked().last().cloned()
    else {
        panic!("{:?}", l.asked());
    };
    assert_eq!(issue, "i1");
    assert_eq!(
        (body.title.as_str(), body.estimate, body.assignee.as_deref()),
        ("First, renamed", Some(8), Some("ola"))
    );
    assert_eq!(status.as_deref(), Some("done"));

    // Unchanged status: no transition.
    l.send(Msg::Issue(Box::new(view())));
    l.submit("details");
    let Some(Request::Save { status, .. }) = l.asked().last().cloned() else {
        panic!()
    };
    assert_eq!(status, None);
}

#[quench_test::test]
fn comments_and_links_are_checked_and_sent() {
    let l = live("/workbench/ui/issues/i1");
    start(&l);
    l.send(Msg::Issue(Box::new(view())));

    l.submit("add-comment");
    assert!(l.text().contains("a comment cannot be empty"));
    l.type_into("textarea", "comment-body", " ship it ");
    l.submit("add-comment");
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::Comment {
            issue: "i1".into(),
            body: "ship it".into()
        }
    );

    l.type_into("input", "link-target", "nonsense");
    l.submit("add-link");
    assert!(l.text().contains("an issue key looks like WB-4"));
    l.type_into("input", "link-target", "WB-4");
    l.type_into("select", "link-kind", "relates_to");
    l.submit("add-link");
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::AddLink {
            issue: "i1".into(),
            key: "WB-4".into(),
            kind: "relates_to".into()
        }
    );

    let row = find(&l.r, "div", ("data-link", "l1"));
    let remove =
        l.r.children(row)
            .into_iter()
            .find(|n| l.r.tag(*n).as_deref() == Some("button"))
            .unwrap();
    l.r.fire(remove, &Event::Click);
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::RemoveLink {
            issue: "i1".into(),
            link: "l1".into()
        }
    );
}

#[quench_test::test]
fn a_refusal_and_a_lost_session_are_said_on_the_screen() {
    let l = live("/workbench/ui/issues/i1");
    start(&l);
    l.send(Msg::Issue(Box::new(view())));
    l.send(Msg::Rejected("no write access to this issue".into()));
    assert!(l.text().contains("no write access to this issue"));
    l.send(Msg::SignedOut);
    let sign_in = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(
        l.r.attr(sign_in, "href").as_deref(),
        Some("/workbench/ui/login")
    );
}

#[quench_test::test]
fn a_missing_issue_or_project_says_so() {
    let l = live("/workbench/ui/issues/nope");
    start(&l);
    l.send(Msg::NotFound);
    assert!(present(&l.r, "not-found"));
    let l = live("/workbench/ui/projects/nope/board");
    start(&l);
    l.send(Msg::NotFound);
    assert!(present(&l.r, "not-found"));
}

#[quench_test::test]
fn the_language_is_chosen_and_remembered_by_the_host() {
    let l = live("/workbench/ui/home");
    start(&l);
    assert!(l.text().contains("Projects"));
    l.type_into("select", "language", "pl-PL");
    assert_eq!(
        l.asked().last().unwrap(),
        &Request::Language("pl-PL".into())
    );
    assert!(
        l.text().contains("Nowy projekt") || l.text().contains("Projekty"),
        "{}",
        l.text()
    );
}

#[quench_test::test]
fn the_sidebar_marks_the_open_project() {
    let l = live("/workbench/ui/projects/p1/board");
    start(&l);
    let open = find(&l.r, "a", ("data-project", "WB"));
    assert!(l.r.attr(open, "class").unwrap().contains("wb-current"));
    let other = find(&l.r, "a", ("data-project", "OT"));
    assert!(!l.r.attr(other, "class").unwrap().contains("wb-current"));
}

quench_test::main!();
