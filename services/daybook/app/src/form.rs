//! The task form: a dialog that is also the way to the pickers and menus it needs (each one takes the dialog's place and
//! returns to the form), so a phone never stacks one modal on another. The rules (what a changed due date does to the
//! reminder, what a repeat needs) are `daybook_core::Draft`'s; this is only the screen.

use std::rc::Rc;

use daybook_core::calendar::{at_local, clock_of, due_day, format_clock, format_due, today};
use daybook_core::nlp::parse_due;
use daybook_core::{
    Data, Draft, Due, ListRef, Mode, Preset, Recurrence, Remind, Request, Task, Unit,
};
use quench_core::date::Date;
use quench_core::time::Time;
use quench_tz::Zone;
use quench_ui::form::Field;
use quench_ui::kit::{dialog, text_field};
use quench_ui::prelude::*;

use crate::pickers::{date_picker, time_picker};

/// One line of a menu: its id, its words, what choosing it does.
type Choice = (String, String, Rc<dyn Fn()>);

/// What the form is for.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A new task, in the view being looked at.
    New(ListRef),
    Edit(Box<Task>),
}

/// What the form needs from the app around it.
#[derive(Clone)]
pub struct Context {
    pub requests: Rc<dyn Fn(Request)>,
    pub data: Signal<Data>,
    pub zone: Zone,
    pub clock: Rc<dyn Fn() -> Time>,
    /// A save is on its way.
    pub saving: Signal<bool>,
    /// Why the last save was refused.
    pub error: Signal<String>,
    /// The id of the label that was just made, for the form to tick.
    pub made_label: Signal<Option<String>>,
    /// Asks whether to delete a task (the form closes).
    pub confirm_delete: Signal<Option<String>>,
}

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Form,
    DueDate,
    DueKind(Date),
    DueTime(Date),
    RemindMenu,
    RemindDate,
    RemindTime(Date),
    RepeatMenu,
    RepeatCustom,
    ListMenu,
    NewLabel,
}

/// The dialog. It is open while `open` is true and builds its content, from `target`, each time it opens.
#[must_use]
pub fn task_dialog(ctx: &Context, target: &Signal<Option<Target>>, open: &Signal<bool>) -> View {
    let (ctx, target, close) = (ctx.clone(), target.clone(), open.clone());
    dialog(
        open,
        "Task",
        move || {
            let Some(target) = target.get_untracked() else {
                return div().into_view();
            };
            form(&ctx, &target, &close)
        },
        Vec::new,
    )
}

fn form(ctx: &Context, target: &Target, close: &Signal<bool>) -> View {
    let zone = ctx.zone.clone();
    let (draft0, id, original) = match target {
        Target::New(list) => (Draft::new(list), None, None),
        Target::Edit(task) => (
            Draft::from_task(task, &zone),
            Some(task.id.clone()),
            Some((**task).clone()),
        ),
    };
    ctx.error.set(String::new());
    ctx.saving.set(false);
    let title = Field::new("task-title", &draft0.content);
    let notes = Field::new("task-notes", &draft0.description);
    let draft = create_signal(draft0);
    let step = create_signal(Step::Form);
    let awaiting_label = create_signal(false);
    {
        let (made, draft, waiting) = (
            ctx.made_label.clone(),
            draft.clone(),
            awaiting_label.clone(),
        );
        create_effect(move || {
            if let Some(label) = made.get()
                && waiting.get_untracked()
            {
                waiting.set(false);
                draft.update(|d| {
                    if !d.label_ids.contains(&label) {
                        d.label_ids.push(label.clone());
                    }
                });
            }
        });
    }
    let (ctx, close) = (ctx.clone(), close.clone());
    let body = move || {
        let (ctx, close, draft, step, title, notes, id, original, waiting) = (
            ctx.clone(),
            close.clone(),
            draft.clone(),
            step.clone(),
            title.clone(),
            notes.clone(),
            id.clone(),
            original.clone(),
            awaiting_label.clone(),
        );
        View::dynamic(move || match step.get() {
            Step::Form => main_form(
                &ctx,
                &close,
                &draft,
                &step,
                &title,
                &notes,
                id.as_deref(),
                original.as_ref(),
            ),
            other => sub_step(&ctx, &draft, &step, &waiting, other),
        })
    };
    div().attr("id", "task-form").child(body()).into_view()
}

