use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use ferrous::config::{self, ModelBackend, ModelRole, SamplingConfig};
use ferrous::core::{Agent, execute_plan, sessions};
use ferrous::ui::interface::InteractionHandler;
use ferrous::ui::query::QueryMode;
use ferrous::ui::repl::ReplMode;
use rustyline::DefaultEditor;

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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let mut conf = config::load();
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

    if conf.sampling.temperature.is_none() || (args.temperature != DEFAULT_PARAMS.temperature && conf.sampling.temperature != Some(args.temperature)) {
        conf.sampling.temperature = Some(args.temperature);
    }
    if conf.sampling.top_p.is_none() || (args.top_p != DEFAULT_PARAMS.top_p && conf.sampling.top_p != Some(args.top_p)) {
        conf.sampling.top_p = Some(args.top_p);
    }
    if conf.sampling.min_p.is_none() || (args.min_p != DEFAULT_PARAMS.min_p && conf.sampling.min_p != Some(args.min_p)) {
        conf.sampling.min_p = Some(args.min_p);
    }
    if conf.sampling.top_k.is_none() || (args.top_k != DEFAULT_PARAMS.top_k && conf.sampling.top_k != Some(args.top_k)) {
        conf.sampling.top_k = Some(args.top_k);
    }
    if conf.sampling.repeat_penalty.is_none() || (args.repeat_penalty != DEFAULT_PARAMS.repeat_penalty && conf.sampling.repeat_penalty != Some(args.repeat_penalty)) {
        conf.sampling.repeat_penalty = Some(args.repeat_penalty);
    }
    if conf.sampling.context.is_none() || (args.context != DEFAULT_PARAMS.context && conf.sampling.context != Some(args.context)) {
        conf.sampling.context = Some(args.context);
    }
    if conf.sampling.max_tokens.is_none() || (args.max_tokens != DEFAULT_PARAMS.max_tokens && conf.sampling.max_tokens != Some(args.max_tokens)) {
        conf.sampling.max_tokens = Some(args.max_tokens);
    }
    if conf.sampling.mirostat.is_none() || (args.mirostat != DEFAULT_PARAMS.mirostat && conf.sampling.mirostat != Some(args.mirostat)) {
        conf.sampling.mirostat = Some(args.mirostat);
    }
    if conf.sampling.mirostat_tau.is_none() || (args.mirostat_tau != DEFAULT_PARAMS.mirostat_tau && conf.sampling.mirostat_tau != Some(args.mirostat_tau)) {
        conf.sampling.mirostat_tau = Some(args.mirostat_tau);
    }
    if conf.sampling.mirostat_eta.is_none() || (args.mirostat_eta != DEFAULT_PARAMS.mirostat_eta && conf.sampling.mirostat_eta != Some(args.mirostat_eta)) {
        conf.sampling.mirostat_eta = Some(args.mirostat_eta);
    }

    if args.debug {
        conf.debug = Some(true);
    }

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

        let sampling = SamplingConfig {
            temperature: Some(q_temp.unwrap_or(args.temperature)),
            top_p: Some(q_top_p.unwrap_or(args.top_p)),
            min_p: Some(q_min_p.unwrap_or(args.min_p)),
            top_k: Some(q_top_k.unwrap_or(args.top_k)),
            repeat_penalty: Some(q_repeat_penalty.unwrap_or(args.repeat_penalty)),
            context: Some(q_context.unwrap_or(args.context)),
            max_tokens: Some(q_max_tokens.unwrap_or(args.max_tokens)),
            mirostat: Some(q_mirostat.unwrap_or(args.mirostat)),
            mirostat_tau: Some(q_mirostat_tau.unwrap_or(args.mirostat_tau)),
            mirostat_eta: Some(q_mirostat_eta.unwrap_or(args.mirostat_eta)),
        };

        let plan = agent.generate_plan(&text).await?;
        handler.render_plan(&plan);

        execute_plan(&mut agent, plan, sampling, args.debug, &handler).await?;

        return Ok(());
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
                        let plan = match agent.generate_plan(input).await {
                            Ok(p) => p,
                            Err(e) => {
                                handler.print_error(&format!("Planning error: {e}"));
                                continue;
                            }
                        };

                        handler.render_plan(&plan);

                        let sampling = SamplingConfig {
                            temperature: Some(args.temperature),
                            top_p: Some(args.top_p),
                            min_p: Some(args.min_p),
                            top_k: Some(args.top_k),
                            repeat_penalty: Some(args.repeat_penalty),
                            context: Some(args.context),
                            max_tokens: Some(args.max_tokens),
                            mirostat: Some(args.mirostat),
                            mirostat_tau: Some(args.mirostat_tau),
                            mirostat_eta: Some(args.mirostat_eta),
                        };

                        // ── EXECUTION PHASE ──────────────────────────
                        if let Err(e) =
                            execute_plan(&mut agent, plan, sampling, args.debug, &handler).await
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
