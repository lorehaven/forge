//! Dialogs for changing the data: a person, a couple and their wedding, a relative to add, and
//! the confirmations for removing. They are forms over `genevia_core::wire`'s `PersonEdit` and
//! `FamilyEdit`; saving goes through the server functions, and what they change is read again
//! afterwards (`Shared::version`).

use std::rc::Rc;

use genevia_core::wire::{Card, EventEdit, FACT_KINDS, FactEdit, FamilyEdit, PersonEdit};
use quench_ui::prelude::*;
use quench_ui::router::RouteContext;

use crate::api::{
    ApiError, add_relative, delete_family, delete_person, family_edit, person, person_edit,
    save_family, save_person, search, spawn_local,
};
use crate::shell::at;
use crate::state::{Dialog, Load, Shared, resource};
use crate::util::{lifespan, sex_class};

/// Messages a save came back with, by the name of the field they are about.
type Errors = Signal<Vec<(String, String)>>;

fn errors_of(e: &ApiError) -> Vec<(String, String)> {
    e.fields()
}

// ---------------------------------------------------------------------------------------------
// Pieces of a form
// ---------------------------------------------------------------------------------------------

fn error_view(errors: &Errors, field: &'static str) -> View {
    let errors = errors.clone();
    View::dynamic(move || {
        let msg = errors.with(|e| e.iter().find(|(f, _)| f == field).map(|(_, m)| m.clone()));
        match msg {
            Some(m) => div().class("fui-field-error").text(m).into_view(),
            None => View::Empty,
        }
    })
}

/// A labelled one-line field bound to `value`; `field` names it for the messages.
fn text(
    label: &str,
    value: &Signal<String>,
    errors: &Errors,
    field: &'static str,
    hint: &str,
) -> View {
    let (read, write) = (value.clone(), value.clone());
    let mut f = div()
        .class("fui-field")
        .child(element("label").class("fui-label").text(label.to_string()))
        .child(
            input()
                .class("fui-input")
                .attr("type", "text")
                .attr_fn("value", move || read.get())
                .attr_fn("aria-invalid", {
                    let errors = errors.clone();
                    move || {
                        errors
                            .with(|e| e.iter().any(|(f, _)| f == field))
                            .to_string()
                    }
                })
                .on_input(move |v| write.set(v.to_string())),
        );
    if !hint.is_empty() {
        f = f.child(div().class("fui-hint").text(hint.to_string()));
    }
    f.child(error_view(errors, field)).into_view()
}

fn area(label: &str, value: &Signal<String>, hint: &str) -> View {
    let (read, write) = (value.clone(), value.clone());
    let mut f = div()
        .class("fui-field")
        .child(element("label").class("fui-label").text(label.to_string()))
        .child(
            element("textarea")
                .class("fui-textarea")
                .attr("rows", "3")
                .attr_fn("value", move || read.get())
                .on_input(move |v| write.set(v.to_string())),
        );
    if !hint.is_empty() {
        f = f.child(div().class("fui-hint").text(hint.to_string()));
    }
    f.into_view()
}

fn choice(label: &str, value: &Signal<String>, options: &[(&str, &str)]) -> View {
    let write = value.clone();
    let mut s = select()
        .class("fui-select")
        .on_change(move |v| write.set(v.to_string()));
    for (v, l) in options {
        let (read, v) = (value.clone(), (*v).to_string());
        let mark = v.clone();
        s = s.child(
            element("option")
                .attr("value", v)
                .attr_bool("selected", move || read.get() == mark)
                .text((*l).to_string()),
        );
    }
    div()
        .class("fui-field")
        .child(element("label").class("fui-label").text(label.to_string()))
        .child(s)
        .into_view()
}

fn section(title: &str, body: Vec<View>) -> View {
    div()
        .class("dlg-section")
        .child(h3().text(title.to_string()))
        .child(div().class("dlg-grid").children(body))
        .into_view()
}

fn grid_wide(body: View) -> View {
    div().class("dlg-wide").child(body).into_view()
}

/// The frame of every dialog: a scrim, a panel with a title, the body and the buttons.
fn modal(title: &str, body: View, footer: View, sh: &Shared) -> View {
    let (close_x, close_scrim, close_key) =
        (sh.dialog.clone(), sh.dialog.clone(), sh.dialog.clone());
    div()
        .class("fui-modal fui-open")
        .on_keydown(move |k| {
            if k.key == "Escape" {
                close_key.set(None);
            }
        })
        .child(
            div()
                .class("fui-modal-scrim")
                .on_click(move || close_scrim.set(None)),
        )
        .child(
            div()
                .class("fui-modal-panel fui-modal-lg")
                .attr("role", "dialog")
                .attr("aria-modal", "true")
                .trap_focus()
                .child(
                    div()
                        .class("fui-modal-header")
                        .child(h2().text(title.to_string()))
                        .child(
                            button()
                                .class("fui-icon-btn")
                                .attr("title", "Zamknij")
                                .text("✕")
                                .on_click(move || close_x.set(None)),
                        ),
                )
                .child(div().class("fui-modal-body").child(body))
                .child(div().class("fui-modal-footer").child(footer)),
        )
        .into_view()
}

