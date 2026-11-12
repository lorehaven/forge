//! The screens over a stand-in host: mounted into a test tree, driven by events, read back as text. The host answers every
//! request at once, so one poll of a spawned future runs it to the end.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use genevia_app::api::{Boxed, Host, HttpRequest, HttpResponse};
use genevia_app::{Setup, genevia};
use genevia_core::wire::{AncestorSlot, Ancestry, Card, Me, PersonDetail, TreeInfo};
use quench_serde::json;
use quench_ui::Event;
use quench_ui::prelude::*;
use quench_ui::router::MemoryHistory;
use quench_ui::test_renderer::NodeId;

/// A service in memory: what was asked, and what it answers.
struct Fake {
    base: String,
    asked: RefCell<Vec<(String, String)>>,
    me: Me,
    editable: bool,
}

fn card(xref: &str, name: &str, ahn_parents: bool) -> Card {
    let (given, surname) = name.split_once(' ').unwrap();
    Card {
        xref: xref.into(),
        name: name.into(),
        given: given.into(),
        surname: surname.into(),
        born: "3 wrz 1920".into(),
        died: String::new(),
        birth_year: Some(1920),
        death_year: None,
        birth_place: "Gliwice".into(),
        sex: "M".into(),
        has_parents: ahn_parents,
    }
}

fn tree() -> TreeInfo {
    TreeInfo {
        id: "kowal".into(),
        name: "Kowal".into(),
        people: 3,
        families: 1,
        root: "I1".into(),
        root_name: "Jan Kowal".into(),
    }
}

fn ancestry() -> Ancestry {
    let root = card("I1", "Jan Kowal", true);
    Ancestry {
        tree: "kowal".into(),
        root: root.clone(),
        slots: vec![
            AncestorSlot {
                ahn: 1,
                generation: 0,
                card: root,
            },
            AncestorSlot {
                ahn: 2,
                generation: 1,
                card: card("I4", "Józef Kowal", false),
            },
        ],
        depth: 1,
    }
}

fn detail(editable: bool) -> PersonDetail {
    PersonDetail {
        card: card("I1", "Jan Kowal", true),
        akas: vec![],
        nickname: String::new(),
        prefix: String::new(),
        suffix: String::new(),
        maiden_name: String::new(),
        birth: None,
        death: None,
        father: Some(card("I4", "Józef Kowal", false)),
        mother: None,
        parents_family: None,
        unions: vec![],
        facts: vec![],
        notes: vec![],
        sources: vec![],
        ahn: Some(1),
        editable,
    }
}

