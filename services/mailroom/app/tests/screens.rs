//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::pedantic,
    clippy::nursery
)]

use std::cell::RefCell;
use std::rc::Rc;

use mailroom_app::screens::admin::megabytes;
use mailroom_app::screens::compose::{Uploaded, complete, prefill, typing, upload_outcome};
use mailroom_app::screens::reader::unsubscribe_link;
use mailroom_app::{ListQuery, Mailroom, Msg, Request, Setup, mailroom};
use mailroom_core::admin::{
    AdminMailbox, AuditEntry, Delivery, Page, Settings, SettingsInput, Stats,
};
use mailroom_core::{
    AttachmentInfo, Change, Contact, Draft, DraftInput, Folder, Me, MessageList, MessageSummary,
    MessageView, Upload,
};
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::{History, MemoryHistory};
use quench_ui::test_renderer::NodeId;

const NOW: i64 = 1_790_683_200;

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

    fn click_attr(&self, tag: &str, attr: (&str, &str)) {
        self.r.fire(find(&self.r, tag, attr), &Event::Click);
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
        self.asked().last().cloned().expect("something was asked")
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

fn live(path: &str) -> Live {
    let asked: Asked = Rc::default();
    let log = asked.clone();
    let history = Rc::new(MemoryHistory::new(path));
    let Mailroom { view, deliver } = mailroom(
        Rc::new(move |q| log.borrow_mut().push(q)),
        Setup {
            base: "/mailroom".into(),
            language: "en-US".into(),
            login_url: "/mailroom/ui/login".into(),
            utc_offset_minutes: 0,
            now: NOW,
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

fn folders() -> Vec<Folder> {
    vec![
        Folder {
            id: "inbox".into(),
            unread: 2,
            total: 10,
        },
        Folder {
            id: "sent".into(),
            unread: 0,
            total: 4,
        },
        Folder {
            id: "trash".into(),
            unread: 5,
            total: 5,
        },
    ]
}

fn person(address: &str) -> Contact {
    Contact {
        name: String::new(),
        address: address.into(),
    }
}

fn message(id: &str, subject: &str, seen: bool) -> MessageSummary {
    MessageSummary {
        id: id.into(),
        folder: "inbox".into(),
        subject: subject.into(),
        from: Contact {
            name: "Alice".into(),
            address: "alice@home.example".into(),
        },
        to: vec![person("ann@home.example")],
        date: NOW - 3600,
        preview: "the first words".into(),
        seen,
        flagged: false,
        answered: false,
        attachments: false,
        size: 1200,
        automated: false,
    }
}

fn open_view(id: &str, seen: bool) -> MessageView {
    MessageView {
        summary: message(id, "Hello there", seen),
        cc: vec![Contact {
            name: "Carol".into(),
            address: "carol@home.example".into(),
        }],
        message_id: "<m1@home.example>".into(),
        text: "line one\nline two".into(),
        has_html: false,
        remote_images: 0,
        attachments: vec![],
        unsubscribe: String::new(),
    }
}

fn start(l: &Live, admin: bool) {
    l.send(Msg::Me(Me {
        username: "ann".into(),
        address: "ann@home.example".into(),
        can_write: true,
        admin,
        signature: String::new(),
    }));
    l.send(Msg::Folders(folders()));
}

fn list(items: Vec<MessageSummary>, total: u32, offset: u32) -> MessageList {
    MessageList {
        items,
        total,
        offset,
    }
}

fn query(folder: &str) -> ListQuery {
    ListQuery {
        folder: folder.into(),
        ..ListQuery::default()
    }
}

// -- the shell ---------------------------------------------------------------

#[quench_test::test]
fn the_app_starts_by_asking_who_and_what() {
    let l = live("/mailroom/ui/home");
    assert_eq!(l.asked()[0], Request::Startup);
    assert_eq!(l.asked()[1], Request::List(query("inbox")));
    assert!(present(&l.r, "waiting"));
}

#[quench_test::test]
fn the_sidebar_lists_the_three_folders_and_only_the_inbox_counts_unread() {
    let l = live("/mailroom/ui/home");
    // Before anything is known the folders are still there.
    for id in ["inbox", "sent", "trash"] {
        find(&l.r, "a", ("data-folder", id));
    }
    start(&l, false);
    let text = l.text();
    for needle in ["Inbox", "Sent", "Trash"] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    let inbox = find(&l.r, "a", ("data-folder", "inbox"));
    assert_eq!(
        l.r.attr(inbox, "href").as_deref(),
        Some("/mailroom/ui/folder/inbox")
    );
    assert!(
        l.r.text_of(inbox).contains('2'),
        "the unread count is shown"
    );
    assert!(
        l.r.attr(inbox, "aria-current").is_some(),
        "home is the inbox"
    );
    // Unread mail in the trash is not news.
    assert!(
        !l.r.text_of(find(&l.r, "a", ("data-folder", "trash")))
            .contains('5')
    );
    assert!(present(&l.r, "nav-compose"));
    assert!(
        !present(&l.r, "nav-admin"),
        "no administration for an ordinary person"
    );
    assert!(
        !present(&l.r, "nav-accounts"),
        "there are no outside mailboxes to manage"
    );
}

#[quench_test::test]
fn the_current_folder_is_marked() {
    let l = live("/mailroom/ui/folder/sent");
    start(&l, false);
    let sent = find(&l.r, "a", ("data-folder", "sent"));
    assert!(l.r.attr(sent, "aria-current").is_some());
    let inbox = find(&l.r, "a", ("data-folder", "inbox"));
    assert!(l.r.attr(inbox, "aria-current").is_none());
}

#[quench_test::test]
fn administrators_get_the_panel_link() {
    let l = live("/mailroom/ui/home");
    start(&l, true);
    assert!(present(&l.r, "nav-admin"));
}

#[quench_test::test]
fn problems_are_said_in_words() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Failed("cannot reach the service: down".into()));
    assert!(l.text().contains("cannot reach the service: down"));
    l.send(Msg::SignedOut);
    let link = find(&l.r, "a", ("id", "sign-in"));
    assert_eq!(
        l.r.attr(link, "href").as_deref(),
        Some("/mailroom/ui/login")
    );
    l.send(Msg::NotFound);
    assert!(present(&l.r, "not-found"));
    let l = live("/mailroom/ui/nowhere");
    assert!(present(&l.r, "not-found"));
}

#[quench_test::test]
fn a_refused_change_is_shown_until_the_next_page() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Rejected("the mailbox is full".into()));
    assert!(present(&l.r, "notice") && l.text().contains("the mailbox is full"));
    l.send(Msg::Messages(list(vec![message("m1", "x", true)], 1, 0)));
    // A page that arrives does not clear it; a change that worked does.
    assert!(present(&l.r, "notice"));
    l.send(Msg::Changed);
    assert!(!present(&l.r, "notice"));
}

#[quench_test::test]
fn the_language_choice_is_handed_to_the_host() {
    let l = live("/mailroom/ui/home");
    assert!(!l.asked().iter().any(|r| matches!(r, Request::Language(_))));
    // The bar offers every language the app has; picking one is the host's to remember.
    let options: Vec<_> =
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.attr(*n, "data-lang").is_some())
            .collect();
    assert!(
        options.is_empty() || options.len() == mailroom_app::LANGUAGES.len(),
        "{}",
        options.len()
    );
    assert_eq!(mailroom_app::LANGUAGES[0].0, "en-US");
}

// -- lists ---------------------------------------------------------------

#[quench_test::test]
fn the_inbox_lists_messages_with_unread_in_bold() {
    let l = live("/mailroom/ui/home");
    start(&l, false);
    l.send(Msg::Messages(list(
        vec![
            message("m1", "Unread one", false),
            message("m2", "Read one", true),
        ],
        2,
        0,
    )));
    let text = l.text();
    assert!(
        text.contains("Unread one") && text.contains("Alice") && text.contains("the first words"),
        "{text}"
    );
    assert!(
        text.contains("11:00"),
        "today's mail shows its time: {text}"
    );
    let first = find(&l.r, "div", ("data-message", "m1"));
    assert!(l.r.attr(first, "class").unwrap().contains("mr-unread"));
    assert!(
        !l.r.attr(find(&l.r, "div", ("data-message", "m2")), "class")
            .unwrap()
            .contains("mr-unread")
    );
    assert_eq!(
        l.r.attr(
            find(&l.r, "a", ("href", "/mailroom/ui/message/m1")),
            "class"
        )
        .unwrap(),
        "mr-row-link"
    );
    assert_eq!(l.r.text_of(find(&l.r, "h1", ("id", "list-title"))), "Inbox");
}