fn button_row(
    sh: &Shared,
    save_label: &str,
    saving: &Signal<bool>,
    on_save: impl Fn() + 'static,
    extra: Option<View>,
) -> View {
    let cancel = sh.dialog.clone();
    let busy = saving.clone();
    let label = save_label.to_string();
    let mut row = div().class("dlg-buttons");
    if let Some(e) = extra {
        row = row.child(e).child(div().class("fui-topbar-spacer"));
    }
    row.child(
        button()
            .class("fui-btn fui-btn-secondary")
            .text("Anuluj")
            .on_click(move || cancel.set(None)),
    )
    .child(
        button()
            .class("fui-btn fui-btn-primary")
            .attr_bool("disabled", move || busy.get())
            .text(label)
            .on_click(on_save),
    )
    .into_view()
}

fn loading() -> View {
    p().class("fui-muted").text("Wczytywanie…").into_view()
}

// ---------------------------------------------------------------------------------------------
// A person
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
struct EventDraft {
    date: Signal<String>,
    place: Signal<String>,
    cause: Signal<String>,
    witnesses: Signal<String>,
    sources: Signal<String>,
    notes: Signal<String>,
}

impl EventDraft {
    fn new(e: &EventEdit) -> Self {
        Self {
            date: create_signal(e.date.clone()),
            place: create_signal(e.place.clone()),
            cause: create_signal(e.cause.clone()),
            witnesses: create_signal(e.witnesses.clone()),
            sources: create_signal(e.sources.clone()),
            notes: create_signal(e.notes.clone()),
        }
    }

    fn collect(&self) -> EventEdit {
        EventEdit {
            date: self.date.get_untracked(),
            place: self.place.get_untracked(),
            cause: self.cause.get_untracked(),
            witnesses: self.witnesses.get_untracked(),
            sources: self.sources.get_untracked(),
            notes: self.notes.get_untracked(),
        }
    }
}

#[derive(Clone)]
struct FactDraft {
    kind: Signal<String>,
    value: Signal<String>,
    date: Signal<String>,
    place: Signal<String>,
    sources: Signal<String>,
    notes: Signal<String>,
}

impl FactDraft {
    fn new(f: &FactEdit) -> Self {
        Self {
            kind: create_signal(f.kind.clone()),
            value: create_signal(f.value.clone()),
            date: create_signal(f.date.clone()),
            place: create_signal(f.place.clone()),
            sources: create_signal(f.sources.clone()),
            notes: create_signal(f.notes.clone()),
        }
    }
}

#[derive(Clone)]
struct PersonDraft {
    given: Signal<String>,
    surname: Signal<String>,
    maiden: Signal<String>,
    nickname: Signal<String>,
    prefix: Signal<String>,
    suffix: Signal<String>,
    akas: Signal<String>,
    sex: Signal<String>,
    birth: EventDraft,
    death: EventDraft,
    facts: Signal<Vec<FactDraft>>,
    notes: Signal<String>,
    sources: Signal<String>,
}

impl PersonDraft {
    fn new(p: &PersonEdit) -> Self {
        Self {
            given: create_signal(p.given.clone()),
            surname: create_signal(p.surname.clone()),
            maiden: create_signal(p.maiden_name.clone()),
            nickname: create_signal(p.nickname.clone()),
            prefix: create_signal(p.prefix.clone()),
            suffix: create_signal(p.suffix.clone()),
            akas: create_signal(p.akas.clone()),
            sex: create_signal(p.sex.clone()),
            birth: EventDraft::new(&p.birth),
            death: EventDraft::new(&p.death),
            facts: create_signal(p.facts.iter().map(FactDraft::new).collect()),
            notes: create_signal(p.notes.clone()),
            sources: create_signal(p.sources.clone()),
        }
    }

    fn collect(&self) -> PersonEdit {
        PersonEdit {
            given: self.given.get_untracked(),
            surname: self.surname.get_untracked(),
            maiden_name: self.maiden.get_untracked(),
            nickname: self.nickname.get_untracked(),
            prefix: self.prefix.get_untracked(),
            suffix: self.suffix.get_untracked(),
            akas: self.akas.get_untracked(),
            sex: self.sex.get_untracked(),
            birth: self.birth.collect(),
            death: self.death.collect(),
            facts: self
                .facts
                .get_untracked()
                .iter()
                .map(|f| FactEdit {
                    kind: f.kind.get_untracked(),
                    value: f.value.get_untracked(),
                    date: f.date.get_untracked(),
                    place: f.place.get_untracked(),
                    sources: f.sources.get_untracked(),
                    notes: f.notes.get_untracked(),
                })
                .collect(),
            notes: self.notes.get_untracked(),
            sources: self.sources.get_untracked(),
        }
    }
}

