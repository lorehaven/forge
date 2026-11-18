//! Reading mail: folders, lists, opening a message, its safe HTML, its attachments, and changes (read, star, move, delete).

use crate::app::App;
use crate::connect;
use crate::error::{Error, Result, now};
use crate::store::accounts::AccountRow;
use crate::store::bodies::BodyRow;
use crate::store::folders::FolderRow;
use crate::store::messages::{MessageRow, Query};
use mail_client::message;
use mail_client::sanitize::{self, Options};
use mailroom_core::{
    AttachmentInfo, Change, Contact, Folder, MessageList, MessageSummary, MessageView,
};
use std::collections::{BTreeMap, HashMap};

/// Most messages in one list page.
pub const MAX_PAGE: u32 = 100;
/// Most ids in one change.
pub const MAX_CHANGE: usize = 200;
/// Largest message fetched to read (the rest is refused, not truncated: a cut MIME message misleads).
const MAX_FETCH: u32 = 40 * 1024 * 1024;
/// Largest inline image kept (bytes) and most kept per message.
const MAX_INLINE_IMAGE: usize = 512 * 1024;
const MAX_INLINE_TOTAL: usize = 2 * 1024 * 1024;

/// The API's view of a folder row.
#[must_use]
pub fn folder_model(row: &FolderRow) -> Folder {
    Folder {
        id: row.id.clone(),
        account_id: row.account_id.clone(),
        path: row.path.clone(),
        name: row.name.clone(),
        role: row.role.clone(),
        unread: u32::try_from(row.unread).unwrap_or(0),
        total: u32::try_from(row.total).unwrap_or(0),
    }
}

/// The API's view of a message row.
#[must_use]
pub fn summary_model(row: &MessageRow) -> MessageSummary {
    let to: Vec<Contact> = quench_serde::json::from_str(&row.to_json).unwrap_or_default();
    MessageSummary {
        id: row.id.clone(),
        account_id: row.account_id.clone(),
        folder_id: row.folder_id.clone(),
        subject: row.subject.clone(),
        from: Contact {
            name: row.from_name.clone(),
            address: row.from_address.clone(),
        },
        to,
        date: row.date,
        preview: row.preview.clone(),
        seen: row.seen,
        flagged: row.flagged,
        answered: row.answered,
        attachments: row.attachments,
        size: u32::try_from(row.size).unwrap_or(0),
    }
}

/// The folders of a mailbox `owner` owns.
///
/// # Errors
///
/// [`Error::NotFound`] when `owner` has no such mailbox.
pub async fn folders(app: &App, owner: &str, account_id: &str) -> Result<Vec<Folder>> {
    app.store
        .account_get_owned(account_id, owner)
        .await?
        .ok_or(Error::NotFound)?;
    Ok(app
        .store
        .folders_of(account_id)
        .await?
        .iter()
        .map(folder_model)
        .collect())
}

/// What a list asks for.
#[derive(Debug, Clone, Default)]
pub struct ListSpec {
    /// One folder, or...
    pub folder_id: Option<String>,
    /// ...every folder with this role across the person's mailboxes.
    pub role: Option<String>,
    pub text: String,
    pub unread: bool,
    pub flagged: bool,
    pub offset: u32,
    pub limit: u32,
}

/// The words of a search box, lower-cased, at most eight, each at most 64 characters.
#[must_use]
pub fn search_terms(text: &str) -> Vec<String> {
    text.split_whitespace()
        .take(8)
        .map(|w| w.chars().take(64).collect::<String>().to_lowercase())
        .collect()
}