#[quench_test::test]
fn a_row_marks_service_mail_attachments_and_nameless_subjects() {
    let l = live("/mailroom/ui/home");
    let mut auto = message("m1", "", true);
    auto.automated = true;
    auto.attachments = true;
    auto.preview = String::new();
    l.send(Msg::Messages(list(vec![auto], 1, 0)));
    let row = find(&l.r, "div", ("data-message", "m1"));
    let text = l.r.text_of(row);
    assert!(text.contains("Automatic message"), "{text}");
    assert!(text.contains('—'), "an empty subject is a dash: {text}");
}

#[quench_test::test]
fn a_sent_row_names_who_it_went_to() {
    let l = live("/mailroom/ui/folder/sent");
    let mut sent = message("s1", "Plans", true);
    sent.folder = "sent".into();
    sent.from = person("ann@home.example");
    sent.to = vec![person("bob@home.example")];
    let mut nobody = message("s2", "Draft", true);
    nobody.folder = "sent".into();
    nobody.to = vec![];
    l.send(Msg::Messages(list(vec![sent, nobody], 2, 0)));
    assert!(
        l.r.text_of(find(&l.r, "div", ("data-message", "s1")))
            .contains("bob@home.example")
    );
    assert!(
        l.r.text_of(find(&l.r, "div", ("data-message", "s2")))
            .contains('—')
    );
}

#[quench_test::test]
fn an_empty_list_says_so() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Messages(list(vec![], 0, 0)));
    assert!(present(&l.r, "empty"));
}

#[quench_test::test]
fn a_folder_page_asks_for_its_folder_and_is_titled_by_it() {
    let l = live("/mailroom/ui/folder/sent");
    start(&l, false);
    assert!(l.asked().contains(&Request::List(query("sent"))));
    assert_eq!(l.r.text_of(find(&l.r, "h1", ("id", "list-title"))), "Sent");
    let l = live("/mailroom/ui/folder/trash");
    assert_eq!(l.r.text_of(find(&l.r, "h1", ("id", "list-title"))), "Trash");
}

#[quench_test::test]
fn a_list_that_fails_says_so_instead_of_waiting() {
    let l = live("/mailroom/ui/home");
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Failed("the service answered 500".into()));
    assert!(present(&l.r, "problem"));
    assert!(l.text().contains("the service answered 500"));
}

#[quench_test::test]
fn searching_filtering_and_paging_ask_again() {
    let l = live("/mailroom/ui/home");
    start(&l, false);
    l.send(Msg::Messages(list(
        (0..50)
            .map(|i| message(&format!("m{i}"), "x", true))
            .collect(),
        120,
        0,
    )));
    assert!(l.text().contains("1–50 / 120"));
    // Search.
    l.type_into("input", "search", "invoice");
    l.submit("search-form");
    assert_eq!(
        l.last(),
        Request::List(ListQuery {
            folder: "inbox".into(),
            text: "invoice".into(),
            ..ListQuery::default()
        })
    );
    // Unread only keeps the text.
    l.click("button", "unread-toggle");
    assert_eq!(
        l.last(),
        Request::List(ListQuery {
            folder: "inbox".into(),
            text: "invoice".into(),
            unread: true,
            ..ListQuery::default()
        })
    );
    // So does starred only.
    l.click("button", "starred-toggle");
    assert_eq!(
        l.last(),
        Request::List(ListQuery {
            folder: "inbox".into(),
            text: "invoice".into(),
            unread: true,
            flagged: true,
            offset: 0
        })
    );
    // The next page keeps all of them.
    l.click("button", "older");
    assert_eq!(
        l.last(),
        Request::List(ListQuery {
            folder: "inbox".into(),
            text: "invoice".into(),
            unread: true,
            flagged: true,
            offset: 50
        })
    );
    l.send(Msg::Messages(list(
        (0..50)
            .map(|i| message(&format!("n{i}"), "x", true))
            .collect(),
        120,
        50,
    )));
    assert!(l.text().contains("51–100 / 120"));
    l.click("button", "newer");
    assert!(matches!(l.last(), Request::List(q) if q.offset == 0));
    // Switching a filter off asks again too.
    l.click("button", "starred-toggle");
    assert!(matches!(l.last(), Request::List(q) if !q.flagged && q.unread));
}

#[quench_test::test]
fn the_first_page_cannot_go_newer_and_the_last_cannot_go_older() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Messages(list(vec![message("m1", "only", true)], 1, 0)));
    assert!(
        l.r.attr(find(&l.r, "button", ("id", "newer")), "disabled")
            .is_some()
    );
    assert!(
        l.r.attr(find(&l.r, "button", ("id", "older")), "disabled")
            .is_some()
    );
}

#[quench_test::test]
fn the_star_asks_for_a_change_and_a_change_asks_for_the_list_again() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Messages(list(
        vec![message("m1", "Star me", true)],
        1,
        0,
    )));
    l.click_attr("button", ("data-star", "m1"));
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["m1".into()],
            flagged: Some(true),
            ..Change::default()
        })
    );
    let before = l.asked().len();
    l.send(Msg::Changed);
    let after = l.asked();
    assert!(
        after[before..]
            .iter()
            .any(|r| matches!(r, Request::List(_)))
    );
    assert!(after[before..].contains(&Request::Folders));
    // A starred message offers to take the star away.
    let mut starred = message("m2", "Starred", true);
    starred.flagged = true;
    l.send(Msg::Messages(list(vec![starred], 1, 0)));
    l.click_attr("button", ("data-star", "m2"));
    assert!(matches!(l.last(), Request::Change(c) if c.flagged == Some(false)));
}

#[quench_test::test]
fn selecting_messages_offers_what_can_be_done_with_them() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Messages(list(
        vec![
            message("m1", "One", false),
            message("m2", "Two", true),
            message("m3", "Three", true),
        ],
        3,
        0,
    )));
    assert!(
        idle(&l),
        "nothing is selected yet: the buttons are there and wait"
    );
    l.click_attr("input", ("data-select", "m1"));
    l.click_attr("input", ("data-select", "m2"));
    l.click_attr("input", ("data-select", "m3"));
    // Clicking again takes a message out of the selection.
    l.click_attr("input", ("data-select", "m3"));
    assert!(
        present(&l.r, "bulk") && !idle(&l),
        "something chosen: the buttons work"
    );
    assert!(
        l.r.text_of(find(&l.r, "span", ("id", "selected-count")))
            .starts_with('2')
    );
    assert!(!present(&l.r, "bulk-delete") && !present(&l.r, "bulk-restore"));
    for (button, expected) in [
        (
            "bulk-read",
            Change {
                ids: vec!["m1".into(), "m2".into()],
                seen: Some(true),
                ..Change::default()
            },
        ),
        (
            "bulk-unread",
            Change {
                ids: vec!["m1".into(), "m2".into()],
                seen: Some(false),
                ..Change::default()
            },
        ),
        (
            "bulk-star",
            Change {
                ids: vec!["m1".into(), "m2".into()],
                flagged: Some(true),
                ..Change::default()
            },
        ),
        (
            "bulk-trash",
            Change {
                ids: vec!["m1".into(), "m2".into()],
                move_to: Some("trash".into()),
                ..Change::default()
            },
        ),
    ] {
        l.click("button", button);
        assert_eq!(l.last(), Request::Change(expected), "{button}");
        // Asking for the change empties the selection; select the two again for the next.
        assert!(idle(&l));
        l.click_attr("input", ("data-select", "m1"));
        l.click_attr("input", ("data-select", "m2"));
    }
}

#[quench_test::test]
fn the_trash_offers_restoring_and_deleting_for_good() {
    let l = live("/mailroom/ui/folder/trash");
    let mut gone = message("t1", "Old", true);
    gone.folder = "trash".into();
    l.send(Msg::Messages(list(vec![gone], 1, 0)));
    l.click_attr("input", ("data-select", "t1"));
    assert!(!present(&l.r, "bulk-trash"));
    l.click("button", "bulk-restore");
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["t1".into()],
            move_to: Some("inbox".into()),
            ..Change::default()
        })
    );
    l.click_attr("input", ("data-select", "t1"));
    l.click("button", "bulk-delete");
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["t1".into()],
            delete: true,
            ..Change::default()
        })
    );
}

