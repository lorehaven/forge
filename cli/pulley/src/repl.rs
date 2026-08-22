use crate::config::{Config, Job};
use crate::rsync;
use quench_cli::prelude::{
    ReplControl, Tone, print_box_banner, print_status, repl_prompt, repl_run,
};
use std::io::{self, Write};

fn prompt() -> String {
    repl_prompt("pulley", "repl")
}

pub struct Repl {
    config: Config,
}

impl Repl {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        print_box_banner("Pulley REPL", "backup and sync jobs");
        print_status(Tone::Info, "hint", "type `help` for available commands");
        println!();

        repl_run(prompt(), |line| self.handle_command(line))?;

        print_status(Tone::Info, "repl", "session closed");
        Ok(())
    }

    pub fn handle_command(&mut self, input: &str) -> ReplControl {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let Some(command) = parts.first() else {
            return ReplControl::Continue(prompt());
        };

        let result = match *command {
            "help" => {
                self.show_help();
                Ok(())
            }
            "list" => {
                self.list_jobs();
                Ok(())
            }
            "run" => self.run_jobs(&parts[1..]),
            "reload" => self.reload_config(),
            "quit" | "exit" => {
                print_status(Tone::Info, "repl", "goodbye");
                return ReplControl::Exit;
            }
            _ => {
                print_status(
                    Tone::Warn,
                    "command",
                    &format!("unknown command `{command}`. type `help`"),
                );
                Ok(())
            }
        };

        if let Err(error) = result {
            print_status(Tone::Error, "error", &error.to_string());
        }

        ReplControl::Continue(prompt())
    }

    fn show_help(&self) {
        print_status(Tone::Info, "help", "available commands");
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
            if let Some(interval) = job.interval {
                println!("    interval: {interval}s (daemon-eligible)");
            }
            println!();
        }
    }

    pub fn run_jobs(&self, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
        if args.is_empty() {
            println!("Usage: run <job_id> [...] | run all");
            return Ok(());
        }

        let jobs = select_jobs(&self.config.jobs, args);

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
                println!("**Nothing to do**");
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

/// Which jobs `run <args>` targets: every job for `run all`, or exactly the
/// ones named, in whatever order they appear in `jobs` (not `args`) - so a
/// `run c a` still runs them in the estate's configured order.
pub fn select_jobs(jobs: &[Job], args: &[&str]) -> Vec<Job> {
    if args.first() == Some(&"all") {
        return jobs.to_vec();
    }

    let job_ids: Vec<&str> = args.to_vec();
    jobs.iter()
        .filter(|j| job_ids.contains(&j.id.as_str()))
        .cloned()
        .collect()
}