fn row(
    id: &str,
    label: &str,
    value: impl Fn() -> String + 'static,
    on_tap: impl Fn() + 'static,
    clear: Option<Rc<dyn Fn()>>,
) -> View {
    let mut line = div().class("dbk-field").child(
        button()
            .attr("type", "button")
            .attr("id", id)
            .class("dbk-field-main")
            .child(span().class("dbk-field-label").text(label))
            .child(span().class("dbk-field-value").text_fn(value))
            .on_click(on_tap),
    );
    if let Some(clear) = clear {
        line = line.child(
            button()
                .attr("type", "button")
                .attr("id", format!("{id}-clear"))
                .attr("aria-label", format!("Clear {}", label.to_lowercase()))
                .class("q-iconbtn")
                .child(crate::icons::close("q-icon"))
                .on_click(move || clear()),
        );
    }
    line.into_view()
}

#[allow(clippy::too_many_arguments)]
fn main_form(
    ctx: &Context,
    close: &Signal<bool>,
    draft: &Signal<Draft>,
    step: &Signal<Step>,
    title: &Field,
    notes: &Field,
    id: Option<&str>,
    original: Option<&Task>,
) -> View {
    let (zone, clock) = (ctx.zone.clone(), Rc::clone(&ctx.clock));
    let go = |to: Step| {
        let step = step.clone();
        move || step.set(to.clone())
    };
    let due_text = {
        let (draft, zone, clock) = (draft.clone(), zone.clone(), Rc::clone(&clock));
        move || match draft.get().due() {
            None => "None".to_string(),
            Some(due) => {
                let at = due.instant(&zone).unwrap_or_default();
                format_due(at, due.all_day(), clock(), &zone)
            }
        }
    };
    let due_clear: Rc<dyn Fn()> = {
        let draft = draft.clone();
        Rc::new(move || draft.update(Draft::clear_due))
    };
    let due_row = {
        let (draft, step, zone, clock) =
            (draft.clone(), step.clone(), zone.clone(), Rc::clone(&clock));
        View::dynamic({
            let (due_text, due_clear) = (due_text, Rc::clone(&due_clear));
            let due_text = Rc::new(due_text);
            move || {
                let has = draft.get().due().is_some();
                let _ = (&zone, &clock);
                let step = step.clone();
                let text = Rc::clone(&due_text);
                row(
                    "due",
                    "Due",
                    move || text(),
                    move || step.set(Step::DueDate),
                    has.then(|| Rc::clone(&due_clear)),
                )
            }
        })
    };
    let repeat_row = {
        let (draft, step) = (draft.clone(), step.clone());
        View::dynamic(move || {
            let d = draft.clone();
            let step = step.clone();
            let has_due = d.get().due().is_some();
            row(
                "repeat",
                "Repeat",
                move || {
                    d.get()
                        .recurrence()
                        .map_or_else(|| "Does not repeat".to_string(), Recurrence::describe)
                },
                move || {
                    if has_due {
                        step.set(Step::RepeatMenu);
                    }
                },
                None,
            )
        })
    };
    let remind_row = {
        let (draft, step, zone, clock) =
            (draft.clone(), step.clone(), zone.clone(), Rc::clone(&clock));
        let (d2, clear) = (draft.clone(), draft.clone());
        View::dynamic(move || {
            let has = !matches!(d2.get().remind(), Remind::None);
            let (d, zone, clock, step) =
                (draft.clone(), zone.clone(), Rc::clone(&clock), step.clone());
            let clear = clear.clone();
            row(
                "reminder",
                "Reminder",
                move || match d.get().remind() {
                    Remind::None => "None".to_string(),
                    Remind::Preset(p) => p.label().to_string(),
                    Remind::At(t) => format_due(t, false, clock(), &zone),
                },
                move || step.set(Step::RemindMenu),
                has.then(|| Rc::new(move || clear.update(Draft::clear_reminder)) as Rc<dyn Fn()>),
            )
        })
    };
    let list_row = {
        let (draft, data, step) = (draft.clone(), ctx.data.clone(), step.clone());
        row(
            "list",
            "List",
            move || {
                let current = draft.get().project_id;
                current
                    .and_then(|id| {
                        data.get()
                            .projects
                            .iter()
                            .find(|p| p.id == id)
                            .map(|p| p.name.clone())
                    })
                    .unwrap_or_else(|| "Inbox".to_string())
            },
            move || step.set(Step::ListMenu),
            None,
        )
    };
    let labels = {
        let (draft, data, step) = (draft.clone(), ctx.data.clone(), step.clone());
        View::dynamic(move || {
            let mut chips: Vec<View> = data
                .get()
                .labels
                .iter()
                .map(|l| {
                    let (id, d) = (l.id.clone(), draft.clone());
                    let on = d.clone();
                    let label_id = id.clone();
                    button()
                        .attr("type", "button")
                        .attr("id", format!("label-{id}"))
                        .class("q-chip")
                        .class_if("q-chip-on", move || on.get().label_ids.contains(&label_id))
                        .attr_fn("aria-pressed", {
                            let (d, id) = (d.clone(), id.clone());
                            move || d.get().label_ids.contains(&id).to_string()
                        })
                        .text(l.name.clone())
                        .on_click(move || d.update(|x| x.toggle_label(&id)))
                        .into_view()
                })
                .collect();
            let step = step.clone();
            chips.push(
                button()
                    .attr("type", "button")
                    .attr("id", "new-label")
                    .class("q-chip")
                    .text("New label")
                    .on_click(move || step.set(Step::NewLabel))
                    .into_view(),
            );
            div().class("dbk-chips").children(chips).into_view()
        })
    };
    let repeating_actions = original.filter(|t| t.recurrence.is_some()).map(|task| {
        let (done_id, ctx2, close2) = (task.id.clone(), ctx.clone(), close.clone());
        let (stop_ctx, stop_close, stop_zone) = (ctx.clone(), close.clone(), zone.clone());
        let before = Draft::from_task(task, &zone);
        let stop_id = task.id.clone();
        div()
            .class("dbk-actions")
            .child(
                button()
                    .attr("type", "button")
                    .attr("id", "occurrence-done")
                    .text("Mark occurrence done")
                    .on_click(move || {
                        (ctx2.requests)(Request::SetCompleted {
                            id: done_id.clone(),
                            done: true,
                        });
                        close2.set(false);
                    }),
            )
            .child(
                button()
                    .attr("type", "button")
                    .attr("id", "stop-repeating")
                    .text("Stop repeating")
                    .on_click({
                        let draft = draft.clone();
                        let _ = &stop_zone;
                        move || {
                            let mut stopped = before.clone();
                            stopped.set_recurrence(None);
                            stopped.content = draft.get_untracked().content;
                            (stop_ctx.requests)(Request::Save {
                                id: Some(stop_id.clone()),
                                draft: Box::new(stopped),
                            });
                            stop_close.set(false);
                        }
                    }),
            )
    });
    let (save_ctx, save_draft, save_title, save_notes, save_id, saving, error) = (
        ctx.clone(),
        draft.clone(),
        title.clone(),
        notes.clone(),
        id.map(str::to_string),
        ctx.saving.clone(),
        ctx.error.clone(),
    );
    let save = move || {
        let (save_zone, save_clock) = (save_ctx.zone.clone(), Rc::clone(&save_ctx.clock));
        save_draft.update(|d| {
            d.content = save_title.value_untracked();
            d.description = save_notes.value_untracked();
            // A date written in the title ("... nov 6 5pm") becomes the due date and leaves the title.
            if let Some(found) = parse_due(&d.content, save_clock(), &save_zone) {
                d.content = found.content;
                d.set_due(found.due);
            }
        });
        let d = save_draft.get_untracked();
        if !d.can_save() {
            save_ctx.error.set("Enter a title".into());
            return;
        }
        save_ctx.error.set(String::new());
        save_ctx.saving.set(true);
        (save_ctx.requests)(Request::Save {
            id: save_id.clone(),
            draft: Box::new(d),
        });
    };
    let cancel = close.clone();
    let delete = id.map(|id| {
        let (id, confirm, close) = (id.to_string(), ctx.confirm_delete.clone(), close.clone());
        button()
            .attr("type", "button")
            .attr("id", "delete-task")
            .class("dbk-danger")
            .text("Delete")
            .on_click(move || {
                confirm.set(Some(id.clone()));
                close.set(false);
            })
    });
    let busy = ctx.saving.clone();
    let _ = go;
    // What the title says about when, as it is typed.
    let hint = {
        let (title, zone, clock) = (title.clone(), zone.clone(), Rc::clone(&clock));
        p().attr("id", "due-hint")
            .attr("aria-live", "polite")
            .class("dbk-hint")
            .text_fn(move || {
                let now = clock();
                parse_due(&title.value(), now, &zone).map_or_else(String::new, |found| {
                    let at = found.due.instant(&zone).unwrap_or_default();
                    format!("Due {}", format_due(at, found.due.all_day(), now, &zone))
                })
            })
    };
    div()
        .child(text_field("Task", title, "text", None))
        .child(hint)
        .child(
            div()
                .class("q-textfield")
                .child(
                    label()
                        .attr("for", notes.name())
                        .class("q-textfield-label")
                        .text("Notes"),
                )
                .child(notes.bind(textarea()).class("q-textfield-input dbk-notes")),
        )
        .child(due_row)
        .child(repeat_row)
        .child(remind_row)
        .child(list_row)
        .child(labels)
        .child(repeating_actions)
        .child(
            p().attr("id", "form-error")
                .class("dbk-error")
                .text_fn(move || error.get()),
        )
        .child(
            div()
                .class("q-dialog-actions")
                .child(delete)
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "cancel-task")
                        .text("Cancel")
                        .on_click(move || cancel.set(false)),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "save-task")
                        .class("dbk-primary-small")
                        .attr_fn("aria-busy", move || busy.get().to_string())
                        .text_fn(move || {
                            if saving.get() {
                                "Saving…".to_string()
                            } else {
                                "Save".to_string()
                            }
                        })
                        .on_click(save),
                ),
        )
        .into_view()
}

