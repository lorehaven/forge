//! The platform of the Android program. The calls to the services run on the worker thread; reminders are the
//! `notifications` plugin's (alarms that the system keeps and re-arms after a reboot).

use std::rc::Rc;
use std::sync::Arc;

use daybook_core::{Data, Reminder};
use daybook_host::Platform;
use daybook_host::remote::{Op, Remote, Reply};
use daybook_host::worker::Worker;
use daybook_net::{NetError, Store};
use quench_android_surface::{Bridge, Map, Value};
use quench_core::time::Time;
use quench_tz::Zone;

/// The channel reminders are shown in.
pub const CHANNEL: &str = "reminders";

fn object(pairs: &[(&str, Value)]) -> Value {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert(*key, value.clone());
    }
    Value::Object(map)
}

/// What `notifications.schedule` takes for a reminder.
#[must_use]
pub fn reminder_args(r: &Reminder) -> Value {
    object(&[
        ("id", Value::from(i64::from(r.id))),
        ("at_ms", Value::from(r.at.timestamp_millis())),
        ("title", Value::String(r.title.clone())),
        ("body", Value::String(r.body.clone())),
        ("channel", Value::String(CHANNEL.into())),
    ])
}

/// The ids of the reminders the plugin holds, from its `pending` answer.
#[must_use]
pub fn pending_ids(answer: &Value) -> Vec<i32> {
    answer
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("id").and_then(Value::as_i64))
                .filter_map(|id| i32::try_from(id).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub struct Android {
    bridge: Bridge,
    worker: Rc<Worker>,
    remote: Arc<Remote>,
    store: Store,
}

impl Android {
    #[must_use]
    pub const fn new(
        bridge: Bridge,
        worker: Rc<Worker>,
        remote: Arc<Remote>,
        store: Store,
    ) -> Self {
        Self {
            bridge,
            worker,
            remote,
            store,
        }
    }
}

impl Platform for Android {
    fn call(&self, op: Op, then: Box<dyn FnOnce(Result<Reply, NetError>)>) {
        self.worker.call(&self.remote, op, then);
    }

    fn replace_reminders(&self, reminders: &[Reminder]) {
        let (bridge, wanted) = (self.bridge.clone(), reminders.to_vec());
        self.bridge
            .call("notifications", "pending", Value::Null, move |answer| {
                // What is armed but no longer wanted goes (a completed or deleted task); what is wanted is armed again, which
                // replaces the alarm of the same number.
                let held = answer.map(|a| pending_ids(&a)).unwrap_or_default();
                for id in held
                    .into_iter()
                    .filter(|id| !wanted.iter().any(|r| r.id == *id))
                {
                    bridge.call(
                        "notifications",
                        "cancel",
                        object(&[("id", Value::from(i64::from(id)))]),
                        |_| {},
                    );
                }
                for r in &wanted {
                    bridge.call("notifications", "schedule", reminder_args(r), |_| {});
                }
            });
    }

    fn ask_notifications(&self, then: Box<dyn FnOnce(bool)>) {
        self.bridge
            .call("notifications", "permission", Value::Null, move |answer| {
                then(answer.is_ok_and(|v| matches!(v.get("granted"), Some(Value::Bool(true)))));
            });
    }

    fn data_changed(&self, data: &Data, now: Time, zone: &Zone) {
        self.store
            .set_widget_snapshot(&daybook_core::widget::snapshot(data, now, zone));
        self.bridge
            .call("daybook", "refresh_widgets", Value::Null, |_| {});
    }

    fn signed_out(&self) {
        self.store.clear_widget_snapshot();
        self.bridge
            .call("daybook", "refresh_widgets", Value::Null, |_| {});
    }

    fn now(&self) -> Time {
        Time::now()
    }

    fn zone(&self) -> Zone {
        Zone::local()
    }

    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        quench_ui::platform::after(u32::try_from(ms).unwrap_or(u32::MAX), f);
    }
}