const DATE_HINT: &str = "np. 21.08.1967, 08.1967, 1967, ok. 1900, przed 1904, od 1925 do 1960";

fn fact_rows(d: &PersonDraft, errors: &Errors) -> View {
    let (facts, errors) = (d.facts.clone(), errors.clone());
    View::dynamic(move || {
        let list = facts.get();
        let rows: Vec<View> = list
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let kinds: Vec<(&str, &str)> = FACT_KINDS.to_vec();
                let remove = facts.clone();
                div()
                    .class("dlg-fact")
                    .child(choice("Rodzaj", &f.kind, &kinds))
                    .child(text("Opis", &f.value, &errors, "fact", ""))
                    .child(text("Data", &f.date, &errors, "fact-date", ""))
                    .child(text("Miejsce", &f.place, &errors, "fact-place", ""))
                    .child(
                        button()
                            .class("fui-btn fui-btn-tertiary fui-btn-sm dlg-remove")
                            .attr("title", "Usuń")
                            .text("Usuń")
                            .on_click(move || {
                                remove.update(|v| {
                                    if i < v.len() {
                                        v.remove(i);
                                    }
                                });
                            }),
                    )
                    .into_view()
            })
            .collect();
        fragment(rows)
    })
}

/// The fields of a person. `short` leaves out all but the names, sex and birth: what is wanted
/// when adding a relative.
fn person_fields(d: &PersonDraft, errors: &Errors, short: bool) -> Vec<View> {
    let sexes = [("", "nieznana"), ("M", "mężczyzna"), ("K", "kobieta")];
    let mut names = vec![
        text("Imię (imiona)", &d.given, errors, "given", ""),
        text("Nazwisko", &d.surname, errors, "surname", ""),
        choice("Płeć", &d.sex, &sexes),
        text(
            "Nazwisko rodowe",
            &d.maiden,
            errors,
            "maiden_name",
            "tylko gdy różni się od używanego",
        ),
    ];
    if !short {
        names.push(text("Pseudonim", &d.nickname, errors, "nickname", ""));
        names.push(text(
            "Inne imiona i nazwiska",
            &d.akas,
            errors,
            "akas",
            "oddziel średnikiem",
        ));
        names.push(text(
            "Tytuł przed nazwiskiem",
            &d.prefix,
            errors,
            "prefix",
            "np. dr",
        ));
        names.push(text(
            "Tytuł po nazwisku",
            &d.suffix,
            errors,
            "suffix",
            "np. jr",
        ));
    }
    let mut v = vec![section("Imię i nazwisko", names)];
    let mut birth = vec![
        text(
            "Data urodzenia",
            &d.birth.date,
            errors,
            "birth.date",
            DATE_HINT,
        ),
        text(
            "Miejsce urodzenia",
            &d.birth.place,
            errors,
            "birth.place",
            "",
        ),
    ];
    if !short {
        birth.push(text(
            "Chrzestni",
            &d.birth.witnesses,
            errors,
            "birth.witnesses",
            "oddziel średnikiem",
        ));
        birth.push(grid_wide(area(
            "Akt urodzenia i inne źródła",
            &d.birth.sources,
            "jedno źródło w wierszu: „Akt urodzenia: USC Mikołów 1967 nr 893”",
        )));
        birth.push(grid_wide(area("Uwagi do urodzenia", &d.birth.notes, "")));
    }
    v.push(section("Urodzenie", birth));
    if short {
        return v;
    }
    v.push(section(
        "Zgon",
        vec![
            text("Data zgonu", &d.death.date, errors, "death.date", DATE_HINT),
            text("Miejsce zgonu", &d.death.place, errors, "death.place", ""),
            text("Przyczyna zgonu", &d.death.cause, errors, "death.cause", ""),
            grid_wide(area("Akt zgonu i inne źródła", &d.death.sources, "")),
            grid_wide(area(
                "Uwagi do zgonu (np. wiek w chwili zgonu)",
                &d.death.notes,
                "",
            )),
        ],
    ));
    let add = d.facts.clone();
    v.push(
        div()
            .class("dlg-section")
            .child(h3().text("Fakty i zdarzenia"))
            .child(fact_rows(d, errors))
            .child(
                button()
                    .class("fui-btn fui-btn-secondary fui-btn-sm")
                    .text("Dodaj fakt (zawód, pochówek…)")
                    .on_click(move || {
                        add.update(|v| {
                            v.push(FactDraft::new(&FactEdit {
                                kind: "OCCU".to_string(),
                                ..FactEdit::default()
                            }));
                        });
                    }),
            )
            .into_view(),
    );
    v.push(section(
        "Notatki i źródła",
        vec![
            grid_wide(area("Notatki", &d.notes, "jedna notatka w wierszu")),
            grid_wide(area(
                "Źródła osoby",
                &d.sources,
                "jedno źródło w wierszu: „Tytuł: szczegóły”",
            )),
        ],
    ));
    v
}