/// A page of messages.
///
/// # Errors
///
/// [`Error::NotFound`] when the folder is not in one of the person's mailboxes; [`Error::Invalid`] without a folder or role.
pub async fn list(app: &App, owner: &str, spec: &ListSpec) -> Result<MessageList> {
    let limit = i64::from(spec.limit.clamp(1, MAX_PAGE));
    let offset = i64::from(spec.offset);
    let query = Query {
        unread_only: spec.unread,
        flagged_only: spec.flagged,
        terms: search_terms(&spec.text),
    };
    let (rows, total) = if let Some(folder_id) = &spec.folder_id {
        let folder = app
            .store
            .folder_get(folder_id)
            .await?
            .ok_or(Error::NotFound)?;
        app.store
            .account_get_owned(&folder.account_id, owner)
            .await?
            .ok_or(Error::NotFound)?;
        app.store
            .messages_in_folder(folder_id, &query, offset, limit)
            .await?
    } else if let Some(role) = &spec.role {
        if !matches!(
            role.as_str(),
            "inbox" | "sent" | "drafts" | "trash" | "junk" | "archive"
        ) {
            return Err(Error::Invalid("unknown folder role".into()));
        }
        app.store
            .messages_by_role(owner, role, &query, offset, limit)
            .await?
    } else {
        return Err(Error::Invalid("say which folder".into()));
    };
    Ok(MessageList {
        items: rows.iter().map(summary_model).collect(),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        offset: spec.offset,
    })
}

/// Parses a fetched message and keeps its body: the text, the HTML as sent, the attachment list, inline images (small, as
/// `data:` URIs the sanitiser can use), and a preview for the list.
///
/// # Errors
///
/// A database failure.
pub async fn keep_body(
    app: &App,
    _account: &AccountRow,
    row: &MessageRow,
    raw: &[u8],
) -> Result<BodyRow> {
    let parsed = message::parse(raw);
    let mut inline: BTreeMap<String, String> = BTreeMap::new();
    let mut total = 0;
    for attachment in &parsed.attachments {
        let Some(cid) = &attachment.content_id else {
            continue;
        };
        if !attachment.inline
            || !matches!(
                attachment.media_type.as_str(),
                "image/png" | "image/jpeg" | "image/gif" | "image/webp"
            )
            || attachment.size > MAX_INLINE_IMAGE
            || total + attachment.size > MAX_INLINE_TOTAL
        {
            continue;
        }
        if let Some((_, media_type, data)) = message::attachment_data(raw, attachment.index) {
            total += data.len();
            inline.insert(
                cid.clone(),
                format!(
                    "data:{media_type};base64,{}",
                    mail_client::encoding::base64_encode(&data)
                ),
            );
        }
    }
    let contacts = |list: &[mail_client::address::Contact]| -> String {
        let converted: Vec<Contact> = list
            .iter()
            .map(|c| Contact {
                name: c.name.clone(),
                address: c.address.clone(),
            })
            .collect();
        quench_serde::json::to_string(&converted)
    };
    let attachments: Vec<AttachmentInfo> = parsed
        .attachments
        .iter()
        .map(|a| AttachmentInfo {
            index: u32::try_from(a.index).unwrap_or(0),
            filename: a.filename.clone(),
            media_type: a.media_type.clone(),
            size: u32::try_from(a.size).unwrap_or(u32::MAX),
            inline: a.inline,
        })
        .collect();
    let body = BodyRow {
        message_id: row.id.clone(),
        text: parsed.text.clone(),
        html: parsed.html.clone().filter(|h| h.len() <= 2 * 1024 * 1024),
        cc_json: contacts(&parsed.summary.cc),
        reply_to_json: contacts(&parsed.summary.reply_to),
        list_unsubscribe: parsed.summary.list_unsubscribe.chars().take(1000).collect(),
        attachments_json: quench_serde::json::to_string(&attachments),
        inline_json: quench_serde::json::to_string(&inline),
        fetched: now(),
    };
    app.store.body_put(&body).await?;
    let preview: String = parsed
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(160)
        .collect();
    app.store.message_set_preview(&row.id, &preview).await?;
    Ok(body)
}

struct Opened {
    message: MessageRow,
    account: AccountRow,
    body: BodyRow,
}

