//! The fan view: the person in the middle, one ring per generation, the father's line on the left
//! and the mother's on the right. Built from plain elements clipped to ring segments, so it needs
//! nothing beyond HTML and CSS. Labels are fitted to their sectors (`genevia_core::fan::plan`);
//! hovering a sector shows the person in the middle; the chart can be zoomed and dragged.

use std::cell::Cell;
use std::rc::Rc;

use genevia_core::fan::{self, Role, Sector};
use genevia_core::wire::{AncestorSlot, Ancestry, Card};
use quench_ui::EventKind;
use quench_ui::prelude::*;

use crate::state::{Load, Shared};
use crate::util::{lifespan, sex_class};
use crate::viewport::Viewport;

const HOLE: f64 = 11.0;
const GAP: f64 = 0.25;

fn polygon(s: &Sector) -> String {
    // A hair of space between neighbours, so the segments read as separate.
    let angle_gap = if s.generation == 0 {
        0.0
    } else {
        (GAP * 50.0 / s.outer).min(1.0)
    };
    let inner = if s.inner > 0.0 { s.inner + GAP } else { 0.0 };
    let t = Sector {
        start: s.start + angle_gap,
        end: s.end - angle_gap,
        inner,
        outer: s.outer - GAP,
        ..*s
    };
    let pts: Vec<String> = fan::outline(&t, 2.0)
        .into_iter()
        .map(|(x, y)| format!("{x:.2}% {y:.2}%"))
        .collect();
    format!("polygon({})", pts.join(","))
}

struct Interaction {
    selected: Signal<Option<String>>,
    hover: Signal<Option<Card>>,
    /// Set while a drag is going on or has just ended, so the click that ends it selects nothing.
    dragged: Rc<Cell<bool>>,
    zoom: f64,
}

fn segment(slot: &AncestorSlot, gens: u32, arc: f64, ix: &Interaction) -> Vec<View> {
    let s = fan::sector(slot.ahn, gens, arc, HOLE);
    let c = &slot.card;
    let line = match slot.ahn >> slot.generation.saturating_sub(1) {
        2 if slot.generation > 0 => "pat",
        3 if slot.generation > 0 => "mat",
        _ => "root",
    };
    let (xref, sel, pick) = (c.xref.clone(), ix.selected.clone(), ix.selected.clone());
    let me = c.xref.clone();
    let (dragged, hover, card) = (ix.dragged.clone(), ix.hover.clone(), c.clone());
    let shape = polygon(&s);
    let region = div()
        .class(format!("fsec {line} g{} {}", slot.generation, sex_class(c)))
        .class_if("sel", move || sel.get().as_deref() == Some(xref.as_str()))
        .attr("title", format!("{} {}", c.name, lifespan(c)))
        .css("clip-path", move || shape.clone())
        .on(EventKind::PointerMove, move |_| {
            hover.set_if_changed(Some(card.clone()));
        })
        .on_click(move || {
            if !dragged.get() {
                pick.set(Some(me.clone()));
            }
        });
    let mut out = vec![region.into_view()];

    let l = fan::label(&s);
    if let Some(plan) = fan::plan(&s, &l, &c.given, &c.surname, &lifespan(c), ix.zoom) {
        let (x, y) = (format!("{:.2}%", l.x), format!("{:.2}%", l.y));
        let transform = format!("translate(-50%,-50%) rotate({:.1}deg)", l.rotation);
        let size = format!("{:.3}cqw", plan.font);
        let lines: Vec<View> = plan
            .lines
            .iter()
            .map(|ln| {
                let class = match ln.role {
                    Role::Given => "fn",
                    Role::Surname => "sn",
                    Role::Years => "yr",
                    Role::Name => "nm",
                };
                span().class(class).text(ln.text.clone()).into_view()
            })
            .collect();
        out.push(
            div()
                .class("flabel")
                .css("left", move || x.clone())
                .css("top", move || y.clone())
                .css("transform", move || transform.clone())
                .css("font-size", move || size.clone())
                .children(lines)
                .into_view(),
        );
    }
    out
}

/// The middle of the chart: whoever the pointer is over, otherwise the person the chart is about.
fn centre(root: &Card, hover: Signal<Option<Card>>) -> View {
    let root = root.clone();
    View::dynamic(move || {
        let c = hover.get().unwrap_or_else(|| root.clone());
        div()
            .class(format!("fcentre {}", sex_class(&c)))
            .child(div().class("cn").text(c.name.clone()))
            .child(div().class("cy").text(lifespan(&c)))
            .child(div().class("cp").text(c.birth_place.clone()))
            .into_view()
    })
}

fn chart(a: &Ancestry, sh: &Shared, ix: &Interaction) -> View {
    let gens = sh.fan_gens.get();
    let arc = f64::from(sh.fan_span.get());
    let parts: Vec<View> = a
        .slots
        .iter()
        .filter(|s| s.generation <= gens)
        .flat_map(|s| segment(s, gens, arc, ix))
        .collect();
    div()
        .class("fan")
        .children(parts)
        .child(centre(&a.root, ix.hover.clone()))
        .into_view()
}

#[must_use]
pub fn fan_view(data: Signal<Load<Ancestry>>, sh: &Shared) -> View {
    let sh = sh.clone();
    let vp = Viewport::new();
    let hover: Signal<Option<Card>> = create_signal(None);
    // Labels are fitted for a zoom rounded to quarters, so a smooth pinch rebuilds them a few
    // times, not on every tick.
    let fit_zoom = vp.coarse_zoom();
    let dragged = vp.dragged.clone();
    let content = View::dynamic({
        let hover = hover.clone();
        move || match data.get() {
            Load::Loading => p().class("muted").text("Wczytywanie…").into_view(),
            Load::Failed(e) => p().class("error").text(e).into_view(),
            Load::Ready(a) => {
                let ix = Interaction {
                    selected: sh.selected.clone(),
                    hover: hover.clone(),
                    dragged: dragged.clone(),
                    zoom: fit_zoom.get(),
                };
                chart(&a, &sh, &ix)
            }
        }
    });
    // Moving over the background, rather than a sector, clears the preview in the middle.
    let background = div().on(EventKind::PointerMove, move |_| hover.set_if_changed(None));
    vp.frame("fanwrap", "centre", background, content)
}
