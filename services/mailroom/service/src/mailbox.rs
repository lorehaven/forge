//! Reading mail: folders, lists, opening a message, its safe HTML, its attachments, and changes (read, star, move, delete).

use crate::app::App;
use crate::error::{Error, Result, now};
use crate::store::mailboxes::MailboxRow;
use crate::store::messages::{MessageRow, Query};
use mail_html::sanitize::{self, Options};
use mailroom_core::{
    AttachmentInfo, Change, Contact, FOLDERS, Folder, Me, MessageList, MessageSummary, MessageView,
    draft,
};

/// Most messages in one list page.
pub const MAX_PAGE: u32 = 100;
/// Most ids in one change.
pub const MAX_CHANGE: usize = 200;
/// Most people the address completion offers.
pub const MAX_PEOPLE: i64 = 20;

/// The mailbox of a person, made on first use, unless an administrator disabled it.
///
/// # Errors
///
/// [`Error::Forbidden`] for a disabled mailbox, [`Error::Invalid`] for a user name that cannot be an address.
pub async fn open_mailbox(app: &App, owner: &str) -> Result<MailboxRow> {
    // Callers pass the lower-cased user name; anything else would make an address that does not match its owner.
    if !draft::local_part_ok(owner) || owner != owner.to_ascii_lowercase() {
        return Err(Error::Invalid(
            "this user name cannot have a mailbox (letters, digits, dots, hyphens and underscores only)".into(),
        ));
    }
    let stamp = now();
    let mailbox = app
        .store
        .mailbox_ensure(owner, &app.address_of(owner), stamp)
        .await?;
    if mailbox.disabled {
        return Err(Error::Forbidden);
    }
    // A minute is fine-grained enough for "last seen".
    if mailbox.last_seen.is_none_or(|seen| stamp - seen >= 60) {
        app.store.mailbox_touch(owner, stamp).await?;
    }
    Ok(mailbox)
}

/// Who the caller is, for the client.
///
/// # Errors
///
/// As [`open_mailbox`].
pub async fn me(app: &App, owner: &str, can_write: bool, admin: bool) -> Result<Me> {
    let mailbox = open_mailbox(app, owner).await?;
    Ok(Me {
        username: owner.to_string(),
        address: mailbox.address,
        can_write,
        admin,
    })
}

/// The API's view of a message row.
#[must_use]
pub fn summary_model(row: &MessageRow) -> MessageSummary {
    let to: Vec<Contact> = quench_serde::json::from_str(&row.to_json).unwrap_or_default();
    MessageSummary {
        id: row.id.clone(),
        folder: row.folder.clone(),
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
        automated: !row.source.is_empty(),
    }
}

/// The folders with their counts (all three, empty ones too).
///
/// # Errors
///
/// A database failure or a disabled mailbox.
pub async fn folders(app: &App, owner: &str) -> Result<Vec<Folder>> {
    open_mailbox(app, owner).await?;
    let counts = app.store.folder_counts(owner).await?;
    Ok(FOLDERS
        .iter()
        .map(|id| {
            let (unread, total) = counts
                .iter()
                .find(|(f, _, _)| f == id)
                .map_or((0, 0), |(_, u, t)| (*u, *t));
            Folder {
                id: (*id).to_string(),
                unread: u32::try_from(unread).unwrap_or(0),
                total: u32::try_from(total).unwrap_or(0),
            }
        })
        .collect())
}

/// What a list asks for.
#[derive(Debug, Clone, Default)]
pub struct ListSpec {
    pub folder: String,
    pub text: String,
    pub unread: bool,
    pub flagged: bool,
    pub offset: u32,
    pub limit: u32,
}

/// A page of a folder, newest first.
///
/// # Errors
///
/// [`Error::Invalid`] for an unknown folder.
pub async fn list(app: &App, owner: &str, spec: &ListSpec) -> Result<MessageList> {
    open_mailbox(app, owner).await?;
    let folder = if spec.folder.is_empty() {
        "inbox"
    } else {
        spec.folder.as_str()
    };
    if !FOLDERS.contains(&folder) {
        return Err(Error::Invalid("there is no such folder".into()));
    }
    let limit = spec.limit.clamp(1, MAX_PAGE);
    let terms: Vec<String> = spec
        .text
        .split_whitespace()
        .take(8)
        .map(|t| t.chars().take(100).collect::<String>().to_lowercase())
        .collect();
    let query = Query {
        unread_only: spec.unread,
        flagged_only: spec.flagged,
        terms,
    };
    let (rows, total) = app
        .store
        .messages_page(
            owner,
            folder,
            &query,
            i64::from(spec.offset),
            i64::from(limit),
        )
        .await?;
    Ok(MessageList {
        items: rows.iter().map(summary_model).collect(),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        offset: spec.offset,
    })
}

