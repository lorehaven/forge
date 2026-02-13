use crate::config::UiTheme;
use crate::core::ExecutionPlan;
use crate::ui::interface::InteractionHandler;
use crate::ui::render::ModelLoadPhase;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_EVENTS: usize = 500;
const MAX_CALLS: usize = 50;

#[derive(Debug, Clone, Serialize)]
pub struct UiEvent {
    pub level: String,
    pub message: String,
    pub call_id: u64,
    pub step_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallEntry {
    pub id: u64,
    pub prompt: String,
    pub status: String,
    pub session_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebSnapshot {
    pub busy: bool,
    pub current_step: Option<usize>,
    pub current_call_id: u64,
    pub cwd: String,
    pub theme: UiTheme,
    pub plan: Option<ExecutionPlan>,
    pub events: Vec<UiEvent>,
    pub call_history: Vec<CallEntry>,
}

#[derive(Debug, Default)]
struct WebUiState {
    busy: bool,
    current_step: Option<usize>,
    current_call_id: u64,
    theme: UiTheme,
    plan: Option<ExecutionPlan>,
    events: VecDeque<UiEvent>,
    call_history: VecDeque<CallEntry>,
    stream_buffer: String,
    tool_stream_buffer: String,
    code_stream_buffer: String,
}

impl WebUiState {
    fn push_event(&mut self, level: &str, message: impl Into<String>) {
        self.events.push_back(UiEvent {
            level: level.to_string(),
            message: message.into(),
            call_id: self.current_call_id,
            step_id: self.current_step,
        });
        while self.events.len() > MAX_EVENTS {
            let _ = self.events.pop_front();
        }
    }

    fn flush_stream_buffer(&mut self) {
        let content = self.stream_buffer.trim_end().to_string();
        if !content.is_empty() {
            self.push_event("assistant", content);
        }
        self.stream_buffer.clear();
    }

    fn flush_tool_buffer(&mut self) {
        let content = self.tool_stream_buffer.trim_end().to_string();
        if !content.is_empty() {
            self.push_event("tool_call", content);
        }
        self.tool_stream_buffer.clear();
    }

    fn flush_code_buffer(&mut self) {
        let content = self.code_stream_buffer.trim_end().to_string();
        if !content.is_empty() {
            self.push_event("code", content);
        }
        self.code_stream_buffer.clear();
    }
}

#[derive(Clone, Debug)]
pub struct WebMode {
    state: Arc<Mutex<WebUiState>>,
    cwd: String,
}

impl WebMode {
    #[must_use]
    pub fn new(cwd: String, theme: UiTheme) -> Self {
        Self {
            state: Arc::new(Mutex::new(WebUiState {
                theme,
                ..WebUiState::default()
            })),
            cwd,
        }
    }

    pub fn set_theme(&self, theme: UiTheme) {
        self.with_state(|state| {
            state.theme = theme;
        });
    }

    #[must_use]
    pub fn clear_history(&self) -> bool {
        self.with_state(|state| {
            if state.busy {
                return false;
            }
            state.events.clear();
            state.call_history.clear();
            state.plan = None;
            state.current_step = None;
            true
        })
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut WebUiState) -> T) -> T {
        match self.state.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                f(&mut guard)
            }
        }
    }

    #[must_use]
    pub fn try_start_request(&self, prompt: &str) -> bool {
        self.with_state(|state| {
            if state.busy {
                return false;
            }

            state.busy = true;
            state.current_call_id = state.current_call_id.saturating_add(1);
            state.current_step = None;
            state.plan = None;
            state.stream_buffer.clear();
            state.tool_stream_buffer.clear();
            state.code_stream_buffer.clear();
            state.call_history.push_back(CallEntry {
                id: state.current_call_id,
                prompt: prompt.to_string(),
                status: "running".to_string(),
                session_file: None,
            });
            while state.call_history.len() > MAX_CALLS {
                let _ = state.call_history.pop_front();
            }
            state.push_event("info", format!("User prompt: {prompt}"));
            true
        })
    }

    pub fn finish_request(&self, error: Option<&str>) {
        self.with_state(|state| {
            state.flush_stream_buffer();
            state.flush_tool_buffer();
            state.flush_code_buffer();
            if let Some(err) = error {
                state.push_event("error", err);
                if let Some(entry) = state.call_history.back_mut() {
                    entry.status = "error".to_string();
                }
            } else {
                state.push_event("info", "Done");
                if let Some(entry) = state.call_history.back_mut() {
                    entry.status = "done".to_string();
                }
            }
            state.busy = false;
            state.current_step = None;
        });
    }

    #[must_use]
    pub fn snapshot(&self) -> WebSnapshot {
        self.with_state(|state| WebSnapshot {
            busy: state.busy,
            current_step: state.current_step,
            current_call_id: state.current_call_id,
            cwd: self.cwd.clone(),
            theme: state.theme,
            plan: state.plan.clone(),
            events: state.events.iter().cloned().collect(),
            call_history: state.call_history.iter().cloned().collect(),
        })
    }

    pub fn set_latest_call_session_file(&self, filename: Option<String>) {
        self.with_state(|state| {
            if let Some(entry) = state.call_history.back_mut() {
                entry.session_file.clone_from(&filename);
            }
            if let Some(file) = filename {
                state.push_event("info", format!("Session autosaved: {file}"));
            }
        });
    }
}