/// The message, its mailbox and its body (fetched from the server the first time).
async fn opened(app: &App, owner: &str, id: &str) -> Result<Opened> {
    let message = app
        .store
        .message_get_owned(id, owner)
        .await?
        .ok_or(Error::NotFound)?;
    let account = app
        .store
        .account_get(&message.account_id)
        .await?
        .ok_or(Error::NotFound)?;
    if account.disabled {
        return Err(Error::Forbidden);
    }
    if let Some(body) = app.store.body_get(id).await? {
        return Ok(Opened {
            message,
            account,
            body,
        });
    }
    let raw = fetch_raw(app, &account, &message).await?;
    let body = keep_body(app, &account, &message, &raw).await?;
    Ok(Opened {
        message,
        account,
        body,
    })
}

/// The raw message from the server.
async fn fetch_raw(app: &App, account: &AccountRow, message: &MessageRow) -> Result<Vec<u8>> {
    if u32::try_from(message.size).unwrap_or(u32::MAX) > MAX_FETCH {
        return Err(Error::Invalid(
            "this message is too large to open here".into(),
        ));
    }
    let folder = app
        .store
        .folder_get(&message.folder_id)
        .await?
        .ok_or(Error::NotFound)?;
    let mut client = connect::open_imap(app, account).await?;
    let selected = client
        .select(&folder.path, true)
        .await
        .map_err(|e| Error::Upstream(connect::upstream_text(&e)))?;
    if folder.uid_validity != 0 && i64::from(selected.uid_validity) != folder.uid_validity {
        // The folder was renumbered since we last looked: this UID is not this message any more.
        client.logout().await;
        app.hub.request(&account.id);
        return Err(Error::Conflict(
            "the mailbox changed; refresh and try again".into(),
        ));
    }
    let uid = u32::try_from(message.uid).map_err(|_| Error::NotFound)?;
    let raw = client
        .fetch_message(uid)
        .await
        .map_err(|e| Error::Upstream(connect::upstream_text(&e)))?;
    client.logout().await;
    raw.ok_or_else(|| {
        app.hub.request(&account.id);
        Error::NotFound
    })
}

/// Opens a message.
///
/// # Errors
///
/// [`Error::NotFound`], or [`Error::Upstream`] when the mail server cannot be reached.
pub async fn open(app: &App, owner: &str, id: &str) -> Result<MessageView> {
    let Opened { message, body, .. } = opened(app, owner, id).await?;
    let attachments: Vec<AttachmentInfo> =
        quench_serde::json::from_str(&body.attachments_json).unwrap_or_default();
    let has_html = body.html.as_deref().is_some_and(|h| !h.trim().is_empty());
    let remote_images = if has_html {
        let html = body.html.as_deref().unwrap_or("");
        let inline: HashMap<String, String> =
            quench_serde::json::from_str(&body.inline_json).unwrap_or_default();
        sanitize::sanitize(html, &Options::default(), |cid| inline.get(cid).cloned()).remote_blocked
    } else {
        0
    };
    Ok(MessageView {
        summary: summary_model(&message),
        cc: quench_serde::json::from_str(&body.cc_json).unwrap_or_default(),
        reply_to: quench_serde::json::from_str(&body.reply_to_json).unwrap_or_default(),
        message_id: message.message_id.clone(),
        text: body.text,
        has_html,
        remote_images: u32::try_from(remote_images).unwrap_or(u32::MAX),
        attachments,
        list_unsubscribe: body.list_unsubscribe,
    })
}

/// The content-security-policy the HTML of a message is served under: no script, no frames, no forms, no connections; images
/// from the message itself (and from the network only when the person allowed it); the document is sandboxed.
#[must_use]
pub fn html_policy(remote: bool) -> String {
    let images = if remote {
        "data: https: http:"
    } else {
        "data:"
    };
    format!(
        "default-src 'none'; img-src {images}; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; \
         frame-ancestors 'self'; sandbox allow-popups allow-popups-to-escape-sandbox"
    )
}

