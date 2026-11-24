//! `/api/v1/me`, `/folders`, `/people` and `/messages`: who is asking, the mailbox, one message, its safe HTML, its attachments,
//! changes, and sending.

use super::{Caller, PrefersDark, created, done, reply};
use crate::app::App;
use crate::drafts;
use crate::error::Error;
use crate::mailbox::{self, ListSpec};
use crate::outgoing;
use mailroom_core::{Change, Compose, DraftInput, Signature};
use quench_http::multipart::Multipart;
use quench_http::prelude::{
    Inject, Json, Limited, Path, Query, Response, delete, get, http::StatusCode, post, put,
};
use quench_serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub unread: Option<String>,
    #[serde(default)]
    pub flagged: Option<String>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
}

fn truthy(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true" | "yes"))
}

#[get("/api/v1/me")]
pub async fn me(Inject(app): Inject<App>, caller: Caller) -> Response {
    reply(mailbox::me(&app, &caller.name, caller.write, caller.admin).await)
}

#[get("/api/v1/folders")]
pub async fn folders(Inject(app): Inject<App>, caller: Caller) -> Response {
    reply(mailbox::folders(&app, &caller.name).await)
}

#[derive(Debug, Deserialize)]
pub struct PeopleQuery {
    #[serde(default)]
    pub q: Option<String>,
}

/// Addresses that start like what is being typed, to complete a recipient.
#[get("/api/v1/people")]
pub async fn people(
    Inject(app): Inject<App>,
    caller: Caller,
    Query(q): Query<PeopleQuery>,
) -> Response {
    reply(mailbox::people(&app, &caller.name, &q.q.unwrap_or_default()).await)
}

#[get("/api/v1/messages")]
pub async fn list(
    Inject(app): Inject<App>,
    caller: Caller,
    Query(q): Query<ListQuery>,
) -> Response {
    let spec = ListSpec {
        folder: q.folder.unwrap_or_default(),
        text: q.q.unwrap_or_default(),
        unread: truthy(q.unread.as_deref()),
        flagged: truthy(q.flagged.as_deref()),
        offset: q.offset.unwrap_or(0),
        limit: q.limit.unwrap_or(50),
    };
    reply(mailbox::list(&app, &caller.name, &spec).await)
}

#[post("/api/v1/message-changes")]
pub async fn change(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(body)): Limited<Json<Change>, { super::JSON_LIMIT }>,
) -> Response {
    done(mailbox::change(&app, &caller.name, &body).await)
}

#[post("/api/v1/outgoing")]
pub async fn send(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(body)): Limited<Json<Compose>, { super::JSON_LIMIT }>,
) -> Response {
    created(outgoing::send_user(&app, &caller.name, &body).await)
}

#[get("/api/v1/messages/{id}")]
pub async fn open(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    reply(mailbox::open(&app, &caller.name, &id).await)
}

#[derive(Debug, Deserialize)]
pub struct HtmlQuery {
    #[serde(default)]
    pub remote: Option<String>,
    /// `dark` or `light`: the look to draw the message in, over the person's theme.
    #[serde(default)]
    pub theme: Option<String>,
}

/// The sanitised HTML of a message, for a sandboxed frame: its own content-security-policy, never cached.
#[get("/api/v1/messages/{id}/html")]
pub async fn html(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
    Query(q): Query<HtmlQuery>,
    PrefersDark(prefers_dark): PrefersDark,
) -> Response {
    let remote = truthy(q.remote.as_deref());
    let dark = match q.theme.as_deref() {
        Some("dark") => true,
        Some("light") => false,
        _ => prefers_dark,
    };
    match mailbox::html(&app, &caller.name, &id, remote, dark).await {
        Ok(document) => Response::html(StatusCode::OK, document)
            .header("content-security-policy", mailbox::html_policy(remote))
            .header("cache-control", "private, no-store")
            .header("referrer-policy", "no-referrer")
            .header("x-content-type-options", "nosniff"),
        Err(e) => e.response(),
    }
}

