use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::domain::models::{Conversation, File, FileChunk, Project};
use crate::files::{STATUS_UPLOADED, pipeline};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use quench_auth::domain::jwt::JwtConfig;
use quench_auth::http::routers::ui::get_user_from_req;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{
    FromRequest, HttpError, Inject, Multipart, Path, Query, Request, Response, delete, get,
    http::StatusCode, post,
};
use uuid::Uuid;

const DEFAULT_MAX_FILE_SIZE_MB: u64 = 25;

fn max_file_size_bytes() -> u64 {
    envmnt::get_u64("SAGE_FILE_MAX_SIZE_MB", DEFAULT_MAX_FILE_SIZE_MB) * 1024 * 1024
}

fn db_schema() -> String {
    envmnt::get_or("DB_SCHEMA", "sage")
}

/// Single source of truth for accepted extensions - both upload validation
/// and the composer's file-picker `accept` filter read from this.
pub const ALLOWED_UPLOAD_TYPES: &[(&str, &str)] = &[
    // Images (sent to vision models, not text-extracted)
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    // Documents
    ("pdf", "application/pdf"),
    ("txt", "text/plain"),
    ("log", "text/plain"),
    ("ini", "text/plain"),
    ("conf", "text/plain"),
    ("cfg", "text/plain"),
    ("csv", "text/csv"),
    ("md", "text/markdown"),
    ("html", "text/html"),
    ("htm", "text/html"),
    // Data & config formats
    ("json", "application/json"),
    ("yaml", "application/yaml"),
    ("yml", "application/yaml"),
    ("toml", "application/toml"),
    ("xml", "application/xml"),
    // Source code
    ("rs", "text/x-rust"),
    ("py", "text/x-python"),
    ("js", "text/javascript"),
    ("mjs", "text/javascript"),
    ("jsx", "text/javascript"),
    ("ts", "text/x-typescript"),
    ("tsx", "text/x-typescript"),
    ("java", "text/x-java"),
    ("kt", "text/x-kotlin"),
    ("kts", "text/x-kotlin"),
    ("go", "text/x-go"),
    ("c", "text/x-c"),
    ("h", "text/x-c"),
    ("cpp", "text/x-c++"),
    ("cc", "text/x-c++"),
    ("cxx", "text/x-c++"),
    ("hpp", "text/x-c++"),
    ("cs", "text/x-csharp"),
    ("rb", "text/x-ruby"),
    ("php", "text/x-php"),
    ("swift", "text/x-swift"),
    ("sh", "text/x-shellscript"),
    ("bash", "text/x-shellscript"),
    ("zsh", "text/x-shellscript"),
    ("fish", "text/x-shellscript"),
    ("sql", "application/sql"),
    ("css", "text/css"),
    ("scss", "text/css"),
];

/// Map a file name to its stored MIME type; `None` if the extension is unsupported.
pub fn allowed_mime_type(file_name: &str) -> Option<&'static str> {
    let ext = file_name.rsplit('.').next()?.to_lowercase();
    ALLOWED_UPLOAD_TYPES
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, mime)| *mime)
}

