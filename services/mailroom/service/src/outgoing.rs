//! Sending: a person writes to other people, a service writes to people. Both end in [`delivery::deliver`].

use crate::app::App;
use crate::delivery::{self, Author, File, Letter};
use crate::error::{Error, Result, new_id, now};
use crate::mailbox::open_mailbox;
use crate::store::deliveries::DeliveryRow;
use crate::store::uploads::UploadRow;
use mailroom_core::{Compose, SendRequest, Sent, Upload, draft};

/// Largest file uploaded for a message; most bytes of uploads a person may have waiting.
pub const MAX_UPLOAD: usize = 20 * 1024 * 1024;
const MAX_UPLOADS_PER_USER: i64 = 60 * 1024 * 1024;

/// A name that is safe to show and to put in a download header: no path, no control characters, not too long.
#[must_use]
pub fn clean_filename(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '<' | '>' | ':' | '|' | '?' | '*'))
        .take(120)
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "attachment".into()
    } else {
        cleaned
    }
}

/// `type/subtype` or `application/octet-stream`.
#[must_use]
pub fn clean_media_type(media_type: &str) -> String {
    let t = media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let ok = t.len() <= 100
        && t.split_once('/').is_some_and(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '+' | '.' | '_'))
        });
    if ok {
        t
    } else {
        "application/octet-stream".into()
    }
}

/// Stores a file the person will attach.
///
/// # Errors
///
/// [`Error::Invalid`] for an empty or too large file or too many waiting files, [`Error::Limited`] for too many uploads.
pub async fn upload(
    app: &App,
    owner: &str,
    filename: &str,
    media_type: &str,
    data: Vec<u8>,
) -> Result<Upload> {
    open_mailbox(app, owner).await?;
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
    let row = UploadRow {
        id: new_id(),
        owner: owner.to_string(),
        filename: clean_filename(filename),
        media_type: clean_media_type(media_type),
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
        filename: row.filename,
        size: u32::try_from(row.size).unwrap_or(0),
    })
}

/// The user names in a list of recipients a **person** typed, each at this estate's domain (or without one), lower-cased: typing
/// `Ann` reaches `ann`.
fn names(app: &App, entries: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for entry in entries {
        let name = draft::recipient_name(entry).ok_or_else(|| {
            Error::Invalid(format!("{} is not a user name or address", shorten(entry)))
        })?;
        let inner = entry
            .rsplit_once('<')
            .and_then(|(_, rest)| rest.strip_suffix('>'))
            .unwrap_or(entry)
            .trim();
        if let Some((_, domain)) = inner.split_once('@')
            && !domain.eq_ignore_ascii_case(&app.config.domain)
        {
            return Err(Error::Invalid(format!(
                "mail stays inside this estate: write to name@{}, not {}",
                app.config.domain,
                shorten(inner)
            )));
        }
        if !out.contains(&name) {
            out.push(name);
        }
    }
    Ok(out)
}

/// The user names in a **service's** request: exactly as Gatehouse spells them (user names there are case-sensitive and may
/// hold `@` or `+`; such a name has no mailbox here, so it is refused rather than turned into somebody else's).
fn service_names(entries: &[String]) -> Result<Vec<String>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for entry in entries {
        if !draft::local_part_ok(entry) || *entry != entry.to_ascii_lowercase() {
            return Err(Error::Invalid(format!(
                "{} cannot have a mailbox here (lower-case letters, digits, dots, hyphens and underscores only)",
                shorten(entry)
            )));
        }
        if seen.insert(entry.clone()) {
            out.push(entry.clone());
        }
    }
    Ok(out)
}

fn shorten(text: &str) -> String {
    text.chars().take(60).collect()
}

