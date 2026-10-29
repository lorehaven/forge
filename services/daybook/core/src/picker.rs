//! The calendar the date picker draws.

use quench_core::date::Date;

use crate::calendar::iso_weekday;

pub const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// "October 2026".
#[must_use]
pub fn month_title(year: i32, month: u32) -> String {
    format!(
        "{} {year}",
        MONTH_NAMES[(month as usize).saturating_sub(1) % 12]
    )
}

/// The month before (`-1`) or after (`1`).
#[must_use]
pub const fn shift_month(year: i32, month: u32, by: i32) -> (i32, u32) {
    let index = year * 12 + month as i32 - 1 + by;
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

/// The weeks of a month, Monday first; `None` is a day of the neighbouring month.
#[must_use]
pub fn month_grid(year: i32, month: u32) -> Vec<[Option<Date>; 7]> {
    let Some(first) = Date::from_ymd(year, month, 1) else {
        return Vec::new();
    };
    let lead = usize::from(iso_weekday(first) - 1);
    let days = Date::days_in_month(year, month) as usize;
    let mut weeks = Vec::new();
    let mut cell = 0;
    while cell < lead + days {
        let mut week = [None; 7];
        for slot in &mut week {
            if cell >= lead && cell < lead + days {
                *slot = Date::from_ymd(year, month, u32::try_from(cell - lead + 1).unwrap_or(1));
            }
            cell += 1;
        }
        weeks.push(week);
    }
    weeks
}
