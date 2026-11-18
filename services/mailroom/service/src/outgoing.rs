//! Writing mail: a person's messages from their mailboxes, and the estate's from the services.

use crate::app::App;
use crate::error::{Error, Result, new_id, now};
use crate::store::outbox::OutboxRow;
use crate::store::uploads::UploadRow;
use mail_client::compose::{self, Attachment, Draft};
use mailroom_core::{Compose, Queued, SendRequest, Upload};

/// Largest file uploaded for a message; most bytes of uploads a person may have waiting.
pub const MAX_UPLOAD: usize = 20 * 1024 * 1024;
pub const MAX_UPLOADS_PER_USER: i64 = 60 * 1024 * 1024;
/// Most recipients of one service message.
pub const MAX_SERVICE_RECIPIENTS: usize = 20;
/// A person's messages per hour.
const USER_PER_HOUR: u32 = 60;
const MAX_TEXT: usize = 1024 * 1024;
const MAX_HTML: usize = 2 * 1024 * 1024;

fn compose_error(e: &compose::ComposeError) -> Error {
    Error::Invalid(e.to_string())
}

/// Keeps a file a person chose to attach.
///
/// # Errors
///
/// [`Error::Invalid`] for an empty or oversized file or a full upload quota.
pub async fn upload(
    app: &App,
    owner: &str,
    filename: &str,
    media_type: &str,
    data: Vec<u8>,
) -> Result<Upload> {
    if data.is_empty() {
        return Err(Error::Invalid("the file is empty".into()));
    }
    if data.len() > MAX_UPLOAD {
        return Err(Error::Invalid(
            "the file is too large (20 MB at most)".into(),
        ));
    }
    app.limits
        .hit(&format!("upload:{owner}"), 60, 3600, now())
        .map_err(Error::Limited)?;
    let filename = mail_client::entity::clean_filename(filename);
    let row = UploadRow {
        id: new_id(),
        owner: owner.to_string(),
        filename: filename.clone(),
        media_type: media_type.chars().take(100).collect(),
        size: i32::try_from(data.len()).unwrap_or(i32::MAX),
        data,
        created: now(),
    };
    if !app.store.upload_insert(&row, MAX_UPLOADS_PER_USER).await? {
        return Err(Error::Invalid(
            "too many files are waiting to be sent; send or remove some".into(),
        ));
    }
    Ok(Upload {
        id: row.id,
        filename,
        size: u32::try_from(row.size).unwrap_or(0),
    })
}

/// Queues a person's message from one of their mailboxes.
///
/// # Errors
///
/// [`Error::Invalid`] for a bad draft, [`Error::Limited`] when sending too much, [`Error::NotFound`] for an unknown mailbox.
pub async fn send_user(app: &App, owner: &str, request: &Compose) -> Result<Queued> {
    app.limits
        .hit(&format!("send:{owner}"), USER_PER_HOUR, 3600, now())
        .map_err(Error::Limited)?;
    let account = app
        .store
        .account_get_owned(&request.account_id, owner)
        .await?
        .ok_or(Error::NotFound)?;
    if account.disabled {
        return Err(Error::Forbidden);
    }
    if request.text.len() > MAX_TEXT {
        return Err(Error::Invalid("the message is too long".into()));
    }
    let uploads = app.store.uploads_owned(owner, &request.attachments).await?;
    if uploads.len() != request.attachments.len() {
        return Err(Error::Invalid(
            "an attachment is no longer available; attach it again".into(),
        ));
    }
    let mut draft = Draft {
        from: account.address.clone(),
        to: request.to.clone(),
        cc: request.cc.clone(),
        bcc: request.bcc.clone(),
        subject: request.subject.clone(),
        text: Some(with_signature(&request.text, &account.signature)),
        attachments: uploads
            .iter()
            .map(|u| Attachment {
                filename: u.filename.clone(),
                media_type: u.media_type.clone(),
                data: u.data.clone(),
            })
            .collect(),
        ..Draft::default()
    };
    let reply_to_message = if let Some(reply_id) = &request.in_reply_to {
        let original = app
            .store
            .message_get_owned(reply_id, owner)
            .await?
            .ok_or(Error::NotFound)?;
        // A Message-ID with spaces or line breaks (some senders write them) cannot be threaded on; the reply still goes.
        if well_formed_message_id(&original.message_id) {
            draft.in_reply_to = Some(original.message_id.clone());
            draft.references = vec![original.message_id];
        }
        Some(original.id)
    } else {
        None
    };
    let built = compose::build(&draft, now()).map_err(|e| compose_error(&e))?;
    let recipients: Vec<String> = built.envelope.to.iter().map(ToString::to_string).collect();
    let row = OutboxRow {
        id: new_id(),
        kind: "user".into(),
        source: owner.to_string(),
        account_id: Some(account.id.clone()),
        envelope_from: built.envelope.from.to_string(),
        recipients: quench_serde::json::to_string(&recipients),
        subject: request.subject.chars().take(200).collect(),
        raw: Some(built.raw),
        status: "queued".into(),
        attempts: 0,
        last_error: None,
        created: now(),
        next_attempt: Some(now()),
        sent: None,
        idem_key: None,
        reply_to_message,
    };
    app.store.outbox_insert(&row).await?;
    app.wake.notify_one();
    app.store
        .uploads_delete(owner, &request.attachments)
        .await?;
    Ok(Queued {
        id: row.id,
        status: "queued".into(),
    })
}