/// A person sends a message.
///
/// # Errors
///
/// [`Error::Invalid`] for a bad message or a recipient with no mailbox (nothing is sent then), [`Error::Limited`] for too
/// many sent in an hour, [`Error::NotFound`] for a reply to or an attachment that is not theirs.
pub async fn send_user(app: &App, owner: &str, compose: &Compose) -> Result<Sent> {
    let mailbox = open_mailbox(app, owner).await?;
    // Cheap limits first: nothing below should have to chew through a huge request.
    let listed = compose.to.len() + compose.cc.len() + compose.bcc.len();
    if listed == 0
        || listed > delivery::MAX_RECIPIENTS
        || compose.attachments.len() > delivery::MAX_FILES
    {
        return Err(Error::Invalid(format!(
            "write to 1 to {} people, with at most {} attachments",
            delivery::MAX_RECIPIENTS,
            delivery::MAX_FILES
        )));
    }
    let letter_to = names(app, &compose.to)?;
    let letter_cc = names(app, &compose.cc)?;
    let letter_bcc = names(app, &compose.bcc)?;
    // A message counts once for every mailbox it goes to: fifty recipients are fifty deliveries.
    let mut everyone: Vec<&String> = letter_to
        .iter()
        .chain(&letter_cc)
        .chain(&letter_bcc)
        .collect();
    everyone.sort_unstable();
    everyone.dedup();
    let per_hour = app.settings().hourly_per_person;
    app.limits
        .hit_n(
            &format!("send:{owner}"),
            u32::try_from(everyone.len()).unwrap_or(u32::MAX),
            per_hour,
            3600,
            now(),
        )
        .map_err(Error::Limited)?;
    let original = match compose.in_reply_to.as_deref().filter(|r| !r.is_empty()) {
        Some(reply_id) => Some(
            app.store
                .message_get_owned(reply_id, owner)
                .await?
                .ok_or(Error::NotFound)?,
        ),
        None => None,
    };
    let in_reply_to = original
        .as_ref()
        .map(|row| row.message_id.clone())
        .unwrap_or_default();
    // The size of what is attached is known before any of it is read.
    if app.store.uploads_size(owner, &compose.attachments).await?
        > i64::try_from(delivery::MAX_ATTACHMENTS).unwrap_or(i64::MAX)
    {
        return Err(Error::Invalid("too many or too large attachments".into()));
    }
    let uploads = app.store.uploads_owned(owner, &compose.attachments).await?;
    if uploads.len() != compose.attachments.len() {
        return Err(Error::NotFound);
    }
    let letter = Letter {
        to: letter_to,
        cc: letter_cc,
        bcc: letter_bcc,
        subject: compose.subject.trim().to_string(),
        text: compose.text.clone(),
        html: None,
        unsubscribe: String::new(),
        in_reply_to,
        files: uploads
            .iter()
            .map(|u| File {
                filename: u.filename.clone(),
                media_type: u.media_type.clone(),
                data: u.data.clone(),
            })
            .collect(),
    };
    let author = Author {
        name: String::new(),
        address: mailbox.address,
        source: String::new(),
        owner: Some(owner.to_string()),
    };
    let outcome = delivery::deliver(app, &author, &letter).await?;
    app.store
        .uploads_delete(owner, &compose.attachments)
        .await?;
    if let Some(original) = original {
        app.store.message_mark_answered(&original.id, owner).await?;
    }
    // What was written is sent: the draft it was written in goes.
    if let Some(draft) = compose.draft.as_deref().filter(|d| !d.is_empty()) {
        app.store.draft_delete(draft, owner).await?;
    }
    Ok(Sent {
        id: outcome.sent_id.unwrap_or_default(),
        status: "delivered".into(),
        delivered: outcome.delivered,
    })
}

/// The name a service writes under: its client name without the `-mailroom` suffix (`gatehouse-mailroom` is `gatehouse`).
#[must_use]
pub fn service_name(subject: &str) -> String {
    let lower = subject.trim().to_ascii_lowercase();
    let base = lower.strip_suffix("-mailroom").unwrap_or(&lower);
    if draft::local_part_ok(base) {
        base.to_string()
    } else {
        "service".to_string()
    }
}