/// The message's HTML, sanitised, as a complete document for a sandboxed frame.
///
/// # Errors
///
/// [`Error::NotFound`] when there is no such message or it has no HTML.
pub async fn html(app: &App, owner: &str, id: &str, remote: bool) -> Result<String> {
    let Opened { body, .. } = opened(app, owner, id).await?;
    let html = body
        .html
        .as_deref()
        .filter(|h| !h.trim().is_empty())
        .ok_or(Error::NotFound)?;
    let inline: HashMap<String, String> =
        quench_serde::json::from_str(&body.inline_json).unwrap_or_default();
    let clean = sanitize::sanitize(
        html,
        &Options {
            allow_remote_images: remote,
            ..Options::default()
        },
        |cid| inline.get(cid).cloned(),
    );
    Ok(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"referrer\" content=\"no-referrer\">\
         <base target=\"_blank\"><style>body{{margin:0;padding:12px;font:14px/1.5 system-ui,sans-serif;color:#222;background:#fff;\
         overflow-wrap:anywhere}}img{{max-width:100%;height:auto}}table{{max-width:100%}}blockquote{{margin:.5em 0 .5em .5em;\
         padding-left:.8em;border-left:3px solid #ccc;color:#555}}pre{{white-space:pre-wrap}}a{{color:#1a5fb4}}</style></head>\
         <body>{}</body></html>",
        clean.html
    ))
}

/// One attachment: its file name and bytes (fetched from the server again; attachments are not cached).
///
/// # Errors
///
/// [`Error::NotFound`] when there is no such message or attachment.
pub async fn attachment(app: &App, owner: &str, id: &str, index: u32) -> Result<(String, Vec<u8>)> {
    let Opened {
        message, account, ..
    } = opened(app, owner, id).await?;
    let raw = fetch_raw(app, &account, &message).await?;
    let (filename, _media_type, data) =
        message::attachment_data(&raw, index as usize).ok_or(Error::NotFound)?;
    Ok((filename, data))
}

/// Applies a change to messages: seen, starred, moved, deleted. The server is changed first, then the cache.
///
/// # Errors
///
/// [`Error::NotFound`] for an unknown message, [`Error::Invalid`] for a change that cannot be made, [`Error::Upstream`] when
/// the mail server refuses.
pub async fn change(app: &App, owner: &str, change: &Change) -> Result<()> {
    if change.ids.is_empty() || change.ids.len() > MAX_CHANGE {
        return Err(Error::Invalid(format!("choose 1 to {MAX_CHANGE} messages")));
    }
    if change.seen.is_none()
        && change.flagged.is_none()
        && change.move_to.is_none()
        && !change.delete
    {
        return Err(Error::Invalid("nothing to change".into()));
    }
    if change.delete && change.move_to.is_some() {
        return Err(Error::Invalid("delete or move, not both".into()));
    }
    let mut rows = Vec::new();
    for id in &change.ids {
        rows.push(
            app.store
                .message_get_owned(id, owner)
                .await?
                .ok_or(Error::NotFound)?,
        );
    }
    let mut by_folder: BTreeMap<String, Vec<MessageRow>> = BTreeMap::new();
    for row in rows {
        by_folder
            .entry(row.folder_id.clone())
            .or_default()
            .push(row);
    }
    let mut sessions: HashMap<String, mail_client::imap::Client> = HashMap::new();
    let result = apply(app, owner, change, by_folder, &mut sessions).await;
    for (_, client) in sessions {
        client.logout().await;
    }
    result
}

