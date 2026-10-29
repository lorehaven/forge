//! `--widget`: the background refresh of the home-screen widget. No window: it reads the tasks with the saved login,
//! writes the snapshot the widget draws, and ends. The Kotlin job that starts it redraws the widget afterwards.

use daybook_core::widget::snapshot;
use daybook_host::remote::{Op, Remote, Reply};
use daybook_net::{NetError, Store};
use quench_core::time::Time;
use quench_tz::Zone;

/// What a run did, for the line it prints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The snapshot was written (this many open tasks).
    Refreshed(usize),
    /// Nobody is signed in: the widget is emptied.
    SignedOut,
    /// The services could not be reached: the widget keeps what it had.
    Unreachable(String),
}

/// One refresh.
pub fn refresh(store: &Store) -> Outcome {
    let remote = Remote::new(store.clone());
    if !remote.signed_in() {
        store.clear_widget_snapshot();
        return Outcome::SignedOut;
    }
    match remote.execute(Op::Load) {
        Ok(Reply::Loaded(data)) => {
            store.set_widget_snapshot(&snapshot(&data, Time::now(), &Zone::local()));
            Outcome::Refreshed(data.tasks.len())
        }
        Ok(_) => Outcome::Unreachable("an unexpected answer".into()),
        Err(NetError::Unauthorized) => {
            // The login is no good any more: sign out, as the window would.
            store.clear_token();
            store.clear_widget_snapshot();
            Outcome::SignedOut
        }
        Err(e) => Outcome::Unreachable(e.to_string()),
    }
}