impl InteractionHandler for WebMode {
    fn set_current_step(&self, step_id: Option<usize>) {
        self.with_state(|state| {
            state.current_step = step_id;
        });
    }

    fn render_plan(&self, plan: &ExecutionPlan) {
        self.with_state(|state| {
            state.plan = Some(plan.clone());
        });
    }

    fn render_model_progress(&self, phase: ModelLoadPhase) {
        let label = match phase {
            ModelLoadPhase::StartingServer => "Starting model server",
            ModelLoadPhase::WaitingForPort => "Waiting for model server",
            ModelLoadPhase::Ready => "Model ready",
        };
        self.with_state(|state| state.push_event("info", label));
    }

    fn print_message(&self, message: &str) {
        self.with_state(|state| state.push_event("message", message));
    }

    fn print_error(&self, error: &str) {
        self.with_state(|state| state.push_event("error", error));
    }

    fn print_info(&self, info: &str) {
        self.with_state(|state| state.push_event("info", info));
    }

    fn print_response(&self, response: &str) {
        self.with_state(|state| state.push_event("tool_output", response));
    }

    fn print_stream_start(&self) {
        self.with_state(|state| state.stream_buffer.clear());
    }

    fn print_stream_chunk(&self, chunk: &str) {
        self.with_state(|state| state.stream_buffer.push_str(chunk));
    }

    fn print_stream_end(&self) {
        self.with_state(WebUiState::flush_stream_buffer);
    }

    fn print_stream_code_start(&self, lang: &str) {
        self.with_state(|state| {
            state.code_stream_buffer.clear();
            if lang.is_empty() {
                state.code_stream_buffer.push_str("```\n");
            } else {
                state.code_stream_buffer.push_str("```");
                state.code_stream_buffer.push_str(lang);
                state.code_stream_buffer.push('\n');
            }
        });
    }

    fn print_stream_code_chunk(&self, chunk: &str) {
        self.with_state(|state| state.code_stream_buffer.push_str(chunk));
    }

    fn print_stream_code_end(&self) {
        self.with_state(|state| {
            state.code_stream_buffer.push_str("\n```");
            state.flush_code_buffer();
        });
    }

    fn print_stream_tool_start(&self) {
        self.with_state(|state| state.tool_stream_buffer.clear());
    }

    fn print_stream_tool_chunk(&self, chunk: &str) {
        self.with_state(|state| state.tool_stream_buffer.push_str(chunk));
    }

    fn print_stream_tool_end(&self) {
        self.with_state(WebUiState::flush_tool_buffer);
    }

    fn print_debug(&self, message: &str) {
        self.with_state(|state| state.push_event("debug", message));
    }
}

