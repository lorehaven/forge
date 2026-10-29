//! The message that comes up over the bottom of the screen ("Sign in again", "Downloaded ...") and goes away by itself.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use quench_ui::platform::{Timer, after};
use quench_ui::prelude::*;

/// Whether `text` tells of something that went wrong, which the toast shows in the warning colours.
#[must_use]
pub fn is_problem(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "sign in again",
        "failed",
        "could not",
        "cannot",
        "wrong",
        "error",
        "not available",
        "unavailable",
        "refused",
        "no server",
        "no storage",
        "full",
        "locked",
        "disabled",
        "confirm the email",
    ]
    .iter()
    .any(|word| text.contains(word))
}

/// The toast: nothing while `message` is empty, else a rounded card over the bottom of the screen (above the bar).
#[must_use]
pub fn toast(message: &Signal<String>) -> View {
    let (when, text, class) = (message.clone(), message.clone(), message.clone());
    View::show(
        move || !when.get().is_empty(),
        move || {
            let (text, class) = (text.clone(), class.clone());
            div()
                .attr("id", "message")
                .attr("role", "status")
                .attr_fn("class", move || {
                    if is_problem(&class.get()) {
                        "forklift-toast forklift-toast-problem".to_string()
                    } else {
                        "forklift-toast".to_string()
                    }
                })
                .text_fn(move || text.get())
                .into_view()
        },
    )
}

/// `busy`, kept up for at least `min_ms` once it has come on, so that quick work still shows the loader. With
/// `min_ms` of 0 it is `busy` itself.
#[must_use]
pub fn lingering(busy: &Signal<bool>, min_ms: u32) -> Signal<bool> {
    if min_ms == 0 {
        return busy.clone();
    }
    let shown = create_signal(false);
    let (busy, out) = (busy.clone(), shown.clone());
    let since: Rc<Cell<Option<Instant>>> = Rc::default();
    create_effect(move || {
        if busy.get() {
            if since.get().is_none() {
                since.set(Some(Instant::now()));
            }
            out.set(true);
        } else if let Some(at) = since.take() {
            let left = u128::from(min_ms).saturating_sub(at.elapsed().as_millis());
            if left == 0 {
                out.set(false);
            } else {
                let out = out.clone();
                after(left as u32, move || out.set(false));
            }
        }
    });
    shown
}

/// Clears `message` `ms` milliseconds after it was set, unless something else was said meanwhile.
pub fn dismiss_after(message: &Signal<String>, ms: u32) {
    let message = message.clone();
    let pending: Rc<Cell<Option<Timer>>> = Rc::default();
    create_effect(move || {
        let text = message.get();
        if let Some(timer) = pending.take() {
            timer.cancel();
        }
        if text.is_empty() {
            return;
        }
        let message = message.clone();
        pending.set(Some(after(ms, move || {
            if message.get_untracked() == text {
                message.set(String::new());
            }
        })));
    });
}
