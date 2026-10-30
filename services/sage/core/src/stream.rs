//! The events of a streamed answer: what the service sends on `GET /api/v1/turns/{id}/stream` and the browser's `EventSource`
//! receives. Each is a named event whose data is JSON, so the page never has to trust markup from the stream.
//!
//! `status` and `delta` come first and keep coming while the answer is written; `replace` swaps the text for a checked one;
//! the stream ends with `done` or `failure` (`error` is the browser's own name for a lost connection, so it is not used).

use quench_serde::json;
use quench_serde::{Deserialize, Serialize};

/// The names the browser has to be told to listen for.
pub const EVENT_NAMES: [&str; 5] = ["status", "delta", "replace", "done", "failure"];

/// What the grounded pipeline is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// `planning`, `searching`, `reading` or `checking`.
    pub stage: String,
    /// The search queries, for `searching`.
    pub detail: Option<String>,
}

/// The turn is stored: where the messages ended up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Done {
    pub conversation_id: String,
    /// The person's message (none for a regeneration).
    pub user_message_id: Option<String>,
    pub message_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Text {
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Failure {
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Status(Status),
    /// More of the answer.
    Delta(String),
    /// The answer as it stands now, replacing what came.
    Replace(String),
    Done(Done),
    Failure(String),
}

impl StreamEvent {
    /// The SSE event name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Status(_) => "status",
            Self::Delta(_) => "delta",
            Self::Replace(_) => "replace",
            Self::Done(_) => "done",
            Self::Failure(_) => "failure",
        }
    }

    /// The JSON on the event's `data:` line. It has no line break.
    #[must_use]
    pub fn data(&self) -> String {
        match self {
            Self::Status(status) => json::to_string(status),
            Self::Delta(text) | Self::Replace(text) => {
                json::to_string(&Text { text: text.clone() })
            }
            Self::Done(done) => json::to_string(done),
            Self::Failure(message) => json::to_string(&Failure {
                message: message.clone(),
            }),
        }
    }

    /// The event as the wire frames it: `event:`, one `data:` line, a blank line.
    #[must_use]
    pub fn frame(&self) -> String {
        format!("event: {}\ndata: {}\n\n", self.name(), self.data())
    }

    /// Reads an event the browser received; `None` for a name or data that are not Sage's.
    #[must_use]
    pub fn parse(name: &str, data: &str) -> Option<Self> {
        match name {
            "status" => json::from_str(data).ok().map(Self::Status),
            "delta" => json::from_str::<Text>(data)
                .ok()
                .map(|t| Self::Delta(t.text)),
            "replace" => json::from_str::<Text>(data)
                .ok()
                .map(|t| Self::Replace(t.text)),
            "done" => json::from_str(data).ok().map(Self::Done),
            "failure" => json::from_str::<Failure>(data)
                .ok()
                .map(|f| Self::Failure(f.message)),
            _ => None,
        }
    }
}
