use anyhow::{Result, anyhow};
use clap::{Parser, Subcommand};
use colored::Colorize;
use ferrous::config::{self, ModelBackend, ModelRole, SamplingConfig, UiMode, UiTheme};
use ferrous::core::{Agent, execute_plan, sessions};
use ferrous::ui::interface::InteractionHandler;
use ferrous::ui::query::QueryMode;
use ferrous::ui::repl::ReplMode;
use ferrous::ui::web::{EMBEDDED_PAGE, WebMode};
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Clone, Copy, PartialEq)]
struct Params {
    model: &'static str,
    port: u16,
    temperature: f32,
    top_p: f32,
    min_p: f32,
    top_k: i32,
    repeat_penalty: f32,
    context: u32,
    max_tokens: u32,
    mirostat: i32,
    mirostat_tau: f32,
    mirostat_eta: f32,
    debug: bool,
}

const DEFAULT_PARAMS: Params = Params {
    model: "models/model.gguf",
    port: 8080,
    temperature: 0.01,
    top_p: 0.85,
    min_p: 0.05,
    top_k: 50,
    repeat_penalty: 1.15,
    context: 49152,
    max_tokens: 24576,
    mirostat: 0,
    mirostat_tau: 4.0,
    mirostat_eta: 0.1,
    debug: false,
};

#[derive(Parser)]
#[command(name = "ferrous")]
#[command(about = "Local coding assistant powered by llama.cpp server")]
struct Args {
    #[arg(long, default_value = DEFAULT_PARAMS.model)]
    model: String,

    #[arg(long, default_value_t = DEFAULT_PARAMS.port)]
    port: u16,

    #[arg(long, default_value_t = DEFAULT_PARAMS.temperature)]
    temperature: f32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.top_p)]
    top_p: f32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.min_p)]
    min_p: f32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.top_k)]
    top_k: i32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.repeat_penalty)]
    repeat_penalty: f32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.context)]
    context: u32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.max_tokens)]
    max_tokens: u32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.mirostat)]
    mirostat: i32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.mirostat_tau)]
    mirostat_tau: f32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.mirostat_eta)]
    mirostat_eta: f32,

    #[arg(long, default_value_t = DEFAULT_PARAMS.debug)]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Query {
        #[arg(long)]
        text: String,

        #[arg(long)]
        temperature: Option<f32>,

        #[arg(long)]
        top_p: Option<f32>,

        #[arg(long)]
        min_p: Option<f32>,

        #[arg(long)]
        top_k: Option<i32>,

        #[arg(long)]
        repeat_penalty: Option<f32>,

        #[arg(long)]
        context: Option<u32>,

        #[arg(long)]
        max_tokens: Option<u32>,

        #[arg(long)]
        mirostat: Option<i32>,

        #[arg(long)]
        mirostat_tau: Option<f32>,

        #[arg(long)]
        mirostat_eta: Option<f32>,
    },
}

async fn generate_valid_plan(
    agent: &mut Agent,
    query: &str,
    max_attempts: usize,
    handler: &dyn InteractionHandler,
    is_debug: bool,
) -> Result<ferrous::core::ExecutionPlan> {
    for attempt in 1..=max_attempts {
        let plan = agent.generate_plan(query).await?;
        handler.render_plan(&plan);

        match agent.validate_plan(query, &plan).await {
            Ok(true) => return Ok(plan),
            Ok(false) => {
                if attempt < max_attempts {
                    handler.print_error(&format!(
                        "Plan validation failed (attempt {attempt}/{max_attempts}). Regenerating..."
                    ));
                }
            }
            Err(e) => {
                if is_debug {
                    handler.print_debug(&format!(
                        "Plan validation error (continuing with current plan): {e}"
                    ));
                }
                return Ok(plan);
            }
        }
    }

    Err(anyhow!(
        "Unable to generate a valid plan after {} attempts",
        max_attempts
    ))
}

fn apply_sampling_context_to_local_models(conf: &mut ferrous::config::Config) {
    let Some(target_ctx) = conf.sampling.context else {
        return;
    };

    for backend in conf.models.values_mut() {
        if let ModelBackend::LocalLlama { context_size, .. } = backend
            && *context_size < target_ctx
        {
            *context_size = target_ctx;
        }
    }
}