/// Value for the `accept` attribute of the upload inputs: every extension the
/// server accepts, so the browser's file picker offers exactly those.
pub fn upload_accept_attribute() -> String {
    ALLOWED_UPLOAD_TYPES
        .iter()
        .map(|(ext, _)| format!(".{ext}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub struct FileUploadForm {
    pub file_name: Option<String>,
    pub file_data: Bytes,
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
}

/// Reads a `multipart/form-data` body into a `FileUploadForm`. The real,
/// user-facing size limit is `max_file_size_bytes()`, checked below.
pub(crate) async fn parse_upload_form(mut form: Multipart) -> Result<FileUploadForm, HttpError> {
    let mut file_name = None;
    let mut file_data = Bytes::new();
    let mut conversation_id = None;
    let mut project_id = None;

    while let Some(field) = form.next_field().await? {
        match field.name() {
            Some("file") => {
                file_name = field.file_name().map(|s| s.to_string());
                file_data = field.bytes().await?;
            }
            Some("conversation_id") => conversation_id = Some(field.text().await?),
            Some("project_id") => project_id = Some(field.text().await?),
            _ => {}
        }
    }

    Ok(FileUploadForm {
        file_name,
        file_data,
        conversation_id,
        project_id,
    })
}

/// Log the underlying error and return the generic `api_error_*` code the UI resolves via i18n.
fn internal_error<E: std::fmt::Display>(e: E) -> Response {
    tracing::error!("Internal error: {}", e);
    Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
}

/// Validate and store an uploaded file, then start background processing. Shared by the JSON
/// API and the UI upload endpoint; errors come back as ready-to-return HTTP responses.
pub async fn create_uploaded_file(
    db: &Db,
    switchboard: &SwitchboardClient,
    vllm: &VllmClient,
    username: &str,
    form: FileUploadForm,
) -> Result<File, Response> {
    let (conversation_id, project_id) = match (&form.conversation_id, &form.project_id) {
        (Some(c), None) => (Some(c.clone()), None),
        (None, Some(p)) => (None, Some(p.clone())),
        _ => {
            return Err(Response::text(
                StatusCode::BAD_REQUEST,
                "api_error_file_scope_required",
            ));
        }
    };

    let Some(file_name) = form.file_name.clone() else {
        return Err(Response::text(
            StatusCode::BAD_REQUEST,
            "api_error_missing_file_name",
        ));
    };

    let Some(mime_type) = allowed_mime_type(&file_name) else {
        return Err(Response::text(
            StatusCode::BAD_REQUEST,
            "api_error_unsupported_file_type",
        ));
    };

    let max_size = max_file_size_bytes();
    if form.file_data.len() as u64 > max_size {
        return Err(Response::text(
            StatusCode::PAYLOAD_TOO_LARGE,
            "api_error_file_too_large",
        ));
    }
    if form.file_data.is_empty() {
        return Err(Response::text(
            StatusCode::BAD_REQUEST,
            "api_error_file_empty",
        ));
    }

    // The upload target must exist and belong to the requesting user.
    if let Some(cid) = &conversation_id {
        match db.repository::<Conversation>().read(cid).await {
            Ok(Some(c)) if c.owner == username => {}
            Ok(Some(_)) => return Err(Response::new(StatusCode::FORBIDDEN)),
            Ok(None) => {
                return Err(Response::text(
                    StatusCode::NOT_FOUND,
                    "api_error_conversation_not_found",
                ));
            }
            Err(e) => return Err(internal_error(e)),
        }
    }
    if let Some(pid) = &project_id {
        match db.repository::<Project>().read(pid).await {
            Ok(Some(p)) if p.owner == username => {}
            Ok(Some(_)) => return Err(Response::new(StatusCode::FORBIDDEN)),
            Ok(None) => {
                return Err(Response::text(
                    StatusCode::NOT_FOUND,
                    "api_error_project_not_found",
                ));
            }
            Err(e) => return Err(internal_error(e)),
        }
    }

    let max_files = envmnt::get_u64("SAGE_MAX_FILES_PER_SCOPE", 50);
    if let Db::Postgres(pg_db) = db {
        let schema = db_schema();
        let count_sql = format!(
            "SELECT count(*) FROM {schema}.files WHERE conversation_id = $1 OR project_id = $2"
        );
        let (count,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(count_sql.as_str()))
            .bind(&conversation_id)
            .bind(&project_id)
            .fetch_one(pg_db.pool())
            .await
            .map_err(internal_error)?;
        if count as u64 >= max_files {
            return Err(Response::text(
                StatusCode::UNPROCESSABLE_ENTITY,
                "api_error_file_limit_reached",
            ));
        }
    }

    // Images are not text-extracted: ready as soon as the blob is stored, never enter the chunk/embed pipeline.
    let is_image = crate::files::is_image_mime(mime_type);
    let now = Utc::now().to_rfc3339();
    let file = File {
        id: Uuid::new_v4().to_string(),
        owner: username.to_string(),
        file_name,
        mime_type: mime_type.to_string(),
        file_size: form.file_data.len() as i64,
        conversation_id,
        project_id,
        message_id: None,
        status: if is_image {
            crate::files::STATUS_READY.to_string()
        } else {
            STATUS_UPLOADED.to_string()
        },
        error_message: None,
        created_at: now.clone(),
        updated_at: now,
    };

    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let insert_file = format!(
                "INSERT INTO {schema}.files (id, owner, file_name, mime_type, file_size, conversation_id, project_id, status, error_message, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
            );
            let insert_blob =
                format!("INSERT INTO {schema}.file_blobs (file_id, data) VALUES ($1, $2)");

            let result: Result<(), sqlx::Error> = async {
                let mut tx = pg_db.pool().begin().await?;
                sqlx::query(sqlx::AssertSqlSafe(insert_file.as_str()))
                    .bind(&file.id)
                    .bind(&file.owner)
                    .bind(&file.file_name)
                    .bind(&file.mime_type)
                    .bind(file.file_size)
                    .bind(&file.conversation_id)
                    .bind(&file.project_id)
                    .bind(&file.status)
                    .bind(&file.error_message)
                    .bind(&file.created_at)
                    .bind(&file.updated_at)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(sqlx::AssertSqlSafe(insert_blob.as_str()))
                    .bind(&file.id)
                    .bind(form.file_data.as_ref())
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                Ok(())
            }
            .await;

            if let Err(e) = result {
                tracing::error!("Failed to store uploaded file: {}", e);
                return Err(Response::text(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "api_error_internal",
                ));
            }

            if !is_image {
                pipeline::spawn_processing(
                    db.clone(),
                    switchboard.clone(),
                    vllm.clone(),
                    file.id.clone(),
                );
            }

            Ok(file)
        }
        Db::InMemory(_) => Err(Response::text(
            StatusCode::NOT_IMPLEMENTED,
            "api_error_postgres_required",
        )),
    }
}