async fn apply(
    app: &App,
    owner: &str,
    change: &Change,
    by_folder: BTreeMap<String, Vec<MessageRow>>,
    sessions: &mut HashMap<String, mail_client::imap::Client>,
) -> Result<()> {
    let upstream = |e: mail_client::imap::Error| Error::Upstream(connect::upstream_text(&e));
    for (folder_id, messages) in by_folder {
        let folder = app
            .store
            .folder_get(&folder_id)
            .await?
            .ok_or(Error::NotFound)?;
        let account = app
            .store
            .account_get_owned(&folder.account_id, owner)
            .await?
            .ok_or(Error::NotFound)?;
        if account.disabled {
            return Err(Error::Forbidden);
        }
        let uids: Vec<u32> = messages
            .iter()
            .filter_map(|m| u32::try_from(m.uid).ok())
            .collect();
        let set = crate::sync::uid_set(&uids);

        let destination = match &change.move_to {
            Some(target) => Some(resolve_destination(app, &account, target).await?),
            None => None,
        };
        if change.delete && !matches!(folder.role.as_str(), "trash" | "junk") {
            return Err(Error::Invalid(
                "messages are deleted for good only from the trash; move them there first".into(),
            ));
        }

        if !sessions.contains_key(&account.id) {
            sessions.insert(account.id.clone(), connect::open_imap(app, &account).await?);
        }
        let client = sessions
            .get_mut(&account.id)
            .ok_or_else(|| Error::Internal("no session".into()))?;
        let selected = client.select(&folder.path, false).await.map_err(upstream)?;
        if folder.uid_validity != 0 && i64::from(selected.uid_validity) != folder.uid_validity {
            // The server renumbered the folder since the last look: these UIDs are other messages now.
            app.hub.request(&account.id);
            return Err(Error::Conflict(
                "the mailbox changed; refresh and try again".into(),
            ));
        }
        if let Some(seen) = change.seen {
            client
                .store_flags(&set, if seen { "+" } else { "-" }, &["\\Seen"])
                .await
                .map_err(upstream)?;
        }
        if let Some(flagged) = change.flagged {
            client
                .store_flags(&set, if flagged { "+" } else { "-" }, &["\\Flagged"])
                .await
                .map_err(upstream)?;
        }
        let ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
        if let Some(destination) = destination {
            if destination.id == folder.id {
                return Err(Error::Invalid("the messages are already there".into()));
            }
            client
                .move_to(&set, &destination.path)
                .await
                .map_err(upstream)?;
            app.store.messages_delete_ids(&ids).await?;
            app.store.folder_recount(&destination.id).await?;
            app.hub.request(&account.id);
        } else if change.delete {
            client
                .store_flags(&set, "+", &["\\Deleted"])
                .await
                .map_err(upstream)?;
            client.expunge(Some(&set)).await.map_err(upstream)?;
            app.store.messages_delete_ids(&ids).await?;
        } else {
            for id in &ids {
                app.store
                    .message_set_state(id, change.seen, change.flagged, None)
                    .await?;
            }
        }
        app.store.folder_recount(&folder.id).await?;
    }
    Ok(())
}

/// The folder a move goes to: a role name (`trash`, `archive`, `junk`, `inbox`) or a folder id, in the same mailbox.
async fn resolve_destination(app: &App, account: &AccountRow, target: &str) -> Result<FolderRow> {
    if matches!(
        target,
        "trash" | "archive" | "junk" | "inbox" | "sent" | "drafts"
    ) {
        return app
            .store
            .folder_by_role(&account.id, target)
            .await?
            .ok_or_else(|| Error::Invalid(format!("this mailbox has no {target} folder")));
    }
    let folder = app.store.folder_get(target).await?.ok_or(Error::NotFound)?;
    if folder.account_id != account.id {
        return Err(Error::Invalid("messages stay in their own mailbox".into()));
    }
    Ok(folder)
}

/// Asks for a mailbox to be refreshed now and waits for it.
///
/// # Errors
///
/// [`Error::NotFound`] when `owner` has no such mailbox; [`Error::Limited`] when asked too often.
pub async fn refresh(app: &App, owner: &str, account_id: &str) -> Result<()> {
    app.store
        .account_get_owned(account_id, owner)
        .await?
        .ok_or(Error::NotFound)?;
    app.limits
        .hit(&format!("refresh:{owner}"), 20, 60, now())
        .map_err(Error::Limited)?;
    crate::sync::sync_account(app, account_id).await
}
