//! The list of alarms and the editor, as views over plain data.

use std::rc::Rc;

use klaxon_core::{Alarm, DAY_NAMES, Now, SNOOZE_CHOICES, clock, countdown, repeat_words};
use quench_ui::form::Field;
use quench_ui::kit::text_field;
use quench_ui::prelude::*;

use crate::icons;

/// An alarm being made or changed: its parts as signals, until Save.
#[derive(Clone)]
pub struct Draft {
    pub id: u32,
    /// A new alarm (no Delete for it).
    pub fresh: bool,
    pub hour: Signal<u8>,
    pub minute: Signal<u8>,
    pub days: Signal<u8>,
    pub snooze: Signal<u8>,
    pub vibrate: Signal<bool>,
    pub loud: Signal<bool>,
    /// The sound's URI (empty: the phone's default) and its name.
    pub sound: Signal<String>,
    pub sound_name: Signal<String>,
    pub enabled: bool,
    pub label: Field,
}

impl std::fmt::Debug for Draft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Draft")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl Draft {
    #[must_use]
    pub fn new(alarm: &Alarm, fresh: bool) -> Self {
        Self {
            id: alarm.id,
            fresh,
            hour: create_signal(alarm.hour),
            minute: create_signal(alarm.minute),
            days: create_signal(alarm.days),
            snooze: create_signal(alarm.snooze),
            vibrate: create_signal(alarm.vibrate),
            loud: create_signal(alarm.loud),
            sound: create_signal(alarm.sound.clone()),
            sound_name: create_signal(alarm.sound_name.clone()),
            enabled: alarm.enabled || fresh,
            label: Field::new("label", &alarm.label),
        }
    }

    /// The alarm as it is now; saving an alarm switches it on.
    #[must_use]
    pub fn alarm(&self) -> Alarm {
        Alarm {
            id: self.id,
            hour: self.hour.get_untracked(),
            minute: self.minute.get_untracked(),
            label: self.label.value_untracked().trim().to_string(),
            days: self.days.get_untracked(),
            enabled: true,
            snooze: self.snooze.get_untracked(),
            vibrate: self.vibrate.get_untracked(),
            loud: self.loud.get_untracked(),
            sound: self.sound.get_untracked(),
            sound_name: self.sound_name.get_untracked(),
        }
    }
}

/// Moves `value` by `by`, wrapping around `modulus`.
#[must_use]
pub fn wrap(value: u8, by: i16, modulus: u8) -> u8 {
    let m = i16::from(modulus);
    u8::try_from((i16::from(value) + by).rem_euclid(m)).unwrap_or(0)
}

/// The alarms by time of day.
#[must_use]
pub fn sorted(alarms: &[Alarm]) -> Vec<Alarm> {
    let mut list = alarms.to_vec();
    list.sort_by_key(|a| (a.minute_of_day(), a.id));
    list
}

fn switch(on: bool, id: u32, toggle: Rc<dyn Fn(u32, bool)>) -> Element {
    button()
        .attr("type", "button")
        .attr("role", "switch")
        .attr("id", format!("toggle-{id}"))
        .attr("aria-label", "On")
        .attr("aria-checked", on.to_string())
        .class("q-switch")
        .child(
            span()
                .class("q-switch-track")
                .attr("aria-hidden", "true")
                .child(span().class("q-switch-thumb")),
        )
        .on_click(move || toggle(id, !on))
}

