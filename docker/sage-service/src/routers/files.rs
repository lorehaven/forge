use crate::clients::switchboard::SwitchboardClient;
use crate::clients::vllm::VllmClient;
use crate::files::{STATUS_UPLOADED, pipeline};
use crate::models::{Conversation, File, FileChunk, Project};
use actix_multipart::form::{MultipartForm, bytes::Bytes as MultipartBytes, text::Text};
use actix_web::{HttpRequest, HttpResponse, Responder, delete, get, post, web};
use chrono::Utc;
use quench_auth::actix::routers::ui::get_user_from_req;
use quench_auth::prelude::JwtConfig;
use quench_db::prelude::{Crud, Db};
use uuid::Uuid;

const DEFAULT_MAX_FILE_SIZE_MB: u64 = 25;

fn max_file_size_bytes() -> u64 {
    envmnt::get_u64("SAGE_FILE_MAX_SIZE_MB", DEFAULT_MAX_FILE_SIZE_MB) * 1024 * 1024
}

fn db_schema() -> String {
    envmnt::get_or("DB_SCHEMA", "sage")
}

/// Map a file name to its stored MIME type. Returns None for unsupported formats.
fn allowed_mime_type(file_name: &str) -> Option<&'static str> {
    let ext = file_name.rsplit('.').next()?.to_lowercase();
    match ext.as_str() {
        "pdf" => Some("application/pdf"),
        "txt" => Some("text/plain"),
        "csv" => Some("text/csv"),
        "md" => Some("text/markdown"),
        _ => None,
    }
}

#[derive(MultipartForm)]
pub struct FileUploadForm {
    #[multipart(limit = "100MB")]
    pub file: MultipartBytes,
    pub conversation_id: Option<Text<String>>,
    pub project_id: Option<Text<String>>,
}

/// Log the underlying error and answer with the generic i18n error code. The
/// UI resolves `api_error_*` codes through the i18n dictionary.
fn internal_error<E: std::fmt::Display>(e: E) -> HttpResponse {
    tracing::error!("Internal error: {}", e);
    HttpResponse::InternalServerError().body("api_error_internal")
}

/// Validate and store an uploaded file, then start background processing.
/// Shared by the JSON API and the UI upload endpoint; errors come back as
/// ready-to-return HTTP responses.
pub async fn create_uploaded_file(
    db: &Db,
    switchboard: &SwitchboardClient,
    vllm: &VllmClient,
    username: &str,
    form: FileUploadForm,
) -> Result<File, HttpResponse> {
    let (conversation_id, project_id) = match (&form.conversation_id, &form.project_id) {
        (Some(c), None) => (Some(c.0.clone()), None),
        (None, Some(p)) => (None, Some(p.0.clone())),
        _ => {
            return Err(HttpResponse::BadRequest()
                .body("api_error_file_scope_required"));
        }
    };

    let Some(file_name) = form.file.file_name.clone() else {
        return Err(HttpResponse::BadRequest().body("api_error_missing_file_name"));
    };

    let Some(mime_type) = allowed_mime_type(&file_name) else {
        return Err(
            HttpResponse::BadRequest().body("api_error_unsupported_file_type")
        );
    };

    let max_size = max_file_size_bytes();
    if form.file.data.len() as u64 > max_size {
        return Err(HttpResponse::PayloadTooLarge().body("api_error_file_too_large"));
    }
    if form.file.data.is_empty() {
        return Err(HttpResponse::BadRequest().body("api_error_file_empty"));
    }

    // The upload target must exist and belong to the requesting user.
    if let Some(cid) = &conversation_id {
        match db.repository::<Conversation>().read(cid).await {
            Ok(Some(c)) if c.owner == username => {}
            Ok(Some(_)) => return Err(HttpResponse::Forbidden().finish()),
            Ok(None) => return Err(HttpResponse::NotFound().body("api_error_conversation_not_found")),
            Err(e) => return Err(internal_error(e)),
        }
    }
    if let Some(pid) = &project_id {
        match db.repository::<Project>().read(pid).await {
            Ok(Some(p)) if p.owner == username => {}
            Ok(Some(_)) => return Err(HttpResponse::Forbidden().finish()),
            Ok(None) => return Err(HttpResponse::NotFound().body("api_error_project_not_found")),
            Err(e) => return Err(internal_error(e)),
        }
    }

    let max_files = envmnt::get_u64("SAGE_MAX_FILES_PER_SCOPE", 50);
    if let Db::Postgres(pg_db) = db {
        let schema = db_schema();
        let count_sql = format!(
            "SELECT count(*) FROM {schema}.files \
             WHERE conversation_id = $1 OR project_id = $2"
        );
        let (count,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(count_sql.as_str()))
            .bind(&conversation_id)
            .bind(&project_id)
            .fetch_one(pg_db.pool())
            .await
            .map_err(internal_error)?;
        if count as u64 >= max_files {
            return Err(HttpResponse::UnprocessableEntity().body("api_error_file_limit_reached"));
        }
    }

    let now = Utc::now().to_rfc3339();
    let file = File {
        id: Uuid::new_v4().to_string(),
        owner: username.to_string(),
        file_name,
        mime_type: mime_type.to_string(),
        file_size: form.file.data.len() as i64,
        conversation_id,
        project_id,
        message_id: None,
        status: STATUS_UPLOADED.to_string(),
        error_message: None,
        created_at: now.clone(),
        updated_at: now,
    };

    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let insert_file = format!(
                "INSERT INTO {schema}.files \
                 (id, owner, file_name, mime_type, file_size, conversation_id, project_id, \
                  status, error_message, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
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
                    .bind(form.file.data.as_ref())
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                Ok(())
            }
            .await;

            if let Err(e) = result {
                tracing::error!("Failed to store uploaded file: {}", e);
                return Err(HttpResponse::InternalServerError().body("api_error_internal"));
            }

            pipeline::spawn_processing(
                db.clone(),
                switchboard.clone(),
                vllm.clone(),
                file.id.clone(),
            );

            Ok(file)
        }
        Db::InMemory(_) => {
            Err(HttpResponse::NotImplemented().body("api_error_postgres_required"))
        }
    }
}