/// Username or the 401 to answer - `HttpError::into_response` only renders
/// fixed text, so this travels as the success value instead.
pub enum Username {
    Ok(String),
    Unauthorized,
}

impl Username {
    pub fn or_401(self) -> Result<String, Response> {
        match self {
            Self::Ok(username) => Ok(username),
            Self::Unauthorized => Err(Response::new(StatusCode::UNAUTHORIZED)),
        }
    }
}

#[async_trait]
impl FromRequest for Username {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let Ok(config) = req.container().get::<JwtConfig>() else {
            return Ok(Self::Unauthorized);
        };
        match get_user_from_req(req, &config).await {
            Some(claims) => Ok(Self::Ok(claims.sub)),
            None => Ok(Self::Unauthorized),
        }
    }
}

#[post("/api/v1/files")]
pub async fn upload_file(
    username: Username,
    Inject(db): Inject<Db>,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(vllm): Inject<VllmClient>,
    form: Multipart,
) -> Result<Response, HttpError> {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return Ok(response),
    };
    let form = parse_upload_form(form).await?;

    Ok(
        match create_uploaded_file(&db, &switchboard, &vllm, &username, form).await {
            Ok(file) => Response::json(StatusCode::CREATED, &file)
                .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR)),
            Err(resp) => resp,
        },
    )
}

#[derive(serde::Deserialize)]
pub struct ListFilesQuery {
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
}

#[get("/api/v1/files")]
pub async fn list_files(
    username: Username,
    Inject(db): Inject<Db>,
    Query(query): Query<ListFilesQuery>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };

    match (&query.conversation_id, &query.project_id) {
        (Some(cid), None) => {
            let conversation = match db.repository::<Conversation>().read(cid).await {
                Ok(Some(c)) if c.owner == username => c,
                Ok(Some(_)) => return Response::new(StatusCode::FORBIDDEN),
                Ok(None) => {
                    return Response::text(
                        StatusCode::NOT_FOUND,
                        "api_error_conversation_not_found",
                    );
                }
                Err(e) => return internal_error(e),
            };

            match visible_files_for_conversation(&db, &conversation).await {
                Ok(files) => json_ok(&files),
                Err(e) => internal_error(e),
            }
        }
        (None, Some(pid)) => {
            match db.repository::<Project>().read(pid).await {
                Ok(Some(p)) if p.owner == username => {}
                Ok(Some(_)) => return Response::new(StatusCode::FORBIDDEN),
                Ok(None) => {
                    return Response::text(StatusCode::NOT_FOUND, "api_error_project_not_found");
                }
                Err(e) => return internal_error(e),
            }

            match visible_files_for_project(&db, pid).await {
                Ok(files) => json_ok(&files),
                Err(e) => internal_error(e),
            }
        }
        _ => Response::text(StatusCode::BAD_REQUEST, "api_error_file_scope_required"),
    }
}

