//! An alarm, the clock's reading, and the arithmetic between them.

/// Days of the week, Monday first (the order of the bits in [`Alarm::days`]).
pub const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Monday to Friday as bits.
pub const WEEKDAYS: u8 = 0b001_1111;
/// Saturday and Sunday as bits.
pub const WEEKEND: u8 = 0b110_0000;
const EVERY_DAY: u8 = 0b111_1111;

/// The snooze lengths the editor offers, in minutes.
pub const SNOOZE_CHOICES: [u8; 5] = [5, 10, 15, 20, 30];

/// One alarm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alarm {
    pub id: u32,
    pub hour: u8,
    pub minute: u8,
    pub label: String,
    /// The days it rings on, bit 0 Monday to bit 6 Sunday. None set: it rings once, at the next such time, and then
    /// switches itself off.
    pub days: u8,
    pub enabled: bool,
    /// Minutes of snooze, one of [`SNOOZE_CHOICES`].
    pub snooze: u8,
    /// Whether the phone vibrates while it rings.
    pub vibrate: bool,
    /// Whether it rings at the full alarm volume, whatever the volume is set to.
    pub loud: bool,
    /// The sound: a content URI the system's sound picker gave, or empty for the phone's default alarm sound.
    pub sound: String,
    /// What the sound is called, for the editor.
    pub sound_name: String,
}

impl Alarm {
    /// A new alarm at 07:00, once, on.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self {
            id,
            hour: 7,
            minute: 0,
            label: String::new(),
            days: 0,
            enabled: true,
            snooze: 10,
            vibrate: true,
            loud: false,
            sound: String::new(),
            sound_name: String::new(),
        }
    }

    /// Minutes after midnight.
    #[must_use]
    pub fn minute_of_day(&self) -> u16 {
        u16::from(self.hour.min(23)) * 60 + u16::from(self.minute.min(59))
    }

    /// Whether it rings on `weekday` (0 is Monday): a one-time alarm rings on any day.
    #[must_use]
    pub const fn rings_on(&self, weekday: u8) -> bool {
        self.days == 0 || self.days & (1 << (weekday % 7)) != 0
    }

    /// How many whole days from `now` it next rings, if it is on: 0 is later today. An alarm at the minute that is
    /// now does not ring again for this minute.
    #[must_use]
    pub fn days_ahead(&self, now: Now) -> Option<u8> {
        if !self.enabled {
            return None;
        }
        (0..=7_u8).find(|d| {
            self.rings_on((now.weekday + d) % 7) && (*d > 0 || self.minute_of_day() > now.minute)
        })
    }

    /// Minutes from `now` until it rings.
    #[must_use]
    pub fn minutes_until(&self, now: Now) -> Option<u32> {
        let days = u32::from(self.days_ahead(now)?);
        Some(days * 1440 + u32::from(self.minute_of_day()) - u32::from(now.minute))
    }
}

/// The local time the host reads off the wall: the day of the week (0 is Monday) and the minutes after midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    pub weekday: u8,
    pub minute: u16,
}

/// The alarm that rings next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Next {
    pub id: u32,
    pub days_ahead: u8,
    pub hour: u8,
    pub minute: u8,
    pub label: String,
    pub snooze: u8,
    pub vibrate: bool,
    pub loud: bool,
    pub sound: String,
}

/// The alarm that rings first from `now`, if any is on. Of two at the same time the lower id wins.
#[must_use]
pub fn next(alarms: &[Alarm], now: Now) -> Option<Next> {
    alarms
        .iter()
        .filter_map(|a| a.minutes_until(now).map(|m| (m, a)))
        .min_by_key(|(m, a)| (*m, a.id))
        .and_then(|(_, a)| {
            Some(Next {
                id: a.id,
                days_ahead: a.days_ahead(now)?,
                hour: a.hour,
                minute: a.minute,
                label: a.label.clone(),
                snooze: a.snooze,
                vibrate: a.vibrate,
                loud: a.loud,
                sound: a.sound.clone(),
            })
        })
}

/// `07:05`.
#[must_use]
pub fn clock(hour: u8, minute: u8) -> String {
    format!("{hour:02}:{minute:02}")
}

/// In words: `Once`, `Every day`, `Weekdays`, `Weekends`, or the days (`Mon, Wed, Fri`).
#[must_use]
pub fn repeat_words(days: u8) -> String {
    match days & EVERY_DAY {
        0 => "Once".into(),
        EVERY_DAY => "Every day".into(),
        WEEKDAYS => "Weekdays".into(),
        WEEKEND => "Weekends".into(),
        d => (0..7_u8)
            .filter(|i| d & (1_u8 << i) != 0)
            .map(|i| DAY_NAMES[usize::from(i)])
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// How long until an alarm, as a person says it: `Rings in 7 h 32 min`, `Rings in 1 day 2 h`, `Rings in less than a
/// minute`.
#[must_use]
pub fn countdown(minutes: u32) -> String {
    let (d, h, m) = (minutes / 1440, minutes % 1440 / 60, minutes % 60);
    let mut parts = Vec::new();
    if d > 0 {
        parts.push(format!("{d} {}", if d == 1 { "day" } else { "days" }));
    }
    if h > 0 {
        parts.push(format!("{h} h"));
    }
    // Days and hours are enough once a day is in it.
    if m > 0 && d == 0 {
        parts.push(format!("{m} min"));
    }
    if parts.is_empty() {
        "Rings in less than a minute".into()
    } else {
        format!("Rings in {}", parts.join(" "))
    }
}
