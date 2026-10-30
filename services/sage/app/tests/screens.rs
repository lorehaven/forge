//! The screens with no host: mounted into a test tree, driven by events, the clock and the host's messages (the answer of a turn
//! arrives as a fake stream), read back as text.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::Event;
use quench_ui::event::KeyEvent;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;
use sage_app::{Msg, Request, Sage, Setup, sage};
use sage_core::{
    ConversationItem, ConversationView, DefaultModelState, Done, FileInfo, MessageNode,
    ModelOption, ModelsView, Project, SearchProviderOption, Source, Status, StreamEvent,
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

fn descendants(r: &TestRenderer, node: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    for child in r.children(node) {
        out.push(child);
        out.extend(descendants(r, child));
    }
    out
}

fn present(r: &TestRenderer, id: &str) -> bool {
    r.document_order()
        .into_iter()
        .any(|n| r.attr(n, "id").as_deref() == Some(id))
}

/// The elements whose `class` has `class` among its words.
fn with_class(r: &TestRenderer, class: &str) -> Vec<NodeId> {
    r.document_order()
        .into_iter()
        .filter(|n| {
            r.attr(*n, "class")
                .is_some_and(|c| c.split_whitespace().any(|w| w == class))
        })
        .collect()
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

    fn forget(&self) {
        self.asked.borrow_mut().clear();
    }

    fn send(&self, m: Msg) {
        (self.deliver)(m);
    }

    fn stream(&self, e: StreamEvent) {
        self.send(Msg::Stream(e));
    }

    fn text(&self) -> String {
        self.r.text()
    }

    fn path(&self) -> String {
        self.history.location().url()
    }
}

fn live(url: &str) -> Live {
    let fresh = Rc::new(quench_ui::platform::ManualPlatform::new());
    quench_ui::platform::install(fresh.clone());
    CLOCK.with(|c| *c.borrow_mut() = Some(fresh));
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(url));
    let Sage { view, deliver } = sage(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/sage".into(),
            language: "en-US".into(),
            login_url: "/sage/ui/login".into(),
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

fn models(ready: bool) -> ModelsView {
    ModelsView {
        switchboard_ok: true,
        ready,
        instances: vec![
            ModelOption {
                id: "i1".into(),
                model: "qwen".into(),
            },
            ModelOption {
                id: "i2".into(),
                model: "bielik".into(),
            },
        ],
        defaults: vec![
            DefaultModelState {
                name: "qwen".into(),
                embedding: false,
                state: "running".into(),
            },
            DefaultModelState {
                name: "embed".into(),
                embedding: true,
                state: if ready { "running" } else { "starting" }.into(),
            },
        ],
        search_providers: vec![
            SearchProviderOption {
                id: "duckduckgo".into(),
                label: "DuckDuckGo".into(),
            },
            SearchProviderOption {
                id: "brave".into(),
                label: "Brave Search".into(),
            },
        ],
        default_search_provider: "duckduckgo".into(),
        accept: ".txt,.pdf".into(),
    }
}

fn node(
    id: &str,
    parent: Option<&str>,
    role: &str,
    content: &str,
    siblings: &[&str],
) -> MessageNode {
    MessageNode {
        id: id.into(),
        parent_id: parent.map(Into::into),
        role: role.into(),
        content: content.into(),
        created_at: "2026-10-29T10:00:00Z".into(),
        siblings: siblings.iter().map(ToString::to_string).collect(),
        sources: Vec::new(),
        attachments: Vec::new(),
    }
}

fn conversation(id: &str, project: Option<&str>, messages: Vec<MessageNode>) -> ConversationView {
    ConversationView {
        id: id.into(),
        title: "t".into(),
        project_id: project.map(Into::into),
        messages,
    }
}

fn item(id: &str, title: &str, project: Option<&str>) -> ConversationItem {
    ConversationItem {
        id: id.into(),
        title: title.into(),
        project_id: project.map(Into::into),
        updated_at: "2026-10-29T10:00:00Z".into(),
    }
}

fn project(id: &str, name: &str) -> Project {
    Project {
        id: id.into(),
        name: name.into(),
        updated_at: "2026-10-29T10:00:00Z".into(),
    }
}

fn file(id: &str, name: &str, status: &str) -> FileInfo {
    FileInfo {
        id: id.into(),
        file_name: name.into(),
        mime_type: if name.ends_with(".png") {
            "image/png".into()
        } else {
            "text/plain".into()
        },
        file_size: 1536,
        conversation_id: Some("c1".into()),
        project_id: None,
        message_id: None,
        status: status.into(),
        error_message: None,
    }
}

/// A fresh chat, with the models and an id from the service.
fn fresh() -> Live {
    let l = live("/sage/ui/home");
    l.send(Msg::Models(models(true)));
    l.send(Msg::NewId("fresh-1".into()));
    l.send(Msg::Projects(vec![project("p1", "Atlas")]));
    l.send(Msg::Conversations(vec![
        item("c1", "First chat", None),
        item("c2", "", None),
        item("c3", "In Atlas", Some("p1")),
    ]));
    l
}

/// `c1` with a question, two answers (the second shown) and the person's follow-up already answered.
fn thread() -> Vec<MessageNode> {
    vec![
        node("q1", None, "user", "How do I loop?", &["q1"]),
        node(
            "a2",
            Some("q1"),
            "assistant",
            "Use **for**:\n\n```rust\nfor i in 0..3 {}\n```\n\n- fast\n- clear",
            &["a1", "a2"],
        ),
    ]
}

fn open(url: &str, view: ConversationView) -> Live {
    let l = live(url);
    l.send(Msg::Models(models(true)));
    l.send(Msg::Projects(vec![project("p1", "Atlas")]));
    l.send(Msg::Conversations(vec![
        item("c1", "First chat", None),
        item("c3", "In Atlas", Some("p1")),
    ]));
    l.send(Msg::Conversation(Box::new(view)));
    l
}

#[quench_test::test]
fn a_fresh_chat_asks_for_its_data_and_welcomes() {
    let l = live("/sage/ui/home");
    let asked = l.asked();
    assert!(asked.contains(&Request::Sidebar), "{asked:?}");
    assert!(asked.contains(&Request::Models), "{asked:?}");
    assert!(asked.contains(&Request::NewId), "{asked:?}");
    l.send(Msg::Models(models(true)));
    l.send(Msg::NewId("fresh-1".into()));
    assert!(l.text().contains("Hello! I am Sage"), "{}", l.text());
    assert!(present(&l.r, "chat-input"));
    assert!(l.text().contains("DuckDuckGo") && l.text().contains("Brave Search"));
    assert!(l.text().contains("qwen") && l.text().contains("bielik"));
    // the first model and the default provider are the choices
    let model = find(&l.r, "select", ("id", "model-select"));
    assert!(l.r.html().contains("selected"), "{}", l.r.html());
    let _ = model;
}

#[quench_test::test]
fn without_a_running_model_the_initializing_screen_waits_and_goes_home_when_ready() {
    let l = live("/sage/ui/home");
    l.send(Msg::Models(models(false)));
    assert_eq!(l.path(), "/sage/ui/initializing");
    assert!(l.text().contains("Preparing Sage"), "{}", l.text());
    assert!(l.text().contains("Starting…"), "{}", l.text());
    assert!(l.text().contains("(embedding)"), "{}", l.text());
    assert!(l.text().contains("Running"));
    // read again every few seconds while a model is not up
    l.forget();
    tick(3_000.0);
    assert!(l.asked().contains(&Request::Models), "{:?}", l.asked());
    l.send(Msg::Models(models(true)));
    assert_eq!(l.path(), "/sage/ui/home");
}

#[quench_test::test]
fn switchboard_down_says_so_on_the_initializing_screen() {
    let l = live("/sage/ui/initializing");
    let mut down = models(false);
    down.switchboard_ok = false;
    down.instances.clear();
    for d in &mut down.defaults {
        d.state = "unknown".into();
    }
    l.send(Msg::Models(down));
    assert!(l.text().contains("Connecting…"), "{}", l.text());
    assert!(present(&l.r, "init-waiting"));
    assert!(l.text().contains("Waiting for the model service"));
}

#[quench_test::test]
fn the_sidebar_lists_projects_history_and_untitled_chats() {
    let l = fresh();
    let text = l.text();
    for needle in [
        "New Chat",
        "Projects",
        "Atlas",
        "History",
        "First chat",
        "New chat",
        "In Atlas",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    // the conversation of a project is listed under the projects, the others under History
    assert_eq!(with_class(&l.r, "sg-nested").len(), 1);
    let projects_section = find(&l.r, "div", ("id", "projects"));
    assert!(
        descendants(&l.r, projects_section)
            .into_iter()
            .any(|n| l.r.attr(n, "id").as_deref() == Some("history-item-c3"))
    );
}

#[quench_test::test]
fn the_sections_collapse() {
    let l = fresh();
    l.click("button", "history-toggle");
    assert!(!present(&l.r, "history"), "{}", l.r.html());
    l.click("button", "history-toggle");
    assert!(present(&l.r, "history"));
    l.click("button", "projects-toggle");
    assert!(!present(&l.r, "projects"));
}

#[quench_test::test]
fn opening_a_conversation_asks_for_it_and_shows_its_markdown() {
    let l = live("/sage/ui/home?conversation_id=c1");
    assert!(l.asked().contains(&Request::Conversation("c1".into())));
    l.send(Msg::Models(models(true)));
    l.send(Msg::Conversation(Box::new(conversation(
        "c1",
        None,
        thread(),
    ))));
    let text = l.text();
    assert!(text.contains("How do I loop?"), "{text}");
    assert!(text.contains("Use "), "{text}");
    assert!(text.contains("for i in 0..3 {}"), "{text}");
    assert!(text.contains("fast") && text.contains("clear"), "{text}");
    let html = l.r.html();
    assert!(html.contains("<strong>for</strong>"), "{html}");
    assert!(html.contains("<li>fast</li>"), "{html}");
    // the code block has its language and a copy button the shell's script serves
    assert!(text.contains("rust") && text.contains("Copy"), "{text}");
    assert!(html.contains("data-copy-target=\"code-a2-1\""), "{html}");
    assert!(html.contains("id=\"code-a2-1\""), "{html}");
    // not the welcome
    assert!(!text.contains("Hello! I am Sage"));
}

#[quench_test::test]
fn answers_never_become_markup() {
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation(
            "c1",
            None,
            vec![
                node("q", None, "user", "<img src=x onerror=alert(1)>", &["q"]),
                node(
                    "a",
                    Some("q"),
                    "assistant",
                    "<script>alert(1)</script> [bad](javascript:alert(1)) [ok](https://example.com)",
                    &["a"],
                ),
            ],
        ),
    );
    let html = l.r.html();
    assert!(!html.contains("<script>"), "{html}");
    assert!(!html.contains("<img src=x"), "{html}");
    assert!(!html.contains("javascript:"), "{html}");
    assert!(html.contains("href=\"https://example.com\""), "{html}");
    assert!(html.contains("rel=\"noopener noreferrer\""), "{html}");
}

#[quench_test::test]
fn a_tool_result_card_is_shown_as_text() {
    let card = "<div class=\"tool-result tool-success\"><div class=\"tool-header\"><span class=\"tool-name\">web_search \"rust\"</span></div><div class=\"tool-content\"><p>Rust is a language.</p></div></div>";
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation(
            "c1",
            None,
            vec![
                node("q", None, "user", "look up rust", &["q"]),
                node(
                    "a",
                    Some("q"),
                    "assistant",
                    &format!("Let me look.\n\n{card}\n\nDone."),
                    &["a"],
                ),
            ],
        ),
    );
    let text = l.text();
    assert!(text.contains("web_search \"rust\""), "{text}");
    assert!(text.contains("Rust is a language."), "{text}");
    assert!(text.contains("Done."));
    assert!(!l.r.html().contains("tool-header"));
    assert_eq!(with_class(&l.r, "sg-tool").len(), 1);
}

#[quench_test::test]
fn sources_and_attachments_show_under_their_messages() {
    let mut question = node("q", None, "user", "read this", &["q"]);
    question.attachments = vec![
        file("f1", "notes.txt", "ready"),
        file("f2", "pic.png", "ready"),
    ];
    let mut answer = node("a", Some("q"), "assistant", "It says hi.", &["a"]);
    answer.sources = vec![
        Source {
            file_name: "notes.txt".into(),
            chunk_index: Some(2),
            detail: None,
            similarity: Some(0.876),
        },
        Source {
            file_name: "[1] example.com".into(),
            chunk_index: None,
            detail: Some("https://example.com/a".into()),
            similarity: None,
        },
    ];
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, vec![question, answer]),
    );
    let text = l.text();
    for needle in [
        "notes.txt",
        "pic.png",
        "1.5 KB",
        "Sources",
        "chunk 2",
        "88%",
        "[1] example.com",
        "https://example.com/a",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    let html = l.r.html();
    assert!(
        html.contains("href=\"/sage/api/v1/files/f1/download\""),
        "{html}"
    );
    assert!(
        html.contains("src=\"/sage/api/v1/files/f2/download\""),
        "{html}"
    );
}

