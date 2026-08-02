//! Watching a job's output as it happens.
//!
//! A running job's log lives in the executor that is running it; a finished
//! job's lives in the database, written when it ended. This endpoint serves
//! whichever applies, so a caller does not have to know which.
//!
//! One caveat, and it is the phase-4 log-persistence trade showing through:
//! only the replica actually running a job holds its live output. With several
//! replicas behind one address, a browser can land on one that is not running
//! the job and will see nothing until it finishes. Single-replica deployments -
//! which is every one of these so far - are unaffected.

use crate::executors::{Handle, JobExecutor, LogChunk, Stream as LogStream};
use crate::routers::api::authz::can_on_project;
use crate::routers::api::{ApiError, json_error};
use crate::scheduler::queue;
use actix_web::http::StatusCode;
use actix_web::{Error, HttpRequest, HttpResponse, Responder, get, web};
use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::stream::iter as stream_iter;
use quench_db::prelude::Db;
use quench_web::prelude::*;
use serde::Deserialize;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;

/// What a frame's `data:` carries.
///
/// A query parameter rather than content negotiation, because the choice is
/// forced by the mechanism: htmx's SSE extension uses `EventSource`, which
/// sends `Accept: text/event-stream` and gives the page no way to ask for
/// anything else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// The line as it was written. What `conveyor logs --follow` reads.
    #[default]
    Text,
    /// An escaped `<span>`, for a page that appends each frame with
    /// `hx-swap="beforeend"`.
    Html,
}

#[derive(Debug, Default, Deserialize)]
pub struct StreamQuery {
    #[serde(default)]
    pub format: Format,
}

/// Server-sent events, one per log line.
///
/// `id:` carries the sequence number, so a browser reconnecting sends
/// `Last-Event-ID` and can be given everything after it rather than the whole
/// log again.
#[get("/jobs/{id}/stream")]
pub async fn stream_logs(
    request: HttpRequest,
    path: web::Path<String>,
    query: web::Query<StreamQuery>,
    db: web::Data<Db>,
    executor: web::Data<Arc<dyn JobExecutor>>,
) -> impl Responder {
    let job_id = path.into_inner();

    match crate::scheduler::queue::repo_id_for_job(&db, &job_id).await {
        Ok(Some(repo_id)) => match crate::scheduler::repos::read(&db, &repo_id).await {
            Ok(Some(repo)) => {
                if !can_on_project(&request, &db, &repo.project_id, "read").await {
                    return json_error(StatusCode::FORBIDDEN, "no read access to this job's logs");
                }
            }
            Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such job"),
            Err(error) => return ApiError::from(error).into_response(),
        },
        Ok(None) => return json_error(StatusCode::NOT_FOUND, "no such job"),
        Err(error) => return ApiError::from(error).into_response(),
    }

    let format = query.format;
    let handle = Handle::new(job_id.clone());

    // The executor first: if it knows this job, the job is still running here
    // and its output is not in the database yet.
    if let Ok(tail) = executor.logs(&handle).await {
        let history = stream_iter(
            tail.history
                .into_iter()
                .map(move |chunk| Ok(frame(&chunk, format))),
        );

        let live = BroadcastStream::new(tail.live).map(move |message| match message {
            Ok(chunk) => Ok(frame(&chunk, format)),
            // The subscriber fell behind the channel. The lines are not lost -
            // they are in the history the next connection will replay - so say
            // so rather than pretending the stream is intact.
            Err(_) => Ok::<_, Error>(Bytes::from(
                "event: lagged\ndata: some lines were skipped; reload to see them\n\n",
            )),
        });

        // The channel closes when the scheduler forgets the job, which it does
        // once the log is in the database. Saying so beats letting the
        // connection simply end - a browser reads that as a drop and
        // reconnects, and would fetch the whole log again.
        let ended = stream_iter([Ok::<_, Error>(done())]);

        return HttpResponse::Ok()
            .content_type("text/event-stream")
            .append_header(("Cache-Control", "no-cache"))
            // Without this a reverse proxy will sit on the response until the
            // job ends, which is exactly what streaming is for avoiding.
            .append_header(("X-Accel-Buffering", "no"))
            .streaming(history.chain(live).chain(ended));
    }

    // Otherwise it is finished, and complete, in the database.
    let chunks = match queue::read_logs(&db, &job_id, -1).await {
        Ok(chunks) => chunks,
        Err(error) => return ApiError::from(error).into_response(),
    };

    let mut body: Vec<Bytes> = chunks.iter().map(|chunk| frame(chunk, format)).collect();
    body.push(done());

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .append_header(("Cache-Control", "no-cache"))
        .append_header(("X-Accel-Buffering", "no"))
        .streaming(stream_iter(body.into_iter().map(Ok::<_, Error>)))
}

/// Says the log is complete, so a reader stops rather than reconnecting.
pub fn done() -> Bytes {
    Bytes::from("event: done\ndata: end of log\n\n")
}

/// One line, as an SSE frame.
///
/// A line containing a newline would end the frame early and the rest would be
/// read as a new event, so any that survived the reader are flattened.
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

/// A log line as an escaped element.
///
/// Built through `Element`, whose `text` escapes, rather than by formatting a
/// string: build output is written by whoever owns the repository, and a line
/// containing `<script>` must reach the page as characters rather than as a
/// tag.
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