/// The display name of a service: its name with a capital.
#[must_use]
pub fn display_name(service: &str) -> String {
    let mut chars = service.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

/// A service writes to people.
///
/// # Errors
///
/// [`Error::Invalid`] for a bad request or one nobody could receive, [`Error::Limited`] over the service's or the estate's
/// budget.
pub async fn send_service(app: &App, source: &str, request: &SendRequest) -> Result<Sent> {
    let service = service_name(source);
    let stamp = now();
    let settings = app.settings();
    if request.to.len() > delivery::MAX_RECIPIENTS {
        return Err(Error::Invalid(format!(
            "write to 1 to {} people",
            delivery::MAX_RECIPIENTS
        )));
    }
    let recipients = service_names(&request.to)?;
    if recipients.is_empty() {
        return Err(Error::Invalid(format!(
            "write to 1 to {} people",
            delivery::MAX_RECIPIENTS
        )));
    }
    let key = request
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    if key.is_some_and(|k| k.len() > 128 || k.chars().any(char::is_control)) {
        return Err(Error::Invalid("the idempotency key is too long".into()));
    }
    let kind: String = request
        .kind
        .as_deref()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(40)
        .collect();
    let letter = Letter {
        to: recipients.clone(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: request.subject.trim().to_string(),
        text: request.text.clone(),
        html: request.html.clone(),
        unsubscribe: request.unsubscribe.clone().unwrap_or_default(),
        in_reply_to: String::new(),
        files: Vec::new(),
    };
    delivery::check(&letter)?;
    let weight = u32::try_from(recipients.len()).unwrap_or(u32::MAX);
    app.limits
        .hit_n(
            &format!("source:{service}"),
            weight,
            settings.hourly_per_source,
            3600,
            stamp,
        )
        .map_err(Error::Limited)?;
    let id = new_id();
    let row = DeliveryRow {
        id: id.clone(),
        source: service.clone(),
        kind,
        recipients: recipients.iter().map(|n| app.address_of(n)).collect(),
        subject: letter.subject.chars().take(300).collect(),
        status: "delivering".into(),
        delivered: 0,
        error: None,
        created: stamp,
        idem_key: key.map(str::to_string),
    };
    if !app.store.delivery_begin(&row).await? {
        let (earlier, delivered) = app
            .store
            .delivery_by_key(&service, key.unwrap_or_default())
            .await?
            .unwrap_or_default();
        return Ok(Sent {
            id: earlier,
            status: "duplicate".into(),
            delivered: u32::try_from(delivered).unwrap_or(0),
        });
    }
    // The estate's day is charged only for what is really delivered: a retry that is found out above costs nothing.
    if let Err(seconds) = app
        .limits
        .hit_n("estate", weight, settings.daily_budget, 86_400, stamp)
    {
        app.store
            .delivery_finish(
                &id,
                "failed",
                0,
                Some("the estate's daily budget is used up"),
            )
            .await?;
        app.store.delivery_release_key(&id).await?;
        return Err(Error::Limited(seconds));
    }
    let author = Author {
        name: display_name(&service),
        address: app.address_of(&service),
        source: service,
        owner: None,
    };
    match delivery::deliver(app, &author, &letter).await {
        Ok(outcome) => {
            let status = if outcome.refused.is_empty() {
                "delivered"
            } else if outcome.delivered == 0 {
                "failed"
            } else {
                "partial"
            };
            let why: Vec<String> = outcome
                .refused
                .iter()
                .map(|(n, w)| format!("{n} {w}"))
                .collect();
            let why = (!why.is_empty()).then(|| why.join("; "));
            app.store
                .delivery_finish(
                    &id,
                    status,
                    i32::try_from(outcome.delivered).unwrap_or(0),
                    why.as_deref(),
                )
                .await?;
            if outcome.delivered == 0 {
                return Err(Error::Invalid(format!(
                    "nobody received it: {}",
                    why.unwrap_or_default()
                )));
            }
            Ok(Sent {
                id,
                status: status.into(),
                delivered: outcome.delivered,
            })
        }
        Err(e) => {
            let (_, text) = e.parts();
            app.store
                .delivery_finish(&id, "failed", 0, Some(&text))
                .await?;
            // The attempt failed: the same key may be tried again.
            app.store.delivery_release_key(&id).await?;
            Err(e)
        }
    }
}