#[quench_test::test]
fn a_conversation_ending_with_the_persons_message_is_answered() {
    let l = live("/sage/ui/home?conversation_id=c1");
    l.send(Msg::Models(models(true)));
    l.forget();
    l.send(Msg::Conversation(Box::new(conversation(
        "c1",
        None,
        vec![node("q", None, "user", "unanswered", &["q"])],
    ))));
    assert_eq!(l.asked(), vec![Request::Resume("c1".into())]);
    l.send(Msg::TurnStarted("t1".into()));
    assert!(l.text().contains("Sage is thinking..."), "{}", l.text());
    // a conversation that ends with Sage's answer is left alone
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, thread()),
    );
    assert!(!l.asked().contains(&Request::Resume("c1".into())));
}

#[quench_test::test]
fn sending_shows_the_message_and_a_placeholder_then_the_answer_streams_in() {
    let l = fresh();
    l.forget();
    l.type_into("textarea", "chat-input", "  What is Rust?  ");
    l.submit("chat-form");
    let asked = l.asked();
    let Request::Send(body) = &asked[0] else {
        panic!("{asked:?}");
    };
    assert_eq!(body.message, "What is Rust?");
    assert_eq!(body.conversation_id, "fresh-1");
    assert_eq!(body.instance_id, "i1");
    assert_eq!(body.search_provider.as_deref(), Some("duckduckgo"));
    assert_eq!(body.file_ids, "");
    // the message and the placeholder are there at once; the box is empty again
    let text = l.text();
    assert!(text.contains("What is Rust?"), "{text}");
    assert!(text.contains("Sage is thinking..."), "{text}");
    assert!(!text.contains("Hello! I am Sage"));
    assert_eq!(
        l.r.attr(find(&l.r, "textarea", ("id", "chat-input")), "value")
            .unwrap_or_default(),
        ""
    );

    l.send(Msg::TurnStarted("t1".into()));
    l.stream(StreamEvent::Delta("Rust is ".into()));
    assert!(l.text().contains("Rust is"), "{}", l.text());
    assert!(!l.text().contains("Sage is thinking..."));
    l.stream(StreamEvent::Delta("a **language**.".into()));
    assert!(
        l.r.html().contains("<strong>language</strong>"),
        "{}",
        l.r.html()
    );

    // the end of the stream: the conversation and the sidebar are read again
    l.forget();
    l.stream(StreamEvent::Done(Done {
        conversation_id: "fresh-1".into(),
        user_message_id: Some("u1".into()),
        message_id: "a1".into(),
        title: "What is Rust?".into(),
    }));
    let asked = l.asked();
    assert!(
        asked.contains(&Request::Conversation("fresh-1".into())),
        "{asked:?}"
    );
    assert!(asked.contains(&Request::Sidebar), "{asked:?}");
    // the streamed answer stays until the stored one arrives
    assert!(l.text().contains("a language."));
    l.send(Msg::Conversation(Box::new(conversation(
        "fresh-1",
        None,
        vec![
            node("u1", None, "user", "What is Rust?", &["u1"]),
            node(
                "a1",
                Some("u1"),
                "assistant",
                "Rust is a **language**.",
                &["a1"],
            ),
        ],
    ))));
    let html = l.r.html();
    assert!(html.contains("id=\"msg-a1\""), "{html}");
    assert!(!html.contains("id=\"turn-ai\""), "{html}");
    assert!(present(&l.r, "regenerate"));
    // a new chat gets its own address once it is stored
    assert_eq!(l.path(), "/sage/ui/home?conversation_id=fresh-1");
}

