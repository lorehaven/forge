//! Unit tests for `routers/api/stream.rs`.
//!
//! The frames are asserted directly rather than by reading the endpoint's body.
//! A live stream is meant not to end - that is the whole point of it - so
//! draining one in a test hangs, which is how this file was written the first
//! time.
//!
//! The formatting is where the behaviour worth testing lives: the two audiences
//! this endpoint serves, and the fact that only one of them is escaped.

use conveyor_service::executors::{LogChunk, Stream};
use conveyor_service::routers::api::stream::{Format, done, frame};

fn chunk(seq: u64, stream: Stream, line: &str) -> LogChunk {
    LogChunk {
        seq,
        stream,
        line: line.to_string(),
        at: chrono::Utc::now(),
    }
}

fn rendered(chunk: &LogChunk, format: Format) -> String {
    String::from_utf8_lossy(&frame(chunk, format)).to_string()
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

#[test]
fn the_default_format_is_the_line_as_it_was_written() {
    // What `conveyor logs --follow` parses.
    let text = rendered(&chunk(0, Stream::Stdout, "compiling thing"), Format::Text);

    assert!(text.contains("data: compiling thing\n"), "{text}");
    assert!(!text.contains("<span"), "{text}");
}

#[test]
fn every_frame_carries_its_sequence_number_and_stream() {
    // The sequence number is what lets a reconnecting reader resume rather than
    // replay the whole log.
    let out = rendered(&chunk(7, Stream::Stdout, "x"), Format::Text);
    assert!(out.contains("id: 7\n"), "{out}");
    assert!(out.contains("event: stdout\n"), "{out}");

    let err = rendered(&chunk(8, Stream::Stderr, "x"), Format::Text);
    assert!(err.contains("event: stderr\n"), "{err}");
}

#[test]
fn the_html_format_wraps_each_line_in_an_element() {
    // The run page appends these with `hx-swap="beforeend"`.
    let html = rendered(&chunk(0, Stream::Stdout, "compiling thing"), Format::Html);

    assert!(html.contains("<span"), "{html}");
    assert!(html.contains("log-line"), "{html}");
    assert!(html.contains("compiling thing"), "{html}");
}

#[test]
fn stderr_is_distinguishable_in_the_html_format() {
    // Both event names swap into the same element, so the class is the only
    // thing that tells them apart once they are on the page.
    let html = rendered(&chunk(0, Stream::Stderr, "it broke"), Format::Html);
    assert!(html.contains("log-stderr"), "{html}");
}

#[test]
fn the_stream_ends_by_saying_so() {
    // A connection that simply closes reads as a drop, and the browser fetches
    // the whole log again.
    let ending = String::from_utf8_lossy(&done()).to_string();
    assert!(ending.contains("event: done"), "{ending}");
}

// ---------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------

#[test]
fn html_frames_escape_the_build_output() {
    // Build output is written by whoever owns the repository. A line holding a
    // tag has to reach the page as characters, not as markup - otherwise a
    // `.conveyor.toml` that echoes a script tag runs it in the browser of
    // whoever looks at the run.
    let html = rendered(
        &chunk(0, Stream::Stdout, "<script>alert('pwned')</script>"),
        Format::Html,
    );

    assert!(
        !html.contains("<script>"),
        "the build's tag reached the page intact: {html}"
    );
    assert!(html.contains("&lt;script&gt;"), "{html}");
}

#[test]
fn an_ampersand_in_the_output_is_escaped_too() {
    let html = rendered(
        &chunk(0, Stream::Stdout, "cargo build && cargo test"),
        Format::Html,
    );
    assert!(html.contains("&amp;&amp;"), "{html}");
}

#[test]
fn a_line_cannot_close_the_element_it_is_put_in() {
    // The other half of the same problem: escaping the opening tag is no use
    // if the line can end the span and start its own.
    let html = rendered(
        &chunk(0, Stream::Stdout, "</span><img src=x onerror=alert(1)>"),
        Format::Html,
    );

    assert!(!html.contains("</span><img"), "{html}");
    assert!(html.contains("&lt;/span&gt;"), "{html}");
}

#[test]
fn the_text_format_is_deliberately_not_escaped() {
    // It is read by a terminal, where `&lt;` would be wrong rather than safe.
    let text = rendered(&chunk(0, Stream::Stdout, "a < b && c"), Format::Text);
    assert!(text.contains("data: a < b && c\n"), "{text}");
}

// ---------------------------------------------------------------------------
// Frame integrity
// ---------------------------------------------------------------------------

#[test]
fn a_newline_inside_a_line_cannot_split_the_frame() {
    // A blank line ends an SSE frame. One that survived the reader would make
    // the rest of the line arrive as a separate, malformed event.
    for format in [Format::Text, Format::Html] {
        let out = rendered(&chunk(0, Stream::Stdout, "first\nsecond"), format);
        let frames: Vec<&str> = out.split("\n\n").filter(|f| !f.trim().is_empty()).collect();
        assert_eq!(frames.len(), 1, "{format:?} split into {frames:?}");
    }
}

#[test]
fn a_carriage_return_cannot_split_the_frame_either() {
    // Progress bars emit them constantly.
    for format in [Format::Text, Format::Html] {
        let out = rendered(&chunk(0, Stream::Stdout, "50%\r100%"), format);
        assert!(!out.contains('\r'), "{format:?}: {out}");
    }
}