fn menu(heading: &str, options: Vec<Choice>, back: Rc<dyn Fn()>) -> View {
    let items: Vec<View> = options
        .into_iter()
        .map(|(id, text, pick)| {
            button()
                .attr("type", "button")
                .attr("id", id)
                .class("dbk-option")
                .text(text)
                .on_click(move || pick())
                .into_view()
        })
        .collect();
    div()
        .child(h3().text(heading.to_string()))
        .child(div().class("dbk-options").children(items))
        .child(
            div().class("q-dialog-actions").child(
                button()
                    .attr("type", "button")
                    .attr("id", "menu-back")
                    .text("Back")
                    .on_click(move || back()),
            ),
        )
        .into_view()
}

fn sub_step(
    ctx: &Context,
    draft: &Signal<Draft>,
    step: &Signal<Step>,
    waiting: &Signal<bool>,
    which: Step,
) -> View {
    let (zone, clock) = (ctx.zone.clone(), Rc::clone(&ctx.clock));
    let now = clock();
    let today_date = today(now, &zone);
    let to_form: Rc<dyn Fn()> = {
        let step = step.clone();
        Rc::new(move || step.set(Step::Form))
    };
    let go = |to: Step| -> Rc<dyn Fn()> {
        let step = step.clone();
        Rc::new(move || step.set(to.clone()))
    };
    match which {
        Step::Form => div().into_view(),
        Step::DueDate => {
            let initial = draft.get_untracked().due().map_or(today_date, |d| d.date);
            let (step2, back) = (step.clone(), Rc::clone(&to_form));
            date_picker(
                initial,
                today_date,
                Rc::new(move |date| step2.set(Step::DueKind(date))),
                back,
            )
        }
        Step::DueKind(date) => {
            let (d1, s1) = (draft.clone(), step.clone());
            let all_day: Rc<dyn Fn()> = Rc::new(move || {
                d1.update(|d| d.set_due(Due { date, clock: None }));
                s1.set(Step::Form);
            });
            menu(
                "Due time",
                vec![
                    ("all-day".into(), "All day".into(), all_day),
                    (
                        "choose-time".into(),
                        "Choose a time…".into(),
                        go(Step::DueTime(date)),
                    ),
                ],
                go(Step::DueDate),
            )
        }
        Step::DueTime(date) => {
            let initial = draft
                .get_untracked()
                .due()
                .and_then(|d| d.clock)
                .unwrap_or((9, 0));
            let (d1, s1) = (draft.clone(), step.clone());
            time_picker(
                initial,
                Rc::new(move |(h, m)| {
                    d1.update(|d| {
                        d.set_due(Due {
                            date,
                            clock: Some((h, m)),
                        })
                    });
                    s1.set(Step::Form);
                }),
                go(Step::DueDate),
            )
        }
        Step::RemindMenu => {
            let kind = draft.get_untracked().due().map(|d| d.all_day());
            let mut options: Vec<Choice> = Vec::new();
            let none: Rc<dyn Fn()> = {
                let (d, s) = (draft.clone(), step.clone());
                Rc::new(move || {
                    d.update(Draft::clear_reminder);
                    s.set(Step::Form);
                })
            };
            options.push(("remind-none".into(), "No reminder".into(), none));
            if let Some(all_day) = kind {
                for (i, preset) in Preset::for_kind(all_day).iter().copied().enumerate() {
                    let (d, s) = (draft.clone(), step.clone());
                    options.push((
                        format!("remind-{i}"),
                        preset.label().to_string(),
                        Rc::new(move || {
                            d.update(|x| {
                                x.set_preset(preset);
                            });
                            s.set(Step::Form);
                        }),
                    ));
                }
            }
            options.push((
                "remind-custom".into(),
                "Pick date and time…".into(),
                go(Step::RemindDate),
            ));
            menu("Remind me", options, to_form)
        }
        Step::RemindDate => {
            let initial = match draft.get_untracked().remind() {
                Remind::At(t) => due_day(t, false, &zone),
                _ => draft.get_untracked().due().map_or(today_date, |d| d.date),
            };
            let step2 = step.clone();
            date_picker(
                initial,
                today_date,
                Rc::new(move |date| step2.set(Step::RemindTime(date))),
                go(Step::RemindMenu),
            )
        }
        Step::RemindTime(date) => {
            let initial = match draft.get_untracked().remind() {
                Remind::At(t) => clock_of(t, &zone),
                _ => (9, 0),
            };
            let (d1, s1, z) = (draft.clone(), step.clone(), zone);
            time_picker(
                initial,
                Rc::new(move |(h, m)| {
                    if let Some(at) = at_local(date, h, m, &z) {
                        d1.update(|d| d.set_reminder_at(at));
                    }
                    s1.set(Step::Form);
                }),
                go(Step::RemindDate),
            )
        }
        Step::RepeatMenu => {
            let mut options: Vec<Choice> = Vec::new();
            let none: Rc<dyn Fn()> = {
                let (d, s) = (draft.clone(), step.clone());
                Rc::new(move || {
                    d.update(|x| {
                        x.set_recurrence(None);
                    });
                    s.set(Step::Form);
                })
            };
            options.push(("repeat-none".into(), "Does not repeat".into(), none));
            for unit in Unit::ALL {
                let (d, s, name) = (draft.clone(), step.clone(), zone.name().to_string());
                options.push((
                    format!("repeat-{}", unit.wire()),
                    format!("Every {}", unit.wire()),
                    Rc::new(move || {
                        d.update(|x| {
                            x.set_recurrence(Some(Recurrence::every(unit, &name)));
                        });
                        s.set(Step::Form);
                    }),
                ));
            }
            options.push((
                "repeat-custom".into(),
                "Custom…".into(),
                go(Step::RepeatCustom),
            ));
            menu("Repeat", options, to_form)
        }
        Step::RepeatCustom => repeat_custom(draft, step, zone.name()),
        Step::ListMenu => {
            let mut options: Vec<Choice> = Vec::new();
            let mut add = |id: String, name: String, project: Option<String>| {
                let (d, s) = (draft.clone(), step.clone());
                options.push((
                    id,
                    name,
                    Rc::new(move || {
                        d.update(|x| x.project_id.clone_from(&project));
                        s.set(Step::Form);
                    }),
                ));
            };
            add("list-inbox".into(), "Inbox".into(), None);
            for p in &ctx.data.get_untracked().projects {
                add(format!("list-{}", p.id), p.name.clone(), Some(p.id.clone()));
            }
            menu("List", options, to_form)
        }
        Step::NewLabel => new_label(ctx, step, waiting),
    }
}

