//! What both clients (the program's blocking one, the browser's) agree on: the paths, the bodies, how the rows are read and
//! what gatehouse's refusals mean.

use daybook_core::{Label, Project, Task};
use quench_serde::{Map, Value};

use crate::NetError;

/// Where the service's API is, under the server.
pub const API: &str = "/daybook/api/v1";

/// Where gatehouse's sign-in and refresh are.
pub const LOGIN: &str = "/gatehouse/api/v1/auth/login";
pub const REFRESH: &str = "/gatehouse/api/v1/auth/refresh";

#[must_use]
pub fn project_body(name: &str, sort_order: i64) -> Value {
    let mut m = Map::new();
    m.insert("name", Value::String(name.trim().to_string()));
    m.insert("sort_order", Value::Int(sort_order));
    Value::Object(m)
}

#[must_use]
pub fn label_body(name: &str, color: &str) -> Value {
    let mut m = Map::new();
    m.insert("name", Value::String(name.trim().to_string()));
    m.insert("color", Value::String(color.to_string()));
    Value::Object(m)
}

/// One path segment, percent-encoded.
#[must_use]
pub fn segment(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The tasks of a JSON list.
#[must_use]
pub fn tasks(value: &Value) -> Vec<Task> {
    rows(value, Task::from_value)
}

#[must_use]
pub fn projects(value: &Value) -> Vec<Project> {
    rows(value, Project::from_value)
}

#[must_use]
pub fn labels(value: &Value) -> Vec<Label> {
    rows(value, Label::from_value)
}

fn rows<T>(value: &Value, parse: fn(&Value) -> Option<T>) -> Vec<T> {
    value
        .as_array()
        .map(|rows| rows.iter().filter_map(parse).collect())
        .unwrap_or_default()
}

/// What a refused sign-in or refresh means: gatehouse says why in the body (`{"error": "account_locked"}`), and a locked
/// account is not a wrong password.
#[must_use]
pub fn sign_in_error(status: u16, body: &str) -> NetError {
    match status {
        400..=403 if body.contains("mfa_required") => NetError::MfaRequired,
        400 | 401 | 403 if body.contains("account_locked") => NetError::AccountLocked,
        400 | 401 | 403 if body.contains("account_disabled") => NetError::AccountDisabled,
        400 | 401 | 403 if body.contains("email_unverified") => NetError::EmailUnverified,
        400 | 401 | 403 => NetError::Unauthorized,
        other => NetError::Status(other),
    }
}
