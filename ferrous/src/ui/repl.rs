use crate::core::ExecutionPlan;
use crate::core::plan::StepStatus;
use crate::ui::interface::InteractionHandler;
use crate::ui::render::{ModelLoadPhase, pretty_print_response, render_model_progress};
use colored::Colorize;
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    Exit,
    Clear,
    Help { verbose: bool },
    ShowConfig,
    ListSessions,
    Save { name: Option<String> },
    Load { selector: String },
    Delete { selector: String },
    UserPrompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplParseResult {
    Empty,
    Command(ReplCommand),
    UsageError(String),
}

pub fn prompt() -> String {
    format!("{}", "ferrous> ".bright_magenta().bold())
}

pub fn print_banner() {
    println!();
    println!("{}", "Ferrous".bright_cyan().bold());
    println!("{}", "local coding agent".dimmed());
    println!(
        "{}",
        "────────────────────────────────────────".bright_black()
    );
    println!("{}", "Type a request and press Enter.".dimmed());
    println!(
        "{}",
        "Use /help for commands, /exit to quit, /help all for detailed docs.".dimmed()
    );
    println!();
}

pub fn print_repl_help() {
    println!("{}", "Commands".bright_yellow().bold());
    println!("  /help               Show this command list");
    println!("  /help all           Show full help with tools and CLI flags");
    println!("  /clear              Reset conversation history");
    println!("  /save [name]        Save current conversation");
    println!("  /load <prefix|id>   Load conversation by name prefix or short id");
    println!("  /list               List saved conversations");
    println!("  /delete <prefix|id> Delete saved conversation");
    println!("  /config             Show merged configuration");
    println!("  /exit               Exit REPL");
    println!();
    println!("{}", "Usage".bright_yellow().bold());
    println!("  Enter any non-command text to ask Ferrous for a coding task.");
    println!("  Commands work with or without the leading '/'.");
    println!();
}