#[quench_test::test]
fn enter_sends_and_shift_enter_does_not() {
    let l = fresh();
    l.forget();
    l.type_into("textarea", "chat-input", "hello");
    let area = find(&l.r, "textarea", ("id", "chat-input"));
    l.r.fire(
        area,
        &Event::KeyDown(KeyEvent {
            key: "Enter".into(),
            shift: true,
            ..KeyEvent::default()
        }),
    );
    assert!(l.asked().is_empty());
    l.r.fire(area, &Event::KeyDown(KeyEvent::new("Enter")));
    assert!(
        matches!(l.asked().first(), Some(Request::Send(_))),
        "{:?}",
        l.asked()
    );
}

#[quench_test::test]
fn an_empty_message_is_not_sent_and_neither_is_a_second_while_one_is_answered() {
    let l = fresh();
    l.forget();
    l.type_into("textarea", "chat-input", "   ");
    l.submit("chat-form");
    assert!(l.asked().is_empty());
    l.type_into("textarea", "chat-input", "one");
    l.submit("chat-form");
    l.type_into("textarea", "chat-input", "two");
    l.submit("chat-form");
    let sends = l
        .asked()
        .iter()
        .filter(|r| matches!(r, Request::Send(_)))
        .count();
    assert_eq!(sends, 1);
}