pub const EMBEDDED_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Ferrous Web UI</title>
  <style>
    :root {
      --bg: #0b1320;
      --panel: #111a2b;
      --line: #243247;
      --ink: #e6edf8;
      --muted: #9fb0c9;
      --brand: #1d4ed8;
      --ok: #0f766e;
      --warn: #b45309;
      --bad: #b91c1c;
      --events-bg: #090f1b;
      --events-ink: #d1d5db;
      --events-line: #1f2937;
      --evt-info: #60a5fa;
      --evt-error: #f87171;
      --evt-debug: #a78bfa;
      --evt-tool: #f59e0b;
      --evt-assistant: #34d399;
      --evt-code: #22d3ee;
      --evt-message: #94a3b8;
      --shadow: rgba(2, 8, 23, 0.35);
      --bg-mid: #101a2b;
      --bg-end: #0e1726;
      --page-max-width: 1560px;
      --side-panel-width: 430px;
    }
    body[data-theme="light"] {
      --bg: #eef2f7;
      --panel: #ffffff;
      --line: #d9dee5;
      --ink: #1f2937;
      --muted: #4b5563;
      --events-bg: #0b1220;
      --events-ink: #d1d5db;
      --events-line: #111827;
      --evt-info: #1d4ed8;
      --evt-error: #b91c1c;
      --evt-debug: #7c3aed;
      --evt-tool: #b45309;
      --evt-assistant: #047857;
      --evt-code: #0e7490;
      --evt-message: #475569;
      --shadow: rgba(15, 23, 42, 0.05);
      --bg-mid: #f8fafc;
      --bg-end: #f3f5f7;
      --page-max-width: 1560px;
      --side-panel-width: 430px;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: "Segoe UI", Tahoma, Geneva, Verdana, sans-serif;
      color: var(--ink);
      background: linear-gradient(180deg, var(--bg), var(--bg-mid) 38%, var(--bg-end));
    }
    .container {
      width: min(var(--page-max-width), calc(100% - 2rem));
      margin: 1rem auto;
      display: grid;
      gap: 1rem;
      grid-template-rows: auto 1fr auto;
      height: calc(100vh - 2rem);
    }
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
      box-shadow: 0 8px 30px var(--shadow);
    }
    .card-body { padding: 1rem; }
    h1 { margin: 0; font-size: 1.15rem; color: var(--ink); }
    .muted { color: var(--muted); font-size: 0.9rem; }
    .main-grid {
      display: grid;
      gap: 1rem;
      grid-template-columns: minmax(0, 1fr) var(--side-panel-width);
      min-height: 0;
      height: 100%;
      align-items: stretch;
    }
    .side-stack {
      display: grid;
      grid-template-rows: 1fr 1fr;
      gap: 1rem;
      min-height: 0;
      width: var(--side-panel-width);
    }
    .side-stack .card {
      min-height: 0;
      display: flex;
    }
    .side-stack .card-body {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-height: 0;
    }
    .main-grid > .card {
      min-height: 0;
    }
    .events-card {
      display: flex;
      min-height: 0;
    }
    .events-card .card-body {
      display: flex;
      flex-direction: column;
      flex: 1;
      min-height: 0;
    }
    .log {
      background: var(--events-bg);
      color: var(--events-ink);
      border-radius: 8px;
      border: 1px solid var(--events-line);
      padding: 0.75rem;
      flex: 1;
      min-height: 0;
      max-height: 100%;
      max-width: 100%;
      overflow: auto;
      white-space: normal;
      font-family: "Segoe UI", Tahoma, Geneva, Verdana, sans-serif;
      line-height: 1.34;
    }
    .events-empty {
      color: var(--muted);
      font-size: 0.9rem;
      padding: 0.25rem 0.15rem;
    }
    .event-item {
      border: 1px solid var(--events-line);
      border-left: 3px solid var(--events-line);
      border-radius: 8px;
      padding: 0.55rem 0.65rem;
      margin-bottom: 0.5rem;
      background: rgba(15, 23, 42, 0.35);
      width: 100%;
      max-width: 100%;
    }
    .event-item:last-child {
      margin-bottom: 0;
    }
    body[data-theme="light"] .event-item {
      background: rgba(148, 163, 184, 0.08);
    }
    .event-head {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 0.45rem;
      gap: 0.5rem;
    }
    .event-level {
      font-size: 0.74rem;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.03em;
      border-radius: 999px;
      border: 1px solid currentColor;
      padding: 0.08rem 0.45rem;
    }
    .event-body {
      font-family: "Consolas", "Menlo", monospace;
      white-space: pre-wrap;
      word-break: break-word;
      font-size: 0.9rem;
    }
    .event-item.info { border-left-color: var(--evt-info); }
    .event-item.info .event-level { color: var(--evt-info); }
    .event-item.error { border-left-color: var(--evt-error); }
    .event-item.error .event-level { color: var(--evt-error); }
    .event-item.debug { border-left-color: var(--evt-debug); }
    .event-item.debug .event-level { color: var(--evt-debug); }
    .event-item.assistant { border-left-color: var(--evt-assistant); }
    .event-item.assistant .event-level { color: var(--evt-assistant); }
    .event-item.tool_call,
    .event-item.tool_output { border-left-color: var(--evt-tool); }
    .event-item.tool_call .event-level,
    .event-item.tool_output .event-level { color: var(--evt-tool); }
    .event-item.message { border-left-color: var(--evt-message); }
    .event-item.message .event-level { color: var(--evt-message); }
    .event-item.code { border-left-color: var(--evt-code); }
    .event-item.code .event-level { color: var(--evt-code); }
    .event-code-block {
      margin: 0.4rem 0 0;
      border: 1px solid var(--events-line);
      border-radius: 6px;
      background: #030712;
      overflow: auto;
    }
    .event-code-block code {
      display: block;
      padding: 0.6rem 0.7rem;
      color: #e2e8f0;
      font-family: "Consolas", "Menlo", monospace;
      font-size: 0.87rem;
      line-height: 1.35;
      white-space: pre;
    }
    .event-code-block .code-line {
      display: block;
    }
    .event-code-block .line-meta { color: #93c5fd; }
    .event-code-block .line-hunk { color: #c4b5fd; }
    .event-code-block .line-add { color: #86efac; background: rgba(22, 101, 52, 0.22); }
    .event-code-block .line-del { color: #fca5a5; background: rgba(127, 29, 29, 0.24); }
    .event-code-block .tok-kw { color: #93c5fd; font-weight: 600; }
    .event-code-block .tok-str { color: #fbbf24; }
    .event-code-block .tok-num { color: #67e8f9; }
    .event-code-block .tok-bool { color: #f9a8d4; }
    .event-code-block .tok-key { color: #c4b5fd; }
    .event-code-block .tok-cmt { color: #94a3b8; font-style: italic; }
    .event-code-label {
      font-size: 0.72rem;
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.03em;
      color: var(--muted);
      margin-top: 0.35rem;
    }
    #plan {
      margin-top: 0.6rem;
      overflow: auto;
      flex: 1;
      min-height: 0;
    }
    #history {
      margin-top: 0.6rem;
      overflow: auto;
      flex: 1;
      min-height: 0;
    }
    .history ul {
      margin: 0.5rem 0 0;
      padding-left: 1.1rem;
    }
    .history li {
      margin: 0.35rem 0;
      line-height: 1.25;
      border: 1px solid var(--line);
      border-radius: 7px;
      padding: 0.35rem 0.45rem;
      cursor: pointer;
    }
    .plan li { cursor: pointer; border-radius: 7px; padding: 0.2rem 0.3rem; border: 1px solid transparent; }
    .plan li:hover, .history li:hover { border-color: var(--line); background: rgba(148, 163, 184, 0.08); }
    .plan li.active, .history li.active { border-color: var(--brand); background: rgba(29, 78, 216, 0.12); }
    .history-item-line {
      display: block;
      word-break: break-word;
    }
    .history-item-meta {
      color: var(--muted);
      font-size: 0.8rem;
      display: block;
      margin-top: 0.12rem;
    }
    .plan {
      display: flex;
      flex-direction: column;
      min-height: 0;
    }
    .plan ul { margin: 0.5rem 0 0; padding-left: 1.2rem; }
    .plan li { margin: 0.25rem 0; }
    .row { display: flex; gap: 0.6rem; align-items: center; justify-content: space-between; flex-wrap: wrap; }
    .row-left { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
    textarea {
      width: 100%;
      min-height: 96px;
      resize: vertical;
      padding: 0.7rem;
      border-radius: 8px;
      border: 1px solid var(--line);
      background: #0d1524;
      color: var(--ink);
      font: inherit;
    }
    body[data-theme="light"] textarea {
      background: #ffffff;
      color: var(--ink);
    }
    .btn {
      border: 0;
      border-radius: 8px;
      padding: 0.5rem 0.85rem;
      cursor: pointer;
      font-weight: 600;
      background: var(--brand);
      color: white;
    }
    .btn:disabled { background: #93c5fd; cursor: not-allowed; }
    .btn-secondary {
      background: transparent;
      color: var(--ink);
      border: 1px solid var(--line);
    }
    .select {
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 0.35rem 0.5rem;
      background: var(--panel);
      color: var(--ink);
    }
    #status { font-size: 0.9rem; color: var(--muted); }
    .pill {
      display: inline-block;
      border-radius: 999px;
      padding: 0.1rem 0.5rem;
      font-size: 0.75rem;
      border: 1px solid var(--line);
      margin-left: 0.4rem;
    }
    .running { color: var(--warn); border-color: #f59e0b; }
    .done { color: var(--ok); border-color: #10b981; }
    .failed { color: var(--bad); border-color: #ef4444; }
    @media (max-width: 960px) {
      .container { min-height: auto; }
      .main-grid { grid-template-columns: 1fr; }
      .log { min-height: 240px; }
    }
  </style>
</head>
<body>
  <div class="container">
    <div class="card"><div class="card-body">
      <div class="row">
        <div>
          <h1>Ferrous</h1>
          <div id="cwd" class="muted">Context: -</div>
        </div>
        <button class="btn btn-secondary" id="themeBtn">Light Mode</button>
      </div>
    </div></div>

    <div class="main-grid">
      <div class="card events-card"><div class="card-body">
        <div class="row">
          <strong>Events</strong>
          <div class="row-left">
            <div id="status">Idle</div>
          </div>
        </div>
        <div id="events" class="log"></div>
      </div></div>

      <div class="side-stack">
        <div class="card"><div class="card-body plan">
          <div class="row">
            <strong>Plan</strong>
            <span class="muted">Click step to focus</span>
          </div>
          <div id="plan" class="muted">No plan yet.</div>
        </div></div>

        <div class="card"><div class="card-body history">
          <div class="row">
            <strong>History</strong>
            <div class="row-left">
              <button class="btn btn-secondary" id="clearHistoryBtn">Clear History</button>
              <span class="muted">Click call to focus</span>
            </div>
          </div>
          <div id="history" class="muted">No calls yet.</div>
        </div></div>
      </div>
    </div>

    <div class="card"><div class="card-body">
      <label for="prompt"><strong>Prompt</strong></label>
      <textarea id="prompt" placeholder="Describe the coding task..."></textarea>
      <div class="row" style="margin-top:0.7rem;">
        <div class="row-left">
          <button class="btn" id="runBtn">Run</button>
        </div>
        <div class="muted">Prompt composer is docked at bottom</div>
      </div>
    </div></div>
  </div>
  <script>
    const statusEl = document.getElementById("status");
    const cwdEl = document.getElementById("cwd");
    const planEl = document.getElementById("plan");
    const eventsEl = document.getElementById("events");
    const promptEl = document.getElementById("prompt");
    const runBtn = document.getElementById("runBtn");
    const themeBtn = document.getElementById("themeBtn");
    const clearHistoryBtn = document.getElementById("clearHistoryBtn");
    const historyEl = document.getElementById("history");

    let hasRenderedEvents = false;
    let selectedStepId = null;
    let selectedCallId = null;

    function statusLabel(stepStatus) {
      if (stepStatus === "Running") return { cls: "running", label: "running" };
      if (stepStatus === "Done") return { cls: "done", label: "done" };
      if (typeof stepStatus === "object" && stepStatus.Failed) return { cls: "failed", label: "failed" };
      return { cls: "", label: "pending" };
    }

    function applyTheme(theme) {
      const effective = theme === "light" ? "light" : "dark";
      document.body.setAttribute("data-theme", effective);
      themeBtn.textContent = effective === "dark" ? "Light Mode" : "Dark Mode";
    }

    function renderPlan(plan) {
      if (!plan || !Array.isArray(plan.steps) || plan.steps.length === 0) {
        planEl.innerHTML = '<span class="muted">No plan yet.</span>';
        selectedStepId = null;
        return;
      }

      const hasSelected = selectedStepId !== null
        && plan.steps.some((s) => Number(s.id) === Number(selectedStepId));
      if (selectedStepId !== null && !hasSelected) {
        selectedStepId = null;
      }

      const steps = selectedStepId === null
        ? plan.steps
        : plan.steps.filter((s) => Number(s.id) === Number(selectedStepId));

      if (steps.length === 0) {
        planEl.innerHTML = '<span class="muted">No step selected.</span>';
        return;
      }

      const html = ["<ul>"];
      for (const step of steps) {
        const s = statusLabel(step.status);
        const active = Number(step.id) === Number(selectedStepId) ? " active" : "";
        html.push(`<li class="${active}" data-step-id="${step.id}">${step.id}. ${step.description} <span class="pill ${s.cls}">${s.label}</span></li>`);
      }
      html.push("</ul>");
      planEl.innerHTML = html.join("");
    }

    function renderHistory(callHistory, currentCallId) {
      if (!Array.isArray(callHistory) || callHistory.length === 0) {
        historyEl.innerHTML = '<span class="muted">No calls yet.</span>';
        return;
      }

      const html = ["<ul>"];
      for (const call of [...callHistory].reverse()) {
        const id = Number(call.id || 0);
        const isCurrent = id === Number(currentCallId);
        const isSelected = selectedCallId !== null && id === Number(selectedCallId);
        const prompt = String(call.prompt || "").trim() || "(no prompt)";
        const shortPrompt = prompt.length > 92 ? `${prompt.slice(0, 92)}...` : prompt;
        const status = String(call.status || "unknown");
        const saved = call.session_file ? `Saved: ${String(call.session_file)}` : "Not saved";
        html.push(
          `<li class="${isSelected ? "active" : ""}" data-call-id="${id}">` +
          `<span class="history-item-line">${isCurrent ? "<strong>" : ""}#${id} ${status}${isCurrent ? " (current)</strong>" : ""}</span>` +
          `<span class="history-item-line">${shortPrompt}</span>` +
          `<span class="history-item-meta">${saved}</span>` +
          `</li>`
        );
      }
      html.push("</ul>");
      historyEl.innerHTML = html.join("");
    }

    function renderEvents(events) {
      const escapeHtml = (v) =>
        String(v)
          .replaceAll("&", "&amp;")
          .replaceAll("<", "&lt;")
          .replaceAll(">", "&gt;");

      const detectLang = (code, langHint, level) => {
        const hint = String(langHint || "").toLowerCase();
        if (hint) return hint;
        const text = String(code || "");
        if (
          /^diff --git /m.test(text) ||
          /^index [0-9a-f]+\.\.[0-9a-f]+/m.test(text) ||
          /^@@ .* @@/m.test(text) ||
          /^--- /m.test(text) ||
          /^\+\+\+ /m.test(text)
        ) {
          return "diff";
        }
        if (String(level || "") === "code") return "text";
        return "";
      };

      const highlightDiff = (code) => {
        return String(code || "")
          .split("\n")
          .map((line) => {
            let cls = "code-line";
            if (/^(diff --git|index |--- |\+\+\+ )/.test(line)) cls += " line-meta";
            else if (/^@@/.test(line)) cls += " line-hunk";
            else if (/^\+/.test(line) && !/^\+\+\+ /.test(line)) cls += " line-add";
            else if (/^-/.test(line) && !/^--- /.test(line)) cls += " line-del";
            return `<span class="${cls}">${escapeHtml(line)}</span>`;
          })
          .join("\n");
      };

      const applyPatterns = (escapedCode, patterns) => {
        let html = escapedCode;
        for (const p of patterns) {
          html = html.replace(p.re, p.fn);
        }
        return html;
      };

      const highlightCode = (code, langHint, level) => {
        const lang = detectLang(code, langHint, level);
        const escaped = escapeHtml(String(code || ""));

        if (lang === "diff") return highlightDiff(code);

        if (lang === "json") {
          return applyPatterns(escaped, [
            { re: /("(?:\\.|[^"\\])*")(?=\s*:)/g, fn: '<span class="tok-key">$1</span>' },
            { re: /"(?:\\.|[^"\\])*"/g, fn: '<span class="tok-str">$&</span>' },
            { re: /\b(true|false|null)\b/g, fn: '<span class="tok-bool">$1</span>' },
            { re: /\b-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/g, fn: '<span class="tok-num">$&</span>' },
          ]);
        }

        if (lang === "rust") {
          return applyPatterns(escaped, [
            { re: /\/\/[^\n]*/g, fn: '<span class="tok-cmt">$&</span>' },
            { re: /"(?:\\.|[^"\\])*"/g, fn: '<span class="tok-str">$&</span>' },
            { re: /\b(-?\d+(?:\.\d+)?)\b/g, fn: '<span class="tok-num">$1</span>' },
            {
              re: /\b(fn|let|mut|pub|impl|struct|enum|match|if|else|for|while|loop|return|use|mod|trait|where|async|await|const|static|crate|self|super)\b/g,
              fn: '<span class="tok-kw">$1</span>',
            },
          ]);
        }

        if (lang === "bash" || lang === "sh" || lang === "shell") {
          return applyPatterns(escaped, [
            { re: /#[^\n]*/g, fn: '<span class="tok-cmt">$&</span>' },
            { re: /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g, fn: '<span class="tok-str">$&</span>' },
            { re: /\b(-?\d+(?:\.\d+)?)\b/g, fn: '<span class="tok-num">$1</span>' },
            {
              re: /\b(if|then|else|fi|for|do|done|case|esac|function|in|while|until|export|local|readonly)\b/g,
              fn: '<span class="tok-kw">$1</span>',
            },
          ]);
        }

        return escaped;
      };

      const renderMessageWithCode = (message) => {
        const text = String(message || "");
        const fence = /```([a-zA-Z0-9_-]+)?\n([\s\S]*?)```/g;
        let out = [];
        let last = 0;
        let m = null;
        while ((m = fence.exec(text)) !== null) {
          const before = text.slice(last, m.index).trim();
          if (before) {
            out.push(`<div class="event-body">${escapeHtml(before)}</div>`);
          }
          const lang = (m[1] || "").trim();
          const code = m[2] || "";
          if (lang) out.push(`<div class="event-code-label">${escapeHtml(lang)}</div>`);
          const rendered = highlightCode(code, lang, "code");
          out.push(`<pre class="event-code-block"><code>${rendered}</code></pre>`);
          last = fence.lastIndex;
        }
        const tail = text.slice(last).trim();
        if (tail) {
          const tailLang = detectLang(tail, "", "");
          if (tailLang === "diff") {
            out.push(`<div class="event-code-label">diff</div>`);
            out.push(`<pre class="event-code-block"><code>${highlightCode(tail, "diff", "tool_output")}</code></pre>`);
          } else {
            out.push(`<div class="event-body">${escapeHtml(tail)}</div>`);
          }
        }
        if (out.length === 0) {
          out.push('<div class="event-body"></div>');
        }
        return out.join("");
      };

      if (!Array.isArray(events) || events.length === 0) {
        eventsEl.innerHTML = '<div class="events-empty">No events yet.</div>';
        hasRenderedEvents = true;
        return;
      }

      const filtered = selectedCallId === null
        ? events
        : events.filter((e) => Number(e.call_id) === Number(selectedCallId));
      const stepFiltered = selectedStepId === null
        ? filtered
        : filtered.filter((e) => Number(e.step_id) === Number(selectedStepId));

      if (stepFiltered.length === 0) {
        let reason = "selected filters";
        if (selectedCallId !== null && selectedStepId !== null) {
          reason = `call #${selectedCallId} and step ${selectedStepId}`;
        } else if (selectedCallId !== null) {
          reason = `call #${selectedCallId}`;
        } else if (selectedStepId !== null) {
          reason = `step ${selectedStepId}`;
        }
        eventsEl.innerHTML = `<div class="events-empty">No events for ${reason}. Click again to clear filter.</div>`;
        hasRenderedEvents = true;
        return;
      }

      const prevTop = eventsEl.scrollTop;
      const wasNearBottom =
        (eventsEl.scrollHeight - (eventsEl.scrollTop + eventsEl.clientHeight)) < 36;

      const html = [];
      for (const e of stepFiltered) {
        const level = String(e.level || "info");
        html.push(
          `<div class="event-item ${escapeHtml(level)}">` +
          `<div class="event-code-label">call #${escapeHtml(e.call_id)}</div>` +
          `<div class="event-head"><span class="event-level">${escapeHtml(level)}</span></div>` +
          renderMessageWithCode(e.message) +
          `</div>`
        );
      }
      eventsEl.innerHTML = html.join("");

      if (!hasRenderedEvents) {
        eventsEl.scrollTop = 0;
      } else if (wasNearBottom) {
        eventsEl.scrollTop = eventsEl.scrollHeight;
      } else {
        eventsEl.scrollTop = prevTop;
      }
      hasRenderedEvents = true;
    }

    async function refresh() {
      try {
        const res = await fetch("/api/state");
        if (!res.ok) return;
        const data = await res.json();
        runBtn.disabled = data.busy;
        clearHistoryBtn.disabled = data.busy;
        statusEl.textContent = data.busy ? "Running..." : "Idle";
        cwdEl.textContent = `Context: ${data.cwd || "-"}`;
        applyTheme(data.theme || "dark");
        renderPlan(data.plan);
        renderHistory(data.call_history || [], data.current_call_id || 0);
        renderEvents(data.events || []);
      } catch (_err) {}
    }

    planEl.addEventListener("click", async (e) => {
      const row = e.target.closest("[data-step-id]");
      if (!row) return;
      const id = Number(row.getAttribute("data-step-id"));
      selectedStepId = selectedStepId === id ? null : id;
      await refresh();
    });

    historyEl.addEventListener("click", async (e) => {
      const row = e.target.closest("[data-call-id]");
      if (!row) return;
      const id = Number(row.getAttribute("data-call-id"));
      selectedCallId = selectedCallId === id ? null : id;
      await refresh();
    });

    clearHistoryBtn.addEventListener("click", async () => {
      const res = await fetch("/api/history/clear", {
        method: "POST",
        headers: {"content-type": "application/json"},
        body: "{}"
      });
      if (res.ok) {
        hasRenderedEvents = false;
        selectedCallId = null;
        selectedStepId = null;
      }
      refresh();
    });

    async function submitPrompt() {
      const text = promptEl.value.trim();
      if (!text) return;
      runBtn.disabled = true;
      statusEl.textContent = "Submitting...";
      await fetch("/api/ask", {
        method: "POST",
        headers: {"content-type": "application/json"},
        body: JSON.stringify({ text })
      });
      refresh();
    }

    runBtn.addEventListener("click", async () => {
      await submitPrompt();
    });

    promptEl.addEventListener("keydown", async (e) => {
      if (e.key !== "Enter") return;
      if (e.shiftKey) return; // keep newline behavior for Shift+Enter
      e.preventDefault();
      await submitPrompt();
    });

    themeBtn.addEventListener("click", async () => {
      const current = document.body.getAttribute("data-theme") === "light" ? "light" : "dark";
      const next = current === "dark" ? "light" : "dark";
      const res = await fetch("/api/theme", {
        method: "POST",
        headers: {"content-type": "application/json"},
        body: JSON.stringify({ theme: next })
      });
      if (res.ok) applyTheme(next);
      refresh();
    });

    refresh();
    setInterval(refresh, 1200);
  </script>
</body>
</html>
"#;
