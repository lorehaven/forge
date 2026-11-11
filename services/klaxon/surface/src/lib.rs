//! Klaxon on Android: the alarms kept in a file, the clock, and the two small commands the Kotlin side runs with no
//! window (`--next`, `--fired`) to learn when to set the system's alarm. The window is `main.rs`.

use std::path::PathBuf;

use klaxon_core::{Alarms, Next, Now, parse, render};

/// Where the alarms are kept: `$HOME/.local/share/klaxon/alarms` (the Kotlin host starts the program with `HOME` as
/// the app's files directory, so the window and the background commands share them).
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The app's own directory.
    #[must_use]
    pub fn standard() -> Self {
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        Self::at(home.join(".local/share/klaxon"))
    }

    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("alarms")
    }

    /// The alarms; none when nothing was saved yet.
    #[must_use]
    pub fn load(&self) -> Alarms {
        std::fs::read_to_string(self.file()).map_or_else(|_| Alarms::default(), |t| parse(&t))
    }

    /// Writes the alarms through a temporary file, so a reader never sees half a list.
    pub fn save(&self, alarms: &Alarms) {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return;
        }
        let temporary = self.dir.join("alarms.tmp");
        if std::fs::write(&temporary, render(alarms)).is_ok() {
            let _ = std::fs::rename(&temporary, self.file());
        }
    }

    /// When the list was last written, to tell whether another process changed it.
    #[must_use]
    pub fn stamp(&self) -> Option<std::time::SystemTime> {
        std::fs::metadata(self.file())
            .and_then(|m| m.modified())
            .ok()
    }
}

/// The local time from `date +%u:%H:%M` (the zone is the phone's; Android's `date` is toybox's).
#[must_use]
pub fn local_now() -> Now {
    let out = std::process::Command::new("date")
        .arg("+%u:%H:%M")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    read_now(out.trim()).unwrap_or(Now {
        weekday: 0,
        minute: 0,
    })
}

/// `3:07:05` (Wednesday, 07:05) as a [`Now`].
#[must_use]
pub fn read_now(text: &str) -> Option<Now> {
    let mut f = text.split(':');
    let day: u8 = f.next()?.parse().ok()?;
    let hour: u16 = f.next()?.parse().ok()?;
    let minute: u16 = f.next()?.parse().ok()?;
    if !(1..=7).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    Some(Now {
        weekday: day - 1,
        minute: hour * 60 + minute,
    })
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// The alarm that rings next as one line of JSON, for the Kotlin side: `{"none":true}` when none is on.
#[must_use]
pub fn next_json(next: Option<&Next>) -> String {
    next.map_or_else(
        || "{\"none\":true}".to_string(),
        |n| {
            format!(
                "{{\"id\":{},\"days_ahead\":{},\"hour\":{},\"minute\":{},\"snooze\":{},\"vibrate\":{},\"loud\":{},\"sound\":\"{}\",\"label\":\"{}\"}}",
                n.id,
                n.days_ahead,
                n.hour,
                n.minute,
                n.snooze,
                n.vibrate,
                n.loud,
                escape(&n.sound),
                escape(&n.label)
            )
        },
    )
}

/// The background commands, answered as one line of JSON; `None` when the arguments are not a command.
///
/// `--next` says which alarm rings next. `--fired <id>` first switches that alarm off if it is a one-time one (it has
/// just rung), then says the same.
#[must_use]
pub fn command(args: &[String], store: &Store, now: Now) -> Option<String> {
    match args.first().map(String::as_str)? {
        "--next" => {}
        "--fired" => {
            let id = args.get(1)?.parse().ok()?;
            let mut alarms = store.load();
            alarms.fired(id);
            store.save(&alarms);
        }
        _ => return None,
    }
    Some(next_json(store.load().next(now).as_ref()))
}