/// Files visible in a conversation: attached directly, or — if it belongs to a project —
/// attached to the project or any of the project's conversations.
pub async fn visible_files_for_conversation(
    db: &Db,
    conversation: &Conversation,
) -> Result<Vec<File>, String> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!(
                "SELECT f.* FROM {schema}.files f LEFT JOIN {schema}.conversations c ON f.conversation_id = c.id WHERE f.conversation_id = $1 OR ($2::text IS NOT NULL AND (f.project_id = $2 OR c.project_id = $2)) ORDER BY f.created_at"
            );
            sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(query.as_str()))
                .bind(&conversation.id)
                .bind(&conversation.project_id)
                .fetch_all(pg_db.pool())
                .await
                .map_err(|e| e.to_string())
        }
        Db::InMemory(_) => {
            let files = db
                .repository::<File>()
                .list()
                .await
                .map_err(|e| e.to_string())?;
            let project_conversation_ids: Vec<String> = match &conversation.project_id {
                Some(pid) => db
                    .repository::<Conversation>()
                    .list()
                    .await
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter(|c| c.project_id.as_deref() == Some(pid))
                    .map(|c| c.id)
                    .collect(),
                None => Vec::new(),
            };
            Ok(files
                .into_iter()
                .filter(|f| {
                    f.conversation_id.as_deref() == Some(&conversation.id)
                        || (conversation.project_id.is_some()
                            && (f.project_id == conversation.project_id
                                || f.conversation_id
                                    .as_ref()
                                    .is_some_and(|cid| project_conversation_ids.contains(cid))))
                })
                .collect())
        }
    }
}

/// Files visible in a project: attached to it directly or to any of its conversations.
pub async fn visible_files_for_project(db: &Db, project_id: &str) -> Result<Vec<File>, String> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!(
                "SELECT f.* FROM {schema}.files f LEFT JOIN {schema}.conversations c ON f.conversation_id = c.id WHERE f.project_id = $1 OR c.project_id = $1 ORDER BY f.created_at"
            );
            sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(query.as_str()))
                .bind(project_id)
                .fetch_all(pg_db.pool())
                .await
                .map_err(|e| e.to_string())
        }
        Db::InMemory(_) => {
            let conversation_ids: Vec<String> = db
                .repository::<Conversation>()
                .list()
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|c| c.project_id.as_deref() == Some(project_id))
                .map(|c| c.id)
                .collect();
            Ok(db
                .repository::<File>()
                .list()
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|f| {
                    f.project_id.as_deref() == Some(project_id)
                        || f.conversation_id
                            .as_ref()
                            .is_some_and(|cid| conversation_ids.contains(cid))
                })
                .collect())
        }
    }
}

/// Links staged files to a sent message; only files owned by `username` in
/// `conversation_id` qualify, so a forged id can't steal another user's file.
pub async fn link_files_to_message(
    db: &Db,
    file_ids: &[String],
    message_id: &str,
    conversation_id: &str,
    username: &str,
) -> Result<(), String> {
    if file_ids.is_empty() {
        return Ok(());
    }
    let Db::Postgres(pg_db) = db else {
        return Ok(());
    };
    let schema = db_schema();
    let sql = format!(
        "UPDATE {schema}.files SET message_id = $1, updated_at = $2 WHERE id = ANY($3) AND owner = $4 AND conversation_id = $5 AND message_id IS NULL"
    );
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(message_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(file_ids)
        .bind(username)
        .bind(conversation_id)
        .execute(pg_db.pool())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Load files attached to each of the given messages, keyed by message id.
pub async fn files_by_message(
    db: &Db,
    message_ids: &[String],
) -> std::collections::HashMap<String, Vec<File>> {
    let mut map: std::collections::HashMap<String, Vec<File>> = std::collections::HashMap::new();
    if message_ids.is_empty() {
        return map;
    }
    let Db::Postgres(pg_db) = db else {
        return map;
    };
    let schema = db_schema();
    let query =
        format!("SELECT * FROM {schema}.files WHERE message_id = ANY($1) ORDER BY created_at");
    let files = match sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(query.as_str()))
        .bind(message_ids)
        .fetch_all(pg_db.pool())
        .await
    {
        Ok(files) => files,
        Err(e) => {
            tracing::error!("Failed to load message attachments: {}", e);
            return map;
        }
    };
    for file in files {
        if let Some(mid) = file.message_id.clone() {
            map.entry(mid).or_default().push(file);
        }
    }
    map
}

async fn load_owned_file(db: &Db, file_id: &str, username: &str) -> Result<File, Response> {
    match db.repository::<File>().read(file_id).await {
        Ok(Some(f)) if f.owner == username => Ok(f),
        Ok(Some(_)) => Err(Response::new(StatusCode::FORBIDDEN)),
        Ok(None) => Err(Response::text(
            StatusCode::NOT_FOUND,
            "api_error_file_not_found",
        )),
        Err(e) => Err(internal_error(e)),
    }
}

#[get("/api/v1/files/{file_id}")]
pub async fn get_file(
    username: Username,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };

    match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => json_ok(&file),
        Err(resp) => resp,
    }
}

