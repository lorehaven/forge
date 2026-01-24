use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use ferrous::agent::Agent;
use ferrous::cli::{pretty_print_response, print_help};
use ferrous::config;
use ferrous::llm::is_port_open;
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
    max_tokens: u32,
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
    max_tokens: 32768,
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

    #[arg(long, default_value_t = DEFAULT_PARAMS.max_tokens)]
    max_tokens: u32,

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
        max_tokens: Option<u32>,
    },
}

// Helper macro — applies config value only if CLI argument is still at default
macro_rules! apply_if_default {
    ($args:expr, $field:ident, $defaults:expr, $conf:expr) => {
        if $args.$field == $defaults.$field {
            if let Some(v) = $conf.$field {
                $args.$field = v;
            }
        }
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = Args::parse();
    let conf = config::load();
    config::print_loaded(&conf, args.debug);

    apply_if_default!(args, model, DEFAULT_PARAMS, conf);
    apply_if_default!(args, port, DEFAULT_PARAMS, conf);
    apply_if_default!(args, temperature, DEFAULT_PARAMS, conf);
    apply_if_default!(args, top_p, DEFAULT_PARAMS, conf);
    apply_if_default!(args, min_p, DEFAULT_PARAMS, conf);
    apply_if_default!(args, top_k, DEFAULT_PARAMS, conf);
    apply_if_default!(args, repeat_penalty, DEFAULT_PARAMS, conf);
    apply_if_default!(args, max_tokens, DEFAULT_PARAMS, conf);
    if !args.debug
        && let Some(debug) = conf.debug
    {
        args.debug = debug;
    }

    let server_running = is_port_open("127.0.0.1", args.port).await;

    let mut agent = if server_running {
        Agent::connect_only(args.port).await?
    } else {
        Agent::new(&args.model, args.port, args.debug).await?
    };

    // ── One-shot query mode ───────────────────────────────────────────────
    if let Some(Commands::Query {
        text,
        temperature: q_temp,
        top_p: q_top_p,
        min_p: q_min_p,
        top_k: q_top_k,
        repeat_penalty: q_repeat_penalty,
        max_tokens: q_max,
    }) = args.command
    {
        println!("{}", "Processing query...".dimmed());

        let temp = q_temp.unwrap_or(args.temperature);
        let top_p = q_top_p.unwrap_or(args.top_p);
        let min_p = q_min_p.unwrap_or(args.min_p);
        let top_k = q_top_k.unwrap_or(args.top_k);
        let repeat_penalty = q_repeat_penalty.unwrap_or(args.repeat_penalty);
        let max_t = q_max.unwrap_or(args.max_tokens);

        match agent
            .stream(&text, temp, top_p, min_p, top_k, repeat_penalty, max_t)
            .await
        {
            Ok(resp) => {
                println!("\n{}", "Final response:".bright_green());
                pretty_print_response(&resp);
            }
            Err(e) => eprintln!("{} {}", "Error:".red().bold(), e),
        }

        if let Some(server) = agent.server.take() {
            let _ = server.lock().unwrap().kill();
        }
        return Ok(());
    }

    // ── REPL mode ────────────────────────────────────────────────────────
    println!(
        "{} {}",
        "Ferrous coding agent ready.".bright_cyan().bold(),
        "Type 'help' for commands, 'exit' to quit.".dimmed()
    );

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
                        println!("{}", "Conversation cleared.".bright_yellow());
                    }
                    "help" => print_help(),
                    _ => {
                        println!("{}", "Thinking...".dimmed());
                        match agent
                            .stream(
                                input,
                                args.temperature,
                                args.top_p,
                                args.min_p,
                                args.top_k,
                                args.repeat_penalty,
                                args.max_tokens,
                            )
                            .await
                        {
                            Ok(_) => println!(),
                            Err(e) => eprintln!("{} {}", "Error:".red().bold(), e),
                        }
                    }
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted)
            | Err(rustyline::error::ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("Readline error: {}", err);
                break;
            }
        }
    }

    if let Some(server) = agent.server.take() {
        let _ = server.lock().unwrap().kill();
    }

    Ok(())
}
