//! The ancestors view: the person on the left, parents stacked to the right, generation by generation.

use std::collections::HashMap;
use std::rc::Rc;

use genevia_core::wire::{AncestorSlot, Ancestry, Card};
use quench_ui::prelude::*;
use quench_ui::router::use_router;

use crate::state::{Load, Shared};
use crate::util::{lifespan, sex_class, url};
use crate::viewport::Viewport;

type Slots = Rc<HashMap<u32, AncestorSlot>>;

fn card_view(c: &Card, remaining: u32, tree: &str, sh: &Shared, vp: &Viewport) -> Element {
    let (xref, sel, pick) = (c.xref.clone(), sh.selected.clone(), sh.selected.clone());
    let me = c.xref.clone();
    let dragged = vp.dragged.clone();
    let mut e = div()
        .class(format!("pcard {}", sex_class(c)))
        .class_if("sel", move || sel.get().as_deref() == Some(xref.as_str()))
        .attr("title", format!("{} {}", c.name, lifespan(c)))
        .on_click(move || {
            if !dragged.get() {
                pick.set(Some(me.clone()));
            }
        })
        .child(div().class("pname").text(c.name.clone()))
        .child(div().class("pyears").text(lifespan(c)))
        .child(div().class("pplace").text(c.birth_place.clone()));
    // Parents exist but are beyond the generations shown: offer to move the tree up to this person.
    if remaining == 0 && c.has_parents {
        let (router, to, dragged) = (
            use_router(),
            url(tree, "pedigree", &c.xref),
            vp.dragged.clone(),
        );
        e = e.child(
            button()
                .class("more")
                .attr("title", "Pokaż przodków tej osoby")
                .child(forge_shell::icon("arrow-right"))
                .intercept_click()
                .on_click(move || {
                    if !dragged.get() {
                        router.navigate(&to);
                    }
                }),
        );
    }
    e
}

fn node(slots: &Slots, ahn: u32, remaining: u32, tree: &str, sh: &Shared, vp: &Viewport) -> View {
    let Some(slot) = slots.get(&ahn) else {
        return div()
            .class("pnode ghost")
            .child(div().class("pcard u").text("nieznany"))
            .into_view();
    };
    let mut n = div()
        .class("pnode")
        .child(card_view(&slot.card, remaining, tree, sh, vp));
    if remaining > 0 {
        let (f, m) = (
            slots.contains_key(&(ahn * 2)),
            slots.contains_key(&(ahn * 2 + 1)),
        );
        if f || m {
            n = n.child(
                div()
                    .class("pparents")
                    .child(node(slots, ahn * 2, remaining - 1, tree, sh, vp))
                    .child(node(slots, ahn * 2 + 1, remaining - 1, tree, sh, vp)),
            );
        }
    }
    n.into_view()
}

#[must_use]
pub fn pedigree_view(data: Signal<Load<Ancestry>>, sh: &Shared) -> View {
    let sh = sh.clone();
    let vp = Viewport::new();
    let content = {
        let vp = vp.clone();
        View::dynamic(move || match data.get() {
            Load::Loading => p().class("muted").text("Wczytywanie…").into_view(),
            Load::Failed(e) => p().class("error").text(e).into_view(),
            Load::Ready(a) => {
                let gens = sh.pedigree_gens.get();
                let slots: Slots = Rc::new(a.slots.iter().cloned().map(|s| (s.ahn, s)).collect());
                div()
                    .class(format!("pedigree lv{}", gens + 1))
                    .child(node(&slots, 1, gens, &a.tree, &sh, &vp))
                    .into_view()
            }
        })
    };
    vp.frame("pedwrap", "top-left", div(), content)
}