#[get("/api/v1/files/{file_id}/download")]
pub async fn download_file(
    username: Username,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    match &*db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!("SELECT data FROM {schema}.file_blobs WHERE file_id = $1");
            match sqlx::query_as::<_, (Vec<u8>,)>(sqlx::AssertSqlSafe(query.as_str()))
                .bind(&file.id)
                .fetch_optional(pg_db.pool())
                .await
            {
                Ok(Some((data,))) => {
                    // Strip characters that would corrupt the header value.
                    let safe_name: String = file
                        .file_name
                        .chars()
                        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
                        .collect();
                    // Images render inline (thumbnails, opening in a tab); everything else stays a download.
                    let disposition = if crate::files::is_image_mime(&file.mime_type) {
                        "inline"
                    } else {
                        "attachment"
                    };
                    Response::from_bytes(StatusCode::OK, Bytes::from(data))
                        .header("content-type", &file.mime_type)
                        .header(
                            "Content-Disposition",
                            format!("{}; filename=\"{}\"", disposition, safe_name),
                        )
                }
                Ok(None) => {
                    Response::text(StatusCode::NOT_FOUND, "api_error_file_content_not_found")
                }
                Err(e) => internal_error(e),
            }
        }
        Db::InMemory(_) => {
            Response::text(StatusCode::NOT_IMPLEMENTED, "api_error_postgres_required")
        }
    }
}

#[post("/api/v1/files/{file_id}/reprocess")]
pub async fn reprocess_file(
    username: Username,
    Inject(db): Inject<Db>,
    Inject(switchboard): Inject<SwitchboardClient>,
    Inject(vllm): Inject<VllmClient>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    if file.status == crate::files::STATUS_PROCESSING {
        return Response::text(StatusCode::CONFLICT, "api_error_file_already_processing");
    }
    // Images have no text pipeline to (re)run; they are ready once stored.
    if crate::files::is_image_mime(&file.mime_type) {
        return Response::text(
            StatusCode::UNPROCESSABLE_ENTITY,
            "api_error_image_not_processable",
        );
    }

    pipeline::spawn_processing(
        (*db).clone(),
        (*switchboard).clone(),
        (*vllm).clone(),
        file.id.clone(),
    );
    Response::json(StatusCode::ACCEPTED, &file)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

#[get("/api/v1/files/{file_id}/chunks")]
pub async fn list_chunks(
    username: Username,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    match &*db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!(
                "SELECT id, file_id, chunk_index, content, embedding_model, metadata, created_at FROM {schema}.file_chunks WHERE file_id = $1 ORDER BY chunk_index"
            );
            match sqlx::query_as::<_, FileChunk>(sqlx::AssertSqlSafe(query.as_str()))
                .bind(&file.id)
                .fetch_all(pg_db.pool())
                .await
            {
                Ok(chunks) => json_ok(&chunks),
                Err(e) => internal_error(e),
            }
        }
        Db::InMemory(_) => match db.repository::<FileChunk>().list().await {
            Ok(chunks) => {
                let mut chunks: Vec<FileChunk> = chunks
                    .into_iter()
                    .filter(|c| c.file_id == file.id)
                    .collect();
                chunks.sort_by_key(|c| c.chunk_index);
                json_ok(&chunks)
            }
            Err(e) => internal_error(e),
        },
    }
}

#[delete("/api/v1/files/{file_id}")]
pub async fn delete_file(
    username: Username,
    Inject(db): Inject<Db>,
    Path(file_id): Path<String>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    // Blobs and chunks are removed by ON DELETE CASCADE.
    match db.repository::<File>().delete(&file.id).await {
        Ok(()) => Response::new(StatusCode::NO_CONTENT),
        Err(e) => internal_error(e),
    }
}

fn json_ok<T: serde::Serialize>(value: &T) -> Response {
    Response::json(StatusCode::OK, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

pub fn register_routes() {
    let _ = upload_file as fn(_, _, _, _, _) -> _;
    let _ = list_files as fn(_, _, _) -> _;
    let _ = get_file as fn(_, _, _) -> _;
    let _ = download_file as fn(_, _, _) -> _;
    let _ = reprocess_file as fn(_, _, _, _, _) -> _;
    let _ = list_chunks as fn(_, _, _) -> _;
    let _ = delete_file as fn(_, _, _) -> _;
}