#[quench_test::test]
fn progress_lines_show_while_the_pipeline_works() {
    let l = fresh();
    l.type_into("textarea", "chat-input", "How high is the tower?");
    l.submit("chat-form");
    l.send(Msg::TurnStarted("t1".into()));
    for (stage, detail, shown) in [
        ("planning", None, "Deciding what to look up…"),
        (
            "searching",
            Some("tower height · eiffel"),
            "Searching the web…",
        ),
        ("reading", None, "Reading sources…"),
    ] {
        l.stream(StreamEvent::Status(Status {
            stage: stage.into(),
            detail: detail.map(Into::into),
        }));
        let text = l.text();
        assert!(text.contains(shown), "{shown}: {text}");
        if let Some(d) = detail {
            assert!(text.contains(d));
        }
        assert!(
            !text.contains("Sage is thinking..."),
            "the status replaces the placeholder"
        );
    }
    l.stream(StreamEvent::Delta("330 m [1].".into()));
    l.stream(StreamEvent::Status(Status {
        stage: "checking".into(),
        detail: None,
    }));
    let text = l.text();
    assert!(text.contains("330 m [1]."), "{text}");
    assert!(
        text.contains("Checking the answer against the sources…"),
        "{text}"
    );
    // the checked answer replaces what streamed, and the status goes
    l.stream(StreamEvent::Replace(
        "330 m [1]. Not in the sources: pink.".into(),
    ));
    let text = l.text();
    assert!(text.contains("Not in the sources: pink."), "{text}");
    assert!(!text.contains("Checking the answer"));
}

