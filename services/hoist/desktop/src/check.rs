//! The look for updates that runs without a window: on a phone a scheduled job starts the program in a mode that only
//! does this, on a desktop `hoist --check` does (and a timer can run that). It signs in with the session the app
//! keeps, reads the registry, works out what is older than it, and says so - once for each set of updates, so a
//! notification is a piece of news, not a reminder every few hours.

use hoist_core::{Artifact, Installed, Update, announcement_key, build_entries, pending_updates};
use hoist_net::{NetError, Session};
use quench_serde::{Value, json};

use crate::Store;

/// What a look found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The "check for updates in the background" setting is off.
    Off,
    /// Nobody is signed in (or the session ended): there is nothing to look with.
    SignedOut,
    /// The updates there are (none, perhaps). `notify` is whether they are news: something not yet announced.
    Updates { updates: Vec<Update>, notify: bool },
}

/// The registry's builds that can be installed here: for this architecture, in a format this system installs.
/// `formats` is `None` where the format does not matter (a phone has only APKs).
#[must_use]
pub fn offered(rows: Vec<Artifact>, arch: &str, formats: Option<&[&str]>) -> Vec<Artifact> {
    rows.into_iter()
        .filter(|a| {
            a.arch
                .as_deref()
                .is_none_or(|x| x == arch || x == "universal")
        })
        .filter(|a| formats.is_none_or(|f| f.contains(&a.format.as_str())))
        .collect()
}

/// Looks for updates. `device` is what the system says is installed (a phone's package manager), over what Hoist
/// itself installed; `os`, `arch` and `formats` say which builds apply (as the window's worker does).
///
/// # Errors
/// The registry could not be read (no network, a server error): the message says why.
pub fn check(
    store: &Store,
    device: &[Installed],
    os: &str,
    arch: &str,
    formats: Option<&[&str]>,
) -> Result<Outcome, String> {
    if !store.auto_check() {
        return Ok(Outcome::Off);
    }
    let Some((server, token)) = store.read() else {
        return Ok(Outcome::SignedOut);
    };
    let (keep, follow) = (store.clone(), store.clone());
    let kept_server = server.clone();
    let mut session = Session::resume(&server, &token)
        .keeping_tokens(move |t| keep.write(&kept_server, t))
        // The window may refresh while this runs, or just before: whichever came last kept the token to use.
        .following(move || follow.read().map(|(_, t)| t));
    let rows = match session.catalog(os) {
        Ok(rows) => rows,
        Err(NetError::Unauthorized) => return Ok(Outcome::SignedOut),
        Err(e) => return Err(e.to_string()),
    };
    let mut installed = store.installed();
    installed.extend(device.iter().map(|i| (i.program.clone(), i.clone())));
    let entries = build_entries(
        &offered(rows, arch, formats),
        &installed,
        hoist_core::SELF_PROGRAM,
    );
    let updates = pending_updates(&entries);
    let key = announcement_key(&updates);
    let notify = !updates.is_empty() && store.announced() != key;
    store.set_announced(&key);
    Ok(Outcome::Updates { updates, notify })
}

/// How many titles a notification names before it says "and N more".
const NAMED: usize = 3;

/// The words of a notification: a title, and the programs with their new versions.
#[must_use]
pub fn words(updates: &[Update]) -> (String, String) {
    let title = match updates.len() {
        1 => "1 update available".to_string(),
        n => format!("{n} updates available"),
    };
    let mut named: Vec<String> = updates
        .iter()
        .take(NAMED)
        .map(|u| format!("{} {}", u.title, u.version_name))
        .collect();
    if updates.len() > NAMED {
        named.push(format!("and {} more", updates.len() - NAMED));
    }
    (title, named.join(", "))
}

/// The what a host is told, as one line of JSON: `{"status": "updates", "notify": true, "title": ..., "body": ...}`,
/// or `off`, `signed_out`, `error` (with `error`). The host decides how to tell the user.
#[must_use]
pub fn report(result: &Result<Outcome, String>) -> String {
    let value = match result {
        Ok(Outcome::Off) => json!({ "status": "off" }),
        Ok(Outcome::SignedOut) => json!({ "status": "signed_out" }),
        Ok(Outcome::Updates { updates, notify }) => {
            let (title, body) = words(updates);
            json!({
                "status": if updates.is_empty() { "none" } else { "updates" },
                "notify": *notify,
                "count": updates.len(),
                "title": title,
                "body": body,
            })
        }
        Err(why) => json!({ "status": "error", "error": why.as_str() }),
    };
    quench_serde::json::to_string(&value)
}

/// What a phone's package manager said is installed, as the host sends it:
/// `[{"package": "...", "version_code": 12, "version_name": "1.2"}]`. Anything that is not a list of such entries is
/// no entries.
#[must_use]
pub fn parse_device(text: &str) -> Vec<Installed> {
    let Ok(Value::Array(list)) = quench_serde::json::from_str::<Value>(text) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|app| {
            Some(Installed {
                program: app.get("package")?.as_str()?.to_string(),
                version_code: app.get("version_code")?.as_i64()?,
                version_name: app
                    .get("version_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect()
}
