//! A calendar and a time picker of the app's own: the native renderer draws `<input type="date">` but cannot edit it by
//! touch, and a phone's pickers differ from a desktop's. Both are plain views, shown by the task form in place of itself.

use std::rc::Rc;

use daybook_core::calendar::format_clock;
use daybook_core::picker::{month_grid, month_title, shift_month};
use quench_core::date::Date;
use quench_ui::prelude::*;

const WEEK: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// A month at a time, with the day chosen marked and today ringed.
#[must_use]
pub fn date_picker(
    initial: Date,
    today: Date,
    on_pick: Rc<dyn Fn(Date)>,
    on_cancel: Rc<dyn Fn()>,
) -> View {
    let (year, month, _) = initial.ymd();
    let shown = create_signal((year, month));
    let chosen = create_signal(initial);
    let (title, back, forward, grid) = (shown.clone(), shown.clone(), shown.clone(), shown);
    let (picked, ok) = (chosen.clone(), chosen.clone());
    let header = div()
        .class("dbk-month")
        .child(nav_button("Previous month", "dbk-prev", -1, &back))
        .child(
            span()
                .class("dbk-month-title")
                .attr("id", "month-title")
                .text_fn(move || {
                    let (y, m) = title.get();
                    month_title(y, m)
                }),
        )
        .child(nav_button("Next month", "dbk-next", 1, &forward));
    let week = div().class("dbk-week").children(
        WEEK.iter()
            .map(|d| span().class("dbk-weekday").text(*d).into_view())
            .collect::<Vec<_>>(),
    );
    let days = View::dynamic(move || {
        let (y, m) = grid.get();
        let rows: Vec<View> = month_grid(y, m)
            .into_iter()
            .map(|week| {
                let cells: Vec<View> = week
                    .into_iter()
                    .map(|cell| match cell {
                        None => span().class("dbk-day dbk-day-empty").into_view(),
                        Some(date) => day_button(date, today, &picked),
                    })
                    .collect();
                div().class("dbk-days").children(cells).into_view()
            })
            .collect();
        div().class("dbk-grid").children(rows).into_view()
    });
    div()
        .attr("id", "date-picker")
        .child(header)
        .child(week)
        .child(days)
        .child(
            div()
                .class("q-dialog-actions")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "date-cancel")
                        .text("Cancel")
                        .on_click(move || on_cancel()),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "date-ok")
                        .class("dbk-primary-small")
                        .text("OK")
                        .on_click(move || on_pick(ok.get_untracked())),
                ),
        )
        .into_view()
}

fn nav_button(label: &str, id: &str, by: i32, shown: &Signal<(i32, u32)>) -> Element {
    let shown = shown.clone();
    button()
        .attr("type", "button")
        .attr("id", id)
        .attr("aria-label", label)
        .class("q-iconbtn")
        .child(if by < 0 {
            crate::icons::chevron_left("q-icon")
        } else {
            crate::icons::chevron_right("q-icon")
        })
        .on_click(move || {
            let (y, m) = shown.get_untracked();
            shown.set(shift_month(y, m, by));
        })
}

fn day_button(date: Date, today: Date, chosen: &Signal<Date>) -> View {
    let (on, set) = (chosen.clone(), chosen.clone());
    let day = date.day();
    button()
        .attr("type", "button")
        .attr(
            "id",
            format!("day-{}-{:02}-{day:02}", date.year(), date.month()),
        )
        .class("dbk-day")
        .class_if("dbk-day-today", move || date == today)
        .class_if("dbk-day-on", move || on.get() == date)
        .text(day.to_string())
        .on_click(move || set.set(date))
        .into_view()
}

/// Hours and minutes with a button to move each, and a few usual times.
#[must_use]
pub fn time_picker(
    initial: (u32, u32),
    on_pick: Rc<dyn Fn((u32, u32))>,
    on_cancel: Rc<dyn Fn()>,
) -> View {
    let clock = create_signal(initial);
    let step =
        |id: &str, label: &str, glyph: &str, hours: bool, by: i32, clock: &Signal<(u32, u32)>| {
            let clock = clock.clone();
            button()
                .attr("type", "button")
                .attr("id", id)
                .attr("aria-label", label)
                .class("dbk-step")
                .text(glyph)
                .on_click(move || {
                    let (h, m) = clock.get_untracked();
                    let next = if hours {
                        ((i64::from(h) + i64::from(by)).rem_euclid(24) as u32, m)
                    } else {
                        (h, (i64::from(m) + i64::from(by)).rem_euclid(60) as u32)
                    };
                    clock.set(next);
                })
        };
    let (hours, minutes) = (clock.clone(), clock.clone());
    let readout = div()
        .class("dbk-clock")
        .child(
            div()
                .class("dbk-clock-part")
                .child(step("hour-up", "Hour later", "+", true, 1, &clock))
                .child(
                    span()
                        .attr("id", "hour")
                        .class("dbk-clock-digits")
                        .text_fn(move || format!("{:02}", hours.get().0)),
                )
                .child(step("hour-down", "Hour earlier", "−", true, -1, &clock)),
        )
        .child(span().class("dbk-clock-colon").text(":"))
        .child(
            div()
                .class("dbk-clock-part")
                .child(step("minute-up", "Minute later", "+", false, 5, &clock))
                .child(
                    span()
                        .attr("id", "minute")
                        .class("dbk-clock-digits")
                        .text_fn(move || format!("{:02}", minutes.get().1)),
                )
                .child(step(
                    "minute-down",
                    "Minute earlier",
                    "−",
                    false,
                    -5,
                    &clock,
                )),
        );
    let usual: Vec<View> = [(9, 0), (12, 0), (15, 0), (18, 0), (21, 0)]
        .into_iter()
        .map(|(h, m)| {
            let clock = clock.clone();
            button()
                .attr("type", "button")
                .attr("id", format!("usual-{h:02}{m:02}"))
                .class("dbk-usual")
                .text(format_clock(h, m))
                .on_click(move || clock.set((h, m)))
                .into_view()
        })
        .collect();
    let ok = clock;
    div()
        .attr("id", "time-picker")
        .child(readout)
        .child(div().class("dbk-usuals").children(usual))
        .child(
            div()
                .class("q-dialog-actions")
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "time-cancel")
                        .text("Cancel")
                        .on_click(move || on_cancel()),
                )
                .child(
                    button()
                        .attr("type", "button")
                        .attr("id", "time-ok")
                        .class("dbk-primary-small")
                        .text("OK")
                        .on_click(move || on_pick(ok.get_untracked())),
                ),
        )
        .into_view()
}
