//! The screens with no host: mounted into a test tree, driven by events, read back as text.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::pedantic,
    clippy::nursery
)]

use std::cell::RefCell;
use std::rc::Rc;

use mailroom_app::screens::admin::{identities_text, parse_identities};
use mailroom_app::screens::compose::{Uploaded, prefill, upload_outcome};
use mailroom_app::screens::reader::unsubscribe_link;
use mailroom_app::{ListQuery, Mailroom, Msg, Request, Setup, mailroom};
use mailroom_core::admin::{
    AdminAccount, AuditEntry, Identity, OutboxItem, Page, Settings, Stats, Transport,
};
use mailroom_core::{
    Account, AttachmentInfo, Change, Contact, Folder, Me, MessageList, MessageSummary, MessageView,
    Preset, Step, TestResult, Upload,
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

fn account(id: &str, label: &str, address: &str) -> Account {
    Account {
        id: id.into(),
        label: label.into(),
        address: address.into(),
        username: address.into(),
        imap_host: "imap.example.com".into(),
        imap_port: 993,
        imap_security: "tls".into(),
        smtp_host: "smtp.example.com".into(),
        smtp_port: 465,
        smtp_security: "tls".into(),
        state: "ok".into(),
        last_error: None,
        last_sync: Some(NOW - 600),
        unread: 2,
        signature: "Ann".into(),
        custom_certificate: false,
        save_sent: true,
    }
}

fn folder(id: &str, account: &str, role: &str, name: &str, unread: u32) -> Folder {
    Folder {
        id: id.into(),
        account_id: account.into(),
        path: name.into(),
        name: name.into(),
        role: role.into(),
        unread,
        total: 10,
    }
}

fn message(id: &str, subject: &str, seen: bool) -> MessageSummary {
    MessageSummary {
        id: id.into(),
        account_id: "a1".into(),
        folder_id: "f1".into(),
        subject: subject.into(),
        from: Contact {
            name: "Alice".into(),
            address: "alice@x.example".into(),
        },
        to: vec![Contact {
            name: String::new(),
            address: "me@example.com".into(),
        }],
        date: NOW - 3600,
        preview: "the first words".into(),
        seen,
        flagged: false,
        answered: false,
        attachments: false,
        size: 1200,
    }
}

fn open_view(id: &str, seen: bool) -> MessageView {
    MessageView {
        summary: message(id, "Hello there", seen),
        cc: vec![Contact {
            name: "Carol".into(),
            address: "carol@x.example".into(),
        }],
        reply_to: vec![],
        message_id: "<m1@x.example>".into(),
        text: "line one\nline two".into(),
        has_html: false,
        remote_images: 0,
        attachments: vec![],
        list_unsubscribe: String::new(),
    }
}

fn preset() -> Preset {
    Preset {
        id: "gmail".into(),
        name: "Gmail".into(),
        imap_host: "imap.gmail.com".into(),
        imap_port: 993,
        imap_security: "tls".into(),
        smtp_host: "smtp.gmail.com".into(),
        smtp_port: 465,
        smtp_security: "tls".into(),
        note: "Create an app password first.".into(),
        save_sent: false,
    }
}

fn start(l: &Live, admin: bool) {
    l.send(Msg::Me(Me {
        username: "ann".into(),
        can_write: true,
        admin,
        max_accounts: 10,
    }));
    l.send(Msg::Presets(vec![preset()]));
    l.send(Msg::Accounts(vec![
        account("a1", "Personal", "me@example.com"),
        account("a2", "Work", "ann@work.example"),
    ]));
    l.send(Msg::Folders {
        account: "a1".into(),
        folders: vec![
            folder("f1", "a1", "inbox", "INBOX", 2),
            folder("f2", "a1", "sent", "Sent", 0),
            folder("f3", "a1", "trash", "Trash", 5),
        ],
    });
    l.send(Msg::Folders {
        account: "a2".into(),
        folders: vec![folder("g1", "a2", "inbox", "INBOX", 0)],
    });
}

fn list(items: Vec<MessageSummary>, total: u32, offset: u32) -> MessageList {
    MessageList {
        items,
        total,
        offset,
    }
}

// -- the shell ---------------------------------------------------------------

#[quench_test::test]
fn the_app_starts_by_asking_who_and_what() {
    let l = live("/mailroom/ui/home");
    assert_eq!(l.asked()[0], Request::Startup);
    assert!(matches!(&l.asked()[1], Request::List(q) if q.role.as_deref() == Some("inbox")));
    assert!(present(&l.r, "waiting"));
}

#[quench_test::test]
fn the_sidebar_lists_mailboxes_and_folders_with_unread_counts() {
    let l = live("/mailroom/ui/home");
    start(&l, false);
    // The folders of each mailbox were asked for as the mailboxes arrived.
    let asked = l.asked();
    assert!(
        asked.contains(&Request::Folders("a1".into()))
            && asked.contains(&Request::Folders("a2".into()))
    );
    let text = l.text();
    for needle in ["Personal", "Work", "Inbox", "Sent", "Trash"] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    let inbox = find(&l.r, "a", ("data-folder", "f1"));
    assert_eq!(
        l.r.attr(inbox, "href").as_deref(),
        Some("/mailroom/ui/folder/f1")
    );
    assert!(
        l.r.text_of(inbox).contains('2'),
        "the unread count is shown"
    );
    // Trash is not shouted about: its unread count stays out.
    assert!(
        !l.r.text_of(find(&l.r, "a", ("data-folder", "f3")))
            .contains('5')
    );
    assert!(present(&l.r, "nav-compose") && present(&l.r, "nav-accounts"));
    assert!(
        !present(&l.r, "nav-admin"),
        "no administration for an ordinary person"
    );
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

// -- lists ---------------------------------------------------------------

#[quench_test::test]
fn the_unified_inbox_lists_messages_newest_first_with_unread_in_bold() {
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
    assert_eq!(
        l.r.text_of(find(&l.r, "h1", ("id", "list-title"))),
        "All inboxes"
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
    let l = live("/mailroom/ui/folder/f2");
    start(&l, false);
    assert!(l.asked().contains(&Request::List(ListQuery {
        folder: Some("f2".into()),
        ..ListQuery::default()
    })));
    assert_eq!(
        l.r.text_of(find(&l.r, "h1", ("id", "list-title"))),
        "Personal · Sent"
    );
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
            role: Some("inbox".into()),
            text: "invoice".into(),
            ..ListQuery::default()
        })
    );
    // Unread only keeps the text.
    l.click("button", "unread-toggle");
    assert_eq!(
        l.last(),
        Request::List(ListQuery {
            folder: None,
            role: Some("inbox".into()),
            text: "invoice".into(),
            unread: true,
            offset: 0
        })
    );
    // The next page keeps both.
    l.click("button", "older");
    assert_eq!(
        l.last(),
        Request::List(ListQuery {
            folder: None,
            role: Some("inbox".into()),
            text: "invoice".into(),
            unread: true,
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
    assert!(after[before..].contains(&Request::Accounts));
}

#[quench_test::test]
fn refresh_asks_the_mailboxes_of_the_folder_or_all_of_them() {
    let l = live("/mailroom/ui/home");
    start(&l, false);
    l.click("button", "refresh");
    let asked = l.asked();
    assert!(
        asked.contains(&Request::Refresh("a1".into()))
            && asked.contains(&Request::Refresh("a2".into()))
    );
    let l = live("/mailroom/ui/folder/g1");
    start(&l, false);
    l.click("button", "refresh");
    let refreshes: Vec<_> = l
        .asked()
        .into_iter()
        .filter(|r| matches!(r, Request::Refresh(_)))
        .collect();
    assert_eq!(refreshes, [Request::Refresh("a2".into())]);
}

// -- the reader ---------------------------------------------------------------

#[quench_test::test]
fn opening_a_message_shows_it_and_marks_it_read() {
    let l = live("/mailroom/ui/message/m1");
    assert_eq!(l.asked()[1], Request::Open("m1".into()));
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Message(Box::new(open_view("m1", false))));
    let text = l.text();
    for needle in [
        "Hello there",
        "Alice <alice@x.example>",
        "Carol <carol@x.example>",
        "line one",
        "2026-09-29 11:00",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    assert_eq!(
        l.last(),
        Request::Change(Change {
            ids: vec!["m1".into()],
            seen: Some(true),
            ..Change::default()
        })
    );
    // A message that is read already is not marked again.
    let l = live("/mailroom/ui/message/m1");
    let before = l.asked().len();
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    assert_eq!(l.asked().len(), before);
}

#[quench_test::test]
fn the_reader_acts_on_the_message() {
    let l = live("/mailroom/ui/message/m1");
    start(&l, false);
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    let change = |c: Change| Request::Change(c);
    l.click_attr("button", ("data-action", "star"));
    assert_eq!(
        l.last(),
        change(Change {
            ids: vec!["m1".into()],
            flagged: Some(true),
            ..Change::default()
        })
    );
    l.click_attr("button", ("data-action", "unread"));
    assert_eq!(
        l.last(),
        change(Change {
            ids: vec!["m1".into()],
            seen: Some(false),
            ..Change::default()
        })
    );
    l.click_attr("button", ("data-action", "archive"));
    assert_eq!(
        l.last(),
        change(Change {
            ids: vec!["m1".into()],
            move_to: Some("archive".into()),
            ..Change::default()
        })
    );
    l.click_attr("button", ("data-action", "junk"));
    assert_eq!(
        l.last(),
        change(Change {
            ids: vec!["m1".into()],
            move_to: Some("junk".into()),
            ..Change::default()
        })
    );
    l.click_attr("button", ("data-action", "delete"));
    assert_eq!(
        l.last(),
        change(Change {
            ids: vec!["m1".into()],
            move_to: Some("trash".into()),
            ..Change::default()
        })
    );
    // Reply, reply to all and forward are pages of their own.
    for (mode, href) in [
        ("reply", "/mailroom/ui/compose/reply/m1"),
        ("replyall", "/mailroom/ui/compose/replyall/m1"),
        ("forward", "/mailroom/ui/compose/forward/m1"),
    ] {
        assert_eq!(
            l.r.attr(find(&l.r, "a", ("data-action", mode)), "href")
                .as_deref(),
            Some(href)
        );
    }
}

#[quench_test::test]
fn in_the_trash_delete_is_for_good() {
    let l = live("/mailroom/ui/message/m1");
    start(&l, false);
    let mut view = open_view("m1", true);
    view.summary.folder_id = "f3".into();
    l.send(Msg::Message(Box::new(view)));
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
fn a_move_leaves_the_message_for_the_list() {
    let l = live("/mailroom/ui/message/m1");
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    l.send(Msg::Moved);
    assert_eq!(l.path(), "/mailroom/ui/home");
}

#[quench_test::test]
fn html_is_shown_only_in_a_sandboxed_frame_and_images_are_the_readers_choice() {
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
    // The plain text is one click away.
    l.click("button", "switch-view");
    assert!(present(&l.r, "message-text"));
    assert!(!present(&l.r, "message-html"));
}

#[quench_test::test]
fn attachments_are_downloads_and_inline_pictures_are_not_listed() {
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.attachments = vec![
        AttachmentInfo {
            index: 0,
            filename: "report.pdf".into(),
            media_type: "application/pdf".into(),
            size: 340 * 1024,
            inline: false,
        },
        AttachmentInfo {
            index: 1,
            filename: "logo.png".into(),
            media_type: "image/png".into(),
            size: 100,
            inline: true,
        },
    ];
    l.send(Msg::Message(Box::new(view)));
    let link = find(&l.r, "a", ("download", "report.pdf"));
    assert_eq!(
        l.r.attr(link, "href").as_deref(),
        Some("/mailroom/api/v1/messages/m1/attachments/0")
    );
    assert!(l.text().contains("report.pdf (340 KB)"));
    assert!(!l.text().contains("logo.png"));
}

#[quench_test::test]
fn the_unsubscribe_link_is_https_only() {
    assert_eq!(
        unsubscribe_link("<mailto:u@x.example>, <https://x.example/u?id=1>").as_deref(),
        Some("https://x.example/u?id=1")
    );
    assert_eq!(unsubscribe_link("<http://x.example/u>"), None);
    assert_eq!(unsubscribe_link("<javascript:alert(1)>"), None);
    assert_eq!(unsubscribe_link("<https://x.example/ u>"), None);
    assert_eq!(unsubscribe_link(""), None);
    let l = live("/mailroom/ui/message/m1");
    let mut view = open_view("m1", true);
    view.list_unsubscribe = "<https://x.example/u>".into();
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

// -- compose --------------------------------------------------------------------

#[quench_test::test]
fn composing_needs_a_mailbox() {
    let l = live("/mailroom/ui/compose");
    l.send(Msg::Accounts(vec![]));
    assert!(present(&l.r, "no-mailbox"));
}

#[quench_test::test]
fn a_new_message_is_checked_then_sent_from_the_chosen_mailbox() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    // Nothing to send yet.
    l.submit("compose");
    assert!(l.text().contains("add at least one recipient"));
    assert!(!l.asked().iter().any(|r| matches!(r, Request::Send(_))));
    l.type_into("input", "compose-to", "not an address");
    l.submit("compose");
    assert!(l.text().contains("not an email address"));
    l.type_into(
        "input",
        "compose-to",
        "bob@example.org, Carol <carol@example.org>",
    );
    l.type_into("input", "compose-subject", "Plans");
    l.type_into("textarea", "compose-body", "See you friday.");
    l.type_into("select", "compose-from", "a2");
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!("not sent: {:?}", l.asked())
    };
    assert_eq!(compose.account_id, "a2");
    assert_eq!(compose.to, ["bob@example.org", "Carol <carol@example.org>"]);
    assert_eq!(
        (compose.subject.as_str(), compose.text.as_str()),
        ("Plans", "See you friday.")
    );
    assert!(compose.in_reply_to.is_none());
}

#[quench_test::test]
fn sending_returns_to_the_inbox_and_says_so() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    l.send(Msg::Sent);
    assert_eq!(l.path(), "/mailroom/ui/home");
    assert!(l.text().contains("The message is on its way."));
}

#[quench_test::test]
fn a_reply_starts_from_the_message_and_is_threaded() {
    let l = live("/mailroom/ui/compose/reply/m1");
    start(&l, false);
    assert!(l.asked().contains(&Request::Open("m1".into())));
    assert!(present(&l.r, "waiting"));
    l.send(Msg::Message(Box::new(open_view("m1", true))));
    let to = find(&l.r, "input", ("id", "compose-to"));
    assert_eq!(
        l.r.attr(to, "value").as_deref(),
        Some("Alice <alice@x.example>")
    );
    let subject = find(&l.r, "input", ("id", "compose-subject"));
    assert_eq!(
        l.r.attr(subject, "value").as_deref(),
        Some("Re: Hello there")
    );
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!()
    };
    assert_eq!(compose.in_reply_to.as_deref(), Some("m1"));
    assert_eq!(compose.account_id, "a1");
    assert!(compose.text.contains("> line one"), "{}", compose.text);
}

#[quench_test::test]
fn the_pure_prefill_rules() {
    let mut view = open_view("m1", true);
    view.summary.to = vec![
        Contact {
            name: String::new(),
            address: "me@example.com".into(),
        },
        Contact {
            name: "Dave".into(),
            address: "dave@x.example".into(),
        },
    ];
    let me = vec!["me@example.com".to_string()];
    let reply = prefill("reply", Some(&view), &me, 0);
    assert_eq!(reply.to, "Alice <alice@x.example>");
    assert!(reply.cc.is_empty());
    let all = prefill("replyall", Some(&view), &me, 0);
    assert_eq!(all.to, "Alice <alice@x.example>");
    assert_eq!(
        all.cc, "Dave <dave@x.example>, Carol <carol@x.example>",
        "not the writer's own address"
    );
    let forward = prefill("forward", Some(&view), &me, 0);
    assert_eq!(forward.subject, "Fwd: Hello there");
    assert!(
        forward.body.contains("line one")
            && forward.body.contains("Forwarded message")
            && forward.in_reply_to.is_none()
    );
    view.reply_to = vec![Contact {
        name: String::new(),
        address: "list@x.example".into(),
    }];
    assert_eq!(prefill("reply", Some(&view), &me, 0).to, "list@x.example");
    assert_eq!(
        prefill("new", None, &me, 0),
        mailroom_app::screens::compose::Prefill::default()
    );
}

#[quench_test::test]
fn what_the_upload_control_reports_is_read() {
    let ok = r#"{"name":"a.txt","status":201,"body":"{\"id\":\"u1\",\"filename\":\"a.txt\",\"size\":5}"}"#;
    assert_eq!(
        upload_outcome(ok),
        Uploaded::Done(Upload {
            id: "u1".into(),
            filename: "a.txt".into(),
            size: 5
        })
    );
    let refused = r#"{"name":"a.txt","status":400,"body":"{\"error\":\"the file is too large\"}"}"#;
    assert_eq!(
        upload_outcome(refused),
        Uploaded::Failed("the file is too large".into())
    );
    assert!(
        matches!(upload_outcome(r#"{"name":"a","status":0,"body":"Failed to fetch"}"#), Uploaded::Failed(t) if t.contains("(0)"))
    );
    assert!(matches!(upload_outcome("not json"), Uploaded::Failed(_)));
    assert!(matches!(
        upload_outcome(r#"{"status":201,"body":"junk"}"#),
        Uploaded::Failed(_)
    ));
}

#[quench_test::test]
fn an_uploaded_file_is_listed_removable_and_sent_with_the_message() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    let report = find(&l.r, "input", ("id", "upload-result"));
    let picker = find(&l.r, "input", ("id", "attach"));
    assert_eq!(
        l.r.attr(picker, "data-upload-url").as_deref(),
        Some("/mailroom/api/v1/uploads")
    );
    assert_eq!(
        l.r.attr(picker, "data-upload-result").as_deref(),
        Some("upload-result")
    );
    l.r.fire(report, &Event::Input(r#"{"name":"a.txt","status":201,"body":"{\"id\":\"u1\",\"filename\":\"a.txt\",\"size\":2048}"}"#.into()));
    assert!(l.text().contains("a.txt (2 KB)"));
    l.type_into("input", "compose-to", "bob@example.org");
    l.type_into("textarea", "compose-body", "see attached");
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!()
    };
    assert_eq!(compose.attachments, ["u1"]);
    // Taking it off asks the service to forget it.
    l.click_attr("button", ("aria-label", "Remove the attachment"));
    assert!(l.asked().contains(&Request::RemoveUpload("u1".into())));
    assert!(!l.text().contains("a.txt"));
    // A refused upload is said.
    l.r.fire(
        report,
        &Event::Input(r#"{"name":"b","status":400,"body":"{\"error\":\"no room\"}"}"#.into()),
    );
    assert!(l.text().contains("no room"));
}

#[quench_test::test]
fn the_extra_address_fields_open_on_request() {
    let l = live("/mailroom/ui/compose");
    start(&l, false);
    assert!(
        !present(&l.r, "compose-cc")
            || l.r
                .document_order()
                .iter()
                .all(|n| l.r.attr(*n, "id").as_deref() != Some("compose-cc"))
    );
    l.click("button", "more-fields");
    l.type_into("input", "compose-cc", "cc@example.org");
    l.type_into("input", "compose-bcc", "bcc@example.org");
    l.type_into("input", "compose-to", "to@example.org");
    l.type_into("textarea", "compose-body", "x");
    l.submit("compose");
    let Request::Send(compose) = l.last() else {
        panic!()
    };
    assert_eq!(
        (compose.cc, compose.bcc),
        (
            vec!["cc@example.org".to_string()],
            vec!["bcc@example.org".to_string()]
        )
    );
}

// -- mailboxes -----------------------------------------------------------------

#[quench_test::test]
fn the_mailboxes_page_shows_each_one_and_how_it_is_doing() {
    let l = live("/mailroom/ui/accounts");
    assert_eq!(l.asked()[1], Request::Accounts);
    let mut broken = account("a2", "Work", "ann@work.example");
    broken.state = "error".into();
    broken.last_error = Some("the mail server refused the login".into());
    l.send(Msg::Accounts(vec![
        account("a1", "Personal", "me@example.com"),
        broken,
    ]));
    let text = l.text();
    for needle in [
        "Personal · me@example.com",
        "imap.example.com:993",
        "Working",
        "Problem",
        "the mail server refused the login",
        "2026-09-29 11:50",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    l.send(Msg::Accounts(vec![]));
    assert!(present(&l.r, "no-accounts"));
}

#[quench_test::test]
fn a_provider_fills_the_servers_and_a_good_form_is_sent() {
    let l = live("/mailroom/ui/accounts");
    start(&l, false);
    l.type_into("select", "account-preset", "gmail");
    assert!(l.text().contains("Create an app password first."));
    assert_eq!(
        l.r.attr(find(&l.r, "input", ("id", "account-imap-host")), "value")
            .as_deref(),
        Some("imap.gmail.com")
    );
    // An incomplete form is not sent.
    l.submit("account-form");
    assert!(l.text().contains("not an email address"));
    assert!(
        !l.asked()
            .iter()
            .any(|r| matches!(r, Request::AddAccount(_)))
    );
    l.type_into("input", "account-label", "Gmail");
    l.type_into("input", "account-address", "ann@gmail.com");
    l.type_into("input", "account-password", "abcd efgh ijkl mnop");
    l.submit("account-form");
    let Request::AddAccount(input) = l.last() else {
        panic!("{:?}", l.last())
    };
    assert_eq!(
        (
            input.imap_host.as_str(),
            input.imap_port,
            input.smtp_host.as_str(),
            input.smtp_port
        ),
        ("imap.gmail.com", 993, "smtp.gmail.com", 465)
    );
    assert_eq!(
        (input.username.as_str(), input.password.as_str()),
        ("ann@gmail.com", "abcd efgh ijkl mnop")
    );
    assert!(!input.save_sent, "Gmail keeps its own copy");
}

#[quench_test::test]
fn the_settings_can_be_tried_and_the_result_is_shown_per_half() {
    let l = live("/mailroom/ui/accounts");
    start(&l, false);
    l.type_into("input", "account-label", "Work");
    l.type_into("input", "account-address", "ann@work.example");
    l.type_into("input", "account-password", "pw");
    l.type_into("input", "account-imap-host", "imap.work.example");
    l.type_into("input", "account-smtp-host", "smtp.work.example");
    l.click("button", "test-account");
    assert!(matches!(l.last(), Request::TestAccount(i) if i.imap_host == "imap.work.example"));
    l.send(Msg::Tested(Box::new(TestResult {
        imap: Step {
            ok: true,
            error: String::new(),
        },
        smtp: Step {
            ok: false,
            error: "authentication failed".into(),
        },
    })));
    let text = l.text();
    assert!(
        text.contains("IMAP: works") && text.contains("SMTP: failed — authentication failed"),
        "{text}"
    );
}

#[quench_test::test]
fn removing_a_mailbox_takes_two_clicks() {
    let l = live("/mailroom/ui/accounts");
    start(&l, false);
    l.click_attr("button", ("data-action", "remove"));
    assert!(
        !l.asked()
            .iter()
            .any(|r| matches!(r, Request::RemoveAccount(_)))
    );
    assert!(l.text().contains("Really remove?"));
    l.click_attr("button", ("data-action", "remove"));
    assert_eq!(l.last(), Request::RemoveAccount("a1".into()));
    l.send(Msg::AccountRemoved("a1".into()));
    assert!(!l.text().contains("me@example.com"));
}

#[quench_test::test]
fn changing_a_mailbox_starts_from_what_it_is_and_may_keep_the_password() {
    let l = live("/mailroom/ui/accounts");
    start(&l, false);
    l.click_attr("button", ("data-action", "edit"));
    assert_eq!(
        l.r.attr(find(&l.r, "input", ("id", "account-address")), "value")
            .as_deref(),
        Some("me@example.com")
    );
    assert!(l.text().contains("leave empty to keep it"));
    l.type_into("input", "account-label", "Renamed");
    l.submit("account-form");
    let Request::UpdateAccount { id, input } = l.last() else {
        panic!("{:?}", l.last())
    };
    assert_eq!(
        (id.as_str(), input.label.as_str(), input.password.as_str()),
        ("a1", "Renamed", "")
    );
}

#[quench_test::test]
fn a_saved_mailbox_is_said_and_asked_again() {
    let l = live("/mailroom/ui/accounts");
    let before = l.asked().len();
    l.send(Msg::AccountSaved(Box::new(account(
        "a3",
        "New",
        "new@example.com",
    ))));
    assert!(l.text().contains("The mailbox is saved."));
    assert!(l.asked()[before..].contains(&Request::Accounts));
}

#[quench_test::test]
fn a_refused_change_is_shown_until_the_next_page() {
    let l = live("/mailroom/ui/accounts");
    l.send(Msg::Rejected(
        "the mail server refused the login: check the password".into(),
    ));
    assert!(present(&l.r, "notice") && l.text().contains("check the password"));
}

// -- the panel -------------------------------------------------------------------

#[quench_test::test]
fn the_panel_is_not_offered_to_everybody() {
    let l = live("/mailroom/ui/admin");
    l.send(Msg::Me(Me {
        username: "ann".into(),
        can_write: true,
        admin: false,
        max_accounts: 10,
    }));
    assert!(present(&l.r, "not-allowed"));
    assert!(!l.asked().contains(&Request::Stats));
}

#[quench_test::test]
fn the_overview_shows_the_numbers() {
    let l = live("/mailroom/ui/admin");
    start(&l, true);
    assert!(l.asked().contains(&Request::Stats));
    l.send(Msg::Stats(Stats {
        accounts: 4,
        accounts_failing: 1,
        messages: 1234,
        queued: 2,
        sent_24h: 40,
        failed_24h: 3,
        budget_left: 460,
    }));
    for (key, value) in [
        ("mr_admin_stat_accounts", "4"),
        ("mr_admin_stat_failing", "1"),
        ("mr_admin_stat_messages", "1234"),
        ("mr_admin_stat_budget", "460"),
    ] {
        let card = find(&l.r, "div", ("data-stat", key));
        assert!(
            l.r.text_of(card).starts_with(value),
            "{key}: {}",
            l.r.text_of(card)
        );
    }
    assert_eq!(
        l.r.attr(find(&l.r, "a", ("data-section", "queue")), "href")
            .as_deref(),
        Some("/mailroom/ui/admin/queue")
    );
}

fn settings() -> Settings {
    Settings {
        transport: Some(Transport {
            host: "smtp.relay.example".into(),
            port: 587,
            security: "starttls".into(),
            username: "relay".into(),
            has_password: true,
            custom_certificate: false,
        }),
        identities: vec![
            Identity {
                address: "noreply@example.com".into(),
                name: "Forge".into(),
                default: true,
            },
            Identity {
                address: "alerts@example.com".into(),
                name: String::new(),
                default: false,
            },
        ],
        private_hosts: vec!["bridge.mail.svc".into()],
        extra_ports: vec![1143],
        daily_budget: 500,
        hourly_per_source: 120,
        max_accounts_per_user: 10,
        sync_interval_secs: 120,
    }
}

#[quench_test::test]
fn the_settings_form_shows_what_is_set_and_saves_what_was_changed() {
    let l = live("/mailroom/ui/admin/settings");
    start(&l, true);
    assert!(l.asked().contains(&Request::Settings));
    l.send(Msg::Settings(Box::new(settings())));
    assert_eq!(
        l.r.attr(find(&l.r, "input", ("id", "transport-host")), "value")
            .as_deref(),
        Some("smtp.relay.example")
    );
    assert!(
        l.text().contains("leave empty to keep it"),
        "a stored password is kept, not shown"
    );
    l.type_into("input", "daily-budget", "900");
    l.type_into(
        "textarea",
        "private-hosts",
        "bridge.mail.svc\n*.internal.example",
    );
    l.type_into("input", "extra-ports", "1143, 1025");
    l.submit("settings-form");
    let Request::SaveSettings(input) = l.last() else {
        panic!("{:?}", l.last())
    };
    assert_eq!(input.daily_budget, Some(900));
    assert_eq!(
        input.private_hosts,
        Some(vec![
            "bridge.mail.svc".to_string(),
            "*.internal.example".to_string()
        ])
    );
    assert_eq!(input.extra_ports, Some(vec![1143, 1025]));
    assert_eq!(
        input
            .transport
            .as_ref()
            .map(|t| (t.host.as_str(), t.port, t.password.as_str())),
        Some(("smtp.relay.example", 587, ""))
    );
    assert_eq!(
        input.identities.as_ref().map(|i| (i.len(), i[0].default)),
        Some((2, true))
    );
    // A number that is not a number is not sent.
    l.type_into("input", "daily-budget", "lots");
    let sent = l.asked().len();
    l.submit("settings-form");
    assert_eq!(l.asked().len(), sent);
    assert!(l.text().contains("whole numbers"));
}

#[quench_test::test]
fn the_sending_server_can_be_tried_and_saving_is_said() {
    let l = live("/mailroom/ui/admin/settings");
    start(&l, true);
    l.send(Msg::Settings(Box::new(settings())));
    l.click("button", "test-transport");
    assert_eq!(l.last(), Request::TestTransport);
    l.send(Msg::TransportTested(Box::new(TestResult {
        imap: Step {
            ok: true,
            error: String::new(),
        },
        smtp: Step {
            ok: false,
            error: "authentication failed".into(),
        },
    })));
    assert!(l.text().contains("failed — authentication failed"));
    l.send(Msg::SettingsSaved(Box::new(settings())));
    assert!(l.text().contains("The settings are saved."));
}

#[quench_test::test]
fn sender_addresses_are_written_and_read_as_lines() {
    let list = settings().identities;
    let text = identities_text(&list);
    assert_eq!(text, "* Forge <noreply@example.com>\nalerts@example.com");
    assert_eq!(parse_identities(&text), list);
    assert_eq!(parse_identities("  \n"), vec![]);
    assert_eq!(parse_identities("\"Q, A\" <a@x.example>")[0].name, "Q, A");
}

#[quench_test::test]
fn the_panel_lists_mailboxes_without_mail_and_can_switch_them_off() {
    let l = live("/mailroom/ui/admin/mailboxes");
    start(&l, true);
    assert!(l.asked().contains(&Request::AdminAccounts { offset: 0 }));
    let row = |id: &str, disabled: bool| AdminAccount {
        id: id.into(),
        owner: "ann".into(),
        label: "Personal".into(),
        address: "me@example.com".into(),
        imap_host: "imap.example.com".into(),
        state: if disabled {
            "disabled".into()
        } else {
            "ok".into()
        },
        last_error: None,
        last_sync: None,
        messages: 120,
        disabled,
    };
    l.send(Msg::AdminAccounts(Page {
        items: vec![row("a1", false), row("a2", true)],
        total: 2,
    }));
    let first = find(&l.r, "tr", ("data-account", "a1"));
    assert!(l.r.text_of(first).contains("ann") && l.r.text_of(first).contains("120"));
    l.click_attr("button", ("data-action", "disable"));
    assert_eq!(
        l.last(),
        Request::SetDisabled {
            id: "a1".into(),
            disabled: true
        }
    );
    l.click_attr("button", ("data-action", "enable"));
    assert_eq!(
        l.last(),
        Request::SetDisabled {
            id: "a2".into(),
            disabled: false
        }
    );
    // Deleting takes two clicks.
    l.click_attr("button", ("data-action", "delete"));
    assert!(
        !l.asked()
            .iter()
            .any(|r| matches!(r, Request::AdminRemoveAccount(_)))
    );
    l.click_attr("button", ("data-action", "delete"));
    assert!(matches!(l.last(), Request::AdminRemoveAccount(id) if id == "a1"));
}

#[quench_test::test]
fn the_queue_can_be_filtered_retried_and_cleared() {
    let l = live("/mailroom/ui/admin/queue");
    start(&l, true);
    assert!(l.asked().contains(&Request::Outbox {
        status: String::new(),
        offset: 0
    }));
    let item = |id: &str, status: &str| OutboxItem {
        id: id.into(),
        source: "gatehouse-mailroom".into(),
        kind: "service".into(),
        recipients: vec!["a@example.org".into()],
        subject: "Confirm your address".into(),
        status: status.into(),
        attempts: 2,
        last_error: Some("451 try later".into()),
        created: NOW - 60,
        next_attempt: None,
        sent: None,
    };
    l.send(Msg::Outbox(Page {
        items: vec![item("o1", "queued"), item("o2", "failed")],
        total: 2,
    }));
    let text = l.text();
    assert!(
        text.contains("gatehouse-mailroom (service)")
            && text.contains("Confirm your address")
            && text.contains("451 try later"),
        "{text}"
    );
    // Only a queued message can be retried.
    l.click_attr("button", ("data-action", "retry"));
    assert_eq!(l.last(), Request::Retry("o1".into()));
    assert_eq!(
        l.r.document_order()
            .into_iter()
            .filter(|n| l.r.attr(*n, "data-action").as_deref() == Some("retry"))
            .count(),
        1
    );
    l.click_attr("button", ("data-status", "failed"));
    assert_eq!(
        l.last(),
        Request::Outbox {
            status: "failed".into(),
            offset: 0
        }
    );
    l.click_attr("button", ("data-action", "remove"));
    assert!(matches!(l.last(), Request::RemoveQueued(_)));
}

#[quench_test::test]
fn the_audit_log_lists_what_was_done() {
    let l = live("/mailroom/ui/admin/audit");
    start(&l, true);
    assert!(l.asked().contains(&Request::Audit { offset: 0 }));
    l.send(Msg::Audit(Page {
        items: vec![AuditEntry {
            id: "x".into(),
            at: NOW - 30,
            actor: "adam".into(),
            action: "settings.update".into(),
            target: String::new(),
            detail: "daily_budget".into(),
        }],
        total: 1,
    }));
    let text = l.text();
    assert!(
        text.contains("adam") && text.contains("settings.update") && text.contains("daily_budget"),
        "{text}"
    );
    assert_eq!(count(&l.r, "data-queued"), 0);
}

#[quench_test::test]
fn the_language_choice_is_handed_to_the_host() {
    let l = live("/mailroom/ui/home");
    let _ = &l;
    // The host keeps the choice itself; the app only forwards it (the picker is the shell's).
    assert!(l.asked().iter().all(|r| !matches!(r, Request::Language(_))));
}

quench_test::main!();
