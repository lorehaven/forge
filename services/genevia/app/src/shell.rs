//! The frame around a tree: header with the view tabs and controls, the body, and the side panel.

use crate::api::{ancestry, person, search};
use genevia_core::wire::{Ancestry, Card, EventView, FactView, PersonDetail};
use quench_ui::prelude::*;
use std::rc::Rc;

use quench_ui::i18n::I18n;
use quench_ui::router::{RouteContext, use_router};

use crate::fan::fan_view;
use crate::list::list_view;
use crate::pedigree::pedigree_view;
use crate::state::{Dialog, Load, Shared, resource};
use crate::util::{home_url, lifespan, sex_class, url};

pub const VIEWS: [(&str, &str); 3] = [
    ("list", "Lista"),
    ("pedigree", "Przodkowie"),
    ("fan", "Widok promienisty"),
];

/// Where in a tree the address points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct At {
    pub tree: String,
    pub view: String,
    /// Blank for the tree's own root.
    pub xref: String,
}

/// Reads `[<base>]/ui/t/<tree>/<view>/<xref>`.
#[must_use]
pub fn at(path: &str) -> At {
    let mut parts = path.split('/').filter(|s| !s.is_empty());
    // Whatever the service's prefix is, the tree starts after `ui/t`.
    for part in parts.by_ref() {
        if part == "ui" {
            break;
        }
    }
    let _t = parts.next();
    let tree = parts.next().unwrap_or("").to_string();
    let view = parts
        .next()
        .filter(|v| VIEWS.iter().any(|(k, _)| k == v))
        .unwrap_or("list")
        .to_string();
    let xref = parts.next().unwrap_or("").to_string();
    At { tree, view, xref }
}

fn location_at(ctx: &RouteContext) -> At {
    at(&ctx.location().path)
}

fn option(value: u32, label: &str, current: Signal<u32>) -> Element {
    element("option")
        .attr("value", value.to_string())
        .attr_bool("selected", move || current.get() == value)
        .text(label)
}

fn picker(label: &str, choices: &[(u32, String)], current: Signal<u32>) -> Element {
    let set = current.clone();
    let mut s = select()
        .class("fui-select fui-select-sm")
        .on_change(move |v| {
            if let Ok(n) = v.parse() {
                set.set(n);
            }
        });
    for (v, l) in choices {
        s = s.child(option(*v, l, current.clone()));
    }
    label_().child(span().text(label.to_string())).child(s)
}

fn label_() -> Element {
    element("label").class("ctl")
}

fn numbers(from: u32, to: u32) -> Vec<(u32, String)> {
    (from..=to).map(|n| (n, n.to_string())).collect()
}

fn search_box(at_: Memo<At>, sh: &Shared) -> View {
    let query = create_signal(String::new());
    let sh_version = sh.version.clone();
    let q = query.clone();
    let results = resource(
        {
            let (query, at_) = (query.clone(), at_);
            move || (at_.get().tree, query.get(), sh_version.get())
        },
        |(tree, q, _): (String, String, u32)| async move {
            if q.trim().chars().count() < 2 {
                return Ok::<Vec<Card>, String>(Vec::new());
            }
            search(tree, q).await.map_err(|e| e.message().to_string())
        },
    );
    let (typed, clear, shown) = (query.clone(), query.clone(), query);
    let sh = sh.clone();
    let list = View::dynamic(move || {
        let hits = match results.get() {
            Load::Ready(h) if !shown.get().trim().is_empty() => h,
            _ => return View::Empty,
        };
        if hits.is_empty() {
            return div()
                .class("hits")
                .child(p().class("muted").text("Brak wyników"))
                .into_view();
        }
        let items: Vec<View> = hits
            .into_iter()
            .map(|c| {
                let (pick, clear, xref) = (sh.selected.clone(), clear.clone(), c.xref.clone());
                button()
                    .class(format!("hit {}", sex_class(&c)))
                    .on_click(move || {
                        pick.set(Some(xref.clone()));
                        clear.set(String::new());
                    })
                    .child(span().class("hn").text(c.name.clone()))
                    .child(
                        span()
                            .class("hy")
                            .text(format!("{} {}", lifespan(&c), c.birth_place)),
                    )
                    .into_view()
            })
            .collect();
        div().class("hits").children(items).into_view()
    });
    div()
        .class("search")
        .child(
            input()
                .class("fui-input fui-input-sm")
                .attr("type", "search")
                .attr("placeholder", "Szukaj osoby…")
                .attr_fn("value", move || q.get())
                .on_input(move |v| typed.set(v.to_string())),
        )
        .child(list)
        .into_view()
}