/// The list: when the next alarm rings, and every alarm with its switch.
#[must_use]
pub fn list(
    alarms: &Signal<Vec<Alarm>>,
    now: &Signal<Now>,
    open: Rc<dyn Fn(u32)>,
    toggle: Rc<dyn Fn(u32, bool)>,
) -> View {
    let (head, rows) = ((alarms.clone(), now.clone()), (alarms.clone(), now.clone()));
    div()
        .attr("id", "alarms")
        .class("klaxon-screen")
        .child(
            div()
                .class("klaxon-card klaxon-next")
                .child(icons::bell("q-icon"))
                .child(
                    div().child(p().attr("id", "next").class("klaxon-headline").text_fn(
                        move || {
                            let (alarms, now) = (head.0.get(), head.1.get());
                            klaxon_core::next(&alarms, now).map_or_else(
                                || "No alarm is set".to_string(),
                                |n| {
                                    let minutes = u32::from(n.days_ahead) * 1440
                                        + u32::from(n.hour) * 60
                                        + u32::from(n.minute)
                                        - u32::from(now.minute);
                                    countdown(minutes)
                                },
                            )
                        },
                    )),
                ),
        )
        .child(View::dynamic(move || {
            let alarms = sorted(&rows.0.get());
            if alarms.is_empty() {
                return p()
                    .attr("id", "empty")
                    .class("klaxon-empty")
                    .text("No alarms yet. Press + to add one.")
                    .into_view();
            }
            ul().class("klaxon-list")
                .children(alarms.into_iter().map(|a| {
                    let (open, toggle) = (Rc::clone(&open), Rc::clone(&toggle));
                    let id = a.id;
                    let meta = if a.label.is_empty() {
                        repeat_words(a.days)
                    } else {
                        format!("{} \u{b7} {}", a.label, repeat_words(a.days))
                    };
                    li().attr("id", format!("alarm-{id}"))
                        .class(if a.enabled {
                            "klaxon-row"
                        } else {
                            "klaxon-row klaxon-row-off"
                        })
                        .child(
                            button()
                                .attr("type", "button")
                                .class("klaxon-row-main")
                                .child(
                                    span()
                                        .class("klaxon-row-time")
                                        .text(clock(a.hour, a.minute)),
                                )
                                .child(span().class("klaxon-row-meta").text(meta))
                                .on_click(move || open(id)),
                        )
                        .child(switch(a.enabled, id, toggle))
                        .into_view()
                }))
                .into_view()
        }))
        .into_view()
}

fn switch_row(id: &str, label: &str, on: &Signal<bool>) -> View {
    quench_ui::kit::switch(label, on)
        .attr("id", id.to_string())
        .into_view()
}

fn step(id: &str, text: &str, label: &str, on_click: impl Fn() + 'static) -> View {
    button()
        .attr("type", "button")
        .attr("id", id.to_string())
        .attr("aria-label", label.to_string())
        .text(text.to_string())
        .on_click(on_click)
        .into_view()
}

fn unit(name: &str, value: &Signal<u8>, steps: &[(i16, &str)], modulus: u8) -> View {
    let shown = value.clone();
    div()
        .class("klaxon-unit")
        .child(
            span()
                .attr("id", format!("{name}-value"))
                .class("klaxon-digits")
                .text_fn(move || format!("{:02}", shown.get())),
        )
        .child(
            div()
                .class("klaxon-steps")
                .children(steps.iter().map(|(by, text)| {
                    let value = value.clone();
                    let by = *by;
                    let label = format!(
                        "{} {name} {}",
                        if by > 0 { "Later" } else { "Earlier" },
                        by.abs()
                    );
                    step(
                        &format!("{name}-{}{}", if by > 0 { "up" } else { "down" }, by.abs()),
                        text,
                        &label,
                        move || value.set(wrap(value.get_untracked(), by, modulus)),
                    )
                })),
        )
        .into_view()
}

fn chip(id: String, text: &str, on: impl Fn() -> bool + 'static, tap: impl Fn() + 'static) -> View {
    let pressed = Rc::new(on);
    let aria = Rc::clone(&pressed);
    button()
        .attr("type", "button")
        .attr("id", id)
        .attr_fn("aria-pressed", move || aria().to_string())
        .attr_fn("class", move || {
            if pressed() {
                "klaxon-chip klaxon-chip-on".to_string()
            } else {
                "klaxon-chip".to_string()
            }
        })
        .text(text.to_string())
        .on_click(tap)
        .into_view()
}