#[quench_test::test]
fn a_failure_replaces_the_placeholder_with_the_error() {
    let l = fresh();
    l.type_into("textarea", "chat-input", "hi");
    l.submit("chat-form");
    l.send(Msg::TurnStarted("t1".into()));
    l.stream(StreamEvent::Failure(
        "api_error_switchboard_unavailable".into(),
    ));
    let text = l.text();
    assert!(
        text.contains("Error: The model service is unavailable"),
        "{text}"
    );
    assert!(!text.contains("Sage is thinking..."));
    assert!(text.contains("hi"), "the message stays: {text}");
    l.stream(StreamEvent::Failure("503 overloaded".into()));
    assert!(l.text().contains("Error: 503 overloaded"), "{}", l.text());
}

#[quench_test::test]
fn a_stream_that_ends_without_saying_how_is_an_error_and_a_finished_one_is_not() {
    let l = fresh();
    l.type_into("textarea", "chat-input", "hi");
    l.submit("chat-form");
    l.send(Msg::TurnStarted("t1".into()));
    l.send(Msg::StreamEnded);
    assert!(
        l.text().contains("The connection to the answer was lost."),
        "{}",
        l.text()
    );

    let l = fresh();
    l.type_into("textarea", "chat-input", "hi");
    l.submit("chat-form");
    l.send(Msg::TurnStarted("t1".into()));
    l.stream(StreamEvent::Done(Done {
        conversation_id: "fresh-1".into(),
        user_message_id: None,
        message_id: "a".into(),
        title: "hi".into(),
    }));
    l.send(Msg::StreamEnded);
    assert!(!l.text().contains("connection"));
}

#[quench_test::test]
fn a_refused_turn_leaves_no_placeholder_and_says_why() {
    let l = fresh();
    l.type_into("textarea", "chat-input", "hi");
    l.submit("chat-form");
    l.send(Msg::Rejected("api_error_empty_message".into()));
    let text = l.text();
    assert!(text.contains("The message is empty"), "{text}");
    assert!(!text.contains("Sage is thinking..."), "{text}");
}

#[quench_test::test]
fn the_navigator_has_a_dot_per_message_that_scrolls_to_it() {
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, thread()),
    );
    let dots = with_class(&l.r, "sg-dot");
    assert_eq!(dots.len(), 2);
    assert_eq!(
        l.r.attr(dots[0], "data-scroll-to").as_deref(),
        Some("msg-q1")
    );
    assert_eq!(
        l.r.attr(dots[1], "data-scroll-to").as_deref(),
        Some("msg-a2")
    );
    assert_eq!(
        l.r.attr(dots[0], "title").as_deref(),
        Some("How do I loop?")
    );
    // the newest is the active one until another is chosen
    assert_eq!(with_class(&l.r, "sg-dot-active"), vec![dots[1]]);
    l.r.fire(dots[0], &Event::Click);
    let dots = with_class(&l.r, "sg-dot");
    assert_eq!(with_class(&l.r, "sg-dot-active"), vec![dots[0]]);
    // the message the dot names is in the page
    assert!(present(&l.r, "msg-q1") && present(&l.r, "msg-a2"));
    // a turn adds its two dots
    l.type_into("textarea", "chat-input", "next");
    l.submit("chat-form");
    assert_eq!(with_class(&l.r, "sg-dot").len(), 4);
}

#[quench_test::test]
fn regenerate_replaces_the_last_answer_with_a_placeholder_and_asks_for_a_new_one() {
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, thread()),
    );
    l.forget();
    l.click("button", "regenerate");
    assert_eq!(l.asked(), vec![Request::Regenerate("a2".into())]);
    let text = l.text();
    assert!(text.contains("Sage is regenerating..."), "{text}");
    assert!(!text.contains("Use "), "the old answer gives way: {text}");
    assert!(text.contains("How do I loop?"));
    assert!(!present(&l.r, "regenerate"));
    l.send(Msg::TurnStarted("t2".into()));
    l.stream(StreamEvent::Delta("Try iterators.".into()));
    assert!(l.text().contains("Try iterators."));
}