/// The controls above a tree: generations and angle for the views that have them, and the search.
fn toolbar(ctx: &RouteContext, sh: &Shared) -> View {
    let at_ = create_memo({
        let ctx = ctx.clone();
        move || location_at(&ctx)
    });
    let controls = View::dynamic({
        let at_ = at_.clone();
        let (ped, fan, arc) = (
            sh.pedigree_gens.clone(),
            sh.fan_gens.clone(),
            sh.fan_span.clone(),
        );
        move || match at_.get().view.as_str() {
            "pedigree" => div()
                .class("controls")
                .child(picker("Pokolenia wstecz", &numbers(2, 5), ped.clone()))
                .into_view(),
            "fan" => div()
                .class("controls")
                .child(picker("Pokolenia wstecz", &numbers(2, 8), fan.clone()))
                .child(picker(
                    "Kąt",
                    &[
                        (180, "180°".to_string()),
                        (270, "270°".to_string()),
                        (360, "360°".to_string()),
                    ],
                    arc.clone(),
                ))
                .into_view(),
            _ => View::Empty,
        }
    });
    div()
        .class("gen-toolbar")
        .child(h2().class("gen-title").text_fn({
            let at_ = at_.clone();
            move || {
                VIEWS
                    .iter()
                    .find(|(k, _)| *k == at_.get().view)
                    .map_or(String::new(), |(_, l)| (*l).to_string())
            }
        }))
        .child(controls)
        .child(div().class("fui-topbar-spacer"))
        .child(search_box(at_, sh))
        .into_view()
}

const NAV: [(&str, &str, &str); 3] = [
    ("list", "Lista", "list"),
    ("pedigree", "Przodkowie", "columns"),
    ("fan", "Widok promienisty", "layers"),
];

/// The side panel's navigation: the views of the open tree, and the way back to all trees.
fn nav_items(ctx: &RouteContext) -> View {
    let ctx = ctx.clone();
    View::dynamic(move || {
        let path = ctx.location().path;
        let router = use_router();
        let mut items: Vec<View> = Vec::new();
        if path.contains("/ui/t/") {
            let w = at(&path);
            for (key, label, ico) in NAV {
                let (to, go) = (url(&w.tree, key, &w.xref), router.clone());
                let target = to.clone();
                items.push(
                    a().class("fui-nav-item")
                        .attr("href", to)
                        .attr_opt("aria-current", {
                            let on = w.view == key;
                            move || on.then(|| "page".to_string())
                        })
                        .intercept_click()
                        .on_click(move || go.navigate(&target))
                        .child(forge_shell::icon(ico))
                        .child(span().text(label))
                        .into_view(),
                );
            }
        }
        let go = router;
        items.push(
            a().class("fui-nav-item")
                .attr("href", home_url())
                .attr_opt("aria-current", {
                    let on = !path.contains("/ui/t/");
                    move || on.then(|| "page".to_string())
                })
                .intercept_click()
                .on_click(move || go.navigate(&home_url()))
                .child(forge_shell::icon("folder"))
                .child(span().text("Wszystkie drzewa"))
                .into_view(),
        );
        fragment(items)
    })
}

const PL: &str = "locale_label = Język
ui_dashboard_button = Panel
ui_home_button = Strona główna
ui_theme_toggle = Motyw
ui_theme_mode = Tryb
ui_theme_light = Jasny
ui_theme_dark = Ciemny
ui_theme_palette = Paleta
ui_palette_green = Zielony
ui_palette_teal = Morski
ui_palette_blue = Niebieski
ui_palette_purple = Fioletowy
ui_palette_rose = Różowy
ui_palette_orange = Pomarańczowy
ui_user_menu = Konto
ui_profile_edit = Profil
ui_logout = Wyloguj
";

