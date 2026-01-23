use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use ferrous::agent::Agent;
use ferrous::cli::{pretty_print_response, print_help};
use ferrous::llm::is_port_open;
use rustyline::DefaultEditor;

#[derive(Parser)]
#[command(name = "ferrous")]
#[command(about = "Local coding assistant powered by llama.cpp server")]
struct Args {
    #[arg(long, default_value = "models/model.gguf")]
    model: String,

    #[arg(long, default_value = "8080")]
    port: u16,

    #[arg(long, default_value_t = 0.01)]
    temperature: f32,

    #[arg(long, default_value_t = 0.85)]
    top_p: f32,

    #[arg(long, default_value_t = 0.05)]
    min_p: f32,

    #[arg(long, default_value_t = 50)]
    top_k: i32,

    #[arg(long, default_value_t = 1.15)]
    repeat_penalty: f32,

    #[arg(long, default_value_t = 31768)]
    max_tokens: u32,

    #[arg(long, default_value = "false")]
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

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let server_running = is_port_open("127.0.0.1", args.port).await;

    let mut agent = if server_running {
        Agent::connect_only(args.port).await?
    } else {
        Agent::new(&args.model, args.port, args.debug).await?
    };

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

        match agent.stream(&text, temp, top_p, min_p, top_k, repeat_penalty, max_t).await {
            Ok(resp) => {
                println!("\n{}", "Final response:".bright_green());
                pretty_print_response(&resp);
            }
            Err(e) => eprintln!("{} {}", "Error:".red().bold(), e),
        }

        if let Some(server) = agent.server {
            let _ = server.lock().unwrap().kill();
        }
        return Ok(());
    }

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
                rl.add_history_entry(line.clone())?;

                match input.to_lowercase().as_str() {
                    "exit" | "quit" => break,
                    "clear" => {
                        agent.messages.truncate(1);
                        println!("{}", "Conversation cleared.".bright_yellow());
                    }
                    "help" => print_help(),
                    _ => {
                        println!("{}", "Thinking...".dimmed());
                        let result = agent
                            .stream(
                                input,
                                args.temperature,
                                args.top_p,
                                args.min_p,
                                args.top_k,
                                args.repeat_penalty,
                                args.max_tokens,
                            ).await;

                        match result {
                            Ok(_) => println!(), // already printed
                            Err(e) => eprintln!("{} {}", "Error:".red().bold(), e),
                        }
                    }
                }
            }
            Err(_) => break,
        }
    }

    if let Some(server) = agent.server {
        let _ = server.lock().unwrap().kill();
    }

    Ok(())
}