#[quench_test::test]
fn a_new_search_or_page_forgets_the_selection() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Messages(list(
        (0..50)
            .map(|i| message(&format!("m{i}"), "x", true))
            .collect(),
        80,
        0,
    )));
    l.click_attr("input", ("data-select", "m1"));
    assert!(
        present(&l.r, "bulk") && !idle(&l),
        "something chosen: the buttons work"
    );
    l.submit("search-form");
    assert!(idle(&l));
    l.click_attr("input", ("data-select", "m1"));
    l.click("button", "older");
    assert!(idle(&l));
}

#[quench_test::test]
fn moving_from_the_list_asks_for_the_list_and_the_folders_again() {
    let l = live("/mailroom/ui/home");
    start(&l, false);
    l.send(Msg::Messages(list(vec![message("m1", "x", true)], 1, 0)));
    let before = l.asked().len();
    l.send(Msg::Moved);
    let after = l.asked();
    assert!(after[before..].contains(&Request::Folders));
    assert!(after[before..].contains(&Request::List(query("inbox"))));
    assert_eq!(l.path(), "/mailroom/ui/home", "the list stays where it is");
}

// -- the reader ---------------------------------------------------------------

#[quench_test::test]
fn opening_a_message_asks_for_it_and_shows_it() {
    let l = live("/mailroom/ui/message/m1");
    assert!(l.asked().contains(&Request::Open("m1".into())));
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    assert_eq!(
        l.r.text_of(find(&l.r, "h1", ("id", "subject"))),
        "Hello there"
    );
    let text = l.text();
    for needle in [
        "Alice <alice@home.example>",
        "Carol <carol@home.example>",
        "line one\nline two",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    assert!(present(&l.r, "message-text") && !present(&l.r, "message-html"));
    let back = find(&l.r, "a", ("href", "/mailroom/ui/folder/inbox"));
    assert!(l.r.tag(back).is_some());
}

#[quench_test::test]
fn a_message_with_no_subject_shows_a_dash_and_an_answered_one_says_so() {
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.summary.subject = String::new();
    view.summary.answered = true;
    view.cc = vec![];
    l.send(Msg::Message(Box::new(view)));
    assert_eq!(l.r.text_of(find(&l.r, "h1", ("id", "subject"))), "—");
    assert!(l.text().contains("Answered"));
}

#[quench_test::test]
fn opening_an_unread_message_reads_it_once() {
    let l = live("/mailroom/ui/message/m1");
    l.send(Msg::Message(Box::new(open_view("m1", false))));
    let marks = |l: &Live| {
        l.asked()
            .iter()
            .filter(|r| matches!(r, Request::Change(c) if c.seen == Some(true)))
            .count()
    };
    assert_eq!(marks(&l), 1);
    // The same unread view arriving again (before the change is made) does not ask twice.
    l.send(Msg::Message(Box::new(open_view("m1", false))));
    assert_eq!(marks(&l), 1);
    l.send(Msg::Changed);
    assert!(
        l.asked()
            .iter()
            .filter(|r| **r == Request::Open("m1".into()))
            .count()
            >= 2
    );
}

#[quench_test::test]
fn the_reader_acts_on_the_message() {
    let l = live("/mailroom/ui/message/m1");
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    let reply = find(&l.r, "a", ("data-action", "reply"));
    assert_eq!(
        l.r.attr(reply, "href").as_deref(),
        Some("/mailroom/ui/compose/reply/m1")
    );
    find(&l.r, "a", ("data-action", "replyall"));
    find(&l.r, "a", ("data-action", "forward"));
    l.click_attr("button", ("data-action", "unread"));
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["m1".into()],
            seen: Some(false),
            ..Change::default()
        })
    );
    l.click_attr("button", ("data-action", "star"));
    assert!(matches!(l.last(), Request::Change(c) if c.flagged == Some(true)));
    l.click_attr("button", ("data-action", "delete"));
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["m1".into()],
            move_to: Some("trash".into()),
            ..Change::default()
        })
    );
    assert!(!present(&l.r, "show-images"));
    assert!(
        l.r.document_order()
            .into_iter()
            .all(|n| l.r.attr(n, "data-action").as_deref() != Some("restore"))
    );
}

#[quench_test::test]
fn a_read_message_offers_to_be_marked_unread_and_an_unread_one_read() {
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.summary.flagged = true;
    l.send(Msg::Message(Box::new(view)));
    l.click_attr("button", ("data-action", "star"));
    assert!(matches!(l.last(), Request::Change(c) if c.flagged == Some(false)));
    // An unread message (already asked to be read) offers "read".
    let l = live("/mailroom/ui/message/m2");
    l.send(Msg::Message(Box::new(open_view("m2", false))));
    l.click_attr("button", ("data-action", "unread"));
    assert!(matches!(l.last(), Request::Change(c) if c.seen == Some(true)));
}

#[quench_test::test]
fn service_mail_offers_no_reply() {
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.summary.automated = true;
    l.send(Msg::Message(Box::new(view)));
    let has = |action: &str| {
        l.r.document_order()
            .into_iter()
            .any(|n| l.r.attr(n, "data-action").as_deref() == Some(action))
    };
    assert!(!has("reply") && !has("replyall"));
    assert!(has("forward") && has("delete"));
    assert!(l.text().contains("Automatic message"));
}

#[quench_test::test]
fn in_the_trash_a_message_can_be_restored_or_deleted_for_good() {
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.summary.folder = "trash".into();
    l.send(Msg::Message(Box::new(view)));
    l.click_attr("button", ("data-action", "restore"));
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["m1".into()],
            move_to: Some("inbox".into()),
            ..Change::default()
        })
    );
    l.click_attr("button", ("data-action", "delete"));
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["m1".into()],
            delete: true,
            ..Change::default()
        })
    );
}

#[quench_test::test]
fn a_move_from_the_reader_goes_back_to_the_folder() {
    let l = live("/mailroom/ui/message/m1");
    start(&l, false);
    let mut view = open_view("m1", true);
    view.summary.folder = "sent".into();
    l.send(Msg::Message(Box::new(view)));
    l.send(Msg::Moved);
    assert_eq!(l.path(), "/mailroom/ui/folder/sent");
    assert!(l.asked().contains(&Request::Folders));
}

#[quench_test::test]
fn html_is_shown_in_a_sandboxed_frame_and_remote_images_wait_for_a_click() {
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.has_html = true;
    view.remote_images = 3;
    l.send(Msg::Message(Box::new(view)));
    let frame = find(&l.r, "iframe", ("id", "message-html"));
    assert_eq!(
        l.r.attr(frame, "src").as_deref(),
        Some("/mailroom/api/v1/messages/m1/html")
    );
    let sandbox = l.r.attr(frame, "sandbox").unwrap();
    assert!(
        !sandbox.contains("allow-scripts") && !sandbox.contains("allow-same-origin"),
        "{sandbox}"
    );
    assert_eq!(
        l.r.attr(frame, "referrerpolicy").as_deref(),
        Some("no-referrer")
    );
    assert!(l.text().contains("Show images (3)"));
    l.click("button", "show-images");
    assert_eq!(
        l.r.attr(find(&l.r, "iframe", ("id", "message-html")), "src")
            .as_deref(),
        Some("/mailroom/api/v1/messages/m1/html?remote=1")
    );
    // The plain text is one click away, and the HTML one click back.
    l.click("button", "switch-view");
    assert!(present(&l.r, "message-text"));
    assert!(!present(&l.r, "message-html"));
    l.click("button", "switch-view");
    assert!(present(&l.r, "message-html"));
}

#[quench_test::test]
fn attachments_are_downloads() {
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.attachments = vec![AttachmentInfo {
        index: 0,
        filename: "report.pdf".into(),
        media_type: "application/pdf".into(),
        size: 340 * 1024,
    }];
    l.send(Msg::Message(Box::new(view)));
    let link = find(&l.r, "a", ("download", "report.pdf"));
    assert_eq!(
        l.r.attr(link, "href").as_deref(),
        Some("/mailroom/api/v1/messages/m1/attachments/0")
    );
    assert_eq!(l.r.attr(link, "rel").as_deref(), Some("noopener"));
    assert!(l.text().contains("report.pdf (340 KB)"));
    assert!(present(&l.r, "attachments"));
    // A message without them has no such box.
    let l = live("/mailroom/ui/message/m1");
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    assert!(!present(&l.r, "attachments"));
}