fn repeat_custom(draft: &Signal<Draft>, step: &Signal<Step>, zone_name: &str) -> View {
    let start = draft
        .get_untracked()
        .recurrence()
        .cloned()
        .unwrap_or_else(|| Recurrence::every(Unit::Day, zone_name));
    let rule = create_signal(start);
    let (shown, minus, plus) = (rule.clone(), rule.clone(), rule.clone());
    let stepper = div()
        .class("dbk-interval")
        .child(span().class("dbk-field-label").text("Every"))
        .child(
            button()
                .attr("type", "button")
                .attr("id", "interval-down")
                .class("dbk-step")
                .text("−")
                .on_click(move || {
                    minus.update(|r| r.interval = r.interval.saturating_sub(1).max(1))
                }),
        )
        .child(
            span()
                .attr("id", "interval")
                .class("dbk-clock-digits")
                .text_fn(move || shown.get().interval.to_string()),
        )
        .child(
            button()
                .attr("type", "button")
                .attr("id", "interval-up")
                .class("dbk-step")
                .text("+")
                .on_click(move || plus.update(|r| r.interval = (r.interval + 1).min(999))),
        );
    let units: Vec<View> = Unit::ALL
        .into_iter()
        .map(|u| {
            let (on, set) = (rule.clone(), rule.clone());
            button()
                .attr("type", "button")
                .attr("id", format!("unit-{}", u.wire()))
                .class("q-chip")
                .class_if("q-chip-on", move || on.get().unit == u)
                .text(u.plural())
                .on_click(move || {
                    set.update(|r| {
                        r.unit = u;
                        if u != Unit::Week {
                            r.weekdays.clear();
                        }
                    });
                })
                .into_view()
        })
        .collect();
    let days = {
        let rule = rule.clone();
        View::dynamic(move || {
            let r = rule.get();
            if r.unit != Unit::Week || r.mode == Mode::AfterCompletion {
                return div().into_view();
            }
            let chips: Vec<View> = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let number = u8::try_from(i + 1).unwrap_or(1);
                    let (on, set) = (rule.clone(), rule.clone());
                    button()
                        .attr("type", "button")
                        .attr("id", format!("weekday-{number}"))
                        .class("q-chip")
                        .class_if("q-chip-on", move || on.get().weekdays.contains(&number))
                        .text(*name)
                        .on_click(move || {
                            set.update(|r| {
                                if let Some(at) = r.weekdays.iter().position(|d| *d == number) {
                                    r.weekdays.remove(at);
                                } else {
                                    r.weekdays.push(number);
                                    r.weekdays.sort_unstable();
                                }
                            });
                        })
                        .into_view()
                })
                .collect();
            div().class("dbk-chips").children(chips).into_view()
        })
    };
    let after = {
        let (on, set) = (rule.clone(), rule.clone());
        button()
            .attr("type", "button")
            .attr("id", "after-completion")
            .class("q-chip")
            .class_if("q-chip-on", move || on.get().mode == Mode::AfterCompletion)
            .text("Count from when it is done")
            .on_click(move || {
                set.update(|r| {
                    r.mode = if r.mode == Mode::Fixed {
                        Mode::AfterCompletion
                    } else {
                        Mode::Fixed
                    };
                    if r.mode == Mode::AfterCompletion {
                        r.weekdays.clear();
                    }
                });
            })
    };
    let (d1, s1, ok_rule, back) = (draft.clone(), step.clone(), rule, step.clone());
    div()
        .attr("id", "repeat-custom")
        .child(h3().text("Repeat"))
        .child(stepper)
        .child(div().class("dbk-chips").children(units))
        .child(days)
        .child(div().class("dbk-chips").child(after))
        .child(
            div()
                .class("q-dialog-actions")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "repeat-cancel")
                        .text("Back")
                        .on_click(move || back.set(Step::RepeatMenu)),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "repeat-ok")
                        .class("dbk-primary-small")
                        .text("OK")
                        .on_click(move || {
                            let r = ok_rule.get_untracked();
                            d1.update(|x| {
                                x.set_recurrence(Some(r));
                            });
                            s1.set(Step::Form);
                        }),
                ),
        )
        .into_view()
}