pub fn parse_repl_input(line: &str) -> ReplParseResult {
    let raw = line.trim();
    if raw.is_empty() {
        return ReplParseResult::Empty;
    }

    let had_prefix = raw.starts_with('/');
    let normalized = raw.strip_prefix('/').unwrap_or(raw).trim_start();
    let mut parts = normalized.splitn(2, char::is_whitespace);
    let head = parts.next().unwrap_or_default();
    let tail = parts.next().map(str::trim).unwrap_or_default();
    let head_lower = head.to_ascii_lowercase();

    let parsed = match head_lower.as_str() {
        "exit" | "quit" | "q" | ":q" => Some(ReplCommand::Exit),
        "clear" | "reset" => Some(ReplCommand::Clear),
        "help" | "h" | "?" => Some(ReplCommand::Help {
            verbose: tail.eq_ignore_ascii_case("all"),
        }),
        "config" | "cfg" | "show-config" => Some(ReplCommand::ShowConfig),
        "list" | "ls" => Some(ReplCommand::ListSessions),
        "save" => {
            let name = if tail.is_empty() {
                None
            } else {
                Some(tail.to_string())
            };
            Some(ReplCommand::Save { name })
        }
        "load" => {
            if tail.is_empty() {
                return ReplParseResult::UsageError(
                    "Usage: /load <name prefix or short id>".into(),
                );
            }
            Some(ReplCommand::Load {
                selector: tail.to_string(),
            })
        }
        "delete" | "del" | "rm" => {
            if tail.is_empty() {
                return ReplParseResult::UsageError(
                    "Usage: /delete <name prefix or short id>".into(),
                );
            }
            Some(ReplCommand::Delete {
                selector: tail.to_string(),
            })
        }
        _ => None,
    };

    match parsed {
        Some(cmd) => ReplParseResult::Command(cmd),
        None if had_prefix => ReplParseResult::UsageError(format!(
            "Unknown command '{head}'. Type /help for available commands."
        )),
        None => ReplParseResult::Command(ReplCommand::UserPrompt(raw.to_string())),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepMarker {
    Pending,
    Running,
    Done,
    Failed,
}

impl StepMarker {
    fn from_status(status: &StepStatus) -> Self {
        match status {
            StepStatus::Pending => Self::Pending,
            StepStatus::Running => Self::Running,
            StepStatus::Done => Self::Done,
            StepStatus::Failed(_) => Self::Failed,
        }
    }
}

#[derive(Debug, Default)]
struct ReplRenderState {
    plan_signature: Option<String>,
    seen_steps: std::collections::HashMap<usize, StepMarker>,
    current_step: Option<usize>,
    step_runtime: std::collections::HashMap<usize, StepRuntime>,
    commands_header_printed: bool,
}

#[derive(Debug, Clone)]
struct StepRuntime {
    started_at: Instant,
    tool_calls: usize,
}

#[derive(Debug, Default)]
pub struct ReplMode {
    state: Mutex<ReplRenderState>,
}

impl ReplMode {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn plan_signature(plan: &ExecutionPlan) -> String {
        plan.steps
            .iter()
            .map(|s| format!("{}:{}", s.id, s.description))
            .collect::<Vec<_>>()
            .join("|")
    }
}

impl InteractionHandler for ReplMode {
    fn render_plan(&self, plan: &ExecutionPlan) {
        let mut guard = self
            .state
            .lock()
            .expect("repl render state lock should not be poisoned");

        let signature = Self::plan_signature(plan);
        if guard.plan_signature.as_deref() != Some(signature.as_str()) {
            guard.plan_signature = Some(signature);
            guard.seen_steps.clear();
            guard.current_step = None;
            guard.step_runtime.clear();
            guard.commands_header_printed = false;
            println!();
            println!("{}", "Plan".bright_blue().bold());
            println!(
                "{}",
                "────────────────────────────────────────".bright_black()
            );
        }

        for step in &plan.steps {
            let next = StepMarker::from_status(&step.status);
            let prev = guard
                .seen_steps
                .get(&step.id)
                .copied()
                .unwrap_or(StepMarker::Pending);

            match (prev, next) {
                (_, StepMarker::Running) if prev != StepMarker::Running => {
                    println!(
                        "  {} {}. {}",
                        "step>".bright_cyan().bold(),
                        step.id,
                        step.description.normal()
                    );
                }
                (_, StepMarker::Failed) if prev != StepMarker::Failed => {
                    let reason = if let StepStatus::Failed(reason) = &step.status {
                        reason.as_str()
                    } else {
                        "unknown error"
                    };
                    let summary = guard.step_runtime.get(&step.id).map(|runtime| {
                        format!(
                            "{} tools, {:.1}s",
                            runtime.tool_calls,
                            runtime.started_at.elapsed().as_secs_f32()
                        )
                    });
                    println!(
                        "  {} {}. {} {}",
                        "failed".bright_red().bold(),
                        step.id,
                        step.description.bright_red(),
                        format!("({reason})").dimmed()
                    );
                    if let Some(summary) = summary {
                        println!("    {}", summary.dimmed());
                    }
                }
                _ => {}
            }

            guard.seen_steps.insert(step.id, next);
        }
    }

    fn set_current_step(&self, step_id: Option<usize>) {
        let mut guard = self
            .state
            .lock()
            .expect("repl render state lock should not be poisoned");
        guard.current_step = step_id;
        if let Some(id) = step_id {
            guard.step_runtime.entry(id).or_insert(StepRuntime {
                started_at: Instant::now(),
                tool_calls: 0,
            });
        }
    }

    fn render_model_progress(&self, phase: ModelLoadPhase) {
        render_model_progress(phase);
    }

    fn print_message(&self, message: &str) {
        println!("{message}");
    }

    fn print_error(&self, error: &str) {
        eprintln!("{} {error}", "[error]".red().bold());
    }

    fn print_info(&self, info: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{}", info.dimmed());
        let _ = stdout.flush();
    }

    fn print_response(&self, response: &str) {
        if response.lines().count() > 20 {
            println!();
            println!("{}", "Result".bright_blue().bold());
            println!(
                "{}",
                "────────────────────────────────────────".bright_black()
            );
            pretty_print_response(response);
            println!(
                "{}",
                "────────────────────────────────────────".bright_black()
            );
        } else {
            println!();
            println!("{}", "Result".bright_blue().bold());
            println!(
                "{}",
                "────────────────────────────────────────".bright_black()
            );
            for line in response.lines() {
                println!("  {}", line);
            }
            println!(
                "{}",
                "────────────────────────────────────────".bright_black()
            );
        }
    }

    fn print_stream_start(&self) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "\n{} ", "assistant>".bright_cyan().bold());
        let _ = stdout.flush();
    }

    fn print_stream_chunk(&self, chunk: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{chunk}");
        let _ = stdout.flush();
    }

    fn print_stream_end(&self) {
        println!();
    }

    fn print_stream_code_start(&self, lang: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(
            stdout,
            "\n{} {}\n",
            "assistant>".bright_cyan().bold(),
            format!("```{lang}").bright_black()
        );
        let _ = stdout.flush();
    }

    fn print_stream_code_chunk(&self, chunk: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{chunk}");
        let _ = stdout.flush();
    }

    fn print_stream_code_end(&self) {
        let mut stdout = std::io::stdout();
        let _ = write!(
            stdout,
            "\n{} {}\n",
            "assistant>".bright_cyan().bold(),
            "```".bright_black()
        );
        let _ = stdout.flush();
    }

    fn print_stream_tool_start(&self) {
        let mut guard = self
            .state
            .lock()
            .expect("repl render state lock should not be poisoned");
        if !guard.commands_header_printed {
            println!();
            println!("{}", "Commands".bright_blue().bold());
            println!(
                "{}",
                "────────────────────────────────────────".bright_black()
            );
            guard.commands_header_printed = true;
        }
        if let Some(step_id) = guard.current_step
            && let Some(runtime) = guard.step_runtime.get_mut(&step_id)
        {
            runtime.tool_calls += 1;
        }
        drop(guard);

        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "\n{} ", "used tool>".bright_cyan().bold());
        let _ = stdout.flush();
    }

    fn print_stream_tool_chunk(&self, chunk: &str) {
        let mut stdout = std::io::stdout();
        let _ = write!(stdout, "{}", chunk.bright_cyan());
        let _ = stdout.flush();
    }

    fn print_stream_tool_end(&self) {
        println!();
    }

    fn print_debug(&self, message: &str) {
        eprintln!("{} {message}", "DEBUG:".yellow());
    }
}