#[quench_test::test]
fn the_unsubscribe_link_is_https_only() {
    assert_eq!(
        unsubscribe_link("<mailto:u@x.example>, <https://x.example/u?id=1>").as_deref(),
        Some("https://x.example/u?id=1")
    );
    assert_eq!(
        unsubscribe_link("https://x.example/u").as_deref(),
        Some("https://x.example/u")
    );
    assert_eq!(unsubscribe_link("<http://x.example/u>"), None);
    assert_eq!(unsubscribe_link("<javascript:alert(1)>"), None);
    assert_eq!(unsubscribe_link("<https://x.example/ u>"), None);
    assert_eq!(
        unsubscribe_link(&format!("https://x.example/{}", "a".repeat(2000))),
        None
    );
    assert_eq!(unsubscribe_link(""), None);
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.unsubscribe = "<https://x.example/u>".into();
    l.send(Msg::Message(Box::new(view)));
    let link = find(&l.r, "a", ("id", "unsubscribe"));
    assert_eq!(
        l.r.attr(link, "rel").as_deref(),
        Some("noopener noreferrer nofollow")
    );
    assert_eq!(l.r.attr(link, "target").as_deref(), Some("_blank"));
}

#[quench_test::test]
fn a_message_that_is_not_the_one_asked_for_is_not_shown() {
    let l = live("/mailroom/ui/message/m1");
    l.send(Msg::Message(Box::new(open_view("other", true))));
    assert!(present(&l.r, "waiting"));
}

#[quench_test::test]
fn a_message_that_is_gone_says_so() {
    let l = live("/mailroom/ui/message/m1");
    l.send(Msg::NotFound);
    assert!(present(&l.r, "not-found"));
}

// -- compose --------------------------------------------------------------------

#[quench_test::test]
fn composing_waits_for_who_is_writing() {
    let l = live("/mailroom/ui/compose");
    assert!(present(&l.r, "waiting"));
    start(&l, false);
    assert!(present(&l.r, "compose"));
}

#[quench_test::test]
fn the_form_says_which_domain_stays_inside() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    assert!(
        l.r.text_of(find(&l.r, "p", ("id", "compose-hint")))
            .ends_with("@home.example")
    );
}

#[quench_test::test]
fn a_new_message_is_checked_then_sent() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    // Nothing to send yet.
    l.submit("compose");
    assert!(l.text().contains("add at least one recipient"));
    assert!(!l.asked().iter().any(|r| matches!(r, Request::Send(_))));
    l.type_into("input", "compose-to", "not an address");
    l.submit("compose");
    assert!(l.text().contains("user name or name@domain"));
    l.type_into("input", "compose-to", "bob, Carol <carol@home.example>");
    l.type_into("input", "compose-subject", "Plans");
    l.type_into("textarea", "compose-body", "See you friday.");
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!("not sent: {:?}", l.asked())
    };
    assert_eq!(compose.to, ["bob", "Carol <carol@home.example>"]);
    assert_eq!(
        (compose.subject.as_str(), compose.text.as_str()),
        ("Plans", "See you friday.")
    );
    assert!(compose.in_reply_to.is_none() && compose.attachments.is_empty());
}

#[quench_test::test]
fn what_was_sent_goes_to_the_sent_folder_and_is_said() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    l.send(Msg::Uploaded(Upload {
        id: "u1".into(),
        filename: "a.txt".into(),
        size: 3,
    }));
    l.send(Msg::Sent);
    assert_eq!(l.path(), "/mailroom/ui/folder/sent");
    assert!(l.text().contains("The message is on its way."));
    assert!(l.asked().contains(&Request::Folders));
    // The next compose starts without the old attachment.
    let l2 = live("/mailroom/ui/compose");
    start(&l2, false);
    assert!(!present(&l2.r, "attached"));
}

#[quench_test::test]
fn a_refused_send_is_shown_on_the_page() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    l.send(Msg::Rejected(
        "not sent: zed@home.example has no mailbox here".into(),
    ));
    assert!(l.text().contains("has no mailbox here"));
}

#[quench_test::test]
fn a_reply_starts_from_the_message_and_is_threaded() {
    let l = live("/mailroom/ui/compose/reply/m1");
    assert!(l.asked().contains(&Request::Open("m1".into())));
    assert!(present(&l.r, "waiting"));
    start(&l, false);
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    let to = find(&l.r, "input", ("id", "compose-to"));
    assert_eq!(
        l.r.attr(to, "value").as_deref(),
        Some("Alice <alice@home.example>")
    );
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(compose.subject, "Re: Hello there");
    assert_eq!(compose.in_reply_to.as_deref(), Some("m1"));
    assert!(compose.text.contains("> line one"));
    assert!(compose.cc.is_empty());
}

#[quench_test::test]
fn a_reply_to_all_opens_the_cc_line() {
    let l = live("/mailroom/ui/compose/replyall/m1");
    start(&l, false);
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    assert!(present(&l.r, "compose-cc"));
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(compose.cc, ["Carol <carol@home.example>"]);
    assert_eq!(compose.to, ["Alice <alice@home.example>"]);
}

#[quench_test::test]
fn a_forward_has_no_recipient_and_keeps_the_text() {
    let l = live("/mailroom/ui/compose/forward/m1");
    start(&l, false);
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    l.type_into("input", "compose-to", "bob");
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(compose.subject, "Fwd: Hello there");
    assert!(compose.in_reply_to.is_none());
    assert!(compose.text.contains("Forwarded message") && compose.text.contains("line two"));
}

#[quench_test::test]
fn the_pure_prefill_rules() {
    let me = "ann@home.example";
    assert_eq!(prefill("new", None, me, 0), Default::default());
    assert_eq!(prefill("reply", None, me, 0), Default::default());
    let mut view = open_view("m1", true);
    view.summary.to = vec![person("ann@home.example"), person("bob@home.example")];
    // A reply goes to the writer.
    let reply = prefill("reply", Some(&view), me, 0);
    assert_eq!(reply.to, "Alice <alice@home.example>");
    assert_eq!(reply.cc, "");
    assert_eq!(reply.in_reply_to.as_deref(), Some("m1"));
    // A reply to all adds everybody else, but not me and not the writer twice.
    let all = prefill("replyall", Some(&view), me, 0);
    assert_eq!(all.to, "Alice <alice@home.example>");
    assert_eq!(all.cc, "bob@home.example, Carol <carol@home.example>");
    // Answering something I sent goes to whom I sent it to.
    let mut mine = open_view("m2", true);
    mine.summary.from = person("ann@home.example");
    mine.summary.to = vec![person("bob@home.example")];
    mine.cc = vec![];
    assert_eq!(prefill("reply", Some(&mine), me, 0).to, "bob@home.example");
    assert_eq!(prefill("replyall", Some(&mine), me, 0).cc, "");
    // ...unless it went to nobody I can name: then it comes back to me.
    mine.summary.to = vec![];
    assert_eq!(prefill("reply", Some(&mine), me, 0).to, "ann@home.example");
    // A forward starts without a recipient.
    let fwd = prefill("forward", Some(&view), me, 0);
    assert_eq!((fwd.to.as_str(), fwd.in_reply_to), ("", None));
    assert_eq!(fwd.subject, "Fwd: Hello there");
    // Re: does not stack.
    view.summary.subject = "Re: Hello there".into();
    assert_eq!(
        prefill("reply", Some(&view), me, 0).subject,
        "Re: Hello there"
    );
}

#[quench_test::test]
fn the_extra_address_fields_open_on_request() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    assert!(!present(&l.r, "compose-cc"));
    l.click("button", "more-fields");
    assert!(present(&l.r, "compose-cc") && present(&l.r, "compose-bcc"));
    l.type_into("input", "compose-to", "bob");
    l.type_into("input", "compose-cc", "carol");
    l.type_into("input", "compose-bcc", "dave");
    l.type_into("input", "compose-subject", "s");
    l.type_into("textarea", "compose-body", "hello");
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(
        (compose.cc, compose.bcc),
        (vec!["carol".to_string()], vec!["dave".to_string()])
    );
}

