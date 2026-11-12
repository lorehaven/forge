//! The list view: every ancestor, one row each, in Ahnentafel order and grouped by generation.

use genevia_core::text::fold;
use genevia_core::wire::{AncestorSlot, Ancestry};
use quench_ui::prelude::*;

use crate::state::{Load, Shared};
use crate::util::{generation_name, line_of, sex_class};

fn th_(text: &str) -> Element {
    element("th").text(text)
}

fn td_(text: impl Into<String>) -> Element {
    element("td").text(text)
}

fn matches(slot: &AncestorSlot, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let c = &slot.card;
    let hay = fold(&format!("{} {} {}", c.name, c.birth_place, slot.ahn));
    needle.split_whitespace().all(|w| hay.contains(w))
}

fn line_badge(ahn: u32) -> View {
    match line_of(ahn) {
        "" => View::Empty,
        l => span()
            .class(format!(
                "fui-badge {}",
                if l == "ojcowska" {
                    "fui-badge-info"
                } else {
                    "gen-badge-mat"
                }
            ))
            .text(l)
            .into_view(),
    }
}

fn end_badge(has_parents: bool) -> View {
    if has_parents {
        View::Empty
    } else {
        span()
            .class("fui-badge fui-badge-warning")
            .text("koniec linii")
            .into_view()
    }
}

fn row(slot: &AncestorSlot, sh: &Shared) -> View {
    let c = &slot.card;
    let xref = c.xref.clone();
    let (sel, pick) = (sh.selected.clone(), sh.selected.clone());
    let me = xref.clone();
    element("tr")
        .class(format!("person {}", sex_class(c)))
        .class_if("sel", move || sel.get().as_deref() == Some(xref.as_str()))
        .on_click(move || pick.set(Some(me.clone())))
        .child(td_(slot.ahn.to_string()).class("ahn"))
        .child(
            element("td")
                .class("name")
                .child(span().text(c.name.clone())),
        )
        .child(td_(c.born.clone()))
        .child(td_(c.died.clone()))
        .child(td_(c.birth_place.clone()))
        .child(element("td").child(line_badge(slot.ahn)))
        .child(element("td").child(end_badge(c.has_parents)))
        .into_view()
}

fn table_of(a: &Ancestry, sh: &Shared) -> View {
    let slots = a.slots.clone();
    let sh = sh.clone();
    let filter = sh.filter.clone();
    let body = View::dynamic(move || {
        let needle = fold(filter.get().trim());
        let mut rows: Vec<View> = Vec::new();
        let mut current = u32::MAX;
        for s in slots.iter().filter(|s| matches(s, &needle)) {
            if s.generation != current {
                current = s.generation;
                let have = slots.iter().filter(|x| x.generation == current).count();
                let of = 1_u64 << current;
                let title = if current == 0 {
                    generation_name(0)
                } else {
                    format!("{} · {} z {}", generation_name(current), have, of)
                };
                rows.push(
                    element("tr")
                        .class("gen")
                        .child(
                            element("th")
                                .attr("colspan", "7")
                                .text(format!("{current}. {title}")),
                        )
                        .into_view(),
                );
            }
            rows.push(row(s, &sh));
        }
        if rows.is_empty() {
            rows.push(
                element("tr")
                    .child(
                        td_("Nic nie pasuje do filtra.")
                            .attr("colspan", "7")
                            .class("muted"),
                    )
                    .into_view(),
            );
        }
        fragment(rows)
    });
    element("table")
        .class("ahnen fui-table")
        .child(
            element("thead").child(
                element("tr")
                    .child(th_("Nr"))
                    .child(th_("Imię i nazwisko"))
                    .child(th_("Urodzony"))
                    .child(th_("Zmarły"))
                    .child(th_("Miejsce urodzenia"))
                    .child(th_("Linia"))
                    .child(th_("")),
            ),
        )
        .child(element("tbody").child(body))
        .into_view()
}

#[must_use]
pub fn list_view(data: Signal<Load<Ancestry>>, sh: &Shared) -> View {
    let sh = sh.clone();
    let filter = sh.filter.clone();
    let typed = filter.clone();
    let bar = div().class("filterbar").child(
        input()
            .class("fui-input fui-input-sm")
            .attr("type", "search")
            .attr("placeholder", "Filtruj po nazwisku, miejscu, numerze…")
            .attr_fn("value", move || filter.get())
            .on_input(move |v| typed.set(v.to_string())),
    );
    let content = View::dynamic(move || match data.get() {
        Load::Loading => p().class("muted").text("Wczytywanie…").into_view(),
        Load::Failed(e) => p().class("error").text(e).into_view(),
        Load::Ready(a) => table_of(&a, &sh),
    });
    div()
        .class("listview")
        .child(bar)
        .child(
            div()
                .class("tablewrap fui-card fui-flush fui-table-wrap")
                .child(content),
        )
        .into_view()
}
