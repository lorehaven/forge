---
title: Events
parent: Conveyor
grand_parent: Services
nav_order: 7
---

Conveyor streams one thing live: a job's log. There is no other SSE or WebSocket endpoint. Sources: `src/routers/api/stream.rs`, `conveyor-web/src/browser.rs`.

## `GET /api/v1/jobs/{id}/stream`

Authorisation: read on the job's repository (`403 no read access to this job's logs`, `404 no such job`), checked before anything is sent.

The handler asks the executor first. If the executor still knows the job (it is running on **this** replica), the response is its history so far, then every new line as it is produced, then the terminator. Otherwise the job is read from the database (complete, written when the job ended) and the response is every stored line followed by the terminator. A job running on another replica therefore shows its stored log, which is empty until the job ends; the live output is only on the replica that runs it.

Response headers: `content-type: text/event-stream`, `Cache-Control: no-cache`, `X-Accel-Buffering: no` (so a proxy does not buffer until the job ends).

Frame format (each ends with a blank line):

```text
id: 7
event: stdout
data: compiling thing

```

* `id` is the line's `seq`, contiguous per job.
* `event` is `stdout` or `stderr`.
* `data` is the line. In `text` format (default) CR and LF inside the line are replaced by spaces, so a line cannot end a frame early. With `?format=html` it is `<span class="log-line">...</span>` (`log-line log-stderr` for stderr) with `& < > " '` escaped as `&amp; &lt; &gt; &quot; &#39;` and CR/LF as spaces, meant for `hx-swap="beforeend"` style appending.
* `event: lagged` / `data: some lines were skipped; reload to see them` is sent when the subscriber fell behind the 1024-line channel.
* `event: done` / `data: end of log` ends every stream, so a browser's `EventSource` stops instead of reconnecting and reading the log twice.

The reconnect hint in the source comment (`Last-Event-ID`) is not implemented: the handler does not read that header, a reconnect starts again from the first line.

Example (the frames asserted by `tests/unit/routers_api_stream_tests.rs`):

```text
id: 7
event: stdout
data: compiling thing

event: done
data: end of log

```

## `GET /api/v1/jobs/{id}/raw`

The same authorisation. `content-type: text/plain; charset=utf-8`. Each line followed by `\n`, no framing. For a running job the response stays open until the job ends (and, on lag, contains `\n[some lines were skipped; reload to see them]\n`); for a finished one it ends after the stored lines. No `done` marker.

## `GET /api/v1/jobs/{id}/logs`

Not streaming: a JSON array of the stored lines (`seq`, `stream`, `line`, `at`), empty while the job runs.

## The browser

`conveyor-web` opens `EventSource` on `{base}/api/v1/jobs/{id}/stream` (text format) when a job's disclosure is opened, listening for `stdout`, `stderr`, `done`, `lagged`. A `lagged` event is shown as a stderr line. `done` closes the source (no reconnect) and marks the log complete; leaving the run page closes every open stream. See [Client](client.md).