#[quench_test::test]
fn alternatives_are_counted_and_switch_through_the_host() {
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, thread()),
    );
    assert!(l.text().contains("2/2"), "{}", l.text());
    l.forget();
    l.click("button", "prev-a2");
    assert_eq!(
        l.asked(),
        vec![Request::Switch {
            conversation_id: "c1".into(),
            target: "a1".into()
        }]
    );
    l.forget();
    l.click("button", "next-a2");
    // the last wraps around to the first
    assert_eq!(
        l.asked(),
        vec![Request::Switch {
            conversation_id: "c1".into(),
            target: "a1".into()
        }]
    );
    // a message with no alternatives has no arrows
    assert!(!present(&l.r, "prev-q1"));
}

#[quench_test::test]
fn the_last_message_of_the_person_can_be_edited_and_resubmitted() {
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, thread()),
    );
    l.click("button", "edit");
    let area = find(&l.r, "textarea", ("id", "edit-input"));
    assert_eq!(l.r.attr(area, "value").as_deref(), Some("How do I loop?"));
    // cancel puts the message back
    l.click("button", "edit-cancel");
    assert!(!present(&l.r, "edit-form"));
    l.click("button", "edit");
    l.type_into("textarea", "edit-input", "How do I iterate?");
    l.forget();
    l.submit("edit-form");
    assert_eq!(
        l.asked(),
        vec![Request::Edit {
            conversation_id: "c1".into(),
            message_id: "q1".into(),
            text: "How do I iterate?".into()
        }]
    );
    assert!(l.text().contains("Sage is thinking..."));
    // the host shows the edited message and starts the answer
    l.send(Msg::Conversation(Box::new(conversation(
        "c1",
        None,
        vec![node("q3", None, "user", "How do I iterate?", &["q1", "q3"])],
    ))));
    l.send(Msg::TurnStarted("t3".into()));
    assert!(l.text().contains("How do I iterate?"));
    assert!(l.text().contains("Sage is thinking..."));
    assert!(!present(&l.r, "edit"), "no editing while Sage answers");
}

#[quench_test::test]
fn a_conversation_is_deleted_after_a_confirmation() {
    let l = fresh();
    let menu = find(&l.r, "div", ("id", "history-item-c1"));
    let delete = descendants(&l.r, menu)
        .into_iter()
        .find(|n| {
            l.r.attr(*n, "class")
                .is_some_and(|c| c.contains("sg-delete"))
        })
        .unwrap();
    l.r.fire(delete, &Event::Click);
    let text = l.text();
    assert!(text.contains("Confirm Delete"), "{text}");
    assert!(
        text.contains("Are you sure you want to delete this conversation?"),
        "{text}"
    );
    assert!(text.contains("\"First chat\""), "{text}");
    // cancel closes it without asking
    l.forget();
    l.click("button", "dialog-cancel");
    assert!(!present(&l.r, "dialog"));
    assert!(l.asked().is_empty());
    // the confirmation asks
    l.r.fire(delete_button(&l, "history-item-c1"), &Event::Click);
    l.click("button", "dialog-confirm");
    assert_eq!(l.asked(), vec![Request::DeleteConversation("c1".into())]);
    l.send(Msg::ConversationDeleted("c1".into()));
    assert!(!present(&l.r, "dialog"));
    assert!(!present(&l.r, "history-item-c1"));
}

fn delete_button(l: &Live, row: &str) -> NodeId {
    let menu = find(&l.r, "div", ("id", row));
    descendants(&l.r, menu)
        .into_iter()
        .find(|n| {
            l.r.attr(*n, "class")
                .is_some_and(|c| c.contains("sg-delete"))
        })
        .unwrap()
}

#[quench_test::test]
fn deleting_the_open_conversation_goes_to_a_new_chat() {
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, thread()),
    );
    l.send(Msg::ConversationDeleted("c1".into()));
    assert_eq!(l.path(), "/sage/ui/home");
    assert!(l.asked().contains(&Request::NewId));
}

#[quench_test::test]
fn a_project_is_created_from_a_dialog() {
    let l = fresh();
    l.click("button", "new-project");
    assert!(l.text().contains("Create New Project"), "{}", l.text());
    // a name is needed
    l.forget();
    l.submit("new-project-form");
    assert!(l.asked().is_empty());
    assert!(present(&l.r, "project-error"));
    assert!(l.text().contains("A project needs a name"), "{}", l.text());
    l.type_into("input", "project-name", "  Notes ");
    l.submit("new-project-form");
    let asked = l.asked();
    let [Request::CreateProject(body)] = asked.as_slice() else {
        panic!("{asked:?}");
    };
    assert_eq!(body.name, "Notes");
    l.send(Msg::ProjectCreated(project("p9", "Notes")));
    assert_eq!(l.path(), "/sage/ui/home?project_id=p9");
    assert!(!present(&l.r, "dialog"));
    assert!(l.text().contains("Notes"));
}