async fn owned(app: &App, owner: &str, id: &str) -> Result<MessageRow> {
    open_mailbox(app, owner).await?;
    app.store
        .message_get_owned(id, owner)
        .await?
        .ok_or(Error::NotFound)
}

/// An opened message.
///
/// # Errors
///
/// [`Error::NotFound`] when it is not in the caller's mailbox.
pub async fn open(app: &App, owner: &str, id: &str) -> Result<MessageView> {
    let row = owned(app, owner, id).await?;
    let body = app.store.message_body(&row.id).await?;
    let attachments = app.store.attachments_of(&row.id).await?;
    let cc: Vec<Contact> = quench_serde::json::from_str(&row.cc_json).unwrap_or_default();
    let remote_images = body
        .html
        .as_deref()
        .filter(|h| !h.trim().is_empty())
        .map_or(0, |h| {
            sanitize::sanitize(h, &Options::default(), |_| None).remote_blocked
        });
    Ok(MessageView {
        summary: summary_model(&row),
        cc,
        message_id: row.message_id.clone(),
        text: body.text,
        has_html: body.html.as_deref().is_some_and(|h| !h.trim().is_empty()),
        remote_images: u32::try_from(remote_images).unwrap_or(u32::MAX),
        attachments: attachments
            .iter()
            .map(|a| AttachmentInfo {
                index: u32::try_from(a.idx).unwrap_or(0),
                filename: a.filename.clone(),
                media_type: a.media_type.clone(),
                size: u32::try_from(a.size).unwrap_or(0),
            })
            .collect(),
        unsubscribe: row.unsubscribe,
    })
}

/// The content-security-policy of the frame that shows a message's HTML: nothing may load, except images from the message itself
/// (and from the network only when the person allowed it); the document is sandboxed.
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
    let row = owned(app, owner, id).await?;
    let body = app.store.message_body(&row.id).await?;
    let html = body
        .html
        .as_deref()
        .filter(|h| !h.trim().is_empty())
        .ok_or(Error::NotFound)?;
    let clean = sanitize::sanitize(
        html,
        &Options {
            allow_remote_images: remote,
            ..Options::default()
        },
        |_| None,
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

/// One attachment: its file name and bytes.
///
/// # Errors
///
/// [`Error::NotFound`] when there is no such message or attachment.
pub async fn attachment(app: &App, owner: &str, id: &str, index: u32) -> Result<(String, Vec<u8>)> {
    let row = owned(app, owner, id).await?;
    let index = i32::try_from(index).map_err(|_| Error::NotFound)?;
    app.store
        .attachment_data(&row.id, index)
        .await?
        .ok_or(Error::NotFound)
}

/// Applies a change to messages: read, starred, moved between inbox and trash, deleted from the trash.
///
/// # Errors
///
/// [`Error::NotFound`] for an unknown message, [`Error::Invalid`] for a change that cannot be made.
pub async fn change(app: &App, owner: &str, change: &Change) -> Result<()> {
    open_mailbox(app, owner).await?;
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
    if let Some(target) = &change.move_to
        && !matches!(target.as_str(), "inbox" | "trash")
    {
        return Err(Error::Invalid(
            "messages move to the inbox or the trash".into(),
        ));
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
    if change.delete && rows.iter().any(|r| r.folder != "trash") {
        return Err(Error::Invalid(
            "only what is in the trash is deleted for good".into(),
        ));
    }
    let stamp = now();
    let own = app.address_of(owner);
    for row in &rows {
        if change.delete {
            app.store.message_delete(&row.id, owner).await?;
            continue;
        }
        app.store
            .message_set_flags(&row.id, owner, change.seen, change.flagged)
            .await?;
        if let Some(target) = &change.move_to {
            // Taking something out of the trash puts it back where it came from: a copy of what the person sent belongs in
            // "sent", never in the inbox.
            let target = if target == "inbox" && row.from_address == own {
                "sent"
            } else {
                target.as_str()
            };
            if target != row.folder {
                app.store
                    .message_move(&row.id, owner, target, stamp)
                    .await?;
            }
        }
    }
    Ok(())
}

/// Addresses that start like `prefix`, for completing a recipient.
///
/// # Errors
///
/// A database failure or a disabled mailbox.
pub async fn people(app: &App, owner: &str, prefix: &str) -> Result<Vec<Contact>> {
    open_mailbox(app, owner).await?;
    let prefix: String = prefix
        .trim()
        .chars()
        .take(64)
        .collect::<String>()
        .to_lowercase();
    let found = app.store.mailbox_search(&prefix, MAX_PEOPLE).await?;
    Ok(found
        .into_iter()
        .map(|address| Contact {
            name: String::new(),
            address,
        })
        .collect())
}