fn build_effective_sampling(base: &SamplingConfig) -> SamplingConfig {
    SamplingConfig {
        temperature: Some(base.temperature.unwrap_or(DEFAULT_PARAMS.temperature)),
        top_p: Some(base.top_p.unwrap_or(DEFAULT_PARAMS.top_p)),
        min_p: Some(base.min_p.unwrap_or(DEFAULT_PARAMS.min_p)),
        top_k: Some(base.top_k.unwrap_or(DEFAULT_PARAMS.top_k)),
        repeat_penalty: Some(base.repeat_penalty.unwrap_or(DEFAULT_PARAMS.repeat_penalty)),
        context: Some(base.context.unwrap_or(DEFAULT_PARAMS.context)),
        max_tokens: Some(base.max_tokens.unwrap_or(DEFAULT_PARAMS.max_tokens)),
        mirostat: Some(base.mirostat.unwrap_or(DEFAULT_PARAMS.mirostat)),
        mirostat_tau: Some(base.mirostat_tau.unwrap_or(DEFAULT_PARAMS.mirostat_tau)),
        mirostat_eta: Some(base.mirostat_eta.unwrap_or(DEFAULT_PARAMS.mirostat_eta)),
    }
}

#[derive(Debug, Deserialize)]
struct WebAskRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct WebThemeRequest {
    theme: String,
}

#[derive(Debug, Serialize)]
struct WebErrorResponse {
    error: String,
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(header: &str) -> usize {
    header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("content-length") {
                return value.trim().parse::<usize>().ok();
            }
            None
        })
        .unwrap_or(0)
}

async fn read_http_request(stream: &mut TcpStream) -> Result<Option<HttpRequest>> {
    let mut data = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 2048];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            if data.is_empty() {
                return Ok(None);
            }
            return Err(anyhow!(
                "Connection closed before HTTP headers were complete"
            ));
        }
        data.extend_from_slice(&chunk[..read]);

        if data.len() > 1024 * 1024 {
            return Err(anyhow!("HTTP request too large"));
        }
        if let Some(pos) = find_header_end(&data) {
            break pos;
        }
    };

    let header_bytes = &data[..header_end];
    let header = std::str::from_utf8(header_bytes).map_err(|_| anyhow!("Invalid HTTP header"))?;
    let mut lines = header.lines();
    let first_line = lines
        .next()
        .ok_or_else(|| anyhow!("Missing HTTP request line"))?;
    let mut parts = first_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("Missing method"))?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| anyhow!("Missing path"))?
        .to_string();

    let content_length = parse_content_length(header);
    let body_start = header_end + 4;
    let mut body = data[body_start..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);

    Ok(Some(HttpRequest { method, path, body }))
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let status_line = format!("HTTP/1.1 {} {}\r\n", status, status_text(status));
    let headers = format!(
        "Content-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(status_line.as_bytes()).await?;
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}