#[post("")]
pub async fn upload_file(
    req: HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    form: MultipartForm<FileUploadForm>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    match create_uploaded_file(
        &db,
        switchboard.get_ref(),
        vllm.get_ref(),
        &username,
        form.into_inner(),
    )
    .await
    {
        Ok(file) => HttpResponse::Created().json(&file),
        Err(resp) => resp,
    }
}

#[derive(serde::Deserialize)]
pub struct ListFilesQuery {
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
}

#[get("")]
pub async fn list_files(
    req: HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    query: web::Query<ListFilesQuery>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    match (&query.conversation_id, &query.project_id) {
        (Some(cid), None) => {
            let conversation = match db.repository::<Conversation>().read(cid).await {
                Ok(Some(c)) if c.owner == username => c,
                Ok(Some(_)) => return HttpResponse::Forbidden().finish(),
                Ok(None) => return HttpResponse::NotFound().body("api_error_conversation_not_found"),
                Err(e) => return internal_error(e),
            };

            match visible_files_for_conversation(&db, &conversation).await {
                Ok(files) => HttpResponse::Ok().json(files),
                Err(e) => internal_error(e),
            }
        }
        (None, Some(pid)) => {
            match db.repository::<Project>().read(pid).await {
                Ok(Some(p)) if p.owner == username => {}
                Ok(Some(_)) => return HttpResponse::Forbidden().finish(),
                Ok(None) => return HttpResponse::NotFound().body("api_error_project_not_found"),
                Err(e) => return internal_error(e),
            }

            match visible_files_for_project(&db, pid).await {
                Ok(files) => HttpResponse::Ok().json(files),
                Err(e) => internal_error(e),
            }
        }
        _ => HttpResponse::BadRequest()
            .body("api_error_file_scope_required"),
    }
}