/// `<local@domain>`: one token, no spaces, no control characters.
#[must_use]
pub fn well_formed_message_id(id: &str) -> bool {
    let id = id.trim();
    id.len() > 2
        && id.len() <= 998
        && id.starts_with('<')
        && id.ends_with('>')
        && !id.chars().any(|c| c.is_whitespace() || c.is_control())
        && id.contains('@')
}

fn with_signature(text: &str, signature: &str) -> String {
    if signature.trim().is_empty() {
        text.to_string()
    } else {
        format!("{text}\r\n\r\n-- \r\n{signature}")
    }
}

/// Queues a message from a service, sent through the estate's transport.
///
/// # Errors
///
/// [`Error::Invalid`] for a bad request or an unknown sender identity, [`Error::Limited`] over the hourly or daily budget,
/// [`Error::Upstream`] when no sending server is configured.
pub async fn send_service(app: &App, source: &str, request: &SendRequest) -> Result<Queued> {
    let settings = app.settings();
    if settings.transport.is_none() {
        return Err(Error::Upstream("no sending server is configured".into()));
    }
    let identity = settings
        .identity(&request.from)
        .ok_or_else(|| Error::Invalid("that sender address is not allowed".into()))?
        .clone();
    if request.to.is_empty() || request.to.len() > MAX_SERVICE_RECIPIENTS {
        return Err(Error::Invalid(format!(
            "give 1 to {MAX_SERVICE_RECIPIENTS} recipients"
        )));
    }
    if request.text.len() > MAX_TEXT || request.html.as_ref().is_some_and(|h| h.len() > MAX_HTML) {
        return Err(Error::Invalid("the message is too long".into()));
    }
    let key = request
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    if let Some(key) = key {
        if key.len() > 128 || key.chars().any(char::is_control) {
            return Err(Error::Invalid("the idempotency key is not valid".into()));
        }
        // A retry of something already queued is answered with what was queued, and counts for nothing.
        if let Some(id) = app.store.outbox_by_key(source, key).await? {
            return Ok(Queued {
                id,
                status: "duplicate".into(),
            });
        }
    }
    let now = now();
    app.limits
        .hit(
            &format!("service:{source}"),
            settings.hourly_per_source,
            3600,
            now,
        )
        .map_err(Error::Limited)?;
    if app
        .store
        .outbox_count_since("", "service", now - 86_400)
        .await?
        >= i64::from(settings.daily_budget)
    {
        return Err(Error::Limited(3600));
    }

    let from = if identity.name.is_empty() {
        identity.address.clone()
    } else {
        format!(
            "\"{}\" <{}>",
            identity.name.replace(['\\', '"', '\r', '\n'], ""),
            identity.address
        )
    };
    // The estate's own tags are not the caller's to set.
    let mut headers: Vec<(String, String)> = request
        .headers
        .iter()
        .filter(|(n, _)| !n.to_ascii_lowercase().starts_with("x-mailroom-"))
        .cloned()
        .collect();
    if !headers
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case("auto-submitted"))
    {
        headers.push(("Auto-Submitted".into(), "auto-generated".into()));
    }
    headers.push((
        "X-Mailroom-Source".into(),
        source
            .chars()
            .filter(char::is_ascii_graphic)
            .take(64)
            .collect(),
    ));
    let draft = Draft {
        from,
        to: request.to.clone(),
        subject: request.subject.clone(),
        text: Some(request.text.clone()),
        html: request.html.clone(),
        reply_to: request.reply_to.clone().filter(|r| !r.trim().is_empty()),
        headers,
        ..Draft::default()
    };
    let built = compose::build(&draft, now).map_err(|e| compose_error(&e))?;
    let recipients: Vec<String> = built.envelope.to.iter().map(ToString::to_string).collect();
    let row = OutboxRow {
        id: new_id(),
        kind: "service".into(),
        source: source.to_string(),
        account_id: None,
        envelope_from: built.envelope.from.to_string(),
        recipients: quench_serde::json::to_string(&recipients),
        subject: request.subject.chars().take(200).collect(),
        raw: Some(built.raw),
        status: "queued".into(),
        attempts: 0,
        last_error: None,
        created: now,
        next_attempt: Some(now),
        sent: None,
        idem_key: key.map(str::to_string),
        reply_to_message: None,
    };
    if !app.store.outbox_insert(&row).await? {
        // Two requests with the same key raced.
        let id = app
            .store
            .outbox_by_key(source, key.unwrap_or(""))
            .await?
            .unwrap_or(row.id);
        return Ok(Queued {
            id,
            status: "duplicate".into(),
        });
    }
    app.wake.notify_one();
    Ok(Queued {
        id: row.id,
        status: "queued".into(),
    })
}
