use crate::config::{Config, Job};
use crate::rsync;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::io::{self, Write};

pub struct Repl {
    config: Config,
}

impl Repl {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut rl = DefaultEditor::new()?;

        println!("Pulley Backup REPL");
        println!("Type 'help' for available commands\n");

        loop {
            let readline = rl.readline("pulley> ");
            match readline {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    let _ = rl.add_history_entry(line);

                    if let Err(e) = self.handle_command(line) {
                        eprintln!("Error: {}", e);
                    }
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    println!("Exiting...");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_command(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "help" => self.show_help(),
            "list" => self.list_jobs(),
            "run" => self.run_jobs(&parts[1..])?,
            "reload" => self.reload_config()?,
            "quit" | "exit" => {
                println!("Goodbye!");
                std::process::exit(0);
            }
            _ => {
                println!(
                    "Unknown command: '{}'. Type 'help' for available commands.",
                    parts[0]
                );
            }
        }

        Ok(())
    }

    fn show_help(&self) {
        println!("Available commands:");
        println!("  list                    - List all configured jobs");
        println!("  run <job_id> [...]      - Run specific job(s) by ID");
        println!("  run all                 - Run all jobs");
        println!("  reload                  - Reload configuration file");
        println!("  help                    - Show this help message");
        println!("  quit, exit              - Exit the REPL");
    }

    fn list_jobs(&self) {
        if self.config.jobs.is_empty() {
            println!("No jobs configured");
            return;
        }

        println!("Configured jobs:");
        for job in &self.config.jobs {
            println!("  {} - {}", job.id, job.desc);
            println!("    src: {}", job.src);
            println!("    dest: {}", job.dest);
            if job.delete {
                println!("    delete: true");
            }
            if !job.skip.is_empty() {
                println!("    skip: {}", job.skip.join(", "));
            }
            if job.no_confirm {
                println!("    no-confirm: true");
            }
            println!();
        }
    }

    fn run_jobs(&self, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        if args.is_empty() {
            println!("Usage: run <job_id> [...] | run all");
            return Ok(());
        }

        let jobs: Vec<Job> = if args[0] == "all" {
            self.config.jobs.clone()
        } else {
            let job_ids: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            self.config
                .jobs
                .iter()
                .filter(|j| job_ids.contains(&j.id))
                .cloned()
                .collect()
        };

        if jobs.is_empty() {
            println!("No matching jobs found");
            return Ok(());
        }

        let job_ids = jobs
            .iter()
            .map(|j| j.id.clone())
            .collect::<Vec<String>>()
            .join(", ");
        println!("Jobs to be run: {job_ids}\n");

        for job in jobs {
            println!("Starting job: `{}`", job.desc);
            if rsync::dry_run(&job)? {
                if job.no_confirm {
                    rsync::update(&job)?;
                } else {
                    print!("Continue? (y/n): ");
                    io::stdout().flush()?;
                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    if input.trim().eq_ignore_ascii_case("y") {
                        rsync::update(&job)?;
                    } else {
                        println!("Skipped");
                    }
                }
            } else {
                println!("*Nothing to do*");
            }
            println!("Done job: `{}`\n", job.desc);
        }

        Ok(())
    }

    fn reload_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Reloading configuration...");
        let new_config = Config::load_merged()?;
        self.config = new_config;
        println!(
            "Configuration reloaded successfully. {} job(s) found.",
            self.config.jobs.len()
        );
        Ok(())
    }
}
