//! The alarm clock's logic, with no I/O: what an alarm is, when it rings next, and how the list is written down.
//!
//! Times here are the wall clock's: an alarm at 07:00 rings at 07:00 whatever the zone's offset is on that day. The
//! host turns "N days ahead at 07:00" into an instant with the zone's own rules (a change of clocks included), which
//! is why [`Next`] carries days and not a number of minutes.

mod alarm;
mod store;

pub use alarm::{
    Alarm, DAY_NAMES, Next, Now, SNOOZE_CHOICES, WEEKDAYS, WEEKEND, clock, countdown, next,
    repeat_words,
};
pub use store::{Alarms, parse, render};