#[quench_test::test]
fn the_upload_control_reports_into_the_page() {
    assert_eq!(
        upload_outcome(
            r#"{"name":"a.txt","status":201,"body":"{\"id\":\"u1\",\"filename\":\"a.txt\",\"size\":3}"}"#
        ),
        Uploaded::Done(Upload {
            id: "u1".into(),
            filename: "a.txt".into(),
            size: 3
        })
    );
    assert_eq!(
        upload_outcome(r#"{"status":400,"body":"{\"error\":\"the file is empty\"}"}"#),
        Uploaded::Failed("the file is empty".into())
    );
    assert_eq!(
        upload_outcome(r#"{"status":502,"body":"<html>"}"#),
        Uploaded::Failed("the upload failed (502)".into())
    );
    assert!(matches!(upload_outcome("nope"), Uploaded::Failed(_)));
    assert!(matches!(
        upload_outcome(r#"{"status":201,"body":"garbage"}"#),
        Uploaded::Failed(_)
    ));
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    let picker = find(&l.r, "input", ("id", "attach"));
    assert_eq!(
        l.r.attr(picker, "data-upload-url").as_deref(),
        Some("/mailroom/api/v1/uploads")
    );
    l.type_into(
        "input",
        "upload-result",
        r#"{"status":201,"body":"{\"id\":\"u1\",\"filename\":\"a.txt\",\"size\":3}"}"#,
    );
    assert!(l.text().contains("a.txt (3 B)"), "{}", l.text());
    l.type_into(
        "input",
        "upload-result",
        r#"{"status":400,"body":"{\"error\":\"too big\"}"}"#,
    );
    assert!(l.text().contains("too big"));
    l.type_into("input", "compose-to", "bob");
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!("{:?}", l.asked())
    };
    assert_eq!(
        compose.attachments,
        ["u1"],
        "an attachment alone is a message"
    );
    // Removing it asks the service to forget it and takes it off the page.
    l.click_attr("button", ("aria-label", "Remove the attachment"));
    assert_eq!(l.last(), Request::RemoveUpload("u1".into()));
    assert!(!present(&l.r, "attached"));
}

#[quench_test::test]
fn the_last_recipient_typed_is_completed_from_the_people_there_are() {
    assert_eq!(typing("bo").as_deref(), Some("bo"));
    assert_eq!(typing("ann@home.example, bo").as_deref(), Some("bo"));
    assert_eq!(typing("ann; b").as_deref(), Some("b"));
    for none in ["", "  ", "ann@home", "Ann <a@b.cc>", "two words", "a, "] {
        assert_eq!(typing(none), None, "{none:?}");
    }
    assert_eq!(complete("bo", "bob@home.example"), "bob@home.example, ");
    assert_eq!(
        complete("ann@home.example, bo", "bob@home.example"),
        "ann@home.example, bob@home.example, "
    );
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    assert!(!present(&l.r, "people"));
    l.type_into("input", "compose-to", "bo");
    assert_eq!(l.last(), Request::People("bo".into()));
    l.send(Msg::People(vec![
        person("bob@home.example"),
        person("bo2@home.example"),
    ]));
    assert!(present(&l.r, "people"));
    l.click_attr("button", ("data-person", "bob@home.example"));
    let to = find(&l.r, "input", ("id", "compose-to"));
    assert_eq!(l.r.attr(to, "value").as_deref(), Some("bob@home.example, "));
    assert!(
        !present(&l.r, "people"),
        "the suggestions go once one is taken"
    );
    // Typing a whole address or leaving the field ends the asking.
    l.send(Msg::People(vec![person("x@home.example")]));
    l.type_into("input", "compose-to", "bob@home.example");
    assert!(!present(&l.r, "people"));
}

// -- the panel -------------------------------------------------------------------

#[quench_test::test]
fn the_panel_is_not_offered_to_everybody() {
    let l = live("/mailroom/ui/admin");
    assert!(present(&l.r, "waiting"));
    start(&l, false);
    assert!(present(&l.r, "not-allowed"));
    assert!(!l.asked().contains(&Request::Stats));
}

#[quench_test::test]
fn the_overview_shows_the_numbers() {
    let l = live("/mailroom/ui/admin");
    start(&l, true);
    assert!(l.asked().contains(&Request::Stats));
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Stats(Stats {
        mailboxes: 3,
        messages: 120,
        bytes: 5 * 1024 * 1024 + 300 * 1024,
        delivered_24h: 7,
        failed_24h: 2,
        budget_left: 1990,
    }));
    let stat = |key: &str| l.r.text_of(find(&l.r, "div", ("data-stat", key)));
    assert!(stat("mr_admin_stat_mailboxes").starts_with('3'));
    assert!(stat("mr_admin_stat_messages").starts_with("120"));
    assert!(stat("mr_admin_stat_storage").starts_with("5.2 MB"));
    assert!(stat("mr_admin_stat_delivered").starts_with('7'));
    assert!(stat("mr_admin_stat_failed").starts_with('2'));
    assert!(stat("mr_admin_stat_budget").starts_with("1990"));
    let failed = find(&l.r, "div", ("data-stat", "mr_admin_stat_failed"));
    assert!(l.r.attr(failed, "class").unwrap().contains("mr-red"));
    l.send(Msg::Stats(Stats {
        failed_24h: 0,
        ..Stats::default()
    }));
    let failed = find(&l.r, "div", ("data-stat", "mr_admin_stat_failed"));
    assert!(!l.r.attr(failed, "class").unwrap().contains("mr-red"));
    // The tabs.
    for section in ["overview", "settings", "mailboxes", "deliveries", "audit"] {
        find(&l.r, "a", ("data-section", section));
    }
    assert!(
        l.r.attr(
            find(&l.r, "a", ("data-section", "overview")),
            "aria-current"
        )
        .is_some()
    );
}

#[quench_test::test]
fn sizes_are_shown_in_megabytes() {
    assert_eq!(megabytes(0), "0.0 MB");
    assert_eq!(megabytes(1024 * 1024), "1.0 MB");
    assert_eq!(megabytes(1536 * 1024), "1.5 MB");
    assert_eq!(megabytes(u64::MAX), megabytes(u64::MAX));
}

fn settings() -> Settings {
    Settings {
        domain: "home.example".into(),
        daily_budget: 2000,
        hourly_per_source: 600,
        hourly_per_person: 120,
        quota_mb: 500,
    }
}

#[quench_test::test]
fn the_settings_form_shows_what_is_set_and_saves_what_was_changed() {
    let l = live("/mailroom/ui/admin/settings");
    start(&l, true);
    assert!(l.asked().contains(&Request::Settings));
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Settings(Box::new(settings())));
    assert!(l.text().contains("@home.example"));
    let value = |id: &str| {
        l.r.attr(find(&l.r, "input", ("id", id)), "value")
            .unwrap_or_default()
    };
    assert_eq!(value("daily-budget"), "2000");
    assert_eq!(value("hourly-source"), "600");
    assert_eq!(value("hourly-person"), "120");
    assert_eq!(value("quota-mb"), "500");
    l.type_into("input", "quota-mb", "1024");
    l.type_into("input", "daily-budget", "x");
    l.submit("settings-form");
    assert!(l.text().contains("must be whole numbers"));
    assert!(
        !l.asked()
            .iter()
            .any(|r| matches!(r, Request::SaveSettings(_)))
    );
    l.type_into("input", "daily-budget", "3000");
    l.submit("settings-form");
    assert_eq!(
        l.last(),
        Request::SaveSettings(SettingsInput {
            daily_budget: Some(3000),
            hourly_per_source: Some(600),
            hourly_per_person: Some(120),
            quota_mb: Some(1024),
        })
    );
    l.send(Msg::SettingsSaved(Box::new(Settings {
        quota_mb: 1024,
        daily_budget: 3000,
        ..settings()
    })));
    assert!(l.text().contains("The settings are saved."));
}

#[quench_test::test]
fn a_refused_setting_is_shown() {
    let l = live("/mailroom/ui/admin/settings");
    start(&l, true);
    l.send(Msg::Settings(Box::new(settings())));
    l.submit("settings-form");
    l.send(Msg::Rejected(
        "the mailbox size must be from 1 to 100000".into(),
    ));
    assert!(l.text().contains("must be from 1 to 100000"));
}

fn mailbox(owner: &str, disabled: bool) -> AdminMailbox {
    AdminMailbox {
        owner: owner.into(),
        address: format!("{owner}@home.example"),
        messages: 12,
        bytes: 2 * 1024 * 1024,
        created: NOW - 86_400,
        last_seen: Some(NOW - 60),
        disabled,
    }
}