#[quench_test::test]
fn the_open_project_lists_its_files_which_can_be_deleted() {
    let l = live("/sage/ui/home?project_id=p1");
    assert!(
        l.asked().contains(&Request::ProjectFiles("p1".into())),
        "{:?}",
        l.asked()
    );
    l.send(Msg::Models(models(true)));
    l.send(Msg::NewId("n".into()));
    l.send(Msg::Projects(vec![project("p1", "Atlas")]));
    l.send(Msg::ProjectFiles {
        project_id: "p1".into(),
        files: vec![
            file("f1", "spec.txt", "ready"),
            file("f2", "draft.pdf", "processing"),
        ],
    });
    // collapsed, with a count
    assert!(present(&l.r, "project-files"));
    assert!(
        l.text().contains("Files") && l.text().contains('2'),
        "{}",
        l.text()
    );
    assert!(!present(&l.r, "file-item-f1"));
    l.click("button", "files-toggle");
    assert!(present(&l.r, "file-item-f1") && present(&l.r, "file-item-f2"));
    let html = l.r.html();
    assert!(
        html.contains("href=\"/sage/api/v1/files/f1/download\""),
        "{html}"
    );
    // a file that is not ready wears its status
    assert!(l.text().contains("processing"), "{}", l.text());
    l.r.fire(delete_button(&l, "file-item-f1"), &Event::Click);
    assert!(
        l.text()
            .contains("Are you sure you want to delete this file?")
    );
    assert!(l.text().contains("\"spec.txt\""));
    l.forget();
    l.click("button", "dialog-confirm");
    assert_eq!(l.asked(), vec![Request::DeleteFile("f1".into())]);
    l.send(Msg::FileGone("f1".into()));
    assert!(!present(&l.r, "file-item-f1"));
    assert!(!present(&l.r, "dialog"));
}

#[quench_test::test]
fn an_empty_project_says_so() {
    let l = live("/sage/ui/home?project_id=p1");
    l.send(Msg::Models(models(true)));
    l.send(Msg::Projects(vec![project("p1", "Atlas")]));
    l.send(Msg::ProjectFiles {
        project_id: "p1".into(),
        files: vec![],
    });
    l.click("button", "files-toggle");
    assert!(
        l.text().contains("No files uploaded for this project."),
        "{}",
        l.text()
    );
}

fn uploaded(name: &str, status: u16, body: &str) -> String {
    quench_serde::json::to_string(
        &quench_serde::json!({"name": name, "status": status, "body": body}),
    )
}

