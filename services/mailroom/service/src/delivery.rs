//! Putting a message into mailboxes: the one place mail is written.
//!
//! A message becomes one row in every recipient's `inbox` (and one in the sender's `sent` when a person wrote it), all in one
//! transaction: either everybody gets it or nobody does. The attachment bytes are stored once and shared by those rows.

use crate::app::App;
use crate::error::{Error, Result, new_id, now};
use crate::store::messages::{BodyRow, Copy, MessageRow};
use mailroom_core::{Contact, time::snippet};

/// Most people one message may go to (to, cc and bcc together).
pub const MAX_RECIPIENTS: usize = 50;
/// Largest text, HTML and subject accepted.
pub const MAX_TEXT: usize = 1024 * 1024;
pub const MAX_HTML: usize = 2 * 1024 * 1024;
pub const MAX_SUBJECT: usize = 300;
/// Most bytes of attachments on one message.
pub const MAX_ATTACHMENTS: usize = 20 * 1024 * 1024;
/// Most files on one message.
pub const MAX_FILES: usize = 10;

/// Names a person may not have a mailbox under: the estate's services write under them, and a mailbox of the same name would be
/// indistinguishable from the service's address.
pub const RESERVED: [&str; 23] = [
    "gatehouse",
    "mailroom",
    "conveyor",
    "warehouse",
    "gantry",
    "foundry",
    "sage",
    "switchboard",
    "workbench",
    "daybook",
    "hoist",
    "songbook",
    "klaxon",
    "forklift",
    "gauge",
    "lectern",
    "genevia",
    "postmaster",
    "noreply",
    "no-reply",
    "service",
    "system",
    "root",
];

/// Whether `name` is one of the [`RESERVED`] names.
#[must_use]
pub fn reserved(name: &str) -> bool {
    RESERVED.contains(&name)
}

/// Who wrote it.
#[derive(Debug, Clone)]
pub struct Author {
    pub name: String,
    pub address: String,
    /// The service's name, empty for a person.
    pub source: String,
    /// A person's mailbox (their `sent` folder gets a copy); `None` for a service.
    pub owner: Option<String>,
}

/// A file travelling with a message.
#[derive(Debug, Clone)]
pub struct File {
    pub filename: String,
    pub media_type: String,
    pub data: Vec<u8>,
}

/// What is written.
#[derive(Debug, Clone, Default)]
pub struct Letter {
    /// User names (already reduced: lowercase, valid).
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub unsubscribe: String,
    /// The Message-ID of the message answered.
    pub in_reply_to: String,
    pub files: Vec<File>,
}

/// What happened.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// The id of the copy in the author's `sent` folder, for a person.
    pub sent_id: Option<String>,
    /// How many mailboxes it reached (the author's own copy not counted).
    pub delivered: u32,
    /// Recipients that did not get it, and why.
    pub refused: Vec<(String, String)>,
}

fn contact(app: &App, name: &str) -> Contact {
    Contact {
        name: String::new(),
        address: app.address_of(name),
    }
}

/// Most tags and deepest nesting a message's HTML may have. The sanitiser is thorough rather than fast on pathological input
/// (hundreds of thousands of nested elements take longer than anyone waits), so what a service may send is bounded here.
pub const MAX_HTML_TAGS: usize = 20_000;
pub const MAX_HTML_DEPTH: usize = 128;