#[quench_test::test]
fn the_panel_lists_mailboxes_without_mail_and_can_switch_them_off() {
    let l = live("/mailroom/ui/admin/mailboxes");
    start(&l, true);
    assert!(l.asked().contains(&Request::Mailboxes { offset: 0 }));
    assert!(present(&l.r, "waiting"));
    let mut never = mailbox("zed", true);
    never.last_seen = None;
    l.send(Msg::Mailboxes(Page {
        items: vec![mailbox("bob", false), never],
        total: 2,
    }));
    let bob = l.r.text_of(find(&l.r, "tr", ("data-mailbox", "bob")));
    assert!(
        bob.contains("bob@home.example") && bob.contains("12") && bob.contains("2.0 MB"),
        "{bob}"
    );
    let zed = l.r.text_of(find(&l.r, "tr", ("data-mailbox", "zed")));
    assert!(zed.contains("never") && zed.contains("off"), "{zed}");
    // Switching off, and on again.
    let row = |owner: &str| {
        l.r.document_order()
            .into_iter()
            .find(|n| l.r.attr(*n, "data-mailbox").as_deref() == Some(owner))
            .unwrap()
    };
    let _ = row("bob");
    let buttons = |action: &str| -> Vec<NodeId> {
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.attr(*n, "data-action").as_deref() == Some(action))
            .collect()
    };
    l.r.fire(buttons("disable")[0], &Event::Click);
    assert_eq!(
        l.last(),
        Request::SetDisabled {
            owner: "bob".into(),
            disabled: true
        }
    );
    l.r.fire(buttons("enable")[0], &Event::Click);
    assert_eq!(
        l.last(),
        Request::SetDisabled {
            owner: "zed".into(),
            disabled: false
        }
    );
}

#[quench_test::test]
fn deleting_a_mailbox_takes_two_clicks() {
    let l = live("/mailroom/ui/admin/mailboxes");
    start(&l, true);
    l.send(Msg::Mailboxes(Page {
        items: vec![mailbox("bob", false)],
        total: 1,
    }));
    l.click_attr("button", ("data-action", "delete"));
    assert!(
        !l.asked()
            .iter()
            .any(|r| matches!(r, Request::RemoveMailbox(_)))
    );
    assert!(l.text().contains("Delete this mailbox"));
    l.click_attr("button", ("data-action", "delete"));
    assert_eq!(l.last(), Request::RemoveMailbox("bob".into()));
}

#[quench_test::test]
fn the_lists_of_the_panel_page_and_remember_where_they_are() {
    let l = live("/mailroom/ui/admin/mailboxes");
    start(&l, true);
    let many: Vec<AdminMailbox> = (0..50).map(|i| mailbox(&format!("u{i}"), false)).collect();
    l.send(Msg::Mailboxes(Page {
        items: many.clone(),
        total: 120,
    }));
    assert!(l.text().contains("1–50 / 120"));
    l.click("button", "older");
    assert_eq!(l.last(), Request::Mailboxes { offset: 50 });
    l.send(Msg::Mailboxes(Page {
        items: many,
        total: 120,
    }));
    assert!(l.text().contains("51–100 / 120"), "{}", l.text());
    l.click("button", "newer");
    assert_eq!(l.last(), Request::Mailboxes { offset: 0 });
    // An empty list reads 0–0.
    l.send(Msg::Mailboxes(Page {
        items: vec![],
        total: 0,
    }));
    assert!(l.text().contains("0–0 / 0"));
}

#[quench_test::test]
fn the_deliveries_of_the_services_are_listed() {
    let l = live("/mailroom/ui/admin/deliveries");
    start(&l, true);
    assert!(l.asked().contains(&Request::Deliveries { offset: 0 }));
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Deliveries(Page {
        items: vec![
            Delivery {
                id: "d1".into(),
                source: "gatehouse".into(),
                kind: "verification".into(),
                recipients: vec!["bob@home.example".into()],
                subject: "Confirm your email".into(),
                status: "delivered".into(),
                delivered: 1,
                error: None,
                created: NOW - 30,
            },
            Delivery {
                id: "d2".into(),
                source: "conveyor".into(),
                kind: String::new(),
                recipients: vec!["a@home.example".into(), "b@home.example".into()],
                subject: "Build".into(),
                status: "partial".into(),
                delivered: 1,
                error: Some("b has no mailbox here".into()),
                created: NOW - 20,
            },
        ],
        total: 2,
    }));
    let first = l.r.text_of(find(&l.r, "tr", ("data-delivery", "d1")));
    for needle in [
        "gatehouse (verification)",
        "bob@home.example",
        "Confirm your email",
        "delivered",
    ] {
        assert!(first.contains(needle), "{needle}: {first}");
    }
    let second = l.r.text_of(find(&l.r, "tr", ("data-delivery", "d2")));
    for needle in [
        "conveyor",
        "a@home.example, b@home.example",
        "b has no mailbox here",
    ] {
        assert!(second.contains(needle), "{needle}: {second}");
    }
    assert!(!second.contains("conveyor ("));
    l.click("button", "older");
    // 2 of 2: nothing older to ask for.
    assert!(
        l.r.attr(find(&l.r, "button", ("id", "older")), "disabled")
            .is_some()
    );
}

#[quench_test::test]
fn the_audit_log_lists_what_was_done() {
    let l = live("/mailroom/ui/admin/audit");
    start(&l, true);
    assert!(l.asked().contains(&Request::Audit { offset: 0 }));
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Audit(Page {
        items: vec![AuditEntry {
            id: "e1".into(),
            at: NOW - 100,
            actor: "ann".into(),
            action: "disable".into(),
            target: "bob".into(),
            detail: "".into(),
        }],
        total: 1,
    }));
    let table = l.r.text_of(find(&l.r, "table", ("id", "admin-audit")));
    for needle in ["ann", "disable", "bob"] {
        assert!(table.contains(needle), "{needle}: {table}");
    }
    let l = live("/mailroom/ui/admin/audit");
    start(&l, true);
    l.send(Msg::Audit(Page {
        items: (0..50)
            .map(|i| AuditEntry {
                id: format!("e{i}"),
                at: NOW,
                actor: "ann".into(),
                action: "x".into(),
                target: "y".into(),
                detail: String::new(),
            })
            .collect(),
        total: 60,
    }));
    l.click("button", "older");
    assert_eq!(l.last(), Request::Audit { offset: 50 });
}

#[quench_test::test]
fn an_unknown_panel_section_is_the_overview() {
    let l = live("/mailroom/ui/admin/whatever");
    start(&l, true);
    assert!(l.asked().contains(&Request::Stats));
}

quench_test::main!();

// -- moving between the pages of one kind -------------------------------------------

#[quench_test::test]
fn switching_folders_shows_the_folder_without_a_reload() {
    // Inbox to Sent worked; Sent to Trash left the Sent page on screen (the router keeps a page whose route is the same and
    // only its parameter changed). Every move between folders must ask for, and show, the folder chosen.
    let l = live("/mailroom/ui/home");
    start(&l, false);
    for (folder, title) in [
        ("sent", "Sent"),
        ("trash", "Trash"),
        ("inbox", "Inbox"),
        ("trash", "Trash"),
        ("sent", "Sent"),
    ] {
        l.click_attr("a", ("data-folder", folder));
        assert_eq!(l.path(), format!("/mailroom/ui/folder/{folder}"));
        assert_eq!(l.last(), Request::List(query(folder)), "{folder}");
        let heading = find(&l.r, "h1", ("id", "list-title"));
        assert_eq!(l.r.text_of(heading), title, "{folder}");
    }
}

#[quench_test::test]
fn switching_admin_tabs_shows_the_tab_without_a_reload() {
    let l = live("/mailroom/ui/admin");
    start(&l, true);
    for section in [
        "settings",
        "mailboxes",
        "audit",
        "deliveries",
        "overview",
        "audit",
    ] {
        l.click_attr("a", ("data-section", section));
        let wanted = match section {
            "settings" => matches!(l.last(), Request::Settings),
            "mailboxes" => matches!(l.last(), Request::Mailboxes { .. }),
            "audit" => matches!(l.last(), Request::Audit { .. }),
            "deliveries" => matches!(l.last(), Request::Deliveries { .. }),
            _ => matches!(l.last(), Request::Stats),
        };
        assert!(wanted, "{section} asked for {:?}", l.last());
        let current = find(&l.r, "a", ("data-section", section));
        assert!(l.r.attr(current, "aria-current").is_some(), "{section}");
    }
}

#[quench_test::test]
fn the_inbox_can_archive_and_read_everything_and_the_trash_asks_twice_before_emptying() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Messages(list(vec![message("m1", "Hi", false)], 1, 0)));
    l.click_attr("input", ("data-select", "m1"));
    l.click("button", "bulk-archive");
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["m1".into()],
            move_to: Some("archive".into()),
            ..Change::default()
        })
    );
    l.click("button", "mark-all-read");
    assert_eq!(
        l.last(),
        Request::Change(Change {
            all_in: Some("inbox".into()),
            seen: Some(true),
            ..Change::default()
        })
    );

    let t = live("/mailroom/ui/folder/trash");
    let mut old = message("t1", "Old", true);
    old.folder = "trash".into();
    t.send(Msg::Messages(list(vec![old], 1, 0)));
    t.click("button", "empty-trash");
    assert_ne!(
        t.last(),
        Request::Change(Change {
            all_in: Some("trash".into()),
            delete: true,
            ..Change::default()
        }),
        "one click only asks"
    );
    t.click("button", "empty-trash");
    assert_eq!(
        t.last(),
        Request::Change(Change {
            all_in: Some("trash".into()),
            delete: true,
            ..Change::default()
        })
    );
}