fn bar() -> forge_shell::Bar {
    let base = crate::api::base();
    forge_shell::Bar {
        brand: forge_shell::Brand {
            name: "Genevia",
            icon: "users",
        },
        image: Some(format!("{base}/ui/assets/favicon.png")),
        home: home_url(),
        spa: true,
        account: true,
        brand_in_bar: false,
        logout: format!("{base}/ui/logout"),
        on_logout: None,
        dashboard: true,
        languages: &[],
        on_language: Rc::new(|_| {}),
    }
}

/// The page around everything: the shared side panel and top bar of the forge apps.
#[must_use]
pub fn page(ctx: &RouteContext, inner: View) -> View {
    let t = Rc::new(I18n::new(&[("pl", PL)], "pl").expect("valid catalogue"));
    let bar = bar();
    div()
        .attr("id", "genevia")
        .class("fui-app fui-client")
        .child(forge_shell::sidebar(&bar, vec![nav_items(ctx)]))
        .child(
            div()
                .class("fui-client-col")
                .child(forge_shell::topbar(&bar, &t))
                .child(div().class("gen-body").child(inner)),
        )
        .into_view()
}

/// A labelled line: `Urodzony  3 wrz 1993, Mikołów`. Empty values make no line.
fn kv(label: &str, value: &str) -> Option<View> {
    if value.is_empty() {
        return None;
    }
    Some(
        p().class("kv")
            .child(span().class("k").text(label.to_string()))
            .child(span().class("v").text(value.to_string()))
            .into_view(),
    )
}

fn when_where(e: &EventView) -> String {
    [e.date.as_str(), e.place.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(", ")
}

fn person_button(c: &Card, sh: &Shared, role: &str) -> View {
    let (pick, xref) = (sh.selected.clone(), c.xref.clone());
    button()
        .class(format!("rel {}", sex_class(c)))
        .on_click(move || pick.set(Some(xref.clone())))
        .child(span().class("role").text(role.to_string()))
        .child(span().text(c.name.clone()))
        .child(span().class("hy").text(lifespan(c)))
        .into_view()
}

/// A section of the panel: a heading and its content, spaced from the sections around it.
fn block(title: &str, content: Vec<View>) -> Option<View> {
    if content.is_empty() {
        return None;
    }
    Some(
        div()
            .class("blk")
            .child(h3().text(title.to_string()))
            .children(content)
            .into_view(),
    )
}

/// What a fact is called.
fn fact_label(kind: &str) -> &'static str {
    match kind {
        "CHR" | "BAPM" => "Chrzest",
        "BURI" => "Pochówek",
        "CREM" => "Kremacja",
        "OCCU" => "Zawód",
        "EDUC" => "Wykształcenie",
        "RESI" => "Zamieszkanie",
        "RELI" => "Wyznanie",
        "NATI" => "Narodowość",
        "TITL" => "Tytuł",
        "IMMI" => "Imigracja",
        "EMIG" => "Emigracja",
        "NATU" => "Naturalizacja",
        "CENS" => "Spis ludności",
        "PROB" => "Zatwierdzenie testamentu",
        "WILL" => "Testament",
        "GRAD" => "Ukończenie studiów",
        "RETI" => "Przejście na emeryturę",
        "CAST" => "Stan",
        "DSCR" => "Opis",
        "PROP" => "Własność",
        _ => "Wydarzenie",
    }
}

/// Facts that belong with the basics, next to birth and death.
fn is_life_event(kind: &str) -> bool {
    matches!(kind, "CHR" | "BAPM" | "BURI" | "CREM")
}

/// A fact as a line: its text, then when and where.
fn fact_line(f: &FactView) -> Option<View> {
    let when = when_where(&f.event);
    let text = match (f.value.is_empty(), when.is_empty()) {
        (false, false) => format!("{} ({when})", f.value),
        (false, true) => f.value.clone(),
        (true, false) => when,
        (true, true) => String::new(),
    };
    kv(fact_label(&f.kind), &text)
}

fn names_line(label: &str, names: &[String]) -> Option<View> {
    kv(label, &names.join(", "))
}

