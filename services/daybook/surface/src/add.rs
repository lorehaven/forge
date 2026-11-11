//! `--add`: makes a task with no window (the widget's quick-add dialog).
//!
//! `--add --title <text> [--list <id>] [--due <YYYY-MM-DD[THH:MM]>] [--remind <preset>]`. A date written in the title
//! ("call mum tomorrow 5pm") is the due date when none is given, as in the app. The program prints a word for what
//! happened and, when the task has a reminder ahead, one more line: the arguments of `notifications.schedule` for it,
//! which the Kotlin side arms (this program has no bridge to the plugin).

use daybook_core::nlp::parse_due;
use daybook_core::reminders::pending_reminders;
use daybook_core::widget::parse_list_id;
use daybook_core::{Draft, Due, ListRef, Preset};
use daybook_host::is_unreachable;
use daybook_host::remote::{Op, Remote, Reply};
use daybook_net::{NetError, Store};
use quench_core::date::Date;
use quench_core::time::Time;
use quench_serde::json;
use quench_tz::Zone;

use crate::complete::{cached, queue, write_snapshot};

/// What to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub title: String,
    pub list: ListRef,
    pub due: Option<Due>,
    pub remind: Option<Preset>,
}

/// What a run did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The service has the task; the reminder, if any, is the line to hand the alarm clock.
    Added(Option<String>),
    /// The services could not be reached: the task is in the outbox and goes out when the app next starts.
    Queued,
    SignedOut,
    /// Nothing to make (no title), or the service refused.
    Failed(String),
}

impl Outcome {
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Added(_) => "added",
            Self::Queued => "queued",
            Self::SignedOut => "signed_out",
            Self::Failed(_) => "failed",
        }
    }
}

/// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`.
#[must_use]
pub fn parse_due_arg(text: &str) -> Option<Due> {
    let (day, clock) = match text.split_once('T') {
        Some((d, c)) => (d, Some(c)),
        None => (text, None),
    };
    let mut parts = day.split('-').map(str::parse::<u32>);
    let year = parts.next()?.ok()?;
    let date = Date::from_ymd(i32::try_from(year).ok()?, parts.next()?.ok()?, parts.next()?.ok()?)?;
    let clock = match clock {
        None => None,
        Some(c) => {
            let (h, m) = c.split_once(':')?;
            let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
            if h > 23 || m > 59 {
                return None;
            }
            Some((h, m))
        }
    };
    Some(Due { date, clock })
}

/// The task the arguments after `--add` describe.
#[must_use]
pub fn parse_args(args: &[String]) -> NewTask {
    let mut task = NewTask {
        title: String::new(),
        list: ListRef::Inbox,
        due: None,
        remind: None,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it.next().map(String::as_str).unwrap_or_default();
        match flag.as_str() {
            "--title" => task.title = value.to_string(),
            "--list" => task.list = parse_list_id(value).unwrap_or(ListRef::Inbox),
            "--due" => task.due = parse_due_arg(value),
            "--remind" => task.remind = Preset::from_key(value),
            _ => {}
        }
    }
    task
}

/// Makes the task with the saved login.
#[must_use]
pub fn add(store: &Store, task: &NewTask) -> Outcome {
    let remote = Remote::new(store.clone());
    if !remote.signed_in() {
        store.clear_widget_snapshot();
        return Outcome::SignedOut;
    }
    add_with(store, task, Time::now(), &Zone::local(), |op| {
        remote.execute(op)
    })
}

/// [`add`] over `run`, the way to the services (a stand-in in the tests), as of `now` in `zone`.
pub fn add_with(
    store: &Store,
    task: &NewTask,
    now: Time,
    zone: &Zone,
    mut run: impl FnMut(Op) -> Result<Reply, NetError>,
) -> Outcome {
    let mut draft = Draft::new(&task.list);
    draft.content = task.title.trim().to_string();
    if let Some(found) = parse_due(&draft.content, now, zone) {
        draft.content = found.content;
        draft.set_due(found.due);
    }
    if let Some(due) = task.due {
        draft.set_due(due);
    }
    if let Some(preset) = task.remind {
        // A reminder follows the due date; the dialog offers it only with one.
        draft.set_preset(preset);
    }
    if !draft.can_save() {
        return Outcome::Failed("a task needs a title".into());
    }
    let op = Op::CreateTask(draft.body(zone, None, 0));
    let signed_out = || {
        store.clear_token();
        store.clear_widget_snapshot();
        Outcome::SignedOut
    };
    let mut data = match run(Op::Load) {
        Ok(Reply::Loaded(data)) => Some(*data),
        Err(NetError::Unauthorized) => return signed_out(),
        Err(e) if is_unreachable(&e) => cached(store),
        Ok(_) => return Outcome::Failed("an unexpected answer".into()),
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    match run(op.clone()) {
        Ok(Reply::Task(made)) => {
            let reminder = pending_reminders(std::slice::from_ref(&made), now, zone)
                .first()
                .map(|r| json::to_string(&crate::android::reminder_args(r)));
            if let Some(data) = data.as_mut() {
                data.put_task(*made);
                write_snapshot(store, data);
            }
            Outcome::Added(reminder)
        }
        Ok(_) => Outcome::Failed("an unexpected answer".into()),
        Err(NetError::Unauthorized) => signed_out(),
        Err(e) if is_unreachable(&e) => {
            queue(store, &op);
            Outcome::Queued
        }
        Err(e) => Outcome::Failed(e.to_string()),
    }
}
