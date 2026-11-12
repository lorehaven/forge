//! The home page: the trees there are, and the way to add one by importing a GEDCOM file.

use crate::api::{SIGNED_OUT, list_trees, me};
use genevia_core::wire::TreeInfo;
use quench_serde::{Deserialize, json};
use quench_ui::prelude::*;
use quench_ui::router::link;

use crate::state::{Load, resource};
use crate::util::url;

/// What the shell's upload script reports back: the file, the HTTP status and the body.
#[derive(Debug, Deserialize)]
struct Report {
    name: String,
    status: i32,
    body: String,
}

#[derive(Debug, Default, Deserialize)]
struct Reply {
    #[serde(default)]
    name: String,
    #[serde(default)]
    people: u32,
    #[serde(default)]
    families: u32,
    #[serde(default)]
    replaced: bool,
    #[serde(default)]
    error: String,
}

/// A line to show under the import button.
#[derive(Clone, Debug, PartialEq)]
enum Outcome {
    Idle,
    Working,
    Done(String),
    Failed(String),
}

fn outcome_of(raw: &str) -> Outcome {
    let Ok(report) = json::from_str::<Report>(raw) else {
        return Outcome::Failed("Nie udało się odczytać odpowiedzi serwera.".to_string());
    };
    let reply = json::from_str::<Reply>(&report.body).unwrap_or_default();
    if report.status == 200 {
        let verb = if reply.replaced {
            "Zaktualizowano"
        } else {
            "Zaimportowano"
        };
        Outcome::Done(format!(
            "{verb} „{}”: {} osób, {} rodzin.",
            reply.name, reply.people, reply.families
        ))
    } else if !reply.error.is_empty() {
        Outcome::Failed(format!("{}: {}", report.name, reply.error))
    } else {
        Outcome::Failed(format!(
            "{}: serwer odpowiedział kodem {}.",
            report.name, report.status
        ))
    }
}

fn card(t: &TreeInfo) -> View {
    link(&url(&t.id, "list", ""))
        .class("fui-link-card fui-link-card-stack")
        .child(
            span()
                .class("fui-link-card-icon")
                .child(forge_shell::icon("users")),
        )
        .child(
            div()
                .class("fui-link-card-body")
                .child(div().class("fui-link-card-title").text(t.name.clone()))
                .child(div().class("fui-link-card-desc").text(format!(
                    "{} osób · {} rodzin · {}",
                    t.people, t.families, t.root_name
                ))),
        )
        .child(
            span()
                .class("fui-link-card-arrow")
                .child(forge_shell::icon("arrow-right")),
        )
        .into_view()
}

/// The import control. The file is sent by the forge shell's script (`data-upload-url`), which
/// reports the outcome by writing it into the hidden input and raising `input` on it.
fn import_control(outcome: Signal<Outcome>, reload: Signal<u32>) -> View {
    let (started, finished) = (outcome.clone(), outcome.clone());
    let report = input()
        .attr("type", "hidden")
        .attr("id", "import-result")
        .on_input(move |raw| {
            let result = outcome_of(raw);
            if matches!(result, Outcome::Done(_)) {
                reload.update(|n| *n += 1);
            }
            finished.set(result);
        });
    let picker = input()
        .class("sr-only")
        .attr("type", "file")
        .attr("accept", ".ged,.GED")
        .attr(
            "data-upload-url",
            format!("{}/api/v1/trees", crate::api::base()),
        )
        .attr("data-upload-result", "import-result")
        .on_change(move |_| started.set(Outcome::Working));
    div()
        .class("import")
        .child(
            element("label")
                .class("fui-btn fui-btn-primary")
                .child(forge_shell::icon("plus"))
                .child(span().text("Importuj plik GEDCOM"))
                .child(picker),
        )
        .child(report)
        .child(View::dynamic(move || match outcome.get() {
            Outcome::Idle => View::Empty,
            Outcome::Working => p().class("fui-muted").text("Importowanie…").into_view(),
            Outcome::Done(text) => p().class("fui-notice fui-notice-ok").text(text).into_view(),
            Outcome::Failed(text) => p()
                .class("fui-notice fui-notice-error")
                .text(text)
                .into_view(),
        }))
        .into_view()
}

/// What a failed load says, and for a lapsed session the way back in.
fn failure(text: &str) -> View {
    let mut line = p().class("error").text(text.to_string());
    if text == SIGNED_OUT {
        line = line.child(
            a().attr("href", format!("{}/ui/login", crate::api::base()))
                .class("fui-link")
                .text(" Zaloguj się"),
        );
    }
    line.into_view()
}

#[must_use]
pub fn home() -> View {
    let reload = create_signal(0_u32);
    let outcome = create_signal(Outcome::Idle);
    let trees = resource(
        {
            let reload = reload.clone();
            move || reload.get()
        },
        |_| async { list_trees().await.map_err(|e| e.message().to_string()) },
    );
    // Whether the person may add trees: the import control is theirs alone.
    let me = resource(
        || (),
        |()| async { me().await.map_err(|e| e.message().to_string()) },
    );
    div()
        .class("home fui-client-main")
        .child(h1().text("Drzewa genealogiczne"))
        .child(View::dynamic(move || match me.get() {
            Load::Ready(m) if m.write => div()
                .child(
                    p().class("fui-muted")
                        .text("Wybierz drzewo albo zaimportuj nowe z pliku GEDCOM."),
                )
                .child(import_control(outcome.clone(), reload.clone()))
                .into_view(),
            _ => p().class("fui-muted").text("Wybierz drzewo.").into_view(),
        }))
        .child(View::dynamic(move || match trees.get() {
            Load::Loading => p().class("fui-muted").text("Wczytywanie…").into_view(),
            Load::Failed(e) => failure(&e),
            Load::Ready(list) if list.is_empty() => p()
                .class("fui-muted")
                .text("Brak drzew do wyświetlenia.")
                .into_view(),
            Load::Ready(list) => div()
                .class("fui-card-grid")
                .children(list.iter().map(card))
                .into_view(),
        }))
        .into_view()
}
