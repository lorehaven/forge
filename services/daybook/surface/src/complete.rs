//! `--complete <task id>`: marks a task done with no window (from the reminder's *Done* button and from the widget).
//!
//! It reads the tasks with the saved login, sends the task as done, writes the widget's snapshot and ends. With the
//! services out of reach the change waits in the outbox, which the app sends the next time it starts, and the widget
//! already shows it done.

use daybook_core::widget::snapshot;
use daybook_core::{Data, Task};
use daybook_host::is_unreachable;
use daybook_host::remote::{Op, Remote, Reply};
use daybook_net::{NetError, Store};
use quench_core::time::Time;
use quench_serde::{Value, json};
use quench_tz::Zone;

/// What a run did, for the line it prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The service has the task done (a repeating task moved to its next date).
    Completed,
    /// The services could not be reached: the change is in the outbox and shows in the widget.
    Queued,
    /// There is no such task any more (it was done or deleted elsewhere): the widget is brought up to date.
    Gone,
    /// Nobody is signed in, or the login is no good any more.
    SignedOut,
    /// The service refused, or the tasks could not be read at all.
    Failed(String),
}

impl Outcome {
    /// The one word the Kotlin side reads.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Queued => "queued",
            Self::Gone => "gone",
            Self::SignedOut => "signed_out",
            Self::Failed(_) => "failed",
        }
    }
}

/// Marks `task_id` done with the saved login.
#[must_use]
pub fn complete(store: &Store, task_id: &str) -> Outcome {
    let remote = Remote::new(store.clone());
    if !remote.signed_in() {
        store.clear_widget_snapshot();
        return Outcome::SignedOut;
    }
    complete_with(store, task_id, |op| remote.execute(op))
}

/// [`complete`] over `run`, the way to the services (a stand-in in the tests).
pub fn complete_with(
    store: &Store,
    task_id: &str,
    mut run: impl FnMut(Op) -> Result<Reply, NetError>,
) -> Outcome {
    let signed_out = |store: &Store| {
        // The login is no good any more: sign out, as the window would.
        store.clear_token();
        store.clear_widget_snapshot();
        Outcome::SignedOut
    };
    let mut data = match run(Op::Load) {
        Ok(Reply::Loaded(data)) => *data,
        Err(NetError::Unauthorized) => return signed_out(store),
        // Out of reach: what the app knew last time is what there is to go by.
        Err(e) if is_unreachable(&e) => match cached(store) {
            Some(data) => data,
            None => return Outcome::Failed(e.to_string()),
        },
        Ok(_) => return Outcome::Failed("an unexpected answer".into()),
        Err(e) => return Outcome::Failed(e.to_string()),
    };
    let Some(task) = data.task(task_id).cloned() else {
        write_snapshot(store, &data);
        return Outcome::Gone;
    };
    let mut done: Task = task;
    done.is_completed = true;
    let op = Op::UpdateTask(task_id.to_string(), done.to_update());
    match run(op.clone()) {
        Ok(Reply::Task(task)) => {
            data.put_task(*task);
            write_snapshot(store, &data);
            Outcome::Completed
        }
        Ok(_) => Outcome::Failed("an unexpected answer".into()),
        Err(NetError::Unauthorized) => signed_out(store),
        Err(e) if is_unreachable(&e) => {
            queue(store, &op);
            data.mark(task_id, true, Time::now());
            write_snapshot(store, &data);
            Outcome::Queued
        }
        Err(NetError::NotFound) => {
            // Already gone on the service is as good as done.
            data.mark(task_id, true, Time::now());
            write_snapshot(store, &data);
            Outcome::Gone
        }
        Err(e) => Outcome::Failed(e.to_string()),
    }
}

fn cached(store: &Store) -> Option<Data> {
    let value = json::from_str::<Value>(&store.cache()?).ok()?;
    Some(Data::from_value(&value))
}

fn write_snapshot(store: &Store, data: &Data) {
    store.set_widget_snapshot(&snapshot(data, Time::now(), &Zone::local()));
}

/// Adds the change to the outbox, as the window does for one it could not send.
fn queue(store: &Store, op: &Op) {
    let Some(line) = op.to_value().map(|v| json::to_string(&v)) else {
        return;
    };
    let mut lines = store.outbox();
    if !lines.is_empty() && !lines.ends_with('\n') {
        lines.push('\n');
    }
    lines.push_str(&line);
    lines.push('\n');
    store.set_outbox(&lines);
}
