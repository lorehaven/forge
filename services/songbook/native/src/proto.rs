//! The conversation with the shell: one JSON object per line, in each direction.
//!
//! A request is `{"id": 7, "op": "children", "parent": "popular"}` (`id` numbers the request; what `item` and `queue` are asked about is `target`); the answer is `{"id": 7, "ok": …}` or
//! `{"id": 7, "err": {"code": "signed_out", "message": "…"}}`, in any order (a slow answer does not hold the others up).

use std::sync::Arc;

use quench_serde::{Value, json};
use songbook_core::{Resume, SongInfo};

use crate::api::Api;
use crate::auth::{Auth, Failure};
use crate::store::{Last, Store};
use crate::tree::Tree;

/// Everything the shell can ask of it.
pub struct Brain {
    store: Store,
    auth: Arc<Auth>,
    tree: Tree,
}

impl std::fmt::Debug for Brain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Brain { .. }")
    }
}

fn text<'a>(request: &'a Value, key: &str) -> &'a str {
    request.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A value of the protocol as JSON.
fn as_value<T: quench_serde::Serialize>(value: &T) -> Value {
    quench_serde::json::parse(&quench_serde::json::to_string(value)).unwrap_or(Value::Null)
}

fn song_of(request: &Value, key: &str) -> Option<SongInfo> {
    quench_serde::json::from_str(&quench_serde::json::to_string(request.get(key)?)).ok()
}

impl Brain {
    #[must_use]
    pub fn new(store: Store) -> Self {
        let auth = Arc::new(Auth::new(store.clone()));
        let api: Arc<dyn Api> = auth.clone();
        Self {
            tree: Tree::new(api, store.clone()),
            store,
            auth,
        }
    }

    /// With the API stood in for (the tree and the settings are the real ones).
    #[must_use]
    pub fn with(store: Store, auth: Arc<Auth>, api: Arc<dyn Api>) -> Self {
        Self {
            tree: Tree::new(api, store.clone()),
            store,
            auth,
        }
    }

    /// Answers one request line with one answer line.
    #[must_use]
    pub fn handle(&self, line: &str) -> String {
        let Ok(request) = quench_serde::json::parse(line) else {
            return json!({ "id": 0, "err": { "code": "bad_request", "message": "not JSON" } })
                .to_string();
        };
        let id = request.get("id").and_then(Value::as_i64).unwrap_or(0);
        match self.run(&request) {
            Ok(ok) => json!({ "id": id, "ok": ok }).to_string(),
            Err(e) => {
                json!({ "id": id, "err": { "code": e.code(), "message": e.message() } }).to_string()
            }
        }
    }

    fn run(&self, r: &Value) -> Result<Value, Failure> {
        let to_value =
            |v: &dyn Fn() -> String| quench_serde::json::parse(&v()).unwrap_or(Value::Null);
        let items =
            |items: &[crate::tree::Item]| to_value(&|| quench_serde::json::to_string(&items));
        match text(r, "op") {
            "status" => {
                let s = self.auth.settings();
                Ok(json!({
                    "signed_in": self.auth.signed_in(),
                    "server": s.server,
                    "user": s.user,
                    "api": if s.server.is_empty() { String::new() } else { self.auth.api() },
                }))
            }
            "login" => {
                self.auth
                    .login(text(r, "server"), text(r, "user"), text(r, "password"))?;
                Ok(json!({}))
            }
            "sign_out" => {
                self.auth.sign_out();
                Ok(json!({}))
            }
            "token" => Ok(json!({ "token": self.auth.token()? })),
            "children" => {
                self.signed_in()?;
                Ok(json!({ "items": items(&self.tree.children(text(r, "parent"))?) }))
            }
            "item" => {
                self.signed_in()?;
                Ok(
                    json!({ "item": to_value(&|| quench_serde::json::to_string(&self.tree.item(text(r, "target")).ok().flatten())) }),
                )
            }
            "queue" => {
                self.signed_in()?;
                Ok(
                    json!({ "queue": to_value(&|| quench_serde::json::to_string(&self.tree.queue(text(r, "target")).ok().flatten())) }),
                )
            }
            "search" => {
                self.signed_in()?;
                Ok(json!({ "items": items(&self.tree.search(text(r, "q"))?) }))
            }
            "songs" => {
                self.signed_in()?;
                let ids: Vec<i64> = r
                    .get("ids")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_i64).collect())
                    .unwrap_or_default();
                Ok(json!({ "items": items(&self.tree.songs(&ids)?) }))
            }
            "resume_get" => {
                self.signed_in()?;
                let found = self.tree.resume()?;
                Ok(json!({ "resume": as_value(&found) }))
            }
            "resume_put" => {
                self.signed_in()?;
                let ids = |key: &str| -> Vec<i64> {
                    r.get(key)
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_i64).collect())
                        .unwrap_or_default()
                };
                let resume = Resume {
                    song_id: r.get("song_id").and_then(Value::as_i64).unwrap_or(0),
                    position_ms: r.get("position_ms").and_then(Value::as_u64).unwrap_or(0),
                    queue: ids("queue"),
                    index: r.get("index").and_then(Value::as_i64).unwrap_or(0),
                    device: text(r, "device").to_string(),
                    at: 0,
                };
                let kept = self.tree.set_resume(&resume)?;
                Ok(json!({ "at": kept.at }))
            }
            "played" => {
                if let Some(song) = song_of(r, "song") {
                    self.tree.played(&song);
                }
                Ok(json!({}))
            }
            "last_save" => {
                if let Some(song) = song_of(r, "song") {
                    self.store.set_last(&Last {
                        id: text(r, "queue_id").to_string(),
                        position_ms: r.get("position_ms").and_then(Value::as_i64).unwrap_or(0),
                        song,
                    });
                }
                Ok(json!({}))
            }
            "last_load" => Ok(
                json!({ "last": to_value(&|| quench_serde::json::to_string(&self.store.last())) }),
            ),
            other => Err(Failure::Refused(format!("unknown op {other}"))),
        }
    }

    fn signed_in(&self) -> Result<(), Failure> {
        if self.auth.signed_in() {
            Ok(())
        } else {
            Err(Failure::SignedOut)
        }
    }
}