/// Files visible in a conversation: attached to it directly, or — when it
/// belongs to a project — attached to the project or to any of the project's
/// conversations.
pub async fn visible_files_for_conversation(
    db: &Db,
    conversation: &Conversation,
) -> Result<Vec<File>, String> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!(
                "SELECT f.* FROM {schema}.files f \
                 LEFT JOIN {schema}.conversations c ON f.conversation_id = c.id \
                 WHERE f.conversation_id = $1 \
                    OR ($2::text IS NOT NULL AND (f.project_id = $2 OR c.project_id = $2)) \
                 ORDER BY f.created_at"
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

/// Files visible in a project: attached to it directly or to any of its
/// conversations.
pub async fn visible_files_for_project(db: &Db, project_id: &str) -> Result<Vec<File>, String> {
    match db {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!(
                "SELECT f.* FROM {schema}.files f \
                 LEFT JOIN {schema}.conversations c ON f.conversation_id = c.id \
                 WHERE f.project_id = $1 OR c.project_id = $1 \
                 ORDER BY f.created_at"
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

/// Attach staged files (message_id IS NULL) to a sent user message. Only files
/// owned by `username` and belonging to `conversation_id` are linked, so a
/// forged file id in the form cannot steal another user's or scope's file.
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
        "UPDATE {schema}.files SET message_id = $1, updated_at = $2 \
         WHERE id = ANY($3) AND owner = $4 AND conversation_id = $5 AND message_id IS NULL"
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

async fn load_owned_file(db: &Db, file_id: &str, username: &str) -> Result<File, HttpResponse> {
    match db.repository::<File>().read(file_id).await {
        Ok(Some(f)) if f.owner == username => Ok(f),
        Ok(Some(_)) => Err(HttpResponse::Forbidden().finish()),
        Ok(None) => Err(HttpResponse::NotFound().body("api_error_file_not_found")),
        Err(e) => Err(internal_error(e)),
    }
}

#[get("/{file_id}")]
pub async fn get_file(
    req: HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => HttpResponse::Ok().json(file),
        Err(resp) => resp,
    }
}

#[get("/{file_id}/download")]
pub async fn download_file(
    req: HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    match db.get_ref() {
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
                    HttpResponse::Ok()
                        .content_type(file.mime_type.clone())
                        .append_header((
                            "Content-Disposition",
                            format!("attachment; filename=\"{}\"", safe_name),
                        ))
                        .body(data)
                }
                Ok(None) => HttpResponse::NotFound().body("api_error_file_content_not_found"),
                Err(e) => internal_error(e),
            }
        }
        Db::InMemory(_) => {
            HttpResponse::NotImplemented().body("api_error_postgres_required")
        }
    }
}

#[post("/{file_id}/reprocess")]
pub async fn reprocess_file(
    req: HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    switchboard: web::Data<SwitchboardClient>,
    vllm: web::Data<VllmClient>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    if file.status == crate::files::STATUS_PROCESSING {
        return HttpResponse::Conflict().body("api_error_file_already_processing");
    }

    pipeline::spawn_processing(
        db.get_ref().clone(),
        switchboard.get_ref().clone(),
        vllm.get_ref().clone(),
        file.id.clone(),
    );
    HttpResponse::Accepted().json(file)
}

#[get("/{file_id}/chunks")]
pub async fn list_chunks(
    req: HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    match db.get_ref() {
        Db::Postgres(pg_db) => {
            let schema = db_schema();
            let query = format!(
                "SELECT id, file_id, chunk_index, content, embedding_model, metadata, created_at \
                 FROM {schema}.file_chunks WHERE file_id = $1 ORDER BY chunk_index"
            );
            match sqlx::query_as::<_, FileChunk>(sqlx::AssertSqlSafe(query.as_str()))
                .bind(&file.id)
                .fetch_all(pg_db.pool())
                .await
            {
                Ok(chunks) => HttpResponse::Ok().json(chunks),
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
                HttpResponse::Ok().json(chunks)
            }
            Err(e) => internal_error(e),
        },
    }
}

#[delete("/{file_id}")]
pub async fn delete_file(
    req: HttpRequest,
    jwt_config: web::Data<JwtConfig>,
    db: web::Data<Db>,
    file_id: web::Path<String>,
) -> impl Responder {
    let username = match get_user_from_req(&req, &jwt_config).await {
        Some(claims) => claims.sub,
        None => return HttpResponse::Unauthorized().finish(),
    };

    let file = match load_owned_file(&db, &file_id, &username).await {
        Ok(file) => file,
        Err(resp) => return resp,
    };

    // Blobs and chunks are removed by ON DELETE CASCADE.
    match db.repository::<File>().delete(&file.id).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => internal_error(e),
    }
}

pub fn scope() -> actix_web::Scope {
    web::scope("/api/v1/files")
        .service(upload_file)
        .service(list_files)
        .service(download_file)
        .service(reprocess_file)
        .service(list_chunks)
        .service(get_file)
        .service(delete_file)
}