fn sex_word(
    c: &Card,
    male: &'static str,
    female: &'static str,
    either: &'static str,
) -> &'static str {
    match c.sex.as_str() {
        "M" => male,
        "F" => female,
        _ => either,
    }
}

fn citation_item(context: &str, c: &genevia_core::wire::CitationView) -> View {
    let mut item = li().child(strong().text(c.title.clone()));
    if !c.detail.is_empty() {
        item = item.child(span().text(format!(" — {}", c.detail)));
    }
    item.child(span().class("ctx").text(format!("  {context}")))
        .into_view()
}

/// A small button that opens a dialog.
fn action(label: &str, dialog: Dialog, sh: &Shared) -> View {
    let open = sh.dialog.clone();
    button()
        .class("fui-btn fui-btn-secondary fui-btn-sm")
        .attr("type", "button")
        .text(label.to_string())
        .on_click(move || open.set(Some(dialog.clone())))
        .into_view()
}

/// `view` when the person may change the tree, nothing otherwise.
fn gate(editable: bool, view: View) -> View {
    if editable { view } else { View::Empty }
}

fn add_action(label: &str, of: &str, relation: &str, family: &str, sh: &Shared) -> View {
    action(
        label,
        Dialog::AddRelative {
            of: of.to_string(),
            relation: relation.to_string(),
            family: family.to_string(),
        },
        sh,
    )
}

