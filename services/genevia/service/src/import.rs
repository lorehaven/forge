//! Reading an upload: a GEDCOM file, as the `file` field of a `multipart/form-data` body (what the forge shell's
//! `<input type="file" data-upload-url>` sends).

use quench_http::multipart::Multipart;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::*;

/// What an upload may weigh. GEDCOM is text, and a large family is a few megabytes.
pub const MAX_UPLOAD: usize = 64 * 1024 * 1024;

/// The file that was uploaded: its name and its text.
#[derive(Debug)]
pub struct Upload {
    pub name: String,
    pub text: String,
}

/// Why an upload cannot be used.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    pub status: StatusCode,
    pub message: String,
}

impl Refused {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

/// The `file` field of the body, read as UTF-8 text.
///
/// # Errors
/// When the body is not a multipart form, has no `file` field, or the file is not UTF-8.
pub async fn read(req: &mut Request) -> Result<Upload, Refused> {
    use quench_http::extract::FromRequest;
    let mut form = Multipart::from_request(req)
        .await
        .map_err(|e| Refused::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    let mut upload: Option<(String, Vec<u8>)> = None;
    loop {
        match form.next_field().await {
            Ok(Some(field)) if field.name() == Some("file") => {
                let name = field.file_name().unwrap_or("tree.ged").to_string();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| Refused::new(StatusCode::BAD_REQUEST, e.to_string()))?;
                upload = Some((name, bytes.to_vec()));
            }
            Ok(Some(_)) => {}
            Ok(None) => break,
            Err(e) => return Err(Refused::new(StatusCode::BAD_REQUEST, e.to_string())),
        }
    }
    let Some((name, bytes)) = upload else {
        return Err(Refused::new(
            StatusCode::BAD_REQUEST,
            "no `file` field in the upload",
        ));
    };
    let text = String::from_utf8(bytes).map_err(|_| {
        Refused::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "the file is not UTF-8 text; save the GEDCOM as UTF-8",
        )
    })?;
    Ok(Upload { name, text })
}

/// The request's upload, or why it cannot be used: for a route that answers the refusal itself, as JSON.
#[derive(Debug)]
pub struct Incoming(pub Result<Upload, Refused>);

#[quench_core::async_trait]
impl quench_http::extract::FromRequest for Incoming {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        Ok(Self(read(req).await))
    }
}
