//! The list of alarms as a text file: one line per alarm,
//! `id|hour|minute|days|enabled|snooze|vibrate|loud|sound|sound name|label`. The sound and its name have `%` and `|`
//! written as `%25` and `%7C`; the label is last, so it may hold anything but a line break. A line that does not read is
//! left out, the others stay.

use std::fmt::Write as _;

use crate::alarm::{Alarm, Next, Now, SNOOZE_CHOICES, next};

/// The alarms, in the order the person made them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Alarms {
    pub list: Vec<Alarm>,
}

impl Alarms {
    /// The id for a new alarm.
    #[must_use]
    pub fn fresh_id(&self) -> u32 {
        self.list.iter().map(|a| a.id).max().map_or(1, |m| m + 1)
    }

    /// Adds `alarm` or, when one with its id is there, replaces it.
    pub fn put(&mut self, alarm: Alarm) {
        match self.list.iter_mut().find(|a| a.id == alarm.id) {
            Some(slot) => *slot = alarm,
            None => self.list.push(alarm),
        }
    }

    pub fn remove(&mut self, id: u32) {
        self.list.retain(|a| a.id != id);
    }

    /// Switches an alarm on or off.
    pub fn set_enabled(&mut self, id: u32, on: bool) {
        if let Some(a) = self.list.iter_mut().find(|a| a.id == id) {
            a.enabled = on;
        }
    }

    /// An alarm has rung: a one-time alarm switches itself off.
    pub fn fired(&mut self, id: u32) {
        if let Some(a) = self.list.iter_mut().find(|a| a.id == id && a.days == 0) {
            a.enabled = false;
        }
    }

    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Alarm> {
        self.list.iter().find(|a| a.id == id)
    }

    /// The alarm that rings first.
    #[must_use]
    pub fn next(&self, now: Now) -> Option<Next> {
        next(&self.list, now)
    }
}

/// Writes the list down.
#[must_use]
pub fn render(alarms: &Alarms) -> String {
    let mut out = String::new();
    for a in &alarms.list {
        let label = a.label.replace(['\n', '\r'], " ");
        let _ = writeln!(
            out,
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            a.id,
            a.hour,
            a.minute,
            a.days,
            u8::from(a.enabled),
            a.snooze,
            u8::from(a.vibrate),
            u8::from(a.loud),
            escape(&a.sound),
            escape(&a.sound_name),
            label
        );
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('%', "%25")
        .replace('|', "%7C")
        .replace(['\n', '\r'], " ")
}

fn unescape(text: &str) -> String {
    text.replace("%7C", "|").replace("%25", "%")
}

fn line(text: &str) -> Option<Alarm> {
    let mut f = text.splitn(11, '|');
    let id = f.next()?.parse().ok()?;
    let hour: u8 = f.next()?.parse().ok()?;
    let minute: u8 = f.next()?.parse().ok()?;
    let days: u8 = f.next()?.parse().ok()?;
    let enabled = f.next()? == "1";
    let snooze: u8 = f.next()?.parse().ok()?;
    let vibrate = f.next()? == "1";
    let loud = f.next()? == "1";
    let sound = unescape(f.next()?);
    let sound_name = unescape(f.next()?);
    let label = f.next().unwrap_or_default().to_string();
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(Alarm {
        id,
        hour,
        minute,
        label,
        days: days & 0b111_1111,
        enabled,
        snooze: if SNOOZE_CHOICES.contains(&snooze) {
            snooze
        } else {
            10
        },
        vibrate,
        loud,
        sound,
        sound_name,
    })
}

/// Reads the list.
#[must_use]
pub fn parse(text: &str) -> Alarms {
    Alarms {
        list: text.lines().filter_map(line).collect(),
    }
}