impl Host for Fake {
    fn send(&self, request: HttpRequest) -> Boxed<'_, Result<HttpResponse, String>> {
        self.asked
            .borrow_mut()
            .push((request.method.to_string(), request.path.clone()));
        let path = request.path.split('?').next().unwrap().to_string();
        let (status, body) = match path.as_str() {
            "/api/v1/me" => (200, json::to_string(&self.me)),
            "/api/v1/trees" => (200, json::to_string(&vec![tree()])),
            "/api/v1/trees/kowal/ancestry" => (200, json::to_string(&ancestry())),
            "/api/v1/trees/kowal/people/I1" => (200, json::to_string(&detail(self.editable))),
            "/api/v1/trees/secret/ancestry" => {
                (403, r#"{"error":"no access to this tree"}"#.to_string())
            }
            _ => (404, r#"{"error":"no such page"}"#.to_string()),
        };
        Box::pin(async move { Ok(HttpResponse { status, body }) })
    }

    fn spawn(&self, work: Boxed<'static, ()>) {
        let mut work = pin!(work);
        let mut cx = Context::from_waker(Waker::noop());
        assert!(
            matches!(work.as_mut().poll(&mut cx), Poll::Ready(())),
            "the stand-in host answers at once"
        );
    }

    fn base(&self) -> String {
        self.base.clone()
    }
}

struct Live {
    r: Rc<TestRenderer>,
    host: Rc<Fake>,
    _mount: quench_ui::Mount<TestRenderer>,
}

fn live(path: &str, base: &str, write: bool, editable: bool) -> Live {
    let host = Rc::new(Fake {
        base: base.into(),
        asked: RefCell::default(),
        me: Me {
            name: "alice".into(),
            write,
        },
        editable,
    });
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    let history = Rc::new(MemoryHistory::new(path));
    let h = host.clone();
    let (mount, _scope) = create_root(|| mount(&r, &root, genevia(Setup { history, host: h })));
    Live {
        r,
        host,
        _mount: mount,
    }
}

fn nodes_with_text(r: &TestRenderer, tag: &str, text: &str) -> Vec<NodeId> {
    r.document_order()
        .into_iter()
        .filter(|n| r.tag(*n).as_deref() == Some(tag) && r.text_of(*n).contains(text))
        .collect()
}

#[quench_test::test]
fn the_home_lists_the_trees_under_the_service_prefix_and_offers_the_import_to_writers() {
    let app = live("/genevia/ui", "/genevia", true, false);
    let text = app.r.text();
    assert!(text.contains("Kowal"), "{text}");
    assert!(text.contains("Importuj plik GEDCOM"), "{text}");
    let html = app.r.html();
    assert!(
        html.contains(r#"href="/genevia/ui/t/kowal/list""#),
        "{html}"
    );
    assert!(
        html.contains(r#"data-upload-url="/genevia/api/v1/trees""#),
        "{html}"
    );
    assert!(
        html.contains("/genevia/ui/logout"),
        "the account menu signs out through the service: {html}"
    );
}

#[quench_test::test]
fn a_reader_is_not_offered_the_import() {
    let app = live("/genevia/ui", "/genevia", false, false);
    let text = app.r.text();
    assert!(text.contains("Kowal"), "{text}");
    assert!(!text.contains("Importuj plik GEDCOM"), "{text}");
}

#[quench_test::test]
fn the_screens_work_at_the_root_of_a_host_too() {
    let app = live("/ui/t/kowal/list", "", true, false);
    let text = app.r.text();
    assert!(
        text.contains("Jan Kowal") && text.contains("Józef Kowal"),
        "{text}"
    );
    assert!(
        app.host
            .asked
            .borrow()
            .iter()
            .any(|(m, p)| m == "GET" && p.starts_with("/api/v1/trees/kowal/ancestry"))
    );
}

#[quench_test::test]
fn a_tree_shows_its_people_and_the_panel_offers_editing_only_where_it_is_allowed() {
    for (editable, expected) in [(true, true), (false, false)] {
        let app = live("/genevia/ui/t/kowal/list", "/genevia", true, editable);
        assert!(app.r.text().contains("Jan Kowal"), "{}", app.r.html());
        let row = nodes_with_text(&app.r, "tr", "Jan Kowal")
            .into_iter()
            .next()
            .expect("a row");
        app.r.fire(row, &Event::Click);
        let text = app.r.text();
        assert!(
            text.contains("Pokaż przodków tej osoby"),
            "the panel is open: {text}"
        );
        assert_eq!(
            text.contains("Edytuj osobę"),
            expected,
            "editable={editable}: {text}"
        );
        assert_eq!(
            text.contains("Usuń osobę"),
            expected,
            "editable={editable}: {text}"
        );
        assert_eq!(
            text.contains("Dodaj matkę"),
            expected,
            "editable={editable}: {text}"
        );
    }
}

#[quench_test::test]
fn the_side_panel_links_stay_under_the_service_prefix() {
    let app = live("/genevia/ui/t/kowal/pedigree", "/genevia", false, false);
    let html = app.r.html();
    assert!(html.contains(r#"href="/genevia/ui/t/kowal/fan""#), "{html}");
    assert!(html.contains(r#"href="/genevia/ui""#), "{html}");
}

#[quench_test::test]
fn a_tree_the_person_may_not_open_says_so() {
    let app = live("/genevia/ui/t/secret/list", "/genevia", false, false);
    let text = app.r.text();
    assert!(text.contains("Brak uprawnień do tego drzewa."), "{text}");
}

quench_test::main!();