/// The sound, vibration and loudness: what the alarm does when it rings.
fn sound_section(draft: &Draft, pick_sound: Rc<dyn Fn()>) -> View {
    let sound_name = draft.sound_name.clone();
    div()
        .child(h3().text("Sound"))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "sound")
                .class("klaxon-sound")
                .text_fn(move || {
                    let name = sound_name.get();
                    if name.is_empty() {
                        "Default alarm sound".to_string()
                    } else {
                        name
                    }
                })
                .on_click(move || pick_sound()),
        )
        .child(
            div()
                .class("klaxon-switches")
                .child(switch_row("vibrate", "Vibrate", &draft.vibrate))
                .child(switch_row("loud", "Extra loud", &draft.loud)),
        )
        .child(
            p().class("klaxon-sub")
                .text("Extra loud rings at the full alarm volume, whatever it is set to."),
        )
        .into_view()
}

/// The editor: the time, the label, the days, the snooze, and Save / Delete.
#[must_use]
pub fn editor(
    draft: &Draft,
    save: Rc<dyn Fn(Alarm)>,
    delete: Rc<dyn Fn(u32)>,
    cancel: &Rc<dyn Fn()>,
    pick_sound: Rc<dyn Fn()>,
) -> View {
    let days = |i: u8| {
        let (on, tap) = (draft.days.clone(), draft.days.clone());
        chip(
            format!("day-{i}"),
            &DAY_NAMES[usize::from(i)][..2],
            move || on.get() & (1 << i) != 0,
            move || tap.set(tap.get_untracked() ^ (1 << i)),
        )
    };
    let snoozes = SNOOZE_CHOICES.iter().map(|m| {
        let (on, tap, m) = (draft.snooze.clone(), draft.snooze.clone(), *m);
        chip(
            format!("snooze-{m}"),
            &m.to_string(),
            move || on.get() == m,
            move || tap.set(m),
        )
    });
    let words = draft.days.clone();
    let (do_save, d) = (save, draft.clone());
    let (do_delete, id) = (delete, draft.id);
    div()
        .attr("id", "editor")
        .class("klaxon-screen")
        .child(
            div()
                .class("klaxon-time")
                .child(unit(
                    "hour",
                    &draft.hour,
                    &[(-1, "\u{2212}1"), (1, "+1")],
                    24,
                ))
                .child(span().class("klaxon-colon").text(":"))
                .child(unit(
                    "minute",
                    &draft.minute,
                    &[(-5, "\u{2212}5"), (-1, "\u{2212}1"), (1, "+1"), (5, "+5")],
                    60,
                )),
        )
        .child(
            div()
                .class("klaxon-label")
                .child(text_field("Label", &draft.label, "text", None)),
        )
        .child(h3().text("Repeat"))
        .child(div().class("klaxon-chips").children((0..7_u8).map(days)))
        .child(
            p().attr("id", "repeat-words")
                .class("klaxon-sub")
                .text_fn(move || repeat_words(words.get())),
        )
        .child(sound_section(draft, pick_sound))
        .child(h3().text("Snooze (minutes)"))
        .child(div().class("klaxon-chips").children(snoozes))
        .child(
            div()
                .class("klaxon-actions")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "cancel")
                        .text("Cancel")
                        .on_click({
                            let cancel = Rc::clone(cancel);
                            move || cancel()
                        }),
                )
                // A new alarm has nothing to delete.
                .child((!draft.fresh).then(|| {
                    button()
                        .attr("type", "button")
                        .attr("id", "delete")
                        .class("klaxon-danger")
                        .text("Delete")
                        .on_click(move || do_delete(id))
                }))
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "save")
                        .class("klaxon-save")
                        .text("Save")
                        .on_click(move || do_save(d.alarm())),
                ),
        )
        .into_view()
}