#[quench_test::test]
fn the_page_looks_again_every_half_minute_and_the_tab_says_what_is_unread() {
    alone(|| {
        let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
        quench_ui::platform::install(clock.clone());
        let l = live("/mailroom/ui/folder/inbox");
        l.send(Msg::Folders(folders()));
        assert!(
            l.r.html().contains("data-fui-badge=\"2\""),
            "{}",
            l.r.html()
        );
        let before = l.asked().len();
        clock.advance(29_000.0);
        assert_eq!(l.asked().len(), before, "not yet");
        clock.advance(2_000.0);
        let again = l.asked()[before..].to_vec();
        assert!(again.contains(&Request::Folders), "{again:?}");
        assert!(
            again
                .iter()
                .any(|r| matches!(r, Request::List(q) if q.folder == "inbox")),
            "{again:?}"
        );
        // Writing a message is not interrupted by a new list under it.
        l.click("a", "nav-compose");
        let mark = l.asked().len();
        clock.advance(31_000.0);
        let later = l.asked()[mark..].to_vec();
        assert!(
            !later.iter().any(|r| matches!(r, Request::List(_))),
            "{later:?}"
        );
    });
}

#[quench_test::test]
fn a_row_says_who_and_when_then_the_subject_with_its_tags_then_the_first_words() {
    let l = live("/mailroom/ui/home");
    let mut m = message("m1", "Build failed", false);
    m.preview = "The run on main failed at the tests".into();
    m.automated = true;
    m.attachments = true;
    l.send(Msg::Messages(list(vec![m], 1, 0)));
    let html = l.r.html();
    let at = |needle: &str| {
        html.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {html}"))
    };
    let (from, time, subject, tag, clip, preview) = (
        at("mr-from"),
        at("mr-time"),
        at("mr-subject-text"),
        at("mr-badge"),
        at("fui-ico-paperclip"),
        at("The run on main failed"),
    );
    assert!(from < time && time < subject && subject < tag.max(clip) && tag.max(clip) < preview);
    assert!(
        !html.contains(" — The run"),
        "the preview is a line of its own"
    );
}

fn kept(id: &str, subject: &str) -> Draft {
    Draft {
        id: id.into(),
        to: "bob".into(),
        subject: subject.into(),
        text: "dear bob".into(),
        updated: NOW - 60,
        ..Draft::default()
    }
}

#[quench_test::test]
fn what_is_typed_is_kept_a_moment_later_and_sent_with_the_draft_it_came_from() {
    alone(|| {
        let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
        quench_ui::platform::install(clock.clone());
        let l = live("/mailroom/ui/compose");
        start(&l, false);
        // Nothing typed, nothing kept.
        clock.advance(5_000.0);
        assert!(
            !l.asked()
                .iter()
                .any(|r| matches!(r, Request::SaveDraft { .. }))
        );
        l.type_into("input", "compose-to", "bob");
        l.type_into("input", "compose-subject", "Hello");
        l.type_into("textarea", "compose-body", "dear bob");
        clock.advance(1_000.0);
        assert!(
            !l.asked()
                .iter()
                .any(|r| matches!(r, Request::SaveDraft { .. })),
            "not yet: the typing may go on"
        );
        clock.advance(1_000.0);
        assert_eq!(
            l.last(),
            Request::SaveDraft {
                id: None,
                input: DraftInput {
                    to: "bob".into(),
                    subject: "Hello".into(),
                    text: "dear bob".into(),
                    ..DraftInput::default()
                }
            }
        );
        // The service answers with the id; a later change goes to that draft, not to a new one.
        l.send(Msg::DraftSaved(kept("d1", "Hello")));
        assert!(present(&l.r, "draft-status"));
        l.type_into("textarea", "compose-body", "dear bob, hello");
        clock.advance(2_000.0);
        assert!(
            matches!(l.last(), Request::SaveDraft { id: Some(id), .. } if id == "d1"),
            "{:?}",
            l.last()
        );
        l.send(Msg::DraftSaved(kept("d1", "Hello")));
        // Sending says which draft it was, after a few seconds in which it can be taken back.
        l.submit("compose");
        assert!(present(&l.r, "undo-send"));
        clock.advance(5_100.0);
        let Request::Send(compose) = l.last() else {
            panic!("{:?}", l.asked())
        };
        assert_eq!(compose.draft.as_deref(), Some("d1"));
        // And a message on its way is not kept again.
        let before = l.asked().len();
        l.type_into("textarea", "compose-body", "changed after sending");
        clock.advance(3_000.0);
        assert_eq!(l.asked().len(), before);
    });
}

#[quench_test::test]
fn a_draft_opens_with_everything_it_kept_and_is_not_saved_again_unchanged() {
    alone(|| {
        let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
        quench_ui::platform::install(clock.clone());
        let l = live("/mailroom/ui/compose/draft/d1");
        start(&l, false);
        assert!(l.asked().contains(&Request::OpenDraft("d1".into())));
        assert!(!present(&l.r, "compose"), "waiting for the draft");
        let mut d = kept("d1", "Hello");
        d.cc = "cy".into();
        d.bcc = "dee".into();
        d.in_reply_to = Some("m9".into());
        l.send(Msg::Draft(d));
        let html = l.r.html();
        for want in [
            "value=\"bob\"",
            "value=\"Hello\"",
            "value=\"cy\"",
            "value=\"dee\"",
            "dear bob",
        ] {
            assert!(html.contains(want), "{want}: {html}");
        }
        clock.advance(5_000.0);
        assert!(
            !l.asked()
                .iter()
                .any(|r| matches!(r, Request::SaveDraft { .. }))
        );
        // Throwing it away deletes it.
        l.click("a", "discard");
        assert_eq!(l.last(), Request::DeleteDraft("d1".into()));
    });
}

#[quench_test::test]
fn the_drafts_are_listed_with_a_way_to_open_and_delete_them_and_counted_in_the_panel() {
    let l = live("/mailroom/ui/drafts");
    l.send(Msg::Folders({
        let mut f = folders();
        f.insert(
            1,
            Folder {
                id: "drafts".into(),
                unread: 0,
                total: 2,
            },
        );
        f
    }));
    assert!(l.asked().contains(&Request::Drafts));
    l.send(Msg::Drafts(vec![kept("d1", "Hello"), kept("d2", "")]));
    let html = l.r.html();
    assert!(
        html.contains("href=\"/mailroom/ui/compose/draft/d1\""),
        "{html}"
    );
    assert!(html.contains("(no subject)"));
    assert!(html.contains("data-folder=\"drafts\""));
    l.click_attr("button", ("data-delete-draft", "d2"));
    assert_eq!(l.last(), Request::DeleteDraft("d2".into()));
    l.send(Msg::Drafts(vec![]));
    assert!(present(&l.r, "empty"));
}

#[quench_test::test]
fn a_sender_is_muted_from_the_reader_and_let_back_in_from_the_list() {
    let l = live("/mailroom/ui/message/m1");
    start(&l, false);
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    l.click_attr("button", ("data-action", "mute"));
    assert_eq!(l.last(), Request::Mute("alice@home.example".into()));
    l.send(Msg::MuteChanged {
        address: "alice@home.example".into(),
        muted: true,
    });
    assert!(present(&l.r, "done"));
    assert!(l.r.html().contains("alice@home.example"), "{}", l.r.html());
    assert_eq!(
        l.last(),
        Request::Mutes,
        "the list of muted senders is asked for again"
    );

    let m = live("/mailroom/ui/muted");
    start(&m, false);
    assert!(m.asked().contains(&Request::Mutes));
    m.send(Msg::Mutes(vec!["alice@home.example".into()]));
    m.click_attr("button", ("data-unmute", "alice@home.example"));
    assert_eq!(m.last(), Request::Unmute("alice@home.example".into()));
    m.send(Msg::Mutes(vec![]));
    assert!(present(&m.r, "empty"));
}

