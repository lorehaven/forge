//! Keeping the cache of a mailbox up to date: folders, new messages, vanished messages, changed flags.
//!
//! One run per mailbox at a time. A run reads the server and never writes to it. What it fetches is bounded (the newest
//! messages on the first look, a few hundred new ones per run), so a huge mailbox is learned over a few runs.

use crate::app::App;
use crate::connect;
use crate::error::{Error, Result, new_id, now};
use crate::mailbox;
use crate::store::accounts::AccountRow;
use crate::store::folders::FolderRow;
use crate::store::messages::MessageRow;
use mail_client::imap::{self, Client, Search};
use mail_client::message;
use mailroom_core::Contact;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

/// New messages fetched per folder per run.
const MAX_NEW_PER_RUN: usize = 500;
/// Messages whose flags are looked at again per folder per run.
const FLAG_WINDOW: usize = 500;
/// UIDs per command.
const CHUNK: usize = 100;
/// Bodies prefetched for the inbox per run (they give the list its previews).
const PREFETCH: i64 = 10;
/// Largest message prefetched.
const PREFETCH_MAX_SIZE: i32 = 300_000;

/// Which mailboxes are being synced or asked to be.
#[derive(Debug, Default)]
pub struct SyncHub {
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    dirty: Mutex<HashSet<String>>,
}

impl SyncHub {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for a mailbox to be looked at soon.
    pub fn request(&self, account_id: &str) {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(account_id.to_string());
    }

    /// The mailboxes asked for since the last call.
    pub fn take_requested(&self) -> Vec<String> {
        self.dirty
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain()
            .collect()
    }

    fn lock_for(&self, account_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(account_id.to_string())
            .or_default()
            .clone()
    }

    /// Forgets a mailbox that was removed.
    pub fn forget(&self, account_id: &str) {
        self.locks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(account_id);
    }
}

/// Looks at one mailbox now (waiting if a run is already going) and records how it went.
///
/// # Errors
///
/// A database failure; a failure of the mail server is recorded on the mailbox, not returned.
pub async fn sync_account(app: &App, id: &str) -> Result<()> {
    let lock = app.hub.lock_for(id);
    let _running = lock.lock().await;
    let Some(row) = app.store.account_get(id).await? else {
        return Ok(());
    };
    if row.disabled {
        return Ok(());
    }
    // A server that drips bytes must not hold a sync slot for ever.
    let result = tokio::time::timeout(std::time::Duration::from_mins(15), run(app, &row))
        .await
        .unwrap_or_else(|_| Err(Error::Upstream("the mail server took too long".into())));
    match result {
        Ok(()) => {
            app.store
                .account_set_state(id, "ok", None, Some(now()))
                .await
        }
        Err(Error::Db(e)) => Err(Error::Db(e)),
        Err(e) => {
            let (_, message) = e.parts();
            quench_log::warn!("sync of mailbox {id} failed: {e}");
            app.store
                .account_set_state(id, "error", Some(&message), Some(now()))
                .await
        }
    }
}

async fn run(app: &App, row: &AccountRow) -> Result<()> {
    let mut client = connect::open_imap(app, row).await?;
    let listed = client.list_folders().await.map_err(|e| upstream(&e))?;
    let mut keep = Vec::new();
    let mut folders = Vec::new();
    for folder in listed.iter().filter(|f| f.selectable()) {
        let role = folder.role();
        // "All Mail" and "Starred" repeat what other folders hold.
        if matches!(role, "all" | "flagged") {
            continue;
        }
        let name = folder
            .path
            .rsplit(folder.delimiter.unwrap_or('/'))
            .next()
            .unwrap_or(&folder.path)
            .to_string();
        keep.push(folder.path.clone());
        folders.push(
            app.store
                .folder_upsert(&row.id, &folder.path, &name, role)
                .await?,
        );
    }
    app.store.folders_keep(&row.id, &keep).await?;
    for folder in &folders {
        match sync_folder(app, &mut client, row, folder).await {
            Ok(()) => {}
            // One odd folder (permissions, a server quirk) must not stop the rest.
            Err(Error::Upstream(text)) => {
                quench_log::warn!("sync of folder {} skipped: {text}", folder.path)
            }
            Err(e) => return Err(e),
        }
    }
    client.logout().await;
    Ok(())
}

fn upstream(error: &imap::Error) -> Error {
    Error::Upstream(connect::upstream_text(error))
}

