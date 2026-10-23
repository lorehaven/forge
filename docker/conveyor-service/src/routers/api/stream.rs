//! Watching a job's output live (executor) or after the fact (database).
//! Caveat: only the replica actually running a job holds its live output.

use crate::executors::{Executor, Handle, LogChunk, Stream as LogStream};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, OptionalClaims, json_error};
use crate::scheduler::queue;
use quench_auth::domain::jwt::JwtConfig;
use quench_core::bytes::Bytes;
use quench_core::stream::{Stream, StreamExt, iter as stream_iter, unfold};
use quench_db::prelude::Db;
use quench_http::prelude::{Inject, Path, Query, Response, get, http::StatusCode};
use quench_serde::Deserialize;
use quench_web::prelude::*;
use std::sync::Arc;
use tokio::sync::broadcast::{Receiver, error::RecvError};

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

/// A running job's broadcast channel as a body stream: each chunk rendered, a gap in the channel (the
/// subscriber fell behind) replaced by `lagged`, the end of the job ending the stream.
fn live_stream(
    receiver: Receiver<LogChunk>,
    render: impl Fn(&LogChunk) -> Bytes + Send + Sync + 'static,
    lagged: &'static str,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + Unpin + 'static {
    let render = Arc::new(render);
    unfold(receiver, move |mut receiver| {
        let render = Arc::clone(&render);
        async move {
            let item = match receiver.recv().await {
                Ok(chunk) => render(&chunk),
                Err(RecvError::Lagged(_)) => Bytes::from(lagged),
                Err(RecvError::Closed) => return None,
            };
            Some((Ok::<_, std::io::Error>(item), receiver))
        }
    })
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

        // A subscriber that fell behind the channel is told so rather than left to pretend.
        let live = live_stream(
            tail.live,
            move |chunk| frame(chunk, format),
            "event: lagged\ndata: some lines were skipped; reload to see them\n\n",
        );

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

    let body = chunks
        .into_iter()
        .map(move |chunk| frame(&chunk, format))
        .chain(std::iter::once(done()))
        .map(Ok::<_, std::io::Error>);

    Response::streaming(StatusCode::OK, stream_iter(body))
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

        let live = live_stream(
            tail.live,
            raw_line,
            "\n[some lines were skipped; reload to see them]\n",
        );

        return Response::streaming(StatusCode::OK, history.chain(live))
            .header("content-type", "text/plain; charset=utf-8")
            .header("Cache-Control", "no-cache")
            .header("X-Accel-Buffering", "no");
    }

    let chunks = match queue::read_logs(&db, &job_id, -1).await {
        Ok(chunks) => chunks,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let body = chunks
        .into_iter()
        .map(|chunk| raw_line(&chunk))
        .map(Ok::<_, std::io::Error>);

    Response::streaming(StatusCode::OK, stream_iter(body))
        .header("content-type", "text/plain; charset=utf-8")
}

/// One log line as written - a raw view has nothing else to end the line with.
#[must_use]
pub fn raw_line(chunk: &LogChunk) -> Bytes {
    Bytes::from(format!("{}\n", chunk.line))
}

/// Says the log is complete, so a reader stops rather than reconnecting.
#[must_use]
pub fn done() -> Bytes {
    Bytes::from("event: done\ndata: end of log\n\n")
}

/// One line as an SSE frame - embedded newlines are flattened, or they'd end the frame early.
#[must_use]
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