fn finish(sh: &Shared, saving: &Signal<bool>, errors: &Errors, result: Result<(), ApiError>) {
    saving.set(false);
    match result {
        Ok(()) => {
            sh.version.update(|n| *n += 1);
            sh.dialog.set(None);
        }
        Err(e) => errors.set(errors_of(&e)),
    }
}

fn general_error(errors: &Errors) -> View {
    let errors = errors.clone();
    View::dynamic(move || {
        match errors.with(|e| {
            e.iter()
                .find(|(f, _)| f.is_empty() || f == "relation" || f == "family" || f == "xref")
                .map(|(_, m)| m.clone())
        }) {
            Some(m) => p().class("fui-notice fui-notice-error").text(m).into_view(),
            None => View::Empty,
        }
    })
}

fn edit_person(tree: String, xref: String, sh: &Shared) -> View {
    let sh = sh.clone();
    let loaded = resource(
        {
            let (t, x) = (tree.clone(), xref.clone());
            move || (t.clone(), x.clone())
        },
        |(t, x): (String, String)| async move {
            person_edit(t, x).await.map_err(|e| e.message().to_string())
        },
    );
    View::dynamic(move || match loaded.get() {
        Load::Loading => modal("Edycja osoby", loading(), View::Empty, &sh),
        Load::Failed(e) => modal(
            "Edycja osoby",
            p().class("error").text(e).into_view(),
            View::Empty,
            &sh,
        ),
        Load::Ready(init) => {
            let draft = PersonDraft::new(&init);
            let errors: Errors = create_signal(Vec::new());
            let saving = create_signal(false);
            let body = div()
                .class("dlg-form")
                .child(general_error(&errors))
                .children(person_fields(&draft, &errors, false))
                .into_view();
            let (t, x, shared, d, e, s) = (
                tree.clone(),
                xref.clone(),
                sh.clone(),
                draft,
                errors,
                saving.clone(),
            );
            let footer = button_row(
                &sh,
                "Zapisz",
                &saving,
                move || {
                    s.set(true);
                    e.set(Vec::new());
                    let (t, x, edit) = (t.clone(), x.clone(), d.collect());
                    let (shared, s, e) = (shared.clone(), s.clone(), e.clone());
                    spawn_local(async move {
                        let r = save_person(t, x, edit).await;
                        finish(&shared, &s, &e, r);
                    });
                },
                None,
            );
            modal("Edycja osoby", body, footer, &sh)
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Choosing a person who is already in the tree
// ---------------------------------------------------------------------------------------------

/// A search box with results; picking one calls `on_pick`.
fn picker(tree: &str, on_pick: Rc<dyn Fn(Card)>, version: Signal<u32>) -> View {
    let query = create_signal(String::new());
    let tree = tree.to_string();
    let results = resource(
        {
            let (query, tree, version) = (query.clone(), tree, version);
            move || (tree.clone(), query.get(), version.get())
        },
        |(t, q, _): (String, String, u32)| async move {
            if q.trim().chars().count() < 2 {
                return Ok::<Vec<Card>, String>(Vec::new());
            }
            search(t, q).await.map_err(|e| e.message().to_string())
        },
    );
    let (write, read) = (query.clone(), query.clone());
    let list = View::dynamic(move || match results.get() {
        Load::Ready(hits) if !read.get().trim().is_empty() => {
            if hits.is_empty() {
                return p().class("fui-muted").text("Brak wyników").into_view();
            }
            let items: Vec<View> = hits
                .into_iter()
                .map(|c| {
                    let (pick, card) = (on_pick.clone(), c.clone());
                    button()
                        .class(format!("hit {}", sex_class(&c)))
                        .attr("type", "button")
                        .on_click(move || pick(card.clone()))
                        .child(span().class("hn").text(c.name.clone()))
                        .child(span().class("hy").text(format!(
                            "{} {}",
                            lifespan(&c),
                            c.birth_place
                        )))
                        .into_view()
                })
                .collect();
            div().class("pick-hits").children(items).into_view()
        }
        _ => View::Empty,
    });
    div()
        .class("picker")
        .child(
            input()
                .class("fui-input")
                .attr("type", "search")
                .attr("placeholder", "Szukaj osoby w drzewie…")
                .attr_fn("value", move || query.get())
                .on_input(move |v| write.set(v.to_string())),
        )
        .child(list)
        .into_view()
}

fn chip(c: &Card, on_clear: impl Fn() + 'static) -> View {
    div()
        .class(format!("chip {}", sex_class(c)))
        .child(span().class("hn").text(c.name.clone()))
        .child(span().class("hy").text(lifespan(c)))
        .child(
            button()
                .class("fui-icon-btn")
                .attr("title", "Usuń z tego małżeństwa")
                .text("✕")
                .on_click(on_clear),
        )
        .into_view()
}

// ---------------------------------------------------------------------------------------------
// A couple
// ---------------------------------------------------------------------------------------------

fn edit_family(tree: String, family: String, sh: &Shared) -> View {
    let sh = sh.clone();
    let loaded = resource(
        {
            let (t, f) = (tree.clone(), family.clone());
            move || (t.clone(), f.clone())
        },
        |(t, f): (String, String)| async move {
            family_edit(t, f).await.map_err(|e| e.message().to_string())
        },
    );
    View::dynamic(move || match loaded.get() {
        Load::Loading => modal("Małżeństwo", loading(), View::Empty, &sh),
        Load::Failed(e) => modal(
            "Małżeństwo",
            p().class("error").text(e).into_view(),
            View::Empty,
            &sh,
        ),
        Load::Ready(form) => {
            let husband: Signal<Option<Card>> = create_signal(form.husband.clone());
            let wife: Signal<Option<Card>> = create_signal(form.wife.clone());
            let children: Signal<Vec<Card>> = create_signal(form.children.clone());
            let marriage = EventDraft::new(&form.edit.marriage);
            let notes = create_signal(form.edit.notes);
            let errors: Errors = create_signal(Vec::new());
            let saving = create_signal(false);

            let spouse_row = |label: &str, slot: &Signal<Option<Card>>| {
                let (show, set) = (slot.clone(), slot.clone());
                let (tree, version, label) = (tree.clone(), sh.version.clone(), label.to_string());
                div()
                    .class("fui-field")
                    .child(element("label").class("fui-label").text(label))
                    .child(View::dynamic(move || {
                        if let Some(c) = show.get() {
                            let clear = set.clone();
                            chip(&c, move || clear.set(None))
                        } else {
                            let pick = set.clone();
                            picker(&tree, Rc::new(move |c| pick.set(Some(c))), version.clone())
                        }
                    }))
                    .into_view()
            };
            let spouses = section(
                "Małżonkowie",
                vec![spouse_row("Mąż", &husband), spouse_row("Żona", &wife)],
            );

            let kids = {
                let (list, tree, version) = (children.clone(), tree.clone(), sh.version.clone());
                let rows = View::dynamic(move || {
                    let items = list.get();
                    let n = items.len();
                    let rows: Vec<View> = items
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            let (up, down, gone) = (list.clone(), list.clone(), list.clone());
                            div()
                                .class(format!("chip {}", sex_class(c)))
                                .child(span().class("hn").text(c.name.clone()))
                                .child(span().class("hy").text(lifespan(c)))
                                .child(
                                    button()
                                        .class("fui-icon-btn")
                                        .attr("title", "Wyżej")
                                        .text("↑")
                                        .attr_bool("disabled", move || i == 0)
                                        .on_click(move || {
                                            up.update(|v| {
                                                if i > 0 {
                                                    v.swap(i, i - 1);
                                                }
                                            });
                                        }),
                                )
                                .child(
                                    button()
                                        .class("fui-icon-btn")
                                        .attr("title", "Niżej")
                                        .text("↓")
                                        .attr_bool("disabled", move || i + 1 >= n)
                                        .on_click(move || {
                                            down.update(|v| {
                                                if i + 1 < v.len() {
                                                    v.swap(i, i + 1);
                                                }
                                            });
                                        }),
                                )
                                .child(
                                    button()
                                        .class("fui-icon-btn")
                                        .attr("title", "Usuń z rodziny")
                                        .text("✕")
                                        .on_click(move || {
                                            gone.update(|v| {
                                                if i < v.len() {
                                                    v.remove(i);
                                                }
                                            });
                                        }),
                                )
                                .into_view()
                        })
                        .collect();
                    fragment(rows)
                });
                let add = children.clone();
                div()
                    .class("dlg-section")
                    .child(h3().text("Dzieci"))
                    .child(div().class("chips").child(rows))
                    .child(
                        div()
                            .class("fui-field")
                            .child(
                                element("label")
                                    .class("fui-label")
                                    .text("Dodaj istniejącą osobę jako dziecko"),
                            )
                            .child(picker(
                                &tree,
                                Rc::new(move |c| {
                                    add.update(|v| {
                                        if v.iter().all(|x| x.xref != c.xref) {
                                            v.push(c);
                                        }
                                    });
                                }),
                                version,
                            )),
                    )
                    .into_view()
            };

            let wedding = section(
                "Ślub",
                vec![
                    text(
                        "Data ślubu",
                        &marriage.date,
                        &errors,
                        "marriage.date",
                        DATE_HINT,
                    ),
                    text(
                        "Miejsce ślubu",
                        &marriage.place,
                        &errors,
                        "marriage.place",
                        "",
                    ),
                    grid_wide(text(
                        "Świadkowie",
                        &marriage.witnesses,
                        &errors,
                        "marriage.witnesses",
                        "oddziel średnikiem",
                    )),
                    grid_wide(area(
                        "Akt ślubu i inne źródła",
                        &marriage.sources,
                        "jedno źródło w wierszu: „Akt ślubu: USC Mikołów 1928 nr 80”",
                    )),
                    grid_wide(area("Uwagi do ślubu", &marriage.notes, "")),
                    grid_wide(area("Notatki o rodzinie", &notes, "")),
                ],
            );
            let body = div()
                .class("dlg-form")
                .child(general_error(&errors))
                .child(error_view(&errors, "husband"))
                .child(error_view(&errors, "wife"))
                .child(error_view(&errors, "children"))
                .child(spouses)
                .child(wedding)
                .child(kids)
                .into_view();

            let (t, f, shared, e, s) = (
                tree.clone(),
                family.clone(),
                sh.clone(),
                errors,
                saving.clone(),
            );
            let (h, w, c, m, n) = (husband, wife, children, marriage, notes);
            let remove = {
                let (dialog, f) = (sh.dialog.clone(), family.clone());
                button()
                    .class("fui-btn fui-btn-danger-soft")
                    .text("Usuń małżeństwo")
                    .on_click(move || dialog.set(Some(Dialog::DeleteFamily(f.clone()))))
                    .into_view()
            };
            let footer = button_row(
                &sh,
                "Zapisz",
                &saving,
                move || {
                    s.set(true);
                    e.set(Vec::new());
                    let edit = FamilyEdit {
                        husband: h.get_untracked().map(|c| c.xref).unwrap_or_default(),
                        wife: w.get_untracked().map(|c| c.xref).unwrap_or_default(),
                        children: c.get_untracked().into_iter().map(|c| c.xref).collect(),
                        marriage: m.collect(),
                        notes: n.get_untracked(),
                    };
                    let (t, f, shared, s, e) =
                        (t.clone(), f.clone(), shared.clone(), s.clone(), e.clone());
                    spawn_local(async move {
                        let r = save_family(t, f, edit).await;
                        finish(&shared, &s, &e, r);
                    });
                },
                Some(remove),
            );
            modal("Małżeństwo i dzieci", body, footer, &sh)
        }
    })
}

// ---------------------------------------------------------------------------------------------
// A relative
// ---------------------------------------------------------------------------------------------

fn relation_name(relation: &str) -> &'static str {
    match relation {
        "father" => "ojca",
        "mother" => "matkę",
        "spouse" => "małżonka",
        _ => "dziecko",
    }
}

fn add_relative_dialog(
    tree: String,
    of: String,
    relation: String,
    family: String,
    sh: &Shared,
) -> View {
    let sh = sh.clone();
    let who = resource(
        {
            let (t, x) = (tree.clone(), of.clone());
            move || (t.clone(), x.clone())
        },
        |(t, x): (String, String)| async move {
            person(t, x).await.map_err(|e| e.message().to_string())
        },
    );
    View::dynamic(move || {
        let title = format!("Dodaj {}", relation_name(&relation));
        match who.get() {
            Load::Loading => modal(&title, loading(), View::Empty, &sh),
            Load::Failed(e) => modal(
                &title,
                p().class("error").text(e).into_view(),
                View::Empty,
                &sh,
            ),
            Load::Ready(anchor) => {
                let mode = create_signal("new".to_string());
                let picked: Signal<Option<Card>> = create_signal(None);
                let draft = PersonDraft::new(&PersonEdit::default());
                let errors: Errors = create_signal(Vec::new());
                let saving = create_signal(false);
                let chosen_family = create_signal(family.clone());

                let tabs = {
                    let (a, b, ma, mb) = (mode.clone(), mode.clone(), mode.clone(), mode.clone());
                    div()
                        .class("fui-tabs dlg-tabs")
                        .child(
                            button()
                                .class("fui-tab")
                                .attr_opt("aria-selected", move || {
                                    (ma.get() == "new").then(|| "true".to_string())
                                })
                                .text("Nowa osoba")
                                .on_click(move || a.set("new".to_string())),
                        )
                        .child(
                            button()
                                .class("fui-tab")
                                .attr_opt("aria-selected", move || {
                                    (mb.get() == "existing").then(|| "true".to_string())
                                })
                                .text("Osoba z drzewa")
                                .on_click(move || b.set("existing".to_string())),
                        )
                        .into_view()
                };

                // A child of someone with several unions needs to say which.
                let family_choice = if relation == "child" && anchor.unions.len() > 1 {
                    let options: Vec<(String, String)> = anchor
                        .unions
                        .iter()
                        .map(|u| {
                            (
                                u.family.clone(),
                                u.spouse.as_ref().map_or_else(
                                    || "(bez drugiego rodzica)".to_string(),
                                    |s| format!("z {}", s.name),
                                ),
                            )
                        })
                        .collect();
                    let write = chosen_family.clone();
                    let mut s = select()
                        .class("fui-select")
                        .on_change(move |v| write.set(v.to_string()));
                    s = s.child(element("option").attr("value", "").text("— wybierz —"));
                    for (v, l) in options {
                        let read = chosen_family.clone();
                        let mark = v.clone();
                        s = s.child(
                            element("option")
                                .attr("value", v)
                                .attr_bool("selected", move || read.get() == mark)
                                .text(l),
                        );
                    }
                    div()
                        .class("fui-field")
                        .child(
                            element("label")
                                .class("fui-label")
                                .text("Z którego małżeństwa"),
                        )
                        .child(s)
                        .into_view()
                } else {
                    View::Empty
                };

                let new_form = {
                    let (m, d, e) = (mode.clone(), draft.clone(), errors.clone());
                    View::dynamic(move || {
                        if m.get() == "new" {
                            div()
                                .class("dlg-form")
                                .children(person_fields(&d, &e, true))
                                .into_view()
                        } else {
                            View::Empty
                        }
                    })
                };
                let existing = {
                    let (m, pk, tr, version) = (
                        mode.clone(),
                        picked.clone(),
                        tree.clone(),
                        sh.version.clone(),
                    );
                    View::dynamic(move || {
                        if m.get() != "existing" {
                            return View::Empty;
                        }
                        if let Some(c) = pk.get() {
                            let clear = pk.clone();
                            chip(&c, move || clear.set(None))
                        } else {
                            let set = pk.clone();
                            picker(&tr, Rc::new(move |c| set.set(Some(c))), version.clone())
                        }
                    })
                };
                let body = div()
                    .class("dlg-form")
                    .child(
                        p().class("fui-muted")
                            .text(format!("Przy osobie: {}", anchor.card.name)),
                    )
                    .child(general_error(&errors))
                    .child(error_view(&errors, "existing"))
                    .child(tabs)
                    .child(family_choice)
                    .child(new_form)
                    .child(existing)
                    .into_view();

                let (t, o, r, shared, d, e, s, md, pk, cf) = (
                    tree.clone(),
                    of.clone(),
                    relation.clone(),
                    sh.clone(),
                    draft,
                    errors,
                    saving.clone(),
                    mode,
                    picked,
                    chosen_family,
                );
                let footer = button_row(
                    &sh,
                    "Dodaj",
                    &saving,
                    move || {
                        s.set(true);
                        e.set(Vec::new());
                        let existing = if md.get_untracked() == "existing" {
                            pk.get_untracked().map(|c| c.xref).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        if md.get_untracked() == "existing" && existing.is_empty() {
                            s.set(false);
                            e.set(vec![(
                                "existing".to_string(),
                                "Wybierz osobę z listy.".to_string(),
                            )]);
                            return;
                        }
                        let (t, o, r, f, edit) = (
                            t.clone(),
                            o.clone(),
                            r.clone(),
                            cf.get_untracked(),
                            d.collect(),
                        );
                        let (shared, s, e) = (shared.clone(), s.clone(), e.clone());
                        spawn_local(async move {
                            match add_relative(t, o, r, existing, edit, f).await {
                                Ok(new) => {
                                    s.set(false);
                                    shared.version.update(|n| *n += 1);
                                    shared.dialog.set(None);
                                    shared.selected.set(Some(new));
                                }
                                Err(err) => {
                                    s.set(false);
                                    e.set(errors_of(&err));
                                }
                            }
                        });
                    },
                    None,
                );
                modal(&title, body, footer, &sh)
            }
        }
    })
}

// ---------------------------------------------------------------------------------------------
// Removing
// ---------------------------------------------------------------------------------------------

fn confirm(
    title: &str,
    what: String,
    warning: &str,
    sh: &Shared,
    on_confirm: impl Fn() + 'static,
    label: &str,
) -> View {
    let cancel = sh.dialog.clone();
    let body = div()
        .class("dlg-form")
        .child(p().text(what))
        .child(p().class("fui-muted").text(warning.to_string()))
        .into_view();
    let footer = div()
        .class("dlg-buttons")
        .child(
            button()
                .class("fui-btn fui-btn-secondary")
                .text("Anuluj")
                .on_click(move || cancel.set(None)),
        )
        .child(
            button()
                .class("fui-btn fui-btn-danger")
                .text(label.to_string())
                .on_click(on_confirm),
        )
        .into_view();
    modal(title, body, footer, sh)
}

fn delete_person_dialog(tree: String, xref: String, sh: &Shared) -> View {
    let sh = sh.clone();
    let who = resource(
        {
            let (t, x) = (tree.clone(), xref.clone());
            move || (t.clone(), x.clone())
        },
        |(t, x): (String, String)| async move {
            person(t, x).await.map_err(|e| e.message().to_string())
        },
    );
    View::dynamic(move || {
        let name = match who.get() {
            Load::Ready(p) => p.card.name,
            _ => String::new(),
        };
        let (t, x, shared) = (tree.clone(), xref.clone(), sh.clone());
        confirm(
            "Usuń osobę",
            format!("Usunąć „{name}”?"),
            "Osoba zniknie z drzewa i ze wszystkich rodzin, w których była. Tego nie można cofnąć.",
            &sh,
            move || {
                let (t, x, shared) = (t.clone(), x.clone(), shared.clone());
                spawn_local(async move {
                    if delete_person(t, x).await.is_ok() {
                        shared.selected.set(None);
                        shared.version.update(|n| *n += 1);
                        shared.dialog.set(None);
                    }
                });
            },
            "Usuń",
        )
    })
}

fn delete_family_dialog(tree: String, family: String, sh: &Shared) -> View {
    let (t, f, shared) = (tree, family, sh.clone());
    confirm(
        "Usuń małżeństwo",
        "Usunąć to małżeństwo?".to_string(),
        "Osoby zostają w drzewie, ale tracą powiązanie jako małżonkowie i rodzice dzieci z tej rodziny. Tego nie można cofnąć.",
        sh,
        move || {
            let (t, f, shared) = (t.clone(), f.clone(), shared.clone());
            spawn_local(async move {
                if delete_family(t, f).await.is_ok() {
                    shared.version.update(|n| *n += 1);
                    shared.dialog.set(None);
                }
            });
        },
        "Usuń",
    )
}

/// Whatever dialog is open, over the tree whose address this is.
#[must_use]
pub fn layer(ctx: &RouteContext, sh: &Shared) -> View {
    let (ctx, sh) = (ctx.clone(), sh.clone());
    View::dynamic(move || {
        let Some(open) = sh.dialog.get() else {
            return View::Empty;
        };
        let tree = at(&ctx.location().path).tree;
        match open {
            Dialog::EditPerson(x) => edit_person(tree, x, &sh),
            Dialog::EditFamily(f) => edit_family(tree, f, &sh),
            Dialog::AddRelative {
                of,
                relation,
                family,
            } => add_relative_dialog(tree, of, relation, family, &sh),
            Dialog::DeletePerson(x) => delete_person_dialog(tree, x, &sh),
            Dialog::DeleteFamily(f) => delete_family_dialog(tree, f, &sh),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn the_short_person_form_mounts() {
        let renderer = Rc::new(TestRenderer::new());
        let root = renderer.root();
        let (mounted, scope) = create_root(|| {
            let d = PersonDraft::new(&PersonEdit::default());
            let errors: Errors = create_signal(Vec::new());
            let view = div().children(person_fields(&d, &errors, true)).into_view();
            mount(&renderer, &root, view)
        });
        assert!(renderer.text().contains("Imię"));
        mounted.unmount();
        scope.dispose();
    }

    #[test]
    fn the_full_person_form_mounts_and_takes_a_fact() {
        let renderer = Rc::new(TestRenderer::new());
        let root = renderer.root();
        let init = PersonEdit {
            given: "Jan".into(),
            facts: vec![FactEdit {
                kind: "OCCU".into(),
                value: "górnik".into(),
                ..FactEdit::default()
            }],
            ..PersonEdit::default()
        };
        let ((mounted, d), scope) = create_root(|| {
            let d = PersonDraft::new(&init);
            let errors: Errors = create_signal(Vec::new());
            let view = div()
                .children(person_fields(&d, &errors, false))
                .into_view();
            (mount(&renderer, &root, view), d)
        });
        assert!(renderer.text().contains("Fakty i zdarzenia"));
        d.facts
            .update(|v| v.push(FactDraft::new(&FactEdit::default())));
        assert_eq!(d.collect().facts.len(), 2);
        mounted.unmount();
        scope.dispose();
    }
}
