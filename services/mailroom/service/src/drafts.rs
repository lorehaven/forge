//! Drafts (a message started and not sent) and muted senders (whose mail goes straight to the archive).

use crate::app::App;
use crate::error::{Error, Result, new_id, now};
use crate::mailbox::open_mailbox;
use crate::store::drafts::DraftRow;
use mailroom_core::{Draft, DraftInput};

/// How many drafts one person keeps.
pub const MAX_DRAFTS: i64 = 100;
/// How much one draft may hold: a field is a line or two, the text is a letter.
pub const MAX_FIELD: usize = 2_000;
pub const MAX_TEXT: usize = 200_000;
/// How many senders one person mutes.
pub const MAX_MUTES: i64 = 200;

fn model(row: &DraftRow) -> Draft {
    Draft {
        id: row.id.clone(),
        to: row.to_text.clone(),
        cc: row.cc_text.clone(),
        bcc: row.bcc_text.clone(),
        subject: row.subject.clone(),
        text: row.text.clone(),
        in_reply_to: row.in_reply_to.clone(),
        updated: row.updated,
    }
}

/// The person's drafts, the latest first.
///
/// # Errors
///
/// A database failure or a disabled mailbox.
pub async fn list(app: &App, owner: &str) -> Result<Vec<Draft>> {
    open_mailbox(app, owner).await?;
    Ok(app
        .store
        .drafts_of(owner)
        .await?
        .iter()
        .map(model)
        .collect())
}

/// One draft.
///
/// # Errors
///
/// [`Error::NotFound`] when it is not the person's.
pub async fn get(app: &App, owner: &str, id: &str) -> Result<Draft> {
    open_mailbox(app, owner).await?;
    app.store
        .draft_get(id, owner)
        .await?
        .map(|row| model(&row))
        .ok_or(Error::NotFound)
}

/// Keeps what was typed: a new draft when `id` is `None`, else the person's draft of that id.
///
/// # Errors
///
/// [`Error::Invalid`] for something too long or too many drafts, [`Error::NotFound`] for another person's draft.
pub async fn save(app: &App, owner: &str, id: Option<&str>, input: &DraftInput) -> Result<Draft> {
    open_mailbox(app, owner).await?;
    let fields = [&input.to, &input.cc, &input.bcc, &input.subject];
    if fields.iter().any(|f| f.len() > MAX_FIELD) || input.text.len() > MAX_TEXT {
        return Err(Error::Invalid("this draft is too long to keep".into()));
    }
    let existing = match id {
        Some(id) => Some(
            app.store
                .draft_get(id, owner)
                .await?
                .ok_or(Error::NotFound)?,
        ),
        None => None,
    };
    if existing.is_none() && app.store.draft_count(owner).await? >= MAX_DRAFTS {
        return Err(Error::Invalid(format!(
            "you keep at most {MAX_DRAFTS} drafts: delete some first"
        )));
    }
    let row = DraftRow {
        id: existing.map_or_else(new_id, |d| d.id),
        owner: owner.to_string(),
        to_text: input.to.clone(),
        cc_text: input.cc.clone(),
        bcc_text: input.bcc.clone(),
        subject: input.subject.replace(['\r', '\n'], " "),
        text: input.text.clone(),
        in_reply_to: input.in_reply_to.clone().filter(|r| !r.is_empty()),
        updated: now(),
    };
    if !app.store.draft_put(&row).await? {
        return Err(Error::NotFound);
    }
    Ok(model(&row))
}

/// Throws a draft away. Not finding it is not an error: the aim is that it is gone.
///
/// # Errors
///
/// A database failure or a disabled mailbox.
pub async fn delete(app: &App, owner: &str, id: &str) -> Result<()> {
    open_mailbox(app, owner).await?;
    app.store.draft_delete(id, owner).await?;
    Ok(())
}

/// The addresses whose mail the person does not want in the inbox.
///
/// # Errors
///
/// A database failure or a disabled mailbox.
pub async fn mutes(app: &App, owner: &str) -> Result<Vec<String>> {
    open_mailbox(app, owner).await?;
    app.store.mutes_of(owner).await
}

/// Mutes a sender. Their mail from now on goes to the archive, read.
///
/// # Errors
///
/// [`Error::Invalid`] for something that is not an address or too many muted senders.
pub async fn mute(app: &App, owner: &str, address: &str) -> Result<()> {
    open_mailbox(app, owner).await?;
    let address = address.trim().to_ascii_lowercase();
    if address.is_empty() || address.len() > 254 || address.contains(char::is_whitespace) {
        return Err(Error::Invalid("not an address".into()));
    }
    if !app
        .store
        .mute_add(owner, &address, now(), MAX_MUTES)
        .await?
    {
        return Err(Error::Invalid(format!(
            "you mute at most {MAX_MUTES} senders"
        )));
    }
    Ok(())
}

/// Lets a sender's mail into the inbox again.
///
/// # Errors
///
/// A database failure or a disabled mailbox.
pub async fn unmute(app: &App, owner: &str, address: &str) -> Result<()> {
    open_mailbox(app, owner).await?;
    app.store
        .mute_remove(owner, &address.trim().to_ascii_lowercase())
        .await
}

/// How long a signature may be.
pub const MAX_SIGNATURE: usize = 2_000;

/// What the person has added to the end of the messages they start (empty: nothing).
///
/// # Errors
///
/// A database failure or a disabled mailbox.
pub async fn signature(app: &App, owner: &str) -> Result<String> {
    open_mailbox(app, owner).await?;
    app.store.signature_get(owner).await
}

/// Keeps the signature.
///
/// # Errors
///
/// [`Error::Invalid`] for one that is too long.
pub async fn set_signature(app: &App, owner: &str, text: &str) -> Result<String> {
    open_mailbox(app, owner).await?;
    let text = text.trim_end().to_string();
    if text.chars().count() > MAX_SIGNATURE {
        return Err(Error::Invalid(format!(
            "a signature is at most {MAX_SIGNATURE} characters"
        )));
    }
    app.store.signature_put(owner, &text).await?;
    Ok(text)
}
