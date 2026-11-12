//! Data the screens wait for, and what they share.

use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;

use quench_ui::prelude::*;

use crate::api::spawn_local;

/// A value that arrives from the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Load<T> {
    Loading,
    Ready(T),
    Failed(String),
}

/// Fetches whenever what `key` reads changes; an answer that arrives after a newer request was
/// made is dropped.
pub fn resource<K, T, F, Fut>(key: impl Fn() -> K + 'static, fetch: F) -> Signal<Load<T>>
where
    K: 'static,
    T: 'static,
    F: Fn(K) -> Fut + 'static,
    Fut: Future<Output = Result<T, String>> + 'static,
{
    let out = create_signal(Load::Loading);
    let latest = Rc::new(Cell::new(0_u32));
    let sink = out.clone();
    create_effect(move || {
        let k = key();
        let n = latest.get().wrapping_add(1);
        latest.set(n);
        sink.set(Load::Loading);
        let (sink, latest) = (sink.clone(), latest.clone());
        let work = fetch(k);
        spawn_local(async move {
            let result = work.await;
            if latest.get() == n {
                sink.set(match result {
                    Ok(v) => Load::Ready(v),
                    Err(e) => Load::Failed(e),
                });
            }
        });
    });
    out
}

/// A dialog that is open over the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    EditPerson(String),
    EditFamily(String),
    /// Add a `father`, `mother`, `spouse` or `child` next to `of`; a child goes into `family`
    /// (empty: their only family, or a new one).
    AddRelative {
        of: String,
        relation: String,
        family: String,
    },
    DeletePerson(String),
    DeleteFamily(String),
}

/// State that lives across pages.
#[derive(Clone)]
#[allow(missing_debug_implementations)] // signals have no `Debug`
pub struct Shared {
    /// The person whose details are open in the side panel.
    pub selected: Signal<Option<String>>,
    /// How many generations the pedigree and fan show.
    pub pedigree_gens: Signal<u32>,
    pub fan_gens: Signal<u32>,
    /// Total angle of the fan, in degrees.
    pub fan_span: Signal<u32>,
    /// The filter box of the list view.
    pub filter: Signal<String>,
    /// The dialog open over the tree, if any.
    pub dialog: Signal<Option<Dialog>>,
    /// Counts changes made by editing, so what was read before is read again.
    pub version: Signal<u32>,
}

impl Shared {
    #[must_use]
    pub fn new() -> Self {
        Self {
            selected: create_signal(None),
            pedigree_gens: create_signal(4),
            fan_gens: create_signal(6),
            fan_span: create_signal(270),
            filter: create_signal(String::new()),
            dialog: create_signal(None),
            version: create_signal(0),
        }
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}