#[quench_test::test]
fn a_message_in_a_conversation_shows_the_rest_of_it_as_links() {
    let l = live("/mailroom/ui/message/m1");
    start(&l, false);
    assert!(l.asked().contains(&Request::Thread("m1".into())));
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    assert!(!present(&l.r, "conversation"), "alone: nothing to show");
    let mut earlier = message("m0", "Hello there", true);
    earlier.folder = "sent".into();
    l.send(Msg::Thread(vec![
        earlier,
        message("m2", "Re: Hello there", false),
    ]));
    let html = l.r.html();
    assert!(present(&l.r, "conversation"), "{html}");
    assert!(
        html.contains("href=\"/mailroom/ui/message/m0\"")
            && html.contains("href=\"/mailroom/ui/message/m2\"")
    );
    // What the person wrote shows who it went to.
    assert!(html.contains("data-thread=\"m0\""));
}

#[quench_test::test]
fn a_message_waits_a_few_seconds_and_can_be_taken_back() {
    alone(|| {
        let clock = Rc::new(quench_ui::platform::ManualPlatform::new());
        quench_ui::platform::install(clock.clone());
        let l = live("/mailroom/ui/compose");
        start(&l, false);
        l.type_into("input", "compose-to", "bob");
        l.type_into("textarea", "compose-body", "hello");
        l.submit("compose");
        assert!(present(&l.r, "undo-send"));
        clock.advance(2_000.0);
        assert!(
            !l.asked().iter().any(|r| matches!(r, Request::Send(_))),
            "still held"
        );
        l.click("button", "undo-send");
        clock.advance(10_000.0);
        assert!(
            !l.asked().iter().any(|r| matches!(r, Request::Send(_))),
            "taken back"
        );
        assert!(!present(&l.r, "undo-send"));
        // The form is still there and the draft keeps being kept; sending again goes through after the wait.
        l.submit("compose");
        clock.advance(5_100.0);
        assert!(
            matches!(l.last(), Request::Send(c) if c.text == "hello"),
            "{:?}",
            l.last()
        );
    });
}

#[quench_test::test]
fn a_signature_ends_what_is_started_and_is_kept_from_the_settings() {
    let l = live("/mailroom/ui/compose");
    l.send(Msg::Me(Me {
        username: "ann".into(),
        address: "ann@home.example".into(),
        can_write: true,
        admin: false,
        signature: "Ann, estate office".into(),
    }));
    assert!(
        l.r.html().contains("-- \nAnn, estate office"),
        "{}",
        l.r.html()
    );

    let s = live("/mailroom/ui/muted");
    start(&s, false);
    s.send(Msg::Mutes(vec![]));
    s.type_into("textarea", "signature", "Ann");
    s.submit("signature-form");
    assert_eq!(s.last(), Request::SaveSignature("Ann".into()));
    s.send(Msg::SignatureSaved("Ann".into()));
    assert!(present(&s.r, "done"));
}

#[quench_test::test]
fn a_message_opens_on_its_own_with_an_arrow_back_to_its_folder() {
    let l = live("/mailroom/ui/message/m1");
    start(&l, false);
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    let html = l.r.html();
    assert!(!html.contains("id=\"messages\""), "no list beside it");
    assert!(
        html.contains("href=\"/mailroom/ui/folder/inbox\""),
        "{html}"
    );
    assert!(html.contains("fui-ico-arrow-left") && html.contains("id=\"back\""));
    l.click("a", "back");
    assert_eq!(l.path(), "/mailroom/ui/folder/inbox");
    // The notice for a muted sender names them.
    l.send(Msg::MuteChanged {
        address: "alice@home.example".into(),
        muted: true,
    });
    assert!(l.r.html().contains("alice@home.example"));
}

#[quench_test::test]
fn the_page_is_chosen_with_one_box_and_the_whole_folder_with_one_more_click() {
    let l = live("/mailroom/ui/home");
    let page = |n: usize| {
        (0..n)
            .map(|i| message(&format!("m{i}"), "x", true))
            .collect::<Vec<_>>()
    };
    l.send(Msg::Messages(list(page(50), 80, 0)));
    assert!(!present(&l.r, "select-note"));
    l.click("input", "select-page");
    assert!(l.r.html().contains("50 selected"), "{}", l.r.html());
    assert!(
        present(&l.r, "select-everything"),
        "more than the page: offered"
    );
    assert!(l.r.html().contains("Select all 80 in this folder"));
    l.click("button", "select-everything");
    assert!(l.r.html().contains("80 selected"), "{}", l.r.html());
    l.click("button", "bulk-archive");
    assert_eq!(
        l.last(),
        Request::Change(Change {
            all_in: Some("inbox".into()),
            move_to: Some("archive".into()),
            ..Change::default()
        })
    );
    assert!(idle(&l), "the choice is spent");
    // The box again takes the choice away; a short folder has nothing more to offer.
    l.click("input", "select-page");
    assert!(!l.r.html().contains("80 selected"), "back to the page");
    l.click("input", "select-page");
    assert!(idle(&l), "chosen again, then not");
    l.send(Msg::Messages(list(page(3), 3, 0)));
    l.click("input", "select-page");
    assert!(!present(&l.r, "select-everything"));
    assert!(l.r.html().contains("3 selected"));
}

#[quench_test::test]
fn the_whole_folder_is_not_offered_for_a_search_which_lists_only_part_of_it() {
    let l = live("/mailroom/ui/home");
    l.type_into("input", "search", "alice");
    l.submit("search-form");
    let page = (0..50)
        .map(|i| message(&format!("m{i}"), "x", true))
        .collect::<Vec<_>>();
    l.send(Msg::Messages(list(page, 80, 0)));
    l.click("input", "select-page");
    assert!(!present(&l.r, "select-everything"), "{}", l.r.html());
    assert!(l.r.html().contains("50 selected"));
}

#[quench_test::test]
fn the_tab_carries_what_a_notification_would_say_and_the_settings_offer_to_allow_them() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Folders(folders()));
    assert!(
        l.r.html()
            .contains("data-fui-badge-text=\"2 unread messages\""),
        "{}",
        l.r.html()
    );
    let s = live("/mailroom/ui/muted");
    start(&s, false);
    s.send(Msg::Mutes(vec![]));
    let html = s.r.html();
    assert!(
        html.contains("id=\"notify-enable\"") && html.contains("data-fui-notify-enable"),
        "{html}"
    );
    for state in ["mr-notify-on", "mr-notify-denied", "mr-notify-unsupported"] {
        assert!(html.contains(state), "{state}");
    }
}

#[quench_test::test]
fn the_tab_says_nothing_until_the_counts_are_known() {
    let l = live("/mailroom/ui/home");
    assert!(
        !l.r.html().contains("data-fui-badge"),
        "no count yet: {}",
        l.r.html()
    );
    l.send(Msg::Folders(folders()));
    assert!(l.r.html().contains("data-fui-badge=\"2\""));
}

/// Runs a test that installs a clock on a thread of its own: the clock belongs to the thread, and a test run in the same thread
/// afterwards would otherwise find its messages held back by it.
fn alone(test: impl FnOnce() + Send + 'static) {
    std::thread::spawn(test).join().expect("the test passed");
}

/// The bulk bar is always there; with nothing chosen its buttons are disabled.
fn idle(l: &Live) -> bool {
    present(&l.r, "bulk")
        && l.r
            .attr(find(&l.r, "button", ("id", "bulk-read")), "disabled")
            .is_some()
}

#[quench_test::test]
fn the_buttons_are_always_there_with_the_box_beside_them_and_wait_for_a_choice() {
    let l = live("/mailroom/ui/home");
    l.send(Msg::Messages(list(
        vec![message("m1", "One", true), message("m2", "Two", true)],
        2,
        0,
    )));
    let html = l.r.html();
    // One line: the box first, then the buttons, whatever is chosen.
    let at = |needle: &str| {
        html.find(needle)
            .unwrap_or_else(|| panic!("{needle}: {html}"))
    };
    assert!(at("id=\"select-page\"") < at("id=\"bulk-read\""));
    for id in [
        "bulk-read",
        "bulk-unread",
        "bulk-star",
        "bulk-archive",
        "bulk-trash",
    ] {
        assert!(
            l.r.attr(find(&l.r, "button", ("id", id)), "disabled")
                .is_some(),
            "{id} waits"
        );
    }
    l.click("input", "select-page");
    for id in [
        "bulk-read",
        "bulk-unread",
        "bulk-star",
        "bulk-archive",
        "bulk-trash",
    ] {
        assert!(
            l.r.attr(find(&l.r, "button", ("id", id)), "disabled")
                .is_none(),
            "{id} works"
        );
    }
    assert!(l.r.html().contains("2 selected"));
    // Letting go of everything disables them again.
    l.click("input", "select-page");
    assert!(idle(&l));
}
