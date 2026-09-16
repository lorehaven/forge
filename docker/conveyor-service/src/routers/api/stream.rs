//! Watching a job's output live (executor) or after the fact (database).
//! Caveat: only the replica actually running a job holds its live output.

use crate::executors::{Executor, Handle, LogChunk, Stream as LogStream};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, OptionalClaims, json_error};
use crate::scheduler::queue;
use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::stream::iter as stream_iter;
use quench_auth::domain::jwt::JwtConfig;
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_web::prelude::*;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;

/// What a frame's `data:` carries - a query param since `EventSource`
/// (htmx's SSE extension) sends a fixed `Accept` header, no room to negotiate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// The line as written - what `conveyor logs --follow` reads.
    #[default]
    Text,
    /// An escaped `<span>` for `hx-swap="beforeend"` appending.
    Html,
}

#[derive(Debug, Default, Deserialize)]
pub struct StreamQuery {
    #[serde(default)]
    pub format: Format,
}

/// Shared by [`stream_logs`] and [`raw_logs`] - checks read access before
/// either touches the job's output.
async fn authorize_job_read(
    claims: Option<&quench_auth::domain::jwt::Claims>,
    config: &JwtConfig,
    db: &Db,
    job_id: &str,
) -> Result<(), Response> {
    match crate::scheduler::queue::repo_id_for_job(db, job_id).await {
        Ok(Some(repo_id)) => match crate::scheduler::repos::read(db, &repo_id).await {
            Ok(Some(repo)) => {
                if !can_on_project(claims, config, db, &repo.project_id, "read").await {
                    return Err(json_error(
                        StatusCode::FORBIDDEN,
                        "no read access to this job's logs",
                    ));
                }
                Ok(())
            }
            Ok(None) => Err(json_error(StatusCode::NOT_FOUND, "no such job")),
            Err(error) => Err(ApiError::from(error).into_response()),
        },
        Ok(None) => Err(json_error(StatusCode::NOT_FOUND, "no such job")),
        Err(error) => Err(ApiError::from(error).into_response()),
    }
}

/// SSE, one event per log line; `id:` carries the sequence number so a
/// reconnect can send `Last-Event-ID` instead of replaying the whole log.
#[get("/api/v1/jobs/{id}/stream")]
pub async fn stream_logs(
    OptionalClaims(claims): OptionalClaims,
    Path(job_id): Path<String>,
    Query(query): Query<StreamQuery>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
    Inject(executor): Inject<Executor>,
) -> Response {
    if let Err(response) = authorize_job_read(claims.as_ref(), &config, &db, &job_id).await {
        return response;
    }

    let format = query.format;
    let handle = Handle::new(job_id.clone());

    // The executor first: if it knows this job, it's still running.
    if let Ok(tail) = executor.0.logs(&handle).await {
        let history = stream_iter(
            tail.history
                .into_iter()
                .map(move |chunk| Ok(frame(&chunk, format))),
        );

        let live = BroadcastStream::new(tail.live).map(move |message| match message {
            Ok(chunk) => Ok(frame(&chunk, format)),
            // Subscriber fell behind the channel; say so rather than pretend.
            Err(_) => Ok::<_, std::io::Error>(Bytes::from(
                "event: lagged\ndata: some lines were skipped; reload to see them\n\n",
            )),
        });

        // Say the log is done, or a browser reads a plain close as a drop and refetches everything.
        let ended = stream_iter([Ok::<_, std::io::Error>(done())]);

        return Response::streaming(StatusCode::OK, history.chain(live).chain(ended))
            .header("content-type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            // Otherwise a reverse proxy buffers until the job ends.
            .header("X-Accel-Buffering", "no");
    }

    // Otherwise it's finished, and complete, in the database.
    let chunks = match queue::read_logs(&db, &job_id, -1).await {
        Ok(chunks) => chunks,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let mut body: Vec<Bytes> = chunks.iter().map(|chunk| frame(chunk, format)).collect();
    body.push(done());

    Response::streaming(
        StatusCode::OK,
        stream_iter(body.into_iter().map(Ok::<_, std::io::Error>)),
    )
    .header("content-type", "text/event-stream")
    .header("Cache-Control", "no-cache")
    .header("X-Accel-Buffering", "no")
}

/// The log as plain lines, no SSE framing - what "open raw" and `grep`-piping want.
/// A running job's output still streams; the response just stays open until it ends.
#[get("/api/v1/jobs/{id}/raw")]
pub async fn raw_logs(
    OptionalClaims(claims): OptionalClaims,
    Path(job_id): Path<String>,
    Inject(db): Inject<Db>,
    Inject(config): Inject<JwtConfig>,
    Inject(executor): Inject<Executor>,
) -> Response {
    if let Err(response) = authorize_job_read(claims.as_ref(), &config, &db, &job_id).await {
        return response;
    }

    let handle = Handle::new(job_id.clone());

    if let Ok(tail) = executor.0.logs(&handle).await {
        let history = stream_iter(tail.history.into_iter().map(|chunk| Ok(raw_line(&chunk))));

        let live = BroadcastStream::new(tail.live).map(|message| match message {
            Ok(chunk) => Ok(raw_line(&chunk)),
            Err(_) => Ok::<_, std::io::Error>(Bytes::from(
                "\n[some lines were skipped; reload to see them]\n",
            )),
        });

        return Response::streaming(StatusCode::OK, history.chain(live))
            .header("content-type", "text/plain; charset=utf-8")
            .header("Cache-Control", "no-cache")
            .header("X-Accel-Buffering", "no");
    }

    let chunks = match queue::read_logs(&db, &job_id, -1).await {
        Ok(chunks) => chunks,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let body: Vec<Bytes> = chunks.iter().map(raw_line).collect();

    Response::streaming(
        StatusCode::OK,
        stream_iter(body.into_iter().map(Ok::<_, std::io::Error>)),
    )
    .header("content-type", "text/plain; charset=utf-8")
}

/// One log line as written - a raw view has nothing else to end the line with.
pub fn raw_line(chunk: &LogChunk) -> Bytes {
    Bytes::from(format!("{}\n", chunk.line))
}

/// Says the log is complete, so a reader stops rather than reconnecting.
pub fn done() -> Bytes {
    Bytes::from("event: done\ndata: end of log\n\n")
}

/// One line as an SSE frame - embedded newlines are flattened, or they'd end the frame early.
pub fn frame(chunk: &LogChunk, format: Format) -> Bytes {
    let data = match format {
        Format::Text => chunk.line.replace(['\n', '\r'], " "),
        Format::Html => html_line(chunk),
    };

    Bytes::from(format!(
        "id: {}\nevent: {}\ndata: {}\n\n",
        chunk.seq,
        chunk.stream.as_str(),
        data
    ))
}

/// Built through `Element` (which escapes), since build output can contain `<script>`.
fn html_line(chunk: &LogChunk) -> String {
    let class = match chunk.stream {
        LogStream::Stdout => "log-line",
        LogStream::Stderr => "log-line log-stderr",
    };

    span()
        .class(class)
        .text(&chunk.line)
        .render()
        .replace(['\n', '\r'], " ")
}

pub fn register_routes() {
    let _ = stream_logs as fn(_, _, _, _, _, _) -> _;
    let _ = raw_logs as fn(_, _, _, _, _) -> _;
}
