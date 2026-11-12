//! A zoomable, draggable frame around a chart: `+`, `−` and `⟲` buttons, drag to pan, and
//! the buttons zoom. (Ctrl + wheel zooms about the pointer once quench-ui releases wheel events.)

use std::cell::Cell;
use std::rc::Rc;

use quench_ui::prelude::*;
use quench_ui::{Event, EventKind};

pub const MAX_ZOOM: f64 = 5.0;
pub const MIN_ZOOM: f64 = 0.3;

#[derive(Clone)]
#[allow(missing_debug_implementations)] // signals have no `Debug`
pub struct Viewport {
    pub zoom: Signal<f64>,
    pub pan: Signal<(f64, f64)>,
    /// Set while a drag is going on or has just ended, so the click that ends it selects nothing.
    pub dragged: Rc<Cell<bool>>,
}

impl Viewport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            zoom: create_signal(1.0),
            pan: create_signal((0.0, 0.0)),
            dragged: Rc::new(Cell::new(false)),
        }
    }

    /// Zoom rounded to quarters: what to size things by when rebuilding on every wheel tick would
    /// be too much.
    #[must_use]
    pub fn coarse_zoom(&self) -> Memo<f64> {
        let zoom = self.zoom.clone();
        create_memo(move || (zoom.get() * 4.0).round() / 4.0)
    }

    pub fn reset(&self) {
        self.zoom.set(1.0);
        self.pan.set((0.0, 0.0));
    }

    /// The frame. `background` sits under the content and sees the pointer where the content does
    /// not; `anchor` is where the content starts (`"centre"` or `"top-left"`).
    #[must_use]
    pub fn frame(&self, class: &str, anchor: &str, background: Element, content: View) -> View {
        let (zoom, pan, dragged) = (self.zoom.clone(), self.pan.clone(), self.dragged.clone());
        // Where a drag started: the pointer and the pan at that moment.
        let start: Rc<Cell<Option<((f64, f64), (f64, f64))>>> = Rc::new(Cell::new(None));

        let (z_in, z_out, me) = (zoom.clone(), zoom.clone(), self.clone());
        let controls = div()
            .class("vp-zoom")
            .child(
                button()
                    .class("fui-btn fui-btn-secondary fui-btn-sm")
                    .attr("title", "Powiększ")
                    .text("+")
                    .on_click(move || z_in.update(|z| *z = (*z * 1.35).min(MAX_ZOOM))),
            )
            .child(
                button()
                    .class("fui-btn fui-btn-secondary fui-btn-sm")
                    .attr("title", "Pomniejsz")
                    .text("−")
                    .on_click(move || z_out.update(|z| *z = (*z / 1.35).max(MIN_ZOOM))),
            )
            .child(
                button()
                    .class("fui-btn fui-btn-secondary fui-btn-sm")
                    .attr("title", "Wyśrodkuj")
                    .text("⟲")
                    .on_click(move || me.reset()),
            );

        let (down_start, down_pan, down_moved) = (start.clone(), pan.clone(), dragged.clone());
        let (move_start, move_pan, move_moved) = (start.clone(), pan.clone(), dragged);
        let (up_start, cancel_start) = (start.clone(), start);
        let (t_zoom, t_pan) = (zoom, pan);
        div()
            .class(format!("vp {class}"))
            .on(EventKind::PointerDown, move |e| {
                if let Event::PointerDown(p) = e {
                    down_moved.set(false);
                    down_start.set(Some(((p.x, p.y), down_pan.get_untracked())));
                }
            })
            .on(EventKind::PointerMove, move |e| {
                if let (Event::PointerMove(p), Some(((sx, sy), (px, py)))) = (e, move_start.get()) {
                    let (dx, dy) = (p.x - sx, p.y - sy);
                    if dx.abs() + dy.abs() > 4.0 {
                        move_moved.set(true);
                    }
                    if move_moved.get() {
                        move_pan.set((px + dx, py + dy));
                    }
                }
            })
            .on(EventKind::PointerUp, move |_| up_start.set(None))
            .on(EventKind::PointerCancel, move |_| cancel_start.set(None))
            .child(background.class("vp-bg"))
            .child(
                div()
                    .class(format!("vp-pan {anchor}"))
                    .css("transform", move || {
                        let (x, y) = t_pan.get();
                        let origin =
                            format!("translate({x:.1}px,{y:.1}px) scale({:.3})", t_zoom.get());
                        origin
                    })
                    .child(content),
            )
            .child(controls)
            .into_view()
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}