fn json_error(message: &str) -> Vec<u8> {
    serde_json::to_vec(&WebErrorResponse {
        error: message.to_string(),
    })
    .unwrap_or_else(|_| br#"{"error":"internal"}"#.to_vec())
}

fn persist_theme(theme: UiTheme) -> Result<()> {
    let mut cfg = config::load();
    cfg.theme = Some(theme);
    config::save(&cfg)?;
    Ok(())
}

async fn run_web_mode(agent: Agent, conf: ferrous::config::Config, is_debug: bool) -> Result<()> {
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let handler = WebMode::new(cwd, conf.theme.unwrap_or(UiTheme::Dark));
    let web_agent = Arc::new(AsyncMutex::new(agent));
    let listener = TcpListener::bind("127.0.0.1:8787").await?;

    println!(
        "{} http://127.0.0.1:8787",
        "Web UI ready at".bright_cyan().bold()
    );

    loop {
        let (mut stream, _) = listener.accept().await?;
        let handler = handler.clone();
        let conf = conf.clone();
        let web_agent = Arc::clone(&web_agent);

        tokio::spawn(async move {
            let req = match read_http_request(&mut stream).await {
                Ok(Some(r)) => r,
                Ok(None) => return,
                Err(e) => {
                    let _ = write_http_response(
                        &mut stream,
                        400,
                        "application/json",
                        &json_error(&e.to_string()),
                    )
                    .await;
                    return;
                }
            };

            if req.method == "GET" && req.path == "/" {
                let _ = write_http_response(
                    &mut stream,
                    200,
                    "text/html; charset=utf-8",
                    EMBEDDED_PAGE.as_bytes(),
                )
                .await;
                return;
            }

            if req.method == "GET" && req.path == "/api/state" {
                let snapshot = handler.snapshot();
                let body = serde_json::to_vec(&snapshot)
                    .unwrap_or_else(|_| br#"{"error":"failed to serialize state"}"#.to_vec());
                let _ = write_http_response(&mut stream, 200, "application/json", &body).await;
                return;
            }

            if req.method == "POST" && req.path == "/api/ask" {
                let payload = serde_json::from_slice::<WebAskRequest>(&req.body);
                let ask = match payload {
                    Ok(v) => v,
                    Err(_) => {
                        let _ = write_http_response(
                            &mut stream,
                            400,
                            "application/json",
                            &json_error("Invalid JSON body"),
                        )
                        .await;
                        return;
                    }
                };

                let prompt = ask.text.trim();
                if prompt.is_empty() {
                    let _ = write_http_response(
                        &mut stream,
                        400,
                        "application/json",
                        &json_error("Prompt is empty"),
                    )
                    .await;
                    return;
                }

                if !handler.try_start_request(prompt) {
                    let _ = write_http_response(
                        &mut stream,
                        409,
                        "application/json",
                        &json_error("A request is already running"),
                    )
                    .await;
                    return;
                }

                let prompt_owned = prompt.to_string();
                let handler_bg = handler.clone();
                let conf_bg = conf.clone();
                let web_agent_bg = Arc::clone(&web_agent);
                tokio::spawn(async move {
                    let mut agent_guard = web_agent_bg.lock().await;
                    let sampling = build_effective_sampling(&conf_bg.sampling);
                    let result = async {
                        let plan = generate_valid_plan(
                            &mut agent_guard,
                            &prompt_owned,
                            3,
                            &handler_bg,
                            is_debug,
                        )
                        .await?;
                        execute_plan(
                            &mut agent_guard,
                            plan,
                            &prompt_owned,
                            sampling,
                            is_debug,
                            &handler_bg,
                        )
                        .await
                    }
                    .await;

                    let autosave_name = {
                        let trimmed = prompt_owned.trim();
                        let short: String = trimmed.chars().take(64).collect();
                        if short.is_empty() {
                            "web-session".to_string()
                        } else {
                            format!("web {short}")
                        }
                    };
                    match agent_guard.save_conversation_named(&autosave_name) {
                        Ok(filename) => handler_bg.set_latest_call_session_file(Some(filename)),
                        Err(e) => handler_bg.print_error(&format!("Session autosave failed: {e}")),
                    }

                    match result {
                        Ok(()) => handler_bg.finish_request(None),
                        Err(e) => handler_bg.finish_request(Some(&e.to_string())),
                    }
                });

                let _ = write_http_response(
                    &mut stream,
                    200,
                    "application/json",
                    br#"{"status":"accepted"}"#,
                )
                .await;
                return;
            }

            if req.method == "POST" && req.path == "/api/theme" {
                let payload = serde_json::from_slice::<WebThemeRequest>(&req.body);
                let requested_theme = match payload {
                    Ok(v) => v.theme,
                    Err(_) => {
                        let _ = write_http_response(
                            &mut stream,
                            400,
                            "application/json",
                            &json_error("Invalid JSON body"),
                        )
                        .await;
                        return;
                    }
                };

                let theme = match requested_theme.as_str() {
                    "dark" => UiTheme::Dark,
                    "light" => UiTheme::Light,
                    _ => {
                        let _ = write_http_response(
                            &mut stream,
                            400,
                            "application/json",
                            &json_error("Theme must be 'dark' or 'light'"),
                        )
                        .await;
                        return;
                    }
                };

                handler.set_theme(theme);
                if let Err(e) = persist_theme(theme) {
                    let _ = write_http_response(
                        &mut stream,
                        500,
                        "application/json",
                        &json_error(&format!("Failed to persist theme: {e}")),
                    )
                    .await;
                    return;
                }

                let _ = write_http_response(
                    &mut stream,
                    200,
                    "application/json",
                    br#"{"status":"ok"}"#,
                )
                .await;
                return;
            }

            if req.method == "POST" && req.path == "/api/history/clear" {
                if !handler.clear_history() {
                    let _ = write_http_response(
                        &mut stream,
                        409,
                        "application/json",
                        &json_error("Cannot clear history while a request is running"),
                    )
                    .await;
                    return;
                }

                let _ = write_http_response(
                    &mut stream,
                    200,
                    "application/json",
                    br#"{"status":"ok"}"#,
                )
                .await;
                return;
            }

            if req.path.starts_with("/api/") {
                let _ = write_http_response(
                    &mut stream,
                    404,
                    "application/json",
                    &json_error("Endpoint not found"),
                )
                .await;
                return;
            }

            let _ =
                write_http_response(&mut stream, 404, "text/plain; charset=utf-8", b"Not Found")
                    .await;
        });
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut conf = config::load();
    if conf.theme.is_none() {
        conf.theme = Some(UiTheme::Dark);
        if let Err(e) = config::save(&conf) {
            eprintln!(
                "{} Failed to persist default theme in .ferrous/config.toml: {e}",
                "Warning:".yellow().bold()
            );
        }
    }
    config::print_loaded(&conf, args.debug);

    // Merge CLI args into config if they are NOT default
    if args.model != DEFAULT_PARAMS.model || args.port != DEFAULT_PARAMS.port {
        conf.models.insert(
            ModelRole::Chat,
            ModelBackend::LocalLlama {
                model_path: args.model.clone(),
                port: args.port,
                context_size: args.context,
                num_gpu_layers: 999,
            },
        );
    }

    // Default chat model if nothing configured
    conf.models
        .entry(ModelRole::Chat)
        .or_insert_with(|| ModelBackend::LocalLlama {
            model_path: DEFAULT_PARAMS.model.to_string(),
            port: DEFAULT_PARAMS.port,
            context_size: DEFAULT_PARAMS.context,
            num_gpu_layers: 999,
        });

    // Ensure Planner role has a backend (fallback to Chat)
    if !conf.models.contains_key(&ModelRole::Planner)
        && let Some(chat_backend) = conf.models.get(&ModelRole::Chat).cloned()
    {
        conf.models.insert(ModelRole::Planner, chat_backend);
    }

    if conf.sampling.temperature.is_none()
        || (args.temperature != DEFAULT_PARAMS.temperature
            && conf.sampling.temperature != Some(args.temperature))
    {
        conf.sampling.temperature = Some(args.temperature);
    }
    if conf.sampling.top_p.is_none()
        || (args.top_p != DEFAULT_PARAMS.top_p && conf.sampling.top_p != Some(args.top_p))
    {
        conf.sampling.top_p = Some(args.top_p);
    }
    if conf.sampling.min_p.is_none()
        || (args.min_p != DEFAULT_PARAMS.min_p && conf.sampling.min_p != Some(args.min_p))
    {
        conf.sampling.min_p = Some(args.min_p);
    }
    if conf.sampling.top_k.is_none()
        || (args.top_k != DEFAULT_PARAMS.top_k && conf.sampling.top_k != Some(args.top_k))
    {
        conf.sampling.top_k = Some(args.top_k);
    }
    if conf.sampling.repeat_penalty.is_none()
        || (args.repeat_penalty != DEFAULT_PARAMS.repeat_penalty
            && conf.sampling.repeat_penalty != Some(args.repeat_penalty))
    {
        conf.sampling.repeat_penalty = Some(args.repeat_penalty);
    }
    if conf.sampling.context.is_none()
        || (args.context != DEFAULT_PARAMS.context && conf.sampling.context != Some(args.context))
    {
        conf.sampling.context = Some(args.context);
    }
    if conf.sampling.max_tokens.is_none()
        || (args.max_tokens != DEFAULT_PARAMS.max_tokens
            && conf.sampling.max_tokens != Some(args.max_tokens))
    {
        conf.sampling.max_tokens = Some(args.max_tokens);
    }
    if conf.sampling.mirostat.is_none()
        || (args.mirostat != DEFAULT_PARAMS.mirostat
            && conf.sampling.mirostat != Some(args.mirostat))
    {
        conf.sampling.mirostat = Some(args.mirostat);
    }
    if conf.sampling.mirostat_tau.is_none()
        || (args.mirostat_tau != DEFAULT_PARAMS.mirostat_tau
            && conf.sampling.mirostat_tau != Some(args.mirostat_tau))
    {
        conf.sampling.mirostat_tau = Some(args.mirostat_tau);
    }
    if conf.sampling.mirostat_eta.is_none()
        || (args.mirostat_eta != DEFAULT_PARAMS.mirostat_eta
            && conf.sampling.mirostat_eta != Some(args.mirostat_eta))
    {
        conf.sampling.mirostat_eta = Some(args.mirostat_eta);
    }

    if args.debug {
        conf.debug = Some(true);
    }

    apply_sampling_context_to_local_models(&mut conf);

    let mut agent = Agent::with_config(conf.clone())?;

    // ── One-shot query mode ───────────────────────────────────────────────
    if let Some(Commands::Query {
        text,
        temperature: q_temp,
        top_p: q_top_p,
        min_p: q_min_p,
        top_k: q_top_k,
        repeat_penalty: q_repeat_penalty,
        context: q_context,
        max_tokens: q_max_tokens,
        mirostat: q_mirostat,
        mirostat_tau: q_mirostat_tau,
        mirostat_eta: q_mirostat_eta,
    }) = args.command
    {
        let handler = QueryMode;
        handler.print_info("Processing query...");

        let mut sampling = build_effective_sampling(&conf.sampling);
        sampling.temperature =
            Some(q_temp.unwrap_or(sampling.temperature.unwrap_or(args.temperature)));
        sampling.top_p = Some(q_top_p.unwrap_or(sampling.top_p.unwrap_or(args.top_p)));
        sampling.min_p = Some(q_min_p.unwrap_or(sampling.min_p.unwrap_or(args.min_p)));
        sampling.top_k = Some(q_top_k.unwrap_or(sampling.top_k.unwrap_or(args.top_k)));
        sampling.repeat_penalty = Some(
            q_repeat_penalty.unwrap_or(sampling.repeat_penalty.unwrap_or(args.repeat_penalty)),
        );
        sampling.context = Some(q_context.unwrap_or(sampling.context.unwrap_or(args.context)));
        sampling.max_tokens =
            Some(q_max_tokens.unwrap_or(sampling.max_tokens.unwrap_or(args.max_tokens)));
        sampling.mirostat = Some(q_mirostat.unwrap_or(sampling.mirostat.unwrap_or(args.mirostat)));
        sampling.mirostat_tau =
            Some(q_mirostat_tau.unwrap_or(sampling.mirostat_tau.unwrap_or(args.mirostat_tau)));
        sampling.mirostat_eta =
            Some(q_mirostat_eta.unwrap_or(sampling.mirostat_eta.unwrap_or(args.mirostat_eta)));

        let plan = generate_valid_plan(&mut agent, &text, 3, &handler, args.debug).await?;

        execute_plan(&mut agent, plan, &text, sampling, args.debug, &handler).await?;

        return Ok(());
    }

    if conf.ui == Some(UiMode::Web) {
        return run_web_mode(agent, conf.clone(), args.debug).await;
    }

    // ── REPL mode ────────────────────────────────────────────────────────
    let handler = ReplMode;
    handler.print_message(&format!(
        "{} {}",
        "Ferrous coding agent ready.".bright_cyan().bold(),
        "Type 'help' for commands, 'exit' to quit.".dimmed()
    ));

    let mut rl = DefaultEditor::new()?;

    loop {
        let readline = rl.readline(&format!("{}", ">> ".bright_magenta()));
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(&line);

                match input.to_lowercase().as_str() {
                    "exit" | "quit" => break,
                    "clear" => {
                        agent.messages.truncate(1);
                        handler
                            .print_message(&format!("{}", "Conversation cleared.".bright_yellow()));
                    }
                    "help" => ferrous::ui::render::print_help(),
                    "config" | "show-config" | "cfg" => {
                        conf.display();
                    }
                    "list" => match sessions::list_conversations() {
                        Ok(items) if items.is_empty() => {
                            handler.print_message(&format!(
                                "{}",
                                "No saved conversations yet.".bright_yellow()
                            ));
                        }
                        Ok(items) => {
                            handler.print_message(&format!(
                                "{}",
                                "Saved conversations:".bright_cyan().bold()
                            ));
                            for (name, short_id, date) in items {
                                handler.print_message(&format!(
                                    "  • {} ({short_id}) [{date}]",
                                    name.bright_white(),
                                ));
                            }
                        }
                        Err(e) => handler.print_error(&format!("{e}")),
                    },
                    cmd if cmd.starts_with("save") => {
                        let rest = input[4..].trim();
                        let name = if rest.is_empty() {
                            "".to_string()
                        } else {
                            rest.to_string()
                        };

                        match agent.save_conversation_named(&name) {
                            Ok(filename) => {
                                let extra = if name.trim().is_empty() {
                                    " (auto-named)"
                                } else {
                                    ""
                                };
                                handler.print_message(&format!(
                                    "{} {filename}{extra}",
                                    "Conversation saved as".bright_green(),
                                ));
                            }
                            Err(e) => handler.print_error(&format!("Save failed: {e}")),
                        }
                    }

                    cmd if cmd.starts_with("load") => {
                        let rest = input[4..].trim();
                        if rest.is_empty() {
                            handler.print_message(&format!(
                                "{}",
                                "Usage: load <name prefix or short id>".yellow()
                            ));
                            continue;
                        }

                        match agent.load_conversation(rest) {
                            Ok(name) => handler.print_message(&format!(
                                "{} {name} {}",
                                "Loaded conversation:".bright_green(),
                                "(current history replaced)".dimmed()
                            )),
                            Err(e) => handler.print_error(&format!("Load failed: {e}")),
                        }
                    }

                    cmd if cmd.starts_with("delete") => {
                        let rest = input[6..].trim();
                        if rest.is_empty() {
                            handler.print_message(&format!(
                                "{}",
                                "Usage: delete <name prefix or short id>".yellow()
                            ));
                            continue;
                        }

                        match sessions::delete_conversation_by_prefix(rest) {
                            Ok(name) => handler.print_message(&format!(
                                "{} {name} {}",
                                "Deleted:".bright_green(),
                                "(removed from disk)".dimmed()
                            )),
                            Err(e) => handler.print_error(&format!("Delete failed: {e}")),
                        }
                    }
                    _ => {
                        // ── PLAN PHASE ───────────────────────────────
                        let plan =
                            match generate_valid_plan(&mut agent, input, 3, &handler, args.debug)
                                .await
                            {
                                Ok(p) => p,
                                Err(e) => {
                                    handler.print_error(&format!("Planning error: {e}"));
                                    continue;
                                }
                            };

                        let sampling = build_effective_sampling(&conf.sampling);

                        // ── EXECUTION PHASE ──────────────────────────
                        if let Err(e) =
                            execute_plan(&mut agent, plan, input, sampling, args.debug, &handler)
                                .await
                        {
                            handler.print_error(&format!("Execution error: {e}"));
                        }
                    }
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted)
            | Err(rustyline::error::ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Readline error: {err}");
                break;
            }
        }
    }

    // REPL loop cleanup is not strictly necessary as processes will terminate on exit,
    // but we could implement a more formal shutdown in ModelManager if needed.
    Ok(())
}