async fn sync_folder(
    app: &App,
    client: &mut Client,
    row: &AccountRow,
    folder: &FolderRow,
) -> Result<()> {
    let selected = client
        .select(&folder.path, true)
        .await
        .map_err(|e| upstream(&e))?;
    let validity = i64::from(selected.uid_validity);
    if folder.uid_validity != 0 && folder.uid_validity != validity {
        // The server renumbered the folder: every UID we hold means something else now.
        app.store.messages_clear_folder(&folder.id).await?;
    }
    let server: Vec<u32> = client
        .uid_search(Search::all())
        .await
        .map_err(|e| upstream(&e))?;
    let server_set: HashSet<i64> = server.iter().map(|u| i64::from(*u)).collect();
    let cached = app.store.message_uids(&folder.id).await?;
    let cached_set: HashSet<i64> = cached.iter().copied().collect();

    let gone: Vec<i64> = cached
        .iter()
        .copied()
        .filter(|u| !server_set.contains(u))
        .collect();
    app.store.messages_delete_uids(&folder.id, &gone).await?;

    let mut fresh: Vec<u32> = server
        .iter()
        .copied()
        .filter(|u| !cached_set.contains(&i64::from(*u)))
        .collect();
    fresh.sort_unstable_by(|a, b| b.cmp(a));
    let budget = if cached.is_empty() {
        app.config.initial_messages as usize
    } else {
        MAX_NEW_PER_RUN
    };
    fresh.truncate(budget);
    for chunk in fresh.chunks(CHUNK) {
        let set = uid_set(chunk);
        let fetched = client
            .fetch_headers(
                &set,
                "FROM TO CC SUBJECT DATE MESSAGE-ID IN-REPLY-TO REFERENCES CONTENT-TYPE",
            )
            .await
            .map_err(|e| upstream(&e))?;
        for item in fetched {
            // A server answering for a UID that was not asked for is ignored.
            if !chunk.contains(&item.uid) {
                continue;
            }
            app.store
                .message_insert(&message_row(row, folder, &item))
                .await?;
        }
    }

    // Flags may have changed elsewhere: look again at the newest cached messages that still exist.
    let mut window: Vec<u32> = cached
        .iter()
        .filter(|u| server_set.contains(u))
        .filter_map(|u| u32::try_from(*u).ok())
        .collect();
    window.sort_unstable_by(|a, b| b.cmp(a));
    window.truncate(FLAG_WINDOW);
    for chunk in window.chunks(CHUNK) {
        for (uid, flags) in client
            .fetch_flags(&uid_set(chunk))
            .await
            .map_err(|e| upstream(&e))?
        {
            let (seen, flagged, answered) = flag_state(&flags);
            app.store
                .message_set_flags(&folder.id, i64::from(uid), seen, flagged, answered)
                .await?;
        }
    }

    if folder.role == "inbox" {
        prefetch(app, client, row, folder).await?;
    }
    app.store
        .folder_set_sync(&folder.id, validity, i64::from(selected.uid_next))
        .await?;
    app.store.folder_recount(&folder.id).await?;
    Ok(())
}

async fn prefetch(
    app: &App,
    client: &mut Client,
    row: &AccountRow,
    folder: &FolderRow,
) -> Result<()> {
    let wanted = app
        .store
        .messages_to_prefetch(&folder.id, PREFETCH_MAX_SIZE, PREFETCH)
        .await?;
    for message in wanted {
        let Ok(uid) = u32::try_from(message.uid) else {
            continue;
        };
        let Some(raw) = client.fetch_message(uid).await.map_err(|e| upstream(&e))? else {
            continue;
        };
        mailbox::keep_body(app, row, &message, &raw).await?;
    }
    Ok(())
}

/// `(seen, flagged, answered)` from a server's flag list.
#[must_use]
pub fn flag_state(flags: &[String]) -> (bool, bool, bool) {
    let has = |name: &str| flags.iter().any(|f| f.eq_ignore_ascii_case(name));
    (has("\\Seen"), has("\\Flagged"), has("\\Answered"))
}

fn message_row(account: &AccountRow, folder: &FolderRow, item: &imap::Fetched) -> MessageRow {
    let summary = message::summarize(&item.data);
    let from = summary
        .from
        .first()
        .cloned()
        .unwrap_or(mail_client::address::Contact {
            name: String::new(),
            address: String::new(),
        });
    let to: Vec<Contact> = summary
        .to
        .iter()
        .take(20)
        .map(|c| Contact {
            name: c.name.clone(),
            address: c.address.clone(),
        })
        .collect();
    let (seen, flagged, answered) = flag_state(&item.flags);
    let content_type = header_value(&item.data, "content-type").to_ascii_lowercase();
    let search = format!(
        "{} {} {} {}",
        summary.subject,
        from.name,
        from.address,
        to.iter()
            .map(|c| format!("{} {}", c.name, c.address))
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_lowercase();
    MessageRow {
        id: new_id(),
        account_id: account.id.clone(),
        folder_id: folder.id.clone(),
        uid: i64::from(item.uid),
        subject: summary.subject.chars().take(1000).collect(),
        from_name: from.name.chars().take(200).collect(),
        from_address: from.address.chars().take(320).collect(),
        to_json: quench_serde::json::to_string(&to),
        date: summary.date.or(item.internal_date).unwrap_or(0),
        preview: String::new(),
        message_id: summary.message_id,
        in_reply_to: summary.in_reply_to,
        seen,
        flagged,
        answered,
        attachments: content_type.starts_with("multipart/mixed"),
        size: i32::try_from(item.size).unwrap_or(i32::MAX),
        search: search.chars().take(2000).collect(),
    }
}

fn header_value(block: &[u8], name: &str) -> String {
    mail_client::header::Headers::parse(block)
        .0
        .get(name)
        .unwrap_or("")
        .to_string()
}

/// `1,5:9,12` for the UIDs (any order).
#[must_use]
pub fn uid_set(uids: &[u32]) -> String {
    let mut sorted = uids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut out = String::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = sorted[i];
        let mut end = start;
        while i + 1 < sorted.len() && sorted[i + 1] == end + 1 {
            i += 1;
            end = sorted[i];
        }
        if !out.is_empty() {
            out.push(',');
        }
        if start == end {
            out.push_str(&start.to_string());
        } else {
            let _ = write!(out, "{start}:{end}");
        }
        i += 1;
    }
    out
}