fn new_label(ctx: &Context, step: &Signal<Step>, waiting: &Signal<bool>) -> View {
    let name = Field::new("label-name", "");
    let color = create_signal(daybook_core::model::LABEL_COLORS[0].to_string());
    let swatches: Vec<View> = daybook_core::model::LABEL_COLORS
        .iter()
        .map(|hex| {
            let (on, set) = (color.clone(), color.clone());
            let hex = (*hex).to_string();
            let (selected, value) = (hex.clone(), hex.clone());
            button()
                .attr("type", "button")
                .attr("id", format!("color-{}", &hex[1..]))
                .attr("aria-label", hex.clone())
                .class("dbk-swatch")
                .class_if("dbk-swatch-on", move || on.get() == selected)
                .css("background", move || hex.clone())
                .on_click(move || set.set(value.clone()))
                .into_view()
        })
        .collect();
    let (requests, waiting, back, go) = (
        Rc::clone(&ctx.requests),
        waiting.clone(),
        step.clone(),
        step.clone(),
    );
    let field = name.clone();
    div()
        .attr("id", "new-label-form")
        .child(h3().text("New label"))
        .child(text_field("Name", &name, "text", None))
        .child(div().class("dbk-swatches").children(swatches))
        .child(
            div()
                .class("q-dialog-actions")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "label-cancel")
                        .text("Back")
                        .on_click(move || back.set(Step::Form)),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "label-ok")
                        .class("dbk-primary-small")
                        .text("Add")
                        .on_click(move || {
                            let text = field.value_untracked();
                            if text.trim().is_empty() {
                                field.set_error("Enter a name");
                                return;
                            }
                            waiting.set(true);
                            requests(Request::CreateLabel {
                                name: text,
                                color: color.get_untracked(),
                            });
                            go.set(Step::Form);
                        }),
                ),
        )
        .into_view()
}

/// "09:05", for the pickers' readouts.
#[must_use]
pub fn clock_text(h: u32, m: u32) -> String {
    format_clock(h, m)
}