fn detail_view(d: &PersonDetail, at_: &At, sh: &Shared) -> View {
    let c = &d.card;
    let ed = d.editable;
    let full_name = [d.prefix.as_str(), c.name.as_str(), d.suffix.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let mut head = div()
        .class("detail")
        .child(h2().class(sex_class(c)).text(full_name));
    let mut aliases = Vec::new();
    if !d.maiden_name.is_empty() {
        aliases.push(format!("z domu {}", d.maiden_name));
    }
    if !d.nickname.is_empty() {
        aliases.push(format!("„{}”", d.nickname));
    }
    if !d.akas.is_empty() {
        aliases.push(format!("także: {}", d.akas.join(", ")));
    }
    if !aliases.is_empty() {
        head = head.child(p().class("muted aliases").text(aliases.join(" · ")));
    }
    if let Some(n) = d.ahn {
        head = head.child(
            p().class("muted")
                .text(format!("Numer w tablicy przodków: {n}")),
        );
    }
    head = head.child(div().class("actions").child(gate(
        ed,
        action("Edytuj osobę", Dialog::EditPerson(c.xref.clone()), sh),
    )));

    // The basics, in the order MyHeritage shows them.
    let mut basics: Vec<View> = Vec::new();
    basics.extend(kv("Płeć", sex_word(c, "mężczyzna", "kobieta", "")));
    if let Some(b) = &d.birth {
        basics.extend(kv(
            sex_word(c, "Urodzony", "Urodzona", "Urodzony/a"),
            &when_where(b),
        ));
        basics.extend(names_line("Chrzestni", &b.witnesses));
    }
    for f in d
        .facts
        .iter()
        .filter(|f| matches!(f.kind.as_str(), "CHR" | "BAPM"))
    {
        basics.extend(fact_line(f));
        basics.extend(names_line("Chrzestni", &f.event.witnesses));
    }
    if let Some(x) = &d.death {
        basics.extend(kv(
            sex_word(c, "Zmarły", "Zmarła", "Zmarł(a)"),
            &when_where(x),
        ));
        basics.extend(kv("Przyczyna zgonu", &x.cause));
    }
    for f in d
        .facts
        .iter()
        .filter(|f| matches!(f.kind.as_str(), "BURI" | "CREM"))
    {
        let mut line = f.clone();
        line.kind = if f.kind == "BURI" {
            sex_word(c, "Pochowany", "Pochowana", "Pochowany/a")
        } else {
            "Kremacja"
        }
        .to_string();
        let when = when_where(&line.event);
        let text = [line.value.as_str(), when.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        basics.extend(kv(&line.kind, &text));
    }
    let basics = block("Podstawowe informacje", basics);

    // Everything else a person was or did.
    let facts: Vec<View> = d
        .facts
        .iter()
        .filter(|f| !is_life_event(&f.kind))
        .filter_map(fact_line)
        .collect();
    let facts = block("Fakty i zdarzenia", facts);

    let mut parents = Vec::new();
    if let Some(f) = &d.father {
        parents.push(person_button(f, sh, "ojciec"));
    }
    if let Some(m) = &d.mother {
        parents.push(person_button(m, sh, "matka"));
    }
    if d.father.is_none() {
        parents.push(gate(
            ed,
            add_action("Dodaj ojca", &c.xref, "father", "", sh),
        ));
    }
    if d.mother.is_none() {
        parents.push(gate(
            ed,
            add_action("Dodaj matkę", &c.xref, "mother", "", sh),
        ));
    }
    let parents = block(
        "Rodzice",
        vec![div().class("rows").children(parents).into_view()],
    );

    let unions: Vec<View> = d
        .unions
        .iter()
        .filter_map(|u| {
            let mut rows = Vec::new();
            if let Some(m) = &u.marriage {
                rows.extend(kv("Ślub", &when_where(m)));
                rows.extend(names_line("Świadkowie", &m.witnesses));
            }
            if let Some(s) = &u.spouse {
                rows.push(person_button(s, sh, "małżonek"));
            }
            rows.extend(u.children.iter().map(|k| person_button(k, sh, "dziecko")));
            rows.push(
                div()
                    .class("actions")
                    .child(gate(
                        ed,
                        action(
                            "Edytuj małżeństwo",
                            Dialog::EditFamily(u.family.clone()),
                            sh,
                        ),
                    ))
                    .child(gate(
                        ed,
                        add_action("Dodaj dziecko", &c.xref, "child", &u.family, sh),
                    ))
                    .into_view(),
            );
            block("Małżeństwo", rows)
        })
        .collect();

    // Notes: the person's own, then the ones under their events.
    let mut notes: Vec<View> = d
        .notes
        .iter()
        .map(|n| li().text(n.clone()).into_view())
        .collect();
    let mut event_notes = |context: &str, e: &EventView| {
        notes.extend(e.notes.iter().map(|n| {
            li().child(span().text(n.clone()))
                .child(span().class("ctx").text(format!("  {context}")))
                .into_view()
        }));
    };
    if let Some(b) = &d.birth {
        event_notes("urodzenie", b);
    }
    if let Some(x) = &d.death {
        event_notes("zgon", x);
    }
    for f in &d.facts {
        event_notes(&fact_label(&f.kind).to_lowercase(), &f.event);
    }
    for u in &d.unions {
        if let Some(m) = &u.marriage {
            event_notes("ślub", m);
        }
    }
    let notes = if notes.is_empty() {
        None
    } else {
        block(
            "Notatki",
            vec![ul().class("plain").children(notes).into_view()],
        )
    };

    // Sources: the person's, then each event's.
    let mut cites: Vec<View> = d
        .sources
        .iter()
        .map(|c| citation_item("osoba", c))
        .collect();
    if let Some(b) = &d.birth {
        cites.extend(b.sources.iter().map(|c| citation_item("urodzenie", c)));
    }
    if let Some(x) = &d.death {
        cites.extend(x.sources.iter().map(|c| citation_item("zgon", c)));
    }
    for f in &d.facts {
        cites.extend(
            f.event
                .sources
                .iter()
                .map(|c| citation_item(&fact_label(&f.kind).to_lowercase(), c)),
        );
    }
    for u in &d.unions {
        if let Some(m) = &u.marriage {
            cites.extend(m.sources.iter().map(|c| citation_item("ślub", c)));
        }
    }
    let sources = if cites.is_empty() {
        None
    } else {
        block(
            "Źródła",
            vec![ul().class("plain").children(cites).into_view()],
        )
    };

    let mut more = vec![add_action("Dodaj małżonka", &c.xref, "spouse", "", sh)];
    if d.unions.is_empty() {
        more.push(add_action("Dodaj dziecko", &c.xref, "child", "", sh));
    }
    let relatives = if ed {
        block(
            "Rodzina",
            vec![div().class("actions").children(more).into_view()],
        )
    } else {
        None
    };
    let remove = {
        let open = sh.dialog.clone();
        let x = c.xref.clone();
        button()
            .class("fui-btn fui-btn-tertiary fui-btn-sm gen-delete")
            .text("Usuń osobę")
            .on_click(move || open.set(Some(Dialog::DeletePerson(x.clone()))))
            .into_view()
    };
    let (router, to) = (
        use_router(),
        url(
            &at_.tree,
            if at_.view == "list" {
                "pedigree"
            } else {
                &at_.view
            },
            &c.xref,
        ),
    );
    let close = sh.selected.clone();
    let reroot = button()
        .class("fui-btn fui-btn-primary fui-btn-block gen-reroot")
        .text("Pokaż przodków tej osoby")
        .on_click(move || {
            // Close the panel, so the change of root is what you see.
            close.set(None);
            router.navigate(&to);
        });

    head.children(
        [basics, facts, parents]
            .into_iter()
            .flatten()
            .chain(unions)
            .chain([notes, sources, relatives].into_iter().flatten()),
    )
    .child(reroot)
    .child(gate(ed, remove))
    .into_view()
}

fn drawer(ctx: &RouteContext, sh: &Shared) -> View {
    let at_ = create_memo({
        let ctx = ctx.clone();
        move || location_at(&ctx)
    });
    let selected = sh.selected.clone();
    let version = sh.version.clone();
    let detail = resource(
        {
            let (selected, at_, version) = (selected.clone(), at_.clone(), version);
            move || (at_.get().tree, selected.get(), version.get())
        },
        |(tree, who, _): (String, Option<String>, u32)| async move {
            match who {
                None => Ok::<Option<PersonDetail>, String>(None),
                Some(x) => person(tree, x)
                    .await
                    .map(Some)
                    .map_err(|e| e.message().to_string()),
            }
        },
    );
    let (close, open) = (selected.clone(), selected);
    let sh = sh.clone();
    div()
        .class("drawer")
        .class_if("open", move || open.get().is_some())
        .child(
            button()
                .class("close fui-icon-btn")
                .attr("title", "Zamknij")
                .text("✕")
                .on_click(move || close.set(None)),
        )
        .child(View::dynamic(move || match detail.get() {
            Load::Loading | Load::Ready(None) => View::Empty,
            Load::Failed(e) => p().class("error").text(e).into_view(),
            Load::Ready(Some(d)) => detail_view(&d, &location_at_cached(&at_), &sh),
        }))
        .into_view()
}

fn location_at_cached(at_: &Memo<At>) -> At {
    at_.get_untracked()
}

/// The frame of every `/t/<tree>/...` address: the toolbar, the body and the side panel.
#[must_use]
pub fn frame(ctx: &RouteContext, inner: View, sh: &Shared) -> View {
    div()
        .class("gen-tree")
        .child(toolbar(ctx, sh))
        .child(
            div()
                .class("content")
                .child(element("main").class("fui-client-main").child(inner))
                .child(drawer(ctx, sh)),
        )
        .child(crate::dialogs::layer(ctx, sh))
        .into_view()
}

/// The body for the address: loads the ancestors and shows the chosen view.
#[must_use]
pub fn body(ctx: &RouteContext, sh: &Shared) -> View {
    let at_ = create_memo({
        let ctx = ctx.clone();
        move || location_at(&ctx)
    });
    let (ped, fan, version) = (
        sh.pedigree_gens.clone(),
        sh.fan_gens.clone(),
        sh.version.clone(),
    );
    let data: Signal<Load<Ancestry>> = resource(
        {
            let at_ = at_.clone();
            move || {
                let w = at_.get();
                let gens = match w.view.as_str() {
                    "pedigree" => ped.get(),
                    "fan" => fan.get(),
                    _ => 12,
                };
                let _ = version.get();
                (w.tree, w.xref, gens)
            }
        },
        |(tree, xref, gens): (String, String, u32)| async move {
            ancestry(tree, xref, gens)
                .await
                .map_err(|e| e.message().to_string())
        },
    );
    let sh = sh.clone();
    View::dynamic(move || match at_.get().view.as_str() {
        "pedigree" => pedigree_view(data.clone(), &sh),
        "fan" => fan_view(data.clone(), &sh),
        _ => list_view(data.clone(), &sh),
    })
}