/// Elements that have no closing tag.
const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Whether `html` is within [`MAX_HTML_TAGS`] and [`MAX_HTML_DEPTH`]. A rough count, deliberately on the strict side: it is
/// a guard against abuse, not a parser.
#[must_use]
pub fn html_shape_ok(html: &str) -> bool {
    let (mut tags, mut depth) = (0_usize, 0_usize);
    let bytes = html.as_bytes();
    let mut i = 0;
    while let Some(at) = html[i..].find('<') {
        i += at + 1;
        match bytes.get(i) {
            Some(b'/') => depth = depth.saturating_sub(1),
            Some(c) if c.is_ascii_alphabetic() => {
                tags += 1;
                let name: String = html[i..]
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .take(16)
                    .collect::<String>()
                    .to_ascii_lowercase();
                if !VOID.contains(&name.as_str()) {
                    depth += 1;
                }
                if tags > MAX_HTML_TAGS || depth > MAX_HTML_DEPTH {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

/// Checks the sizes and the shape of a letter.
///
/// # Errors
///
/// [`Error::Invalid`] saying what is wrong.
pub fn check(letter: &Letter) -> Result<()> {
    if letter.subject.len() > MAX_SUBJECT || letter.subject.contains(['\r', '\n', '\0']) {
        return Err(Error::Invalid(
            "the subject is one line of at most 300 characters".into(),
        ));
    }
    if letter.text.len() > MAX_TEXT
        || letter.html.as_ref().is_some_and(|h| h.len() > MAX_HTML)
        || letter.text.contains('\0')
        || letter.html.as_ref().is_some_and(|h| h.contains('\0'))
    {
        return Err(Error::Invalid(
            "the message is too large or malformed".into(),
        ));
    }
    if letter.html.as_deref().is_some_and(|h| !html_shape_ok(h)) {
        return Err(Error::Invalid(
            "the HTML has too many elements or is nested too deeply".into(),
        ));
    }
    if letter.text.trim().is_empty() && letter.html.is_none() && letter.files.is_empty() {
        return Err(Error::Invalid("the message is empty".into()));
    }
    if letter.files.len() > MAX_FILES
        || letter.files.iter().map(|f| f.data.len()).sum::<usize>() > MAX_ATTACHMENTS
    {
        return Err(Error::Invalid("too many or too large attachments".into()));
    }
    if letter.unsubscribe.len() > 2000
        || letter
            .unsubscribe
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(Error::Invalid("the unsubscribe link is not valid".into()));
    }
    Ok(())
}

/// Delivers `letter`. A person's mail goes only to existing mailboxes with room, and is refused as a whole otherwise; a
/// service's mail makes the mailbox of a person it names when there is none yet (the person has simply not signed in), and
/// reaches whoever it can.
///
/// # Errors
///
/// [`Error::Invalid`] for a bad letter or (from a person) an unknown or full mailbox; a database failure.
pub async fn deliver(app: &App, author: &Author, letter: &Letter) -> Result<Outcome> {
    check(letter)?;
    let mut names: Vec<&str> = Vec::new();
    for name in letter.to.iter().chain(&letter.cc).chain(&letter.bcc) {
        if !names.contains(&name.as_str()) {
            names.push(name);
        }
    }
    if names.is_empty() || names.len() > MAX_RECIPIENTS {
        return Err(Error::Invalid(format!(
            "write to 1 to {MAX_RECIPIENTS} people"
        )));
    }
    let stamp = now();
    let attached: usize = letter.files.iter().map(|f| f.data.len()).sum();
    let size = letter.text.len() + letter.html.as_ref().map_or(0, String::len) + attached;
    let size = i32::try_from(size).unwrap_or(i32::MAX);
    let quota = app.settings().quota_bytes();

    // The author's own copy counts too: "sent" is not a way round the size of a mailbox.
    if let Some(owner) = &author.owner
        && app.store.mailbox_bytes(owner).await? + i64::from(size) > quota
    {
        return Err(Error::Invalid(
            "your mailbox is full: delete something first".into(),
        ));
    }
    let mut targets = Vec::new();
    let mut refused: Vec<(String, String)> = Vec::new();
    for name in &names {
        if reserved(name) {
            refused.push(((*name).to_string(), "has no mailbox here".into()));
            continue;
        }
        let mailbox = if author.owner.is_some() {
            app.store.mailbox_get(name).await?
        } else {
            Some(
                app.store
                    .mailbox_ensure(name, &app.address_of(name), stamp)
                    .await?,
            )
        };
        match mailbox {
            None => refused.push(((*name).to_string(), "has no mailbox here".into())),
            Some(mb) => {
                if app.store.mailbox_bytes(&mb.owner).await? + i64::from(size) > quota {
                    refused.push(((*name).to_string(), "mailbox is full".into()));
                } else {
                    targets.push(mb.owner);
                }
            }
        }
    }
    if author.owner.is_some() && !refused.is_empty() {
        let who: Vec<String> = refused
            .iter()
            .map(|(name, why)| format!("{} {why}", app.address_of(name)))
            .collect();
        return Err(Error::Invalid(format!("not sent: {}", who.join("; "))));
    }

    let message_id = format!("<{}@{}>", new_id(), app.config.domain);
    let to: Vec<Contact> = letter.to.iter().map(|n| contact(app, n)).collect();
    let cc: Vec<Contact> = letter.cc.iter().map(|n| contact(app, n)).collect();
    let from = Contact {
        name: author.name.clone(),
        address: author.address.clone(),
    };
    let body_text = if letter.text.trim().is_empty() {
        letter
            .html
            .as_deref()
            .map(mail_html::sanitize::to_text)
            .unwrap_or_default()
    } else {
        letter.text.clone()
    };
    let search: String = format!(
        "{} {} {} {} {}",
        letter.subject,
        from.full(),
        to.iter()
            .chain(&cc)
            .map(|c| c.address.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        letter
            .files
            .iter()
            .map(|f| f.filename.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        body_text.chars().take(2000).collect::<String>()
    )
    .to_lowercase();
    let body = BodyRow {
        text: body_text.clone(),
        html: letter.html.clone().filter(|h| !h.trim().is_empty()),
    };
    let blobs: Vec<(String, Vec<u8>)> = letter
        .files
        .iter()
        .map(|f| (new_id(), f.data.clone()))
        .collect();
    let attachments: Vec<(String, String, i32, String)> = letter
        .files
        .iter()
        .zip(&blobs)
        .map(|(f, (blob_id, _))| {
            (
                f.filename.clone(),
                f.media_type.clone(),
                i32::try_from(f.data.len()).unwrap_or(i32::MAX),
                blob_id.clone(),
            )
        })
        .collect();

    let row_for = |owner: &str, folder: &str, seen: bool| MessageRow {
        id: new_id(),
        owner: owner.to_string(),
        folder: folder.to_string(),
        message_id: message_id.clone(),
        in_reply_to: letter.in_reply_to.clone(),
        from_name: from.name.clone(),
        from_address: from.address.clone(),
        source: author.source.clone(),
        to_json: quench_serde::json::to_string(&to),
        cc_json: quench_serde::json::to_string(&cc),
        subject: letter.subject.clone(),
        unsubscribe: letter.unsubscribe.clone(),
        date: stamp,
        moved: stamp,
        preview: snippet(&body_text, 140),
        seen,
        flagged: false,
        answered: false,
        attachments: !letter.files.is_empty(),
        size,
        search: search.clone(),
    };
    let mut rows: Vec<MessageRow> = targets.iter().map(|o| row_for(o, "inbox", false)).collect();
    let delivered = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    let mut sent_id = None;
    if let Some(owner) = &author.owner {
        let row = row_for(owner, "sent", true);
        sent_id = Some(row.id.clone());
        rows.push(row);
    }
    let copies: Vec<Copy<'_>> = rows
        .iter()
        .map(|row| Copy {
            row,
            body: &body,
            attachments: &attachments,
        })
        .collect();
    app.store.deliver_all(&copies, &blobs, stamp, quota).await?;
    Ok(Outcome {
        sent_id,
        delivered,
        refused,
    })
}