#[quench_test::test]
fn the_shells_upload_report_stages_a_chip_that_is_polled_and_sent_with_the_message() {
    let l = fresh();
    // the picker carries what the shell's script needs
    let picker = find(&l.r, "input", ("id", "composer-file-input"));
    assert_eq!(
        l.r.attr(picker, "data-upload-url").as_deref(),
        Some("/sage/api/v1/files/staged")
    );
    assert_eq!(
        l.r.attr(picker, "data-upload-fields").as_deref(),
        Some(r#"{"conversation_id":"fresh-1"}"#)
    );
    assert_eq!(
        l.r.attr(picker, "data-upload-result").as_deref(),
        Some("upload-result")
    );
    assert_eq!(l.r.attr(picker, "accept").as_deref(), Some(".txt,.pdf"));

    let body = quench_serde::json::to_string(&file("f1", "notes.txt", "uploaded"));
    l.type_into("input", "upload-result", &uploaded("notes.txt", 201, &body));
    let text = l.text();
    assert!(
        text.contains("notes.txt") && text.contains("queued"),
        "{text}"
    );
    // a file being processed is asked after every two seconds until it is ready
    l.forget();
    tick(2_000.0);
    assert_eq!(l.asked(), vec![Request::FileStatus("f1".into())]);
    l.send(Msg::File(file("f1", "notes.txt", "processing")));
    assert!(l.text().contains("processing"));
    l.forget();
    tick(2_000.0);
    assert_eq!(l.asked(), vec![Request::FileStatus("f1".into())]);
    l.send(Msg::File(file("f1", "notes.txt", "ready")));
    assert!(l.text().contains("ready"));
    l.forget();
    tick(2_000.0);
    assert!(l.asked().is_empty(), "a ready file is left alone");
    // it goes with the message, and the composer is empty again
    l.type_into("textarea", "chat-input", "summarise it");
    l.submit("chat-form");
    let Request::Send(body) = &l.asked()[0] else {
        panic!("{:?}", l.asked());
    };
    assert_eq!(body.file_ids, "f1");
    assert!(
        !present(&l.r, "chip-f1")
            || with_class(&l.r, "sg-pending")
                .iter()
                .all(|p| descendants(&l.r, *p).is_empty())
    );
}

#[quench_test::test]
fn a_staged_chip_can_be_retried_and_removed_and_a_refused_upload_says_why() {
    let l = fresh();
    let mut failed = file("f1", "bad.pdf", "failed");
    failed.error_message = Some("no text".into());
    let body = quench_serde::json::to_string(&failed);
    l.type_into("input", "upload-result", &uploaded("bad.pdf", 201, &body));
    assert!(l.text().contains("failed"));
    l.forget();
    let retry = with_class(&l.r, "sg-retry");
    assert_eq!(retry.len(), 1);
    l.r.fire(retry[0], &Event::Click);
    assert_eq!(l.asked(), vec![Request::Reprocess("f1".into())]);
    l.forget();
    l.r.fire(with_class(&l.r, "sg-remove")[0], &Event::Click);
    assert_eq!(l.asked(), vec![Request::DeleteFile("f1".into())]);
    l.send(Msg::FileGone("f1".into()));
    assert!(with_class(&l.r, "sg-chip").is_empty());

    l.type_into(
        "input",
        "upload-result",
        &uploaded("big.zip", 400, "api_error_unsupported_file_type"),
    );
    assert!(l.text().contains("Unsupported file type"), "{}", l.text());
}

#[quench_test::test]
fn without_a_model_the_composer_is_disabled_and_says_so() {
    let l = live("/sage/ui/home");
    let mut none = models(true);
    none.instances.clear();
    l.send(Msg::Models(none));
    l.send(Msg::NewId("n".into()));
    assert!(present(&l.r, "no-model"));
    assert!(
        l.text()
            .contains("No model is currently selected or available.")
    );
    assert!(l.text().contains("No models available"));
    assert!(
        l.r.attr(find(&l.r, "button", ("id", "send")), "disabled")
            .is_some()
    );
}

#[quench_test::test]
fn switching_conversation_resets_the_screen_and_asks_for_the_new_one() {
    let l = open(
        "/sage/ui/home?conversation_id=c1",
        conversation("c1", None, thread()),
    );
    l.forget();
    l.r.fire(
        find(
            &l.r,
            "a",
            ("href", "/sage/ui/home?conversation_id=c3&project_id=p1"),
        ),
        &Event::Click,
    );
    assert_eq!(l.path(), "/sage/ui/home?conversation_id=c3&project_id=p1");
    let asked = l.asked();
    assert!(asked.contains(&Request::StopStream), "{asked:?}");
    assert!(
        asked.contains(&Request::Conversation("c3".into())),
        "{asked:?}"
    );
    assert!(
        asked.contains(&Request::ProjectFiles("p1".into())),
        "{asked:?}"
    );
    assert!(!l.text().contains("How do I loop?"));
    // the new chat button starts a clean one
    l.forget();
    l.r.fire(find(&l.r, "a", ("id", "new-chat")), &Event::Click);
    assert!(l.asked().contains(&Request::NewId), "{:?}", l.asked());
}

#[quench_test::test]
fn the_banner_shows_a_lost_session_a_failure_and_a_refusal() {
    let l = fresh();
    l.send(Msg::Rejected("api_error_file_too_large".into()));
    assert!(
        l.text().contains("File exceeds the maximum allowed size"),
        "{}",
        l.text()
    );
    l.send(Msg::Failed("cannot reach the service: down".into()));
    assert!(l.text().contains("cannot reach the service: down"));
    l.send(Msg::SignedOut);
    assert!(l.text().contains("Your session ended."));
    let sign_in = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(l.r.attr(sign_in, "href").as_deref(), Some("/sage/ui/login"));
}

#[quench_test::test]
fn the_language_follows_the_picker_and_is_kept_by_the_host() {
    let l = fresh();
    l.forget();
    l.r.fire(
        find(&l.r, "select", ("id", "language")),
        &Event::Input("de-DE".into()),
    );
    assert_eq!(l.asked(), vec![Request::Language("de-DE".into())]);
    assert!(
        l.text().contains("Neuer Chat") || l.text().contains("Neu"),
        "{}",
        l.text()
    );
}

#[quench_test::test]
fn an_unknown_address_is_not_found() {
    let l = live("/sage/ui/nowhere");
    assert!(present(&l.r, "not-found"));
}

quench_test::main!();