/// `filename="..."` made of safe ASCII, and `filename*=` carrying the real name.
#[must_use]
pub fn content_disposition(filename: &str) -> String {
    let ascii: String = filename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let encoded: String = filename
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

/// An attachment, always as a download: whatever its type says, it is never shown by the browser.
#[get("/api/v1/messages/{id}/attachments/{index}")]
pub async fn attachment(
    Inject(app): Inject<App>,
    caller: Caller,
    Path((id, index)): Path<(String, u32)>,
) -> Response {
    match mailbox::attachment(&app, &caller.name, &id, index).await {
        Ok((filename, data)) => {
            Response::from_bytes(StatusCode::OK, quench_core::bytes::Bytes::from(data))
                .header("content-type", "application/octet-stream")
                .header("content-disposition", content_disposition(&filename))
                .header("content-security-policy", "default-src 'none'; sandbox")
                .header("x-content-type-options", "nosniff")
                .header("cache-control", "private, no-store")
        }
        Err(e) => e.response(),
    }
}

/// A file to attach, sent by the page's upload control as `multipart/form-data` (the file in the field `file`).
#[post("/api/v1/uploads")]
pub async fn upload(Inject(app): Inject<App>, caller: Caller, mut form: Multipart) -> Response {
    let mut file: Option<(String, String, Vec<u8>)> = None;
    loop {
        match form.next_field().await {
            Ok(Some(field)) if field.name() == Some("file") => {
                let name = field.file_name().unwrap_or("attachment").to_string();
                let media_type = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                match field.bytes().await {
                    Ok(bytes) => file = Some((name, media_type, bytes.to_vec())),
                    Err(_) => {
                        return Error::Invalid("the file could not be read (too large?)".into())
                            .response();
                    }
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(_) => return Error::Invalid("that is not an upload".into()).response(),
        }
    }
    let Some((name, media_type, data)) = file else {
        return Error::Invalid("no file in the upload".into()).response();
    };
    created(outgoing::upload(&app, &caller.name, &name, &media_type, data).await)
}

#[delete("/api/v1/uploads/{id}")]
pub async fn remove_upload(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
) -> Response {
    done(
        async {
            crate::mailbox::open_mailbox(&app, &caller.name).await?;
            if app.store.upload_delete_one(&caller.name, &id).await? {
                Ok(())
            } else {
                Err(Error::NotFound)
            }
        }
        .await,
    )
}

#[get("/api/v1/drafts")]
pub async fn drafts_list(Inject(app): Inject<App>, caller: Caller) -> Response {
    reply(drafts::list(&app, &caller.name).await)
}

#[post("/api/v1/drafts")]
pub async fn draft_create(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(body)): Limited<Json<DraftInput>, { super::JSON_LIMIT }>,
) -> Response {
    created(drafts::save(&app, &caller.name, None, &body).await)
}

#[get("/api/v1/drafts/{id}")]
pub async fn draft_open(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
) -> Response {
    reply(drafts::get(&app, &caller.name, &id).await)
}

#[put("/api/v1/drafts/{id}")]
pub async fn draft_save(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
    Limited(Json(body)): Limited<Json<DraftInput>, { super::JSON_LIMIT }>,
) -> Response {
    reply(drafts::save(&app, &caller.name, Some(&id), &body).await)
}

#[delete("/api/v1/drafts/{id}")]
pub async fn draft_delete(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(id): Path<String>,
) -> Response {
    done(drafts::delete(&app, &caller.name, &id).await)
}

#[get("/api/v1/mutes")]
pub async fn mutes_list(Inject(app): Inject<App>, caller: Caller) -> Response {
    reply(drafts::mutes(&app, &caller.name).await)
}

#[put("/api/v1/mutes/{address}")]
pub async fn mute_add(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(address): Path<String>,
) -> Response {
    done(drafts::mute(&app, &caller.name, &address).await)
}

#[delete("/api/v1/mutes/{address}")]
pub async fn mute_remove(
    Inject(app): Inject<App>,
    caller: Caller,
    Path(address): Path<String>,
) -> Response {
    done(drafts::unmute(&app, &caller.name, &address).await)
}

/// The other messages of the conversation a message belongs to.
#[get("/api/v1/messages/{id}/thread")]
pub async fn thread(Inject(app): Inject<App>, caller: Caller, Path(id): Path<String>) -> Response {
    reply(mailbox::thread(&app, &caller.name, &id).await)
}

#[put("/api/v1/signature")]
pub async fn signature_save(
    Inject(app): Inject<App>,
    caller: Caller,
    Limited(Json(body)): Limited<Json<Signature>, { super::JSON_LIMIT }>,
) -> Response {
    reply(
        drafts::set_signature(&app, &caller.name, &body.text)
            .await
            .map(|text| Signature { text }),
    )
}

pub fn register_routes() {
    let _ = signature_save as fn(_, _, _) -> _;
    let _ = thread as fn(_, _, _) -> _;
    let _ = drafts_list as fn(_, _) -> _;
    let _ = draft_create as fn(_, _, _) -> _;
    let _ = draft_open as fn(_, _, _) -> _;
    let _ = draft_save as fn(_, _, _, _) -> _;
    let _ = draft_delete as fn(_, _, _) -> _;
    let _ = mutes_list as fn(_, _) -> _;
    let _ = mute_add as fn(_, _, _) -> _;
    let _ = mute_remove as fn(_, _, _) -> _;
    let _ = me as fn(_, _) -> _;
    let _ = folders as fn(_, _) -> _;
    let _ = people as fn(_, _, _) -> _;
    let _ = list as fn(_, _, _) -> _;
    let _ = change as fn(_, _, _) -> _;
    let _ = send as fn(_, _, _) -> _;
    let _ = open as fn(_, _, _) -> _;
    let _ = html as fn(_, _, _, _, _) -> _;
    let _ = attachment as fn(_, _, _) -> _;
    let _ = upload as fn(_, _, _) -> _;
    let _ = remove_upload as fn(_, _, _) -> _;
}
